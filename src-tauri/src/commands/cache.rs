// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Settings > Media and Cache: where the app keeps its working files
//! and how much room they take. Only the waveform drawings are cleared
//! from here: they are rebuilt on demand. Edit results, untitled files
//! and recordings stay, since an open file or session may still use them.

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::async_runtime::spawn_blocking;
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Serialize)]
pub struct CacheFolder {
    pub name: String,
    pub bytes: u64,
    /// Safe to clear from Settings.
    pub clearable: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CacheInfo {
    pub path: PathBuf,
    pub folders: Vec<CacheFolder>,
}

fn size(p: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(p) else {
        return 0;
    };
    entries
        .flatten()
        .map(|e| match e.metadata() {
            Ok(m) if m.is_dir() => size(&e.path()),
            Ok(m) => m.len(),
            Err(_) => 0,
        })
        .sum()
}

/// Folders the app writes, with what each one holds.
const FOLDERS: [(&str, bool); 5] = [
    ("peaks", true),
    ("spectral", false),
    ("wave-fx", false),
    ("untitled", false),
    ("recordings", false),
];

/// File in the app config folder that holds a folder the user picked.
const FOLDER_SETTING: &str = "cache-folder.txt";

/// Where working files go: the folder the user picked in Settings, else
/// `Documents/Audiform/Cache`, else the app's own cache folder
/// when Documents can't be found. Missing folders fall back to the default.
pub fn cache_root(app: &AppHandle) -> Result<PathBuf, String> {
    if let Ok(cfg) = app.path().app_config_dir() {
        if let Ok(text) = std::fs::read_to_string(cfg.join(FOLDER_SETTING)) {
            let p = PathBuf::from(text.trim());
            if !text.trim().is_empty() && p.is_dir() {
                return Ok(p);
            }
        }
    }
    match app.path().document_dir() {
        Ok(docs) => Ok(docs.join("Audiform").join("Cache")),
        Err(_) => app.path().app_cache_dir().map_err(|e| e.to_string()),
    }
}

fn cache_dir(app: &AppHandle) -> Result<PathBuf, String> {
    cache_root(app)
}

/// Uses `path` for working files from now on (None goes back to the
/// default). Files already made stay where they are; drawings are rebuilt.
#[tauri::command]
pub fn cache_set_folder(app: AppHandle, path: Option<String>) -> Result<(), String> {
    let cfg = app.path().app_config_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&cfg).map_err(|e| e.to_string())?;
    let file = cfg.join(FOLDER_SETTING);
    match path.filter(|p| !p.trim().is_empty()) {
        Some(p) => {
            let dir = PathBuf::from(&p);
            std::fs::create_dir_all(&dir).map_err(|e| format!("Can't use that folder: {e}"))?;
            let probe = dir.join(".write-test");
            std::fs::write(&probe, b"x").map_err(|e| format!("That folder isn't writable: {e}"))?;
            let _ = std::fs::remove_file(probe);
            std::fs::write(file, p).map_err(|e| e.to_string())
        }
        None => match std::fs::remove_file(file) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        },
    }
}

#[tauri::command]
pub async fn cache_info(app: AppHandle) -> Result<CacheInfo, String> {
    let dir = cache_dir(&app)?;
    spawn_blocking(move || CacheInfo {
        folders: FOLDERS
            .iter()
            .map(|(n, c)| CacheFolder {
                name: (*n).to_string(),
                bytes: size(&dir.join(n)),
                clearable: *c,
            })
            .collect(),
        path: dir,
    })
    .await
    .map_err(|e| e.to_string())
}

/// Deletes the waveform drawings. Returns the bytes freed.
#[tauri::command]
pub async fn cache_clear_peaks(app: AppHandle) -> Result<u64, String> {
    let dir = cache_dir(&app)?.join("peaks");
    spawn_blocking(move || {
        let freed = size(&dir);
        if dir.exists() {
            std::fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
        }
        Ok(freed)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_add_up_through_folders() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("a/b")).unwrap();
        std::fs::write(d.path().join("a/x"), [0u8; 10]).unwrap();
        std::fs::write(d.path().join("a/b/y"), [0u8; 5]).unwrap();
        assert_eq!(size(&d.path().join("a")), 15);
        assert_eq!(size(&d.path().join("missing")), 0);
    }
}
