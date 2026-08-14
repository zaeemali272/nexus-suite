//! QUIC Client & Server Quinn Endpoint wrappers.

use crate::tls::{create_client_crypto_config, create_server_crypto_config, generate_self_signed_cert};
use nexus_core::{NexusError, NexusResult};
use quinn::{Connection, Endpoint, ServerConfig};
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::info;

/// QUIC Server endpoint for receiving incoming peer connections.
pub struct QuicServer {
    endpoint: Endpoint,
}

impl QuicServer {
    /// Bind QUIC server endpoint to specified address using self-signed TLS.
    pub fn bind(bind_addr: SocketAddr) -> NexusResult<Self> {
        let (cert_chain, private_key) = generate_self_signed_cert()?;
        let quic_server_config = create_server_crypto_config(cert_chain, private_key)?;

        let server_config = ServerConfig::with_crypto(Arc::new(quic_server_config));

        let endpoint = Endpoint::server(server_config, bind_addr)
            .map_err(|e| NexusError::Network(format!("Failed to bind QUIC server endpoint: {e}")))?;

        info!("QUIC server listening on UDP {}", bind_addr);
        Ok(Self { endpoint })
    }

    /// Accept incoming peer connection.
    pub async fn accept(&self) -> NexusResult<Connection> {
        let incoming = self
            .endpoint
            .accept()
            .await
            .ok_or_else(|| NexusError::Network("Server endpoint closed".to_string()))?;

        let connection = incoming
            .await
            .map_err(|e| NexusError::Network(format!("QUIC connection handshake failed: {e}")))?;

        info!("Accepted QUIC connection from {}", connection.remote_address());
        Ok(connection)
    }

    /// Get local socket address of server endpoint.
    pub fn local_addr(&self) -> NexusResult<SocketAddr> {
        self.endpoint
            .local_addr()
            .map_err(|e| NexusError::Network(e.to_string()))
    }
}

/// QUIC Client endpoint for initiating peer connections.
pub struct QuicClient {
    endpoint: Endpoint,
}

impl QuicClient {
    /// Create a new client endpoint bound to an ephemeral port.
    pub fn new() -> NexusResult<Self> {
        let mut endpoint = Endpoint::client("0.0.0.0:0".parse().unwrap())
            .map_err(|e| NexusError::Network(format!("Failed to bind QUIC client endpoint: {e}")))?;

        let quic_client_config = create_client_crypto_config()?;
        let client_config = quinn::ClientConfig::new(Arc::new(quic_client_config));

        endpoint.set_default_client_config(client_config);
        Ok(Self { endpoint })
    }

    /// Connect to remote QUIC peer address.
    pub async fn connect(&self, remote_addr: SocketAddr, server_name: &str) -> NexusResult<Connection> {
        let connecting = self
            .endpoint
            .connect(remote_addr, server_name)
            .map_err(|e| NexusError::Network(format!("Failed to initiate connection: {e}")))?;

        let connection = connecting
            .await
            .map_err(|e| NexusError::Network(format!("QUIC client handshake failed: {e}")))?;

        info!("Successfully established QUIC connection to {}", remote_addr);
        Ok(connection)
    }
}

/// Helper to extract real-time RTT latency in milliseconds from an active Quinn QUIC connection.
pub fn get_connection_rtt_ms(conn: &Connection) -> f64 {
    conn.rtt().as_secs_f64() * 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_quic_server_client_handshake() {
        let server = QuicServer::bind("127.0.0.1:0".parse().unwrap()).unwrap();
        let server_addr = server.local_addr().unwrap();

        let client = QuicClient::new().unwrap();

        let server_handle = tokio::spawn(async move {
            let conn = server.accept().await.unwrap();
            let (mut send, mut recv) = conn.accept_bi().await.unwrap();
            let mut buf = [0u8; 5];
            recv.read_exact(&mut buf).await.unwrap();
            assert_eq!(&buf, b"ping!");
            send.write_all(b"pong!").await.unwrap();
            send.finish().unwrap();
            conn.closed().await;
        });

        let client_conn = client.connect(server_addr, "localhost").await.unwrap();
        let (mut send, mut recv) = client_conn.open_bi().await.unwrap();

        send.write_all(b"ping!").await.unwrap();
        send.finish().unwrap();
        let mut buf = [0u8; 5];
        recv.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"pong!");

        client_conn.close(0u32.into(), b"done");
        server_handle.await.unwrap();
    }
}
