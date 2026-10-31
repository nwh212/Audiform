// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Converting one audio file to another format: any file the app opens,
//! to WAV, FLAC, MP3 or M4A. Used by Batch Convert and by Save As in the
//! Waveform editor. Batch Convert can run a Voice Chain on each file on
//! the way through.
//!
//! The new file is written next to its destination under a temporary
//! name and moved into place only when it is complete, so a failed or
//! stopped conversion never leaves a half-written file, and converting a
//! file onto itself is safe.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use super::encode::{EncodeError, OutputFormat, Writer};
use crate::codecs::{probe, AudioReader};
use crate::dsp::effects::ChainProcessor;
use crate::session::model::EffectChain;

/// A Voice Chain run over a whole file. Mono files are processed as
/// stereo and folded back; surround files run on all their speakers
/// (see dsp/effects/wide.rs). The chain's look-ahead delay is trimmed from
/// the start and flushed at the end, so the output lines up exactly.
struct FileFx {
    chain: ChainProcessor,
    ch: usize,
    stereo: Vec<f32>,
    /// Frames still to drop from the front (the chain's delay).
    skip: usize,
    latency: usize,
}

impl FileFx {
    fn new(chain: &EffectChain, rate: u32, ch: usize) -> Result<Option<Self>, String> {
        let c = if ch > 2 {
            ChainProcessor::with_channels(chain, rate, &crate::dsp::effects::wide::names_for(ch))
        } else {
            ChainProcessor::new(chain, rate)
        };
        let Some(c) = c else {
            return Ok(None);
        };
        let latency = c.latency();
        Ok(Some(Self {
            chain: c,
            ch,
            stereo: Vec::new(),
            skip: latency,
            latency,
        }))
    }

    /// Processes `buf` (interleaved at the file's channel count) in place;
    /// it may come back shorter while the delay is trimmed.
    fn run(&mut self, buf: &mut Vec<f32>) {
        if self.ch > 2 {
            self.chain.process(buf);
            let drop = self.skip.min(buf.len() / self.ch);
            self.skip -= drop;
            buf.drain(..drop * self.ch);
            return;
        }
        self.stereo.clear();
        if self.ch == 1 {
            self.stereo.extend(buf.iter().flat_map(|v| [*v, *v]));
        } else {
            self.stereo.extend_from_slice(buf);
        }
        self.chain.process(&mut self.stereo);
        let frames = self.stereo.len() / 2;
        let drop = self.skip.min(frames);
        self.skip -= drop;
        buf.clear();
        let kept = &self.stereo[drop * 2..];
        if self.ch == 1 {
            buf.extend(kept.as_chunks::<2>().0.iter().map(|f| (f[0] + f[1]) * 0.5));
        } else {
            buf.extend_from_slice(kept);
        }
    }

    /// The last `latency` frames, still inside the chain.
    fn flush(&mut self) -> Vec<f32> {
        let mut tail = vec![0.0; self.latency * self.ch];
        self.run(&mut tail);
        tail
    }
}

/// Level work done on each file after its effects: peak normalize and
/// fades. All off by default.
#[derive(Debug, Clone, Default, PartialEq, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Shaping {
    /// Normalize so the loudest sample peaks here, in dBFS (-30 to 0).
    pub normalize_db: Option<f64>,
    /// Fade in and fade out at the ends of the file, in milliseconds.
    pub fade_in_ms: f64,
    pub fade_out_ms: f64,
}

impl Shaping {
    fn is_off(&self) -> bool {
        self.normalize_db.is_none() && self.fade_in_ms <= 0.0 && self.fade_out_ms <= 0.0
    }
}

/// Gain for output frame `at` of `total`: fade in, fade out (both a
/// smooth S curve) and the normalize gain.
fn shape_gain(at: u64, total: u64, fade_in: u64, fade_out: u64, gain: f32) -> f32 {
    let curve = |x: f64| {
        let x = x.clamp(0.0, 1.0);
        (x * std::f64::consts::FRAC_PI_2).sin().powi(2)
    };
    let mut g = 1.0;
    if fade_in > 0 && at < fade_in {
        g *= curve(at as f64 / fade_in as f64);
    }
    let left = total.saturating_sub(at + 1);
    if fade_out > 0 && left < fade_out {
        g *= curve(left as f64 / fade_out as f64);
    }
    gain * g as f32
}

#[derive(Debug, Clone, PartialEq)]
pub enum Converted {
    Done { frames: u64 },
    Cancelled,
}

/// The temporary name a conversion writes to before moving into place.
fn partial_path(dest: &Path) -> PathBuf {
    let name = dest
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    dest.with_file_name(format!(".{name}.partial"))
}

/// Converts `src` to `dest`. `progress` gets (done, total) source frames.
/// Refuses to replace an existing `dest` unless `overwrite`.
pub fn convert_file(
    src: &Path,
    dest: &Path,
    fmt: &OutputFormat,
    overwrite: bool,
    cancel: &AtomicBool,
    progress: impl FnMut(u64, u64),
) -> Result<Converted, String> {
    convert_with(
        src,
        dest,
        fmt,
        overwrite,
        None,
        &Shaping::default(),
        cancel,
        progress,
    )
}

/// Peak and length of the file after `process`, for normalize and the
/// fade out. None when cancelled.
fn measure(
    src: &Path,
    channels: usize,
    rate: u32,
    process: Option<&EffectChain>,
    cancel: &AtomicBool,
    mut progress: impl FnMut(u64),
) -> Result<Option<(f32, u64)>, String> {
    let mut reader = AudioReader::open(src, channels, None).map_err(|e| e.to_string())?;
    let mut fx = match process {
        Some(c) => FileFx::new(c, rate, channels)?,
        None => None,
    };
    let mut buf = Vec::new();
    let (mut peak, mut frames, mut read) = (0f32, 0u64, 0u64);
    let mut take = |b: &[f32]| {
        peak = b.iter().fold(peak, |m, v| m.max(v.abs()));
        frames += (b.len() / channels) as u64;
    };
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Ok(None);
        }
        buf.clear();
        if !reader.read_into(&mut buf).map_err(|e| e.to_string())? {
            break;
        }
        read += (buf.len() / channels) as u64;
        if let Some(f) = fx.as_mut() {
            f.run(&mut buf);
        }
        take(&buf);
        progress(read);
    }
    if let Some(f) = fx.as_mut() {
        take(&f.flush());
    }
    Ok(Some((peak, frames)))
}

/// [`convert_file`], running `process` on the audio first when given,
/// then `shape` (normalize and fades). Normalize and fade out need the
/// whole file first, so those read it twice.
#[allow(clippy::too_many_arguments)]
pub fn convert_with(
    src: &Path,
    dest: &Path,
    fmt: &OutputFormat,
    overwrite: bool,
    process: Option<&EffectChain>,
    shape: &Shaping,
    cancel: &AtomicBool,
    mut progress: impl FnMut(u64, u64),
) -> Result<Converted, String> {
    if dest.exists() && !overwrite {
        return Err("A file with that name is already there.".into());
    }
    let info = probe(src).map_err(|e| e.to_string())?;
    let bits = info.bit_depth.unwrap_or(24).min(32) as u16;
    let resolved = fmt
        .resolve(info.sample_rate, info.channels, bits)
        .map_err(|e| e.to_string())?;
    let mut reader =
        AudioReader::open(src, usize::from(info.channels), None).map_err(|e| e.to_string())?;
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let ch = usize::from(info.channels.max(1));
    let rate = f64::from(info.sample_rate);
    // A first pass when the level or the exact length is needed.
    let two_pass = shape.normalize_db.is_some() || shape.fade_out_ms > 0.0;
    let total = info.length;
    let (gain, frames_out) = if two_pass {
        match measure(src, ch, info.sample_rate, process, cancel, |d| {
            progress(d.min(total) / 2, total)
        })? {
            None => return Ok(Converted::Cancelled),
            Some((peak, frames)) => {
                let gain = match shape.normalize_db {
                    Some(db) if peak > 1e-9 => {
                        (10f64.powf(db.clamp(-30.0, 0.0) / 20.0) / f64::from(peak)) as f32
                    }
                    _ => 1.0,
                };
                (gain, frames)
            }
        }
    } else {
        (1.0, total)
    };
    let fade_in = (shape.fade_in_ms.max(0.0) / 1000.0 * rate) as u64;
    let fade_out = (shape.fade_out_ms.max(0.0) / 1000.0 * rate) as u64;
    let shaping = !shape.is_off();
    let tmp = partial_path(dest);
    let result = (|| -> Result<Converted, String> {
        let enc = |e: EncodeError| e.to_string();
        let mut w =
            Writer::create(&tmp, &resolved, info.sample_rate, info.channels).map_err(enc)?;
        let mut buf = Vec::new();
        let mut done = 0u64;
        let mut out_at = 0u64;
        let mut shape_block = |b: &mut [f32]| {
            if !shaping {
                return;
            }
            for f in b.chunks_mut(ch) {
                let g = shape_gain(out_at, frames_out, fade_in, fade_out, gain);
                for v in f {
                    *v *= g;
                }
                out_at += 1;
            }
        };
        let mut fx = match process {
            Some(c) => FileFx::new(c, info.sample_rate, ch)?,
            None => None,
        };
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Ok(Converted::Cancelled);
            }
            buf.clear();
            if !reader.read_into(&mut buf).map_err(|e| e.to_string())? {
                break;
            }
            let read = (buf.len() / ch) as u64;
            if let Some(f) = fx.as_mut() {
                f.run(&mut buf);
            }
            shape_block(&mut buf);
            w.write(&buf).map_err(enc)?;
            done += read;
            if two_pass {
                progress(total / 2 + done.min(total) / 2, total);
            } else {
                progress(done.min(total), total);
            }
        }
        if let Some(f) = fx.as_mut() {
            let mut tail = f.flush();
            shape_block(&mut tail);
            w.write(&tail).map_err(enc)?;
        }
        let frames = w.finish().map_err(enc)?;
        Ok(Converted::Done { frames })
    })();
    match result {
        Ok(Converted::Done { frames }) => {
            if overwrite && dest.exists() {
                // Windows won't rename over an existing file.
                std::fs::remove_file(dest).map_err(|e| e.to_string())?;
            }
            std::fs::rename(&tmp, dest).map_err(|e| e.to_string())?;
            Ok(Converted::Done { frames })
        }
        other => {
            let _ = std::fs::remove_file(&tmp);
            other
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::encode::FormatKind;

    fn tone(path: &Path, frames: u32) {
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 48_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(path, spec).unwrap();
        for i in 0..frames {
            let v = ((i as f32 * 0.03).sin() * 12_000.0) as i16;
            w.write_sample(v).unwrap();
            w.write_sample(v).unwrap();
        }
        w.finalize().unwrap();
    }

    #[test]
    fn runs_a_voice_chain_and_keeps_length_and_timing() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("in.wav");
        tone(&src, 30_000);
        let dest = dir.path().join("out.wav");
        let mut chain = EffectChain::default();
        // A limiter with look-ahead (delay) plus a -6 dB gain.
        for (effect, params) in [
            (
                "altaudio.limiter",
                vec![("ceiling", 0.0), ("lookahead", 5.0)],
            ),
            ("altaudio.gain", vec![("gain", -6.0206)]),
        ] {
            chain.slots.push(crate::session::model::EffectSlot {
                id: crate::session::model::new_id(),
                effect: effect.into(),
                enabled: true,
                params: params
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v))
                    .collect(),
                preset: None,
                sidechain: None,
                plugin_state: None,
                data: None,
                position: None,
                routing: None,
            });
        }
        let fmt = OutputFormat {
            kind: FormatKind::Wav,
            bit_depth: Some(32),
            ..Default::default()
        };
        convert_with(
            &src,
            &dest,
            &fmt,
            false,
            Some(&chain),
            &Shaping::default(),
            &AtomicBool::new(false),
            |_, _| {},
        )
        .unwrap();
        let a: Vec<i16> = hound::WavReader::open(&src)
            .unwrap()
            .samples()
            .map(|s| s.unwrap())
            .collect();
        let b: Vec<f32> = hound::WavReader::open(&dest)
            .unwrap()
            .samples()
            .map(|s| s.unwrap())
            .collect();
        assert_eq!(a.len(), b.len(), "same length");
        for i in [1_000usize, 12_345, 59_000] {
            let want = f32::from(a[i]) / 32_768.0 * 0.5;
            assert!((b[i] - want).abs() < 2e-3, "sample {i}: {} vs {want}", b[i]);
        }
    }

    #[test]
    fn normalizes_and_fades_after_the_effects() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("in.wav");
        tone(&src, 48_000); // peaks near 12,000 / 32,768
        let dest = dir.path().join("out.wav");
        let fmt = OutputFormat {
            kind: FormatKind::Wav,
            bit_depth: Some(32),
            ..Default::default()
        };
        let shape = Shaping {
            normalize_db: Some(-1.0),
            fade_in_ms: 100.0,
            fade_out_ms: 250.0,
        };
        let mut last = (0, 0);
        convert_with(
            &src,
            &dest,
            &fmt,
            false,
            None,
            &shape,
            &AtomicBool::new(false),
            |d, t| last = (d, t),
        )
        .unwrap();
        assert_eq!(
            last,
            (48_000, 48_000),
            "progress runs once across both passes"
        );
        let b: Vec<f32> = hound::WavReader::open(&dest)
            .unwrap()
            .samples()
            .map(|s| s.unwrap())
            .collect();
        assert_eq!(b.len(), 96_000);
        let peak = b.iter().fold(0f32, |m, v| m.max(v.abs()));
        assert!((20.0 * peak.log10() + 1.0).abs() < 0.05, "peak {peak}");
        assert_eq!(b[0], 0.0, "fade in starts silent");
        assert!(
            b[(96_000 - 2)..].iter().all(|v| v.abs() < 1e-3),
            "fade out ends silent"
        );
        // Past the fades the level is untouched apart from the gain.
        let mid = b[24_000 * 2..26_000 * 2]
            .iter()
            .fold(0f32, |m, v| m.max(v.abs()));
        assert!((mid - peak).abs() < 0.02, "{mid} vs {peak}");
        assert!((shape_gain(0, 100, 10, 0, 1.0)).abs() < 1e-6);
        assert!((shape_gain(50, 100, 10, 10, 2.0) - 2.0).abs() < 1e-6);
    }

    #[test]
    fn converts_and_protects_existing_files() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("in.wav");
        tone(&src, 48_000);
        let dest = dir.path().join("out").join("in.flac");
        let fmt = OutputFormat {
            kind: FormatKind::Flac,
            ..Default::default()
        };
        let mut last = (0, 0);
        let r = convert_file(&src, &dest, &fmt, false, &AtomicBool::new(false), |d, t| {
            last = (d, t)
        })
        .unwrap();
        assert_eq!(r, Converted::Done { frames: 48_000 });
        assert_eq!(last, (48_000, 48_000));
        let info = probe(&dest).unwrap();
        assert_eq!(
            (info.length, info.channels, info.bit_depth),
            (48_000, 2, Some(16))
        );
        assert!(
            convert_file(&src, &dest, &fmt, false, &AtomicBool::new(false), |_, _| {}).is_err()
        );
        assert!(!partial_path(&dest).exists());
    }

    #[test]
    fn converts_a_file_onto_itself_and_stops_cleanly() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("same.wav");
        tone(&src, 24_000);
        let fmt = OutputFormat {
            kind: FormatKind::Wav,
            bit_depth: Some(24),
            channels: Some(1),
            ..Default::default()
        };
        convert_file(&src, &src, &fmt, true, &AtomicBool::new(false), |_, _| {}).unwrap();
        let r = hound::WavReader::open(&src).unwrap();
        assert_eq!(
            (r.spec().channels, r.spec().bits_per_sample, r.len()),
            (1, 24, 24_000)
        );

        let dest = dir.path().join("never.mp3");
        let r = convert_file(
            &src,
            &dest,
            &OutputFormat {
                kind: FormatKind::Mp3,
                ..Default::default()
            },
            false,
            &AtomicBool::new(true),
            |_, _| {},
        )
        .unwrap();
        assert_eq!(r, Converted::Cancelled);
        assert!(!dest.exists() && !partial_path(&dest).exists());
    }
}
