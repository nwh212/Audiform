// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Library folders on disk.
//!
//! A library folder holds `library.json` and an `audio/` folder with the
//! files it owns. The interface edits a `Library` value and saves the
//! whole index back, which keeps undo simple: undo saves the previous
//! index. Copied files are never deleted here. Items removed from the
//! index leave their files in `audio/` until the user runs
//! `clean_up`, so undo can always bring an item back.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;
use thiserror::Error;

use super::{Library, LibraryItem, LibraryItemKind, LIBRARY_INDEX};
use crate::codecs::probe;
use crate::session::document::{self, DocumentError};
use crate::session::model::new_id;

/// Sub-folder for files copied into a library.
pub const AUDIO_DIR: &str = "audio";

#[derive(Debug, Error)]
pub enum LibraryError {
    #[error("This folder is not a library (no {LIBRARY_INDEX}).")]
    NotALibrary,
    #[error("This folder already holds a library.")]
    AlreadyALibrary,
    #[error("{0}")]
    Document(#[from] DocumentError),
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

fn io(path: &Path) -> impl FnOnce(std::io::Error) -> LibraryError + '_ {
    move |source| LibraryError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// Creates a new, empty library in `dir`. The folder may exist but must
/// not already hold a library.
pub fn create(dir: &Path, name: &str) -> Result<Library, LibraryError> {
    let index = dir.join(LIBRARY_INDEX);
    if index.exists() {
        return Err(LibraryError::AlreadyALibrary);
    }
    fs::create_dir_all(dir.join(AUDIO_DIR)).map_err(io(dir))?;
    let lib = Library::new(name);
    document::save(&lib, &index)?;
    Ok(lib)
}

pub fn open(dir: &Path) -> Result<Library, LibraryError> {
    let index = dir.join(LIBRARY_INDEX);
    if !index.exists() {
        return Err(LibraryError::NotALibrary);
    }
    Ok(document::load(&index)?)
}

pub fn save(dir: &Path, lib: &Library) -> Result<(), LibraryError> {
    if !dir.is_dir() {
        return Err(LibraryError::NotALibrary);
    }
    Ok(document::save(lib, &dir.join(LIBRARY_INDEX))?)
}

/// Absolute path of an item's file.
pub fn item_path(dir: &Path, item: &LibraryItem) -> PathBuf {
    if item.file.is_absolute() {
        item.file.clone()
    } else {
        dir.join(&item.file)
    }
}

/// One file that could not be added, and why.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Skipped {
    pub path: PathBuf,
    pub reason: String,
}

/// Measures files and, when `copy` is true, copies them into the
/// library's audio folder. Returns new items; the caller adds them to
/// the index and saves. Files already inside the library folder are
/// used in place.
pub fn import(
    dir: &Path,
    paths: &[PathBuf],
    copy: bool,
) -> Result<(Vec<LibraryItem>, Vec<Skipped>), LibraryError> {
    let audio = dir.join(AUDIO_DIR);
    fs::create_dir_all(&audio).map_err(io(&audio))?;
    let dir_canon = fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());

    let mut items = Vec::new();
    let mut skipped = Vec::new();
    let mut taken: HashSet<String> = HashSet::new();
    for src in paths {
        let info = match probe(src) {
            Ok(i) => i,
            Err(e) => {
                skipped.push(Skipped {
                    path: src.clone(),
                    reason: e.to_string(),
                });
                continue;
            }
        };
        let src_canon = fs::canonicalize(src).unwrap_or_else(|_| src.clone());
        let inside = src_canon
            .strip_prefix(&dir_canon)
            .ok()
            .map(Path::to_path_buf);

        let (file, copied, original) = match (inside, copy) {
            (Some(rel), _) => (rel, true, None),
            (None, true) => {
                let name = unique_name(&audio, &info.name, &mut taken);
                let dest = audio.join(&name);
                fs::copy(src, &dest).map_err(io(&dest))?;
                (PathBuf::from(AUDIO_DIR).join(name), true, Some(src.clone()))
            }
            (None, false) => (src.clone(), false, None),
        };

        items.push(LibraryItem {
            id: new_id(),
            name: display_name(&info.name),
            kind: LibraryItemKind::Audio,
            file,
            copied,
            original_path: original,
            sample_rate: info.sample_rate,
            channels: info.channels,
            length: info.length,
            tags: Vec::new(),
            notes: String::new(),
            color: None,
            added: Utc::now(),
        });
    }
    Ok((items, skipped))
}

/// Files in `audio/` that no item uses. Returns how many were deleted.
pub fn clean_up(dir: &Path, lib: &Library, dry_run: bool) -> Result<usize, LibraryError> {
    // Only a real library folder is ever touched. The saved index must
    // exist and describe this same library, so a stray folder picked by
    // mistake can never lose files.
    let index = dir.join(LIBRARY_INDEX);
    match crate::session::document::load::<Library>(&index) {
        Ok(saved) if saved.id == lib.id => {}
        _ => return Err(LibraryError::NotALibrary),
    }
    let audio = dir.join(AUDIO_DIR);
    let used: HashSet<PathBuf> = lib
        .items
        .iter()
        .filter(|i| !i.file.is_absolute())
        .map(|i| dir.join(&i.file))
        .collect();
    let mut removed = 0;
    let Ok(entries) = fs::read_dir(&audio) else {
        return Ok(0);
    };
    for entry in entries.flatten() {
        let path = entry.path();
        // Plain files only: never follow a link out of the folder, and
        // only audio files the library could have copied in.
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        if !meta.file_type().is_file()
            || !crate::codecs::is_supported(&path)
            || used.contains(&path)
        {
            continue;
        }
        if !dry_run {
            fs::remove_file(&path).map_err(io(&path))?;
        }
        removed += 1;
    }
    Ok(removed)
}

/// File name without extension, for the item's display name.
fn display_name(file_name: &str) -> String {
    Path::new(file_name)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| file_name.to_string())
}

/// A file name not used in `dir` or earlier in this import:
/// `promo.wav`, `promo (2).wav`, `promo (3).wav`.
fn unique_name(dir: &Path, wanted: &str, taken: &mut HashSet<String>) -> String {
    let p = Path::new(wanted);
    let stem = p
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = p
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    let mut n = 1;
    loop {
        let candidate = if n == 1 {
            format!("{stem}{ext}")
        } else {
            format!("{stem} ({n}){ext}")
        };
        if !taken.contains(&candidate.to_lowercase()) && !dir.join(&candidate).exists() {
            taken.insert(candidate.to_lowercase());
            return candidate;
        }
        n += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn media(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("media")
            .join(name)
    }

    #[test]
    fn create_open_save() {
        let dir = tempfile::tempdir().unwrap();
        let lib_dir = dir.path().join("Promos");
        let mut lib = create(&lib_dir, "Promos").unwrap();
        assert!(matches!(
            create(&lib_dir, "Again"),
            Err(LibraryError::AlreadyALibrary)
        ));
        lib.name = "Network Promos".into();
        save(&lib_dir, &lib).unwrap();
        assert_eq!(open(&lib_dir).unwrap().name, "Network Promos");
        assert!(matches!(open(dir.path()), Err(LibraryError::NotALibrary)));
    }

    #[test]
    fn import_copies_with_unique_names_and_reports_skips() {
        let dir = tempfile::tempdir().unwrap();
        create(dir.path(), "Beds").unwrap();
        let bad = dir.path().join("notes.txt");
        fs::write(&bad, b"x").unwrap();
        let (items, skipped) = import(
            dir.path(),
            &[media("tone.mp3"), media("tone.mp3"), bad.clone()],
            true,
        )
        .unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(skipped.len(), 1);
        assert_eq!(skipped[0].path, bad);
        assert_eq!(items[0].name, "tone");
        assert_eq!(items[0].file, PathBuf::from("audio").join("tone.mp3"));
        assert_eq!(items[1].file, PathBuf::from("audio").join("tone (2).mp3"));
        assert!(items
            .iter()
            .all(|i| i.copied && item_path(dir.path(), i).exists()));
        assert_eq!(
            items[0].original_path.as_deref(),
            Some(media("tone.mp3").as_path())
        );
        assert_eq!(items[0].sample_rate, 48_000);
    }

    #[test]
    fn import_can_link_instead_of_copy() {
        let dir = tempfile::tempdir().unwrap();
        create(dir.path(), "Linked").unwrap();
        let (items, _) = import(dir.path(), &[media("tone.flac")], false).unwrap();
        assert!(!items[0].copied);
        assert_eq!(items[0].file, media("tone.flac"));
        assert_eq!(fs::read_dir(dir.path().join(AUDIO_DIR)).unwrap().count(), 0);
    }

    #[test]
    fn files_already_inside_are_used_in_place() {
        let dir = tempfile::tempdir().unwrap();
        create(dir.path(), "Here").unwrap();
        let inside = dir.path().join(AUDIO_DIR).join("bed.wav");
        fs::copy(media("tone.aiff"), &inside).unwrap();
        let (items, _) = import(dir.path(), std::slice::from_ref(&inside), true).unwrap();
        assert_eq!(items[0].file, PathBuf::from("audio").join("bed.wav"));
        assert_eq!(fs::read_dir(dir.path().join(AUDIO_DIR)).unwrap().count(), 1);
    }

    #[test]
    fn clean_up_deletes_only_unused_files() {
        let dir = tempfile::tempdir().unwrap();
        let mut lib = create(dir.path(), "Tidy").unwrap();
        let (items, _) = import(dir.path(), &[media("tone.ogg"), media("tone.m4a")], true).unwrap();
        lib.items.push(items[0].clone());
        save(dir.path(), &lib).unwrap();
        fs::write(dir.path().join(AUDIO_DIR).join("notes.txt"), "keep me").unwrap();
        assert_eq!(
            clean_up(dir.path(), &lib, true).unwrap(),
            1,
            "dry run counts"
        );
        assert!(
            item_path(dir.path(), &items[1]).exists(),
            "dry run deletes nothing"
        );
        assert_eq!(clean_up(dir.path(), &lib, false).unwrap(), 1);
        assert!(item_path(dir.path(), &items[0]).exists());
        assert!(!item_path(dir.path(), &items[1]).exists());
        assert!(
            dir.path().join(AUDIO_DIR).join("notes.txt").exists(),
            "non-audio files are never touched"
        );
    }

    #[test]
    fn clean_up_refuses_a_folder_that_is_not_this_library() {
        let dir = tempfile::tempdir().unwrap();
        let lib = create(dir.path(), "Real").unwrap();
        let other = tempfile::tempdir().unwrap();
        fs::create_dir_all(other.path().join(AUDIO_DIR)).unwrap();
        fs::copy(
            media("tone.ogg"),
            other.path().join(AUDIO_DIR).join("mine.ogg"),
        )
        .unwrap();
        assert!(clean_up(other.path(), &lib, false).is_err());
        assert!(other.path().join(AUDIO_DIR).join("mine.ogg").exists());
        let stranger = Library::new("Someone else");
        assert!(clean_up(dir.path(), &stranger, false).is_err());
    }
}
