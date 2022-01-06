use itertools::Itertools;
use std::fmt::Debug;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardButtonKind, InlineKeyboardMarkup};

#[derive(Copy, Clone, Debug)]
pub enum Model {
    // in increasing order of power
    Ada,
    Babbage,
    Curie,
    Davinci,
}

pub struct Settings {
    pub model: Model,
    pub temperature: f64,
    pub trailing_space_in_prompt: bool,
    pub stop_tokens: Vec<String>,
}

impl Settings {
    const DEFAULT_MODEL: Model = Model::Davinci;
    const DEFAULT_TEMPERATURE: f64 = 0.8;
    const DEFAULT_TRAILING_SPACE: bool = true;
    const DEFAULT_STOP_TOKENS: &'static [&'static str] = &["\n", ".", "!", "?"];

    pub fn cycle_model(&mut self) -> Model {
        use Model::*;
        self.model = match self.model {
            Ada => Babbage,
            Babbage => Curie,
            Curie => Davinci,
            Davinci => Ada,
        };
        self.model
    }

    pub const SETTINGS_CYCLE_MODEL: &'static str = "settings_cycle_model";
    pub const SETTINGS_EDIT_TEMPERATURE: &'static str = "settings_edit_temperature";
    pub const SETTINGS_TOGGLE_TRAILING_SPACE: &'static str = "settings_toggle_trailing_space";
    pub const SETTINGS_EDIT_STOP_TOKENS: &'static str = "settings_edit_stop_tokens";
    pub const SETTINGS_DONE: &'static str = "settings_done";

    pub fn get_message_text(&self) -> String {
        "Editing settings".to_string()
    }

    pub fn get_done_text(&self) -> String {
        format!(
            "Done editing settings\n    model: {:?}\n    temperature: {:.1}\n    \
            trailing space: {}\n    stop tokens: {}",
            self.model,
            self.temperature,
            self.trailing_space_in_prompt,
            self.stop_tokens
                .iter()
                .map(|t| format!("{:?}", t))
                .join(", "),
        )
    }

    pub fn get_inline_keyboard_markup(&self) -> InlineKeyboardMarkup {
        let button_text: [&[(_, _)]; 3] = [
            &[
                (
                    format!("model: {:?}", self.model),
                    Self::SETTINGS_CYCLE_MODEL,
                ),
                (
                    format!("temperature: {:.1}", self.temperature),
                    Self::SETTINGS_EDIT_TEMPERATURE,
                ),
            ],
            &[
                (
                    format!("trailing space: {}", self.trailing_space_in_prompt),
                    Self::SETTINGS_TOGGLE_TRAILING_SPACE,
                ),
                (
                    format!(
                        "stop tokens: {}",
                        self.stop_tokens
                            .iter()
                            .map(|t| if t == "\n" {
                                "\\n".to_string()
                            } else {
                                format!("\"{}\"", t)
                            })
                            .join(", ")
                    ),
                    Self::SETTINGS_EDIT_STOP_TOKENS,
                ),
            ],
            &[("done".to_string(), Self::SETTINGS_DONE)],
        ];
        let buttons = button_text.into_iter().map(|row| {
            row.into_iter().map(|(text, data)| {
                InlineKeyboardButton::new(
                    text,
                    InlineKeyboardButtonKind::CallbackData(data.to_string()),
                )
            })
        });
        InlineKeyboardMarkup::new(buttons)
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            model: Self::DEFAULT_MODEL,
            temperature: Self::DEFAULT_TEMPERATURE,
            trailing_space_in_prompt: Self::DEFAULT_TRAILING_SPACE,
            stop_tokens: Self::DEFAULT_STOP_TOKENS
                .into_iter()
                .map(ToString::to_string)
                .collect_vec(),
        }
    }
}
