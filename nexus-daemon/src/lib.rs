//! # Nexus Daemon
//!
//! `nexus-daemon` houses the Tokio-powered background actor runtime, state synchronization engine,
//! offline message queue queueing service, and system event bus.

#![deny(warnings)]
#![forbid(unsafe_code)]

pub mod actor;
pub mod sync;

pub use actor::{DaemonActor, DaemonCommand, DaemonEvent, PeerConnectionRecord, PeerRegistry, SessionState};
pub use sync::OfflineQueueManager;
