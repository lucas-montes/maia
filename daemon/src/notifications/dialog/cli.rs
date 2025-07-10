use clap::{Args, ValueEnum};

use super::Zenity;

#[derive(Debug, Args, Clone)]
pub struct DialogArgs {
    #[arg(
        short,
        long,
        default_value_t = DialogOptions::Message,
        default_missing_value = "Message",
        value_enum,
        required = false
    )]
    work: DialogOptions,
}

impl DialogArgs {
    pub async fn run(&self) -> i16 {
        match &self.work {
            DialogOptions::Question => Zenity::new().show_question("e"),
            DialogOptions::Input => Zenity::new().show_input("e"),
            DialogOptions::Message => Zenity::new().show_message("e"),
            DialogOptions::Password => Zenity::new().show_password("e"),
        };
        0
    }
}

#[derive(ValueEnum, Clone, Debug)]
pub enum DialogOptions {
    Input,
    Message,
    Password,
    Question,
}
