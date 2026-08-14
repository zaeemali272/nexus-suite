//! Error taxonomy for `nexus-suite`.

use thiserror::Error;

/// Primary error enum representing all domain, network, storage, and cryptographic errors.
#[derive(Debug, Error)]
pub enum NexusError {
    #[error("Database error: {0}")]
    Database(String),

    #[error("Network connection error: {0}")]
    Network(String),

    #[error("Serialization / deserialization failure: {0}")]
    Serialization(String),

    #[error("Peer not found: {0}")]
    PeerNotFound(String),

    #[error("Channel error: {0}")]
    ChannelError(String),

    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),

    #[error("Cryptographic operation failed: {0}")]
    CryptoError(String),

    #[error("Internal runtime error: {0}")]
    Internal(String),
}

/// Specialized Result type for `nexus-suite`.
pub type NexusResult<T> = Result<T, NexusError>;
