//! Tokio asynchronous state actor managing application commands, cryptographic sessions, and live connection telemetry.

use crate::sync::OfflineQueueManager;
use chrono::{DateTime, Utc};
use nexus_core::{EncryptedMessage, Message, NexusResult, PeerId, SymKey};
use nexus_db::MessageRepository;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{info, warn};

/// Cryptographic session state lifecycle for peer connections.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionState {
    Uninitialized,
    Handshaking,
    EncryptedSessionEstablished,
    Terminated,
}

/// In-memory record tracking active QUIC peer connection telemetry metrics.
#[derive(Debug, Clone)]
pub struct PeerConnectionRecord {
    pub peer_id: PeerId,
    pub endpoint: SocketAddr,
    pub session_state: SessionState,
    pub rtt_ms: f64,
    pub connected_at: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
}

/// In-memory connection registry tracking active Quinn QUIC handshakes and peer metrics.
#[derive(Debug, Default)]
pub struct PeerRegistry {
    connections: HashMap<PeerId, PeerConnectionRecord>,
}

impl PeerRegistry {
    pub fn new() -> Self {
        Self {
            connections: HashMap::new(),
        }
    }

    pub fn register(&mut self, peer_id: PeerId, endpoint: SocketAddr) {
        let now = Utc::now();
        self.connections.insert(
            peer_id,
            PeerConnectionRecord {
                peer_id,
                endpoint,
                session_state: SessionState::Uninitialized,
                rtt_ms: 0.0,
                connected_at: now,
                last_seen: now,
            },
        );
    }

    pub fn update_state(&mut self, peer_id: &PeerId, state: SessionState) {
        if let Some(record) = self.connections.get_mut(peer_id) {
            record.session_state = state;
            record.last_seen = Utc::now();
        }
    }

    pub fn update_latency(&mut self, peer_id: &PeerId, rtt_ms: f64) {
        if let Some(record) = self.connections.get_mut(peer_id) {
            record.rtt_ms = rtt_ms;
            record.last_seen = Utc::now();
        }
    }

    pub fn remove(&mut self, peer_id: &PeerId) -> Option<PeerConnectionRecord> {
        self.connections.remove(peer_id)
    }

    pub fn active_count(&self) -> usize {
        self.connections.len()
    }

    pub fn get_telemetry_summary(&self) -> String {
        let active = self.connections.len();
        let peers: Vec<String> = self
            .connections
            .iter()
            .map(|(id, record)| {
                format!(
                    "Peer[{}] @ {} (State={:?}, RTT={:.1}ms)",
                    id, record.endpoint, record.session_state, record.rtt_ms
                )
            })
            .collect();
        format!("Active QUIC Connections = {}, Peers: [{}]", active, peers.join(", "))
    }
}

/// Commands sent to the Tokio Daemon Actor.
#[derive(Debug)]
pub enum DaemonCommand {
    ConnectPeer { addr: SocketAddr, peer_id: PeerId },
    DisconnectPeer { peer_id: PeerId },
    InitiateHandshake { peer_id: PeerId },
    ProcessHandshakeKey { peer_id: PeerId, session_key: SymKey },
    UpdatePeerPing { peer_id: PeerId, rtt_ms: f64 },
    SendEncryptedMessage { message: EncryptedMessage },
    StartVoiceChannel { peer_id: PeerId },
    StopVoiceChannel { peer_id: PeerId },
    SendFriendRequest { peer_id: PeerId, username: String },
    AcceptFriendRequest { peer_id: PeerId },
    CreateAccount { username: String, password_hash: String },
    AuthenticateUser { username: String, password_hash: String },
    InitiateP2pRendezvous {
        sender_peer_id: PeerId,
        target_peer_id: PeerId,
        local_endpoint: SocketAddr,
        pubkey: Vec<u8>,
    },
    GetTelemetry,
    Shutdown,
}

/// Events emitted by the Tokio Daemon Actor.
#[derive(Debug, Clone)]
pub enum DaemonEvent {
    PeerConnected(PeerId),
    PeerDisconnected(PeerId),
    SessionStateChanged { peer_id: PeerId, state: SessionState },
    EncryptedMessageReceived(EncryptedMessage),
    MessageReceived(Message),
    VoiceChannelStarted(PeerId),
    VoiceChannelStopped(PeerId),
    FriendRequestReceived { peer_id: PeerId, username: String },
    AuthSuccess { user_id: String, username: String, token: String },
    AuthFailure { error: String },
    TelemetrySummary { summary: String, active_connections: usize },
    P2pRendezvousMatched {
        target_peer_id: PeerId,
        public_endpoint: SocketAddr,
        local_endpoint: SocketAddr,
        pubkey: Vec<u8>,
    },
}

/// Core Tokio Daemon Actor holding state, peer connection telemetry registry, and channels.
pub struct DaemonActor {
    cmd_rx: mpsc::Receiver<DaemonCommand>,
    event_tx: mpsc::Sender<DaemonEvent>,
    queue_manager: OfflineQueueManager,
    peer_registry: PeerRegistry,
    session_states: HashMap<PeerId, SessionState>,
    session_keys: HashMap<PeerId, SymKey>,
    p2p_rendezvous_registry: HashMap<PeerId, (SocketAddr, SocketAddr, Vec<u8>)>,
    repo: Option<Arc<MessageRepository>>,
}

impl DaemonActor {
    /// Create actor channels and handle.
    pub fn new() -> (Self, mpsc::Sender<DaemonCommand>, mpsc::Receiver<DaemonEvent>) {
        let (cmd_tx, cmd_rx) = mpsc::channel(100);
        let (event_tx, event_rx) = mpsc::channel(100);

        let actor = Self {
            cmd_rx,
            event_tx,
            queue_manager: OfflineQueueManager::new(),
            peer_registry: PeerRegistry::new(),
            session_states: HashMap::new(),
            session_keys: HashMap::new(),
            p2p_rendezvous_registry: HashMap::new(),
            repo: None,
        };

        (actor, cmd_tx, event_rx)
    }

    /// Attach persistent database repository to daemon actor.
    pub fn with_repository(mut self, repo: Arc<MessageRepository>) -> Self {
        self.repo = Some(repo);
        self
    }

    /// Run the main actor message processing event loop.
    pub async fn run(mut self) -> NexusResult<()> {
        info!("Nexus Daemon Tokio Actor runtime & live connection tracking engine started");

        while let Some(cmd) = self.cmd_rx.recv().await {
            match cmd {
                DaemonCommand::ConnectPeer { addr, peer_id } => {
                    info!("Connecting to peer {} at address {}", peer_id, addr);
                    self.peer_registry.register(peer_id, addr);
                    self.session_states.insert(peer_id, SessionState::Uninitialized);
                    
                    let telemetry = self.peer_registry.get_telemetry_summary();
                    info!("[TELEMETRY] {}", telemetry);

                    if let Some(repo) = &self.repo {
                        let _ = repo.record_connection_telemetry(&peer_id.to_string(), &addr.to_string(), "Connected", 0.0).await;
                    }

                    let _ = self.event_tx.send(DaemonEvent::PeerConnected(peer_id)).await;

                    // Drain offline queue if any messages were waiting
                    let pending = self.queue_manager.drain(&peer_id).await;
                    for msg in pending {
                        let _ = self.event_tx.send(DaemonEvent::MessageReceived(msg)).await;
                    }
                }
                DaemonCommand::DisconnectPeer { peer_id } => {
                    info!("Disconnecting peer {}", peer_id);
                    if let Some(record) = self.peer_registry.remove(&peer_id) {
                        if let Some(repo) = &self.repo {
                            let _ = repo.record_connection_telemetry(&peer_id.to_string(), &record.endpoint.to_string(), "Disconnected", record.rtt_ms).await;
                        }
                    }
                    self.session_states.remove(&peer_id);
                    self.session_keys.remove(&peer_id);

                    let telemetry = self.peer_registry.get_telemetry_summary();
                    info!("[TELEMETRY] {}", telemetry);

                    let _ = self.event_tx.send(DaemonEvent::PeerDisconnected(peer_id)).await;
                }
                DaemonCommand::InitiateHandshake { peer_id } => {
                    info!("Initiating E2EE handshake with peer {}", peer_id);
                    self.peer_registry.update_state(&peer_id, SessionState::Handshaking);
                    self.session_states.insert(peer_id, SessionState::Handshaking);
                    let _ = self
                        .event_tx
                        .send(DaemonEvent::SessionStateChanged {
                            peer_id,
                            state: SessionState::Handshaking,
                        })
                        .await;
                }
                DaemonCommand::ProcessHandshakeKey { peer_id, session_key } => {
                    info!("Established E2EE session key for peer {}", peer_id);
                    self.session_keys.insert(peer_id, session_key);
                    self.peer_registry.update_state(&peer_id, SessionState::EncryptedSessionEstablished);
                    self.session_states
                        .insert(peer_id, SessionState::EncryptedSessionEstablished);
                    let _ = self
                        .event_tx
                        .send(DaemonEvent::SessionStateChanged {
                            peer_id,
                            state: SessionState::EncryptedSessionEstablished,
                        })
                        .await;
                }
                DaemonCommand::UpdatePeerPing { peer_id, rtt_ms } => {
                    self.peer_registry.update_latency(&peer_id, rtt_ms);
                    info!("[TELEMETRY] Updated ping for peer {}: {:.2} ms", peer_id, rtt_ms);
                }
                DaemonCommand::SendEncryptedMessage { message } => {
                    info!("Dispatching encrypted message {}", message.id);
                    let _ = self
                        .event_tx
                        .send(DaemonEvent::EncryptedMessageReceived(message))
                        .await;
                }
                DaemonCommand::StartVoiceChannel { peer_id } => {
                    info!("Starting low-latency voice channel with peer {}", peer_id);
                    let _ = self
                        .event_tx
                        .send(DaemonEvent::VoiceChannelStarted(peer_id))
                        .await;
                }
                DaemonCommand::StopVoiceChannel { peer_id } => {
                    info!("Stopping voice channel with peer {}", peer_id);
                    let _ = self
                        .event_tx
                        .send(DaemonEvent::VoiceChannelStopped(peer_id))
                        .await;
                }
                DaemonCommand::SendFriendRequest { peer_id, username } => {
                    info!("Dispatching friend request to peer {} ({})", peer_id, username);
                    let _ = self
                        .event_tx
                        .send(DaemonEvent::FriendRequestReceived { peer_id, username })
                        .await;
                }
                DaemonCommand::AcceptFriendRequest { peer_id } => {
                    info!("Accepted friend request from peer {}", peer_id);
                    let _ = self
                        .event_tx
                        .send(DaemonEvent::PeerConnected(peer_id))
                        .await;
                }
                DaemonCommand::CreateAccount { username, password_hash } => {
                    info!("Processing local user account creation for: {}", username);
                    if let Some(repo) = &self.repo {
                        match repo.create_user_account(&username, &password_hash).await {
                            Ok((user_id, token)) => {
                                info!("[AUTH] User account created successfully: {}", username);
                                let _ = self.event_tx.send(DaemonEvent::AuthSuccess { user_id, username, token }).await;
                            }
                            Err(e) => {
                                warn!("[AUTH] User account creation failed for {}: {}", username, e);
                                let _ = self.event_tx.send(DaemonEvent::AuthFailure { error: e.to_string() }).await;
                            }
                        }
                    } else {
                        let user_id = uuid::Uuid::new_v4().to_string();
                        let token = uuid::Uuid::new_v4().to_string();
                        let _ = self.event_tx.send(DaemonEvent::AuthSuccess { user_id, username, token }).await;
                    }
                }
                DaemonCommand::AuthenticateUser { username, password_hash } => {
                    info!("Authenticating local user login for: {}", username);
                    if let Some(repo) = &self.repo {
                        match repo.authenticate_user_credentials(&username, &password_hash).await {
                            Ok(Some(matched_user)) => {
                                info!("[AUTH] User authenticated successfully: {}", matched_user);
                                let user_id = uuid::Uuid::new_v4().to_string();
                                let token = uuid::Uuid::new_v4().to_string();
                                let _ = self.event_tx.send(DaemonEvent::AuthSuccess { user_id, username: matched_user, token }).await;
                            }
                            _ => {
                                warn!("[AUTH] Invalid credentials for user: {}", username);
                                let _ = self.event_tx.send(DaemonEvent::AuthFailure { error: "Invalid username or password".into() }).await;
                            }
                        }
                    } else {
                        let user_id = uuid::Uuid::new_v4().to_string();
                        let token = uuid::Uuid::new_v4().to_string();
                        let _ = self.event_tx.send(DaemonEvent::AuthSuccess { user_id, username, token }).await;
                    }
                }
                DaemonCommand::InitiateP2pRendezvous { sender_peer_id, target_peer_id, local_endpoint, pubkey } => {
                    info!("[MATCHMAKING] Initiating P2P rendezvous between sender {} and target {}", sender_peer_id, target_peer_id);
                    if let Some((target_public, target_local, target_key)) = self.p2p_rendezvous_registry.remove(&target_peer_id) {
                        info!("[SERVER STEP-ASIDE] Matched P2P endpoints between {} and {}. Server stepping out of data path.", sender_peer_id, target_peer_id);
                        let _ = self.event_tx.send(DaemonEvent::P2pRendezvousMatched {
                            target_peer_id,
                            public_endpoint: target_public,
                            local_endpoint: target_local,
                            pubkey: target_key,
                        }).await;
                    } else {
                        let public_endpoint = self.peer_registry.connections.get(&sender_peer_id)
                            .map(|r| r.endpoint)
                            .unwrap_or(local_endpoint);
                        self.p2p_rendezvous_registry.insert(sender_peer_id, (public_endpoint, local_endpoint, pubkey.clone()));
                        info!("[RENDEZVOUS] Stored P2P offer for sender {}. Server awaiting peer response.", sender_peer_id);
                        let _ = self.event_tx.send(DaemonEvent::P2pRendezvousMatched {
                            target_peer_id,
                            public_endpoint,
                            local_endpoint,
                            pubkey,
                        }).await;
                    }
                }
                DaemonCommand::GetTelemetry => {
                    let summary = self.peer_registry.get_telemetry_summary();
                    let count = self.peer_registry.active_count();
                    info!("[TELEMETRY SUMMARY] {}", summary);
                    let _ = self.event_tx.send(DaemonEvent::TelemetrySummary { summary, active_connections: count }).await;
                }
                DaemonCommand::Shutdown => {
                    warn!("Daemon shutdown signal received");
                    break;
                }
            }
        }

        info!("Nexus Daemon Tokio Actor stopped cleanly");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_daemon_actor_handshake_flow() {
        let (actor, cmd_tx, mut event_rx) = DaemonActor::new();
        tokio::spawn(async move {
            actor.run().await.unwrap();
        });

        let peer = PeerId::new();
        cmd_tx.send(DaemonCommand::ConnectPeer { addr: "127.0.0.1:8080".parse().unwrap(), peer_id: peer }).await.unwrap();

        if let Some(DaemonEvent::PeerConnected(p)) = event_rx.recv().await {
            assert_eq!(p, peer);
        }

        cmd_tx.send(DaemonCommand::InitiateHandshake { peer_id: peer }).await.unwrap();

        if let Some(DaemonEvent::SessionStateChanged { peer_id, state }) = event_rx.recv().await {
            assert_eq!(peer_id, peer);
            assert_eq!(state, SessionState::Handshaking);
        } else {
            panic!("Expected SessionStateChanged event");
        }

        let key = SymKey::generate().unwrap();
        cmd_tx.send(DaemonCommand::ProcessHandshakeKey { peer_id: peer, session_key: key }).await.unwrap();

        if let Some(DaemonEvent::SessionStateChanged { peer_id, state }) = event_rx.recv().await {
            assert_eq!(peer_id, peer);
            assert_eq!(state, SessionState::EncryptedSessionEstablished);
        } else {
            panic!("Expected EncryptedSessionEstablished event");
        }
    }
}
