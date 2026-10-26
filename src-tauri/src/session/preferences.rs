// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Small interface settings that should survive a restart, such as the
//! Drive Manager's last folder. Saved as `preferences.alprefs` in the
//! app config folder, never in browser storage.
//!
//! Values are free-form JSON keyed by a dotted name
//! (`drive-manager.folder`). Each panel owns its keys and must accept a
//! missing or malformed value by falling back to its default.

use std::collections::BTreeMap;
use std::path::Path;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::document::{self, Document, DocumentError};

pub const PREFERENCES_FILE: &str = "preferences.alprefs";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Preferences {
    /// Always `altaudio.preferences`.
    #[schemars(extend("const" = "altaudio.preferences"))]
    pub format: String,
    pub schema_version: u32,
    pub values: BTreeMap<String, Value>,
}

impl Document for Preferences {
    const FORMAT: &'static str = "altaudio.preferences";
    const CURRENT_VERSION: u32 = 1;
    const SCHEMA_FILE: &'static str = "preferences.schema.json";
}

impl Preferences {
    pub fn new(values: BTreeMap<String, Value>) -> Self {
        Self {
            format: Self::FORMAT.into(),
            schema_version: Self::CURRENT_VERSION,
            values,
        }
    }
}

/// Loads saved preferences. Empty when none are saved or the file is unreadable,
/// since losing a remembered folder is never worth an error dialog.
pub fn load_preferences(dir: &Path) -> BTreeMap<String, Value> {
    let path = dir.join(PREFERENCES_FILE);
    if !path.exists() {
        return BTreeMap::new();
    }
    document::load::<Preferences>(&path)
        .map(|p| p.values)
        .unwrap_or_default()
}

pub fn save_preferences(dir: &Path, values: BTreeMap<String, Value>) -> Result<(), DocumentError> {
    std::fs::create_dir_all(dir).map_err(|source| DocumentError::Io {
        path: dir.to_path_buf(),
        source,
    })?;
    document::save(&Preferences::new(values), &dir.join(PREFERENCES_FILE))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_and_loads() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load_preferences(dir.path()).is_empty());
        let mut v = BTreeMap::new();
        v.insert("drive-manager.folder".to_string(), Value::from("C:\\Shows"));
        save_preferences(dir.path(), v.clone()).unwrap();
        assert_eq!(load_preferences(dir.path()), v);
    }

    #[test]
    fn unreadable_file_means_defaults() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(PREFERENCES_FILE), b"{ not json").unwrap();
        assert!(load_preferences(dir.path()).is_empty());
    }
}
