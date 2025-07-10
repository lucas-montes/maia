use serde::{Deserialize, Serialize};
use serde_json;
use std::fmt;

/// Protocol-specific errors
#[derive(Debug)]
pub enum ProtocolError {
    /// JSON serialization/deserialization error
    Json(serde_json::Error),
    /// UTF-8 conversion error
    Utf8(std::str::Utf8Error),
    /// Buffer is too short for length prefix
    BufferTooShort { expected: usize, actual: usize },
    /// Invalid message length
    InvalidLength(u32),
    /// Message data is incomplete
    IncompleteMessage { expected: usize, actual: usize },
    /// IO error during message reading/writing
    Io(std::io::Error),
    /// Message too large (over 64MB)
    MessageTooLarge(u32),
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProtocolError::Json(e) => write!(f, "JSON error: {}", e),
            ProtocolError::Utf8(e) => write!(f, "UTF-8 conversion error: {}", e),
            ProtocolError::BufferTooShort { expected, actual } => {
                write!(f, "Buffer too short: expected {} bytes, got {}", expected, actual)
            }
            ProtocolError::InvalidLength(len) => {
                write!(f, "Invalid message length: {}", len)
            }
            ProtocolError::IncompleteMessage { expected, actual } => {
                write!(f, "Incomplete message: expected {} bytes, got {}", expected, actual)
            }
            ProtocolError::Io(e) => write!(f, "IO error: {}", e),
            ProtocolError::MessageTooLarge(size) => {
                write!(f, "Message too large: {} bytes (max 64MB)", size)
            }
        }
    }
}

impl std::error::Error for ProtocolError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ProtocolError::Json(e) => Some(e),
            ProtocolError::Utf8(e) => Some(e),
            ProtocolError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<serde_json::Error> for ProtocolError {
    fn from(error: serde_json::Error) -> Self {
        ProtocolError::Json(error)
    }
}

impl From<std::str::Utf8Error> for ProtocolError {
    fn from(error: std::str::Utf8Error) -> Self {
        ProtocolError::Utf8(error)
    }
}

impl From<std::io::Error> for ProtocolError {
    fn from(error: std::io::Error) -> Self {
        ProtocolError::Io(error)
    }
}

impl From<std::array::TryFromSliceError> for ProtocolError {
    fn from(_: std::array::TryFromSliceError) -> Self {
        ProtocolError::BufferTooShort { expected: 4, actual: 0 }
    }
}

/// Length-prefixed JSON protocol for CLI-daemon communication
/// Format: [4 bytes length][JSON data]
pub trait MessageProtocol where Self: Sized {
    /// Serialize command to length-prefixed bytes
    fn to_bytes(&self) -> Result<Vec<u8>, ProtocolError> where Self: Serialize {
        let json = serde_json::to_string(self)?;
        let json_bytes = json.as_bytes();
        let len = json_bytes.len() as u32;

        // Prevent extremely large messages (64MB limit)
        const MAX_MESSAGE_SIZE: u32 = 64 * 1024 * 1024;
        if len > MAX_MESSAGE_SIZE {
            return Err(ProtocolError::MessageTooLarge(len));
        }

        let mut result = Vec::with_capacity(4 + json_bytes.len());
        result.extend_from_slice(&len.to_le_bytes());
        result.extend_from_slice(json_bytes);

        Ok(result)
    }
    /// Deserialize command from length-prefixed bytes
    fn from_bytes(bytes: &[u8]) -> Result<Self, ProtocolError> where Self: for<'de> Deserialize<'de> {
        if bytes.len() < 4 {
            return Err(ProtocolError::BufferTooShort {
                expected: 4,
                actual: bytes.len()
            });
        }

        let len_bytes: [u8; 4] = bytes[0..4].try_into()?;
        let expected_len = u32::from_le_bytes(len_bytes) as usize;

        // Validate message length
        const MAX_MESSAGE_SIZE: u32 = 64 * 1024 * 1024;
        if expected_len as u32 > MAX_MESSAGE_SIZE {
            return Err(ProtocolError::MessageTooLarge(expected_len as u32));
        }

        if bytes.len() < 4 + expected_len {
            return Err(ProtocolError::IncompleteMessage {
                expected: 4 + expected_len,
                actual: bytes.len()
            });
        }

        let json_bytes = &bytes[4..4 + expected_len];
        let json_str = std::str::from_utf8(json_bytes)?;
        let command = serde_json::from_str(json_str)?;

        Ok(command)
    }
}
