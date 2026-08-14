//! Low-latency voice audio datagram streaming and WebRTC session abstractions.

use nexus_core::{ChannelId, NexusError, NexusResult, PeerId};
use quinn::Connection;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, info};

/// Real-time Opus audio frame packet for low-latency voice streaming over UDP/QUIC datagrams.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoicePacket {
    pub channel_id: ChannelId,
    pub sender_id: PeerId,
    pub sequence: u64,
    pub timestamp_ms: u64,
    pub pcm_opus_data: Vec<u8>,
}

impl VoicePacket {
    /// Serialize voice packet into compact postcard binary bytes.
    pub fn serialize(&self) -> NexusResult<Vec<u8>> {
        postcard::to_allocvec(self)
            .map_err(|e| NexusError::Serialization(e.to_string()))
    }

    /// Deserialize voice packet from raw binary bytes.
    pub fn deserialize(bytes: &[u8]) -> NexusResult<Self> {
        postcard::from_bytes(bytes)
            .map_err(|e| NexusError::Serialization(e.to_string()))
    }
}

/// QUIC low-latency un-ordered UDP datagram channel for audio streaming without head-of-line blocking.
pub struct VoiceDatagramChannel {
    connection: Connection,
}

impl VoiceDatagramChannel {
    /// Create new voice datagram channel wrapping an established Quinn QUIC connection.
    pub fn new(connection: Connection) -> Self {
        Self { connection }
    }

    /// Send low-latency voice datagram over QUIC connection without ordering overhead.
    pub async fn send_voice_frame(&self, packet: &VoicePacket) -> NexusResult<()> {
        let bytes = packet.serialize()?;
        self.connection
            .send_datagram(bytes.into())
            .map_err(|e| NexusError::Network(format!("Failed to send voice datagram: {e}")))?;
        debug!("Dispatched audio voice packet sequence {}", packet.sequence);
        Ok(())
    }

    /// Receive low-latency voice datagram frame.
    pub async fn receive_voice_frame(&self) -> NexusResult<VoicePacket> {
        let datagram_bytes = self
            .connection
            .read_datagram()
            .await
            .map_err(|e| NexusError::Network(format!("Failed to read voice datagram: {e}")))?;

        let packet = VoicePacket::deserialize(&datagram_bytes)?;
        Ok(packet)
    }
}

/// WebRTC Peer-to-Peer Voice Session binding interface.
pub struct WebRtcVoiceSession {
    pub peer_id: PeerId,
    pub channel_id: ChannelId,
    frame_tx: mpsc::Sender<VoicePacket>,
    frame_rx: Arc<tokio::sync::Mutex<mpsc::Receiver<VoicePacket>>>,
}

impl WebRtcVoiceSession {
    /// Initialize WebRTC peer audio session with internal ring buffer channels.
    pub fn new(peer_id: PeerId, channel_id: ChannelId) -> Self {
        let (tx, rx) = mpsc::channel(200);
        info!("Initialized WebRTC audio session for peer {} on channel {}", peer_id, channel_id);
        Self {
            peer_id,
            channel_id,
            frame_tx: tx,
            frame_rx: Arc::new(tokio::sync::Mutex::new(rx)),
        }
    }

    /// Push audio frame to WebRTC outbound transmission buffer.
    pub async fn push_outbound_frame(&self, packet: VoicePacket) -> NexusResult<()> {
        self.frame_tx
            .send(packet)
            .await
            .map_err(|e| NexusError::ChannelError(e.to_string()))?;
        Ok(())
    }

    /// Pop next frame from WebRTC inbound audio buffer.
    pub async fn pop_inbound_frame(&self) -> Option<VoicePacket> {
        let mut rx = self.frame_rx.lock().await;
        rx.recv().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_core::{ChannelId, PeerId};

    #[tokio::test]
    async fn test_voice_packet_serialization_roundtrip() {
        let packet = VoicePacket {
            channel_id: ChannelId::new(),
            sender_id: PeerId::new(),
            sequence: 42,
            timestamp_ms: 1700000000,
            pcm_opus_data: vec![0xFE, 0xFF, 0x01, 0x02, 0x03],
        };

        let encoded = packet.serialize().expect("Serialization failed");
        let decoded = VoicePacket::deserialize(&encoded).expect("Deserialization failed");

        assert_eq!(packet.sequence, decoded.sequence);
        assert_eq!(packet.channel_id, decoded.channel_id);
        assert_eq!(packet.pcm_opus_data, decoded.pcm_opus_data);
    }

    #[tokio::test]
    async fn test_webrtc_voice_session_buffer() {
        let session = WebRtcVoiceSession::new(PeerId::new(), ChannelId::new());
        let packet = VoicePacket {
            channel_id: session.channel_id,
            sender_id: session.peer_id,
            sequence: 1,
            timestamp_ms: 100,
            pcm_opus_data: vec![0x10, 0x20],
        };

        session.push_outbound_frame(packet.clone()).await.unwrap();
        let received = session.pop_inbound_frame().await.unwrap();
        assert_eq!(received.sequence, 1);
    }
}
