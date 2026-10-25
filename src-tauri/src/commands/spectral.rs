// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

//! Spectral view and spectral edits on the Waveform editor's file.
//! Both render the edit list first, so they see exactly what plays.

use std::sync::atomic::AtomicBool;

use serde::Serialize;
use tauri::async_runtime::spawn_blocking;
use tauri::ipc::Response;
use tauri::{AppHandle, State};

use super::wave::OpenWave;
use crate::dsp::spectral::{self, Action, Mask, FFT_SIZE};
use crate::export::mixdown::{self, MixdownRequest};
use crate::session::model::{new_id, ChannelLayout, MediaRef, Session};

/// Longest stretch the view draws at once. Zoom in for longer files.
pub const MAX_VIEW_SECONDS: u64 = 20 * 60;

/// Renders `from..to` (timeline samples, `from` may be negative) as
/// stereo interleaved, with silence where there is no audio.
pub(crate) fn render(session: &Session, from: i64, to: i64) -> Vec<f32> {
    let len = (to - from).max(0) as usize;
    let mut out = vec![0f32; len * 2];
    let a = from.max(0) as u64;
    let b = to.max(0) as u64;
    if b <= a {
        return out;
    }
    let req = MixdownRequest {
        range: Some((a, b)),
        clip_only: true,
        ..Default::default()
    };
    let mut at = ((a as i64 - from) as usize) * 2;
    let _ = mixdown::render_mix(
        session,
        &req,
        &AtomicBool::new(false),
        |_, _| {},
        |block| {
            let end = (at + block.len()).min(out.len());
            out[at..end].copy_from_slice(&block[..end - at]);
            at = end;
            Ok(())
        },
    );
    out
}

/// The open file's edit list, without the Voice Chain being previewed
/// on it: edits see the file as it is, not as the preview sounds.
pub(crate) fn snapshot(wave: &State<'_, OpenWave>) -> Result<Session, String> {
    let w = wave.0.lock().map_err(|e| e.to_string())?;
    Ok(super::wave::without_effects(
        w.as_ref()
            .ok_or("No file is open in the Waveform editor.")?
            .session
            .clone(),
    ))
}

/// Writes `frames` of stereo `audio` from frame `skip` to a new float
/// WAV in `dir` (mono files fold back to one channel). The media ref
/// for the replace op.
pub(crate) fn write_stretch(
    dir: &std::path::Path,
    name: &str,
    audio: &[f32],
    skip: usize,
    frames: usize,
    rate: u32,
    mono: bool,
) -> Result<MediaRef, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let path = mixdown::unique_path(dir, name, "wav");
    let channels: u16 = if mono { 1 } else { 2 };
    let spec = hound::WavSpec {
        channels,
        sample_rate: rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut w = hound::WavWriter::create(&path, spec).map_err(|e| e.to_string())?;
    let all = audio.as_chunks::<2>().0;
    let end = (skip + frames).min(all.len());
    for f in &all[skip.min(end)..end] {
        if mono {
            w.write_sample((f[0] + f[1]) * 0.5)
                .map_err(|e| e.to_string())?;
        } else {
            w.write_sample(f[0]).map_err(|e| e.to_string())?;
            w.write_sample(f[1]).map_err(|e| e.to_string())?;
        }
    }
    w.finalize().map_err(|e| e.to_string())?;
    Ok(MediaRef {
        id: new_id(),
        path,
        relative_path: None,
        sample_rate: rate,
        channels,
        length: (end - skip.min(end)) as u64,
    })
}

/// True when the open file is mono.
pub(crate) fn is_mono(session: &Session) -> bool {
    session
        .tracks
        .first()
        .is_some_and(|t| t.channels == ChannelLayout::Mono)
        || session.media.first().is_some_and(|m| m.channels == 1)
}

/// The spectrogram of `start..end`: `rows` rows of `columns` bytes, top
/// row the highest frequency, each 0 (silent) to 255 (full scale).
#[tauri::command]
pub async fn wave_spectrogram(
    wave: State<'_, OpenWave>,
    start: u64,
    end: u64,
    columns: u32,
    rows: u32,
    hz: Option<Vec<f32>>,
    fft: Option<u32>,
) -> Result<Response, String> {
    let session = snapshot(&wave)?;
    let rate = session.sample_rate;
    if end.saturating_sub(start) > MAX_VIEW_SECONDS * u64::from(rate) {
        return Err("Zoom in to see the spectrum.".into());
    }
    let columns = columns.clamp(1, 4096) as usize;
    let rows = rows.clamp(1, 1024) as usize;
    let n = fft.map_or(FFT_SIZE, |f| {
        (f as usize).clamp(256, 32_768).next_power_of_two()
    });
    // The view's own axis (a zoomed range, a linear or log scale), or the edits' log axis.
    let nyquist = rate as f32 / 2.0;
    let hz: Vec<f32> = match hz {
        Some(h) if h.len() >= 2 && h.len() <= 1025 => {
            h.into_iter().map(|v| v.clamp(0.0, nyquist)).collect()
        }
        _ => (0..=rows)
            .map(|r| spectral::freq_at(1.0 - r as f32 / rows as f32, nyquist))
            .collect(),
    };
    let bytes = spawn_blocking(move || {
        let pad = n as i64;
        let from = start as i64 - pad;
        let stereo = render(&session, from, end as i64 + pad);
        let mono: Vec<f32> = stereo
            .as_chunks::<2>()
            .0
            .iter()
            .map(|f| (f[0] + f[1]) * 0.5)
            .collect();
        spectral::spectrogram_rows(&mono, from, rate, start, end, columns, &hz, n)
    })
    .await
    .map_err(|e| e.to_string())?;
    Ok(Response::new(bytes))
}

/// Where a spectral edit's result lives, for the replace op.
#[derive(Debug, Serialize)]
pub struct SpectralResult {
    pub media: MediaRef,
    /// Timeline range the new file replaces.
    pub start: u64,
    pub end: u64,
}

/// What a spectral edit does inside its mask.
#[derive(Debug, Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SpectralMode {
    /// Turn down by `gain_db` (-120 or lower removes).
    Gain,
    /// Fill from the audio around it.
    Repair,
}

/// Applies a spectral edit inside `mask` (painted, drawn or boxed in the
/// view) and writes the changed stretch to a new file in the app's
/// cache folder. The edit list is not touched here: the interface swaps
/// the stretch for the new file as one undoable op.
#[tauri::command]
pub async fn wave_spectral_edit(
    app: AppHandle,
    wave: State<'_, OpenWave>,
    mask: Mask,
    mode: SpectralMode,
    gain_db: Option<f32>,
) -> Result<SpectralResult, String> {
    let session = snapshot(&wave)?;
    let (m0, m1) = mask.span();
    if m1 <= m0 || mask.data.iter().all(|v| *v == 0) {
        return Err("Select an area of the spectrum first.".into());
    }
    let start = m0.max(0.0) as u64;
    let end = m1.max(0.0).ceil() as u64;
    let dir = crate::commands::cache::cache_root(&app)?.join("spectral");
    spawn_blocking(move || -> Result<SpectralResult, String> {
        let rate = session.sample_rate;
        let total = mixdown::session_end(&session);
        let n = FFT_SIZE as u64;
        // The changed stretch: the mask plus one frame each side.
        let a = start.saturating_sub(n);
        let b = (end + n).min(total.max(end));
        // Render one more frame each side so the stretch rebuilds fully.
        let from = a as i64 - n as i64;
        let to = b as i64 + n as i64;
        let stereo = render(&session, from, to);
        let action = match mode {
            SpectralMode::Repair => Action::Repair,
            SpectralMode::Gain => {
                let db = gain_db.unwrap_or(-120.0);
                Action::Gain(if db <= -120.0 {
                    0.0
                } else {
                    10f32.powf(db / 20.0)
                })
            }
        };
        let origin = from as f64;
        let mono = session
            .tracks
            .first()
            .is_some_and(|t| t.channels == ChannelLayout::Mono)
            || session.media.first().is_some_and(|m| m.channels == 1);
        let frames = stereo.len() / 2;
        let mut left: Vec<f32> = stereo.iter().step_by(2).copied().collect();
        let mut right: Vec<f32> = stereo.iter().skip(1).step_by(2).copied().collect();
        if mono {
            for (l, r) in left.iter_mut().zip(&right) {
                *l = (*l + r) * 0.5;
            }
            spectral::process_channel(&mut left, rate, origin, &mask, action);
        } else {
            spectral::process_channel(&mut left, rate, origin, &mask, action);
            spectral::process_channel(&mut right, rate, origin, &mask, action);
        }
        let skip = (a as i64 - from) as usize;
        let keep = (b - a) as usize;
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = mixdown::unique_path(&dir, "spectral-edit", "wav");
        let channels: u16 = if mono { 1 } else { 2 };
        let spec = hound::WavSpec {
            channels,
            sample_rate: rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut w = hound::WavWriter::create(&path, spec).map_err(|e| e.to_string())?;
        for i in skip..(skip + keep).min(frames) {
            w.write_sample(left[i]).map_err(|e| e.to_string())?;
            if !mono {
                w.write_sample(right[i]).map_err(|e| e.to_string())?;
            }
        }
        w.finalize().map_err(|e| e.to_string())?;
        Ok(SpectralResult {
            media: MediaRef {
                id: new_id(),
                path,
                relative_path: None,
                sample_rate: rate,
                channels,
                length: keep as u64,
            },
            start: a,
            end: a + keep as u64,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The pitch line for `start..end`: one value per column, in Hz, 0 where
/// there is no clear pitch.
#[tauri::command]
pub async fn wave_pitch(
    wave: State<'_, OpenWave>,
    start: u64,
    end: u64,
    columns: u32,
) -> Result<Vec<f32>, String> {
    let session = snapshot(&wave)?;
    let rate = session.sample_rate;
    if end.saturating_sub(start) > MAX_VIEW_SECONDS * u64::from(rate) {
        return Err("Zoom in to see the pitch.".into());
    }
    let columns = columns.clamp(1, 2048) as usize;
    spawn_blocking(move || {
        let pad = FFT_SIZE as i64 * 2;
        let from = start as i64 - pad;
        let stereo = render(&session, from, end as i64 + pad);
        let mono: Vec<f32> = stereo
            .as_chunks::<2>()
            .0
            .iter()
            .map(|f| (f[0] + f[1]) * 0.5)
            .collect();
        spectral::pitch_track(&mono, from, rate, start, end, columns)
    })
    .await
    .map_err(|e| e.to_string())
}
