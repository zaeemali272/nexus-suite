//! Codec and DSP cost measurement. No audio hardware required.

use std::time::Instant;

pub const SAMPLE_RATE: u32 = 48_000;
pub const FRAME_MS: usize = 20;
pub const FRAME_SAMPLES: usize = (SAMPLE_RATE as usize / 1000) * FRAME_MS; // 960 @ 48k/20ms

/// Synthetic speech-like signal: a few harmonics with an amplitude envelope,
/// which is far more representative of Opus's real behaviour than a pure tone.
fn speechlike(n: usize, t0: usize) -> Vec<i16> {
    (0..n)
        .map(|i| {
            let t = (t0 + i) as f32 / SAMPLE_RATE as f32;
            let env = 0.5 + 0.5 * (2.0 * std::f32::consts::PI * 3.0 * t).sin();
            let s = (2.0 * std::f32::consts::PI * 120.0 * t).sin() * 0.5
                + (2.0 * std::f32::consts::PI * 240.0 * t).sin() * 0.25
                + (2.0 * std::f32::consts::PI * 480.0 * t).sin() * 0.12
                + (2.0 * std::f32::consts::PI * 1800.0 * t).sin() * 0.05;
            (s * env * 12000.0) as i16
        })
        .collect()
}

fn pct(v: &mut Vec<f64>, p: f64) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[(((v.len() - 1) as f64) * p) as usize]
}

pub fn run() {
    println!("=== Spike 0.3 — voice pipeline bench ===");
    println!("48 kHz mono, {FRAME_MS} ms frames ({FRAME_SAMPLES} samples)\n");

    opus_bench();
    aec_bench();
}

fn opus_bench() {
    use opus::{Application, Channels, Decoder, Encoder};

    println!("--- Opus ---");
    println!("{:>8} | {:>11} | {:>12} | {:>11} | {:>11}",
             "bitrate", "frame bytes", "actual kbps", "encode us", "decode us");
    println!("{}", "-".repeat(66));

    for kbps in [16i32, 24, 32, 48, 64] {
        let mut enc = Encoder::new(SAMPLE_RATE, Channels::Mono, Application::Voip).unwrap();
        enc.set_bitrate(opus::Bitrate::Bits(kbps * 1000)).unwrap();
        let mut dec = Decoder::new(SAMPLE_RATE, Channels::Mono).unwrap();

        let mut enc_us = Vec::new();
        let mut dec_us = Vec::new();
        let mut total_bytes = 0usize;
        let frames = 500;
        let mut out = vec![0u8; 4000];
        let mut pcm_out = vec![0i16; FRAME_SAMPLES];

        for f in 0..frames {
            let pcm = speechlike(FRAME_SAMPLES, f * FRAME_SAMPLES);

            let t = Instant::now();
            let n = enc.encode(&pcm, &mut out).unwrap();
            enc_us.push(t.elapsed().as_secs_f64() * 1e6);
            total_bytes += n;

            let t = Instant::now();
            let _ = dec.decode(&out[..n], &mut pcm_out, false).unwrap();
            dec_us.push(t.elapsed().as_secs_f64() * 1e6);
        }

        let avg_bytes = total_bytes as f64 / frames as f64;
        let actual_kbps = avg_bytes * 8.0 * (1000.0 / FRAME_MS as f64) / 1000.0;
        println!("{:>6}k  | {:>11.0} | {:>12.1} | {:>11.1} | {:>11.1}",
                 kbps, avg_bytes, actual_kbps,
                 pct(&mut enc_us.clone(), 0.50), pct(&mut dec_us.clone(), 0.50));
    }

    // Packet loss concealment: decode with a lost frame and confirm it produces
    // audio rather than silence or an error.
    let mut enc = Encoder::new(SAMPLE_RATE, Channels::Mono, Application::Voip).unwrap();
    let mut dec = Decoder::new(SAMPLE_RATE, Channels::Mono).unwrap();
    let mut out = vec![0u8; 4000];
    let mut pcm_out = vec![0i16; FRAME_SAMPLES];
    for f in 0..10 {
        let pcm = speechlike(FRAME_SAMPLES, f * FRAME_SAMPLES);
        let n = enc.encode(&pcm, &mut out).unwrap();
        dec.decode(&out[..n], &mut pcm_out, false).unwrap();
    }
    let t = Instant::now();
    dec.decode(&[], &mut pcm_out, false).unwrap(); // simulate a dropped packet
    let plc_us = t.elapsed().as_secs_f64() * 1e6;
    let energy: i64 = pcm_out.iter().map(|s| (*s as i64).abs()).sum();
    println!("\nPLC (concealing 1 lost frame): {:.1} us, output energy {} ({})",
             plc_us, energy, if energy > 0 { "audio generated, not silence" } else { "SILENT - PLC not working" });
}

fn aec_bench() {
    use webrtc_audio_processing::Processor;
    use webrtc_audio_processing_config::{Config, EchoCanceller, NoiseSuppression, HighPassFilter};

    println!("\n--- WebRTC audio processing (AEC / NS / HPF) ---");

    let ap = match Processor::new(SAMPLE_RATE) {
        Ok(p) => p,
        Err(e) => {
            println!("FAILED to initialise processor: {e:?}");
            return;
        }
    };
    ap.set_config(Config {
        echo_canceller: Some(EchoCanceller::default()),
        noise_suppression: Some(NoiseSuppression::default()),
        high_pass_filter: Some(HighPassFilter::default()),
        ..Default::default()
    });

    let n = ap.num_samples_per_frame(); // 10 ms, fixed by WebRTC
    println!("processor up: {} samples/frame ({} ms @ {} Hz), AEC+NS+HPF enabled",
             n, n * 1000 / SAMPLE_RATE as usize, SAMPLE_RATE);

    // ERLE (Echo Return Loss Enhancement) measures how much echo energy the
    // canceller removes. It is measured under FAR-END SINGLE TALK -- the near-end
    // mic is silent apart from the echo -- because during double talk you cannot
    // separate "residual echo" from "near-end speech the NS legitimately altered".
    //
    // The echo path here is synthetic: a fixed delay and gain, no room reverb, no
    // loudspeaker non-linearity, no clock drift. This validates that AEC3 runs and
    // CONVERGES. It does not validate performance in a room -- that still needs
    // two machines with open speakers.
    const ECHO_GAIN: f32 = 0.6;
    const ECHO_DELAY_SAMPLES: usize = 480; // 10 ms acoustic path
    let mut us = Vec::new();
    let mut erle_by_block: Vec<f64> = Vec::new();
    let (mut echo_in_acc, mut resid_acc) = (0f64, 0f64);

    // Far-end reference: broadband, decorrelated from anything near-end.
    let mut lfsr: u32 = 0xACE1;
    let mut noise = move || {
        lfsr ^= lfsr << 13;
        lfsr ^= lfsr >> 17;
        lfsr ^= lfsr << 5;
        (lfsr as i32 as f32 / i32::MAX as f32) * 0.35
    };

    let frames = 2000; // 20 seconds
    let mut echo_tail = vec![0f32; ECHO_DELAY_SAMPLES];

    for f in 0..frames {
        let far: Vec<f32> = (0..n).map(|_| noise()).collect();

        // Delayed, attenuated copy of the far end = the echo the mic picks up.
        let mut echo = vec![0f32; n];
        for i in 0..n {
            echo[i] = if i < ECHO_DELAY_SAMPLES {
                echo_tail[i] * ECHO_GAIN
            } else {
                far[i - ECHO_DELAY_SAMPLES] * ECHO_GAIN
            };
        }
        for i in 0..ECHO_DELAY_SAMPLES {
            echo_tail[i] = far[n - ECHO_DELAY_SAMPLES + i];
        }

        let mut render = vec![far.clone()];
        // Far-end single talk: capture is echo only, near-end silent.
        let mut capture = vec![echo.clone()];

        let t = Instant::now();
        ap.process_render_frame(&mut render).unwrap();
        ap.process_capture_frame(&mut capture).unwrap();
        us.push(t.elapsed().as_secs_f64() * 1e6);

        let echo_in: f64 = echo.iter().map(|x| (*x as f64) * (*x as f64)).sum();
        let resid: f64 = capture[0].iter().map(|x| (*x as f64) * (*x as f64)).sum();
        echo_in_acc += echo_in;
        resid_acc += resid;

        if (f + 1) % 200 == 0 {
            erle_by_block.push(10.0 * (echo_in_acc / resid_acc.max(1e-15)).log10());
            echo_in_acc = 0.0;
            resid_acc = 0.0;
        }
    }

    println!("\nprocessing cost (render+capture, 10 ms frame):");
    println!("  p50 {:.1} us   p99 {:.1} us   = {:.2}% of the 10 ms real-time budget",
             pct(&mut us.clone(), 0.50), pct(&mut us.clone(), 0.99),
             pct(&mut us.clone(), 0.99) / 10_000.0 * 100.0);

    println!("\nERLE convergence, far-end single talk (dB, higher = more echo removed):");
    for (i, e) in erle_by_block.iter().enumerate() {
        println!("  t={:>4.1}s  {:>6.1} dB", (i + 1) as f64 * 2.0, e);
    }
    let final_erle = erle_by_block.last().copied().unwrap_or(0.0);
    println!("\nfinal ERLE: {:.1} dB  ({})", final_erle,
             if final_erle > 20.0 { "good - echo strongly suppressed" }
             else if final_erle > 10.0 { "moderate" }
             else { "WEAK - investigate" });

    println!("\nstats: {:?}", ap.get_stats());
    println!("\nNOTE: synthetic linear echo path. Real rooms add reverb, speaker");
    println!("non-linearity and clock drift. Two machines still required to validate.");
}
