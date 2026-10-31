// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

//! Settings backups (`.afbackup`): every setting in the app's settings
//! folder in one zip, to move to another computer or keep safe.
//!
//! The zip holds `manifest.json` (format, version, when it was made) and
//! the folder's files at their own paths. Working files are left out: the
//! Audio Separation engine and models (downloaded again on use),
//! autosaves, and any file over [`MAX_FILE`]. Restoring writes the files
//! back over the ones there, never outside the folder.

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const FORMAT: &str = "audiform.settings-backup";
pub const SCHEMA_VERSION: u32 = 1;
const MANIFEST: &str = "manifest.json";

/// Files larger than this are working data, not settings.
pub const MAX_FILE: u64 = 20 << 20;

/// Top-level folders left out: downloads and working files.
const SKIP: [&str; 6] = [
    "separation",
    "models",
    "engine",
    "autosave",
    "work",
    "cache",
];

#[derive(Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub format: String,
    pub schema_version: u32,
    /// Seconds since 1970 (UTC).
    pub created: u64,
    pub app_version: String,
    pub files: usize,
}

fn skipped(rel: &Path) -> bool {
    match rel.components().next() {
        Some(Component::Normal(first)) => {
            let first = first.to_string_lossy().to_lowercase();
            SKIP.contains(&first.as_str()) || first.starts_with('.')
        }
        _ => true,
    }
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let Ok(rel) = path.strip_prefix(root) else {
            continue;
        };
        if skipped(rel) {
            continue;
        }
        let meta = entry.metadata()?;
        if meta.is_dir() {
            walk(root, &path, out)?;
        } else if meta.is_file() && meta.len() <= MAX_FILE {
            out.push(rel.to_path_buf());
        }
    }
    Ok(())
}

/// Writes every setting under `config` to the backup at `dest`. Returns
/// how many files went in.
pub fn backup(config: &Path, dest: &Path, app_version: &str) -> Result<usize, String> {
    let mut files = Vec::new();
    if config.is_dir() {
        walk(config, config, &mut files).map_err(|e| e.to_string())?;
    }
    files.sort();
    let tmp = dest.with_extension("afbackup.partial");
    let r = (|| -> Result<(), String> {
        let f =
            File::create(&tmp).map_err(|e| format!("Could not write {}: {e}", dest.display()))?;
        let mut zip = zip::ZipWriter::new(io::BufWriter::new(f));
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        let manifest = Manifest {
            format: FORMAT.into(),
            schema_version: SCHEMA_VERSION,
            created: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs()),
            app_version: app_version.into(),
            files: files.len(),
        };
        zip.start_file(MANIFEST, opts).map_err(|e| e.to_string())?;
        zip.write_all(&serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        for rel in &files {
            let name = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            let mut data = Vec::new();
            File::open(config.join(rel))
                .and_then(|mut f| f.read_to_end(&mut data))
                .map_err(|e| format!("Could not read {}: {e}", rel.display()))?;
            zip.start_file(format!("settings/{name}"), opts)
                .map_err(|e| e.to_string())?;
            zip.write_all(&data).map_err(|e| e.to_string())?;
        }
        let mut w = zip.finish().map_err(|e| e.to_string())?;
        w.flush().map_err(|e| e.to_string())
    })();
    if let Err(e) = r {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    fs::rename(&tmp, dest).map_err(|e| e.to_string())?;
    Ok(files.len())
}

/// Reads a backup's manifest without changing anything.
pub fn inspect(src: &Path) -> Result<Manifest, String> {
    let f = File::open(src).map_err(|e| format!("Could not open {}: {e}", src.display()))?;
    let mut zip = zip::ZipArchive::new(io::BufReader::new(f))
        .map_err(|_| "This is not an Audiform settings backup.".to_string())?;
    let mut text = String::new();
    zip.by_name(MANIFEST)
        .map_err(|_| "This is not an Audiform settings backup.".to_string())?
        .read_to_string(&mut text)
        .map_err(|e| e.to_string())?;
    let m: Manifest =
        serde_json::from_str(&text).map_err(|_| "The backup's manifest is damaged.".to_string())?;
    if m.format != FORMAT {
        return Err("This is not an Audiform settings backup.".into());
    }
    if m.schema_version > SCHEMA_VERSION {
        return Err(
            "This backup was made by a newer Audiform. Update Audiform to restore it.".into(),
        );
    }
    Ok(m)
}

/// Writes a backup's settings into `config`, over what is there.
/// Returns how many files were restored.
pub fn restore(src: &Path, config: &Path) -> Result<usize, String> {
    inspect(src)?;
    let f = File::open(src).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(io::BufReader::new(f)).map_err(|e| e.to_string())?;
    let mut n = 0;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        if entry.is_dir() {
            continue;
        }
        // Only files under settings/, and never outside the folder.
        let Some(rel) = entry
            .enclosed_name()
            .and_then(|p| p.strip_prefix("settings").ok().map(Path::to_path_buf))
        else {
            continue;
        };
        if rel.as_os_str().is_empty() || skipped(&rel) || entry.size() > MAX_FILE {
            continue;
        }
        let dest = config.join(&rel);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut data = Vec::new();
        entry.read_to_end(&mut data).map_err(|e| e.to_string())?;
        fs::write(&dest, data).map_err(|e| format!("Could not write {}: {e}", rel.display()))?;
        n += 1;
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backs_up_settings_and_restores_them_but_not_working_files() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join("cfg");
        fs::create_dir_all(cfg.join("workspaces")).unwrap();
        fs::create_dir_all(cfg.join("separation/models")).unwrap();
        fs::create_dir_all(cfg.join("autosave")).unwrap();
        fs::write(cfg.join("settings.json"), b"{\"a\":1}").unwrap();
        fs::write(cfg.join("keymap.alkeys"), b"keys").unwrap();
        fs::write(cfg.join("workspaces/mine.json"), b"layout").unwrap();
        fs::write(cfg.join("separation/models/big.onnx"), b"model").unwrap();
        fs::write(cfg.join("autosave/x.audfrm"), b"work").unwrap();
        let file = dir.path().join("mine.afbackup");
        assert_eq!(backup(&cfg, &file, "1.0").unwrap(), 3);
        assert_eq!(inspect(&file).unwrap().files, 3);

        let other = dir.path().join("other");
        fs::create_dir_all(&other).unwrap();
        fs::write(other.join("settings.json"), b"old").unwrap();
        assert_eq!(restore(&file, &other).unwrap(), 3);
        assert_eq!(fs::read(other.join("settings.json")).unwrap(), b"{\"a\":1}");
        assert_eq!(
            fs::read(other.join("workspaces/mine.json")).unwrap(),
            b"layout"
        );
        assert!(!other.join("separation").exists());
        assert!(!other.join("autosave").exists());
    }

    #[test]
    fn refuses_other_zips() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("x.afbackup");
        let mut zip = zip::ZipWriter::new(File::create(&file).unwrap());
        zip.start_file("hello.txt", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"hi").unwrap();
        zip.finish().unwrap();
        assert!(inspect(&file).is_err());
        assert!(restore(&file, dir.path()).is_err());
    }
}
