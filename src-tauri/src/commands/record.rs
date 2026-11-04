// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Record and stop. Takes go in a Recordings folder next to the saved
//! session, or in the app's cache folder until the session is saved.

use std::path::PathBuf;

use tauri::{AppHandle, State};

use super::live;
use super::session::OpenSession;
use super::wave::Playing;
use crate::engine::{PreviewPlayer, RecordStatus, RecordTarget, Recorder, Take, Transport};

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
    transport: State<'_, Transport>,
    playing: State<'_, Playing>,
    targets: Vec<RecordTarget>,
) -> Result<PathBuf, String> {
    preview.stop();
    let dir = take_dir(&app, &open)?;
    // A track records over its clips, not along with them: they go quiet
    // for the take (the session itself is untouched).
    let ids: Vec<String> = targets
        .iter()
        .filter(|t| t.key != "wave")
        .map(|t| t.key.clone())
        .collect();
    recorder.start(targets, Some(dir.clone()))?;
    live::set_recording(ids);
    live::resync(&open, &transport, &playing);
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

/// While recording, starts a take for a track armed just now.
#[tauri::command]
pub fn record_add(
    recorder: State<'_, Recorder>,
    open: State<'_, OpenSession>,
    transport: State<'_, Transport>,
    playing: State<'_, Playing>,
    target: RecordTarget,
) -> Result<(), String> {
    let key = target.key.clone();
    recorder.add(target)?;
    live::add_recording(&key);
    live::resync(&open, &transport, &playing);
    Ok(())
}

/// While recording, ends the take of a track disarmed just now. Its clips
/// are heard again from here.
#[tauri::command]
pub fn record_remove(
    recorder: State<'_, Recorder>,
    open: State<'_, OpenSession>,
    transport: State<'_, Transport>,
    playing: State<'_, Playing>,
    key: String,
) -> Result<(), String> {
    recorder.remove(&key)?;
    live::remove_recording(&key);
    live::resync(&open, &transport, &playing);
    Ok(())
}

/// Stops recording and returns the takes. Every track's clips are heard again.
#[tauri::command]
pub fn record_stop(
    recorder: State<'_, Recorder>,
    open: State<'_, OpenSession>,
    transport: State<'_, Transport>,
    playing: State<'_, Playing>,
) -> Result<Vec<Take>, String> {
    let takes = recorder.stop();
    live::set_recording([]);
    live::resync(&open, &transport, &playing);
    takes
}

#[tauri::command]
pub fn record_status(recorder: State<'_, Recorder>) -> RecordStatus {
    recorder.status()
}
