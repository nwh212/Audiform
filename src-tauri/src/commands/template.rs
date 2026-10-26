// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Session templates: a saved session (tracks, levels, clips such as a
//! show's intro and outro, markers, libraries) that new sessions start
//! from. Stored as `.altemplate` files in the app's config folder.

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::async_runtime::spawn_blocking;
use tauri::{AppHandle, Manager, State};

use super::session::OpenSession;
use crate::export::mixdown::unique_path;
use crate::library::Template;
use crate::session::document;
use crate::session::model::{new_id, Session};

pub const TEMPLATE_EXTENSION: &str = "altemplate";

#[derive(Debug, Clone, Serialize)]
pub struct TemplateEntry {
    pub path: PathBuf,
    pub name: String,
    pub description: String,
    pub tracks: usize,
    pub clips: usize,
    pub sample_rate: u32,
}

pub fn templates_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_config_dir()
        .map_err(|e| e.to_string())?
        .join("templates"))
}

fn entry(path: &Path, t: &Template) -> TemplateEntry {
    TemplateEntry {
        path: path.to_path_buf(),
        name: t.name.clone(),
        description: t.description.clone(),
        tracks: t.session.tracks.len(),
        clips: t.session.tracks.iter().map(|t| t.clips.len()).sum(),
        sample_rate: t.session.sample_rate,
    }
}

/// Every template in `dir`, sorted by name. Unreadable files are skipped.
pub fn list_in(dir: &Path) -> Vec<TemplateEntry> {
    let mut out: Vec<TemplateEntry> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == TEMPLATE_EXTENSION))
        .filter_map(|p| document::load::<Template>(&p).ok().map(|t| entry(&p, &t)))
        .collect();
    out.sort_by_key(|e| e.name.to_lowercase());
    out
}

/// Saves `session` as a template named `name` in `dir`.
pub fn save_in(
    dir: &Path,
    name: &str,
    description: &str,
    session: &Session,
) -> Result<TemplateEntry, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut s = session.clone();
    // A template starts at the beginning with nothing selected.
    s.view.playhead = 0;
    s.view.scroll = 0;
    s.view.selection = None;
    s.template_id = None;
    let mut t = Template::from_session(name.trim(), s);
    t.description = description.trim().to_string();
    let path = unique_path(dir, name.trim(), TEMPLATE_EXTENSION);
    document::save(&t, &path).map_err(|e| e.to_string())?;
    Ok(entry(&path, &t))
}

/// A new session from the template at `path`: its own id, the given
/// name, and a note of which template it came from.
pub fn session_from(path: &Path, name: &str) -> Result<Session, String> {
    let t: Template =
        document::load(path).map_err(|e| format!("Could not read the template: {e}"))?;
    let mut s = t.session;
    s.id = new_id();
    s.name = name.to_string();
    s.template_id = Some(t.id);
    let now = chrono::Utc::now();
    s.created = now;
    s.modified = now;
    Ok(s)
}

#[tauri::command]
pub async fn template_list(app: AppHandle) -> Result<Vec<TemplateEntry>, String> {
    let dir = templates_dir(&app)?;
    spawn_blocking(move || list_in(&dir))
        .await
        .map_err(|e| e.to_string())
}

/// File > Save as Session Template: the open session as a template.
#[tauri::command]
pub async fn template_save(
    app: AppHandle,
    open: State<'_, OpenSession>,
    name: String,
    description: Option<String>,
) -> Result<TemplateEntry, String> {
    if name.trim().is_empty() {
        return Err("Give the template a name.".into());
    }
    let session = open.0.lock().map_err(|e| e.to_string())?.session.clone();
    let dir = templates_dir(&app)?;
    spawn_blocking(move || save_in(&dir, &name, description.as_deref().unwrap_or(""), &session))
        .await
        .map_err(|e| e.to_string())?
}

/// Deletes a template. Only files in the templates folder.
#[tauri::command]
pub async fn template_delete(app: AppHandle, path: PathBuf) -> Result<(), String> {
    let dir = templates_dir(&app)?;
    if path.parent() != Some(dir.as_path())
        || path.extension().is_none_or(|x| x != TEMPLATE_EXTENSION)
    {
        return Err("That is not a saved template.".into());
    }
    spawn_blocking(move || std::fs::remove_file(&path).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::model::{ChannelLayout, Track};

    #[test]
    fn save_list_and_start_a_session_from_a_template() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = Session::new("Show", 44_100);
        s.tracks.push(Track::new("Host", ChannelLayout::Mono));
        s.tracks.push(Track::new("Guest", ChannelLayout::Mono));
        s.view.playhead = 9_999;
        let e = save_in(dir.path(), "Weekly Show", "Two mics", &s).unwrap();
        assert_eq!(e.tracks, 2);
        let again = save_in(dir.path(), "Weekly Show", "", &s).unwrap();
        assert_ne!(e.path, again.path, "never overwrites");

        let list = list_in(dir.path());
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "Weekly Show");
        assert_eq!(list[0].sample_rate, 44_100);

        let fresh = session_from(&e.path, "Episode 12").unwrap();
        assert_eq!(fresh.name, "Episode 12");
        assert_ne!(fresh.id, s.id);
        assert_eq!(fresh.tracks.len(), 2);
        assert_eq!(fresh.view.playhead, 0);
        assert!(fresh.template_id.is_some());
    }
}
