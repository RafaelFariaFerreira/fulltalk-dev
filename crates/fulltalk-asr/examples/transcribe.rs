//! Transcribe a WAV file and print timings — handy for benchmarking.
//!
//! ```sh
//! cargo run --release -p fulltalk-asr --example transcribe -- <model-dir> <file.wav>
//! ```

use std::time::Instant;

use fulltalk_asr::{EngineOptions, ParakeetEngine};
use fulltalk_core::audio::{CaptureBuffer, MODEL_SAMPLE_RATE};
use fulltalk_core::ports::SpeechToText;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let (Some(model_dir), Some(wav)) = (args.next(), args.next()) else {
        eprintln!("usage: transcribe <model-dir> <file.wav>");
        std::process::exit(2);
    };

    let mut reader = hound::WavReader::open(&wav)?;
    let spec = reader.spec();
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().collect::<Result<_, _>>()?,
        hound::SampleFormat::Int => {
            let scale = f32::from(1u16 << (spec.bits_per_sample.min(16) - 1));
            reader
                .samples::<i32>()
                .map(|s| s.map(|v| v as f32 / scale))
                .collect::<Result<_, _>>()?
        }
    };
    let mut capture = CaptureBuffer::new(spec.sample_rate, spec.channels);
    capture.push_interleaved(&samples);
    let seconds = capture.duration_seconds();
    let audio = capture.finish().map_err(|r| format!("rejected: {r:?}"))?;
    assert_eq!(audio.len(), (seconds * MODEL_SAMPLE_RATE as f32) as usize);

    let t = Instant::now();
    let mut engine = ParakeetEngine::load(model_dir.as_ref(), EngineOptions::default())?;
    println!("load:       {:>6.0} ms", t.elapsed().as_secs_f64() * 1e3);

    for run in 1..=3 {
        let t = Instant::now();
        let text = engine.transcribe(&audio)?;
        let ms = t.elapsed().as_secs_f64() * 1e3;
        println!("run {run}:      {ms:>6.0} ms for {seconds:.1}s of audio → {text:?}");
    }
    Ok(())
}
