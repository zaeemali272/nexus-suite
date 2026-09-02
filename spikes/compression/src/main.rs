//! Spike 0.6 — how much compression can we actually get on short chat messages?
//!
//! v2: v1 measured 1.55x against ADR-007's claimed 4-6x. Hypothesis for the gap:
//! at a ~37 byte average message, zstd's frame header (magic + descriptor + checksum)
//! is a large fixed cost. This version isolates that overhead and tests two fixes:
//! magicless frames, and block-compressing groups of messages.

use zstd::zstd_safe::{CParameter, FrameFormat};

include!("corpus.rs");

fn pctile(v: &[f64], p: f64) -> f64 {
    let mut v = v.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[((v.len() as f64 - 1.0) * p) as usize]
}

fn main() {
    let all = corpus();
    let split = all.len() * 6 / 10;
    let (train, test) = all.split_at(split);
    let train_bytes: Vec<Vec<u8>> = train.iter().map(|s| s.as_bytes().to_vec()).collect();
    let raw_total: usize = test.iter().map(|s| s.len()).sum();
    let avg = raw_total as f64 / test.len() as f64;

    println!("corpus: {} msgs ({} train / {} test), avg {:.1} B\n",
             all.len(), train.len(), test.len(), avg);

    let dict = zstd::dict::from_samples(&train_bytes, 16 * 1024).unwrap();

    // ---- A. per-message, standard frame ----
    let mut c = zstd::bulk::Compressor::with_dictionary(3, &dict).unwrap();
    let a_total: usize = test.iter().map(|m| c.compress(m.as_bytes()).unwrap().len()).sum();

    // ---- B. per-message, magicless frame, no checksum, no dictID ----
    let mut c = zstd::bulk::Compressor::with_dictionary(3, &dict).unwrap();
    c.set_parameter(CParameter::Format(FrameFormat::Magicless)).unwrap();
    c.set_parameter(CParameter::ChecksumFlag(false)).unwrap();
    c.set_parameter(CParameter::ContentSizeFlag(false)).unwrap();
    c.set_parameter(CParameter::DictIdFlag(false)).unwrap();
    let b_sizes: Vec<usize> = test.iter().map(|m| c.compress(m.as_bytes()).unwrap().len()).collect();
    let b_total: usize = b_sizes.iter().sum();

    // Verify magicless round-trips, or the whole idea is void.
    let mut d = zstd::bulk::Decompressor::with_dictionary(&dict).unwrap();
    d.set_parameter(zstd::zstd_safe::DParameter::Format(FrameFormat::Magicless)).unwrap();
    let mut c2 = zstd::bulk::Compressor::with_dictionary(3, &dict).unwrap();
    c2.set_parameter(CParameter::Format(FrameFormat::Magicless)).unwrap();
    c2.set_parameter(CParameter::ChecksumFlag(false)).unwrap();
    c2.set_parameter(CParameter::ContentSizeFlag(false)).unwrap();
    c2.set_parameter(CParameter::DictIdFlag(false)).unwrap();
    for m in test.iter().take(500) {
        let enc = c2.compress(m.as_bytes()).unwrap();
        let dec = d.decompress(&enc, 4096).unwrap();
        assert_eq!(dec, m.as_bytes(), "magicless round-trip FAILED");
    }
    println!("magicless round-trip verified on 500 messages: OK");

    // Empty-input compressed size = pure per-message frame overhead.
    let overhead_std = {
        let mut c = zstd::bulk::Compressor::with_dictionary(3, &dict).unwrap();
        c.compress(b"").unwrap().len()
    };
    let overhead_magicless = {
        let mut c = zstd::bulk::Compressor::with_dictionary(3, &dict).unwrap();
        c.set_parameter(CParameter::Format(FrameFormat::Magicless)).unwrap();
        c.set_parameter(CParameter::ChecksumFlag(false)).unwrap();
        c.set_parameter(CParameter::ContentSizeFlag(false)).unwrap();
        c.set_parameter(CParameter::DictIdFlag(false)).unwrap();
        c.compress(b"").unwrap().len()
    };

    println!("\nper-message framing overhead: standard {overhead_std} B, magicless {overhead_magicless} B");
    println!("  (against a {avg:.1} B average message, that is the dominant cost)\n");

    println!("A. per-msg, standard frame   : {:>7} B  ratio {:.2}x", a_total, raw_total as f64 / a_total as f64);
    println!("B. per-msg, magicless frame  : {:>7} B  ratio {:.2}x", b_total, raw_total as f64 / b_total as f64);

    let payload: usize = b_total - overhead_magicless * test.len();
    println!("   \\- of which actual payload: {:>7} B  (ratio {:.2}x if framing were free)",
             payload, raw_total as f64 / payload.max(1) as f64);

    // ---- C. block compression: N messages per compressed unit ----
    println!();
    for block in [8usize, 32, 128, 512] {
        let mut c = zstd::bulk::Compressor::with_dictionary(3, &dict).unwrap();
        c.set_parameter(CParameter::Format(FrameFormat::Magicless)).unwrap();
        c.set_parameter(CParameter::ChecksumFlag(false)).unwrap();
        c.set_parameter(CParameter::ContentSizeFlag(false)).unwrap();
        let mut total = 0usize;
        for chunk in test.chunks(block) {
            // Length-prefixed concatenation so individual messages stay recoverable.
            let mut buf = Vec::new();
            for m in chunk {
                buf.extend_from_slice(&(m.len() as u16).to_le_bytes());
                buf.extend_from_slice(m.as_bytes());
            }
            total += c.compress(&buf).unwrap().len();
        }
        println!("C. block of {:>3} msgs         : {:>7} B  ratio {:.2}x",
                 block, total, raw_total as f64 / total as f64);
    }

    // ---- D. block compression WITHOUT a dictionary, for comparison ----
    println!();
    for block in [32usize, 128] {
        let mut c = zstd::bulk::Compressor::new(3).unwrap();
        let mut total = 0usize;
        for chunk in test.chunks(block) {
            let mut buf = Vec::new();
            for m in chunk {
                buf.extend_from_slice(&(m.len() as u16).to_le_bytes());
                buf.extend_from_slice(m.as_bytes());
            }
            total += c.compress(&buf).unwrap().len();
        }
        println!("D. block of {:>3}, NO dictionary: {:>7} B  ratio {:.2}x",
                 block, total, raw_total as f64 / total as f64);
    }

    // ---- E. read-path cost: decompress a block, extract one message ----
    println!();
    for block in [32usize, 128, 512] {
        let mut c = zstd::bulk::Compressor::with_dictionary(3, &dict).unwrap();
        c.set_parameter(CParameter::Format(FrameFormat::Magicless)).unwrap();
        c.set_parameter(CParameter::ChecksumFlag(false)).unwrap();
        let mut d = zstd::bulk::Decompressor::with_dictionary(&dict).unwrap();
        d.set_parameter(zstd::zstd_safe::DParameter::Format(FrameFormat::Magicless)).unwrap();

        let blocks: Vec<Vec<u8>> = test.chunks(block).map(|chunk| {
            let mut buf = Vec::new();
            for m in chunk {
                buf.extend_from_slice(&(m.len() as u16).to_le_bytes());
                buf.extend_from_slice(m.as_bytes());
            }
            c.compress(&buf).unwrap()
        }).collect();

        let cap = block * 512;
        // Warm, then time repeated single-message reads from random blocks.
        let mut sink = 0usize;
        let iters = 20_000;
        let t = std::time::Instant::now();
        for i in 0..iters {
            let b = &blocks[i % blocks.len()];
            let raw = d.decompress(b, cap).unwrap();
            // extract the 5th message from the block
            let (mut off, mut idx) = (0usize, 0usize);
            while off + 2 <= raw.len() {
                let len = u16::from_le_bytes([raw[off], raw[off + 1]]) as usize;
                off += 2;
                if idx == 5 { sink += raw[off..off + len].len(); break; }
                off += len;
                idx += 1;
            }
        }
        let us = t.elapsed().as_secs_f64() * 1e6 / iters as f64;
        println!("E. read 1 msg from block of {:>3}: {:>6.1} us  (sink {})", block, us, sink % 7);
    }

    let sizes: Vec<f64> = b_sizes.iter().map(|&s| s as f64).collect();
    println!("\nper-message compressed size: p50 {:.0} B, p90 {:.0} B, p99 {:.0} B",
             pctile(&sizes, 0.5), pctile(&sizes, 0.9), pctile(&sizes, 0.99));
}

// Appended: does block compression cost too much on the read path?
// Reading one message means decompressing its whole block. If that is slow,
// the ratio win is not worth it.
#[allow(dead_code)]
fn read_path_cost() {}
