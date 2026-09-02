# Nexus Suite — Roadmap

**Version:** 1.0 · 2026-09-01
**Supersedes:** `KANBAN.md`, which marked all four phases COMPLETED against code that did not
implement them (see `AUDIT.md`).

Estimates assume one full-time developer. They are ranges because several items — voice DSP and
NAT traversal in particular — have genuinely uncertain effort. **No phase is marked complete
until its exit criteria are demonstrably met**, which is the discipline the previous plan lacked.

---

## Phase 0 — De-risking spikes · 2–3 weeks

Answer the questions that could invalidate the architecture, before building on it. Throwaway
code is fine and expected. This phase exists because discovering R1 or R2 in month six is
catastrophic and discovering it in week two is cheap.

| # | Spike | Question it answers | Status |
| :--- | :--- | :--- | :--- |
| 0.1 | **Slint capability spike** (R2, ADR-004) | Can Slint do rich text editing, cross-widget text selection, IME input, and screen-reader output? | ⚠️ **3 of 4 pass** — [results](spikes/0.1-slint-capability.md). Cross-message selection absent. |
| 0.2 | **Virtualised list spike** (R2) | Can Slint scroll 100k variable-height items at 120 fps? | ✅ **Passed** — [results](spikes/0.2-virtualised-list.md). Memory budget restated in PSS. |
| 0.3 | **Voice pipeline spike** (R1, ADR-005) | Can `webrtc-audio-processing` + `opus` + `cpal` be wired in Rust, with working AEC? | 🟡 **Built and mostly passed** — [results](spikes/0.3-voice.md). Room AEC + latency still need two machines. |
| 0.4 | **NAT traversal spike** (R3) | What is the real direct-connection success rate with `str0m` ICE? | ⬜ Not started — needs ≥10 real network pairs |
| 0.5 | **MLS spike** (ADR-002) | Is `openmls` workable for our group model and persistence needs? | ✅ **Passed** — [results](spikes/0.5-mls.md). ADR-002 reasoning corrected. |
| 0.6 | **Compression spike** (ADR-007) | Does a trained zstd dictionary actually deliver 4–6× on short messages? | ✅ **Passed** — [results](spikes/0.6-compression.md). ADR-007 revised. |

**Phase 0 gate:** each spike passes, or its ADR is revised and the plan is re-costed. Do not
proceed on assumption.

### Results so far

Five of six spikes complete. Every one of them corrected something in the documents that
preceded it — which is precisely what this phase is for.

- **0.6 (compression)** — ADR-007 assumed per-message compression would give 4–6×. Measured
  **1.57×**; zstd's frame header dominates at a ~37-byte average message. Block-compressing 128
  messages reaches **5.32×** at a 2.2 µs read cost. The storage schema in `ARCHITECTURE.md` §8.2
  was rewritten around segments as a result.
- **0.5 (MLS)** — all functional criteria passed, including verified post-compromise security and
  identical exported secrets across members (which media keying depends on). But ADR-002's
  "O(log n) group operations" claim is **false**: commits grow linearly at ~83 bytes/member.
  The decision survives on a better justification — application messages are **O(1)**, a flat
  173 bytes and ~45 µs from 2 members to 200. A new cost surfaced: a 200-member join broadcasts
  ~3.4 MB of upstream in a P2P fan-out.
- **0.2 (virtualised list)** — Slint renders a 100,000-message list at **4.12 ms p99**, flat from
  500 items to 100,000. Comfortable inside the 8 ms target. Investigating an apparent memory
  breach found the budget itself was wrong: the same workload is **83 MB RSS but 39 MB PSS**,
  the difference being Mesa driver pages that RSS charges in full. `BRD.md` §6.1 now budgets PSS.
- **0.1 (Slint capability)** — accessibility and IME are *stronger* than ADR-004 assumed
  (AccessKit across all platforms, ARIA landmarks, live regions; real IME preedit support).
  Rich text is workable but needs block-composed message rendering. **Cross-message text
  selection does not exist in Slint and has no workaround** — a real regression against every
  competitor, now an open product decision.

- **0.3 (voice)** — a complete working pipeline: mic → AEC/NS → Opus → QUIC datagrams → jitter
  buffer → decode → speaker. `webrtc-audio-processing` builds against the system library in 29 s,
  validating ADR-005's central assumption. Opus encodes a 20 ms frame in **55 µs**; AEC+NS costs
  **1.08% of the real-time budget**; echo cancellation converges to **37 dB ERLE** in 2 seconds.
  A live loopback run moved ~900 frames with **zero loss, zero late frames and zero overruns**.
  Still unverified: mouth-to-ear latency (needs physical measurement) and AEC in a real room
  (needs two machines — reverb, speaker non-linearity and clock drift are absent from a synthetic
  echo path).

### Open decision for the user

**Cross-message text selection.** Dragging across several messages to copy an excerpt works in
Discord, Slack and Element; it cannot be done with stock Slint. Options: accept per-message
selection in v1 (free), build a custom selection layer (~1–2 weeks in Phase 3), or ship
per-message copy affordances as mitigation (~2 days). Recommendation is the affordances now and
the custom layer post-v1. This needs a decision before Phase 3 UI work begins.

---

## Phase 1 — Foundations · 4–5 weeks

Establish the workspace and the security core. Everything here is prerequisite to everything
else.

- **1.1** Restructure the workspace into the crate layout in `ARCHITECTURE.md` §3. Migrate
  retained code per `AUDIT.md` §4, reviewing each module as it moves.
- **1.2** `nexus-types` and `nexus-proto`: domain types with ULIDs, versioned wire format,
  postcard codecs, fuzz targets on all parsing.
- **1.3** `nexus-crypto`: Ed25519 identity generation, encrypted keystore with Argon2id and OS
  keychain integration, `zeroize` discipline throughout.
- **1.4** MLS integration: group creation, add/remove, epoch handling, state persistence.
- **1.5** `nexus-store`: new schema, zstd dictionary compression, at-rest encryption, retention
  and pruning.
- **1.6** CI from day one: `clippy -D warnings`, `cargo-deny`, `cargo-audit`, and the
  performance budget harness — budgets are cheap to add now and painful to retrofit.

**Exit:** two local identities can create an MLS group, exchange encrypted messages, persist and
reload state across restart. All CI gates green.

---

## Phase 2 — Transport · 3–4 weeks

- **2.1** `nexus-transport`: QUIC endpoint with the four-stream-class mapping.
- **2.2** Rewrite TLS for raw-public-key mutual authentication (ADR-003). **Delete
  `SkipServerVerification`.**
- **2.3** ICE integration: candidate gathering, connectivity checks, priority pairing.
- **2.4** mDNS local peer discovery.
- **2.5** Connection manager: the ladder in `ARCHITECTURE.md` §6.1, with background upgrade from
  relay to direct.
- **2.6** `nexus-beacon` v1: STUN and rendezvous only.
- **2.7** Reconnection handling — sleep/wake, IP change, network switch.

**Exit:** two machines on different networks, behind real NAT, exchange encrypted messages
directly with no manual configuration. Direct-connection rate measured and recorded.

---

## Phase 3 — Messaging and client · 6–8 weeks

- **3.1** `nexus-engine`: the actor runtime, state ownership, event sourcing.
- **3.2** `nexus-app`: view-models, intents, subscription diffing.
- **3.3** Design system: tokens, typography, spacing, dark and light themes.
- **3.4** Core UI — spaces, channels, message list, composer, contacts.
- **3.5** Message features: markdown, code highlighting, replies, edits, deletes, reactions.
- **3.6** File and image transfer over the bulk stream, resumable.
- **3.7** Local FTS search.
- **3.8** Offline queue and sealed store-and-forward; extend the beacon accordingly.
- **3.9** Onboarding: identity creation, backup flow, invite links, QR codes, safety numbers.
- **3.10** Native notifications.

**Exit:** a usable daily-driver messaging client. Dogfood it. All P0 messaging requirements met,
all performance budgets holding.

---

## Phase 4 — Voice and video · 8–10 weeks

The longest phase, and the one most likely to overrun. Budgeted accordingly.

- **4.1** `nexus-media`: `cpal` capture and playback, device enumeration and hot-swap.
- **4.2** Opus encode/decode with adaptive bitrate.
- **4.3** AEC, NS, AGC integration with correct delay estimation.
- **4.4** Adaptive jitter buffer and packet loss concealment.
- **4.5** Media keying from MLS epoch secrets; rekey on membership change.
- **4.6** 1:1 call signalling, UI, and lifecycle.
- **4.7** Group calls up to 8 via full mesh, with graceful degradation.
- **4.8** Video: capture, encode, bandwidth adaptation, rendering.
- **4.9** Screen sharing.
- **4.10** Call quality telemetry, local only.

**Exit:** a 30-minute 4-person call across three networks with no dropouts, no echo, and
measured latency under 150 ms.

---

## Phase 5 — Platforms and hardening · 6–8 weeks

- **5.1** Windows and macOS builds, packaging, and code signing.
- **5.2** `nexus-mobile`: Android over `nexus-app`, foreground service, sealed push wake-ups.
- **5.3** Multi-device identity linking.
- **5.4** Accessibility: screen reader support, WCAG AA contrast, full keyboard navigation.
- **5.5** Performance pass against every budget on every platform.
- **5.6** Fuzzing campaign across all untrusted input paths.
- **5.7** **Independent security review.** Resolve all critical and high findings.
- **5.8** Reproducible builds and signed releases.
- **5.9** Documentation: user guide, beacon operator guide, protocol specification.

**Exit:** all `BRD.md` §7 success criteria met.

---

## Summary

| Phase | Duration | Cumulative |
| :--- | :--- | :--- |
| 0 — De-risking | 2–3 weeks (5 of 6 spikes done) | 3 weeks |
| 1 — Foundations | 4–5 weeks | 8 weeks |
| 2 — Transport | 3–4 weeks | 12 weeks |
| 3 — Messaging & client | 6–8 weeks | 20 weeks |
| 4 — Voice & video | 8–10 weeks | 30 weeks |
| 5 — Platforms & hardening | 6–8 weeks | 38 weeks |

**Roughly 9 months to v1 for one full-time developer.** That is a real estimate for a product of
this scope, and it is worth stating plainly given the previous plan claimed the same scope was
already finished. The scope-control lever is `BRD.md`'s P0/P1/P2 split: if time pressure
arrives, cut features to P1, never cut the security review or the performance budgets.

**Fastest path to something usable:** Phases 0–3 give a working, secure, native messaging client
at around 20 weeks. That is a defensible first public release on its own, with voice following.
