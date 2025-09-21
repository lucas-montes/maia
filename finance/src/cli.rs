use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use shared::protocol::MessageProtocol;

/// Finance CLI
#[cfg_attr(feature = "cli", derive(clap::Args))]
#[derive(Debug)]
pub struct Cli {
    /// Finance command group
    #[command(subcommand)]
    command: Commands,
}

impl Cli {
    pub fn message(self) -> Vec<u8> {
        self.command.to_bytes().expect("msgage serialization failed")
    }
}

/// Finance commands
#[cfg_attr(feature = "cli", derive(clap::Subcommand))]
#[derive(Debug, Serialize, Deserialize)]
pub enum Commands {
    /// Add an expense from a ticket/receipt
    Expense(Expense)
}

impl MessageProtocol for Commands {}

/// Expense command
#[cfg_attr(feature = "cli", derive(clap::Args))]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Expense{
    /// Path to the ticket/receipt image
    pub ticket: PathBuf,
}
