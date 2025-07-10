use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};
use shared::protocol::MessageProtocol;

#[derive(Debug, Args)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

impl Cli {
    pub fn message(self) -> Vec<u8> {
        self.command.to_bytes().expect("msgage serialization failed")
    }
}
#[derive(Debug, Subcommand, Serialize, Deserialize)]
pub enum Commands {
    Chat(Chat),
    OneShot {message: String}
}
impl MessageProtocol for Commands {}

#[derive(Debug, Args, Serialize, Deserialize)]
struct Chat{
    message: String,
}
