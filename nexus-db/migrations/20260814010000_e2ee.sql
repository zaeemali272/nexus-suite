-- Migration 20260814010000_e2ee.sql: Encrypted Message Storage for E2EE

CREATE TABLE IF NOT EXISTS encrypted_messages (
    id TEXT PRIMARY KEY NOT NULL,
    channel_id TEXT NOT NULL,
    sender_id TEXT NOT NULL,
    nonce BLOB NOT NULL,
    ciphertext BLOB NOT NULL,
    status TEXT NOT NULL,
    timestamp TEXT NOT NULL,
    FOREIGN KEY(channel_id) REFERENCES channels(id),
    FOREIGN KEY(sender_id) REFERENCES peers(id)
);

CREATE INDEX IF NOT EXISTS idx_encrypted_messages_channel ON encrypted_messages(channel_id, timestamp);
