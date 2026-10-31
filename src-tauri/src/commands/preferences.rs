// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Interface preferences in the app config folder.

use std::collections::BTreeMap;

use serde_json::Value;
use tauri::{AppHandle, Manager};

use crate::session::preferences;

fn config_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    app.path().app_config_dir().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn preferences_load(app: AppHandle) -> Result<BTreeMap<String, Value>, String> {
    Ok(preferences::load_preferences(&config_dir(&app)?))
}

#[tauri::command]
pub fn preferences_save(app: AppHandle, values: BTreeMap<String, Value>) -> Result<(), String> {
    preferences::save_preferences(&config_dir(&app)?, values).map_err(|e| e.to_string())
}

/// Edit > Settings > General > Back Up Settings: asks where, then puts
/// every setting in one `.afbackup` file. Returns the file and how many
/// settings files went in, or None on cancel.
#[tauri::command]
pub async fn settings_backup(app: AppHandle) -> Result<Option<(std::path::PathBuf, usize)>, String> {
    use tauri_plugin_dialog::DialogExt;
    let dir = config_dir(&app)?;
    let version = app.package_info().version.to_string();
    let parent = app.get_webview_window("main");
    tauri::async_runtime::spawn_blocking(move || {
        let mut d = app
            .dialog()
            .file()
            .set_title("Back Up Settings")
            .set_file_name("Audiform Settings.afbackup")
            .add_filter("Audiform settings backup", &["afbackup"]);
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
            path.with_extension("afbackup")
        } else {
            path
        };
        let n = crate::settings_backup::backup(&dir, &path, &version)?;
        Ok(Some((path, n)))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Edit > Settings > General > Restore Settings: asks for a backup, then
/// writes its settings over the ones there. Returns how many files were
/// restored, or None on cancel. They take effect the next time Audiform starts.
#[tauri::command]
pub async fn settings_restore(app: AppHandle) -> Result<Option<usize>, String> {
    use tauri_plugin_dialog::DialogExt;
    let dir = config_dir(&app)?;
    let parent = app.get_webview_window("main");
    tauri::async_runtime::spawn_blocking(move || {
        let mut d = app
            .dialog()
            .file()
            .set_title("Restore Settings")
            .add_filter("Audiform settings backup", &["afbackup"]);
        if let Some(w) = parent.as_ref() {
            d = d.set_parent(w);
        }
        let Some(path) = d
            .blocking_pick_file()
            .and_then(|f| f.as_path().map(|p| p.to_path_buf()))
        else {
            return Ok(None);
        };
        crate::settings_backup::restore(&path, &dir).map(Some)
    })
    .await
    .map_err(|e| e.to_string())?
}
