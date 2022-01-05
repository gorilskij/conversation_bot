#![feature(try_blocks)]
#![deny(unused_must_use)]

use std::env;
use std::time::{SystemTime, UNIX_EPOCH};
use teloxide::Bot;
use lazy_static::lazy_static;
use teloxide::types::MessageKind;
use result::Result;
use tokio::select;
use tokio::signal::ctrl_c;
use futures::lock::Mutex;
use itertools::Itertools;
use error_logging::ErrorLogger;
use teloxide::prelude::*;
use conversation::Conversations;
use crate::result::{AppError, Error};
use openai_api::Client;
use crate::conversation::FromUser;

mod error_logging;
mod result;
mod conversation;

type ChatId = i64;

const DEFAULT_TEMPERATURE: f64 = 0.8;

lazy_static! {
    static ref ERROR_LOGGER: Mutex<ErrorLogger> = Mutex::new(ErrorLogger::new());
    // one per chat
    static ref OPENAI_CLIENT: Mutex<Client> = {
        let token = env::var("OPENAI_TOKEN").expect("error getting openai token");
        let client = Client::new(&token).unwrap();
        Mutex::new(client)
    };
    // static ref RNG: Mutex<StdRng> = Mutex::new(StdRng::from_entropy());
    static ref CONVERSATIONS: Mutex<Conversations> = Mutex::new(Conversations::new());

    static ref TEMPERATURE: Mutex<f64> = Mutex::new(DEFAULT_TEMPERATURE);
}

async fn run_bot() {
    teloxide::enable_logging!();

    let token = env::var("CONVERSATIONBOT_TOKEN").expect("error getting token");
    let bot = Bot::new(token);

    teloxide::repl(bot, |cx| async move {
        let result: Result = try {
            let send_reply = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("you're pre-1970, dude")
                .as_secs()
                .checked_sub(u64::try_from(cx.update.date).expect("message sent before 1970"))
                .map(|elapsed| elapsed < 10) // ignore if message is more than 10 seconds old
                .unwrap_or(true); // don't ignore if message is from the future

            if !send_reply {
                println!("message too old");
                Err(AppError::MessageTooOld)?
            }

            if let MessageKind::Common(_) = cx.update.kind {
                match cx.update.text() {
                    Some("/start" | "/start@nonautisticbot") => {
                        println!("got /start command");
                        match CONVERSATIONS.lock().await.start(cx.chat_id()) {
                            Ok(_) => {}
                            Err(Error::App(AppError::StartDuplicateConversation(_))) => {
                                cx.reply_to("Conversation already running").send().await?; // TODO: since <timestamp>
                            }
                            res => res?,
                        }
                    }
                    Some("/end" | "/end@nonautisticbot") => {
                        println!("got /end command");
                        match CONVERSATIONS.lock().await.end(cx.chat_id()) {
                            Ok(_) => { cx.reply_to("Goodbye").send().await?; },
                            Err(Error::App(AppError::EndNonexistentConversation(_))) => {
                                cx.reply_to("No conversation currently running").send().await?;
                            }
                            res => res?,
                        }
                    }
                    Some(cmd) if cmd.starts_with("/settemperature ") || cmd.starts_with("/settemperature@nonautisticbot ") => {
                        let value = cmd
                            .chars()
                            .skip_while(|&c| c != ' ')
                            .skip(1)
                            .take_while(|c| c.is_numeric())
                            .join("")
                            .parse::<f64>();
                        match value {
                            Ok(temp) if temp <= 2. => {
                                println!("set temperature to {}", temp);
                                *TEMPERATURE.lock().await = temp;
                                cx.reply_to(format!("Set temperature to {}", (temp * 100.).round() / 100.)).send().await?;
                            }
                            _ => {
                                println!("failed to set temperature");
                                cx.reply_to("Invalid value for temperature (must be between 0 and 2)").send().await?;
                            }
                        }
                    }
                    Some("/resettemperature" | "/resettemperature@nonautisticbot") => {
                        *TEMPERATURE.lock().await = DEFAULT_TEMPERATURE;
                        println!("reset temperature to default: {}", DEFAULT_TEMPERATURE);
                        cx.reply_to(format!("Reset temperature to {}", (DEFAULT_TEMPERATURE * 100.).round() / 100.)).send().await?;
                    }
                    Some(msg) => {
                        println!("got message \"{}\"", msg);
                        let user = match cx.update.from(){
                            Some(user) => {
                                println!("sender: user: {}", user.first_name);
                                FromUser::User(user.clone())
                            }
                            None => match &cx.update.via_bot {
                                Some(bot) => {
                                    println!("sender: bot: {}", bot.first_name);
                                    FromUser::OtherBot(bot.clone())
                                }
                                None => {
                                    println!("message without sender");
                                    Err(AppError::MessageWithoutSender(cx.chat_id(), msg.to_string()))?
                                }
                            }
                        };

                        if let Some(conversation) = CONVERSATIONS.lock().await.get_mut(cx.chat_id()) {
                            conversation.add(user.clone(), msg.to_string());
                            let reply = conversation.produce_reply(&*OPENAI_CLIENT.lock().await).await?;
                            cx.reply_to(reply).send().await?;
                        }
                    }
                    _ => {}
                }
            }
        };
        ERROR_LOGGER.lock().await.maybe_log(&result);
        result
    })
        .await;
}

#[tokio::main]
async fn main() {
    select! {
        _ = run_bot() => {},
        _ = ctrl_c() => {
            println!("interrupted");
            ERROR_LOGGER.lock().await.flush().expect("failed to flush error file");
        }
    };
}
