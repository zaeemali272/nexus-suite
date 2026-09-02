//! Spike 0.2 — can Slint scroll 100k variable-height messages at 120 fps?
//!
//! Exit criteria (ROADMAP): measured frame times under 8 ms with the real
//! message renderer. Budget (BRD 6.1): < 8 ms target, 16 ms hard ceiling.
//!
//! Also reports process RSS with the full model loaded, against the 60 MB budget.

use slint::{ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

slint::include_modules!();

fn messages_count() -> usize {
    std::env::var("SPIKE_MESSAGES").ok().and_then(|v| v.parse().ok()).unwrap_or(100_000)
}
const WARMUP_FRAMES: usize = 60;
const MEASURE_FRAMES: usize = 400;

fn rss_kb() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("VmRSS:"))
                .and_then(|l| l.split_whitespace().nth(1).and_then(|v| v.parse().ok()))
        })
        .unwrap_or(0)
}

/// RSS counts shared library and GPU-driver pages in full, which overstates what
/// the process actually costs the system. PSS divides shared pages by the number
/// of sharers; USS is private-only. Report all three.
fn mem_kb() -> (u64, u64, u64) {
    let s = std::fs::read_to_string("/proc/self/smaps_rollup").unwrap_or_default();
    let get = |key: &str| -> u64 {
        s.lines()
            .find(|l| l.starts_with(key))
            .and_then(|l| l.split_whitespace().nth(1).and_then(|v| v.parse().ok()))
            .unwrap_or(0)
    };
    let priv_clean = get("Private_Clean:");
    let priv_dirty = get("Private_Dirty:");
    (get("Rss:"), get("Pss:"), priv_clean + priv_dirty)
}

fn pct(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    sorted[(((sorted.len() - 1) as f64) * p) as usize]
}

/// Variable-height conversational content, so the list cannot cheat with a
/// uniform row height.
fn build_messages() -> Vec<Msg> {
    let _n_messages = messages_count();
    let authors = ["alice", "bob", "charlie", "dana", "erin", "frank"];
    let fragments = [
        "the build is failing again on main",
        "i'll take a look after lunch",
        "did the migration run on staging yet",
        "that PR is ready for review whenever you have a sec, i rebased it onto main this morning and squashed the fixup commits so it should be a clean read now",
        "we should probably just revert it for now and figure out the root cause tomorrow when everyone is around",
        "the latency spike is coming from the db pool",
        "works on my machine",
        "can you send me the logs",
        "i pushed a fix, should be green now",
        "anyone else seeing this error? it started right after the deploy went out and i can reproduce it locally but only when the cache is cold, which makes me think it is a warmup path problem rather than anything to do with the actual query",
        "it's a race condition in the reconnect path",
        "good catch, thanks",
    ];

    let mut seed: u64 = 0x243F6A8885A308D3;
    let mut rnd = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };

    (0..messages_count())
        .map(|i| {
            let r = rnd();
            let body = fragments[(r >> 8) as usize % fragments.len()];
            // Height estimated from content length, mimicking wrapped text.
            let lines = 1 + body.len() / 60;
            Msg {
                author: SharedString::from(authors[(r >> 20) as usize % authors.len()]),
                body: SharedString::from(format!("{body}  #{i}")),
                h: 26.0 + lines as f32 * 19.0,
                own: i % 7 == 0,
            }
        })
        .collect()
}

fn main() -> Result<(), slint::PlatformError> {
    let t_start = Instant::now();
    let rss_before = rss_kb();

    let n = messages_count();
    println!("building {n} messages...");
    let t = Instant::now();
    let msgs = build_messages();
    let total_height: f32 = msgs.iter().map(|m| m.h).sum();
    let build_ms = t.elapsed().as_secs_f64() * 1e3;
    println!("  built in {build_ms:.0} ms, total viewport height {:.0} px", total_height);

    let model = Rc::new(VecModel::from(msgs));
    let app = AppWindow::new()?;
    app.set_messages(ModelRc::from(model.clone()));

    let rss_loaded = rss_kb();
    println!("  RSS after model load: {:.1} MB (delta {:.1} MB)",
             rss_loaded as f64 / 1024.0, (rss_loaded - rss_before) as f64 / 1024.0);

    // Frame instrumentation. `render_us` is the actual cost of producing a frame
    // (BeforeRendering -> AfterRendering). `interval_ms` is wall time between
    // frames, which is vsync-capped and reveals dropped frames.
    struct Stats {
        render_us: Vec<f64>,
        interval_ms: Vec<f64>,
        frame_start: Option<Instant>,
        last_frame: Option<Instant>,
        count: usize,
        done: bool,
    }
    let stats = Rc::new(RefCell::new(Stats {
        render_us: Vec::with_capacity(MEASURE_FRAMES),
        interval_ms: Vec::with_capacity(MEASURE_FRAMES),
        frame_start: None,
        last_frame: None,
        count: 0,
        done: false,
    }));

    let stats_r = stats.clone();
    let notifier = app.window()
        .set_rendering_notifier(move |state, _graphics| {
            let mut s = stats_r.borrow_mut();
            match state {
                slint::RenderingState::BeforeRendering => {
                    let now = Instant::now();
                    if let Some(prev) = s.last_frame {
                        let dt = now.duration_since(prev).as_secs_f64() * 1e3;
                        if s.count >= WARMUP_FRAMES {
                            s.interval_ms.push(dt);
                        }
                    }
                    s.last_frame = Some(now);
                    s.frame_start = Some(now);
                }
                slint::RenderingState::AfterRendering => {
                    if let Some(start) = s.frame_start.take() {
                        let us = start.elapsed().as_secs_f64() * 1e6;
                        if s.count >= WARMUP_FRAMES {
                            s.render_us.push(us);
                        }
                    }
                    s.count += 1;
                    // Never hide() from inside the notifier -- suspending the
                    // renderer mid-frame panics. Signal the timer instead.
                    if s.count >= WARMUP_FRAMES + MEASURE_FRAMES {
                        s.done = true;
                    }
                }
                _ => {}
            }
        });
    // The software renderer has no graphics-API notifier. Fall back to
    // RSS-only measurement there rather than aborting.
    let have_notifier = notifier.is_ok();
    if !have_notifier {
        println!("  note: rendering notifier unsupported on this backend -- RSS only");
    }

    // Drive a continuous scroll. Slint animates on demand, so we push a new
    // scroll offset every tick and let the compositor pace us.
    let app_weak = app.as_weak();
    let stats_t = stats.clone();
    let scroll_timer = slint::Timer::default();
    let max_scroll = (total_height - 700.0).max(1.0);
    scroll_timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(4),
        move || {
            if let Some(a) = app_weak.upgrade() {
                if !have_notifier && t_start.elapsed().as_secs_f64() > 6.0 {
                    let _ = a.window().hide();
                    return;
                }
                if stats_t.borrow().done {
                    let _ = a.window().hide();
                    return;
                }
                let n = stats_t.borrow().count as f32;
                // Sweep deep into the list, not just the first screen, so
                // virtualisation is genuinely exercised.
                let pos = (n * 137.0) % max_scroll;
                a.set_scroll_y(-pos);
                let s = stats_t.borrow();
                if s.count % 60 == 0 && !s.render_us.is_empty() {
                    let last = *s.render_us.last().unwrap();
                    a.set_hud(SharedString::from(format!(
                        "frame {} / {}   render {:.2} ms   scroll {:.0}px",
                        s.count,
                        WARMUP_FRAMES + MEASURE_FRAMES,
                        last / 1000.0,
                        pos
                    )));
                }
            }
        },
    );

    app.run()?;

    // ---- report ----
    let s = stats.borrow();
    let mut r = s.render_us.clone();
    let mut i = s.interval_ms.clone();
    r.sort_by(|a, b| a.partial_cmp(b).unwrap());
    i.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let (rss_end, pss_end, uss_end) = mem_kb();
    println!("\n=== Spike 0.2 results ===");
    println!("messages: {n}, frames measured: {}", r.len());
    println!("\nrender cost (BeforeRendering -> AfterRendering):");
    println!("  p50 {:.2} ms   p90 {:.2} ms   p99 {:.2} ms   max {:.2} ms",
             pct(&r, 0.50) / 1000.0, pct(&r, 0.90) / 1000.0,
             pct(&r, 0.99) / 1000.0, r.last().copied().unwrap_or(0.0) / 1000.0);
    println!("\nframe interval (vsync-capped, reveals dropped frames):");
    println!("  p50 {:.2} ms   p90 {:.2} ms   p99 {:.2} ms   max {:.2} ms",
             pct(&i, 0.50), pct(&i, 0.90), pct(&i, 0.99), i.last().copied().unwrap_or(0.0));
    if !i.is_empty() {
        println!("  effective fps (p50): {:.0}", 1000.0 / pct(&i, 0.50).max(0.001));
    }
    println!("\nmemory with {n} messages resident:");
    println!("  RSS {:.1} MB   PSS {:.1} MB   USS(private) {:.1} MB",
             rss_end as f64 / 1024.0, pss_end as f64 / 1024.0, uss_end as f64 / 1024.0);
    println!("total runtime {:.1}s", t_start.elapsed().as_secs_f64());
    println!("\nBudget (BRD 6.1): render < 8 ms target, 16 ms ceiling. RSS < 60 MB / 90 MB ceiling.");
    Ok(())
}
