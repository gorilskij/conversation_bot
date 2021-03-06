use crate::{ChatId, MessageId};
use itertools::Itertools;
use lazy_static::lazy_static;
use openai_api::api::CompletionArgs;
use openai_api::Client;
use settings::{Model, Settings};
use std::collections::hash_map::Entry;
use std::collections::{HashMap, VecDeque};
use std::iter;
use std::num::NonZeroUsize;
use teloxide::prelude::*;
use teloxide::types::User;

use crate::result::{AppError, Result};

pub mod settings;

const DEFAULT_CONVERSATION_LIMIT: usize = 100;
const MAX_TOKENS: u64 = 100;

lazy_static! {
    static ref STOP_TOKENS: Vec<String> = ["\n", ".", "!", "?"].into_iter().map(ToString::to_string).collect();
    // static ref STOP_TOKENS: Vec<String> = ["\n"].into_iter().map(ToString::to_string).collect();
}

#[derive(Clone)]
pub enum FromUser {
    User(User),
    Myself,
}

impl FromUser {
    fn to_name(&self, conversation: &Conversation) -> String {
        use FromUser::*;
        match self {
            User(user) => user.first_name.clone(),
            Myself => conversation.settings.bot_name.clone(),
        }
    }
}

pub struct Conversation {
    // in chronological order, None user means bot sent the message
    messages: VecDeque<(String, String)>,
    limit: Option<NonZeroUsize>,
    last_reply: Option<String>,
    pub settings: Settings,
    pub active_settings_dialog: Option<(ChatId, MessageId)>,
}

impl Conversation {
    // limit = 0 means no limit
    fn new(limit: usize) -> Self {
        Self {
            messages: VecDeque::with_capacity(if limit == 0 || limit > 100 {
                100
            } else {
                limit
            }),
            limit: NonZeroUsize::new(limit),
            last_reply: None,
            settings: Settings::default(),
            active_settings_dialog: None,
        }
    }

    pub fn add(&mut self, from: FromUser, msg: String) {
        if let Some(limit) = self.limit {
            if self.messages.len() == limit.get() {
                self.messages.pop_front();
            }
        }
        self.messages.push_back((from.to_name(self), msg));
    }

    fn generate_prompt(&self) -> String {
        // TODO: keep cached prompt string
        self.messages
            .iter()
            .map(|(user, msg)| format!("{}: {}", user, msg))
            .chain(iter::once(format!("{}: ", self.settings.bot_name)))
            .join("\n")
    }

    async fn interact_with_api(&self, prompt: String, client: &Client) -> Result<String> {
        use Model::*;
        let engine = match self.settings.model {
            Ada => "ada",
            Babbage => "babbage",
            Curie => "curie",
            Davinci => "davinci",
        };

        let args = CompletionArgs::builder()
            .prompt(prompt)
            .engine(engine)
            .max_tokens(MAX_TOKENS)
            .temperature(self.settings.temperature)
            .stop(STOP_TOKENS.clone())
            .build()
            .unwrap();

        let reply = client.complete_prompt(args).await?.choices[0]
            .text
            .trim_start()
            .to_string();

        Ok(reply)
    }

    pub async fn produce_reply(&mut self, client: &Client) -> Result<String> {
        let prompt = self.generate_prompt();
        println!(">> sending prompt:\n{:?}", prompt);
        let mut reply = self.interact_with_api(prompt, client).await?;
        println!(">> received reply: {:?}", reply);

        if let Some(last_reply) = &self.last_reply {
            if &reply == last_reply {
                println!(">> same as last reply, clear and try again");
                self.messages.drain(0..self.messages.len() - 1);
                let prompt = self.generate_prompt();
                println!(">> sending prompt:\n\"{:?}\"", prompt);
                reply = self.interact_with_api(prompt, client).await?;
                println!(">> received reply: \"{:?}\"", reply);
            }
        }

        self.last_reply = Some(reply.clone());

        self.add(FromUser::Myself, reply.clone());
        Ok(reply)
    }

    pub fn clear_history(&mut self) {
        self.messages.clear();
    }

    pub async fn deactivate_settings_dialog(&mut self, requester: &Bot) -> Result {
        if let Some((chat_id, message_id)) = self.active_settings_dialog.take() {
            requester
                .edit_message_text(chat_id, message_id, self.settings.get_done_text())
                // implicitly remove reply markup
                .send()
                .await?;

            println!("deactivated settings dialog");
        }

        Ok(())
    }

    pub async fn replace_settings_dialog(&mut self, text: &str, requester: &Bot) -> Result {
        if let Some((chat_id, message_id)) = self.active_settings_dialog.take() {
            requester
                .edit_message_text(chat_id, message_id, text)
                .send()
                .await?;

            println!("replaced settings dialog with \"{}\"", text);
        }

        Ok(())
    }
}

pub struct Conversations(HashMap<ChatId, Conversation>);

impl Conversations {
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    pub fn begin(&mut self, chat: ChatId) -> Result {
        match self.0.entry(chat) {
            Entry::Occupied(_) => Err(AppError::ConversationAlreadyRunning(chat))?,
            Entry::Vacant(entry) => {
                entry.insert(Conversation::new(DEFAULT_CONVERSATION_LIMIT));
                Ok(())
            }
        }
    }

    pub fn end(&mut self, chat: ChatId) -> Result {
        match self.0.entry(chat) {
            Entry::Occupied(entry) => {
                entry.remove();
                Ok(())
            }
            Entry::Vacant(_) => Err(AppError::NoConversationRunning(chat))?,
        }
    }

    pub fn get_mut(&mut self, chat: ChatId) -> Option<&mut Conversation> {
        self.0.get_mut(&chat)
    }

    // called before terminating the bot
    pub async fn cleanup(&mut self, requester: &Bot) -> Result {
        for conversation in self.0.values_mut() {
            conversation.deactivate_settings_dialog(requester).await?;
        }
        Ok(())
    }
}
