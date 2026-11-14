//! Audio helpers: capture accumulation, mono down-mix, resampling to the
//! model rate, level metering and "was anything said?" checks.

/// Sample rate expected by the speech model.
pub const MODEL_SAMPLE_RATE: u32 = 16_000;

/// Recordings shorter than this are treated as accidental taps.
pub const MIN_SPEECH_SECONDS: f32 = 0.3;

/// Hard cap that bounds memory use if the shortcut gets stuck.
pub const MAX_RECORDING_SECONDS: u32 = 5 * 60;

/// RMS below this (≈ -50 dBFS) across the whole clip counts as silence.
const SILENCE_RMS: f32 = 0.003;

/// Accumulates interleaved device frames as mono samples at the device rate.
#[derive(Debug, Clone)]
pub struct CaptureBuffer {
    sample_rate: u32,
    channels: u16,
    samples: Vec<f32>,
    max_samples: usize,
}

impl CaptureBuffer {
    /// # Panics
    /// If `sample_rate` or `channels` is zero.
    pub fn new(sample_rate: u32, channels: u16) -> Self {
        assert!(sample_rate > 0 && channels > 0, "invalid audio format");
        let max_samples = (sample_rate * MAX_RECORDING_SECONDS) as usize;
        Self {
            sample_rate,
            channels,
            samples: Vec::with_capacity(sample_rate as usize * 10),
            max_samples,
        }
    }

    /// Appends interleaved frames, averaging channels. Returns the RMS level of
    /// this chunk (for the live meter). Input beyond the cap is dropped.
    pub fn push_interleaved(&mut self, data: &[f32]) -> f32 {
        let channels = usize::from(self.channels);
        let room = self.max_samples.saturating_sub(self.samples.len());
        let before = self.samples.len();
        self.samples.extend(
            data.chunks_exact(channels)
                .take(room)
                .map(|frame| frame.iter().sum::<f32>() / channels as f32),
        );
        rms(&self.samples[before..])
    }

    pub fn duration_seconds(&self) -> f32 {
        self.samples.len() as f32 / self.sample_rate as f32
    }

    /// Converts the capture into model input, or explains why there is nothing to transcribe.
    pub fn finish(self) -> Result<Vec<f32>, Rejected> {
        if self.duration_seconds() < MIN_SPEECH_SECONDS {
            return Err(Rejected::TooShort);
        }
        if rms(&self.samples) < SILENCE_RMS {
            return Err(Rejected::Silent);
        }
        Ok(resample(&self.samples, self.sample_rate, MODEL_SAMPLE_RATE))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rejected {
    TooShort,
    Silent,
}

/// Root-mean-square amplitude; `0.0` for empty input.
pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f64 = samples.iter().map(|s| f64::from(*s) * f64::from(*s)).sum();
    (sum / samples.len() as f64).sqrt() as f32
}

/// Maps an RMS amplitude to a 0–1 meter value on a -60…0 dBFS scale.
pub fn meter_level(rms: f32) -> f32 {
    if rms <= 0.0 {
        return 0.0;
    }
    let db = 20.0 * rms.log10();
    ((db + 60.0) / 60.0).clamp(0.0, 1.0)
}

/// Resamples mono audio with linear interpolation. When downsampling, a moving
/// average of one output period is applied first as a cheap anti-alias filter —
/// plenty for speech recognition and far lighter than a polyphase resampler.
pub fn resample(samples: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || samples.is_empty() {
        return samples.to_vec();
    }
    let ratio = f64::from(from) / f64::from(to);
    let filtered;
    let source = if ratio > 1.0 {
        filtered = moving_average(samples, ratio.round() as usize);
        &filtered
    } else {
        samples
    };

    let out_len = ((source.len() as f64) / ratio).floor() as usize;
    let last = source.len() - 1;
    (0..out_len)
        .map(|i| {
            let pos = i as f64 * ratio;
            let idx = pos.floor() as usize;
            let frac = (pos - idx as f64) as f32;
            let a = source[idx.min(last)];
            let b = source[(idx + 1).min(last)];
            a + (b - a) * frac
        })
        .collect()
}

/// Centered moving average with window `width` (no-op for width <= 1).
fn moving_average(samples: &[f32], width: usize) -> Vec<f32> {
    if width <= 1 {
        return samples.to_vec();
    }
    let half = width / 2;
    let mut prefix = Vec::with_capacity(samples.len() + 1);
    prefix.push(0.0f64);
    for s in samples {
        prefix.push(prefix.last().copied().unwrap_or_default() + f64::from(*s));
    }
    (0..samples.len())
        .map(|i| {
            let lo = i.saturating_sub(half);
            let hi = (i + width - half).min(samples.len());
            ((prefix[hi] - prefix[lo]) / (hi - lo) as f64) as f32
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    fn sine(freq: f32, rate: u32, seconds: f32, amp: f32) -> Vec<f32> {
        let n = (rate as f32 * seconds) as usize;
        (0..n)
            .map(|i| amp * (2.0 * PI * freq * i as f32 / rate as f32).sin())
            .collect()
    }

    /// Estimates the dominant frequency by counting zero crossings.
    fn zero_crossing_freq(samples: &[f32], rate: u32) -> f32 {
        let crossings = samples
            .windows(2)
            .filter(|w| (w[0] < 0.0) != (w[1] < 0.0))
            .count();
        crossings as f32 / 2.0 / (samples.len() as f32 / rate as f32)
    }

    #[test]
    fn rms_of_full_scale_sine_is_one_over_sqrt_two() {
        let s = sine(440.0, 16_000, 1.0, 1.0);
        assert!((rms(&s) - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-3);
        assert!(rms(&[]).abs() < f32::EPSILON);
    }

    #[test]
    fn meter_maps_decibels_to_unit_range() {
        assert!(meter_level(0.0).abs() < f32::EPSILON);
        assert!((meter_level(1.0) - 1.0).abs() < 1e-6);
        assert!((meter_level(0.001) - 0.0).abs() < 1e-6); // -60 dBFS
        assert!((meter_level(0.031_622_8) - 0.5).abs() < 1e-3); // -30 dBFS
    }

    #[test]
    fn resample_is_identity_for_equal_rates() {
        let s = vec![0.1, 0.2, 0.3];
        assert_eq!(resample(&s, 16_000, 16_000), s);
    }

    #[test]
    fn downsampling_48k_to_16k_keeps_duration_and_pitch() {
        let s = sine(440.0, 48_000, 1.0, 0.5);
        let out = resample(&s, 48_000, 16_000);
        assert_eq!(out.len(), 16_000);
        let f = zero_crossing_freq(&out, 16_000);
        assert!((f - 440.0).abs() < 5.0, "got {f} Hz");
    }

    #[test]
    fn downsampling_44_1k_handles_fractional_ratio() {
        let s = sine(300.0, 44_100, 2.0, 0.5);
        let out = resample(&s, 44_100, 16_000);
        assert_eq!(out.len(), 32_000);
        let f = zero_crossing_freq(&out, 16_000);
        assert!((f - 300.0).abs() < 5.0, "got {f} Hz");
    }

    #[test]
    fn upsampling_8k_to_16k_doubles_length() {
        let s = sine(200.0, 8_000, 1.0, 0.5);
        assert_eq!(resample(&s, 8_000, 16_000).len(), 16_000);
    }

    #[test]
    fn anti_alias_filter_attenuates_content_above_new_nyquist() {
        // 7.9 kHz is just below the 8 kHz Nyquist of 16 kHz output; a 20 kHz tone
        // would alias without filtering. It must come out much quieter.
        let alias = sine(20_000.0, 48_000, 0.5, 0.5);
        let out = resample(&alias, 48_000, 16_000);
        assert!(rms(&out) < 0.1, "alias leaked: rms {}", rms(&out));
    }

    #[test]
    fn capture_downmixes_stereo_and_reports_chunk_level() {
        let mut buf = CaptureBuffer::new(16_000, 2);
        let level = buf.push_interleaved(&[1.0, 0.0, 1.0, 0.0]);
        assert!((level - 0.5).abs() < 1e-6);
        assert!((buf.duration_seconds() - 2.0 / 16_000.0).abs() < 1e-9);
    }

    #[test]
    fn capture_rejects_taps_and_silence() {
        let mut short = CaptureBuffer::new(16_000, 1);
        short.push_interleaved(&sine(440.0, 16_000, 0.1, 0.5));
        assert_eq!(short.finish(), Err(Rejected::TooShort));

        let mut silent = CaptureBuffer::new(16_000, 1);
        silent.push_interleaved(&vec![0.0; 16_000]);
        assert_eq!(silent.finish(), Err(Rejected::Silent));
    }

    #[test]
    fn capture_produces_model_rate_audio() {
        let mut buf = CaptureBuffer::new(48_000, 1);
        buf.push_interleaved(&sine(440.0, 48_000, 1.0, 0.3));
        assert_eq!(buf.finish().unwrap().len(), 16_000);
    }

    #[test]
    fn capture_is_capped_to_bound_memory() {
        let mut buf = CaptureBuffer::new(100, 1);
        let too_much = vec![0.1; (100 * MAX_RECORDING_SECONDS + 500) as usize];
        buf.push_interleaved(&too_much);
        assert!((buf.duration_seconds() - MAX_RECORDING_SECONDS as f32).abs() < 1e-3);
    }
}
