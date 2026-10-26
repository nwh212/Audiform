// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Opening a zip of recordings, such as the per-speaker download from a
//! call recorder: the audio files inside are extracted into a folder
//! beside the zip (named after it) so sessions can point at them. Only
//! audio is extracted, every file lands directly in that folder whatever
//! path the zip gives it, and files already extracted are reused.

use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

use super::is_supported;

/// Largest single file we'll extract: 4 GiB, far beyond any episode.
const MAX_ENTRY: u64 = 4 << 30;

/// Extracts the audio in `zip` and returns the extracted files in the
/// zip's order.
pub fn extract_audio(zip: &Path) -> Result<Vec<PathBuf>, String> {
    let file = File::open(zip).map_err(|e| format!("Could not open {}: {e}", zip.display()))?;
    let mut archive = zip::ZipArchive::new(io::BufReader::new(file))
        .map_err(|e| format!("{} isn't a readable zip: {e}", zip.display()))?;
    let stem = zip
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Recordings".into());
    let dir = zip.with_file_name(stem.trim());
    let mut out = Vec::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        if entry.is_dir() {
            continue;
        }
        // The name only: no folders from inside the zip, nothing outside `dir`.
        let Some(name) = entry
            .enclosed_name()
            .and_then(|p| p.file_name().map(|n| n.to_owned()))
        else {
            continue;
        };
        let dest = dir.join(&name);
        if !is_supported(&dest) || name.to_string_lossy().starts_with('.') {
            continue;
        }
        if entry.size() > MAX_ENTRY {
            return Err(format!(
                "{} is too large to extract.",
                name.to_string_lossy()
            ));
        }
        if fs::metadata(&dest)
            .map(|m| m.len() == entry.size())
            .unwrap_or(false)
        {
            out.push(dest);
            continue;
        }
        fs::create_dir_all(&dir).map_err(|e| format!("Could not create {}: {e}", dir.display()))?;
        let tmp = dir.join(format!(".{}.partial", name.to_string_lossy()));
        let r = (|| -> io::Result<()> {
            let mut f = File::create(&tmp)?;
            io::copy(&mut entry, &mut f)?;
            f.sync_all()?;
            drop(f);
            fs::rename(&tmp, &dest)
        })();
        if let Err(e) = r {
            let _ = fs::remove_file(&tmp);
            return Err(format!("Could not extract {}: {e}", name.to_string_lossy()));
        }
        out.push(dest);
    }
    if out.is_empty() {
        return Err(format!("{} has no audio files in it.", zip.display()));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn make_zip(path: &Path, entries: &[(&str, &[u8])]) {
        // A stored (uncompressed) zip written by hand keeps the zip crate's writer out of the build.
        let mut data = Vec::new();
        let mut central = Vec::new();
        for (name, body) in entries {
            let crc = crc32(body);
            let off = data.len() as u32;
            let n = name.len() as u16;
            let size = body.len() as u32;
            data.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
            data.extend_from_slice(&[20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            data.extend_from_slice(&crc.to_le_bytes());
            data.extend_from_slice(&size.to_le_bytes());
            data.extend_from_slice(&size.to_le_bytes());
            data.extend_from_slice(&n.to_le_bytes());
            data.extend_from_slice(&0u16.to_le_bytes());
            data.extend_from_slice(name.as_bytes());
            data.extend_from_slice(body);
            central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
            central.extend_from_slice(&[20, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            central.extend_from_slice(&crc.to_le_bytes());
            central.extend_from_slice(&size.to_le_bytes());
            central.extend_from_slice(&size.to_le_bytes());
            central.extend_from_slice(&n.to_le_bytes());
            central.extend_from_slice(&[0; 12]);
            central.extend_from_slice(&off.to_le_bytes());
            central.extend_from_slice(name.as_bytes());
        }
        let cd_off = data.len() as u32;
        let cd_len = central.len() as u32;
        data.extend_from_slice(&central);
        data.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        data.extend_from_slice(&[0, 0, 0, 0]);
        let count = entries.len() as u16;
        data.extend_from_slice(&count.to_le_bytes());
        data.extend_from_slice(&count.to_le_bytes());
        data.extend_from_slice(&cd_len.to_le_bytes());
        data.extend_from_slice(&cd_off.to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes());
        File::create(path).unwrap().write_all(&data).unwrap();
    }

    fn crc32(data: &[u8]) -> u32 {
        let mut c = 0xFFFF_FFFFu32;
        for b in data {
            c ^= u32::from(*b);
            for _ in 0..8 {
                c = if c & 1 != 0 {
                    (c >> 1) ^ 0xEDB8_8320
                } else {
                    c >> 1
                };
            }
        }
        !c
    }

    #[test]
    fn extracts_only_audio_flat_and_inside_its_folder() {
        let dir = tempfile::tempdir().unwrap();
        let z = dir.path().join("call-download.zip");
        make_zip(
            &z,
            &[
                ("1-host.flac", b"fake flac"),
                ("info.txt", b"notes"),
                ("../../escape.wav", b"nope"),
                ("sub/2-guest.flac", b"fake 2"),
            ],
        );
        let got = extract_audio(&z).unwrap();
        let folder = dir.path().join("call-download");
        // A path that climbs out of the zip is skipped entirely.
        assert_eq!(
            got,
            vec![folder.join("1-host.flac"), folder.join("2-guest.flac")]
        );
        assert!(!dir.path().join("escape.wav").exists() && !folder.join("escape.wav").exists());
        assert!(!folder.join("info.txt").exists());
        assert_eq!(fs::read(folder.join("2-guest.flac")).unwrap(), b"fake 2");
        // Again: reuses what's there.
        assert_eq!(extract_audio(&z).unwrap().len(), 2);
    }
}
