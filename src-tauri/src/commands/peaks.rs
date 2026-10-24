// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

//! Waveform peaks for the Editor. Building runs off the UI thread and
//! reports progress as `peaks-progress` events.

use std::path::PathBuf;

use serde::Serialize;
use tauri::async_runtime::spawn_blocking;
use tauri::ipc::Response;
use tauri::{AppHandle, Emitter};

use crate::peaks::{self, PeakSummary};

fn cache_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(crate::commands::cache::cache_root(app)?.join("peaks"))
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    path: PathBuf,
    progress: f64,
}

/// Makes sure the peak cache for a file exists and returns its summary.
#[tauri::command]
pub async fn peaks_ensure(app: AppHandle, path: PathBuf) -> Result<PeakSummary, String> {
    let dir = cache_dir(&app)?;
    spawn_blocking(move || {
        let p = path.clone();
        peaks::ensure(&dir, &path, |progress| {
            let _ = app.emit(
                "peaks-progress",
                Progress {
                    path: p.clone(),
                    progress,
                },
            );
            true
        })
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// One level of peaks as raw little-endian i16 pairs (min, max per channel).
#[tauri::command]
pub async fn peaks_level(app: AppHandle, path: PathBuf, level: usize) -> Result<Response, String> {
    let dir = cache_dir(&app)?;
    spawn_blocking(move || {
        // A cleared cache (Settings > Media and Cache) is rebuilt here.
        peaks::read_level(&dir, &path, level)
            .or_else(|_| {
                peaks::ensure(&dir, &path, |_| true)
                    .and_then(|_| peaks::read_level(&dir, &path, level))
            })
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
    .map(Response::new)
}

/// The most frames [`media_frames`] reads at once.
const MAX_FRAMES: u64 = 1 << 20;

/// Exact samples of a file, for drawing when zoomed in past what the
/// peak cache holds: `count` frames from frame `start`, at the file's own
/// rate, left and right (mono files have one channel). Little-endian: a
/// u32 channel count, then interleaved f32 samples.
#[tauri::command]
pub async fn media_frames(path: PathBuf, start: u64, count: u64) -> Result<Response, String> {
    spawn_blocking(move || read_frames(&path, start, count.min(MAX_FRAMES)))
        .await
        .map_err(|e| e.to_string())?
        .map(Response::new)
}

fn read_frames(path: &std::path::Path, start: u64, count: u64) -> Result<Vec<u8>, String> {
    let probe = crate::codecs::AudioReader::open(path, 2, None).map_err(|e| e.to_string())?;
    let channels = probe.source_channels().clamp(1, 2);
    drop(probe);
    let mut r =
        crate::codecs::AudioReader::open(path, channels, None).map_err(|e| e.to_string())?;
    let rate = f64::from(r.source_rate().max(1));
    // Some files cannot jump to a spot; read from the start and drop what comes before it.
    let mut skip = 0usize;
    if r.seek(start as f64 / rate).is_err() {
        r = crate::codecs::AudioReader::open(path, channels, None).map_err(|e| e.to_string())?;
        skip = start as usize * channels;
    }
    let want = count as usize * channels;
    let mut samples = Vec::with_capacity(want);
    while skip > 0 {
        let mut block = Vec::new();
        if !r.read_into(&mut block).map_err(|e| e.to_string())? {
            break;
        }
        let drop = block.len().min(skip);
        skip -= drop;
        samples.extend_from_slice(&block[drop..]);
    }
    while samples.len() < want {
        if !r.read_into(&mut samples).map_err(|e| e.to_string())? {
            break;
        }
    }
    samples.truncate(want);
    let mut out = Vec::with_capacity(4 + samples.len() * 4);
    out.extend_from_slice(&(channels as u32).to_le_bytes());
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_exact_frames_from_a_spot() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ramp.wav");
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 44_100,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut w = hound::WavWriter::create(&path, spec).unwrap();
        for i in 0..10_000 {
            w.write_sample(i as f32 / 10_000.0).unwrap();
            w.write_sample(-(i as f32) / 10_000.0).unwrap();
        }
        w.finalize().unwrap();
        let b = read_frames(&path, 4_321, 3).unwrap();
        let f = |k: usize| f32::from_le_bytes(b[4 + k * 4..8 + k * 4].try_into().unwrap());
        assert_eq!(u32::from_le_bytes(b[..4].try_into().unwrap()), 2);
        assert_eq!(b.len(), 4 + 6 * 4);
        assert_eq!((f(0), f(1), f(4)), (0.4321, -0.4321, 0.4323));
        // Past the end: what there is.
        assert_eq!(read_frames(&path, 9_999, 10).unwrap().len(), 4 + 2 * 4);
    }
}

/// The Phase Wheel's picture of what is playing now: the newest `size`
/// frames (one of 1024 to 16384; smaller is lighter), comparing signal
/// `a` against `b` (left, right, mid or side). Little-endian f32s: the
/// correlation, then phase, level and pitch for each bin with sound.
#[tauri::command]
pub fn scope_phase(
    size: usize,
    a: crate::engine::scope::Signal,
    b: crate::engine::scope::Signal,
) -> Response {
    use crate::engine::scope;
    let n = scope::SIZES
        .iter()
        .copied()
        .find(|s| *s >= size)
        .unwrap_or(scope::SIZES[scope::SIZES.len() - 1]);
    let p = scope::analyse(&scope::latest(n), a, b, scope::rate());
    let mut out = Vec::with_capacity(4 + p.bins.len() * 12);
    out.extend_from_slice(&p.correlation.to_le_bytes());
    for bin in p.bins {
        out.extend_from_slice(&bin.phase.to_le_bytes());
        out.extend_from_slice(&bin.level.to_le_bytes());
        out.extend_from_slice(&bin.pitch.to_le_bytes());
    }
    Response::new(out)
}
