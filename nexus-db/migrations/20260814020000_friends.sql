-- Schema migration for storing E2EE peer friends and contact relationships
CREATE TABLE IF NOT EXISTS friends (
    id TEXT PRIMARY KEY NOT NULL,
    friend_peer_id TEXT NOT NULL UNIQUE,
    username TEXT NOT NULL,
    status TEXT NOT NULL, -- 'Pending', 'Accepted'
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_friends_peer_id ON friends(friend_peer_id);
