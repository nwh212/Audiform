// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Drive Manager commands: locations, folder listings, preview playback.

use std::path::PathBuf;

use tauri::async_runtime::spawn_blocking;
use tauri::{AppHandle, Manager, State};

use crate::browse::{self, Entry, Location, LocationKind};
use crate::engine::{PreviewPlayer, PreviewStatus};

/// Drives first, then the user's common folders.
#[tauri::command]
pub async fn browse_locations(app: AppHandle) -> Result<Vec<Location>, String> {
    let mut out = spawn_blocking(browse::roots)
        .await
        .map_err(|e| e.to_string())?;
    let p = app.path();
    let places = [
        ("Home", p.home_dir()),
        ("Desktop", p.desktop_dir()),
        ("Documents", p.document_dir()),
        ("Music", p.audio_dir()),
        ("Downloads", p.download_dir()),
    ];
    for (label, dir) in places {
        if let Ok(path) = dir {
            if path.is_dir() {
                out.push(Location {
                    path,
                    label: label.into(),
                    kind: LocationKind::Place,
                });
            }
        }
    }
    Ok(out)
}

/// Lists a folder for the Drive Manager.
#[tauri::command]
pub async fn browse_list(path: PathBuf, audio_only: bool) -> Result<Vec<Entry>, String> {
    spawn_blocking(move || browse::list_dir(&path, audio_only))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Starts previewing a file, stopping any other preview.
#[tauri::command]
pub async fn preview_play(
    player: State<'_, PreviewPlayer>,
    transport: State<'_, crate::engine::Transport>,
    path: PathBuf,
    start: Option<f64>,
) -> Result<(), String> {
    // One thing plays at a time.
    transport.stop();
    player.play(&path, start.unwrap_or(0.0))
}

#[tauri::command]
pub fn preview_stop(player: State<'_, PreviewPlayer>) {
    player.stop();
}

#[tauri::command]
pub fn preview_status(player: State<'_, PreviewPlayer>) -> PreviewStatus {
    player.status()
}
