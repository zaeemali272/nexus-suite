//! End-to-End Encryption (E2EE) cryptographic primitives built on `ring::aead`.

use crate::{Message, MessageId, ChannelId, PeerId, MessageStatus, NexusError, NexusResult};
use chrono::{DateTime, Utc};
use ring::aead::{BoundKey, UnboundKey, SealingKey, OpeningKey, AES_256_GCM, Nonce, NonceSequence, NONCE_LEN};
use ring::rand::{SystemRandom, SecureRandom};
use serde::{Deserialize, Serialize};

/// 256-bit symmetric encryption key wrapper for E2EE payload encryption.
#[derive(Clone)]
pub struct SymKey(pub [u8; 32]);

impl SymKey {
    /// Generate a cryptographically secure random 256-bit symmetric key.
    pub fn generate() -> NexusResult<Self> {
        let rng = SystemRandom::new();
        let mut key_bytes = [0u8; 32];
        rng.fill(&mut key_bytes)
            .map_err(|_| NexusError::CryptoError("Failed to generate secure random symmetric key".into()))?;
        Ok(Self(key_bytes))
    }
}

impl std::fmt::Debug for SymKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SymKey([REDACTED])")
    }
}

/// Encrypted payload container holding IV/nonce and AEAD ciphertext bytes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EncryptedPayload {
    pub nonce: Vec<u8>,
    pub ciphertext: Vec<u8>,
}

struct OneNonceSequence(Option<Nonce>);

impl NonceSequence for OneNonceSequence {
    fn advance(&mut self) -> Result<Nonce, ring::error::Unspecified> {
        self.0.take().ok_or(ring::error::Unspecified)
    }
}

/// Encrypt raw byte buffer with 256-bit AES-GCM AEAD authenticated encryption.
pub fn encrypt_bytes(key: &SymKey, plaintext: &[u8]) -> NexusResult<EncryptedPayload> {
    let rng = SystemRandom::new();
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rng.fill(&mut nonce_bytes)
        .map_err(|_| NexusError::CryptoError("Failed to generate random IV/nonce".into()))?;

    let unbound_key = UnboundKey::new(&AES_256_GCM, &key.0)
        .map_err(|_| NexusError::CryptoError("Invalid AEAD key size".into()))?;

    let nonce = Nonce::try_assume_unique_for_key(&nonce_bytes)
        .map_err(|_| NexusError::CryptoError("Invalid nonce".into()))?;

    let nonce_seq = OneNonceSequence(Some(nonce));
    let mut sealing_key = SealingKey::new(unbound_key, nonce_seq);

    let mut in_out = plaintext.to_vec();
    sealing_key.seal_in_place_append_tag(ring::aead::Aad::empty(), &mut in_out)
        .map_err(|_| NexusError::CryptoError("Encryption sealing failed".into()))?;

    Ok(EncryptedPayload {
        nonce: nonce_bytes.to_vec(),
        ciphertext: in_out,
    })
}

/// Decrypt an EncryptedPayload using the provided SymKey.
pub fn decrypt_payload(key: &SymKey, payload: &EncryptedPayload) -> NexusResult<Vec<u8>> {
    if payload.nonce.len() != NONCE_LEN {
        return Err(NexusError::CryptoError("Invalid nonce length".into()));
    }

    let unbound_key = UnboundKey::new(&AES_256_GCM, &key.0)
        .map_err(|_| NexusError::CryptoError("Invalid AEAD key size".into()))?;

    let nonce = Nonce::try_assume_unique_for_key(&payload.nonce)
        .map_err(|_| NexusError::CryptoError("Invalid nonce bytes".into()))?;

    let nonce_seq = OneNonceSequence(Some(nonce));
    let mut opening_key = OpeningKey::new(unbound_key, nonce_seq);

    let mut in_out = payload.ciphertext.clone();
    let decrypted = opening_key.open_in_place(ring::aead::Aad::empty(), &mut in_out)
        .map_err(|_| NexusError::CryptoError("AEAD decryption or tag verification failed".into()))?;

    Ok(decrypted.to_vec())
}

/// Encrypted message structure representing client-side encrypted message ready for persistence and transit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedMessage {
    pub id: MessageId,
    pub channel_id: ChannelId,
    pub sender_id: PeerId,
    pub encrypted_payload: EncryptedPayload,
    pub status: MessageStatus,
    pub timestamp: DateTime<Utc>,
}

impl Message {
    /// Encrypt plaintext message content using client-side symmetric key.
    pub fn encrypt(&self, key: &SymKey) -> NexusResult<EncryptedMessage> {
        let payload = encrypt_bytes(key, self.content.as_bytes())?;
        Ok(EncryptedMessage {
            id: self.id,
            channel_id: self.channel_id,
            sender_id: self.sender_id,
            encrypted_payload: payload,
            status: self.status,
            timestamp: self.timestamp,
        })
    }
}

impl EncryptedMessage {
    /// Decrypt ciphertext payload back into a plaintext Message struct.
    pub fn decrypt(&self, key: &SymKey) -> NexusResult<Message> {
        let decrypted_bytes = decrypt_payload(key, &self.encrypted_payload)?;
        let content = String::from_utf8(decrypted_bytes)
            .map_err(|e| NexusError::Serialization(e.to_string()))?;

        Ok(Message {
            id: self.id,
            channel_id: self.channel_id,
            sender_id: self.sender_id,
            content,
            status: self.status,
            timestamp: self.timestamp,
            is_encrypted: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_e2ee_encrypt_decrypt_roundtrip() {
        let key = SymKey::generate().unwrap();
        let msg = Message {
            id: MessageId::new(),
            channel_id: ChannelId::new(),
            sender_id: PeerId::new(),
            content: "Top secret quantum-resistant payload".to_string(),
            status: MessageStatus::Pending,
            timestamp: Utc::now(),
            is_encrypted: true,
        };

        let encrypted_msg = msg.encrypt(&key).expect("Message encryption failed");
        assert_ne!(encrypted_msg.encrypted_payload.ciphertext, msg.content.as_bytes());

        let decrypted_msg = encrypted_msg.decrypt(&key).expect("Message decryption failed");
        assert_eq!(decrypted_msg.content, msg.content);
        assert_eq!(decrypted_msg.id, msg.id);
    }

    #[test]
    fn test_tampered_ciphertext_rejection() {
        let key = SymKey::generate().unwrap();
        let payload = encrypt_bytes(&key, b"Sensitive data").unwrap();

        let mut tampered = payload;
        if let Some(byte) = tampered.ciphertext.first_mut() {
            *byte ^= 0xFF;
        }

        let result = decrypt_payload(&key, &tampered);
        assert!(result.is_err());
    }
}
