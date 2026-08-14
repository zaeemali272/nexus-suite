//! Multiplexed QUIC logical streams (Control, Text, Audio, File).

use nexus_core::{NetworkPacket, NexusError, NexusResult};
use quinn::{RecvStream, SendStream};
use tracing::debug;

/// Distinct multiplexed logical streams over a single QUIC connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamType {
    /// Control and handshake stream (reliable ordered)
    Control,
    /// Text chat messages (reliable ordered)
    Text,
    /// Audio frames stream (unreliable / low latency)
    Audio,
    /// File transfer stream (reliable bulk binary)
    File,
}

/// Helper wrapper for sending multiplexed network packets over a QUIC SendStream.
pub struct TransportChannel;

impl TransportChannel {
    /// Send a NetworkPacket over a Quinn SendStream with zero-copy framing.
    pub async fn send_packet(send: &mut SendStream, packet: &NetworkPacket) -> NexusResult<()> {
        let bytes = packet.serialize()?;
        let len = (bytes.len() as u32).to_be_bytes();

        send.write_all(&len)
            .await
            .map_err(|e| NexusError::Network(format!("Failed to write length header: {e}")))?;
        send.write_all(&bytes)
            .await
            .map_err(|e| NexusError::Network(format!("Failed to write payload: {e}")))?;

        debug!("Sent network packet of length {} bytes", bytes.len());
        Ok(())
    }

    /// Read a NetworkPacket from a Quinn RecvStream.
    pub async fn receive_packet(recv: &mut RecvStream) -> NexusResult<NetworkPacket> {
        let mut len_buf = [0u8; 4];
        recv.read_exact(&mut len_buf)
            .await
            .map_err(|e| NexusError::Network(format!("Failed to read length header: {e}")))?;

        let len = u32::from_be_bytes(len_buf) as usize;
        let mut payload_buf = vec![0u8; len];

        recv.read_exact(&mut payload_buf)
            .await
            .map_err(|e| NexusError::Network(format!("Failed to read payload buffer: {e}")))?;

        let packet = NetworkPacket::deserialize(&payload_buf)?;
        Ok(packet)
    }
}
