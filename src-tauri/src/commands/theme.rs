// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

//! Settings > Appearance: a picture of your own behind the scene themes.
//! The picture is only read from where it is; nothing is copied or sent.

use std::path::{Path, PathBuf};

use tauri::async_runtime::spawn_blocking;
use tauri::ipc::Response;
use tauri::{AppHandle, Manager, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

/// Pictures larger than this are refused (a 8K photo is well under it).
const MAX_PICTURE_BYTES: u64 = 40 * 1024 * 1024;
const KINDS: [&str; 6] = ["jpg", "jpeg", "png", "webp", "bmp", "gif"];

fn is_picture(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| KINDS.contains(&e.to_ascii_lowercase().as_str()))
}

/// Shows an Open dialog for a background picture.
#[tauri::command]
pub async fn theme_pick_background(app: AppHandle) -> Result<Option<PathBuf>, String> {
    let parent: Option<WebviewWindow> = app.get_webview_window("main");
    spawn_blocking(move || {
        let mut d = app
            .dialog()
            .file()
            .set_title("Choose a Background Picture")
            .add_filter("Pictures", &KINDS);
        if let Some(w) = parent.as_ref() {
            d = d.set_parent(w);
        }
        d.blocking_pick_file()
            .and_then(|f| f.as_path().map(Path::to_path_buf))
    })
    .await
    .map_err(|e| e.to_string())
}

/// The picture's bytes, for the interface to show.
#[tauri::command]
pub async fn theme_background(path: PathBuf) -> Result<Response, String> {
    spawn_blocking(move || {
        if !is_picture(&path) {
            return Err("Pick a JPEG, PNG, WebP, BMP or GIF picture.".into());
        }
        let size = std::fs::metadata(&path)
            .map_err(|_| format!("The picture {} is not there any more.", path.display()))?
            .len();
        if size > MAX_PICTURE_BYTES {
            return Err("That picture is over 40 MB. Pick a smaller one.".into());
        }
        std::fs::read(&path)
            .map(Response::new)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_pictures() {
        assert!(is_picture(Path::new("C:/a/Hills.JPG")));
        assert!(is_picture(Path::new("x.webp")));
        assert!(!is_picture(Path::new("x.exe")));
        assert!(!is_picture(Path::new("noext")));
    }
}
