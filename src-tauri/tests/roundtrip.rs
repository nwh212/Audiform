// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Every type survives a trip to JSON and back unchanged, in memory
//! and on disk. Header checks reject files this build cannot read.

mod common;

use std::fmt::Debug;

use audiform_lib::library::{Library, Template};
use audiform_lib::session::document::{self, from_json, load, save, to_json};
use audiform_lib::session::validate::validate;
use audiform_lib::session::{Document, DocumentError, Session, Workspace};
use pretty_assertions::assert_eq;
use serde::de::DeserializeOwned;
use serde::Serialize;

/// Round trips a plain value through serde_json.
fn roundtrip_value<T>(value: &T)
where
    T: Serialize + DeserializeOwned + PartialEq + Debug,
{
    let text = serde_json::to_string_pretty(value).unwrap();
    let back: T = serde_json::from_str(&text).unwrap();
    assert_eq!(&back, value);
}

/// Round trips a document through the versioned loader, in memory and
/// on disk.
fn roundtrip_document<T>(doc: &T)
where
    T: Document + PartialEq + Debug,
{
    let text = to_json(doc).unwrap();
    assert!(text.ends_with('\n'));
    let back: T = from_json(&text).unwrap();
    assert_eq!(&back, doc);

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("doc.json");
    save(doc, &path).unwrap();
    // Saving twice must replace the file, including on Windows.
    save(doc, &path).unwrap();
    let loaded: T = load(&path).unwrap();
    assert_eq!(&loaded, doc);

    let leftovers: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(leftovers.len(), 1, "temp file left behind: {leftovers:?}");
}

#[test]
fn parts_roundtrip() {
    roundtrip_value(&common::track());
    roundtrip_value(&common::bus());
    roundtrip_value(&common::clip());
    roundtrip_value(&common::envelope());
    roundtrip_value(&common::marker());
    roundtrip_value(&common::effect_chain());
    roundtrip_value(&common::library_item());
}

#[test]
fn session_roundtrips() {
    roundtrip_document(&common::session());
}

#[test]
fn new_session_roundtrips() {
    roundtrip_document(&Session::new("Empty", 44_100));
}

#[test]
fn library_roundtrips() {
    roundtrip_document(&common::library());
}

#[test]
fn template_roundtrips() {
    roundtrip_document(&common::template());
}

#[test]
fn workspace_roundtrips() {
    roundtrip_document(&common::workspace());
}

#[test]
fn host_profile_roundtrips() {
    roundtrip_document(&common::host_profile());
}

#[test]
fn keymap_roundtrips() {
    roundtrip_document(&common::keymap());
}

#[test]
fn preferences_roundtrip() {
    roundtrip_document(&common::preferences());
}

#[test]
fn sample_session_is_valid() {
    assert_eq!(validate(&common::session()), vec![]);
}

#[test]
fn every_document_writes_its_header() {
    fn header<T: Document>(doc: &T) -> (String, u64) {
        let v: serde_json::Value = serde_json::from_str(&to_json(doc).unwrap()).unwrap();
        (
            v["format"].as_str().unwrap().to_string(),
            v["schema_version"].as_u64().unwrap(),
        )
    }
    assert_eq!(header(&common::session()), ("altaudio.session".into(), 1));
    assert_eq!(header(&common::library()), ("altaudio.library".into(), 1));
    assert_eq!(header(&common::template()), ("altaudio.template".into(), 1));
    assert_eq!(
        header(&common::workspace()),
        ("altaudio.workspace".into(), 1)
    );
    assert_eq!(
        header(&common::host_profile()),
        ("altaudio.host_profile".into(), 1)
    );
}

fn session_value() -> serde_json::Value {
    serde_json::to_value(common::session()).unwrap()
}

#[test]
fn rejects_wrong_format() {
    let err = document::from_value::<Library>(session_value()).unwrap_err();
    assert!(matches!(err, DocumentError::WrongFormat { .. }), "{err}");
}

#[test]
fn rejects_newer_version() {
    let mut v = session_value();
    v["schema_version"] = 99.into();
    let err = document::from_value::<Session>(v).unwrap_err();
    assert!(
        matches!(err, DocumentError::TooNew { found: 99, .. }),
        "{err}"
    );
    assert!(err.to_string().contains("Update Audiform"));
}

#[test]
fn rejects_version_zero() {
    let mut v = session_value();
    v["schema_version"] = 0.into();
    let err = document::from_value::<Session>(v).unwrap_err();
    assert!(matches!(err, DocumentError::UnknownVersion { .. }), "{err}");
}

#[test]
fn rejects_missing_header() {
    let mut v = session_value();
    v.as_object_mut().unwrap().remove("schema_version");
    let err = document::from_value::<Session>(v).unwrap_err();
    assert!(matches!(
        err,
        DocumentError::MissingHeader("schema_version")
    ));
}

#[test]
fn rejects_invalid_json() {
    let err = from_json::<Session>("{ not json").unwrap_err();
    assert!(matches!(err, DocumentError::Json(_)));
}

#[test]
fn template_checks_its_nested_session() {
    let mut v = serde_json::to_value(common::template()).unwrap();
    v["session"]["schema_version"] = 99.into();
    let err = document::from_value::<Template>(v).unwrap_err();
    assert!(matches!(err, DocumentError::TooNew { .. }), "{err}");
}

#[test]
fn missing_file_reports_path() {
    let err = load::<Workspace>(std::path::Path::new("does/not/exist.alworkspace")).unwrap_err();
    assert!(err.to_string().contains("exist.alworkspace"), "{err}");
}
