// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

//! The open session, owned by the core. The interface sends edits as
//! ops and gets back the new session plus the op that undoes the edit.
//! Also new, open, save, autosave and crash recovery.

use std::path::PathBuf;
use std::sync::Mutex;

use serde::Serialize;
use tauri::async_runtime::spawn_blocking;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;

use crate::session::file::{self, Opened, Recoverable, EXTENSION};
use crate::session::model::{MediaRef, Session, SessionView};
use crate::session::ops::{self, Op};

pub struct OpenState {
    pub session: Session,
    /// Where the session was last saved or opened from.
    pub path: Option<PathBuf>,
}

/// The open session. One at a time in P1.
pub struct OpenSession(pub Mutex<OpenState>);

impl Default for OpenSession {
    fn default() -> Self {
        Self(Mutex::new(OpenState {
            session: fresh_session("Untitled Session".into(), 48_000),
            path: None,
        }))
    }
}

/// File > New Session choices. Everything is optional.
#[derive(Debug, Default, serde::Deserialize)]
pub struct NewSessionOptions {
    pub name: Option<String>,
    pub sample_rate: Option<u32>,
    pub bit_depth: Option<u16>,
    /// Mix to mono instead of stereo.
    pub mono: Option<bool>,
    /// Start from this template (its rate, depth and mix win).
    pub template: Option<PathBuf>,
    /// Create the session in its own folder here and save it at once.
    pub folder: Option<PathBuf>,
}

/// Default folder for new sessions: Documents/Altered Audio/Sessions.
#[tauri::command]
pub fn session_default_folder(app: AppHandle) -> Result<PathBuf, String> {
    let paths = app.path();
    let docs = paths
        .document_dir()
        .or_else(|_| paths.home_dir().map(|h| h.join("Documents")))
        .map_err(|e| e.to_string())?;
    Ok(docs.join("Altered Audio").join("Sessions"))
}

/// The first "`base` N" (N from 1) with no session folder of that name in
/// `folder` yet, for New Session's suggested name.
#[tauri::command]
pub fn session_next_name(folder: PathBuf, base: String) -> String {
    next_free_name(&folder, &base)
}

fn next_free_name(folder: &std::path::Path, base: &str) -> String {
    (1..10_000)
        .map(|n| format!("{base} {n}"))
        .find(|name| !folder.join(name).exists())
        .unwrap_or_else(|| base.to_string())
}

/// Empty tracks a new session starts with, so files can go straight onto a track.
pub const NEW_SESSION_TRACKS: usize = 6;

/// A new, empty session with `NEW_SESSION_TRACKS` stereo tracks named Track 1, Track 2, and so on.
pub fn fresh_session(name: String, sample_rate: u32) -> Session {
    use crate::session::model::{ChannelLayout, Track};
    let mut s = Session::new(name, sample_rate);
    for i in 1..=NEW_SESSION_TRACKS {
        s.tracks
            .push(Track::new(format!("Track {i}"), ChannelLayout::Stereo));
    }
    s
}

#[derive(Debug, Serialize)]
pub struct Applied {
    pub session: Session,
    pub inverse: Op,
}

#[derive(Debug, Serialize)]
pub struct SessionWithPath {
    pub session: Session,
    pub path: Option<PathBuf>,
    /// Media ids not found on disk (open and recover only).
    pub missing: Vec<crate::session::model::Id>,
}

fn autosave_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("autosave"))
}

#[tauri::command]
pub fn session_get(open: State<'_, OpenSession>) -> Result<SessionWithPath, String> {
    let s = open.0.lock().map_err(|e| e.to_string())?;
    Ok(SessionWithPath {
        session: s.session.clone(),
        path: s.path.clone(),
        missing: Vec::new(),
    })
}

#[tauri::command]
pub fn session_apply(
    open: State<'_, OpenSession>,
    transport: State<'_, crate::engine::Transport>,
    playing: State<'_, super::wave::Playing>,
    op: Op,
) -> Result<Applied, String> {
    let mut s = open.0.lock().map_err(|e| e.to_string())?;
    let inverse = ops::apply(&mut s.session, &op).map_err(|e| e.to_string())?;
    // Plug-ins taken out of the session stop running.
    crate::plugins::instance::retain(&crate::commands::plugins::plugin_slots(&s.session));
    // Playback follows edits: gains at once, timing changes by restarting.
    // Not while the Waveform editor's file is the one playing.
    if playing.is(super::wave::PlayingDoc::Session) {
        if let Err(e) = transport.sync(&s.session) {
            eprintln!("playback could not follow the edit: {e}");
            transport.stop();
        }
    }
    Ok(Applied {
        session: s.session.clone(),
        inverse,
    })
}

/// Zoom, scroll and playhead. Not an edit.
#[tauri::command]
pub fn session_set_view(open: State<'_, OpenSession>, view: SessionView) -> Result<(), String> {
    let mut s = open.0.lock().map_err(|e| e.to_string())?;
    ops::set_view(&mut s.session, view);
    Ok(())
}

/// Starts a new, empty session. The interface asks about unsaved
/// changes first.
#[tauri::command]
pub fn session_new(
    app: AppHandle,
    open: State<'_, OpenSession>,
    options: Option<NewSessionOptions>,
) -> Result<SessionWithPath, String> {
    let o = options.unwrap_or_default();
    let name = o
        .name
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .unwrap_or("Untitled Session")
        .to_string();
    let mut session = match &o.template {
        Some(t) => super::template::session_from(t, &name)?,
        None => {
            let mut s = fresh_session(
                name.clone(),
                o.sample_rate.unwrap_or(48_000).clamp(8_000, 384_000),
            );
            if let Some(b) = o.bit_depth.filter(|b| [16, 24, 32].contains(b)) {
                s.bit_depth = b;
            }
            if o.mono == Some(true) {
                s.master.channels = crate::session::model::ChannelLayout::Mono;
            }
            s
        }
    };
    // With a folder, the session gets its own folder and is saved there
    // right away, so merged clips and copies have a home from the start.
    let mut path = None;
    if let Some(folder) = &o.folder {
        let dir = folder.join(&name);
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("Could not create {}: {e}", dir.display()))?;
        let target = with_extension(dir.join(&name));
        if target.exists() {
            return Err(format!(
                "{} already exists. Pick another name or folder.",
                target.display()
            ));
        }
        session = file::save(&session, &target, &[]).map_err(|e| e.to_string())?;
        path = Some(target);
    }
    app.state::<crate::engine::Transport>().stop();
    let mut s = open.0.lock().map_err(|e| e.to_string())?;
    if let Ok(dir) = autosave_dir(&app) {
        file::clear_autosave(&dir, s.session.id);
    }
    s.session = session;
    s.path = path.clone();
    Ok(SessionWithPath {
        session: s.session.clone(),
        path,
        missing: Vec::new(),
    })
}

/// Saves to `path`, or to the current path when `path` is None.
/// `media` lists the Files panel, so files not yet on a track come back.
#[tauri::command]
pub async fn session_save(
    app: AppHandle,
    path: Option<PathBuf>,
    media: Vec<MediaRef>,
    libraries: Option<Vec<PathBuf>>,
    backups: Option<u32>,
) -> Result<SessionWithPath, String> {
    let new_path = path.is_some();
    let (mut session, target) = {
        let open = app.state::<OpenSession>();
        let s = open.0.lock().map_err(|e| e.to_string())?;
        let target = path
            .or_else(|| s.path.clone())
            .ok_or("Choose where to save the session first.")?;
        (s.session.clone(), target)
    };
    let target = with_extension(target);
    if let Some(libs) = libraries {
        session.libraries = libs;
    }
    // Save As, or the first save of an untitled session, names the
    // session after its file.
    if new_path || session.name == "Untitled Session" {
        if let Some(stem) = target.file_stem() {
            session.name = stem.to_string_lossy().into_owned();
        }
    }
    let t2 = target.clone();
    let keep = backups.unwrap_or(0).min(MAX_BACKUPS);
    let saved = spawn_blocking(move || {
        // The file about to be replaced becomes the newest backup.
        if keep > 0 {
            if let Err(e) = backup_previous(&t2, keep as usize, chrono::Local::now()) {
                eprintln!("session backup skipped: {e}");
            }
        }
        file::save(&session, &t2, &media)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;
    let open = app.state::<OpenSession>();
    let mut s = open.0.lock().map_err(|e| e.to_string())?;
    // Keep edits made while the file was being written; take the saved
    // media list and timestamps.
    s.session.media = saved.media.clone();
    s.session.modified = saved.modified;
    s.session.name = saved.name.clone();
    s.session.libraries = saved.libraries.clone();
    s.path = Some(target.clone());
    if let Ok(dir) = autosave_dir(&app) {
        file::clear_autosave(&dir, s.session.id);
    }
    Ok(SessionWithPath {
        session: s.session.clone(),
        path: Some(target),
        missing: Vec::new(),
    })
}

/// Most backups kept per session.
const MAX_BACKUPS: u32 = 50;

/// Copies the session file at `target` (if there is one) into a Backups
/// folder beside it, named with the time, and keeps only the newest `keep`.
fn backup_previous(
    target: &std::path::Path,
    keep: usize,
    now: chrono::DateTime<chrono::Local>,
) -> std::io::Result<()> {
    if !target.exists() {
        return Ok(());
    }
    let dir = target
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .join("Backups");
    std::fs::create_dir_all(&dir)?;
    let stem = target
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let name = format!("{stem} {}.{EXTENSION}", now.format("%Y-%m-%d %H%M%S"));
    std::fs::copy(target, dir.join(name))?;
    let prefix = format!("{stem} ");
    let suffix = format!(".{EXTENSION}");
    let mut ours: Vec<PathBuf> = std::fs::read_dir(&dir)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                n.strip_prefix(&prefix)
                    .and_then(|r| r.strip_suffix(&suffix))
                    // Only our own timestamped names: "2026-10-19 093000".
                    .is_some_and(|t| {
                        t.len() == 17
                            && t.chars()
                                .all(|c| c.is_ascii_digit() || c == '-' || c == ' ')
                    })
            })
        })
        .collect();
    // Timestamps sort by name; drop the oldest beyond `keep`.
    ours.sort();
    let extra = ours.len().saturating_sub(keep);
    for old in &ours[..extra] {
        std::fs::remove_file(old)?;
    }
    Ok(())
}

fn with_extension(p: PathBuf) -> PathBuf {
    if p.extension().and_then(|e| e.to_str()) == Some(EXTENSION) {
        p
    } else {
        let mut s = p.into_os_string();
        s.push(".");
        s.push(EXTENSION);
        PathBuf::from(s)
    }
}

#[tauri::command]
pub async fn session_open(app: AppHandle, path: PathBuf) -> Result<SessionWithPath, String> {
    let p = path.clone();
    let opened: Opened = spawn_blocking(move || file::open(&p))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    app.state::<crate::engine::Transport>().stop();
    let open = app.state::<OpenSession>();
    let mut s = open.0.lock().map_err(|e| e.to_string())?;
    if let Ok(dir) = autosave_dir(&app) {
        file::clear_autosave(&dir, s.session.id);
    }
    s.session = opened.session.clone();
    s.path = Some(path.clone());
    Ok(SessionWithPath {
        session: opened.session,
        path: Some(path),
        missing: opened.missing,
    })
}

/// Writes the crash-recovery copy of the open session.
#[tauri::command]
pub async fn session_autosave(app: AppHandle, media: Vec<MediaRef>) -> Result<(), String> {
    let dir = autosave_dir(&app)?;
    let (session, path) = {
        let open = app.state::<OpenSession>();
        let s = open.0.lock().map_err(|e| e.to_string())?;
        (s.session.clone(), s.path.clone())
    };
    spawn_blocking(move || file::autosave(&dir, &session, path.as_deref(), &media))
        .await
        .map_err(|e| e.to_string())?
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Removes the open session's crash-recovery copy, for example when the
/// user closes without saving.
#[tauri::command]
pub fn session_clear_autosave(app: AppHandle, open: State<'_, OpenSession>) -> Result<(), String> {
    let id = open.0.lock().map_err(|e| e.to_string())?.session.id;
    file::clear_autosave(&autosave_dir(&app)?, id);
    Ok(())
}

/// Autosaves left by a crash, newest first.
#[tauri::command]
pub fn session_recoverable(app: AppHandle) -> Result<Vec<Recoverable>, String> {
    Ok(file::find_recoverable(&autosave_dir(&app)?))
}

/// Opens an autosave as the current session. Its real path is kept so
/// Save writes to the original file.
#[tauri::command]
pub async fn session_recover(app: AppHandle, autosave: PathBuf) -> Result<SessionWithPath, String> {
    let dir = autosave_dir(&app)?;
    let found = file::find_recoverable(&dir)
        .into_iter()
        .find(|r| r.autosave == autosave)
        .ok_or("That recovery file is gone.")?;
    let p = autosave.clone();
    let opened = spawn_blocking(move || file::open(&p))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    app.state::<crate::engine::Transport>().stop();
    let open = app.state::<OpenSession>();
    let mut s = open.0.lock().map_err(|e| e.to_string())?;
    s.session = opened.session.clone();
    s.path = found.note.original_path.clone();
    Ok(SessionWithPath {
        session: opened.session,
        path: found.note.original_path,
        missing: opened.missing,
    })
}

#[tauri::command]
pub fn session_discard_recovery(
    app: AppHandle,
    session_id: crate::session::model::Id,
) -> Result<(), String> {
    file::clear_autosave(&autosave_dir(&app)?, session_id);
    Ok(())
}

/// Save dialog for a session. Returns None on cancel.
#[tauri::command]
pub async fn session_pick_save(app: AppHandle, name: String) -> Result<Option<PathBuf>, String> {
    let parent = app.get_webview_window("main");
    spawn_blocking(move || {
        let mut d = app
            .dialog()
            .file()
            .set_title("Save Session")
            .set_file_name(format!("{name}.{EXTENSION}"))
            .add_filter("Altered Audio session", &[EXTENSION]);
        if let Some(w) = parent.as_ref() {
            d = d.set_parent(w);
        }
        d.blocking_save_file()
            .and_then(|f| f.as_path().map(|p| p.to_path_buf()))
    })
    .await
    .map_err(|e| e.to_string())
}

/// Open dialog for sessions (and, from task 11, .sesx sessions).
#[tauri::command]
pub async fn session_pick_open(app: AppHandle, sesx: bool) -> Result<Option<PathBuf>, String> {
    let parent = app.get_webview_window("main");
    spawn_blocking(move || {
        let mut d = app.dialog().file();
        d = if sesx {
            d.set_title("Import Session")
                .add_filter("Multitrack session (.sesx)", &["sesx"])
        } else {
            d.set_title("Open Session")
                .add_filter("Altered Audio session", &[EXTENSION])
        };
        if let Some(w) = parent.as_ref() {
            d = d.set_parent(w);
        }
        d.blocking_pick_file()
            .and_then(|f| f.as_path().map(|p| p.to_path_buf()))
    })
    .await
    .map_err(|e| e.to_string())
}

/// File > Open: sessions, .sesx sessions and audio files in one
/// dialog. Several files can be picked. Returns an empty list on cancel.
#[tauri::command]
pub async fn file_pick_open(app: AppHandle) -> Result<Vec<PathBuf>, String> {
    let parent = app.get_webview_window("main");
    spawn_blocking(move || {
        let mut all: Vec<&str> = vec![EXTENSION, "sesx"];
        all.extend_from_slice(crate::codecs::SUPPORTED_EXTENSIONS);
        let mut d = app
            .dialog()
            .file()
            .set_title("Open")
            .add_filter("Sessions and audio files", &all)
            .add_filter("Altered Audio session", &[EXTENSION])
            .add_filter("Multitrack session (.sesx)", &["sesx"])
            .add_filter("Audio files", crate::codecs::SUPPORTED_EXTENSIONS);
        if let Some(w) = parent.as_ref() {
            d = d.set_parent(w);
        }
        d.blocking_pick_files()
            .unwrap_or_default()
            .iter()
            .filter_map(|f| f.as_path().map(|p| p.to_path_buf()))
            .collect()
    })
    .await
    .map_err(|e| e.to_string())
}

#[derive(Debug, Serialize)]
pub struct SesxImported {
    #[serde(flatten)]
    pub result: SessionWithPath,
    pub report: crate::sesx::ImportReport,
}

/// Imports a .sesx session as a new, unsaved session.
#[tauri::command]
pub async fn session_import_sesx(app: AppHandle, path: PathBuf) -> Result<SesxImported, String> {
    let imported = spawn_blocking(move || crate::sesx::import(&path))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    app.state::<crate::engine::Transport>().stop();
    let open = app.state::<OpenSession>();
    let mut s = open.0.lock().map_err(|e| e.to_string())?;
    if let Ok(dir) = autosave_dir(&app) {
        file::clear_autosave(&dir, s.session.id);
    }
    s.session = imported.session.clone();
    s.path = None;
    Ok(SesxImported {
        result: SessionWithPath {
            session: imported.session,
            path: None,
            missing: imported.missing,
        },
        report: imported.report,
    })
}

#[derive(Debug, Serialize)]
pub struct SesxExported {
    pub path: PathBuf,
    pub report: crate::sesx::export::SesxExportReport,
}

/// File > Export > Multitrack Session (.sesx): asks where, then writes a
/// copy of the open session in that format. The session itself is not
/// changed and stays tied to its own file. None when cancelled.
#[tauri::command]
pub async fn session_export_sesx(
    app: AppHandle,
    open: State<'_, OpenSession>,
) -> Result<Option<SesxExported>, String> {
    let (session, folder) = {
        let s = open.0.lock().map_err(|e| e.to_string())?;
        (
            s.session.clone(),
            s.path.as_ref().and_then(|p| p.parent()).map(PathBuf::from),
        )
    };
    let parent = app.get_webview_window("main");
    let name = session.name.clone();
    spawn_blocking(move || {
        let mut d = app
            .dialog()
            .file()
            .set_title("Export Multitrack Session (.sesx)")
            .set_file_name(format!("{name}.sesx"))
            .add_filter("Multitrack session (.sesx)", &["sesx"]);
        if let Some(f) = folder.filter(|f| f.is_dir()) {
            d = d.set_directory(f);
        }
        if let Some(w) = parent.as_ref() {
            d = d.set_parent(w);
        }
        let Some(path) = d
            .blocking_save_file()
            .and_then(|f| f.as_path().map(|p| p.to_path_buf()))
        else {
            return Ok(None);
        };
        let path = if path.extension().is_none() {
            path.with_extension("sesx")
        } else {
            path
        };
        let report = crate::sesx::export::export(&session, &path)
            .map_err(|e| format!("Could not write the file: {e}"))?;
        Ok(Some(SesxExported { path, report }))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_sessions_take_the_next_free_name() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            next_free_name(dir.path(), "Untitled Session"),
            "Untitled Session 1"
        );
        std::fs::create_dir(dir.path().join("Untitled Session 1")).unwrap();
        std::fs::create_dir(dir.path().join("Untitled Session 3")).unwrap();
        assert_eq!(
            next_free_name(dir.path(), "Untitled Session"),
            "Untitled Session 2"
        );
    }

    #[test]
    fn a_new_session_has_six_empty_tracks() {
        let s = fresh_session("New".into(), 44_100);
        assert_eq!(s.tracks.len(), NEW_SESSION_TRACKS);
        assert_eq!(s.tracks[0].name, "Track 1");
        assert_eq!(s.tracks[5].name, "Track 6");
        assert!(s.tracks.iter().all(|t| t.clips.is_empty()));
        assert_eq!(s.sample_rate, 44_100);
    }

    #[test]
    fn backups_keep_the_newest_few() {
        use chrono::TimeZone;
        let d = tempfile::tempdir().unwrap();
        let target = d.path().join("Show.altaudio");
        std::fs::write(&target, "v1").unwrap();
        for m in 0..5 {
            let t = chrono::Local
                .with_ymd_and_hms(2026, 10, 19, 9, m, 0)
                .unwrap();
            super::backup_previous(&target, 3, t).unwrap();
        }
        let dir = d.path().join("Backups");
        // Another session's backups in the same folder are left alone.
        std::fs::write(dir.join("Other 2026-01-01 000000.altaudio"), "x").unwrap();
        super::backup_previous(
            &target,
            3,
            chrono::Local
                .with_ymd_and_hms(2026, 10, 19, 10, 0, 0)
                .unwrap(),
        )
        .unwrap();
        let mut names: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(
            names,
            [
                "Other 2026-01-01 000000.altaudio",
                "Show 2026-10-19 090300.altaudio",
                "Show 2026-10-19 090400.altaudio",
                "Show 2026-10-19 100000.altaudio"
            ]
        );
        // No file yet: nothing to back up.
        super::backup_previous(&d.path().join("New.altaudio"), 3, chrono::Local::now()).unwrap();
    }
}
