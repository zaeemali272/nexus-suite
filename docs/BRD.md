# Nexus Suite — Business Requirements Document

**Version:** 1.0
**Date:** 2026-09-01
**Status:** Draft for approval
**Owner:** zaeem

---

## 1. Executive summary

Nexus Suite is a native, peer-to-peer communication application for desktop and mobile that
combines the everyday functions of Discord, Slack, and Element into one client — text channels,
direct messages, voice and video calls, and file sharing — without the resource cost of a
browser engine and without an operator who can read your data.

Two constraints define the product and separate it from everything in the market:

1. **It is not a web page in a window.** The client is native Rust with a GPU-rendered UI. The
   target is under 60 MB of RAM at idle and under 250 ms to an interactive window — roughly an
   order of magnitude better than Electron-based competitors.
2. **There is no server that holds your data.** Identity is a cryptographic keypair on your own
   device. Messages and calls travel directly between peers, encrypted end-to-end. The only
   optional infrastructure is a "beacon" that helps peers find each other and forwards sealed
   ciphertext it cannot read — it never holds an account, a contact list, or a message.

The product exists because the incumbents force a bad trade: Discord and Slack are fast enough
but centralised and heavy; Element is decentralised and private but slow and unpleasant to use.
Nexus targets the empty quadrant — private *and* fast *and* pleasant.

---

## 2. Problem statement

### 2.1 The resource problem

Electron-based clients embed a full Chromium browser per application. Typical idle measurements
on desktop Linux:

| Application | Idle RAM | Cold start | Install size |
| :--- | :--- | :--- | :--- |
| Discord | 350–700 MB | 3–5 s | ~180 MB |
| Slack | 400–800 MB | 4–6 s | ~200 MB |
| Element Desktop | 300–600 MB | 3–5 s | ~250 MB |

On a machine running several of these alongside a browser and an IDE, the communication tools
alone can consume 1.5–2 GB. This is a tax paid continuously, all day, for software that mostly
displays scrolling text.

### 2.2 The trust problem

Centralised platforms see everything that is not end-to-end encrypted — and even where content
is protected, the *metadata* remains: who talks to whom, how often, when, for how long, from
which IP. Social-graph metadata is frequently more revealing than message content, and it is
retained, subpoenable, and monetisable.

### 2.3 The usability problem in the alternatives

Privacy-respecting alternatives exist but consistently lose on experience: slow sync, confusing
device-verification flows, cryptic room aliases, unreliable calls, and UI that feels like a
reference implementation. Users who care about privacy still route around them.

### 2.4 The opportunity

No product currently offers native performance, genuine end-to-end encryption with metadata
minimisation, and a polished modern interface simultaneously. That is the gap Nexus targets.

---

## 3. Goals and non-goals

### 3.1 Goals

- **G1 — Performance.** Idle RAM < 60 MB; cold start < 250 ms; sustained 60 fps scrolling on
  integrated graphics; interaction latency under 16 ms.
- **G2 — Privacy by architecture.** No infrastructure operator, including the author, is
  technically capable of reading message content or reconstructing the social graph. This is a
  property of the design, not a policy promise.
- **G3 — Self-hosted real-time.** Voice and video are peer-to-peer, hosted from participants'
  own machines. No media servers, therefore no per-minute infrastructure cost.
- **G4 — Consolidation.** One client covering the persistent-community model (Discord/Slack
  servers and channels) and the private-messaging model (DMs, small groups).
- **G5 — Quality.** A minimalist, modern, professional interface that a user would choose on
  looks and feel alone, independent of the privacy argument.
- **G6 — Portability.** One Rust core across Linux, Windows, macOS, and Android.

### 3.2 Non-goals for v1

Explicitly out of scope, to be revisited only after v1 ships:

- **Federation with Matrix, XMPP, or IRC.** Interoperability is a large, permanent architectural
  commitment. Deferred (see ADR-001).
- **Large group calls (>8 participants).** Full-mesh P2P does not scale past roughly 8 peers;
  going further requires an SFU media server, which contradicts G3. Deferred.
- **Public directory / user discovery by search.** A searchable directory of users is precisely
  the centralised metadata store the architecture is built to avoid. Contacts are added by
  invite link or QR code.
- **Bots, apps, and a third-party integration platform.**
- **iOS.** Deferred from v1: background socket restrictions and mandatory APNs routing conflict
  with the metadata-minimal design and need dedicated research (see §8, R4).
- **Server-side message search across devices**, video recording, screen-share annotation,
  threads, and voice channels with more than 8 concurrent speakers.

---

## 4. Target users

**Primary — the technical power user.** Developers, sysadmins, security-conscious professionals,
Linux users. Runs many applications at once and notices resource cost. Already frustrated with
Electron. Will self-host, read the threat model, and evaluate the architecture critically. This
group is the initial adopter and the source of credibility.

**Secondary — the small private group.** Friends, families, gaming groups, small teams of
5–50 people who want a private space with reliable voice. Cares about "it just works" and does
not want to run infrastructure. Reached through the primary user, who sets it up for them.

**Tertiary — the privacy-critical user.** Journalists, activists, legal and medical
professionals who need metadata resistance as an operational requirement. Small in number, high
in influence, and the group that most rigorously validates the security claims.

---

## 5. Functional requirements

Priority: **P0** = required to ship v1. **P1** = required within two releases of v1.
**P2** = desirable, unscheduled.

### 5.1 Identity and contacts

| ID | Requirement | Priority |
| :--- | :--- | :--- |
| FR-1.1 | Identity is an Ed25519 keypair generated on-device at first launch. No email, no phone number, no username registration, no server-side account. | P0 |
| FR-1.2 | The private key never leaves the device unencrypted; the local keystore is encrypted at rest with an Argon2id-derived key from a user passphrase or the OS keychain. | P0 |
| FR-1.3 | Users add contacts by exchanging an invite link (`nexus:` URI) or scanning a QR code encoding the public key and reachability hints. | P0 |
| FR-1.4 | Every contact displays a **safety number** (a short fingerprint of both public keys) for out-of-band verification, and warns prominently if it changes. | P0 |
| FR-1.5 | A user may hold one identity across multiple devices; a new device is authorised by an existing device, never by a server. | P1 |
| FR-1.6 | Identity can be exported and imported as an encrypted recovery file. Losing all devices and the recovery file means losing the identity permanently, and this must be stated unambiguously in the UI at key-generation time. | P0 |
| FR-1.7 | Support for multiple unlinked identities (personal / work) in one installation. | P2 |

### 5.2 Messaging

| ID | Requirement | Priority |
| :--- | :--- | :--- |
| FR-2.1 | 1:1 direct messages, end-to-end encrypted with forward secrecy and post-compromise security. | P0 |
| FR-2.2 | Group messaging up to 200 members, end-to-end encrypted, with efficient member add/remove. | P0 |
| FR-2.3 | "Spaces" — named collections of topic channels, providing the Discord/Slack organisational model. A space is a cryptographic group; there is no server that owns it. | P0 |
| FR-2.4 | Messages support markdown, code blocks with syntax highlighting, inline links, and emoji. | P0 |
| FR-2.5 | Replies, edits, deletions, and reactions. | P0 |
| FR-2.6 | Messages sent while a peer is offline are queued and delivered on reconnection, either directly or via sealed store-and-forward. | P0 |
| FR-2.7 | Full-text search across local message history, executed entirely on-device. | P0 |
| FR-2.8 | File and image transfer up to 2 GB, encrypted, sent peer-to-peer, resumable. | P0 |
| FR-2.9 | Typing indicators and read receipts, both individually disableable. | P1 |
| FR-2.10 | Threaded conversations. | P2 |

### 5.3 Voice and video

| ID | Requirement | Priority |
| :--- | :--- | :--- |
| FR-3.1 | 1:1 voice calls, end-to-end encrypted, peer-to-peer, with mouth-to-ear latency under 150 ms on a typical broadband path. | P0 |
| FR-3.2 | Group voice calls up to 8 participants via full mesh. The UI must clearly degrade and explain when a network cannot sustain the mesh. | P0 |
| FR-3.3 | Acoustic echo cancellation, noise suppression, and automatic gain control. | P0 |
| FR-3.4 | Jitter buffering and packet loss concealment; a call must remain intelligible at 5% packet loss. | P0 |
| FR-3.5 | 1:1 video calls at up to 1080p30, bandwidth-adaptive. | P1 |
| FR-3.6 | Screen sharing with selectable window or display. | P1 |
| FR-3.7 | Push-to-talk and per-participant volume control. | P1 |
| FR-3.8 | Persistent voice channels users can drop into, Discord-style. | P1 |

### 5.4 Connectivity

| ID | Requirement | Priority |
| :--- | :--- | :--- |
| FR-4.1 | Direct P2P connection over QUIC where the network permits, with ICE-style NAT traversal (STUN, hole punching). | P0 |
| FR-4.2 | Automatic peer discovery on the local network via mDNS, requiring no internet and no beacon. | P0 |
| FR-4.3 | Graceful fallback to an encrypted relay when direct connection fails, with the connection mode always visible to the user. | P0 |
| FR-4.4 | The beacon is optional and user-configurable; a user may point at their own, at a community one, or at none. | P0 |
| FR-4.5 | Beacons are self-hostable from a single static binary or container, with a documented one-command deployment. | P0 |
| FR-4.6 | Full functionality on a LAN with no internet connection at all. | P1 |

### 5.5 Client experience

| ID | Requirement | Priority |
| :--- | :--- | :--- |
| FR-5.1 | Native GPU-rendered UI on all desktop platforms. No embedded browser engine, no JavaScript runtime. | P0 |
| FR-5.2 | A coherent design system: consistent spacing, typography, and colour, with dark and light themes. | P0 |
| FR-5.3 | Full keyboard navigation, including a command palette for every primary action. | P0 |
| FR-5.4 | Smooth virtualised scrolling of channels containing 100,000+ messages. | P0 |
| FR-5.5 | Native desktop notifications with per-space and per-channel muting. | P0 |
| FR-5.6 | Android client with feature parity for messaging and 1:1 calls. | P1 |
| FR-5.7 | Screen reader support and WCAG AA contrast compliance. | P1 |
| FR-5.8 | Headless daemon mode with a CLI, for servers and scripting. | P2 |

### 5.6 Storage and data handling

| ID | Requirement | Priority |
| :--- | :--- | :--- |
| FR-6.1 | All local data is stored encrypted at rest. | P0 |
| FR-6.2 | Message bodies are compressed with a trained zstd dictionary before encryption, targeting a 4–6× reduction on conversational text. | P0 |
| FR-6.3 | Configurable retention: automatic pruning of message history and cached media by age or by total size. | P0 |
| FR-6.4 | Beacon-stored blobs are ciphertext only, padded to fixed size buckets, and expire automatically after at most 30 days. | P0 |
| FR-6.5 | Complete local data export in an open, documented format. | P1 |

---

## 6. Non-functional requirements

### 6.1 Performance budget

These are **build-gating targets**, enforced in CI. A pull request that regresses a budget fails.

| Metric | Target | Hard ceiling | Measured |
| :--- | :--- | :--- | :--- |
| Idle memory, desktop, 10 spaces loaded (**PSS**) | < 60 MB | 90 MB | **39 MB** (Spike 0.2, 2k messages, GPU) |
| Cold start to interactive window | < 250 ms | 400 ms | — |
| Stripped release binary | < 40 MB | 60 MB | — |
| Message send → peer display, LAN | < 50 ms | 100 ms | — |
| Voice mouth-to-ear, broadband | < 150 ms | 250 ms | not yet measured (needs physical click test) |
| Scroll frame time, 100k messages | < 8 ms (120 fps) | 16 ms (60 fps) | **4.12 ms p99** (Spike 0.2) |
| Idle CPU, connected, no activity | < 0.3% | 1.0% | — |

**Memory is budgeted in PSS, not RSS.** This is not a technicality. Spike 0.2 measured the same
workload at **83 MB RSS but 39 MB PSS**, because RSS charges every process the full weight of
shared library and GPU-driver pages it maps. Roughly 61 MB of the RSS figure is the Mesa/OpenGL
driver stack, shared with every other GPU client on the system. Budgeting in RSS would fail a
build that is actually well inside target. CI measures PSS and records RSS alongside it for
observability.

The earlier "< 38 MB / < 120 ms" figures in the old README were never measured (see
`AUDIT.md` §3.6). The budgets above are deliberately set at achievable levels and will be
tightened once real measurements exist.

### 6.2 Security

- **NFR-S1** — All message content and call media are end-to-end encrypted using **MLS
  (RFC 9420)** via a reviewed implementation. No custom-designed cryptographic protocol.
- **NFR-S2** — Forward secrecy and post-compromise security for all conversations.
- **NFR-S3** — Transport connections are mutually authenticated against pinned raw public keys.
  Certificate verification is never disabled in any build configuration.
- **NFR-S4** — Beacons receive sealed sender envelopes: they can route a blob without learning
  who sent it.
- **NFR-S5** — All cryptographic dependencies are pinned, audited via `cargo-deny`, and
  monitored via `cargo-audit` in CI.
- **NFR-S6** — An independent security review is completed before any release marketed as
  production-ready.

### 6.3 Reliability

- **NFR-R1** — No message is lost. Anything accepted from the user is durably queued locally
  until acknowledged by the recipient.
- **NFR-R2** — Network interruption, sleep, and IP change are recovered from automatically
  without user action.
- **NFR-R3** — The client never loses data on crash; SQLite runs in WAL mode with all writes
  transactional.
- **NFR-R4** — Direct-connection success rate above 85% across common NAT configurations, with
  transparent relay fallback for the remainder.

### 6.4 Compatibility

- **Linux** — glibc 2.31+, x86_64 and aarch64; Wayland and X11. Primary platform.
- **Windows** — 10 1809+, x86_64.
- **macOS** — 12+, Apple Silicon and Intel.
- **Android** — 9.0 (API 28)+, arm64-v8a.

---

## 7. Success criteria

**v1 ships when:**

- All P0 requirements are implemented and tested.
- Every performance budget in §6.1 is met and enforced in CI.
- An independent security review of the cryptographic and transport layers is complete, with
  all critical and high findings resolved.
- Two people on different continents, behind ordinary consumer NAT, can hold a 30-minute voice
  call with no dropout and no manual configuration.
- A new user gets from download to first sent message in under 3 minutes without documentation.

**Adoption indicators, 6 months post-v1:** 1,000 weekly active users; 50 independently operated
beacons; measured idle RAM confirmed below 60 MB across all three desktop platforms.

**Anti-goals — signals we are drifting:** any feature requiring a trusted central service; any
dependency pulling in a browser engine; any growth in idle RAM without a corresponding, agreed
change to the budget.

---

## 8. Key risks

| ID | Risk | Impact | Likelihood | Mitigation |
| :--- | :--- | :--- | :--- | :--- |
| **R1** | **Voice quality is much harder than it looks.** Echo cancellation, jitter buffering, loss concealment, and device handling are specialist work. A call that echoes is worse than no call. | High | **Reduced** | Spike 0.3 built the full pipeline: Opus at 55 µs/frame, AEC+NS at 1.08% of real-time budget, **37 dB ERLE**, zero loss over QUIC on loopback. Approach validated. Residual risk is real-room AEC (reverb, speaker non-linearity, clock drift) and Android cross-compilation of the C++ DSP — both still untested. Keep the 8–10 week Phase 4 budget. |
| **R2** | ~~UI toolkit hits a ceiling.~~ **Partially resolved by Spikes 0.1/0.2.** Performance, memory, IME and accessibility all passed. **Cross-message text selection does not exist in Slint** and has no workaround. | Medium | Confirmed | Slint retained (ADR-004). Message rendering must be block-composed. Cross-message selection is a product decision: ship per-message copy affordances in v1, defer a custom selection layer (~1–2 weeks) to post-v1. UI stays behind the `nexus-app` view-model boundary. |
| **R3** | **NAT traversal underperforms.** Symmetric and carrier-grade NAT defeat hole punching. If the direct-connection rate is low, the "no infrastructure" promise weakens and relay bandwidth costs appear. | High | Medium | Implement full ICE, not just naive punching. Measure success rate against real networks early. Make relay fallback excellent rather than treating it as an edge case. |
| **R4** | **Mobile background execution.** Android aggressively kills background sockets; iOS effectively forbids them. Push notifications normally require a third party (FCM/APNs) that then learns metadata. | High | High | Android: foreground service plus sealed beacon wake-ups. iOS: deliberately deferred from v1 pending research. Never route message content through FCM. |
| **R5** | **Metadata leaks through timing and size.** Even with sealed sender, an observer watching a beacon can correlate traffic patterns. | Medium | High | Fixed-size padding buckets, batched and jittered delivery. Document the residual exposure honestly in the threat model rather than overclaiming. |
| **R6** | **Scope.** "Discord + Slack + Element in one" is three mature products. Attempting all of it at once ships nothing. | High | High | Strict P0/P1/P2 discipline. Ship messaging before calls; ship 1:1 calls before group. Cut scope, never quality. |
| **R7** | **Single maintainer.** The project currently has one developer and a broad surface area. | Medium | High | Ruthless prioritisation, heavy use of proven libraries over custom code, and documentation good enough for contributors to onboard. |

---

## 9. Assumptions

- Users are willing to manage a cryptographic identity, given good UX around backup and
  recovery, in exchange for genuine privacy.
- A meaningful population of users will run beacons voluntarily, as they do for Tor relays and
  Matrix homeservers.
- Native-application performance is a strong enough differentiator to drive adoption on its own,
  independent of the privacy argument.
- The project is open source; no revenue model is assumed for v1, and infrastructure costs are
  near zero by design.

---

## 10. Open questions

1. **Spam and abuse in a system with no central authority.** Invite-only contact addition
   handles the common case, but a public-space model would need a decentralised answer. Deferred
   with P2 features.
2. **Beacon incentives.** What motivates people to run beacons long-term, and what stops a
   malicious operator from running many to observe traffic? Needs design before the beacon
   network is promoted.
3. **Group size ceiling.** Partially measured in Spike 0.5. Application messages are O(1), so
   *messaging* at 200 members is fine. But a membership-change commit is 17 KB at 200 members,
   and with no delivery service the committer broadcasts it to every member — ~3.4 MB of upstream
   for a single join. Large-group membership churn needs a design answer: batched adds, or
   accepting slow joins.
4. **iOS.** Determine whether an acceptable design exists at all under APNs constraints, or
   whether the platform is genuinely incompatible with the privacy goals.
