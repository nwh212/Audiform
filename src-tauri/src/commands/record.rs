// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Record and stop. Takes go in a Recordings folder next to the saved
//! session, or in the app's cache folder until the session is saved.

use std::path::PathBuf;

use tauri::{AppHandle, State};

use super::session::OpenSession;
use crate::engine::{PreviewPlayer, RecordStatus, RecordTarget, Recorder, Take};

fn take_dir(app: &AppHandle, open: &State<'_, OpenSession>) -> Result<PathBuf, String> {
    let saved = open.0.lock().map_err(|e| e.to_string())?.path.clone();
    match saved.as_ref().and_then(|p| p.parent()) {
        Some(dir) => Ok(dir.join("Recordings")),
        None => Ok(crate::commands::cache::cache_root(app)?.join("recordings")),
    }
}

/// Starts recording `targets`. Playback, if any, is started by the
/// interface right after, so the take lines up with what plays.
#[tauri::command]
pub fn record_start(
    app: AppHandle,
    recorder: State<'_, Recorder>,
    preview: State<'_, PreviewPlayer>,
    open: State<'_, OpenSession>,
    targets: Vec<RecordTarget>,
) -> Result<PathBuf, String> {
    preview.stop();
    let dir = take_dir(&app, &open)?;
    recorder.start(targets, Some(dir.clone()))?;
    Ok(dir)
}

/// Opens the armed tracks' inputs for their meters, without recording.
/// An empty list closes them.
#[tauri::command]
pub fn record_monitor(
    recorder: State<'_, Recorder>,
    targets: Vec<RecordTarget>,
) -> Result<(), String> {
    if targets.is_empty() {
        recorder.stop_monitor();
        return Ok(());
    }
    recorder.start(targets, None)
}

/// Stops recording and returns the takes.
#[tauri::command]
pub fn record_stop(recorder: State<'_, Recorder>) -> Result<Vec<Take>, String> {
    recorder.stop()
}

#[tauri::command]
pub fn record_status(recorder: State<'_, Recorder>) -> RecordStatus {
    recorder.status()
}
