/// Representative conversational chat corpus: short informal lines, some code,
/// links and emoji, with the heavy vocabulary reuse real chat exhibits.
fn corpus() -> Vec<String> {
    let openers = [
        "hey", "yo", "ok so", "wait", "hmm", "lol", "yeah", "nah", "tbh", "honestly",
        "i think", "pretty sure", "just pushed", "can you", "did you", "sorry", "np",
        "gm", "afk brb", "back", "one sec", "actually", "oh", "right", "sure",
    ];
    let bodies = [
        "the build is failing again on main",
        "i'll take a look after lunch",
        "did the migration run on staging yet",
        "that PR is ready for review whenever you have a sec",
        "we should probably just revert it for now",
        "the latency spike is coming from the db pool",
        "works on my machine lol",
        "can you send me the logs",
        "i pushed a fix, should be green now",
        "meeting moved to 3pm",
        "anyone else seeing this error",
        "it's a race condition in the reconnect path",
        "let me rebase and try again",
        "the docs are out of date",
        "we need to bump that timeout",
        "i don't think that's the actual root cause",
        "deploy went out fine",
        "rolling back",
        "who's on call this week",
        "that's the third time today",
        "looks like a memory leak in the worker",
        "adding a test for that case now",
        "good catch, thanks",
        "should be fixed in the next release",
        "can we pair on this tomorrow morning",
    ];
    let closers = ["", "", "", " 👍", " lol", " 😅", "?", "!", " thanks", " 🙏", " ok?"];
    let links = [
        "https://github.com/nexus/nexus-suite/pull/1421",
        "https://docs.rs/openmls/latest/openmls/",
        "https://github.com/nexus/nexus-suite/issues/88",
    ];
    let code = [
        "`cargo clippy --workspace -- -D warnings`",
        "```rust\nlet x = foo().await?;\n```",
        "`RUST_LOG=debug cargo run`",
    ];

    let mut out = Vec::new();
    let mut seed: u64 = 0x9E3779B97F4A7C15;
    let mut rnd = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };

    for _ in 0..8000u64 {
        let r = rnd();
        let msg = match r % 20 {
            0 => links[(r >> 8) as usize % links.len()].to_string(),
            1 => code[(r >> 8) as usize % code.len()].to_string(),
            2..=4 => bodies[(r >> 8) as usize % bodies.len()].to_string(),
            5 => openers[(r >> 8) as usize % openers.len()].to_string(),
            _ => format!(
                "{} {}{}",
                openers[(r >> 8) as usize % openers.len()],
                bodies[(r >> 16) as usize % bodies.len()],
                closers[(r >> 24) as usize % closers.len()]
            ),
        };
        out.push(msg);
    }
    out
}
