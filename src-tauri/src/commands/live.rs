// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

//! Unsaved Waveform editor edits, heard in the session.
//!
//! While a file open in the Waveform editor has edits not saved yet, the
//! edit list is rendered to a copy in the app's cache folder (a moment
//! after each edit), and the session plays, follows and exports that
//! copy in place of the file. Saving, going back to the saved state or
//! closing the file drops the copy, so the session hears the file itself
//! again. The file on disk is never touched, and the session's own
//! record of its media never changes: the swap is made on a copy of the
//! session each time it plays or exports.

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};

use serde::Serialize;
use tauri::async_runtime::spawn_blocking;
use tauri::{AppHandle, State};

use super::session::OpenSession;
use super::wave::{OpenWave, Playing, PlayingDoc};
use crate::engine::Transport;
use crate::export::encode::{FormatKind, OutputFormat};
use crate::export::mixdown::{self, MixdownRequest};
use crate::session::model::Session;

/// A file's live copy and the copy's length in samples.
#[derive(Debug, Clone)]
struct Copy {
    path: PathBuf,
    length: u64,
}

/// Live copies by the file they stand in for.
static LIVE: LazyLock<Mutex<HashMap<String, Copy>>> = LazyLock::new(|| Mutex::new(HashMap::new()));
/// Numbers the copies, so a new render never overwrites one being played.
static NEXT: AtomicU64 = AtomicU64::new(1);

/// The same file however its path is spelled (case does not matter on Windows).
fn key(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/");
    if cfg!(windows) {
        s.to_lowercase()
    } else {
        s
    }
}

/// The session as it should be heard: files with unsaved Waveform editor
/// edits replaced by their live copies, and pre-rendered tracks playing
/// their renders (see `prerender`). Unchanged when neither applies.
pub fn heard(s: &Session) -> Cow<'_, Session> {
    match files_heard(s) {
        Cow::Borrowed(b) => super::prerender::heard(b),
        Cow::Owned(o) => Cow::Owned(super::prerender::heard(&o).into_owned()),
    }
}

/// The session with only the live copies swapped in.
pub fn files_heard(s: &Session) -> Cow<'_, Session> {
    let live = LIVE.lock().map(|m| m.clone()).unwrap_or_default();
    if live.is_empty() || !s.media.iter().any(|m| live.contains_key(&key(&m.path))) {
        return Cow::Borrowed(s);
    }
    let mut out = s.clone();
    for m in &mut out.media {
        if let Some(c) = live.get(&key(&m.path)) {
            m.path = c.path.clone();
            m.relative_path = None;
            m.length = c.length;
        }
    }
    Cow::Owned(out)
}

/// What the interface needs about a live copy: its file, to draw it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveCopy {
    pub original: PathBuf,
    pub path: PathBuf,
    pub length: u64,
}

/// Keeps the session's playback in step after a copy came or went.
fn resync(
    open: &State<'_, OpenSession>,
    transport: &State<'_, Transport>,
    playing: &State<'_, Playing>,
) {
    if !playing.is(PlayingDoc::Session) {
        return;
    }
    if let Ok(s) = open.0.lock() {
        if let Err(e) = transport.sync(&heard(&s.session)) {
            eprintln!("playback could not follow the Waveform editor: {e}");
        }
    }
}

fn forget(original: &Path) {
    let old = LIVE.lock().ok().and_then(|mut m| m.remove(&key(original)));
    if let Some(c) = old {
        let _ = std::fs::remove_file(c.path);
    }
}

/// Renders the Waveform editor's file, edits and all, to a live copy the
/// session plays in its place. None for a file not saved anywhere yet.
#[tauri::command]
pub async fn wave_live_render(
    app: AppHandle,
    wave: State<'_, OpenWave>,
    open: State<'_, OpenSession>,
    transport: State<'_, Transport>,
    playing: State<'_, Playing>,
) -> Result<Option<LiveCopy>, String> {
    let (session, original) = {
        let w = wave.0.lock().map_err(|e| e.to_string())?;
        let Some(state) = w.as_ref() else {
            return Ok(None);
        };
        if state.untitled {
            return Ok(None);
        }
        // A previewed chain is not part of the file until it is applied.
        (
            super::wave::without_effects(state.session.clone()),
            state.path.clone(),
        )
    };
    let dir = super::cache::cache_root(&app)?.join("live");
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let stem = original
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "audio".into());
    let path = dir.join(format!("{stem}.live{n}.wav"));
    let target = path.clone();
    let length = spawn_blocking(move || -> Result<u64, String> {
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let channels = super::spectral::file_channels(&session);
        if channels > 2 {
            super::wave::save_multichannel(&session, &target, 32, &mut |_, _| {})?;
        } else {
            let request = MixdownRequest {
                clip_only: true,
                channels: Some(channels as u16),
                format: Some(OutputFormat {
                    kind: FormatKind::Wav,
                    bit_depth: Some(32),
                    channels: Some(channels as u16),
                    ..Default::default()
                }),
                ..Default::default()
            };
            mixdown::mixdown(
                &session,
                &request,
                &target,
                &AtomicBool::new(false),
                |_, _| {},
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(crate::codecs::probe(&target)
            .map_err(|e| e.to_string())?
            .length)
    })
    .await
    .map_err(|e| e.to_string())??;
    forget(&original);
    LIVE.lock().map_err(|e| e.to_string())?.insert(
        key(&original),
        Copy {
            path: path.clone(),
            length,
        },
    );
    resync(&open, &transport, &playing);
    Ok(Some(LiveCopy {
        original,
        path,
        length,
    }))
}

/// Drops a file's live copy: the session hears the file itself again.
#[tauri::command]
pub fn wave_live_clear(
    open: State<'_, OpenSession>,
    transport: State<'_, Transport>,
    playing: State<'_, Playing>,
    original: PathBuf,
) -> Result<(), String> {
    forget(&original);
    resync(&open, &transport, &playing);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::model::{new_id, MediaRef};

    #[test]
    fn the_session_hears_live_copies_without_changing() {
        let mut s = Session::new("s", 48_000);
        s.media.push(MediaRef {
            id: new_id(),
            path: PathBuf::from("/music/take.wav"),
            relative_path: Some(PathBuf::from("take.wav")),
            sample_rate: 48_000,
            channels: 2,
            length: 1_000,
            linked: None,
        });
        assert!(matches!(heard(&s), Cow::Borrowed(_)));
        LIVE.lock().unwrap().insert(
            key(Path::new("/music/take.wav")),
            Copy {
                path: PathBuf::from("/cache/live/take.live1.wav"),
                length: 800,
            },
        );
        let h = heard(&s);
        assert_eq!(h.media[0].path, PathBuf::from("/cache/live/take.live1.wav"));
        assert_eq!(h.media[0].length, 800);
        assert_eq!(s.media[0].path, PathBuf::from("/music/take.wav"));
        LIVE.lock().unwrap().clear();
        assert!(matches!(heard(&s), Cow::Borrowed(_)));
    }
}
