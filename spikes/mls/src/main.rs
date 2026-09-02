//! Spike 0.5 — is `openmls` workable for our group model, and does ADR-002's
//! O(log n) group-scaling claim actually hold?
//!
//! Exit criteria (ROADMAP): a 3-member group exchanging messages, with add/remove
//! and state persisted across restart.
//!
//! We additionally measure commit size and time as the group grows, because that
//! scaling property is the entire justification for choosing MLS over a
//! Signal-style pairwise design.

use openmls::prelude::{tls_codec::*, *};
use openmls_basic_credential::SignatureKeyPair;
use openmls_rust_crypto::OpenMlsRustCrypto;
use openmls_traits::{signatures::Signer, types::Ciphersuite, OpenMlsProvider};
use std::time::Instant;

const CIPHERSUITE: Ciphersuite = Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519;

struct Member {
    provider: OpenMlsRustCrypto,
    credential: CredentialWithKey,
    signer: SignatureKeyPair,
}

impl Member {
    fn new(name: &str) -> Self {
        let provider = OpenMlsRustCrypto::default();
        let signer = SignatureKeyPair::new(CIPHERSUITE.signature_algorithm()).unwrap();
        signer.store(provider.storage()).unwrap();
        let credential = CredentialWithKey {
            credential: BasicCredential::new(name.as_bytes().to_vec()).into(),
            signature_key: signer.to_public_vec().into(),
        };
        Self { provider, credential, signer }
    }

    fn key_package(&self) -> KeyPackageBundle {
        KeyPackage::builder()
            .build(CIPHERSUITE, &self.provider, &self.signer, self.credential.clone())
            .unwrap()
    }
}

fn join_config() -> MlsGroupJoinConfig {
    MlsGroupJoinConfig::builder()
        .use_ratchet_tree_extension(true)
        .build()
}

fn create_config() -> MlsGroupCreateConfig {
    MlsGroupCreateConfig::builder()
        .ciphersuite(CIPHERSUITE)
        .use_ratchet_tree_extension(true)
        .build()
}

/// Round-trip a message through serialization, as it would cross the network.
/// This is both more realistic than passing structs directly and gives us wire sizes.
fn wire(msg: &MlsMessageOut) -> Vec<u8> {
    msg.tls_serialize_detached().unwrap()
}

fn to_protocol(msg: &MlsMessageOut) -> ProtocolMessage {
    let bytes = wire(msg);
    MlsMessageIn::tls_deserialize_exact(&bytes)
        .unwrap()
        .try_into_protocol_message()
        .unwrap()
}

fn to_welcome(msg: MlsMessageOut) -> Welcome {
    let bytes = wire(&msg);
    match MlsMessageIn::tls_deserialize_exact(&bytes).unwrap().extract() {
        MlsMessageBodyIn::Welcome(w) => w,
        _ => panic!("expected a welcome message"),
    }
}

fn ok(label: &str) {
    println!("  [OK]   {label}");
}

fn main() {
    println!("=== Spike 0.5 — openmls 0.9 ===\n");

    // ---------------------------------------------------------------
    // Part 1: the ROADMAP exit criteria — 3-member group, messages,
    // add/remove, persistence across restart.
    // ---------------------------------------------------------------
    println!("Part 1: functional exit criteria");

    let alice = Member::new("alice");
    let bob = Member::new("bob");
    let charlie = Member::new("charlie");

    let mut alice_group =
        MlsGroup::new(&alice.provider, &alice.signer, &create_config(), alice.credential.clone())
            .unwrap();
    ok("alice created group");

    // --- add bob ---
    let bob_kp = bob.key_package();
    let (_out, welcome, _gi) = alice_group
        .add_members(&alice.provider, &alice.signer, core::slice::from_ref(bob_kp.key_package()))
        .unwrap();
    alice_group.merge_pending_commit(&alice.provider).unwrap();

    let mut bob_group =
        StagedWelcome::new_from_welcome(&bob.provider, &join_config(), to_welcome(welcome), None)
            .unwrap()
            .into_group(&bob.provider)
            .unwrap();
    assert_eq!(alice_group.members().count(), 2);
    ok("bob joined via Welcome (2 members)");

    // --- alice -> bob application message ---
    let msg = alice_group
        .create_message(&alice.provider, &alice.signer, b"hello from alice")
        .unwrap();
    let processed = bob_group
        .process_message(&bob.provider, to_protocol(&msg))
        .unwrap();
    match processed.into_content() {
        ProcessedMessageContent::ApplicationMessage(m) => {
            assert_eq!(m.into_bytes(), b"hello from alice");
            ok("alice -> bob application message decrypted");
        }
        _ => panic!("expected application message"),
    }

    // --- bob -> alice, proving it works both directions ---
    let msg = bob_group.create_message(&bob.provider, &bob.signer, b"hi alice").unwrap();
    let processed = alice_group
        .process_message(&alice.provider, to_protocol(&msg))
        .unwrap();
    match processed.into_content() {
        ProcessedMessageContent::ApplicationMessage(m) => {
            assert_eq!(m.into_bytes(), b"hi alice");
            ok("bob -> alice application message decrypted");
        }
        _ => panic!("expected application message"),
    }

    // --- add charlie: bob must process the commit to stay in sync ---
    let charlie_kp = charlie.key_package();
    let (commit, welcome, _gi) = alice_group
        .add_members(&alice.provider, &alice.signer, core::slice::from_ref(charlie_kp.key_package()))
        .unwrap();
    alice_group.merge_pending_commit(&alice.provider).unwrap();

    let processed = bob_group
        .process_message(&bob.provider, to_protocol(&commit))
        .unwrap();
    if let ProcessedMessageContent::StagedCommitMessage(sc) = processed.into_content() {
        bob_group.merge_staged_commit(&bob.provider, *sc).unwrap();
    } else {
        panic!("expected staged commit");
    }

    let mut charlie_group =
        StagedWelcome::new_from_welcome(&charlie.provider, &join_config(), to_welcome(welcome), None)
            .unwrap()
            .into_group(&charlie.provider)
            .unwrap();
    assert_eq!(alice_group.members().count(), 3);
    assert_eq!(bob_group.members().count(), 3);
    ok("charlie added; 3-member group in sync");

    // --- all three agree on the group secret ---
    let sa = alice_group.export_secret(alice.provider.crypto(), "nexus", &[], 32).unwrap();
    let sb = bob_group.export_secret(bob.provider.crypto(), "nexus", &[], 32).unwrap();
    let sc = charlie_group.export_secret(charlie.provider.crypto(), "nexus", &[], 32).unwrap();
    assert_eq!(sa, sb);
    assert_eq!(sa, sc);
    ok("all 3 members derive the identical exported secret (media keying works)");

    // --- remove bob, then verify post-compromise security ---
    let bob_index = alice_group
        .members()
        .find(|m| m.credential.serialized_content() == b"bob")
        .unwrap()
        .index;
    let (commit, _w, _gi) = alice_group
        .remove_members(&alice.provider, &alice.signer, &[bob_index])
        .unwrap();
    alice_group.merge_pending_commit(&alice.provider).unwrap();

    let processed = charlie_group
        .process_message(&charlie.provider, to_protocol(&commit))
        .unwrap();
    if let ProcessedMessageContent::StagedCommitMessage(sc) = processed.into_content() {
        charlie_group.merge_staged_commit(&charlie.provider, *sc).unwrap();
    }
    assert_eq!(alice_group.members().count(), 2);
    ok("bob removed (2 members remain)");

    // Bob processes his own removal, then must fail to read subsequent traffic.
    let processed = bob_group
        .process_message(&bob.provider, to_protocol(&commit))
        .unwrap();
    if let ProcessedMessageContent::StagedCommitMessage(sc) = processed.into_content() {
        let _ = bob_group.merge_staged_commit(&bob.provider, *sc);
    }
    let post = alice_group
        .create_message(&alice.provider, &alice.signer, b"bob must not read this")
        .unwrap();
    let bob_attempt =
        bob_group.process_message(&bob.provider, to_protocol(&post));
    assert!(bob_attempt.is_err(), "SECURITY: removed member decrypted post-removal traffic");
    ok("post-compromise security: removed member CANNOT decrypt later messages");

    // Charlie, still a member, can.
    let processed = charlie_group
        .process_message(&charlie.provider, to_protocol(&post))
        .unwrap();
    match processed.into_content() {
        ProcessedMessageContent::ApplicationMessage(m) => {
            assert_eq!(m.into_bytes(), b"bob must not read this");
            ok("remaining member still decrypts correctly");
        }
        _ => panic!("expected application message"),
    }

    // --- persistence across "restart" ---
    // The provider's storage holds group state. Reload the group by id from storage,
    // simulating a process restart.
    let group_id = alice_group.group_id().clone();
    drop(alice_group);
    let mut reloaded = MlsGroup::load(alice.provider.storage(), &group_id)
        .expect("storage read failed")
        .expect("group not found in storage after restart");
    ok("group state reloaded from storage after simulated restart");

    let msg = reloaded
        .create_message(&alice.provider, &alice.signer, b"after restart")
        .unwrap();
    let processed = charlie_group
        .process_message(&charlie.provider, to_protocol(&msg))
        .unwrap();
    match processed.into_content() {
        ProcessedMessageContent::ApplicationMessage(m) => {
            assert_eq!(m.into_bytes(), b"after restart");
            ok("reloaded group sends a message the peer decrypts (epoch survived restart)");
        }
        _ => panic!("expected application message"),
    }

    // ---------------------------------------------------------------
    // Part 2: does the O(log n) claim in ADR-002 hold?
    //
    // Measured twice: with and without the ratchet tree extension. The extension
    // embeds the full tree in commits so joiners can build state without a
    // delivery service -- convenient for us (we have no server), but it may make
    // commits O(n) rather than O(log n).
    // ---------------------------------------------------------------
    println!("\nPart 2: group scaling (the ADR-002 justification)");

    for use_rte in [true, false] {
        println!("\n  ratchet tree extension: {}", if use_rte { "ON (tree embedded in commits)" } else { "OFF" });
        println!("  {:>7} | {:>12} | {:>13} | {:>14} | {:>11}",
                 "members", "commit bytes", "add member ms", "app msg bytes", "encrypt us");
        println!("  {}", "-".repeat(72));

        let cfg = MlsGroupCreateConfig::builder()
            .ciphersuite(CIPHERSUITE)
            .use_ratchet_tree_extension(use_rte)
            .build();

        let owner = Member::new("owner");
        let mut group =
            MlsGroup::new(&owner.provider, &owner.signer, &cfg, owner.credential.clone()).unwrap();

        let mut checkpoints = vec![2usize, 4, 8, 16, 32, 64, 128, 200];
        checkpoints.reverse();
        let mut next = checkpoints.pop();

        for n in 1..=200usize {
            let m = Member::new(&format!("m{n}"));
            let kp = m.key_package();

            let t = Instant::now();
            let (commit, _welcome, _gi) = group
                .add_members(&owner.provider, &owner.signer, core::slice::from_ref(kp.key_package()))
                .unwrap();
            group.merge_pending_commit(&owner.provider).unwrap();
            let add_ms = t.elapsed().as_secs_f64() * 1e3;
            let commit_bytes = wire(&commit).len();

            if Some(group.members().count()) == next {
                let iters = 300;
                let t = Instant::now();
                let mut app_bytes = 0;
                for _ in 0..iters {
                    let msg = group
                        .create_message(&owner.provider, &owner.signer, b"a typical short chat message")
                        .unwrap();
                    app_bytes = wire(&msg).len();
                }
                let enc_us = t.elapsed().as_secs_f64() * 1e6 / iters as f64;

                println!("  {:>7} | {:>12} | {:>13.2} | {:>14} | {:>11.1}",
                         group.members().count(), commit_bytes, add_ms, app_bytes, enc_us);
                next = checkpoints.pop();
            }
        }
    }

    println!("\nApplication message cost is the number that matters for a chat app:");
    println!("it is flat regardless of group size. A Signal-style pairwise design would");
    println!("need one encryption per recipient -- 200x the work at 200 members.");
}
