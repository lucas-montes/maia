use crate::{Command, Response};
use serde_json;

/// Length-prefixed JSON protocol for CLI-daemon communication
/// Format: [4 bytes length][JSON data]
pub trait MessageProtocol {
    fn to_bytes(&self) -> Result<Vec<u8>, serde_json::Error>;
    fn from_bytes(bytes: &[u8]) -> Result<Self, Box<dyn std::error::Error + Send + Sync>>
    where
        Self: Sized;
}

impl MessageProtocol for Command {
    /// Serialize command to length-prefixed bytes
    fn to_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        let json = serde_json::to_string(self)?;
        let json_bytes = json.as_bytes();
        let len = json_bytes.len() as u32;

        let mut result = Vec::with_capacity(4 + json_bytes.len());
        result.extend_from_slice(&len.to_le_bytes());
        result.extend_from_slice(json_bytes);

        Ok(result)
    }

    /// Deserialize command from length-prefixed bytes
    fn from_bytes(bytes: &[u8]) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        if bytes.len() < 4 {
            return Err("Buffer too short for length prefix".into());
        }

        let len_bytes: [u8; 4] = bytes[0..4].try_into()?;
        let expected_len = u32::from_le_bytes(len_bytes) as usize;

        if bytes.len() < 4 + expected_len {
            return Err("Buffer too short for message data".into());
        }

        let json_bytes = &bytes[4..4 + expected_len];
        let json_str = std::str::from_utf8(json_bytes)?;
        let command = serde_json::from_str(json_str)?;

        Ok(command)
    }
}

impl MessageProtocol for Response {
    /// Serialize response to length-prefixed bytes
    fn to_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        let json = serde_json::to_string(self)?;
        let json_bytes = json.as_bytes();
        let len = json_bytes.len() as u32;

        let mut result = Vec::with_capacity(4 + json_bytes.len());
        result.extend_from_slice(&len.to_le_bytes());
        result.extend_from_slice(json_bytes);

        Ok(result)
    }

    /// Deserialize response from length-prefixed bytes
    fn from_bytes(bytes: &[u8]) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        if bytes.len() < 4 {
            return Err("Buffer too short for length prefix".into());
        }

        let len_bytes: [u8; 4] = bytes[0..4].try_into()?;
        let expected_len = u32::from_le_bytes(len_bytes) as usize;

        if bytes.len() < 4 + expected_len {
            return Err("Buffer too short for message data".into());
        }

        let json_bytes = &bytes[4..4 + expected_len];
        let json_str = std::str::from_utf8(json_bytes)?;
        let response = serde_json::from_str(json_str)?;

        Ok(response)
    }
}

/// Helper function to read a complete message from a stream
pub async fn read_message<R: tokio::io::AsyncRead + Unpin>(
    reader: &mut R,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    use tokio::io::AsyncReadExt;

    // Read length prefix
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf).await?;
    let message_len = u32::from_le_bytes(len_buf) as usize;

    // Read message data
    let mut message_buf = vec![0u8; message_len];
    reader.read_exact(&mut message_buf).await?;

    // Return complete message with length prefix
    let mut complete_message = Vec::with_capacity(4 + message_len);
    complete_message.extend_from_slice(&len_buf);
    complete_message.extend_from_slice(&message_buf);

    Ok(complete_message)
}

/// Helper function to write a message to a stream
pub async fn write_message<W: tokio::io::AsyncWrite + Unpin, T: MessageProtocol>(
    writer: &mut W,
    message: &T,
) -> Result<(), Box<dyn std::error::Error>> {
    use tokio::io::AsyncWriteExt;

    let bytes = message.to_bytes()?;
    writer.write_all(&bytes).await?;
    writer.flush().await?;

    Ok(())
}
