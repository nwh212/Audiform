// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! The user's shortcut changes, kept in the app config folder.

use std::collections::BTreeMap;

use tauri::{AppHandle, Manager};

use crate::session::keymap;

fn config_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    app.path().app_config_dir().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn keymap_load(app: AppHandle) -> Result<BTreeMap<String, Vec<String>>, String> {
    keymap::load_user_keymap(&config_dir(&app)?).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn keymap_save(app: AppHandle, bindings: BTreeMap<String, Vec<String>>) -> Result<(), String> {
    keymap::save_user_keymap(&config_dir(&app)?, bindings).map_err(|e| e.to_string())
}
