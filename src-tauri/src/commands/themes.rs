// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

//! Theme files: the `themes` folder in the app config folder. Users drop
//! `.audtheme` files there. These commands only list the folder and read
//! one file as text; the interface checks what is in it. No network.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, Manager};

/// A theme file is JSON with pictures inside, so it can be large, but not without end.
const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;
/// More files than this in the folder are ignored.
const MAX_FILES: usize = 200;

/// One theme file in the folder.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ThemeFile {
    pub file_name: String,
    pub size: u64,
}

fn has_theme_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("audtheme"))
}

/// The `.audtheme` files directly inside `dir`, by name. A missing folder is an empty list.
fn list_in(dir: &Path) -> Result<Vec<ThemeFile>, String> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("Could not read the Themes folder: {e}")),
    };
    let mut files: Vec<ThemeFile> = entries
        .filter_map(Result::ok)
        .filter(|entry| has_theme_extension(&entry.path()))
        .filter_map(|entry| {
            let meta = entry.metadata().ok()?;
            let file_name = entry.file_name().into_string().ok()?;
            meta.is_file().then_some(ThemeFile {
                file_name,
                size: meta.len(),
            })
        })
        .collect();
    files.sort_by(|a, b| a.file_name.cmp(&b.file_name));
    files.truncate(MAX_FILES);
    Ok(files)
}

/// The text of one theme file in `dir`. `file_name` is a bare name: no folders.
fn read_in(dir: &Path, file_name: &str) -> Result<String, String> {
    let bare = Path::new(file_name).file_name().and_then(|n| n.to_str());
    if bare != Some(file_name) || !has_theme_extension(Path::new(file_name)) {
        return Err("That is not a theme file name.".to_string());
    }
    let path = dir.join(file_name);
    let meta = fs::metadata(&path).map_err(|e| format!("Could not read {file_name}: {e}"))?;
    if !meta.is_file() {
        return Err(format!("{file_name} is not a file."));
    }
    if meta.len() > MAX_FILE_BYTES {
        return Err(format!(
            "{file_name} is too large (the limit is {} MB).",
            MAX_FILE_BYTES / (1024 * 1024)
        ));
    }
    let mut text = String::new();
    fs::File::open(&path)
        .and_then(|f| f.take(MAX_FILE_BYTES + 1).read_to_string(&mut text))
        .map_err(|e| format!("Could not read {file_name}: {e}"))?;
    if text.len() as u64 > MAX_FILE_BYTES {
        return Err(format!("{file_name} is too large."));
    }
    Ok(text)
}

fn themes_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|dir| dir.join("themes"))
        .map_err(|e| e.to_string())
}

/// The Themes folder, made if it is not there yet.
#[tauri::command]
pub fn themes_dir(app: AppHandle) -> Result<PathBuf, String> {
    let dir = themes_path(&app)?;
    fs::create_dir_all(&dir).map_err(|e| format!("Could not make the Themes folder: {e}"))?;
    Ok(dir)
}

/// The `.audtheme` files in the Themes folder.
#[tauri::command]
pub fn themes_list(app: AppHandle) -> Result<Vec<ThemeFile>, String> {
    list_in(&themes_path(&app)?)
}

/// The text of one theme file in the Themes folder.
#[tauri::command]
pub fn themes_read(app: AppHandle, file_name: String) -> Result<String, String> {
    read_in(&themes_path(&app)?, &file_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_only_theme_files_by_name() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("b.audtheme"), "{}").unwrap();
        fs::write(dir.path().join("a.AUDTHEME"), "{}").unwrap();
        fs::write(dir.path().join("notes.txt"), "x").unwrap();
        fs::create_dir(dir.path().join("folder.audtheme")).unwrap();
        let names: Vec<_> = list_in(dir.path())
            .unwrap()
            .into_iter()
            .map(|f| f.file_name)
            .collect();
        assert_eq!(names, ["a.AUDTHEME", "b.audtheme"]);
    }

    #[test]
    fn a_missing_folder_is_an_empty_list() {
        let dir = tempfile::tempdir().unwrap();
        assert!(list_in(&dir.path().join("nope")).unwrap().is_empty());
    }

    #[test]
    fn reads_a_file_and_refuses_other_paths() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.audtheme"), "hello").unwrap();
        assert_eq!(read_in(dir.path(), "a.audtheme").unwrap(), "hello");
        assert!(read_in(dir.path(), "../a.audtheme").is_err());
        assert!(read_in(dir.path(), "sub/a.audtheme").is_err());
        assert!(read_in(dir.path(), "a.txt").is_err());
        assert!(read_in(dir.path(), "missing.audtheme").is_err());
    }
}
