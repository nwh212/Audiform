// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Panel layouts, saved as `.alworkspace`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::document::Document;

/// A saved panel layout.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Workspace {
    /// Always `altaudio.workspace`.
    #[schemars(extend("const" = "altaudio.workspace"))]
    pub format: String,
    pub schema_version: u32,

    /// Stable slug, for example `default` or `podcast-edit`.
    pub id: String,
    pub name: String,
    /// Ships with the app. Built-in workspaces can be reset but not
    /// deleted.
    pub built_in: bool,
    /// Serialized docking layout from the frontend. The core stores it
    /// as-is and never interprets it.
    pub layout: Value,
}

impl Document for Workspace {
    const FORMAT: &'static str = "altaudio.workspace";
    const CURRENT_VERSION: u32 = 1;
    const SCHEMA_FILE: &'static str = "workspace.schema.json";
}

impl Workspace {
    pub fn new(id: impl Into<String>, name: impl Into<String>, layout: Value) -> Self {
        Self {
            format: Self::FORMAT.into(),
            schema_version: Self::CURRENT_VERSION,
            id: id.into(),
            name: name.into(),
            built_in: false,
            layout,
        }
    }
}

/// File name of the user's layout inside the app config folder.
pub const USER_WORKSPACE_FILE: &str = "workspace.alworkspace";

/// Loads the saved layout from `dir`. Returns `Ok(None)` when nothing
/// has been saved yet.
pub fn load_user_layout(dir: &std::path::Path) -> Result<Option<Value>, super::DocumentError> {
    let path = dir.join(USER_WORKSPACE_FILE);
    if !path.exists() {
        return Ok(None);
    }
    let workspace: Workspace = super::document::load(&path)?;
    Ok(Some(workspace.layout))
}

/// Saves the layout to `dir` as the Default workspace, creating the
/// folder if needed.
pub fn save_user_layout(dir: &std::path::Path, layout: Value) -> Result<(), super::DocumentError> {
    std::fs::create_dir_all(dir).map_err(|source| super::DocumentError::Io {
        path: dir.to_path_buf(),
        source,
    })?;
    let mut workspace = Workspace::new("default", "Default", layout);
    workspace.built_in = true;
    super::document::save(&workspace, &dir.join(USER_WORKSPACE_FILE))
}

/// Deletes the saved layout so the next launch uses the Default layout.
pub fn clear_user_layout(dir: &std::path::Path) -> Result<(), super::DocumentError> {
    let path = dir.join(USER_WORKSPACE_FILE);
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(super::DocumentError::Io { path, source }),
    }
}

/// Folder for named workspaces inside the app config folder.
pub const WORKSPACES_DIR: &str = "workspaces";
const EXT: &str = "alworkspace";

/// A saved workspace without its layout, for menus.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceInfo {
    pub id: String,
    pub name: String,
    pub built_in: bool,
}

/// Ids become file names, so only letters, digits and dashes are kept.
pub fn safe_id(id: &str) -> Option<String> {
    let s: String = id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect::<String>()
        .to_ascii_lowercase();
    (!s.is_empty() && s.len() <= 64).then_some(s)
}

fn named_path(config: &std::path::Path, id: &str) -> Option<std::path::PathBuf> {
    safe_id(id).map(|s| config.join(WORKSPACES_DIR).join(format!("{s}.{EXT}")))
}

/// Moves the single layout file from before named workspaces into the
/// workspaces folder as `default`. Safe to call every time.
pub fn migrate_single(config: &std::path::Path) {
    let old = config.join(USER_WORKSPACE_FILE);
    let Some(new) = named_path(config, "default") else {
        return;
    };
    if old.exists() && !new.exists() {
        if let Some(parent) = new.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::rename(&old, &new);
    }
}

/// Every saved workspace, by name. Unreadable files are skipped.
pub fn list_named(config: &std::path::Path) -> Vec<WorkspaceInfo> {
    migrate_single(config);
    let Ok(dir) = std::fs::read_dir(config.join(WORKSPACES_DIR)) else {
        return Vec::new();
    };
    let mut out: Vec<WorkspaceInfo> = dir
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == EXT))
        .filter_map(|e| super::document::load::<Workspace>(&e.path()).ok())
        .map(|w| WorkspaceInfo {
            id: w.id,
            name: w.name,
            built_in: w.built_in,
        })
        .collect();
    out.sort_by_key(|w| w.name.to_lowercase());
    out
}

pub fn load_named(
    config: &std::path::Path,
    id: &str,
) -> Result<Option<Workspace>, super::DocumentError> {
    migrate_single(config);
    let Some(path) = named_path(config, id) else {
        return Ok(None);
    };
    if !path.exists() {
        return Ok(None);
    }
    super::document::load(&path).map(Some)
}

pub fn save_named(
    config: &std::path::Path,
    mut workspace: Workspace,
) -> Result<(), super::DocumentError> {
    let path = named_path(config, &workspace.id).ok_or_else(|| super::DocumentError::Io {
        path: config.to_path_buf(),
        source: std::io::Error::new(std::io::ErrorKind::InvalidInput, "bad workspace id"),
    })?;
    workspace.id = safe_id(&workspace.id).unwrap_or_default();
    let dir = config.join(WORKSPACES_DIR);
    std::fs::create_dir_all(&dir)
        .map_err(|source| super::DocumentError::Io { path: dir, source })?;
    super::document::save(&workspace, &path)
}

pub fn delete_named(config: &std::path::Path, id: &str) -> Result<(), super::DocumentError> {
    let Some(path) = named_path(config, id) else {
        return Ok(());
    };
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(super::DocumentError::Io { path, source }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_layout_loads_as_none() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load_user_layout(dir.path()).unwrap(), None);
    }

    #[test]
    fn layout_roundtrips_through_a_new_folder() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("config").join("app");
        let layout = serde_json::json!({ "grid": { "root": {} }, "panels": { "files": {} } });
        save_user_layout(&nested, layout.clone()).unwrap();
        assert_eq!(load_user_layout(&nested).unwrap(), Some(layout));
    }

    #[test]
    fn corrupt_layout_is_an_error_not_a_crash() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(USER_WORKSPACE_FILE), "{ broken").unwrap();
        assert!(load_user_layout(dir.path()).is_err());
    }

    #[test]
    fn clearing_removes_the_file_and_is_safe_twice() {
        let dir = tempfile::tempdir().unwrap();
        save_user_layout(dir.path(), serde_json::json!({})).unwrap();
        clear_user_layout(dir.path()).unwrap();
        clear_user_layout(dir.path()).unwrap();
        assert_eq!(load_user_layout(dir.path()).unwrap(), None);
    }

    #[test]
    fn named_workspaces_save_list_load_delete() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path();
        save_user_layout(cfg, serde_json::json!({ "old": true })).unwrap();
        // The old single layout becomes the Default workspace.
        let list = list_named(cfg);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "default");
        assert!(!cfg.join(USER_WORKSPACE_FILE).exists());

        let w = Workspace::new("My Mix!", "My Mix", serde_json::json!({ "a": 1 }));
        save_named(cfg, w).unwrap();
        let got = load_named(cfg, "mymix").unwrap().unwrap();
        assert_eq!(got.name, "My Mix");
        assert_eq!(got.id, "mymix");
        assert_eq!(list_named(cfg).len(), 2);
        delete_named(cfg, "mymix").unwrap();
        delete_named(cfg, "mymix").unwrap();
        assert_eq!(load_named(cfg, "mymix").unwrap(), None);
        assert_eq!(safe_id("../../etc"), Some("etc".into()));
        assert_eq!(safe_id("///"), None);
    }
}
