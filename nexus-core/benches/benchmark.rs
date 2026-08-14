//! Criterion benchmark suite for nexus-core AEAD cryptography and postcard serialization.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use nexus_core::{
    decrypt_payload, encrypt_bytes, ChannelId, Message, MessageId, MessageStatus, NetworkPacket,
    PacketType, Payload, PeerId, SymKey,
};
use chrono::Utc;

fn bench_aead_encryption(c: &mut Criterion) {
    let mut group = c.benchmark_group("AEAD_AES_256_GCM");
    let key = SymKey::generate().expect("Failed to generate key");

    for size in [100, 1024, 10240].iter() {
        let plaintext = vec![0x42u8; *size];
        group.throughput(Throughput::Bytes(*size as u64));

        group.bench_with_input(BenchmarkId::new("seal", size), size, |b, _| {
            b.iter(|| {
                encrypt_bytes(&key, black_box(&plaintext)).unwrap();
            });
        });

        let encrypted_payload = encrypt_bytes(&key, &plaintext).unwrap();
        group.bench_with_input(BenchmarkId::new("open", size), size, |b, _| {
            b.iter(|| {
                decrypt_payload(&key, black_box(&encrypted_payload)).unwrap();
            });
        });
    }

    group.finish();
}

fn bench_postcard_serialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("Postcard_Wire_Framing");

    let msg = Message {
        id: MessageId::new(),
        channel_id: ChannelId::new(),
        sender_id: PeerId::new(),
        content: "Benchmarking high-performance zero-copy Rust messaging pipeline".to_string(),
        status: MessageStatus::Sent,
        timestamp: Utc::now(),
        is_encrypted: true,
    };

    let packet = NetworkPacket {
        packet_type: PacketType::TextMessage,
        sender_id: msg.sender_id,
        payload: Payload::Chat(msg),
    };

    group.bench_function("serialize_packet", |b| {
        b.iter(|| {
            packet.serialize().unwrap();
        });
    });

    let bytes = packet.serialize().unwrap();
    group.bench_function("deserialize_packet", |b| {
        b.iter(|| {
            NetworkPacket::deserialize(black_box(&bytes)).unwrap();
        });
    });

    group.finish();
}

criterion_group!(benches, bench_aead_encryption, bench_postcard_serialization);
criterion_main!(benches);
