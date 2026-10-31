// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

//! Linked sessions: another session's whole mix placed as one clip. The
//! session is rendered to a WAV file once, and the clip plays that file,
//! so a heavy session costs the parent no more than one audio clip. The
//! clip's media remembers which session it came from and how that file
//! looked when it was rendered; once the session is saved again, the
//! clip is rendered afresh.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

use super::export::ExportCancel;
use super::session::OpenSession;
use crate::export::mixdown::{self, MixdownRequest};
use crate::session::model::{Id, Session};

/// A session file's size and modified time, to tell when it was saved again.
pub fn stamp(path: &Path) -> Option<String> {
    let m = std::fs::metadata(path).ok()?;
    let t = m.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
    Some(format!("{}-{}", m.len(), t.as_millis()))
}

/// The session in a file: Audiform's own, or one from the other editor.
fn load(path: &Path) -> Result<Session, String> {
    let sesx = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("sesx"));
    if sesx {
        crate::sesx::import(path)
            .map(|i| i.session)
            .map_err(|e| e.to_string())
    } else {
        crate::session::file::open(path)
            .map(|o| o.session)
            .map_err(|e| e.to_string())
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkedRender {
    /// The rendered file.
    pub path: PathBuf,
    /// The session file's stamp when it was rendered.
    pub stamp: String,
    /// The session's name, for the clip and track.
    pub name: String,
    pub sample_rate: u32,
    pub frames: u64,
    pub peak: f32,
    pub cancelled: bool,
}

#[derive(Clone, Serialize)]
struct Progress {
    done: u64,
    total: u64,
}

/// Renders the whole mix of the session at `session` to a new 32-bit
/// float stereo WAV in `folder` (created if needed). Refuses the session
/// in front, which cannot hold its own mix. Emits `export-progress` and
/// stops on `export_cancel`.
#[tauri::command]
pub async fn linked_render(
    app: AppHandle,
    open: State<'_, OpenSession>,
    cancel: State<'_, ExportCancel>,
    session: PathBuf,
    folder: PathBuf,
) -> Result<LinkedRender, String> {
    let own = open.0.lock().map_err(|e| e.to_string())?.path.clone();
    if own.as_deref().is_some_and(|p| same_file(p, &session)) {
        return Err("A session cannot hold its own mix. Pick another session.".into());
    }
    let flag = cancel.0.clone();
    flag.store(false, Ordering::Relaxed);
    tauri::async_runtime::spawn_blocking(move || {
        let stamp = stamp(&session).ok_or("That session file could not be read.")?;
        let inner = load(&session)?;
        let name = inner.name.clone();
        std::fs::create_dir_all(&folder)
            .map_err(|e| format!("Could not create {}: {e}", folder.display()))?;
        let path = mixdown::unique_path(&folder, &file_safe(&name), "wav");
        let request = MixdownRequest {
            bit_depth: Some(32),
            channels: Some(2),
            ..MixdownRequest::default()
        };
        let mut last = 0u64;
        let r = mixdown::mixdown(&inner, &request, &path, &flag, |done, total| {
            if done == total || done - last >= total / 100 {
                last = done;
                let _ = app.emit("export-progress", Progress { done, total });
            }
        })
        .map_err(|e| e.to_string())?;
        Ok(LinkedRender {
            path,
            stamp,
            name,
            sample_rate: r.sample_rate,
            frames: r.frames,
            peak: r.peak,
            cancelled: r.cancelled,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Picks a session to place as a clip: Audiform's own or the other editor's.
#[tauri::command]
pub async fn linked_pick(app: AppHandle) -> Result<Option<PathBuf>, String> {
    let parent = app.get_webview_window("main");
    tauri::async_runtime::spawn_blocking(move || {
        use crate::session::file::{EXTENSION, OLD_EXTENSION};
        let mut d = app
            .dialog()
            .file()
            .set_title("Insert Session as Clip")
            .add_filter("Session", &[EXTENSION, OLD_EXTENSION, "sesx"]);
        if let Some(w) = parent.as_ref() {
            d = d.set_parent(w);
        }
        d.blocking_pick_file()
            .and_then(|f| f.as_path().map(|p| p.to_path_buf()))
    })
    .await
    .map_err(|e| e.to_string())
}

#[derive(Deserialize)]
pub struct LinkedCheck {
    pub id: Id,
    pub session: PathBuf,
    pub stamp: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkedState {
    pub id: Id,
    /// Saved since the clip was rendered.
    pub stale: bool,
    /// The session file is not there any more.
    pub missing: bool,
}

/// For each linked media file, whether its session was saved since it
/// was rendered (or is gone).
#[tauri::command]
pub fn linked_check(items: Vec<LinkedCheck>) -> Vec<LinkedState> {
    items
        .into_iter()
        .map(|c| match stamp(&c.session) {
            Some(now) => LinkedState {
                id: c.id,
                stale: now != c.stamp,
                missing: false,
            },
            None => LinkedState {
                id: c.id,
                stale: false,
                missing: true,
            },
        })
        .collect()
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    }
}

fn file_safe(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if "<>:\"/\\|?*".contains(c) || c.is_control() {
                ' '
            } else {
                c
            }
        })
        .collect();
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.is_empty() {
        "Linked Session".into()
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_saved_session_reads_as_changed() {
        let dir = std::env::temp_dir().join(format!("af-linked-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("inner.audfrm");
        std::fs::write(&p, "one").unwrap();
        let before = stamp(&p).unwrap();
        let check = |s: &str| {
            linked_check(vec![LinkedCheck {
                id: Id::nil(),
                session: p.clone(),
                stamp: s.into(),
            }])
            .remove(0)
        };
        assert!(!check(&before).stale);
        std::fs::write(&p, "one, and more").unwrap();
        assert!(check(&before).stale);
        std::fs::remove_file(&p).unwrap();
        assert!(check(&before).missing);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn names_become_safe_file_names() {
        assert_eq!(file_safe("Beat: v2/final"), "Beat v2 final");
        assert_eq!(file_safe("  "), "Linked Session");
    }
}
