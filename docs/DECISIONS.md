# Nexus Suite — Architecture Decision Records

Each record states the decision, the reasoning, the alternatives rejected, and the cost accepted.
The cost section matters most: a decision recorded without its downside is marketing, not
engineering.

---

## ADR-001 — Custom protocol rather than Matrix federation

**Status:** Accepted · 2026-09-01

**Context.** Building on Matrix would give immediate interoperability with Element, Beeper, and
thousands of existing homeservers, plus a mature, audited specification.

**Decision.** Build a custom protocol. Do not federate with Matrix in v1.

**Reasoning.** Matrix's data model fights every one of this project's goals. Its state
resolution algorithm requires each client to maintain and reconcile a room's full directed
acyclic graph of state events, which is why Element is slow and memory-hungry — the cost is
inherent to the protocol, not to Element's implementation. Matrix is JSON over HTTP long-polling,
where we want binary over QUIC. And Matrix federation assumes homeservers: users have accounts on
servers, and servers hold room state. That is structurally incompatible with an architecture
where identity is a keypair and no server holds anything.

Adopting Matrix would mean adopting its performance ceiling and its server-centric trust model,
which are the two things this project exists to escape.

**Alternatives rejected.** Matrix-native (loses the performance and trust goals). Matrix bridge
as a v1 feature (a bridge is a plaintext chokepoint — it must decrypt to translate, which
reintroduces exactly the trusted third party we removed).

**Cost accepted.** Nexus users can only talk to Nexus users. This is a serious adoption barrier
and the single largest strategic risk in the project. A bridge may be reconsidered post-v1 as an
explicitly user-run, explicitly trust-degrading component.

---

## ADR-002 — MLS for group encryption, not a hand-rolled ratchet

**Status:** Accepted · 2026-09-01 · **Reasoning corrected 2026-09-01 after Spike 0.5**

**Context.** The current code generates a random symmetric key locally and never exchanges it
(`AUDIT.md` §3.1) — there is no key management at all. Something must replace it. The obvious
candidates are a Signal-style Double Ratchet with pairwise sessions, or MLS (RFC 9420).

**Decision.** Use MLS via the `openmls` crate for all conversations, including 1:1 DMs
(a DM is simply a two-member group).

**Reasoning.** Three factors, in order of weight:

1. **Application messages cost O(1) regardless of group size.** Spike 0.5 measured a constant
   **173 bytes and ~45 µs from 2 members to 200** — perfectly flat. One encryption produces one
   ciphertext every member can decrypt. A Signal-style pairwise design needs one encryption and
   one ciphertext *per recipient*: at 200 members that is 200× the CPU and 200× the upstream
   bandwidth on every message. Since a client sends thousands of messages per membership change,
   this operation dominates, and it alone decides the question.
2. **It is a reviewed standard.** RFC 9420 went through years of IETF and academic cryptanalysis.
   `openmls` is an independently audited implementation.
3. **Post-compromise security.** A compromised device is healed out of the group by the next
   epoch change — essential for the multi-device model where devices are lost and replaced.
   Spike 0.5 verified this directly: a removed member fails to decrypt subsequent messages.

**Correction from Spike 0.5.** This ADR originally justified the choice by claiming MLS gives
"O(log n) group operations". **That is wrong.** Measured commit size grows *linearly* at ~83
bytes per member (17 KB at 200 members), because a commit encrypts the path secret to the
resolution of each sibling subtree along the path, and those sizes sum to n−1. The tree depth is
logarithmic; the broadcast commit is not. Disabling the ratchet tree extension changed nothing.

The decision is unchanged and the case for it is actually stronger — but it rests on factor 1
above, not on the O(log n) framing. Full data in `docs/spikes/0.5-mls.md`.

**Alternatives rejected.** Double Ratchet with pairwise sessions (O(n) fan-out fails the group
requirement). A custom protocol (never — the rule is that this project does not invent
cryptography; hand-rolled key management is the single most common source of catastrophic,
silent E2EE failure).

**Cost accepted.** MLS is complex and `openmls` has a demanding API. Group state must be
persisted and recovered correctly, and epoch handling across unreliable networks is genuinely
difficult. This is more upfront work than a naive design — deliberately so.

Additionally, and newly quantified: **membership changes are expensive in a P2P system.** With
no delivery service to fan out a commit, the committer broadcasts 17 KB to each of 200 members —
~3.4 MB of upstream for one join, several seconds on a domestic link. Messaging is unaffected
(it is O(1)), but large-group membership churn needs design attention. Tracked as open question
#3 in `BRD.md` §10.

---

## ADR-003 — Raw public key pinning, no certificate authorities

**Status:** Accepted · 2026-09-01

**Context.** The current transport disables certificate verification entirely
(`AUDIT.md` §3.2) — connections are encrypted but unauthenticated and trivially
machine-in-the-middle-able. Fixing this by adopting the Web PKI would mean CAs, domain names,
and certificate lifecycle management for peers that have none of those things.

**Decision.** TLS 1.3 with raw public keys (RFC 7250) and mutual authentication. Each side
presents its Ed25519 identity key; each verifies the peer against the key pinned in the contact
record. Any unrecognised key refuses the connection.

**Reasoning.** Identity is already established out-of-band at contact-add time via invite link
or QR code. Once you have the peer's public key, a CA adds nothing — it is a mechanism for
binding names to keys when you have no other channel, and we have another channel. Peers have no
domain names, so Web PKI does not apply to them in the first place.

**Alternatives rejected.** Web PKI (inapplicable to peers, adds a CA as a trusted third party).
Trust-on-first-use without pinning (accepts one MITM window per contact, and TOFU without a
verification story is weak).

**Cost accepted.** Users must handle key-change events, which are genuinely confusing UX —
"Bob's safety number changed" is either a reinstalled phone or an attack, and the user has to
distinguish them. This must be designed carefully rather than dumped on the user as a modal.

---

## ADR-004 — Slint for v1 UI, behind a replaceable boundary

**Status:** Accepted · 2026-09-01 · **Gate cleared 2026-09-01 by Spikes 0.1 and 0.2**

**Context.** The UI toolkit must deliver GPU-rendered native performance under ~15 MB of RAM,
run on Linux/Windows/macOS/Android, and handle rich text, text selection across message
boundaries, IME for non-Latin input, and screen-reader accessibility. Existing work includes
1,005 lines of Slint.

**Decision.** Use Slint for v1, but place all application logic in a toolkit-agnostic
`nexus-app` crate so the UI layer is replaceable.

**Gate result (Spikes 0.1, 0.2): PASSED, conditionally.** Performance passed decisively.
Accessibility and IME are stronger than this ADR assumed. Rich text is better than assumed but
has specific gaps. **Cross-message text selection does not exist in Slint and has no workaround**,
which is a genuine regression against every competitor and now a tracked product decision.

**Reasoning.** Slint is the best fit on the hard constraints: genuinely native GPU rendering,
low memory, a declarative language that suits a design system, and it already covers all four
target platforms. It preserves existing work.

The concern was that Slint's strengths are in embedded and dashboard UIs, while a chat client
needs exactly what it might be weakest at. Measured in week one rather than discovered in month
six:

| Capability | Result |
| :--- | :--- |
| Scrolling 100k variable-height messages | ✅ p99 **4.12 ms** render, flat from 500 to 100k items — 4× headroom under the 8 ms target |
| Memory, realistic 2k-message working set | ✅ **PSS 39 MB**, USS 33 MB — inside the 60 MB target |
| Rich text display | ⚠️ `StyledText` renders CommonMark (italics, inline code, links, lists, colours). **No code blocks, no inline images** — messages must be block-composed |
| Rich text editing | ⚠️ Plain-text composer only. Acceptable — Discord, Slack and Element all use markdown-source composers |
| IME | ✅ Real preedit support in core; parley text layout. *API-verified, not yet typed with a CJK IME* |
| Accessibility | ✅ AccessKit across AT-SPI/macOS/Windows/iOS, full ARIA landmark roles, live regions. *API-verified, not yet driven with a screen reader* |
| **Cross-message text selection** | ❌ **Absent.** `Text`/`StyledText` have no selection at all; `TextInput` selects only within itself |

Details in `docs/spikes/0.1-slint-capability.md` and `docs/spikes/0.2-virtualised-list.md`.

**Alternatives rejected.** Tauri (system webview — still HTML/DOM rendering, 80–150 MB RAM;
better than Electron but the same category the project rejects). Flutter (excellent text and
Android support, but 60–100 MB RAM, adds Dart alongside Rust, and weak Linux desktop support).
egui (immediate mode; poor text editing, IME, and accessibility). GPUI (fastest and best-looking,
but no stable public API, no Windows or Android). Iced (promising but immature for an
application this complex).

**Cost accepted.** Two commitments follow from the gate result, neither in the original plan:

1. **Message rendering is block-composed**, not one markdown string per message: paragraphs via
   `StyledText`, code blocks as a custom element (syntax highlighting is achievable by emitting
   `<font color="...">` spans, which `StyledText` does support), images as `Image` elements.
2. **Cross-message selection is either custom-built (~1–2 weeks in Phase 3) or absent in v1.**
   The recommendation is to ship per-message copy affordances for v1 and defer the custom
   selection layer, but this is a product decision, not a technical one. Users dragging across
   three messages to quote a conversation will notice.

A third, quieter cost: memory must be measured in **PSS, not RSS**. Slint on a GPU backend shows
83 MB RSS but 39 MB PSS at the same workload, because Mesa driver pages are counted at full
weight in RSS. Pinning the wrong metric in CI would have made a passing build look like a
failing one.

The `nexus-app` boundary remains the insurance policy: if cross-message selection later proves
unacceptable, replacing the UI layer is a bounded change rather than a rewrite.

---

## ADR-005 — Use `webrtc-audio-processing` rather than writing DSP

**Status:** Accepted · 2026-09-01 · **Validated 2026-09-01 by Spike 0.3**

**Context.** Voice calls need acoustic echo cancellation, noise suppression, and automatic gain
control. Without competent AEC, every speaker-using participant hears themselves echo, which
makes a call unusable regardless of how good the network path is.

**Decision.** Bind `webrtc-audio-processing` (the DSP module extracted from libwebrtc) for
AEC, NS, and AGC. Write only the Rust integration and the jitter buffer.

**Reasoning.** libwebrtc's audio processing represents well over a decade of tuning against real
acoustic environments and has run in billions of calls. Adaptive echo cancellation with accurate
delay estimation is a specialist DSP discipline; a from-scratch implementation would take months
and still be worse. This is not the project's differentiator, and effort spent here is effort not
spent on the things that are.

**Alternatives rejected.** Adopting all of `webrtc-rs` (pulls in a full WebRTC stack — SDP,
DTLS-SRTP, ICE, signalling — most of which duplicates our QUIC transport and MLS keying; we want
the DSP, not the protocol stack). Writing our own DSP (see above). Shipping without AEC
(guarantees unusable calls).

**Cost accepted.** A C++ dependency, which complicates cross-compilation — particularly for
Android — and adds build-time and binary-size cost. Judged clearly worth it.

**Validated by Spike 0.3.** The crate builds against the nixpkgs system library in 29 seconds on
Linux. AEC + NS + high-pass costs **81 µs p50 / 108 µs p99** per 10 ms frame — 1.08% of the
real-time budget — and echo cancellation converges to **37 dB ERLE within 2 seconds**. Opus
encodes a 20 ms frame in 55 µs. The cross-compilation cost for Android remains untested and is
still the open risk on this ADR.

One caution recorded from that spike: the library's `get_stats()` echo fields appear unpopulated
in this build and reported 0.18 dB against a measured 37 dB. Do not drive tuning from them.

---

## ADR-006 — Optional untrusted beacons, not a serverless absolute

**Status:** Accepted · 2026-09-01

**Context.** The stated goal is fully serverless P2P. Three functions make that physically
impossible: a NATed host cannot discover its own public address without an external observer;
symmetric and carrier-grade NAT prevent direct connection for 10–15% of peer pairs; and
delivering to an offline peer requires somewhere to leave the message.

**Decision.** Ship an optional, self-hostable `nexus-beacon` providing STUN, rendezvous, relay,
and sealed store-and-forward. It is never trusted, never required for LAN or cached-endpoint
connections, and structurally incapable of reading content or identifying senders.

**Reasoning.** The meaningful definition of serverless is *no trusted third party*, not *no
packets ever traverse a third machine*. The design therefore removes the beacon's **capability**
to misbehave rather than promising good behaviour: no accounts, no durable storage, no logs,
sealed-sender envelopes, fixed-size padding, short TTLs, and trivial self-hosting so no operator
is structurally important. A user may run their own, use a community one, or use none and accept
LAN-plus-direct-only operation.

**Alternatives rejected.** A pure DHT with no beacons (DHTs leak more metadata than a sealed
beacon — every lookup broadcasts interest to unknown nodes — and cannot solve relay or offline
delivery at all). A traditional server (the thing the project exists to avoid).

**Cost accepted.** The "fully serverless" claim cannot be made without qualification. The
correct claim is "no trusted server", and marketing must not overstate it. Residual metadata
exposure at the beacon is documented in `THREAT-MODEL.md` §5 rather than hidden.

---

## ADR-007 — Compress message blocks with a trained zstd dictionary, then encrypt

**Status:** Accepted · 2026-09-01 · **Revised 2026-09-01 after Spike 0.6**

**Context.** The BRD requires highly compressed storage. Chat messages are short — typically
under 200 bytes — and generic compressors achieve essentially nothing at that size because
there is no internal redundancy to find.

**Decision.** Compress message bodies with zstd using a dictionary trained on representative
conversational text, then encrypt. Order is fixed: compress first, always.

**Revised after measurement:** compress **blocks of ~128 messages** as a unit, not individual
messages. Messages newer than the current block boundary are stored individually using
**magicless** framing until the block seals.

**Reasoning.** A trained dictionary hands the compressor the shared vocabulary up front, which
is exactly what short inputs lack. Encrypting first is not an option: ciphertext is
indistinguishable from random and does not compress at all.

The original version of this ADR assumed per-message compression would reach 4–6×. **Spike 0.6
measured it at 1.57×** (`docs/spikes/0.6-compression.md`). The dictionary was not at fault —
zstd's 13-byte frame header against a 36.7-byte average message is 35% overhead before any data
is encoded. Measured results:

| Configuration | Ratio |
| :--- | ---: |
| Per-message, standard frame | 1.57× |
| Per-message, magicless frame | 2.38× |
| Block of 128 messages | **5.32×** |
| Block of 128, no dictionary | 3.41× |

Amortising the frame across a block reaches the target, and reading one message out of a
128-block costs **2.2 µs** — negligible, and one decompression typically serves an entire
screen. The dictionary remains worth its complexity: 5.32× with it versus 3.41× without.

The CRIME/BREACH attack class against compress-then-encrypt requires an adaptive attacker who
can inject chosen plaintext into a channel and observe the compressed length. At-rest storage of
one's own message history is not that setting. This reasoning is recorded explicitly so that
compression is never later applied to a live network channel without re-analysis.

**Alternatives rejected.** No compression (fails the requirement). Generic zstd without a
dictionary (3.41× versus 5.32× — measurably worse). Per-message compression (1.57×, fails the
requirement — this was the original decision, corrected by measurement). Compressing the network
channel (deferred — see the CRIME caveat above).

**Cost accepted.** The dictionary is a versioned asset that must ship with the client and be
retained forever for old messages to remain readable. Dictionary versioning must be handled from
the first release, not retrofitted.

Block segmentation adds real complexity: two storage paths (sealed blocks and loose recent
messages), a sealing process, and edits or deletions inside a sealed block requiring either
rewrite or tombstoning. Judged worth it for 5.32× versus 1.57×, but it is a genuine cost that
per-message storage would not have.

---

## ADR-008 — Keep and refactor the existing code rather than restart

**Status:** Accepted · 2026-09-01

**Context.** The audit found ~4,100 lines with genuinely working transport, storage, and actor
layers wrapped in documentation claiming a finished product.

**Decision.** Keep `nexus-net`'s QUIC endpoint, `nexus-db`'s pool and repository patterns,
`nexus-core`'s types and framing, and the daemon's actor structure. Rewrite the security
architecture — key management, transport authentication, local auth — from scratch rather than
patching. Rebuild voice from nothing, since nothing exists. Replace all documentation.

**Reasoning.** The transport and storage layers are competent and would cost weeks to rebuild
for no benefit. The security layer is different in kind: retrofitting key management onto a
design that assumed a shared symmetric key reliably produces subtle and permanent flaws, and in
cryptography "mostly right" is indistinguishable from broken. The two categories get opposite
treatment on purpose.

**Cost accepted.** Carrying forward existing code means carrying forward assumptions embedded in
it. Each retained module gets an explicit review pass as it is migrated, rather than being
adopted unexamined.
