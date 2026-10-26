// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Keyboard shortcuts, saved as `.alkeys`.
//!
//! The shipped defaults live in `src/keymap/default.json` in the same
//! format. The user's file in the app config folder holds only the
//! actions they changed; an empty list means "no shortcut".

use std::collections::BTreeMap;
use std::path::Path;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::document::{self, Document, DocumentError};

/// File name of the user's shortcut changes inside the app config folder.
pub const USER_KEYMAP_FILE: &str = "keymap.alkeys";

/// A set of shortcut bindings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Keymap {
    /// Always `altaudio.keymap`.
    #[schemars(extend("const" = "altaudio.keymap"))]
    pub format: String,
    pub schema_version: u32,
    /// Action id to its shortcuts, for example `"edit.undo": ["Ctrl+Z"]`.
    pub bindings: BTreeMap<String, Vec<String>>,
}

impl Document for Keymap {
    const FORMAT: &'static str = "altaudio.keymap";
    const CURRENT_VERSION: u32 = 1;
    const SCHEMA_FILE: &'static str = "keymap.schema.json";
}

impl Keymap {
    pub fn new(bindings: BTreeMap<String, Vec<String>>) -> Self {
        Self {
            format: Self::FORMAT.into(),
            schema_version: Self::CURRENT_VERSION,
            bindings,
        }
    }
}

/// Loads the user's shortcut changes. Empty when none are saved.
pub fn load_user_keymap(dir: &Path) -> Result<BTreeMap<String, Vec<String>>, DocumentError> {
    let path = dir.join(USER_KEYMAP_FILE);
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    Ok(document::load::<Keymap>(&path)?.bindings)
}

/// Saves the user's shortcut changes. Saving no changes deletes the file.
pub fn save_user_keymap(
    dir: &Path,
    bindings: BTreeMap<String, Vec<String>>,
) -> Result<(), DocumentError> {
    let path = dir.join(USER_KEYMAP_FILE);
    if bindings.is_empty() {
        return match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(DocumentError::Io { path, source }),
        };
    }
    std::fs::create_dir_all(dir).map_err(|source| DocumentError::Io {
        path: dir.to_path_buf(),
        source,
    })?;
    document::save(&Keymap::new(bindings), &path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_user_keymap_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load_user_keymap(dir.path()).unwrap().is_empty());
    }

    #[test]
    fn user_keymap_roundtrips_and_clears() {
        let dir = tempfile::tempdir().unwrap();
        let mut b = BTreeMap::new();
        b.insert("help.about".to_string(), vec!["F1".to_string()]);
        b.insert("edit.undo".to_string(), vec![]);
        save_user_keymap(dir.path(), b.clone()).unwrap();
        assert_eq!(load_user_keymap(dir.path()).unwrap(), b);

        save_user_keymap(dir.path(), BTreeMap::new()).unwrap();
        assert!(!dir.path().join(USER_KEYMAP_FILE).exists());
    }

    #[test]
    fn shipped_default_keymap_is_a_valid_document() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("src")
            .join("keymap")
            .join("default.json");
        let km: Keymap = document::load(&path).unwrap();
        assert_eq!(km.bindings["edit.undo"], vec!["Ctrl+Z"]);
    }
}
