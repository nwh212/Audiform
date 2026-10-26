// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Libraries and session templates.
//!
//! A library is a folder holding a `library.json` index plus the audio
//! files it owns. Items are copied into the folder by default so a
//! library stays whole when it moves or syncs.

pub mod store;
pub mod template;

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::session::document::Document;
use crate::session::model::{new_id, Id};

pub use template::Template;

/// File name of the index inside a library folder.
pub const LIBRARY_INDEX: &str = "library.json";

/// A library of reusable assets: promos, intros, outros, beds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Library {
    /// Always `altaudio.library`.
    #[schemars(extend("const" = "altaudio.library"))]
    pub format: String,
    pub schema_version: u32,

    pub id: Id,
    pub name: String,
    /// Scaffolding (P7): show library or network-wide library.
    pub scope: LibraryScope,
    pub created: DateTime<Utc>,
    pub items: Vec<LibraryItem>,
    /// Named groups of items (shown as bins). An item can be in several.
    pub collections: Vec<Collection>,
    /// Show color for the library's badge, as a palette token or `#rrggbb`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

impl Document for Library {
    const FORMAT: &'static str = "altaudio.library";
    const CURRENT_VERSION: u32 = 1;
    const SCHEMA_FILE: &'static str = "library.schema.json";
}

impl Library {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            format: Self::FORMAT.into(),
            schema_version: Self::CURRENT_VERSION,
            id: new_id(),
            name: name.into(),
            scope: LibraryScope::Show,
            created: Utc::now(),
            items: Vec::new(),
            collections: Vec::new(),
            color: None,
        }
    }

    /// Case-insensitive search across name, tags, and notes.
    pub fn search<'a>(&'a self, query: &str) -> Vec<&'a LibraryItem> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return self.items.iter().collect();
        }
        self.items
            .iter()
            .filter(|item| {
                item.name.to_lowercase().contains(&q)
                    || item.notes.to_lowercase().contains(&q)
                    || item.tags.iter().any(|t| t.to_lowercase().contains(&q))
            })
            .collect()
    }
}

/// Who a library serves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LibraryScope {
    /// Assets for one show.
    #[default]
    Show,
    /// Scaffolding (P7): shared across every show on the network, such
    /// as bumpers, promos, and legal reads.
    Network,
}

/// A named group of library items.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Collection {
    pub id: Id,
    pub name: String,
    /// Item ids, in display order.
    pub items: Vec<Id>,
    /// Bin color as a palette token or `#rrggbb`. None is neutral.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

/// What a library item holds. More kinds can be added without a
/// version bump.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LibraryItemKind {
    #[default]
    Audio,
}

/// One asset in a library.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LibraryItem {
    pub id: Id,
    pub name: String,
    pub kind: LibraryItemKind,
    /// Path inside the library folder when copied in, or an absolute
    /// path when linked.
    pub file: PathBuf,
    /// True when the file was copied into the library folder (the
    /// default). False when the item links to a file elsewhere.
    pub copied: bool,
    /// Where the file came from before it was copied in.
    pub original_path: Option<PathBuf>,
    pub sample_rate: u32,
    pub channels: u16,
    /// Length in samples at the file's own rate.
    pub length: u64,
    pub tags: Vec<String>,
    pub notes: String,
    pub color: Option<String>,
    pub added: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(name: &str, tags: &[&str], notes: &str) -> LibraryItem {
        LibraryItem {
            id: new_id(),
            name: name.into(),
            kind: LibraryItemKind::Audio,
            file: format!("{name}.wav").into(),
            copied: true,
            original_path: None,
            sample_rate: 48_000,
            channels: 2,
            length: 48_000,
            tags: tags.iter().map(|t| t.to_string()).collect(),
            notes: notes.into(),
            color: None,
            added: Utc::now(),
        }
    }

    #[test]
    fn search_matches_name_tags_and_notes() {
        let mut lib = Library::new("Promos");
        lib.items.push(item("Show Intro", &["intro"], ""));
        lib.items.push(item("Guest promo", &["promo", "guest"], ""));
        lib.items.push(item("Bed 01", &[], "use under the intro"));

        assert_eq!(lib.search("intro").len(), 2);
        assert_eq!(lib.search("GUEST").len(), 1);
        assert_eq!(lib.search("  ").len(), 3);
        assert!(lib.search("nothing").is_empty());
    }
}
