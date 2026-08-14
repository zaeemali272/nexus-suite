//! # Nexus Net
//!
//! `nexus-net` powers the zero-copy, low-latency QUIC transport subsystem for `nexus-suite`.
//! It utilizes `quinn` to provide multiplexed QUIC streams for control, chat messages, low-latency audio,
//! and binary file streaming over UDP.

#![deny(warnings)]
#![forbid(unsafe_code)]

pub mod p2p;
pub mod quic;
pub mod stream;
pub mod tls;
pub mod voice;

pub use p2p::P2pEndpointManager;
pub use quic::{get_connection_rtt_ms, QuicClient, QuicServer};
pub use stream::{StreamType, TransportChannel};
pub use tls::generate_self_signed_cert;
pub use voice::{VoiceDatagramChannel, VoicePacket, WebRtcVoiceSession};
