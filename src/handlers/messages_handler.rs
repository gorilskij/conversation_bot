use std::time::{SystemTime, UNIX_EPOCH};
use teloxide::prelude::*;
use teloxide::types::MessageKind;
use tokio_stream::wrappers::UnboundedReceiverStream;
use crate::{AppError, CONVERSATIONS, ERROR_LOGGER, FromUser, OPENAI_CLIENT};
use crate::result::{Result, Error};

// TODO: give the bot the ability to end a conversation if it says "bye" or "goodbye"
async fn handle_message(cx: UpdateWithCx<&Bot, Message>) -> Result {
    let fresh = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("you're pre-1970, dude")
        .as_secs()
        .checked_sub(u64::try_from(cx.update.date).expect("message sent before 1970"))
        .map(|elapsed| elapsed < 5) // ignore if message is more than a few seconds old
        .unwrap_or(true); // don't ignore if message is from the future

    if !fresh {
        println!("message too old");
        Err(AppError::MessageTooOld)?
    }

    // if there is a settings dialog active, deactivate it to prevent inconsistencies
    if let Some(conversation) = CONVERSATIONS.lock().await.get_mut(cx.update.chat_id()) {
        conversation.deactivate_settings_dialog(&cx.requester).await?;
    }

    if let MessageKind::Common(_) = cx.update.kind {
        match cx.update.text() {
            Some("/start" | "/start@nonautisticbot") => {
                println!("got /start command");
                match CONVERSATIONS.lock().await.start(cx.update.chat_id()) {
                    Ok(_) => { cx.answer("Hello").send().await?; }
                    Err(Error::App(AppError::ConversationAlreadyRunning(_))) => {
                        cx.answer("Conversation already running").send().await?; // TODO: since <timestamp>
                    }
                    res => res?,
                }
            }
            Some("/end" | "/end@nonautisticbot") => {
                println!("got /end command");
                match CONVERSATIONS.lock().await.end(cx.chat_id()) {
                    Ok(_) => { cx.answer("Goodbye").send().await?; }
                    Err(Error::App(AppError::NoConversationRunning(_))) => {
                        cx.answer("No conversation currently running").send().await?;
                    }
                    res => res?,
                }
            }
            // TODO: make settings per-chat
            Some("/settings" | "/settings@nonautisticbot") => {
                println!("got /settings command");
                match CONVERSATIONS.lock().await.get_mut(cx.chat_id()) {
                    None => { cx.answer("Settings are per-conversation, no conversation currently running").send().await?; }
                    Some(conversation) => {
                        let message = cx
                            .requester
                            .send_message(cx.chat_id(), conversation.settings.get_message_text())
                            .reply_markup(conversation.settings.get_inline_keyboard_markup())
                            .send()
                            .await?;

                        conversation.active_settings_dialog = Some((cx.chat_id(), message.id));
                    }
                }
            }
            Some(msg) => {
                println!("got message \"{}\"", msg);
                let user = match cx.update.from() {
                    Some(user) => {
                        println!("sender: user: {}", user.first_name);
                        FromUser::User(user.clone())
                    }
                    None => {
                        println!("message without sender");
                        Err(AppError::MessageWithoutSender(cx.chat_id(), msg.to_string()))?
                    }
                };

                if let Some(conversation) = CONVERSATIONS.lock().await.get_mut(cx.chat_id()) {
                    conversation.add(user.clone(), msg.to_string());
                    let reply = conversation.produce_reply(&*OPENAI_CLIENT.lock().await).await?;
                    cx.answer(reply).send().await?;
                }
            }
            _ => {}
        }
    }

    Ok(())
}

pub async fn messages_handler(rx: DispatcherHandlerRx<&Bot, Message>) {
    UnboundedReceiverStream::new(rx)
        .for_each_concurrent(None, |message| async move {
            let result = handle_message(message).await;
            ERROR_LOGGER.lock().await.maybe_log(&result);
        })
        .await;
}
