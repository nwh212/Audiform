// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

//! Track Details: a file's tags and cover picture, shown and edited in
//! their own window (see `export/details.rs`).

use std::path::PathBuf;

use altered_plughost::protocol::{base64_decode, base64_encode};
use serde::Serialize;
use tauri::async_runtime::spawn_blocking;
use tauri::{AppHandle, Manager};

use crate::export::details::{prepare_cover, read_cover, writable, write_details};
use crate::export::tags::{read_tags, Tags};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackDetails {
    pub tags: Tags,
    /// The cover as a `data:` URL, ready to show.
    pub cover: Option<String>,
    /// Track Details can write this file (WAV, FLAC, MP3, M4A).
    pub writable: bool,
}

fn data_url(bytes: &[u8]) -> String {
    let mime = if bytes.starts_with(b"\x89PNG") {
        "image/png"
    } else {
        "image/jpeg"
    };
    format!("data:{mime};base64,{}", base64_encode(bytes))
}

fn from_data_url(url: &str) -> Result<Vec<u8>, String> {
    let b64 = url.split_once(',').map_or(url, |(_, b)| b);
    base64_decode(b64).ok_or_else(|| "The picture is damaged.".to_string())
}

/// The tags and cover of an audio file.
#[tauri::command]
pub async fn track_details_read(path: PathBuf) -> Result<TrackDetails, String> {
    spawn_blocking(move || TrackDetails {
        tags: read_tags(&path),
        cover: read_cover(&path).map(|c| data_url(&c)),
        writable: writable(&path),
    })
    .await
    .map_err(|e| e.to_string())
}

/// Asks for a picture and returns it ready to store: shrunk to fit and
/// saved as JPEG. None on cancel.
#[tauri::command]
pub async fn track_details_pick_cover(app: AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let parent = app.get_webview_window("main");
    spawn_blocking(move || {
        let mut d = app
            .dialog()
            .file()
            .set_title("Choose a Cover Picture")
            .add_filter("Pictures", &["jpg", "jpeg", "png", "webp", "bmp", "gif"]);
        if let Some(w) = parent.as_ref() {
            d = d.set_parent(w);
        }
        let Some(path) = d
            .blocking_pick_file()
            .and_then(|f| f.as_path().map(std::path::Path::to_path_buf))
        else {
            return Ok(None);
        };
        let bytes = std::fs::read(&path).map_err(|e| format!("Could not read the picture: {e}"))?;
        Ok(Some(data_url(&prepare_cover(&bytes)?)))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Writes the tags and cover into the file, replacing what was there.
#[tauri::command]
pub async fn track_details_write(
    path: PathBuf,
    tags: Tags,
    cover: Option<String>,
) -> Result<(), String> {
    spawn_blocking(move || {
        let pic = cover.as_deref().map(from_data_url).transpose()?;
        // A picture that came from elsewhere is brought to size first.
        let pic = pic.map(|p| prepare_cover(&p).unwrap_or(p));
        write_details(&path, &tags, pic.as_deref())
            .map_err(|e| format!("Could not save the details: {e}"))
    })
    .await
    .map_err(|e| e.to_string())?
}
