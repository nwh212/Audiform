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
