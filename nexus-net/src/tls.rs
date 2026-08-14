//! TLS configuration and self-signed certificate generation utilities.

use nexus_core::{NexusError, NexusResult};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use std::sync::Arc;

/// Generate self-signed TLS certificates for QUIC peer-to-peer connections.
pub fn generate_self_signed_cert() -> NexusResult<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>)> {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_string(), "nexus-peer".to_string()])
        .map_err(|e| NexusError::CryptoError(format!("Failed to generate self-signed cert: {e}")))?;

    let cert_der = cert.cert.der().to_vec();
    let key_der = cert.key_pair.serialize_der();

    let cert_chain = vec![CertificateDer::from(cert_der)];
    let private_key = PrivateKeyDer::Pkcs8(key_der.into());

    Ok((cert_chain, private_key))
}

/// Create a server CryptoConfig for Quinn using rustls.
pub fn create_server_crypto_config(
    cert_chain: Vec<CertificateDer<'static>>,
    private_key: PrivateKeyDer<'static>,
) -> NexusResult<quinn::crypto::rustls::QuicServerConfig> {
    let mut server_config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(cert_chain, private_key)
        .map_err(|e| NexusError::Network(format!("TLS server config failed: {e}")))?;

    server_config.alpn_protocols = vec![b"nexus-quic/1.0".to_vec()];

    let quic_config = quinn::crypto::rustls::QuicServerConfig::try_from(Arc::new(server_config))
        .map_err(|e| NexusError::Network(format!("Quinn server config conversion failed: {e}")))?;

    Ok(quic_config)
}

/// Dummy verifier for peer-to-peer certificate validation.
#[derive(Debug)]
struct SkipServerVerification;

impl rustls::client::danger::ServerCertVerifier for SkipServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ED25519,
            rustls::SignatureScheme::RSA_PSS_SHA256,
        ]
    }
}

/// Create a client CryptoConfig for Quinn.
pub fn create_client_crypto_config() -> NexusResult<quinn::crypto::rustls::QuicClientConfig> {
    let mut client_config = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(SkipServerVerification))
        .with_no_client_auth();

    client_config.alpn_protocols = vec![b"nexus-quic/1.0".to_vec()];

    let quic_config = quinn::crypto::rustls::QuicClientConfig::try_from(Arc::new(client_config))
        .map_err(|e| NexusError::Network(format!("Quinn client config conversion failed: {e}")))?;

    Ok(quic_config)
}
