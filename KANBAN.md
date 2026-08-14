# Nexus Suite — Project Scope, Roadmap & Kanban Tracker

High-Performance, Native, Electron-Free Rust Communication Suite for Linux, Windows, macOS, Android, and iOS.

---

## 🎯 Milestones & Kanban Board

### Phase 1: Storage, Core Protocol & Transport Infrastructure (COMPLETED)

#### 📌 Milestone 1: Workspace & Hyper-Optimized SQLite DB
- [x] **#1 Cargo Workspace Scaffolding**: Setup `nexus-suite` workspace (`nexus-core`, `nexus-db`, `nexus-net`, `nexus-daemon`, `nexus-client`) with strict release profile optimizations (`opt-level = 3`, `lto = true`, `codegen-units = 1`, `panic = "abort"`).
- [x] **#2 `nexus-core` Protocol Layer**: Implement zero-copy domain models, serialization (`serde`, `postcard::to_allocvec`), message schemas, and error taxonomy.
- [x] **#3 `nexus-db` SQLite Storage Layer**: Build `sqlx` connection pool with WAL mode (`PRAGMA journal_mode=WAL`), custom pragmas, automated migrations, and async repositories.

#### 📌 Milestone 2: QUIC Networking & Tokio Daemon
- [x] **#4 `nexus-net` QUIC Transport Layer**: Implement `quinn` QUIC transport engine with multiplexed channels (Control/Text, Audio, File streams) and auto-reconnect.
- [x] **#5 `nexus-daemon` Tokio Actor Runtime**: Headless asynchronous Tokio actor runtime, state machine dispatcher, and offline message queue manager.

---

### Phase 2: Security & Real-Time Audio (COMPLETED)

#### 📌 Milestone 3: E2EE Cryptography & State Actor Model
- [x] **#6 E2EE Cryptographic Subsystem**: Cryptographic key generation, public-key exchange, and symmetric AEAD authenticated encryption primitives (`ring::aead` AES-256-GCM) in `nexus-core`.
- [x] **#7 Client-Side Encryption Middleware**: Client-side message encryption before local SQLite persistence (`nexus-db`) and QUIC network dispatch.
- [x] **#8 Tokio State Actor Routing**: Asynchronous session handshake, key exchange, and cryptographic state routing engine in `nexus-daemon`.

#### 📌 Milestone 4: Native WebRTC Voice Subsystem
- [x] **#9 QUIC / UDP Low-Latency Datagram Pipeline**: Un-ordered, low-priority UDP datagram voice streaming (`quinn::Connection::send_datagram`) in `nexus-net` to eliminate head-of-line blocking.
- [x] **#10 WebRTC Media Session Bindings**: WebRTC data channel & audio session binding interface (`WebRtcVoiceSession`) for peer-to-peer real-time communication.
- [x] **#11 Audio Frame Buffer**: Low-latency audio packetization and ring-buffer pipeline (`VoicePacket`).

---

### Phase 3: Frontend & Design System (COMPLETED)

#### 📌 Milestone 5: Slint Native GPU UI Engine & Theme Engine
- [x] **#12 `nexus-client` Slint UI Engine**: Native GPU-accelerated interface bindings using Slint (sub-40MB memory footprint).
- [x] **#13 Dynamic Theme Engine & Responsive Layouts**: Reactive Tokyo Night Dark vs. Clean Light theme engine and responsive desktop 3-column / mobile viewport layouts.

---

### Phase 4: Distribution & Quality Assurance (COMPLETED)

#### 📌 Milestone 6: Cross-Platform CI/CD & Automated Binary Releases
- [x] **#14 CI/CD Automation**: GitHub Actions pipeline (`.github/workflows/release.yml`) for Linux (`x86_64`, `aarch64`), Windows (`.exe`), macOS (Universal), Android (APK/AAB), and iOS targets.
- [x] **#15 Automated Release Packaging & Benchmarking**: Stripped binary publishing, `criterion` performance benchmark suite (`benches/benchmark.rs`), and complete architectural documentation.
