# Nexus Suite — Threat Model

**Version:** 1.0 · 2026-09-01

This document states what Nexus protects against, what it does not, and what an adversary can
still learn. The limitations section is the important one. A threat model that only lists
strengths is a marketing document.

---

## 1. Assets

| Asset | Sensitivity | Consequence of compromise |
| :--- | :--- | :--- |
| Identity private key | Critical | Full impersonation; ability to read future messages until removed from groups |
| Message content | Critical | Direct exposure of communications |
| Social graph (who talks to whom) | High | Often more revealing than content; enables targeting |
| MLS group state | High | Ability to decrypt group traffic for the current epoch |
| Local message database | High | Full historical exposure if device is compromised |
| Presence and activity timing | Medium | Behavioural profiling, location inference |
| Media streams | High | Real-time conversation exposure |

---

## 2. Adversaries

**A1 — Passive network observer** (ISP, Wi-Fi operator, national-scale traffic monitor).
Sees all traffic in transit. Cannot modify it.

**A2 — Active network attacker.** Can drop, delay, replay, and inject packets, and attempt
machine-in-the-middle on connection establishment.

**A3 — Malicious beacon operator.** Runs a beacon that peers use. Sees everything that transits
it, can log indefinitely, and can lie about rendezvous data or selectively drop traffic.

**A4 — Compromised peer.** A legitimate group member whose device is under adversary control.
The hardest adversary, because they hold valid keys.

**A5 — Device thief.** Has physical possession of a locked or unlocked device.

**A6 — Malicious contact.** Someone the user legitimately added, behaving abusively within the
protocol.

**A7 — Supply chain attacker.** Compromises a dependency, the build pipeline, or the release
artefacts.

---

## 3. What is protected

| Threat | Defence | Adversary defeated |
| :--- | :--- | :--- |
| Reading message content in transit | MLS end-to-end encryption; the transport never sees plaintext | A1, A2, A3 |
| Reading content at rest on a beacon | Beacons store only sealed ciphertext; no keys exist server-side | A3 |
| MITM on connection setup | Raw-public-key pinning with mutual auth; out-of-band safety numbers | A2, A3 |
| Retroactive decryption after key theft | MLS forward secrecy — past epochs are unrecoverable | A1, A4, A5 |
| Continued access after device compromise | MLS post-compromise security — removal heals the group at the next epoch | A4, A5 |
| Message tampering or forgery | AEAD authentication; every message signed under group state | A2, A3 |
| Replay | MLS epoch and sequence enforcement | A2, A3 |
| Sender identification at the beacon | Sealed sender — the outer envelope carries only a rotating recipient tag | A3 |
| Length-based correlation | Fixed-size padding buckets (1K/4K/16K/64K/256K) | A1, A3 |
| Reading data on a stolen locked device | Argon2id-derived key, AES-256-GCM at-rest encryption, OS keychain integration | A5 |
| Mass surveillance via central collection | There is no central store to collect from | A1, A3 |
| Server-side account compromise | There are no server-side accounts | A3 |

---

## 4. What is explicitly not protected

Stated plainly. Each of these is a real limitation.

**Endpoint compromise.** If an adversary controls the device — malware, a root exploit, an
unlocked stolen phone — they see everything the user sees. No end-to-end encryption survives a
compromised endpoint. This is universal to all E2EE systems, and no amount of protocol design
changes it.

**A malicious group member.** Any legitimate member can screenshot, copy, forward, or leak
everything they receive. Encryption controls who *can* read, never what they do afterwards.

**Traffic analysis by a global passive adversary.** An observer who can watch both ends of a
connection simultaneously can correlate by timing regardless of encryption. Defeating this
requires Tor-style onion routing with cover traffic, which is incompatible with sub-150 ms voice
latency. Nexus does not attempt it.

**Endpoint IP exposure in direct P2P.** In a direct connection, peers learn each other's IP
addresses — that is what "direct" means. For most users this is fine, and it is strictly better
than a central server learning every user's IP. For a user who must hide their IP from a
contact, forcing relay mode is the mitigation, and the UI must make this reachable.

**Compelled disclosure by a user.** Legal or physical coercion of a participant defeats the
system. No deniable-encryption or duress mechanism is provided.

**Metadata inherent to running an application.** An ISP can see that a device is using Nexus
(distinctive QUIC ALPN and traffic patterns), roughly when, and roughly how much. Protocol
obfuscation is not implemented.

**Denial of service.** A beacon can be flooded. A peer can be flooded. Rate limiting is
implemented, but a determined DoS against a specific user's connectivity will succeed.

**Availability without infrastructure.** If all configured beacons are down and a peer is not on
the LAN or at a cached endpoint, messages queue locally and do not deliver. This is a deliberate
trade of availability for trust.

---

## 5. Residual metadata exposure at a beacon

Sealed sender and padding remove the obvious identifiers, but a malicious operator (A3) running
long-term correlation is not fully defeated. Being specific:

| The beacon can observe | Mitigation | Residual risk |
| :--- | :--- | :--- |
| Source IP of connecting clients | None at the protocol layer; users may front with a VPN or Tor | **Real.** A beacon learns which IPs use it and when. |
| Timing of blob deposit and retrieval | Batching and randomised jitter on delivery | **Partial.** Statistical correlation over long observation remains feasible. |
| Volume of traffic per connection | Fixed-size padding buckets | **Low.** Bucketing coarsens but does not eliminate volume signal. |
| Rotating recipient tags | Tags rotate on a schedule from a shared secret | **Low.** Rotation must be frequent enough to prevent linking; parameters need review. |
| Relay session endpoints | Relayed sessions inherently reveal that two connections are paired | **Real.** Relay is the weakest mode and must remain the fallback, never the default. |

**Honest summary:** a beacon operator who logs everything and analyses it over months can learn a
meaningful amount about *patterns* of communication among users of that beacon. They learn
nothing about content. The defences are self-hosting (run your own and the operator is you),
beacon diversity (using several fragments the picture), and the fact that most traffic never
touches a beacon at all.

Overstating this would be the most damaging thing this project could do to its credibility.

---

## 6. Security requirements on implementation

- Cryptographic dependencies are pinned to exact versions and gated by `cargo-deny`;
  `cargo-audit` runs in CI on every commit.
- No cryptographic protocol is designed in-house (ADR-002). Any proposal to do so requires
  external review before merge.
- Certificate verification is never disabled in any build configuration, including tests. The
  existing `SkipServerVerification` (`AUDIT.md` §3.2) is deleted, not feature-gated.
- All key material lives in `zeroize`-wrapped types, `mlock`ed where the platform permits, and
  never appears in logs, error messages, or panic output.
- Fuzz testing covers all parsing of untrusted input: packet framing, MLS message handling, and
  media decode paths.
- An independent security review of the crypto and transport layers is a release gate for v1
  (`BRD.md` §7).
- Reproducible builds and signed release artefacts, to limit A7.

---

## 7. Open security questions

1. **Recipient tag rotation parameters.** How frequently must tags rotate to defeat long-term
   linking, and what is the cost in delivery reliability? Needs analysis before v1.
2. **Sybil beacons.** An adversary running many beacons cheaply increases their share of
   observed traffic. What client-side beacon selection strategy resists this?
3. **Abuse handling without a central authority.** Invite-only contact addition covers most
   cases, but blocking, reporting, and spam resistance in a decentralised system need design.
4. **Android push without metadata leakage.** FCM wake-ups reveal to Google that a device
   received *something*. Quantify the leak and decide whether it is acceptable.
5. **MLS epoch recovery on unreliable networks.** What happens when a client misses commits for
   weeks? The failure modes need enumeration and testing.
