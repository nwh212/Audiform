// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

//! Reversed clips. Files decode forward only, so a clip played backwards
//! reads a reversed copy of its file, made once in the app's cache folder
//! (the original file is never changed). A reversed clip reads that copy
//! at the mirrored spot: what sat at the end of its region comes first.

use std::io::{BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::codecs::AudioReader;

static DIR: OnceLock<PathBuf> = OnceLock::new();

/// Where reversed copies go (set once at startup).
pub fn set_dir(dir: PathBuf) {
    let _ = DIR.set(dir);
}

/// The reversed copy's name for `path`: changes when the file does.
fn copy_path(path: &Path) -> Option<PathBuf> {
    let dir = DIR.get()?;
    let meta = std::fs::metadata(path).ok()?;
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs());
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in path.to_string_lossy().bytes().chain(meta.len().to_le_bytes()).chain(modified.to_le_bytes()) {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    Some(dir.join(format!("{h:016x}.wav")))
}

/// The reversed copy of `path`, if it has been made.
pub fn cached(path: &Path) -> Option<PathBuf> {
    copy_path(path).filter(|p| p.exists())
}

/// Makes the reversed copy of `path` (once), at the file's own rate and
/// channels, 32-bit float. Works through the file in pieces, so long
/// files never sit in memory whole.
pub fn prepare(path: &Path) -> Result<PathBuf, String> {
    let out = copy_path(path).ok_or("There is no cache folder for reversed audio.")?;
    if out.exists() {
        return Ok(out);
    }
    std::fs::create_dir_all(out.parent().unwrap_or(Path::new(".")))
        .map_err(|e| format!("Could not make the cache folder: {e}"))?;
    let probe = AudioReader::open(path, 2, None).map_err(|e| e.to_string())?;
    let channels = probe.source_channels().max(1);
    let rate = probe.source_rate();
    drop(probe);
    let mut r = AudioReader::open(path, channels, None).map_err(|e| e.to_string())?;
    // Forward, as raw samples, into a scratch file.
    let raw = out.with_extension("raw");
    let mut frames: u64 = 0;
    {
        let mut w = BufWriter::new(std::fs::File::create(&raw).map_err(|e| e.to_string())?);
        let mut buf = Vec::new();
        while r.read_into(&mut buf).map_err(|e| e.to_string())? {
            for s in &buf {
                w.write_all(&s.to_le_bytes()).map_err(|e| e.to_string())?;
            }
            frames += (buf.len() / channels) as u64;
            buf.clear();
        }
        w.flush().map_err(|e| e.to_string())?;
    }
    // Then backwards, a piece at a time, into the copy.
    let spec = hound::WavSpec {
        channels: channels as u16,
        sample_rate: rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let part = out.with_extension("part");
    let result = (|| -> Result<(), String> {
        let mut src = std::fs::File::open(&raw).map_err(|e| e.to_string())?;
        let mut w = hound::WavWriter::create(&part, spec).map_err(|e| e.to_string())?;
        const PIECE: u64 = 65_536;
        let frame_bytes = (channels * 4) as u64;
        let mut end = frames;
        let mut bytes = Vec::new();
        while end > 0 {
            let start = end.saturating_sub(PIECE);
            let n = (end - start) as usize;
            src.seek(SeekFrom::Start(start * frame_bytes)).map_err(|e| e.to_string())?;
            bytes.resize(n * frame_bytes as usize, 0);
            src.read_exact(&mut bytes).map_err(|e| e.to_string())?;
            for f in (0..n).rev() {
                for c in 0..channels {
                    let at = (f * channels + c) * 4;
                    let v = f32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
                    w.write_sample(v).map_err(|e| e.to_string())?;
                }
            }
            end = start;
        }
        w.finalize().map_err(|e| e.to_string())
    })();
    let _ = std::fs::remove_file(&raw);
    result?;
    std::fs::rename(&part, &out).map_err(|e| format!("Could not keep the reversed copy: {e}"))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn makes_a_reversed_copy() {
        let dir = tempfile::tempdir().unwrap();
        set_dir(dir.path().join("rev"));
        let src = dir.path().join("a.wav");
        let spec = hound::WavSpec { channels: 2, sample_rate: 48_000, bits_per_sample: 32, sample_format: hound::SampleFormat::Float };
        let mut w = hound::WavWriter::create(&src, spec).unwrap();
        for i in 0..100_000u32 {
            w.write_sample(i as f32 / 100_000.0).unwrap();
            w.write_sample(-(i as f32) / 100_000.0).unwrap();
        }
        w.finalize().unwrap();
        let out = prepare(&src).unwrap();
        assert_eq!(cached(&src), Some(out.clone()));
        let mut r = hound::WavReader::open(&out).unwrap();
        let s: Vec<f32> = r.samples::<f32>().map(Result::unwrap).collect();
        assert_eq!(s.len(), 200_000);
        assert!((s[0] - 99_999.0 / 100_000.0).abs() < 1e-6);
        assert!((s[1] + 99_999.0 / 100_000.0).abs() < 1e-6);
        assert!(s[199_998].abs() < 1e-6);
    }
}
