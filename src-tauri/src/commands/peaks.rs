// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

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
    let channels = probe.source_channels().clamp(1, 64);
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
/// correlation, then phase, level, pitch and coherence for each band with sound.
/// The Phase Wheel while stopped: the same picture for the `size` frames
/// of the session (or the Waveform editor's file, with `wave`) that end
/// at `position`, so it follows the playhead as it is moved.
#[tauri::command]
pub async fn scope_phase_at(
    open: tauri::State<'_, super::session::OpenSession>,
    open_wave: tauri::State<'_, super::wave::OpenWave>,
    position: u64,
    size: usize,
    a: crate::engine::scope::Signal,
    b: crate::engine::scope::Signal,
    wave: bool,
) -> Result<Response, String> {
    use crate::engine::scope;
    let session = if wave {
        let w = open_wave.0.lock().map_err(|e| e.to_string())?;
        match w.as_ref() {
            Some(w) => w.session.clone(),
            None => {
                return Ok(phase_bytes(&scope::Phase {
                    correlation: 0.0,
                    bins: Vec::new(),
                }))
            }
        }
    } else {
        open.0.lock().map_err(|e| e.to_string())?.session.clone()
    };
    let n = window(size);
    let rate = session.sample_rate.max(1);
    let p = spawn_blocking(move || scope::analyse(&mix_before(&session, position, n), a, b, rate))
        .await
        .map_err(|e| e.to_string())?;
    Ok(phase_bytes(&p))
}

/// The `n` frames of `session`'s mix that end at `position`, after the master fader.
fn mix_before(session: &crate::session::model::Session, position: u64, n: usize) -> Vec<[f32; 2]> {
    use crate::engine::{graph, plan, render::Renderer};
    let rate = session.sample_rate.max(1);
    let start = position.saturating_sub(n as u64);
    let master = plan::master_gain(session);
    let session = &*crate::engine::speaker_feed::expanded(session);
    let mut mix = graph::Mix::new(
        Renderer::new(plan::build(session, rate), start),
        graph::Graph::new(session, rate, true),
    );
    let mut out = vec![Vec::new(); mix.outputs()];
    let mut frames: Vec<[f32; 2]> = Vec::with_capacity(n);
    while frames.len() < n {
        let got = mix.render((n - frames.len()).min(4_096), &mut out);
        if got == 0 {
            break;
        }
        frames.extend(
            out[0][..got * 2]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|f| [f[0] * master, f[1] * master]),
        );
    }
    frames
}

/// The spectrum of what is playing, for drawing behind an EQ's curve:
/// `points` levels in dBFS from 20 Hz to 20 kHz, even in pitch. While
/// stopped (`at` set), the audio just before that spot of the session, or
/// of the Waveform editor's file with `wave`.
#[tauri::command]
pub async fn scope_spectrum(
    open: tauri::State<'_, super::session::OpenSession>,
    open_wave: tauri::State<'_, super::wave::OpenWave>,
    size: usize,
    points: usize,
    at: Option<u64>,
    wave: bool,
) -> Result<Response, String> {
    use crate::engine::scope;
    let n = window(size);
    let levels = match at {
        None => scope::spectrum(&scope::latest(n), scope::rate(), points),
        Some(position) => {
            let session = if wave {
                let w = open_wave.0.lock().map_err(|e| e.to_string())?;
                match w.as_ref() {
                    Some(w) => w.session.clone(),
                    None => return Ok(Response::new(Vec::new())),
                }
            } else {
                open.0.lock().map_err(|e| e.to_string())?.session.clone()
            };
            let rate = session.sample_rate.max(1);
            spawn_blocking(move || {
                scope::spectrum(&mix_before(&session, position, n), rate, points)
            })
            .await
            .map_err(|e| e.to_string())?
        }
    };
    Ok(Response::new(
        levels
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect::<Vec<u8>>(),
    ))
}

fn window(size: usize) -> usize {
    use crate::engine::scope::SIZES;
    SIZES
        .iter()
        .copied()
        .find(|s| *s >= size)
        .unwrap_or(SIZES[SIZES.len() - 1])
}

fn phase_bytes(p: &crate::engine::scope::Phase) -> Response {
    let mut out = Vec::with_capacity(4 + p.bins.len() * 16);
    out.extend_from_slice(&p.correlation.to_le_bytes());
    for bin in &p.bins {
        out.extend_from_slice(&bin.phase.to_le_bytes());
        out.extend_from_slice(&bin.level.to_le_bytes());
        out.extend_from_slice(&bin.pitch.to_le_bytes());
        out.extend_from_slice(&bin.coherence.to_le_bytes());
    }
    Response::new(out)
}

/// Async with the work on a blocking thread: a plain (sync) command runs on
/// the main thread, which is also the window's message thread, so up to 120
/// analyses a second there would stall every repaint and click.
#[tauri::command]
pub async fn scope_phase(
    size: usize,
    a: crate::engine::scope::Signal,
    b: crate::engine::scope::Signal,
) -> Result<Response, String> {
    use crate::engine::scope;
    let p = spawn_blocking(move || scope::analyse(&scope::latest(window(size)), a, b, scope::rate()))
        .await
        .map_err(|e| e.to_string())?;
    Ok(phase_bytes(&p))
}
