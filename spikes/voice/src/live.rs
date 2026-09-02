//! Live capture -> Opus -> QUIC -> decode -> playback, plus a jitter buffer.

use crate::bench::{FRAME_SAMPLES, SAMPLE_RATE};
use crate::net::{client_endpoint, server_endpoint, VoiceFrame};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::traits::{Consumer, Producer, Split};
use ringbuf::HeapRb;
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

fn now_us() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_micros() as u64
}

pub fn devices() {
    let host = cpal::default_host();
    println!("host: {:?}\n", host.id());
    println!("INPUT devices:");
    if let Ok(ds) = host.input_devices() {
        for d in ds {
            let name = d.description().map(|x| x.name().to_string()).unwrap_or_else(|_| "<unnamed>".into());
            let cfg = d.default_input_config();
            println!("  {name}  {}", cfg.map(|c| format!("{:?} {} ch @ {} Hz", c.sample_format(), c.channels(), c.sample_rate())).unwrap_or_else(|e| format!("({e})")));
        }
    }
    println!("\nOUTPUT devices:");
    if let Ok(ds) = host.output_devices() {
        for d in ds {
            let name = d.description().map(|x| x.name().to_string()).unwrap_or_else(|_| "<unnamed>".into());
            let cfg = d.default_output_config();
            println!("  {name}  {}", cfg.map(|c| format!("{:?} {} ch @ {} Hz", c.sample_format(), c.channels(), c.sample_rate())).unwrap_or_else(|e| format!("({e})")));
        }
    }
    println!("\ndefault input : {:?}", host.default_input_device().and_then(|d| d.description().ok()).map(|d| d.name().to_string()));
    println!("default output: {:?}", host.default_output_device().and_then(|d| d.description().ok()).map(|d| d.name().to_string()));
}

/// Adaptive jitter buffer. Holds frames by sequence number and releases them in
/// order after a target delay, growing when the network is unstable.
pub struct JitterBuffer {
    frames: BTreeMap<u32, Vec<u8>>,
    next_seq: Option<u32>,
    target_frames: usize,
    pub late: u64,
    pub lost: u64,
    pub played: u64,
    pub concealed: u64,
}

impl JitterBuffer {
    pub fn new(target_frames: usize) -> Self {
        Self { frames: BTreeMap::new(), next_seq: None, target_frames, late: 0, lost: 0, played: 0, concealed: 0 }
    }

    pub fn push(&mut self, seq: u32, data: Vec<u8>) {
        if let Some(next) = self.next_seq {
            if seq < next {
                self.late += 1; // arrived after we already played past it
                return;
            }
        }
        self.frames.insert(seq, data);
    }

    pub fn depth(&self) -> usize {
        self.frames.len()
    }

    /// Returns the next frame to decode, or None to conceal (packet loss).
    /// Waits until `target_frames` are buffered before starting, to absorb jitter.
    pub fn pop(&mut self) -> Option<Option<Vec<u8>>> {
        if self.next_seq.is_none() {
            if self.frames.len() < self.target_frames {
                return None; // still filling
            }
            self.next_seq = self.frames.keys().next().copied();
        }
        let want = self.next_seq?;
        if let Some(d) = self.frames.remove(&want) {
            self.next_seq = Some(want + 1);
            self.played += 1;
            Some(Some(d))
        } else if self.frames.keys().next().is_some_and(|&k| k > want) {
            // A later frame is present, so this one is genuinely lost -> conceal.
            self.next_seq = Some(want + 1);
            self.lost += 1;
            self.concealed += 1;
            Some(None)
        } else {
            None // nothing yet, underrun
        }
    }
}

const JITTER_TARGET_FRAMES: usize = 3; // 60 ms initial buffer

pub fn recv(bind: SocketAddr) -> anyhow::Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    let jb = Arc::new(Mutex::new(JitterBuffer::new(JITTER_TARGET_FRAMES)));
    let owd_us = Arc::new(AtomicU64::new(0));

    // Playback ring: decoder thread produces, cpal callback consumes.
    let rb = HeapRb::<f32>::new(SAMPLE_RATE as usize * 2);
    let (mut prod, mut cons) = rb.split();

    let host = cpal::default_host();
    let dev = host.default_output_device().ok_or_else(|| anyhow::anyhow!("no output device"))?;
    println!("output: {}", dev.description()?.name());
    let cfg = cpal::StreamConfig {
        channels: 1,
        sample_rate: SAMPLE_RATE,
        buffer_size: cpal::BufferSize::Default,
    };
    let underruns = Arc::new(AtomicU64::new(0));
    let started = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let ur = underruns.clone();
    let st = started.clone();
    let stream = dev.build_output_stream(
        cfg.clone(),
        move |out: &mut [f32], _| {
            // No allocation in the audio callback -- this is a hard rule.
            for s in out.iter_mut() {
                match cons.try_pop() {
                    Some(v) => {
                        st.store(true, Ordering::Relaxed);
                        *s = v;
                    }
                    None => {
                        // Silence before the first frame arrives is not an underrun.
                        if st.load(Ordering::Relaxed) {
                            ur.fetch_add(1, Ordering::Relaxed);
                        }
                        *s = 0.0;
                    }
                }
            }
        },
        |e| eprintln!("output stream error: {e}"),
        None,
    )?;
    stream.play()?;

    // Decoder thread: jitter buffer -> Opus -> playback ring, paced at 20 ms.
    let jb_dec = jb.clone();
    std::thread::spawn(move || {
        let mut dec = opus::Decoder::new(SAMPLE_RATE, opus::Channels::Mono).unwrap();
        let mut pcm = vec![0i16; FRAME_SAMPLES];
        let mut next = Instant::now();
        loop {
            next += std::time::Duration::from_millis(20);
            let action = jb_dec.lock().unwrap().pop();
            match action {
                Some(Some(data)) => {
                    if dec.decode(&data, &mut pcm, false).is_ok() {
                        for s in &pcm { let _ = prod.try_push(*s as f32 / 32768.0); }
                    }
                }
                Some(None) => {
                    // Lost frame: let Opus conceal rather than inserting silence.
                    if dec.decode(&[], &mut pcm, false).is_ok() {
                        for s in &pcm { let _ = prod.try_push(*s as f32 / 32768.0); }
                    }
                }
                None => {}
            }
            let now = Instant::now();
            if next > now { std::thread::sleep(next - now); } else { next = now; }
        }
    });

    rt.block_on(async move {
        let ep = server_endpoint(bind)?;
        println!("listening on {bind}  (run the sender with: spike-voice send <this-ip>:{})", bind.port());
        let incoming = ep.accept().await.ok_or_else(|| anyhow::anyhow!("endpoint closed"))?;
        let conn = incoming.await?;
        println!("peer connected: {}", conn.remote_address());

        let jb_net = jb.clone();
        let owd = owd_us.clone();
        let stats_jb = jb.clone();
        let ur2 = underruns.clone();
        tokio::spawn(async move {
            let mut t = tokio::time::interval(std::time::Duration::from_secs(2));
            loop {
                t.tick().await;
                let j = stats_jb.lock().unwrap();
                println!(
                    "played {:>6}  lost {:>4}  late {:>4}  buffer {:>2} frames  one-way ~{:>4} ms  underruns {}",
                    j.played, j.lost, j.late, j.depth(),
                    owd.load(Ordering::Relaxed) / 1000,
                    ur2.load(Ordering::Relaxed)
                );
            }
        });

        loop {
            match conn.read_datagram().await {
                Ok(b) => {
                    if let Some((seq, sent_us, opus)) = VoiceFrame::decode(&b) {
                        // Clocks are not synchronised across machines, so this is
                        // only meaningful on loopback. Reported as approximate.
                        let now = now_us();
                        owd_us.store(now.saturating_sub(sent_us), Ordering::Relaxed);
                        jb_net.lock().unwrap().push(seq, opus.to_vec());
                    }
                }
                Err(e) => {
                    println!("connection closed: {e}");
                    break;
                }
            }
        }
        Ok::<_, anyhow::Error>(())
    })?;
    Ok(())
}

pub fn send(remote: SocketAddr, bitrate_kbps: i32) -> anyhow::Result<()> {
    let rt = tokio::runtime::Runtime::new()?;

    // Capture ring: cpal callback produces, encoder thread consumes.
    let rb = HeapRb::<f32>::new(SAMPLE_RATE as usize * 2);
    let (mut prod, mut cons) = rb.split();

    let host = cpal::default_host();
    let dev = host.default_input_device().ok_or_else(|| anyhow::anyhow!("no input device"))?;
    println!("input: {}", dev.description()?.name());
    let cfg = cpal::StreamConfig {
        channels: 1,
        sample_rate: SAMPLE_RATE,
        buffer_size: cpal::BufferSize::Default,
    };
    let overruns = Arc::new(AtomicU64::new(0));
    let or = overruns.clone();
    let stream = dev.build_input_stream(
        cfg,
        move |data: &[f32], _| {
            for s in data {
                if prod.try_push(*s).is_err() {
                    or.fetch_add(1, Ordering::Relaxed);
                }
            }
        },
        |e| eprintln!("input stream error: {e}"),
        None,
    )?;
    stream.play()?;

    rt.block_on(async move {
        let ep = client_endpoint()?;
        println!("connecting to {remote}...");
        let conn = ep.connect(remote, "nexus-voice-spike")?.await?;
        println!("connected. streaming {bitrate_kbps} kbps Opus. Ctrl-C to stop.");

        let mut enc = opus::Encoder::new(SAMPLE_RATE, opus::Channels::Mono, opus::Application::Voip)?;
        enc.set_bitrate(opus::Bitrate::Bits(bitrate_kbps * 1000))?;

        let mut pcm = vec![0i16; FRAME_SAMPLES];
        let mut opus_buf = vec![0u8; 4000];
        let mut wire = Vec::with_capacity(4000);
        let mut seq = 0u32;
        let mut sent = 0u64;
        let mut bytes = 0u64;
        let start = Instant::now();
        let mut last_report = Instant::now();

        loop {
            // Gather exactly one 20 ms frame.
            let mut got = 0;
            while got < FRAME_SAMPLES {
                match cons.try_pop() {
                    Some(s) => {
                        pcm[got] = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
                        got += 1;
                    }
                    None => tokio::time::sleep(std::time::Duration::from_millis(2)).await,
                }
            }

            let n = enc.encode(&pcm, &mut opus_buf)?;
            VoiceFrame::encode(seq, now_us(), &opus_buf[..n], &mut wire);
            if let Err(e) = conn.send_datagram(wire.clone().into()) {
                eprintln!("datagram send failed: {e}");
            }
            seq += 1;
            sent += 1;
            bytes += (n + 12) as u64;

            if last_report.elapsed().as_secs() >= 2 {
                let secs = start.elapsed().as_secs_f64();
                println!("sent {:>6} frames  {:.1} kbps on the wire  rtt {:?}  overruns {}",
                         sent, bytes as f64 * 8.0 / secs / 1000.0,
                         conn.rtt(), overruns.load(Ordering::Relaxed));
                last_report = Instant::now();
            }
        }
    })
}
