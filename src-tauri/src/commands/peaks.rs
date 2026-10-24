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
    r.seek(start as f64 / rate).map_err(|e| e.to_string())?;
    let want = count as usize * channels;
    let mut samples = Vec::with_capacity(want);
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
