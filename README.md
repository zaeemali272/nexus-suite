# Nexus Suite

A native, peer-to-peer communication client for desktop and mobile — text channels, direct
messages, voice and video — with no server that can read your data and no browser engine
bundled inside it.

> **Status: pre-alpha, under active redesign.** The current tree is a prototype: transport and
> storage plumbing works, but there is no end-to-end encryption, no voice, and no server
> component yet. See [`docs/AUDIT.md`](docs/AUDIT.md) for an honest assessment of what exists,
> and [`docs/ROADMAP.md`](docs/ROADMAP.md) for what happens next. **Do not use this for anything
> that matters yet.**

---

## Why

Discord, Slack, and Element each ask you to accept something bad. The first two are fast enough
but centralised, and idle at 400–800 MB of RAM because they carry a full Chromium inside.
Element is decentralised and private but slow, heavy, and unpleasant to use daily.

Nexus targets the empty quadrant: **private, fast, and pleasant at the same time.**

- **Native, not a web page in a window.** Rust with a GPU-rendered UI. Target: under 60 MB idle
  RAM and under 250 ms cold start — roughly an order of magnitude better than Electron.
- **No server holds your data.** Your identity is a keypair on your device. Messages and calls go
  directly between peers, encrypted end-to-end with MLS (RFC 9420).
- **Calls run on your machines.** Voice and video are peer-to-peer, hosted by the participants.
  No media servers, so no per-minute infrastructure cost and no operator in the middle.

## What "serverless" means here, precisely

No server holds an account, a contact list, a group membership, or a readable message. That part
is absolute.

But NAT traversal genuinely requires a third party — a host behind NAT cannot discover its own
public address, ~10–15% of network pairs cannot connect directly at all, and delivering to an
offline peer needs somewhere to leave the message. So Nexus ships an optional **beacon**: a tiny
self-hostable binary that does address rendezvous and forwards sealed ciphertext it cannot read.
It holds no accounts, keeps no logs, stores nothing past a short TTL, and is not required for
LAN or previously-connected peers.

The honest claim is **"no trusted server"**, not "no server". The residual metadata exposure is
documented in [`docs/THREAT-MODEL.md`](docs/THREAT-MODEL.md) §5 rather than glossed over.

---

## Documentation

| Document | Contents |
| :--- | :--- |
| [`docs/BRD.md`](docs/BRD.md) | Business requirements — goals, users, functional requirements, performance budgets, risks |
| [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) | Technical architecture — crates, identity, crypto, transport, media, storage, UI |
| [`docs/DECISIONS.md`](docs/DECISIONS.md) | Architecture decision records, with the costs each choice accepts |
| [`docs/THREAT-MODEL.md`](docs/THREAT-MODEL.md) | What is protected, what is not, and what an adversary can still learn |
| [`docs/AUDIT.md`](docs/AUDIT.md) | Reality check on the existing prototype |
| [`docs/ROADMAP.md`](docs/ROADMAP.md) | Phased plan to v1, with exit criteria |

Start with the BRD, then the architecture.

---

## Performance budgets

These are **targets enforced in CI**, not measurements. Nothing here has been benchmarked yet;
figures will be published once the harness exists and produces them.

| Metric | Target | Hard ceiling |
| :--- | :--- | :--- |
| Idle RAM (desktop, 10 spaces) | < 60 MB | 90 MB |
| Cold start to interactive | < 250 ms | 400 ms |
| Stripped release binary | < 40 MB | 60 MB |
| Voice mouth-to-ear latency | < 150 ms | 250 ms |
| Scroll frame time (100k messages) | < 8 ms | 16 ms |

---

## Building

Requires a recent stable Rust toolchain.

```bash
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace --release
```

On NixOS, [`shell.nix`](shell.nix) provides Wayland, OpenGL, Vulkan, fontconfig, and SQLite:

```bash
nix-shell
```

## Platform targets

Linux (primary), Windows, and macOS for desktop; Android for mobile. iOS is deferred from v1 —
its background-execution and push restrictions conflict with the metadata-minimal design and
need dedicated research first.

## Licence

To be decided before first release.
