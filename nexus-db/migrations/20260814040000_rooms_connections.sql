-- Migration 20260814040000_rooms_connections.sql: Rooms and Connection Telemetry Tables

CREATE TABLE IF NOT EXISTS rooms (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS connections (
    id TEXT PRIMARY KEY NOT NULL,
    peer_id TEXT NOT NULL,
    endpoint TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'Connected',
    rtt_ms REAL NOT NULL DEFAULT 0.0,
    connected_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_seen DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_connections_peer_id ON connections(peer_id);
