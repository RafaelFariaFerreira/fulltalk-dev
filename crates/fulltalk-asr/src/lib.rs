//! # fulltalk-asr
//!
//! Local, offline speech recognition for **fulltalk dev**.
//!
//! * [`model`] — pinned model files, verified download/installation.
//! * [`ParakeetEngine`] — NVIDIA Parakeet TDT 0.6B v3 (int8) on the CPU via ONNX
//!   Runtime, implementing [`fulltalk_core::ports::SpeechToText`].

pub mod model;

use std::path::Path;

use fulltalk_core::audio::MODEL_SAMPLE_RATE;
use fulltalk_core::ports::{PortError, SpeechToText};
use parakeet_rs::{ExecutionConfig, ParakeetTDT, Transcriber};

/// Runtime knobs for [`ParakeetEngine`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EngineOptions {
    /// Bounds CPU usage; `None` lets ONNX Runtime pick (physical cores).
    pub threads: Option<usize>,
    /// Trade a little speed for a much smaller resident footprint: disables
    /// weight pre-packing (a second, re-laid-out copy of every int8 weight)
    /// and the memory-pattern planner.
    pub lean_memory: bool,
}

impl Default for EngineOptions {
    fn default() -> Self {
        Self {
            threads: None,
            lean_memory: true,
        }
    }
}

/// Parakeet TDT running on ONNX Runtime (CPU).
pub struct ParakeetEngine {
    model: ParakeetTDT,
}

impl std::fmt::Debug for ParakeetEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ParakeetEngine").finish_non_exhaustive()
    }
}

impl ParakeetEngine {
    /// Loads the model from `dir` (see [`model::PARAKEET_TDT_V3_INT8`]).
    pub fn load(dir: &Path, options: EngineOptions) -> Result<Self, PortError> {
        let env = parakeet_rs::ort::init()
            .with_name("fulltalk")
            .build()
            .map_err(|e| PortError::new(format!("ONNX Runtime: {e}")))?;
        let mut config = ExecutionConfig::new().with_environment(env);
        if let Some(n) = options.threads {
            config = config.with_intra_threads(n);
        }
        if options.lean_memory {
            config = config.with_custom_configure(|builder| {
                Ok(builder.with_prepacking(false)?.with_memory_pattern(false)?)
            });
        }
        let model = ParakeetTDT::from_pretrained(dir, Some(config))
            .map_err(|e| PortError::new(format!("não foi possível carregar o modelo: {e}")))?;
        Ok(Self { model })
    }
}

impl SpeechToText for ParakeetEngine {
    fn transcribe(&mut self, samples_16k_mono: &[f32]) -> Result<String, PortError> {
        self.model
            .transcribe_samples(samples_16k_mono.to_vec(), MODEL_SAMPLE_RATE, 1, None)
            .map(|r| r.text)
            .map_err(|e| PortError::new(format!("falha na transcrição: {e}")))
    }
}
