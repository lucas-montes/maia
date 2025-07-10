//! Shared message protocol between CLI and daemon
//! This module defines all message types and serialization logic

pub mod messages;
pub mod protocol;

pub use messages::*;
pub use protocol::*;
