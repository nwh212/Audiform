// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Drive and folder listing for the Drive Manager panel.
//!
//! Platform differences live in `roots()`. Everything else is plain
//! `std::fs` and works the same on every system.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::Serialize;

use crate::codecs::is_supported;

/// Something to show in the Drive Manager's location list.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Location {
    pub path: PathBuf,
    pub label: String,
    pub kind: LocationKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LocationKind {
    Drive,
    Place,
}

/// One row in a folder listing.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    /// Bytes, for files.
    pub size: u64,
    /// Milliseconds since 1970, if known.
    pub modified: Option<u64>,
    /// An audio file the app can open.
    pub audio: bool,
}

/// Drives or mount points, platform specific.
pub fn roots() -> Vec<Location> {
    let mut out = Vec::new();
    #[cfg(windows)]
    {
        for letter in b'A'..=b'Z' {
            let root = format!("{}:\\", letter as char);
            // A floppy or empty card reader can stall; metadata is cheap
            // for mounted drives and fails fast for missing ones.
            if fs::metadata(&root).is_ok() {
                out.push(Location {
                    path: PathBuf::from(&root),
                    label: format!("{}:", letter as char),
                    kind: LocationKind::Drive,
                });
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        out.push(Location {
            path: PathBuf::from("/"),
            label: "Computer".into(),
            kind: LocationKind::Drive,
        });
        push_children(&mut out, Path::new("/Volumes"));
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        out.push(Location {
            path: PathBuf::from("/"),
            label: "Computer".into(),
            kind: LocationKind::Drive,
        });
        // Removable drives on most desktop Linux systems, Zorin included.
        if let Ok(user) = std::env::var("USER") {
            push_children(&mut out, &Path::new("/media").join(&user));
            push_children(&mut out, &Path::new("/run/media").join(&user));
        }
        push_children(&mut out, Path::new("/mnt"));
    }
    out
}

#[cfg(unix)]
fn push_children(out: &mut Vec<Location>, dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut found: Vec<Location> = entries
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|e| Location {
            label: e.file_name().to_string_lossy().into_owned(),
            path: e.path(),
            kind: LocationKind::Drive,
        })
        .collect();
    found.sort_by(|a, b| a.label.cmp(&b.label));
    out.extend(found);
}

/// Lists a folder: subfolders first, then files, each sorted by name.
/// Hidden and system items are skipped. With `audio_only`, files the
/// app cannot open are left out.
pub fn list_dir(dir: &Path, audio_only: bool) -> std::io::Result<Vec<Entry>> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir)? {
        let Ok(entry) = entry else { continue };
        let name = entry.file_name().to_string_lossy().into_owned();
        let Ok(meta) = entry.metadata() else { continue };
        if is_hidden(&name, &meta) {
            continue;
        }
        let path = entry.path();
        let is_dir = meta.is_dir();
        let audio = !is_dir && is_supported(&path);
        if !is_dir && audio_only && !audio {
            continue;
        }
        out.push(Entry {
            name,
            path,
            is_dir,
            size: if is_dir { 0 } else { meta.len() },
            modified: meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64),
            audio,
        });
    }
    out.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| natural_key(&a.name).cmp(&natural_key(&b.name)))
    });
    Ok(out)
}

/// Lowercase name with digit runs padded, so "ep2" sorts before "ep10".
fn natural_key(name: &str) -> String {
    let mut key = String::with_capacity(name.len() + 8);
    let mut digits = String::new();
    for c in name.chars() {
        if c.is_ascii_digit() {
            digits.push(c);
        } else {
            if !digits.is_empty() {
                key.push_str(&format!("{digits:0>12}"));
                digits.clear();
            }
            key.extend(c.to_lowercase());
        }
    }
    if !digits.is_empty() {
        key.push_str(&format!("{digits:0>12}"));
    }
    key
}

#[cfg(windows)]
fn is_hidden(_name: &str, meta: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const HIDDEN: u32 = 0x2;
    const SYSTEM: u32 = 0x4;
    meta.file_attributes() & (HIDDEN | SYSTEM) != 0
}

#[cfg(not(windows))]
fn is_hidden(name: &str, _meta: &fs::Metadata) -> bool {
    name.starts_with('.')
}

/// The folder above `path`, or None at a drive root.
pub fn parent(path: &Path) -> Option<PathBuf> {
    path.parent().map(Path::to_path_buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorts_folders_first_then_natural_order() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir(root.join("Zeta")).unwrap();
        fs::create_dir(root.join("alpha")).unwrap();
        for name in ["ep10.wav", "ep2.wav", "notes.txt"] {
            fs::write(root.join(name), b"x").unwrap();
        }
        // Dot files are hidden on Linux and macOS. Windows uses an
        // attribute instead, which std cannot set.
        #[cfg(not(windows))]
        fs::write(root.join(".hidden.wav"), b"x").unwrap();
        let names = |v: Vec<Entry>| v.into_iter().map(|e| e.name).collect::<Vec<_>>();
        assert_eq!(
            names(list_dir(root, false).unwrap()),
            ["alpha", "Zeta", "ep2.wav", "ep10.wav", "notes.txt"]
        );
        let audio = list_dir(root, true).unwrap();
        assert_eq!(
            names(audio.clone()),
            ["alpha", "Zeta", "ep2.wav", "ep10.wav"]
        );
        assert!(audio[2].audio && !audio[0].audio && audio[0].is_dir);
    }

    #[test]
    fn lists_at_least_one_root() {
        assert!(!roots().is_empty());
    }
}
