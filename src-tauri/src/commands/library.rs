// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Libraries panel commands. The interface edits a whole `Library`
//! value and saves it back; files are copied in by `library_import`.

use std::path::PathBuf;

use serde::Serialize;
use tauri::async_runtime::spawn_blocking;
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::DialogExt;

use crate::library::store::{self, Skipped};
use crate::library::{Library, LibraryItem};

/// Where new libraries go by default: Documents/Audiform/Libraries.
#[tauri::command]
pub fn library_default_root(app: AppHandle) -> Result<PathBuf, String> {
    let docs = app
        .path()
        .document_dir()
        .or_else(|_| app.path().home_dir())
        .map_err(|e| e.to_string())?;
    Ok(docs.join("Audiform").join("Libraries"))
}

#[tauri::command]
pub async fn library_create(dir: PathBuf, name: String) -> Result<Library, String> {
    spawn_blocking(move || store::create(&dir, &name).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn library_open(dir: PathBuf) -> Result<Library, String> {
    spawn_blocking(move || store::open(&dir).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn library_save(dir: PathBuf, library: Library) -> Result<(), String> {
    spawn_blocking(move || store::save(&dir, &library).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}

#[derive(Debug, Serialize)]
pub struct ImportResult {
    pub items: Vec<LibraryItem>,
    pub skipped: Vec<Skipped>,
}

/// Measures files and copies them into the library folder (or links
/// them when `copy` is false). Does not change `library.json`.
#[tauri::command]
pub async fn library_import(
    dir: PathBuf,
    paths: Vec<PathBuf>,
    copy: bool,
) -> Result<ImportResult, String> {
    spawn_blocking(move || {
        let expanded = crate::codecs::expand_paths(&paths);
        store::import(&dir, &expanded, copy)
            .map(|(items, skipped)| ImportResult { items, skipped })
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Deletes copied audio files no item uses any more. Returns the count.
/// With `dry_run`, only counts them, so the interface can ask first.
#[tauri::command]
pub async fn library_clean_up(
    dir: PathBuf,
    library: Library,
    dry_run: bool,
) -> Result<usize, String> {
    spawn_blocking(move || store::clean_up(&dir, &library, dry_run).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}

/// Asks for a folder that holds a library.
#[tauri::command]
pub async fn library_pick_folder(app: AppHandle, title: String) -> Result<Option<PathBuf>, String> {
    let parent = app.get_webview_window("main");
    spawn_blocking(move || {
        let mut dialog = app.dialog().file().set_title(title);
        if let Some(w) = parent.as_ref() {
            dialog = dialog.set_parent(w);
        }
        dialog
            .blocking_pick_folder()
            .and_then(|f| f.as_path().map(|p| p.to_path_buf()))
    })
    .await
    .map_err(|e| e.to_string())
}
