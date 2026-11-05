// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

//! MIDI inputs: listing them, playing armed MIDI tracks live, and
//! recording what is played (see `engine::midi_live`).

use tauri::async_runtime::spawn_blocking;
use tauri::{AppHandle, Manager};

use crate::engine::midi_live::{self, LiveTrack, MidiLive, MidiTake};
use crate::session::model::Id;

/// The MIDI inputs on this computer, by name.
#[tauri::command]
pub async fn midi_inputs() -> Vec<String> {
    spawn_blocking(midi_live::inputs).await.unwrap_or_default()
}

/// The armed MIDI tracks: each listens to its input and plays its
/// instrument live. An empty list closes every input.
#[tauri::command]
pub async fn midi_live_set(app: AppHandle, tracks: Vec<LiveTrack>) -> Result<(), String> {
    spawn_blocking(move || app.state::<MidiLive>().set(tracks))
        .await
        .map_err(|e| e.to_string())?
}

/// Starts recording what the armed MIDI tracks `tracks` are played.
#[tauri::command]
pub fn midi_record_start(live: tauri::State<'_, MidiLive>, tracks: Vec<Id>) -> Result<(), String> {
    live.record_start(&tracks)
}

/// While recording, starts recording a MIDI track armed just now.
#[tauri::command]
pub fn midi_record_add(live: tauri::State<'_, MidiLive>, track: Id) -> Result<(), String> {
    live.record_track(track, true)
}

/// Stops recording and returns each track's notes, in seconds from the start.
#[tauri::command]
pub fn midi_record_stop(live: tauri::State<'_, MidiLive>) -> Result<Vec<MidiTake>, String> {
    live.record_stop()
}
