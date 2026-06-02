use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use shared::protocol::MessageProtocol;

/// Knowledge CLI
#[cfg_attr(feature = "cli", derive(clap::Args))]
#[derive(Debug)]
pub struct Cli {
    /// Knowledge command group
    #[command(subcommand)]
    command: Commands,
}
impl Cli {
    pub fn message(self) -> Vec<u8> {
        self.command
            .to_bytes()
            .expect("msgage serialization failed")
    }
}

/// Knowledge commands
#[cfg_attr(feature = "cli", derive(clap::Subcommand))]
#[derive(Debug, Serialize, Deserialize)]
pub enum Commands {
    /// Add a file to knowledge
    Add(Add),
}

impl MessageProtocol for Commands {}

/// Add a file to knowledge
#[cfg_attr(feature = "cli", derive(clap::Args))]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Add {
    /// File to add
    pub file: PathBuf,
    /// Create a symlink
    pub symlink: bool,
}
