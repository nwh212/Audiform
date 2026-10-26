// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Mixdown commands: pick where to save, render with progress, cancel.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::Serialize;
use tauri::async_runtime::spawn_blocking;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

use super::session::OpenSession;
use crate::export::encode::FormatKind;
use crate::export::loudness::{self, LoudnessReport};
use crate::export::mixdown::{self, MixdownRequest, MixdownResult};
use crate::export::region;

/// Set to stop the running export.
#[derive(Default)]
pub struct ExportCancel(pub Arc<AtomicBool>);

#[derive(Clone, Serialize)]
struct Progress {
    done: u64,
    total: u64,
}

/// Save dialog for a mixdown. `ext` is "wav" (default), "flac" or "mp3".
/// `all` also offers the other formats. `folder` is where it opens, when known.
#[tauri::command]
pub async fn export_pick_path(
    app: AppHandle,
    name: String,
    folder: Option<PathBuf>,
    title: Option<String>,
    ext: Option<String>,
    all: Option<bool>,
) -> Result<Option<PathBuf>, String> {
    let parent = app.get_webview_window("main");
    let kind = ext
        .as_deref()
        .and_then(FormatKind::from_extension)
        .unwrap_or_default();
    let ext = kind.extension();
    let label = |k: FormatKind| match k {
        FormatKind::Wav => "WAV audio",
        FormatKind::Flac => "FLAC audio",
        FormatKind::Mp3 => "MP3 audio",
        FormatKind::M4a => "AAC audio (M4A)",
    };
    spawn_blocking(move || {
        let mut d = app
            .dialog()
            .file()
            .set_title(title.unwrap_or_else(|| "Export Mixdown".into()))
            .set_file_name(format!("{name}.{ext}"))
            .add_filter(label(kind), &[ext]);
        if all == Some(true) {
            for k in [
                FormatKind::Wav,
                FormatKind::Flac,
                FormatKind::Mp3,
                FormatKind::M4a,
            ] {
                if k != kind {
                    d = d.add_filter(label(k), &[k.extension()]);
                }
            }
        }
        if let Some(f) = folder.filter(|f| f.is_dir()) {
            d = d.set_directory(f);
        }
        if let Some(w) = parent.as_ref() {
            d = d.set_parent(w);
        }
        d.blocking_save_file()
            .and_then(|f| f.as_path().map(|p| p.to_path_buf()))
            .map(|p| {
                if p.extension().is_none() {
                    p.with_extension(ext)
                } else {
                    p
                }
            })
    })
    .await
    .map_err(|e| e.to_string())
}

/// Asks where to save a text file (chapters, marker lists) and writes it.
/// Only the path the user picked is written. Returns it, or None when the
/// dialog was cancelled.
#[tauri::command]
pub async fn export_text(
    app: AppHandle,
    name: String,
    folder: Option<PathBuf>,
    title: String,
    filter: String,
    ext: String,
    content: String,
) -> Result<Option<PathBuf>, String> {
    let parent = app.get_webview_window("main");
    let ext: String = ext.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    if ext.is_empty() {
        return Err("No file type given.".into());
    }
    spawn_blocking(move || {
        let mut d = app
            .dialog()
            .file()
            .set_title(title)
            .set_file_name(format!("{name}.{ext}"))
            .add_filter(filter, &[ext.as_str()]);
        if let Some(f) = folder.filter(|f| f.is_dir()) {
            d = d.set_directory(f);
        }
        if let Some(w) = parent.as_ref() {
            d = d.set_parent(w);
        }
        let Some(path) = d
            .blocking_save_file()
            .and_then(|f| f.as_path().map(|p| p.to_path_buf()))
        else {
            return Ok(None);
        };
        let path = if path.extension().is_none() {
            path.with_extension(&ext)
        } else {
            path
        };
        std::fs::write(&path, content.as_bytes())
            .map_err(|e| format!("Could not write the file: {e}"))?;
        Ok(Some(path))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Folder dialog for track stems. `folder` is where it opens, when known.
#[tauri::command]
pub async fn export_pick_folder(
    app: AppHandle,
    folder: Option<PathBuf>,
    title: Option<String>,
) -> Result<Option<PathBuf>, String> {
    let parent = app.get_webview_window("main");
    spawn_blocking(move || {
        let mut d = app
            .dialog()
            .file()
            .set_title(title.unwrap_or_else(|| "Export Track Stems To".into()));
        if let Some(f) = folder.filter(|f| f.is_dir()) {
            d = d.set_directory(f);
        }
        if let Some(w) = parent.as_ref() {
            d = d.set_parent(w);
        }
        d.blocking_pick_folder()
            .and_then(|f| f.as_path().map(|p| p.to_path_buf()))
    })
    .await
    .map_err(|e| e.to_string())
}

/// Which of `paths` already exist, so the app can ask before overwriting.
#[tauri::command]
pub fn paths_exist(paths: Vec<PathBuf>) -> Vec<bool> {
    paths.iter().map(|p| p.exists()).collect()
}

/// Renders the open session to `path`. Emits `export-progress`.
#[tauri::command]
pub async fn export_mixdown(
    app: AppHandle,
    open: State<'_, OpenSession>,
    cancel: State<'_, ExportCancel>,
    path: PathBuf,
    request: MixdownRequest,
) -> Result<MixdownResult, String> {
    let session = open.0.lock().map_err(|e| e.to_string())?.session.clone();
    let flag = cancel.0.clone();
    flag.store(false, Ordering::Relaxed);
    spawn_blocking(move || {
        let mut last = 0u64;
        mixdown::mixdown(&session, &request, &path, &flag, |done, total| {
            // About 100 updates per export is plenty for a progress bar.
            if done == total || done - last >= total / 100 {
                last = done;
                let _ = app.emit("export-progress", Progress { done, total });
            }
        })
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Serialize)]
pub struct MergeResult {
    pub path: PathBuf,
    #[serde(flatten)]
    pub result: MixdownResult,
}

/// Merge Clips: renders the requested clips with only their own
/// settings to a new 32-bit float WAV in `folder` (created if needed),
/// named after `name` without overwriting anything. Emits
/// `export-progress` and stops on `export_cancel`.
#[tauri::command]
pub async fn export_merge(
    app: AppHandle,
    open: State<'_, OpenSession>,
    cancel: State<'_, ExportCancel>,
    folder: PathBuf,
    name: String,
    mut request: MixdownRequest,
) -> Result<MergeResult, String> {
    let session = open.0.lock().map_err(|e| e.to_string())?.session.clone();
    let flag = cancel.0.clone();
    flag.store(false, Ordering::Relaxed);
    request.clip_only = true;
    request.bit_depth = Some(32);
    spawn_blocking(move || {
        std::fs::create_dir_all(&folder)
            .map_err(|e| format!("Could not create {}: {e}", folder.display()))?;
        let path = mixdown::unique_path(&folder, &name, "wav");
        let mut last = 0u64;
        let result = mixdown::mixdown(&session, &request, &path, &flag, |done, total| {
            if done == total || done - last >= total / 100 {
                last = done;
                let _ = app.emit("export-progress", Progress { done, total });
            }
        })
        .map_err(|e| e.to_string())?;
        Ok(MergeResult { path, result })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Bounce to New Track: renders the requested part of the mix (with
/// track, bus and Master processing, as it plays) to a new 32-bit float
/// WAV in `folder` (created if needed) named after `name` without
/// overwriting anything. Emits `export-progress` and stops on
/// `export_cancel`.
#[tauri::command]
pub async fn export_bounce(
    app: AppHandle,
    open: State<'_, OpenSession>,
    cancel: State<'_, ExportCancel>,
    folder: PathBuf,
    name: String,
    mut request: MixdownRequest,
) -> Result<MergeResult, String> {
    let session = open.0.lock().map_err(|e| e.to_string())?.session.clone();
    let flag = cancel.0.clone();
    flag.store(false, Ordering::Relaxed);
    request.bit_depth = Some(32);
    request.format = None;
    spawn_blocking(move || {
        std::fs::create_dir_all(&folder)
            .map_err(|e| format!("Could not create {}: {e}", folder.display()))?;
        let path = mixdown::unique_path(&folder, &name, "wav");
        let mut last = 0u64;
        let result = mixdown::mixdown(&session, &request, &path, &flag, |done, total| {
            if done == total || done - last >= total / 100 {
                last = done;
                let _ = app.emit("export-progress", Progress { done, total });
            }
        })
        .map_err(|e| e.to_string())?;
        Ok(MergeResult { path, result })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Make Unique Copy: copies source frames `from..to` of `source`,
/// untouched, to a new 32-bit float WAV in `folder` (created if needed)
/// named after `name`. Returns the new file's path.
#[tauri::command]
pub async fn export_region_copy(
    source: PathBuf,
    from: u64,
    to: u64,
    folder: PathBuf,
    name: String,
) -> Result<PathBuf, String> {
    spawn_blocking(move || {
        std::fs::create_dir_all(&folder)
            .map_err(|e| format!("Could not create {}: {e}", folder.display()))?;
        let path = mixdown::unique_path(&folder, &name, "wav");
        region::copy_region(&source, from, to, &path).map_err(|e| e.to_string())?;
        Ok(path)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Measures the loudness of the open session's mix. Emits `export-progress`
/// and stops on `export_cancel`, like a mixdown.
#[tauri::command]
pub async fn loudness_check(
    app: AppHandle,
    open: State<'_, OpenSession>,
    cancel: State<'_, ExportCancel>,
    request: MixdownRequest,
) -> Result<LoudnessReport, String> {
    let session = open.0.lock().map_err(|e| e.to_string())?.session.clone();
    let flag = cancel.0.clone();
    flag.store(false, Ordering::Relaxed);
    spawn_blocking(move || {
        let mut last = 0u64;
        loudness::measure(&session, &request, &flag, |done, total| {
            if done == total || done - last >= total / 100 {
                last = done;
                let _ = app.emit("export-progress", Progress { done, total });
            }
        })
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn export_cancel(cancel: State<'_, ExportCancel>) {
    cancel.0.store(true, Ordering::Relaxed);
}

/// Writes chapter markers into an exported MP3 (ID3 chapters) or FLAC
/// (Vorbis comments). Other formats are left as they are.
#[tauri::command]
pub async fn export_embed_chapters(
    path: PathBuf,
    chapters: Vec<crate::export::chapters::Chapter>,
    total_ms: u64,
) -> Result<bool, String> {
    spawn_blocking(move || {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let r = match ext.as_str() {
            "mp3" => crate::export::chapters::embed_mp3(&path, &chapters, total_ms).map(|_| true),
            "flac" => crate::export::chapters::embed_flac(&path, &chapters).map(|_| true),
            _ => Ok(false),
        };
        r.map_err(|e| format!("Could not add chapters: {e}"))
    })
    .await
    .map_err(|e| e.to_string())?
}
