//! Core domain types for users, messages, channels, and peers.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for a peer node or user account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PeerId(pub Uuid);

impl PeerId {
    /// Generates a new random Peer ID.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for PeerId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for PeerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for PeerId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

/// Unique identifier for a chat channel (text, audio, direct message).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChannelId(pub Uuid);

impl ChannelId {
    /// Generates a new random Channel ID.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ChannelId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for ChannelId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique identifier for an individual message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MessageId(pub Uuid);

impl MessageId {
    /// Generates a new random Message ID.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for MessageId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for MessageId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Status lifecycle for a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageStatus {
    Pending,
    Sent,
    Delivered,
    Read,
    Failed,
}

/// Peer connection transport mode (Direct P2P vs Server Relayed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionMode {
    DirectP2p,
    Relayed,
}

impl std::fmt::Display for ConnectionMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectionMode::DirectP2p => write!(f, "Direct P2P"),
            ConnectionMode::Relayed => write!(f, "Relayed"),
        }
    }
}

/// Representation of a user or peer profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    pub id: PeerId,
    pub username: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Core text/media message entity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: MessageId,
    pub channel_id: ChannelId,
    pub sender_id: PeerId,
    pub content: String,
    pub status: MessageStatus,
    pub timestamp: DateTime<Utc>,
    pub is_encrypted: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_creation_and_serialization() {
        let msg = Message {
            id: MessageId::new(),
            channel_id: ChannelId::new(),
            sender_id: PeerId::new(),
            content: "Hello Nexus!".to_string(),
            status: MessageStatus::Pending,
            timestamp: Utc::now(),
            is_encrypted: true,
        };

        let encoded = postcard::to_allocvec(&msg).expect("Failed to serialize message");
        let decoded: Message = postcard::from_bytes(&encoded).expect("Failed to deserialize message");

        assert_eq!(msg.id, decoded.id);
        assert_eq!(msg.content, decoded.content);
        assert_eq!(msg.is_encrypted, decoded.is_encrypted);
    }
}
