// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

use serde::Serialize;

use crate::session::{Document, Session};

/// Basic facts about the running core, shown in the status bar.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub session_schema_version: u32,
}

/// Builds the app info without needing a running Tauri app.
pub fn current_app_info() -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        session_schema_version: Session::CURRENT_VERSION,
    }
}

#[tauri::command]
pub fn app_info() -> AppInfo {
    current_app_info()
}

/// Minimizes the main window. Called from Window > Minimize.
#[tauri::command]
pub fn app_minimize(window: tauri::WebviewWindow) -> Result<(), String> {
    window.minimize().map_err(|e| e.to_string())
}

/// Quits the app. Called from File > Exit.
/// Files Audiform was started with (File Explorer's Open or Open with),
/// handed to the interface once, when it is ready.
#[derive(Default)]
pub struct LaunchPaths(pub std::sync::Mutex<Vec<String>>);

/// The arguments that name existing files, relative ones taken from `cwd`.
pub fn file_args<I: IntoIterator<Item = String>>(args: I, cwd: &std::path::Path) -> Vec<String> {
    args.into_iter()
        .filter(|a| !a.starts_with('-'))
        .map(|a| cwd.join(a))
        .filter(|p| p.is_file())
        .map(|p| p.to_string_lossy().into_owned())
        .collect()
}

/// The files Audiform was started with; empty after the first call.
#[tauri::command]
pub fn launch_paths(state: tauri::State<'_, LaunchPaths>) -> Vec<String> {
    std::mem::take(&mut *state.0.lock().unwrap_or_else(|e| e.into_inner()))
}

/// Whether Alt is held, read from the system: while files are dragged in
/// from the file manager the window hears no key presses. `None` where the
/// system cannot say (the interface then uses its own key events).
#[tauri::command]
pub fn alt_down() -> Option<bool> {
    audiform_os_keys::alt_down()
}

#[tauri::command]
pub fn app_exit(app: tauri::AppHandle) {
    app.exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_package_version_and_schema_version() {
        let info = current_app_info();
        assert_eq!(info.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(info.session_schema_version, Session::CURRENT_VERSION);
    }

    #[test]
    fn serializes_as_camel_case_for_the_frontend() {
        let json = serde_json::to_value(current_app_info()).unwrap();
        assert!(json.get("sessionSchemaVersion").is_some());
    }
}

/// Sets the window title, for example "Ep 440 * - Audiform".
#[tauri::command]
pub fn app_set_title(app: tauri::AppHandle, title: String) -> Result<(), String> {
    use tauri::Manager;
    match app.get_webview_window("main") {
        Some(w) => w.set_title(&title).map_err(|e| e.to_string()),
        None => Ok(()),
    }
}
