//! QUIC Multiplexed Wire Protocol Packets.

use crate::types::{ChannelId, Message, PeerId};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

/// High-level packet type header for stream routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PacketType {
    /// Control signaling & handshake
    Control = 0x01,
    /// Chat text & rich media messages
    TextMessage = 0x02,
    /// Ultra-low latency voice audio frames
    AudioFrame = 0x03,
    /// Binary file transmission stream
    FileChunk = 0x04,
    /// Peer discovery & ping/pong heartbeats
    Heartbeat = 0x05,
    /// Direct P2P matchmaking rendezvous
    Rendezvous = 0x06,
}

/// Dynamic payload variant for multiplexed streams.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Payload {
    Ping,
    Pong,
    Handshake { peer_id: PeerId, username: String },
    Chat(Message),
    Voice { channel_id: ChannelId, sequence: u64, pcm_data: Vec<u8> },
    FileTransfer { file_id: String, chunk_index: u64, data: Vec<u8> },
    RendezvousOffer {
        target_peer_id: PeerId,
        public_endpoint: SocketAddr,
        local_endpoint: SocketAddr,
        pubkey: Vec<u8>,
    },
    RendezvousResponse {
        sender_peer_id: PeerId,
        public_endpoint: SocketAddr,
        local_endpoint: SocketAddr,
        pubkey: Vec<u8>,
    },
    HolePunchProbe {
        sender_peer_id: PeerId,
    },
}

/// Network wire wrapper packet for QUIC stream transport.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkPacket {
    pub packet_type: PacketType,
    pub sender_id: PeerId,
    pub payload: Payload,
}

impl NetworkPacket {
    /// Serialize network packet into zero-copy postcard byte vector.
    pub fn serialize(&self) -> Result<Vec<u8>, crate::NexusError> {
        postcard::to_allocvec(self)
            .map_err(|e| crate::NexusError::Serialization(e.to_string()))
    }

    /// Deserialize network packet from raw bytes.
    pub fn deserialize(bytes: &[u8]) -> Result<Self, crate::NexusError> {
        postcard::from_bytes(bytes)
            .map_err(|e| crate::NexusError::Serialization(e.to_string()))
    }
}
