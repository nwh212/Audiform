// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! The published JSON Schemas in `/schemas` must match the Rust model.
//!
//! After changing the model, regenerate them with:
//!
//! ```text
//! UPDATE_SCHEMAS=1 cargo test --test schemas
//! ```
//!
//! On Windows PowerShell: `$env:UPDATE_SCHEMAS=1; cargo test --test schemas`

use std::path::PathBuf;

use audiform_lib::library::{Library, Template};
use audiform_lib::session::document::schema_json;
use audiform_lib::session::{Document, HostProfile, Keymap, Preferences, Session, Workspace};

fn schemas_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("schemas")
}

fn check<T: Document>() -> Option<String> {
    let path = schemas_dir().join(T::SCHEMA_FILE);
    // Normalize line endings so a Windows checkout with autocrlf passes.
    let expected = schema_json::<T>();
    let update = std::env::var_os("UPDATE_SCHEMAS").is_some();

    let actual = std::fs::read_to_string(&path)
        .ok()
        .map(|s| s.replace("\r\n", "\n"));
    if actual.as_deref() == Some(expected.as_str()) {
        return None;
    }
    if update {
        std::fs::create_dir_all(schemas_dir()).unwrap();
        std::fs::write(&path, &expected).unwrap();
        return None;
    }
    Some(T::SCHEMA_FILE.to_string())
}

#[test]
fn published_schemas_are_current() {
    let stale: Vec<String> = [
        check::<Session>(),
        check::<Library>(),
        check::<Template>(),
        check::<Workspace>(),
        check::<HostProfile>(),
        check::<Keymap>(),
        check::<Preferences>(),
    ]
    .into_iter()
    .flatten()
    .collect();

    assert!(
        stale.is_empty(),
        "these schemas are out of date: {stale:?}. Run UPDATE_SCHEMAS=1 cargo test --test schemas"
    );
}
