// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Panel layout persistence. The layout lives in the app config folder,
//! never in browser storage.

use std::path::PathBuf;

use serde_json::Value;
use tauri::{AppHandle, Manager};

use crate::session::workspace;

fn config_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path().app_config_dir().map_err(|e| e.to_string())
}

/// Saved named workspaces, without their layouts.
#[tauri::command]
pub fn workspace_list(app: AppHandle) -> Result<Vec<workspace::WorkspaceInfo>, String> {
    Ok(workspace::list_named(&config_dir(&app)?))
}

/// One named workspace's saved layout, or null when it has none yet.
#[tauri::command]
pub fn workspace_get(app: AppHandle, id: String) -> Result<Option<Value>, String> {
    workspace::load_named(&config_dir(&app)?, &id)
        .map(|w| w.map(|w| w.layout))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn workspace_put(
    app: AppHandle,
    id: String,
    name: String,
    built_in: bool,
    layout: Value,
) -> Result<(), String> {
    let mut w = workspace::Workspace::new(id, name, layout);
    w.built_in = built_in;
    workspace::save_named(&config_dir(&app)?, w).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn workspace_delete(app: AppHandle, id: String) -> Result<(), String> {
    workspace::delete_named(&config_dir(&app)?, &id).map_err(|e| e.to_string())
}
