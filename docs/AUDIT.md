# Nexus Suite — Codebase Audit (Reality Check)

**Date:** 2026-09-01
**Scope:** Full audit of the existing `nexus-suite` workspace at commit `b9a74e9`.
**Purpose:** Establish an honest baseline before the rewrite. The existing `README.md` and
`KANBAN.md` describe a finished product. The code describes a prototype. This document
records the difference, so no plan is built on a false foundation.

---

## 1. Headline finding

The repository contains **4,143 lines of Rust and Slint** across five crates. Every phase in
`KANBAN.md` is marked `COMPLETED`, and `README.md` publishes a performance comparison table
and four benchmark figures.

Those claims are not supported by the code. What exists is a **coherent, well-organised
skeleton** — good crate boundaries, sensible types, real tests on the parts that are real —
that has been documented as if it were a shipped product. The skeleton is genuinely useful and
much of it is worth keeping. The documentation around it is not.

This matters beyond tidiness: a plan built on "Phase 1–4 complete" would schedule the wrong
work. The sections below are the corrected starting position.

---

## 2. What is real and working

These are implemented, tested, and worth carrying forward:

| Component | File | Assessment |
| :--- | :--- | :--- |
| Domain types | `nexus-core/src/types.rs` | Clean. `PeerId`/`ChannelId`/`MessageId` newtypes over UUID, serde round-trip tested. |
| Wire framing | `nexus-core/src/protocol.rs` | `postcard` binary encoding, tagged packet enum. Sound design. |
| AEAD primitives | `nexus-core/src/crypto.rs` | Correct AES-256-GCM seal/open via `ring`. Nonce is random per message, tamper-rejection is tested. The primitive is fine; the *protocol* around it is not (§3.1). |
| QUIC endpoint | `nexus-net/src/p2p.rs` | Real dual-role `quinn` endpoint with a passing loopback handshake test. Genuine working code. |
| SQLite layer | `nexus-db/` | `sqlx` pool, WAL pragmas, 5 migrations, 703-line repository. Substantial and real. |
| Actor runtime | `nexus-daemon/src/actor.rs` | Real `tokio` mpsc command/event actor with a peer registry. Good bones. |
| Slint UI | `nexus-client/ui/app.slint` | 1,005 lines of real, running native UI. |

**Verdict:** roughly a solid *Milestone 1* of transport and storage plumbing. That is a real
achievement and about 30% of it survives the rewrite largely intact.

---

## 3. What is claimed but absent

### 3.1 There is no end-to-end encryption

This is the most serious gap, because the README advertises E2EE as a headline feature.

`nexus-client/src/main.rs` generates the session key like this:

```rust
let session_key = Arc::new(SymKey::generate()?);
```

A fresh random 256-bit key, generated locally at startup, never exchanged with anybody. There
is **no key agreement, no identity binding, no ratchet, and no forward secrecy**. Two peers
running this build cannot decrypt each other's messages at all — they hold unrelated keys. The
AEAD code is correct in isolation; there is simply no key-management protocol attached to it,
and key management is where essentially all real-world E2EE failures live.

`DaemonCommand::ProcessHandshakeKey { peer_id, session_key: SymKey }` implies the intended
design was to transmit a raw symmetric key to the peer. That would be insecure even if
completed. The rewrite replaces this wholesale with MLS (see `DECISIONS.md`, ADR-002).

### 3.2 Transport authentication is disabled

`nexus-net/src/tls.rs` installs a `SkipServerVerification` verifier that returns
`ServerCertVerified::assertion()` for any certificate presented:

```rust
fn verify_server_cert(...) -> Result<ServerCertVerified, rustls::Error> {
    Ok(ServerCertVerified::assertion())
}
```

Combined with `generate_self_signed_cert()` — which mints a throwaway cert per process, bound to
the meaningless name `"localhost"` — the QUIC layer is **encrypted but entirely unauthenticated**
and trivially machine-in-the-middle-able. This is a normal and acceptable state for a loopback
prototype. It is a critical vulnerability in anything shipped. Tracked as ADR-003 (raw public
key pinning).

### 3.3 There is no voice or video

`VoicePacket` carries a field named `pcm_opus_data: Vec<u8>`, and `WebRtcVoiceSession` is a
`tokio::mpsc` channel with a peer ID attached. Neither name is backed by an implementation:

- No Opus encoder or decoder — no `opus`/`audiopus` dependency exists.
- No audio capture or playback — no `cpal` dependency exists.
- No WebRTC — no `webrtc`/`str0m` dependency exists. `WebRtcVoiceSession` contains no WebRTC.
- No jitter buffer, no packet loss concealment, no echo cancellation, no device handling.
- The test named `test_webrtc_voice_session_buffer` pushes a struct into an mpsc channel and
  reads it back out. It validates `tokio::mpsc`, not voice.

Voice is not partially built; it is unstarted. Realistically it is 6–10 weeks of specialist work
(§ `ROADMAP.md`, Phase 4), and it is the single most underestimated item in the old plan.

### 3.4 There is no server, and no compression

`Dockerfile.server` runs `./nexus-daemon --server --bind 0.0.0.0:4433`. No `--server` flag is
parsed anywhere in `nexus-daemon`; the container would exit immediately. There is no rendezvous
service, no STUN, no relay, and no store-and-forward.

Likewise, the stated goal of "highly compressed, anonymous" storage has no implementation:
messages are stored as `content TEXT` in plaintext SQLite columns, uncompressed. A 139 KB
`nexus.db` sits in the working tree, though `.gitignore` correctly keeps it untracked.

### 3.5 Authentication is not usable

`migrations/20260814030000_auth.sql` stores a `password_hash`, and
`DaemonCommand::CreateAccount { username, password_hash }` accepts that hash **from the caller**.
`nexus-client` depends on `sha2`, which strongly suggests plain SHA-256 password hashing — no
salt, no work factor, no Argon2. It is also conceptually wrong for the target architecture:
in a P2P system with no account server, identity should *be* a keypair. Passwords should only
ever guard the local keystore. The `users`/`sessions` tables are dropped in the rewrite.

### 3.6 The benchmark numbers are unsubstantiated

`README.md` publishes "~480 MB/s AEAD seal", "~42 ns/op serialization", "< 38 MB RAM",
"< 120 ms startup", and "~18 MB binary". `nexus-core/benches/benchmark.rs` exists, but there is
no committed criterion output, no CI job that runs it, and no memory or startup instrumentation
anywhere in the tree. The RAM and startup figures in particular cannot have been measured,
because the features that consume RAM do not exist yet.

These numbers should be deleted and replaced by a CI-enforced performance budget that fails the
build on regression (see `ARCHITECTURE.md` §10). Targets are worth keeping; fabricated
measurements are not.

---

## 4. Disposition of existing code

| Crate | Disposition | Notes |
| :--- | :--- | :--- |
| `nexus-core` | **Keep, extend** | Types and framing survive. `crypto.rs` is demoted to a low-level primitive under a new MLS-based `nexus-crypto`. |
| `nexus-db` | **Keep, re-migrate** | Pool and repository patterns survive. Schema is rewritten: drop `users`/`sessions`, add MLS group state, compress and encrypt message bodies at rest. |
| `nexus-net` | **Keep, harden** | `p2p.rs` and `quic.rs` survive. `tls.rs` is rewritten for raw-public-key pinning. `voice.rs` is deleted and rebuilt as `nexus-media`. |
| `nexus-daemon` | **Keep, refactor** | Actor pattern survives. Command set is rewritten around MLS and identity keys. |
| `nexus-client` | **Keep as reference** | Slint UI is rebuilt against the new design system, reusing layout work where it fits. |
| `Dockerfile.server` | **Delete** | Replaced by a real `nexus-beacon` crate. |
| `KANBAN.md` | **Replace** | Superseded by `ROADMAP.md`. |

---

## 5. Recommendation

Do not restart from an empty directory. The transport, storage, and actor layers are worth
keeping and would cost weeks to rebuild for no gain.

Do rewrite the **security architecture** from scratch (§3.1, §3.2, §3.5) rather than patching
it. Retrofitting key management onto a design that assumed shared symmetric keys reliably
produces subtle, permanent flaws, and this is the one area of the system where "mostly right"
is indistinguishable from "broken".

Proceed as described in `ROADMAP.md`.
