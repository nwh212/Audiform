// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

//! Desktop notifications: one when long work (an export, a conversion, a
//! separation) finishes while the window is in the background. Linux only,
//! through the desktop's notification service; on other systems the
//! command does nothing. The interface decides when to ask (the setting,
//! and whether the window has focus).

use tauri::AppHandle;

/// Shows a desktop notification with Audiform's name, icon and `body`. Linux only.
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
            .icon(linux_icon::path(&app))
            .show()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(not(target_os = "linux"))]
    let _ = (app, body);
    Ok(())
}

/// The app icon for notifications. The notification service wants an icon
/// name from the system's icon theme or a file; a build that is not
/// installed has no themed icon, so the app writes its own icon (the
/// 256 pixel one from `icons/`) to its cache folder once and names that.
#[cfg(target_os = "linux")]
mod linux_icon {
    use std::path::PathBuf;
    use std::sync::OnceLock;

    use tauri::{AppHandle, Manager};

    const ICON: &[u8] = include_bytes!("../../icons/128x128@2x.png");
    /// The themed icon an installed Audiform has (same name as the program).
    const THEME_NAME: &str = "audiform";

    /// The icon file's path, or the themed icon's name when the file cannot be written.
    pub fn path(app: &AppHandle) -> String {
        static PATH: OnceLock<Option<PathBuf>> = OnceLock::new();
        PATH.get_or_init(|| write(app))
            .as_ref()
            .map_or_else(|| THEME_NAME.to_string(), |p| p.display().to_string())
    }

    fn write(app: &AppHandle) -> Option<PathBuf> {
        let dir = app.path().app_cache_dir().ok()?;
        std::fs::create_dir_all(&dir).ok()?;
        let file = dir.join("notification-icon.png");
        let same = std::fs::read(&file).is_ok_and(|b| b == ICON);
        if !same {
            std::fs::write(&file, ICON).ok()?;
        }
        Some(file)
    }
}
