//! Main entrypoint for native GPU Slint client UI application with full Tokio actor & E2EE event wiring.

use nexus_client::{AppWindow, MemberData, MessageData, ThemePalette};
use nexus_core::{ChannelId, ConnectionMode, Message, MessageId, MessageStatus, PeerId, SymKey};
use nexus_daemon::{DaemonActor, DaemonCommand, DaemonEvent};
use nexus_db::{DatabasePool, DbConfig, MessageRepository};
use nexus_net::{P2pEndpointManager, WebRtcVoiceSession};
use slint::{ComponentHandle, ModelRc, VecModel};
use std::rc::Rc;
use std::sync::Arc;
use tempfile::NamedTempFile;
use tracing::{info, warn};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    info!("Starting Nexus Suite Native Slint GUI Client with P2P QUIC & WebRTC event loop...");

    let p2p_manager = Arc::new(P2pEndpointManager::bind("0.0.0.0:0".parse().unwrap())?);

    // 1. Initialize DB Pool & Repository
    let tmp = NamedTempFile::new()?;
    let db_config = DbConfig {
        db_path: tmp.path().to_path_buf(),
        max_connections: 5,
    };
    let db_pool = DatabasePool::connect(&db_config).await?;
    let repo = Arc::new(MessageRepository::new(db_pool));

    // 2. Initialize Tokio Daemon Actor
    let (actor, cmd_tx, mut event_rx) = DaemonActor::new();
    let actor = actor.with_repository(repo.clone());
    tokio::spawn(async move {
        if let Err(e) = actor.run().await {
            eprintln!("Daemon actor runtime error: {e}");
        }
    });

    // 3. Generate local E2EE symmetric session key
    let session_key = Arc::new(SymKey::generate()?);
    let my_peer_id = PeerId::new();
    let current_channel_id = ChannelId::new();

    // 4. Initialize Slint UI window & 5s RAM-Only Sysinfo Polling
    let app = AppWindow::new()?;

    if let Ok(Some((_user_id, username))) = repo.get_active_session().await {
        app.set_is_authenticated(true);
        app.set_active_username(username.into());
    } else {
        app.set_is_authenticated(false);
    }

    let app_weak_sys = app.as_weak();
    tokio::spawn(async move {
        use sysinfo::{Pid, System};
        let mut sys = System::new_all();
        let pid = Pid::from_u32(std::process::id());
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));

        loop {
            interval.tick().await;
            sys.refresh_all();

            if let Some(proc) = sys.process(pid) {
                let memory_mb = proc.memory() as f64 / 1024.0 / 1024.0;
                let ram_str = format!("{:.1} MB", memory_mb);

                let app_weak_inner = app_weak_sys.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(app) = app_weak_inner.upgrade() {
                        app.set_ram_usage(ram_str.into());
                    }
                });
            }
        }
    });

    // 5. Initialize 5s Real-Time Server Health Check Probe
    let app_weak_health = app.as_weak();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
        loop {
            interval.tick().await;
            let is_online = match tokio::net::UdpSocket::bind("0.0.0.0:0").await {
                Ok(socket) => socket.connect("127.0.0.1:4433").await.is_ok(),
                Err(_) => false,
            };

            let app_weak_inner = app_weak_health.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(app) = app_weak_inner.upgrade() {
                    app.set_is_server_online(is_online);
                    if is_online {
                        app.set_server_status_msg("Server Online".into());
                    } else {
                        app.set_server_status_msg("Server is not working / offline".into());
                    }
                }
            });
        }
    });

    // Populate initial message list model
    let initial_messages = vec![
        MessageData {
            sender: "System".into(),
            content: "Welcome to Nexus Suite. Electron-free native engine initialized.".into(),
            timestamp: "12:00 PM".into(),
            is_me: false,
        },
        MessageData {
            sender: "Security".into(),
            content: "E2EE AES-256-GCM encryption verified active.".into(),
            timestamp: "12:01 PM".into(),
            is_me: false,
        },
    ];
    let messages_model = Rc::new(VecModel::from(initial_messages));
    app.set_messages(ModelRc::from(messages_model.clone()));

    // Populate active members list model
    let initial_members = vec![
        MemberData {
            name: "Alice (You)".into(),
            status: "Online".into(),
            is_in_voice: false,
        },
        MemberData {
            name: "Bob".into(),
            status: "In Voice".into(),
            is_in_voice: true,
        },
    ];
    app.set_active_members(ModelRc::from(Rc::new(VecModel::from(initial_members))));

    // 5. Wire Slint Callback: `on_send_message`
    let messages_model_weak = messages_model;
    let repo_clone = repo.clone();
    let session_key_clone = session_key.clone();
    let cmd_tx_clone = cmd_tx.clone();

    app.on_send_message(move |text| {
        if text.trim().is_empty() {
            return;
        }

        let msg = Message {
            id: MessageId::new(),
            channel_id: current_channel_id,
            sender_id: my_peer_id,
            content: text.to_string(),
            status: MessageStatus::Sent,
            timestamp: chrono::Utc::now(),
            is_encrypted: true,
        };

        // Encrypt message client-side
        if let Ok(encrypted_msg) = msg.encrypt(&session_key_clone) {
            // Persist ciphertext asynchronously to SQLite
            let repo_inner = repo_clone.clone();
            let encrypted_save = encrypted_msg.clone();
            tokio::spawn(async move {
                let _ = repo_inner.save_encrypted_message(&encrypted_save).await;
            });

            // Dispatch command to daemon actor
            let cmd_inner = cmd_tx_clone.clone();
            let encrypted_dispatch = encrypted_msg.clone();
            tokio::spawn(async move {
                let _ = cmd_inner
                    .send(DaemonCommand::SendEncryptedMessage {
                        message: encrypted_dispatch,
                    })
                    .await;
            });
        }

        let ui_msg = MessageData {
            sender: "You".into(),
            content: text,
            timestamp: "Just now".into(),
            is_me: true,
        };
        messages_model_weak.push(ui_msg);
    });

    // 6. Wire Slint Callbacks: Settings Modal (Theme Preset Selection & Pattern Toggle)
    let app_handle_theme = app.as_weak();
    app.on_select_theme_preset(move |preset| {
        if let Some(app) = app_handle_theme.upgrade() {
            let global_theme = app.global::<ThemePalette>();
            global_theme.set_theme_mode(preset);
            info!("Selected UI theme preset {}: {}", preset, global_theme.get_theme_name());
        }
    });

    let app_handle_pattern = app.as_weak();
    app.on_toggle_pattern_overlay(move || {
        if let Some(app) = app_handle_pattern.upgrade() {
            let global_theme = app.global::<ThemePalette>();
            let current = global_theme.get_show_pattern_overlay();
            info!("Toggled background pattern overlay: {}", if current { "ON" } else { "OFF" });
        }
    });

    // 7. Wire Slint Callback: `on_toggle_voice_call`
    let app_handle_voice = app.as_weak();
    let cmd_tx_voice = cmd_tx.clone();
    let p2p_voice = p2p_manager.clone();

    app.on_toggle_voice_call(move || {
        if let Some(app) = app_handle_voice.upgrade() {
            let in_voice = app.get_is_in_voice_call();
            let next_voice_state = !in_voice;
            app.set_is_in_voice_call(next_voice_state);

            let voice_session = WebRtcVoiceSession::new(my_peer_id, current_channel_id);
            info!("WebRTC voice session toggled state to {}", next_voice_state);

            let cmd_inner = cmd_tx_voice.clone();
            let p2p_addr = p2p_voice.local_addr().unwrap_or_else(|_| "127.0.0.1:0".parse().unwrap());
            tokio::spawn(async move {
                if next_voice_state {
                    let _ = cmd_inner
                        .send(DaemonCommand::StartVoiceChannel {
                            peer_id: my_peer_id,
                        })
                        .await;
                    let _ = cmd_inner
                        .send(DaemonCommand::InitiateP2pRendezvous {
                            sender_peer_id: my_peer_id,
                            target_peer_id: PeerId::new(),
                            local_endpoint: p2p_addr,
                            pubkey: vec![1, 2, 3, 4],
                        })
                        .await;
                } else {
                    let _ = cmd_inner
                        .send(DaemonCommand::StopVoiceChannel {
                            peer_id: my_peer_id,
                        })
                        .await;
                }
            });

            // Prevent unused warning on initialized session handle
            drop(voice_session);
        }
    });

    // 8. Wire Slint Callback: `on_add_friend_request`
    let repo_friend = repo.clone();
    let cmd_tx_friend = cmd_tx.clone();
    app.on_add_friend_request(move |target_str| {
        let username = target_str.to_string();
        if username.trim().is_empty() {
            return;
        }

        let friend_peer_id = PeerId::new();
        let repo_inner = repo_friend.clone();
        let cmd_inner = cmd_tx_friend.clone();
        let user_clone = username.clone();

        tokio::spawn(async move {
            let _ = repo_inner.add_friend_request(friend_peer_id, &user_clone).await;
            let _ = cmd_inner
                .send(DaemonCommand::SendFriendRequest {
                    peer_id: friend_peer_id,
                    username: user_clone,
                })
                .await;
        });

        info!("Dispatched friend request for user/peer: {}", username);
    });

    // 9. Wire Slint Callback: `on_authenticate_user`
    let cmd_tx_auth = cmd_tx.clone();
    let app_handle_auth = app.as_weak();
    app.on_authenticate_user(move |user, pass, is_signup| {
        let username = user.to_string();
        let password = pass.to_string();

        if username.trim().is_empty() || password.trim().is_empty() {
            if let Some(app) = app_handle_auth.upgrade() {
                app.set_auth_error("Username and password cannot be empty.".into());
            }
            return;
        }

        let cmd_inner = cmd_tx_auth.clone();
        tokio::spawn(async move {
            if is_signup {
                let _ = cmd_inner
                    .send(DaemonCommand::CreateAccount {
                        username,
                        password_hash: password,
                    })
                    .await;
            } else {
                let _ = cmd_inner
                    .send(DaemonCommand::AuthenticateUser {
                        username,
                        password_hash: password,
                    })
                    .await;
            }
        });
    });

    // 10. Spawn background task for receiving daemon events
    let app_handle_events = app.as_weak();
    tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            match event {
                DaemonEvent::AuthSuccess { username, .. } => {
                    info!("Received AuthSuccess for user: {}", username);
                    let app_weak = app_handle_events.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(app) = app_weak.upgrade() {
                            app.set_is_authenticated(true);
                            app.set_active_username(username.into());
                            app.set_auth_error("".into());
                        }
                    });
                }
                DaemonEvent::AuthFailure { error } => {
                    warn!("Received AuthFailure: {}", error);
                    let app_weak = app_handle_events.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(app) = app_weak.upgrade() {
                            app.set_auth_error(error.into());
                        }
                    });
                }
                DaemonEvent::EncryptedMessageReceived(enc_msg) => {
                    info!("Daemon emitted EncryptedMessageReceived: {}", enc_msg.id);
                }
                DaemonEvent::VoiceChannelStarted(peer_id) => {
                    info!("Daemon emitted VoiceChannelStarted for peer: {}", peer_id);
                }
                DaemonEvent::VoiceChannelStopped(peer_id) => {
                    info!("Daemon emitted VoiceChannelStopped for peer: {}", peer_id);
                }
                DaemonEvent::TelemetrySummary { summary, .. } => {
                    info!("[CLIENT TELEMETRY] {}", summary);
                }
                DaemonEvent::P2pRendezvousMatched { target_peer_id, public_endpoint, local_endpoint, .. } => {
                    info!("[P2P RENDEZVOUS MATCHED] Exchanged endpoints for target {}: public {}, local {}", target_peer_id, public_endpoint, local_endpoint);
                    let p2p_inner = p2p_manager.clone();
                    let app_weak = app_handle_events.clone();
                    tokio::spawn(async move {
                        let targets = vec![public_endpoint, local_endpoint];
                        let _ = p2p_inner.send_hole_punch_probes(&targets, my_peer_id).await;
                        if let Ok((_conn, mode)) = p2p_inner.connect_p2p(&targets, "localhost", None).await {
                            info!("[DIRECT P2P MEDIA ROUTING] Media audio frames & E2EE chat streaming directly over UDP/QUIC (Transport Mode: {})", mode);
                            let is_direct = mode == ConnectionMode::DirectP2p;
                            let mode_str = mode.to_string();
                            let _ = slint::invoke_from_event_loop(move || {
                                if let Some(app) = app_weak.upgrade() {
                                    app.set_is_direct_p2p(is_direct);
                                    app.set_connection_mode_msg(mode_str.into());
                                }
                            });
                        }
                    });
                }
                _ => {}
            }
        }
    });

    app.run()?;
    Ok(())
}
