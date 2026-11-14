//! Microphone capture with cpal. The stream only exists while recording, so the
//! OS microphone indicator is off the rest of the time.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream};
use fulltalk_core::audio::{CaptureBuffer, meter_level};

pub struct Recorder {
    stream: Option<Stream>,
    buffer: Arc<Mutex<Option<CaptureBuffer>>>,
    /// Latest meter level (0–1) as `f32` bits, written by the audio thread.
    level: Arc<AtomicU32>,
}

impl std::fmt::Debug for Recorder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Recorder")
            .field("recording", &self.stream.is_some())
            .finish_non_exhaustive()
    }
}

impl Recorder {
    pub fn new() -> Self {
        Self {
            stream: None,
            buffer: Arc::default(),
            level: Arc::default(),
        }
    }

    /// Opens the default microphone and starts buffering.
    /// `on_error` runs on the audio thread if the device fails mid-stream.
    pub fn start(&mut self, on_error: impl Fn(String) + Send + 'static) -> Result<(), String> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or("nenhum microfone encontrado")?;
        let supported = device
            .default_input_config()
            .map_err(|e| format!("microfone indisponível: {e}"))?;
        let config = supported.config();

        *self.lock_buffer() = Some(CaptureBuffer::new(config.sample_rate, config.channels));
        self.level.store(0, Ordering::Relaxed);

        let err_cb = move |e: cpal::Error| on_error(format!("o microfone parou: {e}"));
        let stream = match supported.sample_format() {
            SampleFormat::F32 => {
                let sink = self.sink();
                device.build_input_stream(config, move |d: &[f32], _: &_| sink(d), err_cb, None)
            }
            SampleFormat::I16 => {
                let sink = self.sink();
                device.build_input_stream(
                    config,
                    move |d: &[i16], _: &_| {
                        let f: Vec<f32> = d.iter().map(|s| f32::from(*s) / 32_768.0).collect();
                        sink(&f);
                    },
                    err_cb,
                    None,
                )
            }
            SampleFormat::U16 => {
                let sink = self.sink();
                device.build_input_stream(
                    config,
                    move |d: &[u16], _: &_| {
                        let f: Vec<f32> = d
                            .iter()
                            .map(|s| (f32::from(*s) - 32_768.0) / 32_768.0)
                            .collect();
                        sink(&f);
                    },
                    err_cb,
                    None,
                )
            }
            other => return Err(format!("formato de áudio não suportado: {other}")),
        }
        .map_err(|e| format!("não foi possível abrir o microfone: {e}"))?;

        stream
            .play()
            .map_err(|e| format!("não foi possível iniciar a gravação: {e}"))?;
        self.stream = Some(stream);
        Ok(())
    }

    /// Stops the microphone and hands over what was captured.
    pub fn stop(&mut self) -> Option<CaptureBuffer> {
        self.stream = None; // dropping the stream closes the device
        self.level.store(0, Ordering::Relaxed);
        self.lock_buffer().take()
    }

    /// Live input level, 0–1.
    pub fn level(&self) -> f32 {
        f32::from_bits(self.level.load(Ordering::Relaxed))
    }

    fn sink(&self) -> impl Fn(&[f32]) + Send + 'static {
        let buffer = Arc::clone(&self.buffer);
        let level = Arc::clone(&self.level);
        move |data: &[f32]| {
            if let Ok(mut guard) = buffer.lock()
                && let Some(buf) = guard.as_mut()
            {
                let rms = buf.push_interleaved(data);
                level.store(meter_level(rms).to_bits(), Ordering::Relaxed);
            }
        }
    }

    fn lock_buffer(&self) -> std::sync::MutexGuard<'_, Option<CaptureBuffer>> {
        self.buffer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}
