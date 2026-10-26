// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Files panel commands. Disk work runs on a blocking thread so the
//! window never freezes.

use std::path::{Path, PathBuf};

use tauri::async_runtime::spawn_blocking;
use tauri::{AppHandle, Manager, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use crate::codecs::{self, AudioInfo, SUPPORTED_EXTENSIONS};

/// Expands dropped folders into the audio files inside them.
#[tauri::command]
pub async fn media_expand(paths: Vec<PathBuf>) -> Result<Vec<PathBuf>, String> {
    spawn_blocking(move || codecs::expand_paths(&paths))
        .await
        .map_err(|e| e.to_string())
}

/// Identifies and measures one audio file.
#[tauri::command]
pub async fn media_probe(path: PathBuf) -> Result<AudioInfo, String> {
    spawn_blocking(move || codecs::probe(&path).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}

/// Shows the Import Files dialog. Returns an empty list on cancel.
#[tauri::command]
pub async fn media_pick_files(app: AppHandle) -> Result<Vec<PathBuf>, String> {
    let parent: Option<WebviewWindow> = app.get_webview_window("main");
    spawn_blocking(move || {
        let mut dialog = app
            .dialog()
            .file()
            .set_title("Import Files")
            .add_filter("Audio files", SUPPORTED_EXTENSIONS)
            .add_filter("All files", &["*"]);
        if let Some(w) = parent.as_ref() {
            dialog = dialog.set_parent(w);
        }
        dialog
            .blocking_pick_files()
            .unwrap_or_default()
            .iter()
            .filter_map(|f| f.as_path().map(Path::to_path_buf))
            .collect()
    })
    .await
    .map_err(|e| e.to_string())
}

/// Shows the Insert Files as Tracks dialog: audio files, or zips of them.
#[tauri::command]
pub async fn media_pick_tracks(app: AppHandle) -> Result<Vec<PathBuf>, String> {
    let parent: Option<WebviewWindow> = app.get_webview_window("main");
    spawn_blocking(move || {
        let mut exts: Vec<&str> = SUPPORTED_EXTENSIONS.to_vec();
        exts.push("zip");
        let mut dialog = app
            .dialog()
            .file()
            .set_title("Insert Files as Tracks")
            .add_filter("Audio files and zips", &exts)
            .add_filter("All files", &["*"]);
        if let Some(w) = parent.as_ref() {
            dialog = dialog.set_parent(w);
        }
        dialog
            .blocking_pick_files()
            .unwrap_or_default()
            .iter()
            .filter_map(|f| f.as_path().map(Path::to_path_buf))
            .collect()
    })
    .await
    .map_err(|e| e.to_string())
}

/// Extracts the audio in a zip into a folder beside it and returns the files.
#[tauri::command]
pub async fn media_unzip(path: PathBuf) -> Result<Vec<PathBuf>, String> {
    spawn_blocking(move || codecs::extract_audio(&path))
        .await
        .map_err(|e| e.to_string())?
}

/// Shows the Import Folder dialog. Returns None on cancel.
#[tauri::command]
pub async fn media_pick_folder(app: AppHandle) -> Result<Option<PathBuf>, String> {
    let parent: Option<WebviewWindow> = app.get_webview_window("main");
    spawn_blocking(move || {
        let mut dialog = app.dialog().file().set_title("Import Folder");
        if let Some(w) = parent.as_ref() {
            dialog = dialog.set_parent(w);
        }
        dialog
            .blocking_pick_folder()
            .and_then(|f| f.as_path().map(Path::to_path_buf))
    })
    .await
    .map_err(|e| e.to_string())
}

/// Opens the system file manager with the file selected.
#[tauri::command]
pub fn media_reveal(path: PathBuf) -> Result<(), String> {
    if !path.exists() {
        return Err(format!("File not found: {}", path.display()));
    }
    tauri_plugin_opener::reveal_item_in_dir(&path).map_err(|e| e.to_string())
}

/// Copies audio files into the session's folder (`folder` beside
/// `session`), leaving files already inside the session folder alone.
/// Returns where each file is now.
#[tauri::command]
pub async fn media_copy_in(
    session: PathBuf,
    folder: String,
    paths: Vec<PathBuf>,
) -> Result<crate::session::collect::CopyResult, String> {
    use crate::session::collect;
    spawn_blocking(move || {
        let dir = collect::media_dir(&session, &folder);
        let keep = session.parent().unwrap_or(Path::new("")).to_path_buf();
        collect::copy_in(&paths, &dir, &keep)
    })
    .await
    .map_err(|e| e.to_string())
}
