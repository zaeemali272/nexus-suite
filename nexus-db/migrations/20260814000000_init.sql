-- Initial database schema for nexus-suite SQLite storage

CREATE TABLE IF NOT EXISTS peers (
    id TEXT PRIMARY KEY NOT NULL,
    username TEXT NOT NULL,
    display_name TEXT NOT NULL,
    avatar_url TEXT,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS channels (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    channel_type TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS messages (
    id TEXT PRIMARY KEY NOT NULL,
    channel_id TEXT NOT NULL,
    sender_id TEXT NOT NULL,
    content TEXT NOT NULL,
    status TEXT NOT NULL,
    timestamp TEXT NOT NULL,
    is_encrypted INTEGER NOT NULL DEFAULT 1,
    FOREIGN KEY(channel_id) REFERENCES channels(id),
    FOREIGN KEY(sender_id) REFERENCES peers(id)
);

CREATE TABLE IF NOT EXISTS offline_queue (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    message_id TEXT NOT NULL,
    target_peer_id TEXT NOT NULL,
    payload BLOB NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_messages_channel ON messages(channel_id, timestamp);
CREATE INDEX IF NOT EXISTS idx_offline_queue_target ON offline_queue(target_peer_id);
