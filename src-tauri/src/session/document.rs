// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Versioned JSON documents.
//!
//! Every file the app writes is a JSON object with two header fields:
//! `format` (what kind of file it is) and `schema_version` (which
//! revision of that format). Loading reads the header first, refuses
//! files from a newer app, and upgrades older files one version at a
//! time through [`Document::migrate_step`] before deserializing.
//!
//! Rule: never break an old file. Adding a field with a serde default
//! is not a version bump. Renaming, removing, or changing the meaning
//! of a field is, and it needs a migration step plus a fixture test.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;

/// Errors from loading or saving a document.
#[derive(Debug, thiserror::Error)]
pub enum DocumentError {
    #[error("file is not valid JSON: {0}")]
    Json(#[from] serde_json::Error),

    #[error("file is missing the `{0}` header field")]
    MissingHeader(&'static str),

    #[error("expected a {expected} file but found {found}")]
    WrongFormat {
        expected: &'static str,
        found: String,
    },

    #[error(
        "this {format} file uses version {found}, which is newer than this app supports \
         (up to {supported}). Update Audiform to open it."
    )]
    TooNew {
        format: &'static str,
        found: u32,
        supported: u32,
    },

    #[error("{format} version {found} is not a known version")]
    UnknownVersion { format: &'static str, found: u32 },

    #[error("could not upgrade {format} from version {from}: {reason}")]
    Migration {
        format: &'static str,
        from: u32,
        reason: String,
    },

    #[error("file error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// A top-level file format.
pub trait Document: Serialize + DeserializeOwned + JsonSchema {
    /// Value of the `format` header, for example `altaudio.session`.
    const FORMAT: &'static str;

    /// Version this build writes. Bump only with a migration step.
    const CURRENT_VERSION: u32;

    /// File name of the published JSON Schema in `/schemas`.
    const SCHEMA_FILE: &'static str;

    /// Upgrades a raw document from `from` to `from + 1`.
    ///
    /// The default has no steps, which is correct while
    /// `CURRENT_VERSION` is 1.
    fn migrate_step(value: Value, from: u32) -> Result<Value, DocumentError> {
        let _ = value;
        Err(DocumentError::Migration {
            format: Self::FORMAT,
            from,
            reason: "no migration step is defined".into(),
        })
    }

    /// Upgrades documents nested inside this one, such as the session
    /// inside a template. Runs after this document reaches the current
    /// version.
    fn upgrade_nested(value: &mut Value) -> Result<(), DocumentError> {
        let _ = value;
        Ok(())
    }
}

/// Reads and checks the header of a raw document.
fn read_header<T: Document>(value: &Value) -> Result<u32, DocumentError> {
    let format = value
        .get("format")
        .and_then(Value::as_str)
        .ok_or(DocumentError::MissingHeader("format"))?;
    if format != T::FORMAT {
        return Err(DocumentError::WrongFormat {
            expected: T::FORMAT,
            found: format.to_string(),
        });
    }

    let version = value
        .get("schema_version")
        .and_then(Value::as_u64)
        .ok_or(DocumentError::MissingHeader("schema_version"))?;
    let version = u32::try_from(version).map_err(|_| DocumentError::UnknownVersion {
        format: T::FORMAT,
        found: u32::MAX,
    })?;

    if version == 0 {
        return Err(DocumentError::UnknownVersion {
            format: T::FORMAT,
            found: 0,
        });
    }
    if version > T::CURRENT_VERSION {
        return Err(DocumentError::TooNew {
            format: T::FORMAT,
            found: version,
            supported: T::CURRENT_VERSION,
        });
    }
    Ok(version)
}

/// Brings a raw document up to the current version without
/// deserializing it.
pub fn upgrade<T: Document>(mut value: Value) -> Result<Value, DocumentError> {
    let mut version = read_header::<T>(&value)?;
    while version < T::CURRENT_VERSION {
        value = T::migrate_step(value, version)?;
        version += 1;
        if let Some(obj) = value.as_object_mut() {
            obj.insert("schema_version".into(), Value::from(version));
        }
    }
    T::upgrade_nested(&mut value)?;
    Ok(value)
}

/// Parses a document from JSON text, upgrading it if needed.
pub fn from_json<T: Document>(text: &str) -> Result<T, DocumentError> {
    let value: Value = serde_json::from_str(text)?;
    from_value(value)
}

/// Parses a document from a JSON value, upgrading it if needed.
pub fn from_value<T: Document>(value: Value) -> Result<T, DocumentError> {
    let value = upgrade::<T>(value)?;
    Ok(serde_json::from_value(value)?)
}

/// Writes a document as pretty JSON with a trailing newline.
pub fn to_json<T: Document>(doc: &T) -> Result<String, DocumentError> {
    let mut text = serde_json::to_string_pretty(doc)?;
    text.push('\n');
    Ok(text)
}

/// Loads a document from disk.
pub fn load<T: Document>(path: &Path) -> Result<T, DocumentError> {
    let text = fs::read_to_string(path).map_err(|source| DocumentError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    from_json(&text)
}

/// Saves a document to disk without ever leaving a half-written file.
///
/// Writes to a temporary file in the same folder, flushes it to disk,
/// then renames it over the target. A crash mid-save leaves the old
/// file intact.
pub fn save<T: Document>(doc: &T, path: &Path) -> Result<(), DocumentError> {
    let text = to_json(doc)?;
    let io_err = |source| DocumentError::Io {
        path: path.to_path_buf(),
        source,
    };

    let file_name = path
        .file_name()
        .ok_or_else(|| io_err(std::io::Error::other("path has no file name")))?;
    let mut tmp_name = file_name.to_os_string();
    tmp_name.push(".tmp");
    let tmp_path = path.with_file_name(tmp_name);

    {
        let mut file = fs::File::create(&tmp_path).map_err(io_err)?;
        file.write_all(text.as_bytes()).map_err(io_err)?;
        file.sync_all().map_err(io_err)?;
    }
    fs::rename(&tmp_path, path).map_err(|source| {
        let _ = fs::remove_file(&tmp_path);
        io_err(source)
    })
}

/// Generates the pretty JSON Schema for a document type.
pub fn schema_json<T: Document>() -> String {
    let schema = schemars::schema_for!(T);
    let mut text = serde_json::to_string_pretty(&schema).expect("schemas always serialize");
    text.push('\n');
    text
}
