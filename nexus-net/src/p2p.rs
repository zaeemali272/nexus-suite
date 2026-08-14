//! Direct Peer-to-Peer Quinn QUIC endpoint & UDP hole punching routines.

use crate::tls::{create_client_crypto_config, create_server_crypto_config, generate_self_signed_cert};
use nexus_core::{
    ConnectionMode, NetworkPacket, NexusError, NexusResult, PacketType, Payload, PeerId,
};
use quinn::{ClientConfig, Connection, Endpoint, ServerConfig};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};

/// Dual-role P2P endpoint manager for direct device-to-device QUIC & UDP hole punching.
pub struct P2pEndpointManager {
    endpoint: Endpoint,
}

impl P2pEndpointManager {
    /// Bind a dual-role Quinn P2P endpoint capable of accepting incoming peer connections
    /// and initiating outbound peer handshakes over a single bound UDP socket.
    pub fn bind(bind_addr: SocketAddr) -> NexusResult<Self> {
        let (cert_chain, private_key) = generate_self_signed_cert()?;
        let quic_server_config = create_server_crypto_config(cert_chain, private_key)?;
        let server_config = ServerConfig::with_crypto(Arc::new(quic_server_config));

        let quic_client_config = create_client_crypto_config()?;
        let client_config = ClientConfig::new(Arc::new(quic_client_config));

        let mut endpoint = Endpoint::server(server_config, bind_addr)
            .map_err(|e| NexusError::Network(format!("Failed to bind P2P QUIC server endpoint: {e}")))?;

        endpoint.set_default_client_config(client_config);

        info!("P2P Quinn endpoint initialized listening on UDP {}", bind_addr);
        Ok(Self { endpoint })
    }

    /// Access underlying Quinn endpoint.
    pub fn inner(&self) -> &Endpoint {
        &self.endpoint
    }

    /// Get local socket address of P2P endpoint.
    pub fn local_addr(&self) -> NexusResult<SocketAddr> {
        self.endpoint
            .local_addr()
            .map_err(|e| NexusError::Network(e.to_string()))
    }

    /// Perform UDP hole punching probing to remote target UDP endpoints (public & local).
    pub async fn send_hole_punch_probes(
        &self,
        target_addrs: &[SocketAddr],
        sender_peer_id: PeerId,
    ) -> NexusResult<()> {
        let socket = tokio::net::UdpSocket::bind("0.0.0.0:0")
            .await
            .map_err(|e| NexusError::Network(format!("Failed to bind probe socket: {e}")))?;

        let probe_packet = NetworkPacket {
            packet_type: PacketType::Rendezvous,
            sender_id: sender_peer_id,
            payload: Payload::HolePunchProbe { sender_peer_id },
        };
        let probe_bytes = probe_packet.serialize()?;

        for &target in target_addrs {
            info!("Sending UDP hole punch probe datagram to target endpoint {}", target);
            for _ in 0..3 {
                let _ = socket.send_to(&probe_bytes, target).await;
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }

        Ok(())
    }

    /// Establish a direct P2P connection to target peer addresses with server fallback.
    pub async fn connect_p2p(
        &self,
        target_addrs: &[SocketAddr],
        server_name: &str,
        relay_addr: Option<SocketAddr>,
    ) -> NexusResult<(Connection, ConnectionMode)> {
        for &remote_addr in target_addrs {
            info!("Attempting direct P2P Quinn connection to {}", remote_addr);
            if let Ok(connecting) = self.endpoint.connect(remote_addr, server_name) {
                match tokio::time::timeout(Duration::from_millis(2500), connecting).await {
                    Ok(Ok(conn)) => {
                        info!("Direct P2P Quinn QUIC connection established with {}", remote_addr);
                        return Ok((conn, ConnectionMode::DirectP2p));
                    }
                    Ok(Err(e)) => {
                        warn!("Direct P2P connection to {} failed: {}", remote_addr, e);
                    }
                    Err(_) => {
                        warn!("Direct P2P connection to {} timed out", remote_addr);
                    }
                }
            }
        }

        if let Some(relay) = relay_addr {
            info!("Direct P2P failed. Falling back to relayed transport via {}", relay);
            let conn = self
                .endpoint
                .connect(relay, server_name)
                .map_err(|e| NexusError::Network(format!("Relay connection failed: {e}")))?
                .await
                .map_err(|e| NexusError::Network(format!("Relay handshake failed: {e}")))?;
            return Ok((conn, ConnectionMode::Relayed));
        }

        Err(NexusError::Network(
            "Failed to establish direct P2P or relayed connection to peer".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_p2p_endpoint_direct_handshake() {
        let ep1 = P2pEndpointManager::bind("127.0.0.1:0".parse().unwrap()).unwrap();
        let ep2 = P2pEndpointManager::bind("127.0.0.1:0".parse().unwrap()).unwrap();

        let addr1 = ep1.local_addr().unwrap();
        let _addr2 = ep2.local_addr().unwrap();

        let handle = tokio::spawn(async move {
            let conn = ep1.endpoint.accept().await.unwrap().await.unwrap();
            let (mut send, mut recv) = conn.accept_bi().await.unwrap();
            let mut buf = [0u8; 3];
            recv.read_exact(&mut buf).await.unwrap();
            assert_eq!(&buf, b"p2p");
            send.write_all(b"ack").await.unwrap();
            send.finish().unwrap();
            conn.closed().await;
        });

        let (conn2, mode) = ep2.connect_p2p(&[addr1], "localhost", None).await.unwrap();
        assert_eq!(mode, ConnectionMode::DirectP2p);

        let (mut send2, mut recv2) = conn2.open_bi().await.unwrap();
        send2.write_all(b"p2p").await.unwrap();
        send2.finish().unwrap();

        let mut buf2 = [0u8; 3];
        recv2.read_exact(&mut buf2).await.unwrap();
        assert_eq!(&buf2, b"ack");

        conn2.close(0u32.into(), b"done");
        handle.await.unwrap();
    }
}
