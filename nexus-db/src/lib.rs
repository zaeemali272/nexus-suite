//! # Nexus DB
//!
//! `nexus-db` implements a high-performance SQLite storage subsystem backed by `sqlx`.
//! Features automatic WAL mode configuration (`PRAGMA journal_mode=WAL`), connection pooling,
//! embedded schema migrations, and async data access repositories.

#![deny(warnings)]
#![forbid(unsafe_code)]

pub mod pool;
pub mod repository;

pub use pool::{DatabasePool, DbConfig};
pub use repository::MessageRepository;
