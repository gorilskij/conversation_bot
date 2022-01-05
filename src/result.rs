use futures::io;
use std::result;
use teloxide::RequestError;
use crate::ChatId;

#[must_use]
pub type Result<T = ()> = result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    Request(RequestError),
    Io(io::Error),
    Api(openai_api::Error),
    App(AppError),
}

impl From<RequestError> for Error {
    fn from(e: RequestError) -> Self {
        Self::Request(e)
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<AppError> for Error {
    fn from(e: AppError) -> Self {
        Self::App(e)
    }
}

impl From<openai_api::Error> for Error {
    fn from(e: openai_api::Error) -> Self {
        Self::Api(e)
    }
}

#[derive(Debug)]
pub enum AppError {
    StartDuplicateConversation(ChatId),
    EndNonexistentConversation(ChatId),
    MessageWithoutSender(ChatId, String),
    MessageTooOld,
}