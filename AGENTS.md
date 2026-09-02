# AGENTS.md — Working agreement for Nexus Suite

Read [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) and [`docs/DECISIONS.md`](docs/DECISIONS.md)
before making structural changes. Read [`docs/AUDIT.md`](docs/AUDIT.md) before trusting anything
the old documentation claimed.

## What this project is

A native, peer-to-peer, end-to-end encrypted communication client in Rust. Desktop
(Linux/Windows/macOS) and Android. No browser engine, no trusted server, no invented
cryptography.

## Non-negotiable rules

1. **Never design cryptography.** Use `openmls` for group encryption, `ring` for primitives,
   `rustls` for transport. A proposal to write a custom cryptographic protocol requires external
   review before it is merged. See ADR-002.
2. **Never disable certificate verification** — not in tests, not behind a feature flag, not
   temporarily. The prototype's `SkipServerVerification` is deleted, not preserved. See ADR-003.
3. **Never claim something works without running it.** The previous documentation marked four
   phases COMPLETE against unimplemented features and published benchmark numbers that were
   never measured. If it is not implemented, say so. If it is not measured, do not state a
   figure.
4. **Never regress a performance budget** (`BRD.md` §6.1) without an explicit, agreed change to
   the budget. CI enforces this.
5. **No allocation in the audio callback.** Ever.
6. **Application logic never lives in the UI crate.** It goes in `nexus-app` or below, so the
   toolkit stays replaceable. See ADR-004.

## Conventions

- `cargo clippy --workspace --all-targets -- -D warnings` must pass. Warnings are errors.
- Public items carry doc comments. Modules carry a `//!` header explaining their role.
- Errors are typed via `thiserror`; no `unwrap()` or `expect()` outside tests and startup
  invariants.
- Key material uses `zeroize` wrappers and never appears in `Debug` output, logs, or panics.
- Tests assert real behaviour. A test that pushes a value into a channel and reads it back is
  testing the channel, not the feature — the prototype had several of these.
- Anything parsing untrusted input gets a fuzz target.

## Current phase

**Phase 0 — de-risking spikes.** See [`docs/ROADMAP.md`](docs/ROADMAP.md). The purpose is to
validate the Slint UI capability, the voice pipeline, NAT traversal rates, MLS integration, and
compression ratios *before* building on them. Throwaway code is expected here; production
discipline resumes in Phase 1.

Do not start Phase 1 work until the Phase 0 gate is met or the relevant ADR is revised.
