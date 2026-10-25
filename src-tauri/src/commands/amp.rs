// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

//! The Amp Model's window: picking a capture (`.nam`) or a cabinet
//! response (an audio file) and reading it into the slot's data. The
//! interface then saves the new data as one undoable edit.

use std::path::{Path, PathBuf};

use tauri::async_runtime::spawn_blocking;
use tauri::{AppHandle, Manager, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use crate::codecs::AudioReader;
use crate::dsp::effects::amp::{read_model, AmpData, Ir, IR_MAX_SECONDS};

/// Captures larger than this are not real ones.
const MAX_MODEL_BYTES: u64 = 64 * 1024 * 1024;

/// Shows an Open dialog for a capture or a cabinet response.
#[tauri::command]
pub async fn amp_pick(app: AppHandle, kind: String) -> Result<Option<PathBuf>, String> {
    let parent: Option<WebviewWindow> = app.get_webview_window("main");
    spawn_blocking(move || {
        let mut d = app.dialog().file();
        d = if kind == "ir" {
            d.set_title("Load Cabinet")
                .add_filter("Impulse responses", &["wav", "aif", "aiff", "flac"])
        } else {
            d.set_title("Load Amp Capture")
                .add_filter("Amp captures", &["nam"])
        };
        if let Some(w) = parent.as_ref() {
            d = d.set_parent(w);
        }
        d.blocking_pick_file()
            .and_then(|f| f.as_path().map(Path::to_path_buf))
    })
    .await
    .map_err(|e| e.to_string())
}

fn file_name(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn current(data: Option<String>) -> AmpData {
    data.and_then(|d| serde_json::from_str(&d).ok())
        .unwrap_or_default()
}

/// Reads a capture file into the slot's data, replacing any capture there.
#[tauri::command]
pub async fn amp_load_model(path: PathBuf, data: Option<String>) -> Result<String, String> {
    spawn_blocking(move || {
        let size = std::fs::metadata(&path).map_err(|e| e.to_string())?.len();
        if size > MAX_MODEL_BYTES {
            return Err("This file is too big to be an amp capture.".into());
        }
        let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let (model, _) = read_model(&text)?;
        let mut d = current(data);
        d.model = Some(model);
        d.model_name = Some(file_name(&path));
        serde_json::to_string(&d).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Reads a cabinet response (mixed to mono, at its own rate, up to a
/// second long) into the slot's data.
#[tauri::command]
pub async fn amp_load_ir(path: PathBuf, data: Option<String>) -> Result<String, String> {
    spawn_blocking(move || {
        let mut r = AudioReader::open(&path, 1, None).map_err(|e| e.to_string())?;
        let rate = r.source_rate();
        let max = (IR_MAX_SECONDS * f64::from(rate)) as usize;
        let mut samples = Vec::new();
        let mut buf = Vec::new();
        while samples.len() < max {
            buf.clear();
            let more = r.read_into(&mut buf).map_err(|e| e.to_string())?;
            samples.extend_from_slice(&buf);
            if !more {
                break;
            }
        }
        samples.truncate(max);
        if samples.is_empty() {
            return Err("This file has no sound in it.".into());
        }
        // Rounded to keep the session file smaller; far below hearing.
        for s in &mut samples {
            *s = (*s * 1e6).round() / 1e6;
        }
        let mut d = current(data);
        d.ir = Some(Ir {
            name: file_name(&path),
            rate,
            samples,
        });
        serde_json::to_string(&d).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}
