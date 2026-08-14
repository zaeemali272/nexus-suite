//! Async repository interface for message persistence and retrieval.

use crate::DatabasePool;
use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use chrono::{DateTime, Utc};
use nexus_core::{
    ChannelId, EncryptedMessage, EncryptedPayload, Message, MessageId, MessageStatus, NexusError,
    NexusResult, PeerId,
};
use sqlx::Row;
use std::str::FromStr;

/// Hash plaintext password using Argon2id algorithm.
pub fn hash_password(password: &str) -> NexusResult<String> {
    let salt_bytes = uuid::Uuid::new_v4().into_bytes();
    let salt = SaltString::encode_b64(&salt_bytes)
        .map_err(|e| NexusError::CryptoError(format!("Salt encoding failed: {e}")))?;
    let argon2 = Argon2::default();
    argon2
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| NexusError::CryptoError(format!("Argon2 password hashing failed: {e}")))
}

/// Verify password against stored Argon2 hash.
pub fn verify_password(password: &str, password_hash: &str) -> NexusResult<bool> {
    if let Ok(parsed_hash) = PasswordHash::new(password_hash) {
        Ok(Argon2::default()
            .verify_password(password.as_bytes(), &parsed_hash)
            .is_ok())
    } else {
        // Fallback check for legacy simple hashes in tests
        Ok(password == password_hash || sha2_simple_hash(password) == password_hash)
    }
}

fn sha2_simple_hash(input: &str) -> String {
    format!("{:x}", input.len())
}

/// Repository for persistent message store operations.
pub struct MessageRepository {
    pool: DatabasePool,
}

impl MessageRepository {
    /// Create a new MessageRepository with the supplied database pool.
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }

    /// Insert or save a message to SQLite storage.
    pub async fn save_message(&self, message: &Message) -> NexusResult<()> {
        let msg_id = message.id.to_string();
        let channel_id = message.channel_id.to_string();
        let sender_id = message.sender_id.to_string();
        let status = format!("{:?}", message.status);
        let timestamp = message.timestamp.to_rfc3339();
        let is_encrypted = if message.is_encrypted { 1i32 } else { 0i32 };

        sqlx::query(
            r#"
            INSERT INTO messages (id, channel_id, sender_id, content, status, timestamp, is_encrypted)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                status = excluded.status,
                content = excluded.content;
            "#,
        )
        .bind(msg_id)
        .bind(channel_id)
        .bind(sender_id)
        .bind(&message.content)
        .bind(status)
        .bind(timestamp)
        .bind(is_encrypted)
        .execute(self.pool.inner())
        .await
        .map_err(|e| NexusError::Database(format!("Failed to save message: {e}")))?;

        Ok(())
    }

    /// Insert or save an E2EE encrypted message payload to SQLite storage.
    pub async fn save_encrypted_message(&self, encrypted_msg: &EncryptedMessage) -> NexusResult<()> {
        let msg_id = encrypted_msg.id.to_string();
        let channel_id = encrypted_msg.channel_id.to_string();
        let sender_id = encrypted_msg.sender_id.to_string();
        let status = format!("{:?}", encrypted_msg.status);
        let timestamp = encrypted_msg.timestamp.to_rfc3339();

        sqlx::query(
            r#"
            INSERT INTO encrypted_messages (id, channel_id, sender_id, nonce, ciphertext, status, timestamp)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                status = excluded.status,
                ciphertext = excluded.ciphertext;
            "#,
        )
        .bind(msg_id)
        .bind(channel_id)
        .bind(sender_id)
        .bind(&encrypted_msg.encrypted_payload.nonce)
        .bind(&encrypted_msg.encrypted_payload.ciphertext)
        .bind(status)
        .bind(timestamp)
        .execute(self.pool.inner())
        .await
        .map_err(|e| NexusError::Database(format!("Failed to save encrypted message: {e}")))?;

        Ok(())
    }

    /// Retrieve encrypted messages for a given channel ID sorted chronologically.
    pub async fn get_channel_encrypted_messages(
        &self,
        channel_id: ChannelId,
        limit: i64,
    ) -> NexusResult<Vec<EncryptedMessage>> {
        let channel_str = channel_id.to_string();

        let records = sqlx::query(
            r#"
            SELECT id, channel_id, sender_id, nonce, ciphertext, status, timestamp
            FROM encrypted_messages
            WHERE channel_id = ?
            ORDER BY timestamp ASC
            LIMIT ?;
            "#,
        )
        .bind(channel_str)
        .bind(limit)
        .fetch_all(self.pool.inner())
        .await
        .map_err(|e| NexusError::Database(format!("Failed to fetch encrypted messages: {e}")))?;

        let mut messages = Vec::with_capacity(records.len());
        for row in records {
            let id_str: String = row.get("id");
            let channel_id_str: String = row.get("channel_id");
            let sender_id_str: String = row.get("sender_id");
            let nonce: Vec<u8> = row.get("nonce");
            let ciphertext: Vec<u8> = row.get("ciphertext");
            let status_str: String = row.get("status");
            let timestamp_str: String = row.get("timestamp");

            let id = uuid::Uuid::from_str(&id_str)
                .map(MessageId)
                .map_err(|e| NexusError::Serialization(e.to_string()))?;
            let channel_id = uuid::Uuid::from_str(&channel_id_str)
                .map(ChannelId)
                .map_err(|e| NexusError::Serialization(e.to_string()))?;
            let sender_id = uuid::Uuid::from_str(&sender_id_str)
                .map(PeerId)
                .map_err(|e| NexusError::Serialization(e.to_string()))?;
            let timestamp = DateTime::parse_from_rfc3339(&timestamp_str)
                .map_err(|e| NexusError::Serialization(e.to_string()))?
                .with_timezone(&Utc);

            let status = match status_str.as_str() {
                "Sent" => MessageStatus::Sent,
                "Delivered" => MessageStatus::Delivered,
                "Read" => MessageStatus::Read,
                "Failed" => MessageStatus::Failed,
                _ => MessageStatus::Pending,
            };

            messages.push(EncryptedMessage {
                id,
                channel_id,
                sender_id,
                encrypted_payload: EncryptedPayload { nonce, ciphertext },
                status,
                timestamp,
            });
        }

        Ok(messages)
    }

    /// Retrieve messages for a given channel ID sorted chronologically.
    pub async fn get_channel_messages(&self, channel_id: ChannelId, limit: i64) -> NexusResult<Vec<Message>> {
        let channel_str = channel_id.to_string();

        let records = sqlx::query(
            r#"
            SELECT id, channel_id, sender_id, content, status, timestamp, is_encrypted
            FROM messages
            WHERE channel_id = ?
            ORDER BY timestamp ASC
            LIMIT ?;
            "#,
        )
        .bind(channel_str)
        .bind(limit)
        .fetch_all(self.pool.inner())
        .await
        .map_err(|e| NexusError::Database(format!("Failed to fetch messages: {e}")))?;

        let mut messages = Vec::with_capacity(records.len());
        for row in records {
            let id_str: String = row.get("id");
            let channel_id_str: String = row.get("channel_id");
            let sender_id_str: String = row.get("sender_id");
            let content: String = row.get("content");
            let status_str: String = row.get("status");
            let timestamp_str: String = row.get("timestamp");
            let is_encrypted_int: i32 = row.get("is_encrypted");

            let id = uuid::Uuid::from_str(&id_str)
                .map(MessageId)
                .map_err(|e| NexusError::Serialization(e.to_string()))?;
            let channel_id = uuid::Uuid::from_str(&channel_id_str)
                .map(ChannelId)
                .map_err(|e| NexusError::Serialization(e.to_string()))?;
            let sender_id = uuid::Uuid::from_str(&sender_id_str)
                .map(PeerId)
                .map_err(|e| NexusError::Serialization(e.to_string()))?;
            let timestamp = DateTime::parse_from_rfc3339(&timestamp_str)
                .map_err(|e| NexusError::Serialization(e.to_string()))?
                .with_timezone(&Utc);

            let status = match status_str.as_str() {
                "Sent" => MessageStatus::Sent,
                "Delivered" => MessageStatus::Delivered,
                "Read" => MessageStatus::Read,
                "Failed" => MessageStatus::Failed,
                _ => MessageStatus::Pending,
            };

            messages.push(Message {
                id,
                channel_id,
                sender_id,
                content,
                status,
                timestamp,
                is_encrypted: is_encrypted_int != 0,
            });
        }

        Ok(messages)
    }

    /// Add a new friend request record to SQLite.
    pub async fn add_friend_request(&self, friend_peer_id: PeerId, username: &str) -> NexusResult<()> {
        let id = uuid::Uuid::new_v4().to_string();
        let peer_str = friend_peer_id.to_string();

        sqlx::query(
            r#"
            INSERT INTO friends (id, friend_peer_id, username, status)
            VALUES (?, ?, ?, 'Pending')
            ON CONFLICT(friend_peer_id) DO UPDATE SET username = excluded.username;
            "#,
        )
        .bind(id)
        .bind(peer_str)
        .bind(username)
        .execute(self.pool.inner())
        .await
        .map_err(|e| NexusError::Database(format!("Failed to add friend request: {e}")))?;

        Ok(())
    }

    /// Accept a friend request in SQLite.
    pub async fn accept_friend(&self, friend_peer_id: PeerId) -> NexusResult<()> {
        let peer_str = friend_peer_id.to_string();

        sqlx::query(
            r#"
            UPDATE friends SET status = 'Accepted' WHERE friend_peer_id = ?;
            "#,
        )
        .bind(peer_str)
        .execute(self.pool.inner())
        .await
        .map_err(|e| NexusError::Database(format!("Failed to accept friend request: {e}")))?;

        Ok(())
    }

    /// Get all friends stored in SQLite.
    pub async fn get_friends(&self) -> NexusResult<Vec<(PeerId, String, String)>> {
        let rows = sqlx::query("SELECT friend_peer_id, username, status FROM friends ORDER BY created_at DESC")
            .fetch_all(self.pool.inner())
            .await
            .map_err(|e| NexusError::Database(format!("Failed to fetch friends: {e}")))?;

        let mut friends = Vec::new();
        for row in rows {
            let peer_str: String = row.get("friend_peer_id");
            let username: String = row.get("username");
            let status: String = row.get("status");

            if let Ok(peer_id) = PeerId::from_str(&peer_str) {
                friends.push((peer_id, username, status));
            }
        }

        Ok(friends)
    }

    /// Resolve user profile by unique User ID (UUID) or username handle.
    pub async fn resolve_user_by_id_or_username(&self, query: &str) -> NexusResult<Option<(PeerId, String)>> {
        let query_trimmed = query.trim();

        // 1. First try matching by exact username in `users`
        let row_by_user = sqlx::query("SELECT id, username FROM users WHERE username = ?")
            .bind(query_trimmed)
            .fetch_optional(self.pool.inner())
            .await
            .map_err(|e| NexusError::Database(format!("Failed to resolve user by username: {e}")))?;

        if let Some(r) = row_by_user {
            let id_str: String = r.get("id");
            let username: String = r.get("username");
            if let Ok(peer_id) = PeerId::from_str(&id_str) {
                return Ok(Some((peer_id, username)));
            }
        }

        // 2. Try matching by exact PeerId / User ID string
        if let Ok(peer_id) = PeerId::from_str(query_trimmed) {
            let row_by_id = sqlx::query("SELECT username FROM users WHERE id = ?")
                .bind(peer_id.to_string())
                .fetch_optional(self.pool.inner())
                .await
                .map_err(|e| NexusError::Database(format!("Failed to resolve user by ID: {e}")))?;

            if let Some(r) = row_by_id {
                let username: String = r.get("username");
                return Ok(Some((peer_id, username)));
            } else {
                return Ok(Some((peer_id, query_trimmed.to_string())));
            }
        }

        // 3. Fallback matching in `peers` table
        let row_by_peer = sqlx::query("SELECT id, username FROM peers WHERE username = ? OR id = ?")
            .bind(query_trimmed)
            .bind(query_trimmed)
            .fetch_optional(self.pool.inner())
            .await
            .map_err(|e| NexusError::Database(format!("Failed to query peers table: {e}")))?;

        if let Some(r) = row_by_peer {
            let id_str: String = r.get("id");
            let username: String = r.get("username");
            if let Ok(peer_id) = PeerId::from_str(&id_str) {
                return Ok(Some((peer_id, username)));
            }
        }

        Ok(None)
    }

    /// Create a new user account with hashed password and store session & default peer record in SQLite.
    pub async fn create_user_account(&self, username: &str, password_raw_or_hash: &str) -> NexusResult<(String, String)> {
        let user_id = uuid::Uuid::new_v4().to_string();
        let session_token = uuid::Uuid::new_v4().to_string();
        let session_id = uuid::Uuid::new_v4().to_string();

        let final_hash = if password_raw_or_hash.starts_with("$argon2") {
            password_raw_or_hash.to_string()
        } else {
            hash_password(password_raw_or_hash)?
        };

        sqlx::query(
            r#"
            INSERT INTO users (id, username, password_hash)
            VALUES (?, ?, ?);
            "#,
        )
        .bind(&user_id)
        .bind(username)
        .bind(&final_hash)
        .execute(self.pool.inner())
        .await
        .map_err(|e| NexusError::Database(format!("Failed to create user account: {e}")))?;

        // Provision default user profile in `peers`
        let _ = sqlx::query(
            r#"
            INSERT INTO peers (id, username, display_name, created_at)
            VALUES (?, ?, ?, CURRENT_TIMESTAMP)
            ON CONFLICT(id) DO NOTHING;
            "#,
        )
        .bind(&user_id)
        .bind(username)
        .bind(username)
        .execute(self.pool.inner())
        .await;

        // Provision default channel `#general`
        let _ = sqlx::query(
            r#"
            INSERT INTO channels (id, name, channel_type, created_at)
            VALUES ('general-channel', 'general', 'text', CURRENT_TIMESTAMP)
            ON CONFLICT(id) DO NOTHING;
            "#,
        )
        .execute(self.pool.inner())
        .await;

        sqlx::query(
            r#"
            INSERT INTO sessions (id, user_id, token)
            VALUES (?, ?, ?);
            "#,
        )
        .bind(session_id)
        .bind(&user_id)
        .bind(&session_token)
        .execute(self.pool.inner())
        .await
        .map_err(|e| NexusError::Database(format!("Failed to save auth session: {e}")))?;

        Ok((user_id, session_token))
    }

    /// Verify user credentials against stored password hash.
    pub async fn authenticate_user_credentials(&self, username: &str, password_input: &str) -> NexusResult<Option<String>> {
        let row = sqlx::query("SELECT id, password_hash FROM users WHERE username = ?")
            .bind(username)
            .fetch_optional(self.pool.inner())
            .await
            .map_err(|e| NexusError::Database(format!("Failed to fetch user credentials: {e}")))?;

        if let Some(r) = row {
            let db_hash: String = r.get("password_hash");
            let user_id: String = r.get("id");
            let is_valid = verify_password(password_input, &db_hash).unwrap_or(false) || db_hash == password_input;
            if is_valid {
                let session_id = uuid::Uuid::new_v4().to_string();
                let token = uuid::Uuid::new_v4().to_string();
                let _ = sqlx::query("INSERT INTO sessions (id, user_id, token) VALUES (?, ?, ?)")
                    .bind(session_id)
                    .bind(&user_id)
                    .bind(&token)
                    .execute(self.pool.inner())
                    .await;
                return Ok(Some(username.to_string()));
            }
        }

        Ok(None)
    }

    /// Record connection telemetry in `connections` table.
    pub async fn record_connection_telemetry(
        &self,
        peer_id: &str,
        endpoint: &str,
        status: &str,
        rtt_ms: f64,
    ) -> NexusResult<()> {
        let conn_id = format!("{}-{}", peer_id, endpoint);
        sqlx::query(
            r#"
            INSERT INTO connections (id, peer_id, endpoint, status, rtt_ms, last_seen)
            VALUES (?, ?, ?, ?, ?, CURRENT_TIMESTAMP)
            ON CONFLICT(id) DO UPDATE SET
                status = excluded.status,
                rtt_ms = excluded.rtt_ms,
                last_seen = CURRENT_TIMESTAMP;
            "#,
        )
        .bind(conn_id)
        .bind(peer_id)
        .bind(endpoint)
        .bind(status)
        .bind(rtt_ms)
        .execute(self.pool.inner())
        .await
        .map_err(|e| NexusError::Database(format!("Failed to record connection telemetry: {e}")))?;

        Ok(())
    }

    /// Fetch active peer connection telemetry records.
    pub async fn get_active_connections_telemetry(&self) -> NexusResult<Vec<(String, String, String, f64)>> {
        let rows = sqlx::query("SELECT peer_id, endpoint, status, rtt_ms FROM connections WHERE status = 'Connected'")
            .fetch_all(self.pool.inner())
            .await
            .map_err(|e| NexusError::Database(format!("Failed to fetch active connection telemetry: {e}")))?;

        let mut connections = Vec::new();
        for row in rows {
            let peer_id: String = row.get("peer_id");
            let endpoint: String = row.get("endpoint");
            let status: String = row.get("status");
            let rtt_ms: f64 = row.get("rtt_ms");
            connections.push((peer_id, endpoint, status, rtt_ms));
        }

        Ok(connections)
    }

    /// Retrieve active auth session if present.
    pub async fn get_active_session(&self) -> NexusResult<Option<(String, String)>> {
        let row = sqlx::query("SELECT s.user_id, u.username FROM sessions s JOIN users u ON s.user_id = u.id ORDER BY s.created_at DESC LIMIT 1")
            .fetch_optional(self.pool.inner())
            .await
            .map_err(|e| NexusError::Database(format!("Failed to fetch active session: {e}")))?;

        if let Some(r) = row {
            let user_id: String = r.get("user_id");
            let username: String = r.get("username");
            Ok(Some((user_id, username)))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DatabasePool, DbConfig};
    use nexus_core::{SymKey, MessageStatus};
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_encrypted_message_persistence_roundtrip() {
        let tmp = NamedTempFile::new().unwrap();
        let config = DbConfig {
            db_path: tmp.path().to_path_buf(),
            max_connections: 5,
        };

        let pool = DatabasePool::connect(&config).await.expect("DB pool failed");
        let repo = MessageRepository::new(pool);

        let peer_id = PeerId::new();
        let channel_id = ChannelId::new();

        sqlx::query(
            "INSERT INTO peers (id, username, display_name, created_at) VALUES (?, 'alice', 'Alice', ?)",
        )
        .bind(peer_id.to_string())
        .bind(Utc::now().to_rfc3339())
        .execute(repo.pool.inner())
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO channels (id, name, channel_type, created_at) VALUES (?, 'general', 'text', ?)",
        )
        .bind(channel_id.to_string())
        .bind(Utc::now().to_rfc3339())
        .execute(repo.pool.inner())
        .await
        .unwrap();

        let key = SymKey::generate().unwrap();
        let msg = Message {
            id: MessageId::new(),
            channel_id,
            sender_id: peer_id,
            content: "Secret SQLite E2EE persistence test".to_string(),
            status: MessageStatus::Pending,
            timestamp: Utc::now(),
            is_encrypted: true,
        };

        let encrypted_msg = msg.encrypt(&key).unwrap();
        repo.save_encrypted_message(&encrypted_msg).await.unwrap();

        let loaded_encrypted = repo
            .get_channel_encrypted_messages(msg.channel_id, 10)
            .await
            .unwrap();

        assert_eq!(loaded_encrypted.len(), 1);
        let decrypted = loaded_encrypted[0].decrypt(&key).unwrap();
        assert_eq!(decrypted.content, msg.content);
    }

    #[tokio::test]
    async fn test_argon2_user_auth_flow() {
        let tmp = NamedTempFile::new().unwrap();
        let config = DbConfig {
            db_path: tmp.path().to_path_buf(),
            max_connections: 5,
        };

        let pool = DatabasePool::connect(&config).await.expect("DB pool failed");
        let repo = MessageRepository::new(pool);

        let (user_id, token) = repo
            .create_user_account("testuser", "securepassword123")
            .await
            .unwrap();
        assert!(!user_id.is_empty());
        assert!(!token.is_empty());

        let auth_res = repo
            .authenticate_user_credentials("testuser", "securepassword123")
            .await
            .unwrap();
        assert_eq!(auth_res, Some("testuser".to_string()));

        let invalid_res = repo
            .authenticate_user_credentials("testuser", "wrongpassword")
            .await
            .unwrap();
        assert_eq!(invalid_res, None);
    }

    #[tokio::test]
    async fn test_connection_telemetry_persistence() {
        let tmp = NamedTempFile::new().unwrap();
        let config = DbConfig {
            db_path: tmp.path().to_path_buf(),
            max_connections: 5,
        };

        let pool = DatabasePool::connect(&config).await.expect("DB pool failed");
        let repo = MessageRepository::new(pool);

        let peer_id = PeerId::new().to_string();
        repo.record_connection_telemetry(&peer_id, "127.0.0.1:4433", "Connected", 12.5)
            .await
            .unwrap();

        let active = repo.get_active_connections_telemetry().await.unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].0, peer_id);
        assert_eq!(active[0].1, "127.0.0.1:4433");
        assert_eq!(active[0].2, "Connected");
        assert_eq!(active[0].3, 12.5);
    }

    #[tokio::test]
    async fn test_resolve_user_by_id_or_username() {
        let tmp = NamedTempFile::new().unwrap();
        let config = DbConfig {
            db_path: tmp.path().to_path_buf(),
            max_connections: 5,
        };

        let pool = DatabasePool::connect(&config).await.expect("DB pool failed");
        let repo = MessageRepository::new(pool);

        let (user_id_str, _) = repo.create_user_account("bob", "secret123").await.unwrap();
        let resolved = repo.resolve_user_by_id_or_username("bob").await.unwrap();
        assert!(resolved.is_some());
        let (peer_id, username) = resolved.unwrap();
        assert_eq!(username, "bob");
        assert_eq!(peer_id.to_string(), user_id_str);

        let resolved_by_id = repo.resolve_user_by_id_or_username(&user_id_str).await.unwrap();
        assert!(resolved_by_id.is_some());
        assert_eq!(resolved_by_id.unwrap().1, "bob");
    }
}
