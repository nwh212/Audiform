// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

//! The Convolution Reverb's window: picking an impulse response file and
//! describing it (length, channels, and its shape over time for drawing).
//! The interface saves the new slot data as one undoable edit.

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::async_runtime::spawn_blocking;
use tauri::{AppHandle, Manager, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use crate::dsp::effects::reverb::{forget, read, ReverbData};

/// Shows an Open dialog for an impulse response.
#[tauri::command]
pub async fn reverb_pick(app: AppHandle) -> Result<Option<PathBuf>, String> {
    let parent: Option<WebviewWindow> = app.get_webview_window("main");
    spawn_blocking(move || {
        let mut d = app
            .dialog()
            .file()
            .set_title("Load Impulse Response")
            .add_filter(
                "Impulse responses (audio files)",
                &[
                    "wav", "aif", "aiff", "flac", "mp3", "ogg", "opus", "m4a", "aac",
                ],
            );
        if let Some(w) = parent.as_ref() {
            d = d.set_parent(w);
        }
        d.blocking_pick_file()
            .and_then(|f| f.as_path().map(Path::to_path_buf))
    })
    .await
    .map_err(|e| e.to_string())
}

/// A response file, described for the window.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReverbIr {
    /// The slot data that names it.
    pub data: String,
    pub name: String,
    pub path: PathBuf,
    pub seconds: f64,
    pub rate: u32,
    pub channels: usize,
    /// Its level over time, 0 to 1 per point (the loudest channel), for drawing.
    pub shape: Vec<f32>,
}

const SHAPE_POINTS: usize = 240;
const SHAPE_RATE: u32 = 8_000;

/// Reads a response file (checking it plays) and describes it.
#[tauri::command]
pub async fn reverb_load(path: PathBuf) -> Result<ReverbIr, String> {
    spawn_blocking(move || {
        forget(&path);
        let r = read(&path, SHAPE_RATE)?;
        let frames = r.channels.first().map_or(0, Vec::len);
        let peak = r
            .channels
            .iter()
            .flat_map(|c| c.iter())
            .fold(0f32, |m, v| m.max(v.abs()))
            .max(1e-9);
        let shape = (0..SHAPE_POINTS)
            .map(|k| {
                let (a, b) = (
                    k * frames / SHAPE_POINTS,
                    ((k + 1) * frames / SHAPE_POINTS).max(k * frames / SHAPE_POINTS + 1),
                );
                let m = r
                    .channels
                    .iter()
                    .flat_map(|c| c[a.min(c.len())..b.min(c.len())].iter())
                    .fold(0f32, |m, v| m.max(v.abs()));
                // On a dB scale down to -60, so the tail shows.
                ((20.0 * (m / peak).max(1e-6).log10() + 60.0) / 60.0).clamp(0.0, 1.0)
            })
            .collect();
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let data = serde_json::to_string(&ReverbData {
            path: path.clone(),
            name: name.clone(),
        })
        .map_err(|e| e.to_string())?;
        Ok(ReverbIr {
            data,
            name,
            path,
            seconds: frames as f64 / f64::from(SHAPE_RATE),
            rate: r.file_rate,
            channels: r.file_channels,
            shape,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}
