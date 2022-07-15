use crate::conversation::settings::Settings;
use crate::result::{Error, Result};
use crate::{AppError, ChatId, MessageId, CONVERSATIONS, ERROR_LOGGER};
use async_trait::async_trait;
use itertools::Itertools;
use teloxide::prelude::*;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardButtonKind, InlineKeyboardMarkup};
use tokio_stream::wrappers::UnboundedReceiverStream;
use crate::handlers::messages_handler::{SPECIAL_HANDLERS, SpecialHandler};

const TEMP_SUB_05: &str = "temp_sub_0.5";
const TEMP_SUB_02: &str = "temp_sub_0.2";
const TEMP_SUB_01: &str = "temp_sub_0.1";
const TEMP_ADD_01: &str = "temp_add_0.1";
const TEMP_ADD_02: &str = "temp_add_0.2";
const TEMP_ADD_05: &str = "temp_add_0.5";
const TEMP_BACK: &str = "temp_back";

fn get_temperature_editor_markup() -> InlineKeyboardMarkup {
    const BUTTON_ROW_TEXT: [(&str, &str); 6] = [
        ("-0.5", TEMP_SUB_05),
        ("-0.2", TEMP_SUB_02),
        ("-0.1", TEMP_SUB_01),
        ("+0.1", TEMP_ADD_01),
        ("+0.2", TEMP_ADD_02),
        ("+0.5", TEMP_ADD_05),
    ];
    let button_row = BUTTON_ROW_TEXT
        .into_iter()
        .map(|(text, data)| {
            InlineKeyboardButton::new(
                text,
                InlineKeyboardButtonKind::CallbackData(data.to_string()),
            )
        })
        .collect_vec();
    let back_button = InlineKeyboardButton::new(
        "back",
        InlineKeyboardButtonKind::CallbackData(TEMP_BACK.to_string()),
    );
    InlineKeyboardMarkup::new([button_row, vec![back_button]])
}

fn get_temperature_editor_dialog_text(current_value: f64) -> String {
    format!("Editing temperature\ncurrent value: {:.1}", current_value)
}

async fn handle_temperature_editor_callback_query(
    cx: &UpdateWithCx<&Bot, CallbackQuery>,
    data: &str,
    chat_id: ChatId,
    message_id: MessageId,
    settings: &mut Settings,
) -> Result {
    let delta = match data {
        TEMP_SUB_05 => -0.5,
        TEMP_SUB_02 => -0.2,
        TEMP_SUB_01 => -0.1,
        TEMP_ADD_01 => 0.1,
        TEMP_ADD_02 => 0.2,
        TEMP_ADD_05 => 0.5,
        TEMP_BACK => {
            cx.requester
                .edit_message_text(chat_id, message_id, settings.get_message_text())
                .reply_markup(settings.get_inline_keyboard_markup())
                .send()
                .await?;

            return Ok(());
        }
        data => {
            return Err(Error::App(AppError::UnexpectedCallbackQueryData(
                data.to_string(),
            )))
        }
    };
    let mut new_temperature = settings.temperature + delta;
    if new_temperature < 0. {
        new_temperature = 0.
    };
    if new_temperature > 2. {
        new_temperature = 2.
    };
    settings.temperature = new_temperature;

    cx.requester
        .edit_message_text(
            chat_id,
            message_id,
            get_temperature_editor_dialog_text(settings.temperature),
        )
        .reply_markup(get_temperature_editor_markup())
        .send()
        .await?;

    println!("set temperature to {:?}", new_temperature);

    cx.requester
        .answer_callback_query(cx.update.id.clone())
        .text(format!("Set temperature to: {:.1}", new_temperature))
        .send()
        .await?;

    Ok(())
}

async fn handle_callback_query(cx: UpdateWithCx<&Bot, CallbackQuery>) -> Result {
    let message = cx
        .update
        .message
        .as_ref()
        .ok_or(Error::App(AppError::MessageTooOld))?;

    let chat_id = message.chat_id();

    let mut conversations = CONVERSATIONS.lock().await;
    let conversation = conversations
        .get_mut(chat_id)
        .ok_or(Error::App(AppError::NoConversationRunning(chat_id)))?;
    let settings = &mut conversation.settings;

    macro_rules! answer_callback_query {
        ($( $text:tt )*) => {
            cx
                .requester
                .answer_callback_query(cx.update.id)
                .text( $( $text )* )
                .send()
                .await?;
        }
    }

    match cx.update.data.as_deref() {
        Some(Settings::SETTINGS_CYCLE_MODEL) => {
            println!(
                "editing setting \"model\", current value: {:?}",
                settings.model
            );

            let new_model = settings.cycle_model();

            cx.requester
                .edit_message_reply_markup(chat_id, message.id)
                .reply_markup(settings.get_inline_keyboard_markup())
                .send()
                .await?;

            println!("set model to {:?}", new_model);
            answer_callback_query!(format!("Set model to: {:?}", new_model));
        }
        Some(Settings::SETTINGS_EDIT_TEMPERATURE) => {
            println!(
                "editing setting \"temperature\", current value: {:.1}",
                settings.temperature
            );

            cx.requester
                .edit_message_text(
                    chat_id,
                    message.id,
                    get_temperature_editor_dialog_text(settings.temperature),
                )
                .reply_markup(get_temperature_editor_markup())
                .send()
                .await?;
        }
        Some(Settings::SETTINGS_TOGGLE_TRAILING_SPACE) => {
            println!(
                "editing setting \"trailing space\", current value: {:?}",
                settings.trailing_space_in_prompt
            );

            settings.trailing_space_in_prompt = !settings.trailing_space_in_prompt;

            cx.requester
                .edit_message_reply_markup(chat_id, message.id)
                .reply_markup(settings.get_inline_keyboard_markup())
                .send()
                .await?;

            println!(
                "set trailing space to {:?}",
                settings.trailing_space_in_prompt
            );
            answer_callback_query!(format!(
                "Set trailing space to: {:?}",
                settings.trailing_space_in_prompt
            ));
        }
        Some(Settings::SETTINGS_EDIT_STOP_TOKENS) => {
            // TODO
            println!("editing setting \"stop tokens\"");
        }
        Some(Settings::SETTINGS_EDIT_BOT_NAME) => {
            println!("editing setting \"bot name\"");

            conversation
                .replace_settings_dialog("Enter a new name for the bot:", cx.requester)
                .await?;

            struct EditBotNameHandler(ChatId);

            #[async_trait]
            impl SpecialHandler for EditBotNameHandler {
                async fn handle_message(&self, cx: UpdateWithCx<&Bot, Message>) -> Result<bool> {
                    if let Some(new_name) = cx.update.text() {
                        println!("setting bot name to \"{}\"", new_name);
                        let mut conversations = CONVERSATIONS.lock().await;
                        println!("locked conversations");
                        let conversation = conversations
                            .get_mut(self.0)
                            .ok_or(Error::App(AppError::NoConversationRunning(self.0)))?;
                        let settings = &mut conversation.settings;
                        settings.bot_name = new_name.to_string();
                        Ok(true)
                    } else {
                        cx.answer("Bot name must be text").send().await?;
                        Ok(false)
                    }
                }
            }

            SPECIAL_HANDLERS.lock().await.insert(chat_id, Box::new(EditBotNameHandler(chat_id)));
            answer_callback_query!("Opened bot renaming dialog");
        }
        Some(Settings::SETTINGS_DONE) => {
            conversation
                .deactivate_settings_dialog(cx.requester)
                .await?;
            answer_callback_query!("Done editing settings");
        }
        Some(data) => {
            handle_temperature_editor_callback_query(&cx, data, chat_id, message.id, settings)
                .await?
        }
        None => return Err(Error::App(AppError::NoCallbackQueryData)),
    }

    Ok(())
}

pub async fn callback_queries_handler(rx: DispatcherHandlerRx<&Bot, CallbackQuery>) {
    UnboundedReceiverStream::new(rx)
        .for_each_concurrent(None, |callback| async move {
            let result = handle_callback_query(callback).await;
            ERROR_LOGGER.lock().await.maybe_log(&result);
        })
        .await;
}
