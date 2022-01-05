use std::collections::{HashMap, VecDeque};
use std::collections::hash_map::Entry;
use std::iter;
use std::num::NonZeroUsize;
use itertools::Itertools;
use openai_api::api::CompletionArgs;
use teloxide::types::User;
use crate::ChatId;
use lazy_static::lazy_static;
use openai_api::Client;
use crate::TEMPERATURE;

use crate::result::{Result, AppError};

const DEFAULT_CONVERSATION_LIMIT: usize = 100;
const MAX_TOKENS: u64 = 100;

lazy_static! {
    static ref STOP_TOKENS: Vec<String> = ["\n", ".", "!", "?"].into_iter().map(ToString::to_string).collect();
}

#[derive(Clone)]
pub enum FromUser {
    User(User),
    Myself,
    OtherBot(User),
}

pub struct Conversation {
    // in chronological order, None user means bot sent the message
    messages: VecDeque<(FromUser, String)>,
    limit: Option<NonZeroUsize>,
    last_reply: Option<String>,
}

impl Conversation {
    // limit = 0 means no limit
    fn new(limit: usize) -> Self {
        Self {
            messages: VecDeque::with_capacity(20),
            limit: NonZeroUsize::new(limit),
            last_reply: None,
        }
    }

    pub fn add(&mut self, from: FromUser, msg: String) {
        if let Some(limit) = self.limit {
            if self.messages.len() == limit.get() {
                self.messages.pop_front();
            }
        }
        self.messages.push_back((from, msg));
    }

    fn generate_prompt(&self) -> String {
        // TODO: keep cached prompt string
        self
            .messages
            .iter()
            .map(|(user, msg)| {
                match user {
                    FromUser::User(user) | FromUser::OtherBot(user) => {
                        format!("{}: {}", user.first_name, msg)
                    }
                    FromUser::Myself => {
                        format!("You: {}", msg)
                    }
                }
            })
            .chain(iter::once("You: ".to_string()))
            .join("\n")
    }

    async fn interact_with_api(prompt: String, client: &Client) -> Result<String> {
        let args = CompletionArgs::builder()
            .prompt(prompt)
            .engine("davinci")
            .max_tokens(MAX_TOKENS)
            .temperature(*TEMPERATURE.lock().await)
            .stop(STOP_TOKENS.clone())
            .build()
            .unwrap();
        let reply = client
            .complete_prompt(args)
            .await?
            .choices[0]
            .text
            .trim_start()
            .to_string();
        Ok(reply)
    }

    pub async fn produce_reply(&mut self, client: &Client) -> Result<String> {
        let prompt = self.generate_prompt();
        println!(">> sending prompt:\n\"{:?}\"", prompt);
        let mut reply = Self::interact_with_api(prompt, client).await?;
        println!(">> received reply: \"{:?}\"", reply);

        if let Some(last_reply) = &self.last_reply {
            if &reply == last_reply {
                println!(">> same as last reply, clear and try again");
                self.messages.drain(0..self.messages.len() - 1);
                let prompt = self.generate_prompt();
                println!(">> sending prompt:\n\"{:?}\"", prompt);
                reply = Self::interact_with_api(prompt, client).await?;
                println!(">> received reply: \"{:?}\"", reply);
            }
        }

        self.last_reply = Some(reply.clone());

        self.add(FromUser::Myself, reply.clone());
        Ok(reply)
    }
}

pub struct Conversations(HashMap<ChatId, Conversation>);

impl Conversations {
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    #[must_use]
    pub fn start(&mut self, chat: ChatId) -> Result {
        match self.0.entry(chat) {
            Entry::Occupied(_) => Err(AppError::StartDuplicateConversation(chat))?,
            Entry::Vacant(entry) => {
                entry.insert(Conversation::new(DEFAULT_CONVERSATION_LIMIT));
                Ok(())
            },
        }
    }

    #[must_use]
    pub fn end(&mut self, chat: ChatId) -> Result {
        match self.0.entry(chat) {
            Entry::Occupied(entry) => { entry.remove(); Ok(()) },
            Entry::Vacant(_) => Err(AppError::EndNonexistentConversation(chat))?,
        }
    }

    pub fn get_mut(&mut self, chat: ChatId) -> Option<&mut Conversation> {
        self.0.get_mut(&chat)
    }
}
