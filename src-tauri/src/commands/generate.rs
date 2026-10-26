// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Effects > Generate: writes a tone or noise as a new WAV file that the
//! interface then places on a track. A new file keeps the multitrack
//! non-destructive: nothing existing is touched.

use std::path::PathBuf;

use serde::Deserialize;
use tauri::async_runtime::spawn_blocking;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GenerateKind {
    /// A sine wave at `frequency`.
    Tone,
    /// Even noise across the spectrum.
    White,
    /// Noise that falls off toward the highs, softer to the ear.
    Pink,
    /// Digital silence.
    Silence,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateRequest {
    pub kind: GenerateKind,
    pub seconds: f64,
    /// Hz, for tones.
    pub frequency: f64,
    /// Peak level for tones, average level for noise, dBFS.
    pub level_db: f64,
    pub sample_rate: u32,
    pub channels: u16,
}

/// The samples, interleaved, as floats.
pub fn samples(r: &GenerateRequest) -> Vec<f32> {
    let rate = r.sample_rate.clamp(8_000, 192_000);
    let ch = usize::from(r.channels.clamp(1, 2));
    let frames = (r.seconds.clamp(0.01, 3_600.0) * f64::from(rate)).round() as usize;
    let gain = 10f64.powf(r.level_db.clamp(-96.0, 0.0) / 20.0) as f32;
    let mut out = Vec::with_capacity(frames * ch);
    // Repeatable noise: a small xorshift is plenty for test signals.
    let mut state: u32 = 0x9e37_79b9;
    let mut white = || {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        (state as f32 / u32::MAX as f32) * 2.0 - 1.0
    };
    // Pink by the Paul Kellet economy filter, scaled to about the same loudness as white.
    let (mut b0, mut b1, mut b2) = (0f32, 0f32, 0f32);
    let step =
        std::f64::consts::TAU * r.frequency.clamp(1.0, f64::from(rate) / 2.0) / f64::from(rate);
    // Short fades so a tone does not click on and off.
    let ramp = (f64::from(rate) * 0.005) as usize;
    for i in 0..frames {
        let edge = if ramp == 0 {
            1.0
        } else {
            (i.min(frames - 1 - i) as f32 / ramp as f32).min(1.0)
        };
        let v = match r.kind {
            GenerateKind::Silence => 0.0,
            GenerateKind::Tone => (step * i as f64).sin() as f32 * gain * edge,
            // Uniform noise has an RMS of 1/sqrt(3); bring it to the asked level.
            GenerateKind::White => white() * gain * 1.732 * edge,
            GenerateKind::Pink => {
                let w = white();
                b0 = 0.99765 * b0 + w * 0.099_046;
                b1 = 0.96300 * b1 + w * 0.296_516_4;
                b2 = 0.57000 * b2 + w * 1.052_691_3;
                let p = (b0 + b1 + b2 + w * 0.1848) * 0.25;
                (p * gain * 1.732 * edge).clamp(-1.0, 1.0)
            }
        };
        for _ in 0..ch {
            out.push(v);
        }
    }
    out
}

/// Writes the signal to `path` as a 32-bit float WAV. Returns the path.
#[tauri::command]
pub async fn generate_audio(path: PathBuf, request: GenerateRequest) -> Result<PathBuf, String> {
    spawn_blocking(move || {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("Could not make the folder: {e}"))?;
        }
        let spec = hound::WavSpec {
            channels: request.channels.clamp(1, 2),
            sample_rate: request.sample_rate.clamp(8_000, 192_000),
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut w = hound::WavWriter::create(&path, spec)
            .map_err(|e| format!("Could not write the file: {e}"))?;
        for v in samples(&request) {
            w.write_sample(v)
                .map_err(|e| format!("Could not write the file: {e}"))?;
        }
        w.finalize()
            .map_err(|e| format!("Could not write the file: {e}"))?;
        Ok(path)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(kind: GenerateKind, level_db: f64) -> GenerateRequest {
        GenerateRequest {
            kind,
            seconds: 1.0,
            frequency: 1_000.0,
            level_db,
            sample_rate: 48_000,
            channels: 2,
        }
    }

    fn rms(v: &[f32]) -> f32 {
        (v.iter().map(|x| x * x).sum::<f32>() / v.len() as f32).sqrt()
    }

    #[test]
    fn tone_hits_its_peak_and_length() {
        let s = samples(&req(GenerateKind::Tone, -6.0));
        assert_eq!(s.len(), 96_000);
        let peak = s.iter().fold(0f32, |m, v| m.max(v.abs()));
        assert!((peak - 0.501).abs() < 0.01, "peak {peak}");
        assert_eq!(s[0], 0.0, "starts from silence, no click");
    }

    #[test]
    fn noise_sits_near_its_level() {
        for kind in [GenerateKind::White, GenerateKind::Pink] {
            let s = samples(&req(kind, -20.0));
            let db = 20.0 * rms(&s).log10();
            assert!((db + 20.0).abs() < 3.0, "{kind:?}: {db} dB");
        }
        assert!(samples(&req(GenerateKind::Silence, -6.0))
            .iter()
            .all(|v| *v == 0.0));
    }
}
