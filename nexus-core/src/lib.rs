//! # Nexus Core
//!
//! `nexus-core` contains the fundamental data structures, protocol definitions, zero-copy serialization
//! primitives, and error handling abstractions for the `nexus-suite` ecosystem.

#![deny(warnings)]
#![forbid(unsafe_code)]

pub mod crypto;
pub mod error;
pub mod protocol;
pub mod types;

pub use crypto::{decrypt_payload, encrypt_bytes, EncryptedMessage, EncryptedPayload, SymKey};
pub use error::{NexusError, NexusResult};
pub use protocol::{NetworkPacket, PacketType, Payload};
pub use types::{ChannelId, ConnectionMode, Message, MessageId, MessageStatus, PeerId, UserProfile};
