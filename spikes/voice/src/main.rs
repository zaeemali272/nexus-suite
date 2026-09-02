//! Spike 0.3 — voice pipeline.
//!
//! Modes:
//!   bench     - codec + AEC cost. No audio devices needed. Runs anywhere.
//!   devices   - enumerate audio devices.
//!   loopback  - mic -> opus -> QUIC(localhost) -> decode -> speaker, one machine.
//!   recv <bind> / send <addr> - the two-machine test.
//!
//! Acoustic echo cancellation can only be *validated* with two machines and open
//! speakers. Everything else is measurable locally.

mod bench;
mod live;
mod net;

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "bench".into());
    match mode.as_str() {
        "bench" => bench::run(),
        "devices" => live::devices(),
        "recv" => {
            let bind = std::env::args().nth(2).unwrap_or_else(|| "0.0.0.0:4433".into());
            if let Err(e) = live::recv(bind.parse().expect("bad bind address")) {
                eprintln!("recv failed: {e}");
                std::process::exit(1);
            }
        }
        "send" => {
            let addr = std::env::args().nth(2).expect("usage: send <host:port> [kbps]");
            let kbps = std::env::args().nth(3).and_then(|s| s.parse().ok()).unwrap_or(24);
            if let Err(e) = live::send(addr.parse().expect("bad remote address"), kbps) {
                eprintln!("send failed: {e}");
                std::process::exit(1);
            }
        }
        other => {
            eprintln!("unknown mode: {other}");
            eprintln!("usage: spike-voice [bench|devices|loopback|recv <bind>|send <addr>]");
            std::process::exit(2);
        }
    }
}
