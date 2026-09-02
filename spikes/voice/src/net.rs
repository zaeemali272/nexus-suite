//! QUIC datagram transport for the voice spike.
//!
//! SPIKE ONLY: uses a self-signed certificate with verification disabled, so two
//! machines can be paired with no setup. Production must use raw-public-key
//! pinning per ADR-003. This code must never be copied into the real client.

use quinn::{ClientConfig, Endpoint, ServerConfig};
use std::net::SocketAddr;
use std::sync::Arc;

#[derive(Debug)]
struct SkipVerify;

impl rustls::client::danger::ServerCertVerifier for SkipVerify {
    fn verify_server_cert(
        &self,
        _e: &rustls::pki_types::CertificateDer<'_>,
        _i: &[rustls::pki_types::CertificateDer<'_>],
        _s: &rustls::pki_types::ServerName<'_>,
        _o: &[u8],
        _n: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }
    fn verify_tls12_signature(
        &self,
        _m: &[u8],
        _c: &rustls::pki_types::CertificateDer<'_>,
        _d: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }
    fn verify_tls13_signature(
        &self,
        _m: &[u8],
        _c: &rustls::pki_types::CertificateDer<'_>,
        _d: &rustls::DigitallySignedStruct,
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

pub fn server_endpoint(bind: SocketAddr) -> anyhow::Result<Endpoint> {
    let cert = rcgen::generate_simple_self_signed(vec!["nexus-voice-spike".into()])?;
    let cert_der = rustls::pki_types::CertificateDer::from(cert.cert.der().to_vec());
    let key = rustls::pki_types::PrivateKeyDer::Pkcs8(cert.key_pair.serialize_der().into());

    let mut tls = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![cert_der], key)?;
    tls.alpn_protocols = vec![b"nexus-voice-spike".to_vec()];

    let mut sc = ServerConfig::with_crypto(Arc::new(quinn::crypto::rustls::QuicServerConfig::try_from(
        Arc::new(tls),
    )?));
    // Datagrams are what carry audio; make room for a healthy queue.
    Arc::get_mut(&mut sc.transport).unwrap().datagram_receive_buffer_size(Some(1 << 20));
    Ok(Endpoint::server(sc, bind)?)
}

pub fn client_endpoint() -> anyhow::Result<Endpoint> {
    let mut tls = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(SkipVerify))
        .with_no_client_auth();
    tls.alpn_protocols = vec![b"nexus-voice-spike".to_vec()];

    let mut ep = Endpoint::client("0.0.0.0:0".parse()?)?;
    let mut cc = ClientConfig::new(Arc::new(quinn::crypto::rustls::QuicClientConfig::try_from(
        Arc::new(tls),
    )?));
    let mut tp = quinn::TransportConfig::default();
    tp.datagram_send_buffer_size(1 << 20);
    cc.transport_config(Arc::new(tp));
    ep.set_default_client_config(cc);
    Ok(ep)
}

/// Wire format for one audio frame. Deliberately tiny: 12 bytes of header.
pub struct VoiceFrame;

impl VoiceFrame {
    pub fn encode(seq: u32, send_us: u64, opus: &[u8], out: &mut Vec<u8>) {
        out.clear();
        out.extend_from_slice(&seq.to_le_bytes());
        out.extend_from_slice(&send_us.to_le_bytes());
        out.extend_from_slice(opus);
    }

    pub fn decode(b: &[u8]) -> Option<(u32, u64, &[u8])> {
        if b.len() < 12 {
            return None;
        }
        let seq = u32::from_le_bytes(b[0..4].try_into().ok()?);
        let ts = u64::from_le_bytes(b[4..12].try_into().ok()?);
        Some((seq, ts, &b[12..]))
    }
}
