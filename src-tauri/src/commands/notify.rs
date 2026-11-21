// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

//! Desktop notifications: one when long work (an export, a conversion, a
//! separation) finishes while the window is in the background. Linux only,
//! through the desktop's notification service; on other systems the
//! command does nothing. The interface decides when to ask (the setting,
//! and whether the window has focus).

use tauri::AppHandle;

/// Shows a desktop notification with Audiform's name and `body`. Linux only.
#[tauri::command]
pub fn desktop_notify(app: AppHandle, body: String) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        use tauri_plugin_notification::NotificationExt;
        let body: String = body.chars().take(400).collect();
        app.notification()
            .builder()
            .title("Audiform")
            .body(body)
            .show()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(not(target_os = "linux"))]
    let _ = (app, body);
    Ok(())
}
