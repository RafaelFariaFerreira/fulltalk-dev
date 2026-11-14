//! Measures resident memory after loading the model (Linux only).
//!
//! ```sh
//! cargo run --release -p fulltalk-asr --example memory -- <model-dir> [lean]
//! ```
use fulltalk_asr::{EngineOptions, ParakeetEngine};
use fulltalk_core::ports::SpeechToText;

fn rss_mb() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("VmRSS"))
                .and_then(|l| l.split_whitespace().nth(1)?.parse::<u64>().ok())
        })
        .map_or(0, |kb| kb / 1024)
}

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = args.next().expect("model dir");
    let lean = args.next().as_deref() == Some("lean");
    let options = EngineOptions {
        threads: None,
        lean_memory: lean,
    };
    let t = std::time::Instant::now();
    let mut engine = ParakeetEngine::load(dir.as_ref(), options).unwrap();
    println!(
        "lean={lean} load={}ms rss_after_load={} MB",
        t.elapsed().as_millis(),
        rss_mb()
    );
    let audio: Vec<f32> = (0..16_000 * 5)
        .map(|i| (i as f32 * 0.05).sin() * 0.1)
        .collect();
    for _ in 0..3 {
        let t = std::time::Instant::now();
        engine.transcribe(&audio).unwrap();
        println!(
            "  infer 5s: {}ms rss={} MB",
            t.elapsed().as_millis(),
            rss_mb()
        );
    }
}
