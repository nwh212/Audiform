// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Frozen sample files for every format version ever shipped.
//!
//! Files in `tests/fixtures/v<N>/` are never edited after they are
//! committed. Every future build must still open them. This is how
//! "never break an old file" is enforced.
//!
//! When a format gets a new version, create the new fixture folder with:
//!
//! ```text
//! WRITE_FIXTURES=1 cargo test --test fixtures
//! ```
//!
//! That only writes files that do not exist yet. It never overwrites.

mod common;

use std::fmt::Debug;
use std::path::PathBuf;

use audiform_lib::library::{Library, Template};
use audiform_lib::session::document::{load, to_json};
use audiform_lib::session::validate::validate;
use audiform_lib::session::{Document, HostProfile, Keymap, Preferences, Session, Workspace};

fn fixture_path(version: u32, name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(format!("v{version}"))
        .join(name)
}

/// Writes the current sample as a fixture if asked and it is missing.
fn write_if_missing<T: Document>(name: &str, doc: &T) {
    if std::env::var_os("WRITE_FIXTURES").is_none() {
        return;
    }
    let path = fixture_path(T::CURRENT_VERSION, name);
    if !path.exists() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, to_json(doc).unwrap()).unwrap();
    }
}

/// Loads every fixture version of a format up to the current one.
fn load_all<T: Document + PartialEq + Debug>(name: &str, current: &T) -> Vec<T> {
    write_if_missing(name, current);
    let mut docs = Vec::new();
    for version in 1..=T::CURRENT_VERSION {
        let path = fixture_path(version, name);
        assert!(path.exists(), "missing fixture {}", path.display());
        let doc: T =
            load(&path).unwrap_or_else(|e| panic!("failed to open {}: {e}", path.display()));
        docs.push(doc);
    }
    docs
}

#[test]
fn session_fixtures_open() {
    let docs = load_all("session.altaudio", &common::session());
    for doc in &docs {
        assert_eq!(validate(doc), vec![]);
    }
    // The current-version fixture matches the model exactly.
    assert_eq!(docs.last().unwrap(), &common::session());
}

#[test]
fn library_fixtures_open() {
    let docs = load_all("library.json", &common::library());
    assert_eq!(docs.last().unwrap(), &common::library());
}

#[test]
fn template_fixtures_open() {
    let docs = load_all("template.altemplate", &common::template());
    assert_eq!(docs.last().unwrap(), &common::template());
}

#[test]
fn workspace_fixtures_open() {
    let docs = load_all("default.alworkspace", &common::workspace());
    assert_eq!(docs.last().unwrap(), &common::workspace());
}

#[test]
fn host_profile_fixtures_open() {
    let docs = load_all("host.alhost", &common::host_profile());
    assert_eq!(docs.last().unwrap(), &common::host_profile());
}

#[test]
fn keymap_fixtures_open() {
    let docs = load_all("keymap.alkeys", &common::keymap());
    assert_eq!(docs.last().unwrap(), &common::keymap());
}

#[test]
fn preferences_fixtures_open() {
    let docs = load_all("preferences.alprefs", &common::preferences());
    assert_eq!(docs.last().unwrap(), &common::preferences());
}

#[test]
fn format_ids_are_stable() {
    // Guard against an accidental rename of the session format id,
    // which would orphan every saved session.
    assert_eq!(Session::FORMAT, "altaudio.session");
    assert_eq!(Library::FORMAT, "altaudio.library");
    assert_eq!(Template::FORMAT, "altaudio.template");
    assert_eq!(Workspace::FORMAT, "altaudio.workspace");
    assert_eq!(HostProfile::FORMAT, "altaudio.host_profile");
    assert_eq!(Keymap::FORMAT, "altaudio.keymap");
    assert_eq!(Preferences::FORMAT, "altaudio.preferences");
}
