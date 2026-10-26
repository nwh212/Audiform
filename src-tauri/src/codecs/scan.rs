// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Turns a mix of dropped files and folders into a list of audio files.

use std::fs;
use std::path::{Path, PathBuf};

use super::SUPPORTED_EXTENSIONS;

/// Folders deeper than this are not searched, so a drop of a whole
/// drive cannot run forever.
const MAX_DEPTH: usize = 8;

/// Stops a single drop from flooding the Files panel.
const MAX_FILES: usize = 5000;

/// True when the path has an extension the Files panel accepts.
pub fn is_supported(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| SUPPORTED_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// Expands folders into the supported audio files inside them, sorted
/// by path. Files are passed through even when unsupported, so the
/// Files panel can show the user why each one was refused. Unsupported
/// files found inside folders are skipped silently.
pub fn expand_paths(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for p in paths {
        if out.len() >= MAX_FILES {
            break;
        }
        if p.is_dir() {
            let mut found = Vec::new();
            walk(p, 0, &mut found);
            found.sort();
            let room = MAX_FILES - out.len();
            out.extend(found.into_iter().take(room));
        } else {
            out.push(p.clone());
        }
    }
    out
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > MAX_DEPTH || out.len() >= MAX_FILES {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        // Symlinks are skipped so a loop cannot trap the walk.
        if kind.is_dir() {
            walk(&path, depth + 1, out);
        } else if kind.is_file() && is_supported(&path) {
            out.push(path);
        }
        if out.len() >= MAX_FILES {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_extensions_without_case() {
        assert!(is_supported(Path::new("C:/a/Interview.WAV")));
        assert!(is_supported(Path::new("bumper.m4a")));
        assert!(!is_supported(Path::new("notes.txt")));
        assert!(!is_supported(Path::new("no_extension")));
    }

    #[test]
    fn expands_folders_and_keeps_loose_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir(root.join("sub")).unwrap();
        for name in ["b.wav", "a.mp3", "skip.txt", "sub/c.flac"] {
            fs::write(root.join(name), b"x").unwrap();
        }
        let loose = root.join("readme.txt");
        fs::write(&loose, b"x").unwrap();

        let got = expand_paths(&[root.to_path_buf(), loose.clone()]);
        let names: Vec<_> = got
            .iter()
            .map(|p| {
                p.strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();
        assert_eq!(names, ["a.mp3", "b.wav", "sub/c.flac", "readme.txt"]);
    }
}
