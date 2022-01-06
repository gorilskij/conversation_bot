#![feature(try_blocks)]
#![deny(unused_must_use)]

use std::env;
use teloxide::Bot;
use lazy_static::lazy_static;
use tokio::select;
use tokio::signal::ctrl_c;
use futures::lock::Mutex;
use error_logging::ErrorLogger;
use teloxide::prelude::*;
use conversation::Conversations;
use crate::result::AppError;
use openai_api::Client;
use crate::conversation::FromUser;
use crate::handlers::{messages_handler, callback_queries_handler};

mod error_logging;
mod result;
mod conversation;
mod handlers;

type ChatId = i64;
type MessageId = i32;

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
}

async fn run_bot(bot: &'static Bot) {
    teloxide::enable_logging!();

    // TODO: if someone is typing, wait to reply
    // TODO: dynamically set bot commands at every launch, bypass botfather

    Dispatcher::new(bot)
        .messages_handler(messages_handler)
        .callback_queries_handler(callback_queries_handler)
        .dispatch()
        .await;
}

#[tokio::main]
async fn main() {
    lazy_static! {
        static ref BOT: Bot = {
            let token = env::var("CONVERSATIONBOT_TOKEN").expect("error getting token");
            Bot::new(token)
        };
    }

    select! {
        _ = run_bot(&BOT) => {},
        _ = ctrl_c() => {
            println!("interrupted");
            if let Err(e) = CONVERSATIONS.lock().await.cleanup(&BOT).await {
                eprintln!("UNHANDLED ERROR CLEANING UP CONVERSATIONS: {:?}", e);
            }
            ERROR_LOGGER.lock().await.flush().expect("failed to flush error file");
        }
    };
}
