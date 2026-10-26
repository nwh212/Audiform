// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Waveform peak cache (`.alpk`).
//!
//! For each media file we store the minimum and maximum sample of every
//! block of 256 frames, plus coarser levels of 4,096 and 65,536 frames
//! for zoomed-out views. Values are 16-bit. At most two channels are
//! kept; files with more show their first two.
//!
//! The cache is a rebuildable file, not a document, so it is compact
//! binary rather than JSON. It carries a version and the source file's
//! size and modified time, and is rebuilt whenever either changes.
//!
//! Layout, little-endian:
//! ```text
//! magic "ALPK", version u32,
//! source size u64, source modified (ms since 1970) u64,
//! sample rate u32, channels u16, reserved u16, length (frames) u64,
//! level count u32, then per level: block u32, block count u64,
//! then each level's data: per block, per channel: min i16, max i16.
//! ```

use std::fs;
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::Serialize;
use thiserror::Error;

use crate::codecs::{AudioReader, ProbeError};

const MAGIC: &[u8; 4] = b"ALPK";
/// 2: shorter files get a finer first level, so zooming in stays smooth.
const VERSION: u32 = 2;
/// Frames per block at each level, finest first.
pub const LEVELS: [u32; 3] = [256, 4096, 65536];

/// The levels for a file of `length` frames at `rate`: the standard three,
/// with a finer one in front for files short enough that it stays small
/// (32 frames up to 10 minutes, 64 up to an hour; about 22 MB at most).
pub fn levels_for(length: u64, rate: u32) -> Vec<u32> {
    let minutes = length as f64 / f64::from(rate.max(1)) / 60.0;
    let mut v = Vec::with_capacity(4);
    if length > 0 && minutes <= 10.0 {
        v.push(32);
    } else if length > 0 && minutes <= 60.0 {
        v.push(64);
    }
    v.extend_from_slice(&LEVELS);
    v
}

#[derive(Debug, Error)]
pub enum PeakError {
    #[error("{0}")]
    Decode(#[from] ProbeError),
    #[error("Peak cache: {0}")]
    Io(#[from] std::io::Error),
    #[error("Peak cache file is damaged.")]
    Corrupt,
    #[error("Cancelled")]
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LevelInfo {
    pub block: u32,
    pub count: u64,
}

/// What the interface needs to draw a file's waveform.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeakSummary {
    pub sample_rate: u32,
    pub channels: u16,
    pub length: u64,
    pub levels: Vec<LevelInfo>,
}

struct Header {
    source_size: u64,
    source_modified: u64,
    summary: PeakSummary,
    data_offset: u64,
}

/// FNV-1a, stable across Rust versions, for cache file names.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// Where the cache for `source` lives inside `cache_dir`.
pub fn cache_path(cache_dir: &Path, source: &Path) -> PathBuf {
    let key = fs::canonicalize(source).unwrap_or_else(|_| source.to_path_buf());
    let name = source
        .file_stem()
        .map(|s| s.to_string_lossy().chars().take(40).collect::<String>())
        .unwrap_or_default();
    cache_dir.join(format!(
        "{name}-{:016x}.alpk",
        fnv1a(key.to_string_lossy().as_bytes())
    ))
}

fn source_stamp(source: &Path) -> Result<(u64, u64), PeakError> {
    let meta = fs::metadata(source).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => PeakError::Decode(ProbeError::NotFound),
        _ => PeakError::Io(e),
    })?;
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    Ok((meta.len(), modified))
}

/// Returns the summary from a valid cache, building it first if needed.
/// `progress` gets values from 0 to 1 and returns false to cancel.
pub fn ensure(
    cache_dir: &Path,
    source: &Path,
    mut progress: impl FnMut(f64) -> bool,
) -> Result<PeakSummary, PeakError> {
    let cache = cache_path(cache_dir, source);
    let stamp = source_stamp(source)?;
    if let Ok(h) = read_header(&cache) {
        if (h.source_size, h.source_modified) == stamp {
            return Ok(h.summary);
        }
    }
    build(source, &cache, stamp, &mut progress)
}

fn build(
    source: &Path,
    cache: &Path,
    stamp: (u64, u64),
    progress: &mut impl FnMut(f64) -> bool,
) -> Result<PeakSummary, PeakError> {
    let info = crate::codecs::probe(source)?;
    let channels = usize::from(info.channels).clamp(1, 2);
    let mut reader = AudioReader::open(source, channels, None)?;
    let blocks = levels_for(info.length, info.sample_rate);
    let block0 = blocks[0] as usize;

    // Level 0, built while streaming.
    let mut level0: Vec<i16> =
        Vec::with_capacity((info.length as usize / block0 + 1) * channels * 2);
    let mut acc_min = vec![f32::MAX; channels];
    let mut acc_max = vec![f32::MIN; channels];
    let mut in_block = 0usize;
    let mut frames_done: u64 = 0;
    let mut buf = Vec::with_capacity(16384);
    let mut last_report = 0u64;

    let flush = |level0: &mut Vec<i16>, acc_min: &mut [f32], acc_max: &mut [f32]| {
        for c in 0..acc_min.len() {
            level0.push(to_i16(acc_min[c]));
            level0.push(to_i16(acc_max[c]));
            acc_min[c] = f32::MAX;
            acc_max[c] = f32::MIN;
        }
    };

    loop {
        buf.clear();
        if !reader.read_into(&mut buf)? {
            break;
        }
        for frame in buf.chunks_exact(channels) {
            for (c, v) in frame.iter().enumerate() {
                acc_min[c] = acc_min[c].min(*v);
                acc_max[c] = acc_max[c].max(*v);
            }
            in_block += 1;
            if in_block == block0 {
                flush(&mut level0, &mut acc_min, &mut acc_max);
                in_block = 0;
            }
        }
        frames_done += (buf.len() / channels) as u64;
        if info.length > 0 && frames_done - last_report > 262_144 {
            last_report = frames_done;
            if !progress((frames_done as f64 / info.length as f64).min(0.99)) {
                return Err(PeakError::Cancelled);
            }
        }
    }
    if in_block > 0 {
        flush(&mut level0, &mut acc_min, &mut acc_max);
    }

    let mut levels: Vec<Vec<i16>> = vec![level0];
    for w in blocks.windows(2) {
        let factor = (w[1] / w[0]) as usize;
        let prev = levels.last().expect("level 0 exists");
        levels.push(merge(prev, channels, factor));
    }

    let summary = PeakSummary {
        sample_rate: info.sample_rate,
        channels: channels as u16,
        // The probe's length accounts for encoder padding; decoding may not.
        length: if info.length > 0 {
            info.length
        } else {
            frames_done
        },
        levels: blocks
            .iter()
            .zip(&levels)
            .map(|(b, d)| LevelInfo {
                block: *b,
                count: (d.len() / (channels * 2)) as u64,
            })
            .collect(),
    };
    write(cache, stamp, &summary, &levels)?;
    progress(1.0);
    Ok(summary)
}

pub(crate) fn to_i16(v: f32) -> i16 {
    if v == f32::MAX || v == f32::MIN {
        return 0;
    }
    (v.clamp(-1.0, 1.0) * 32767.0).round() as i16
}

/// Combines `factor` blocks into one.
fn merge(prev: &[i16], channels: usize, factor: usize) -> Vec<i16> {
    let stride = channels * 2;
    let mut out = Vec::with_capacity((prev.len() / stride / factor + 1) * stride);
    for group in prev.chunks(stride * factor) {
        for c in 0..channels {
            let mut lo = i16::MAX;
            let mut hi = i16::MIN;
            for b in group.chunks_exact(stride) {
                lo = lo.min(b[c * 2]);
                hi = hi.max(b[c * 2 + 1]);
            }
            out.push(lo);
            out.push(hi);
        }
    }
    out
}

fn write(
    path: &Path,
    stamp: (u64, u64),
    summary: &PeakSummary,
    levels: &[Vec<i16>],
) -> Result<(), PeakError> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("alpk.tmp");
    {
        let mut w = BufWriter::new(fs::File::create(&tmp)?);
        w.write_all(MAGIC)?;
        w.write_all(&VERSION.to_le_bytes())?;
        w.write_all(&stamp.0.to_le_bytes())?;
        w.write_all(&stamp.1.to_le_bytes())?;
        w.write_all(&summary.sample_rate.to_le_bytes())?;
        w.write_all(&summary.channels.to_le_bytes())?;
        w.write_all(&0u16.to_le_bytes())?;
        w.write_all(&summary.length.to_le_bytes())?;
        w.write_all(&(summary.levels.len() as u32).to_le_bytes())?;
        for l in &summary.levels {
            w.write_all(&l.block.to_le_bytes())?;
            w.write_all(&l.count.to_le_bytes())?;
        }
        for data in levels {
            for v in data {
                w.write_all(&v.to_le_bytes())?;
            }
        }
        w.flush()?;
        w.get_ref().sync_all()?;
    }
    fs::rename(&tmp, path)?;
    Ok(())
}

fn read_header(path: &Path) -> Result<Header, PeakError> {
    let mut f = fs::File::open(path)?;
    let mut fixed = [0u8; 4 + 4 + 8 + 8 + 4 + 2 + 2 + 8 + 4];
    f.read_exact(&mut fixed).map_err(|_| PeakError::Corrupt)?;
    if &fixed[0..4] != MAGIC {
        return Err(PeakError::Corrupt);
    }
    let u32_at = |i: usize| u32::from_le_bytes(fixed[i..i + 4].try_into().expect("4 bytes"));
    let u64_at = |i: usize| u64::from_le_bytes(fixed[i..i + 8].try_into().expect("8 bytes"));
    if u32_at(4) != VERSION {
        return Err(PeakError::Corrupt);
    }
    let source_size = u64_at(8);
    let source_modified = u64_at(16);
    let sample_rate = u32_at(24);
    let channels = u16::from_le_bytes([fixed[28], fixed[29]]);
    let length = u64_at(32);
    let level_count = u32_at(40) as usize;
    if !(1..=8).contains(&level_count) || !(1..=2).contains(&channels) {
        return Err(PeakError::Corrupt);
    }
    let mut levels = Vec::with_capacity(level_count);
    for _ in 0..level_count {
        let mut b = [0u8; 12];
        f.read_exact(&mut b).map_err(|_| PeakError::Corrupt)?;
        levels.push(LevelInfo {
            block: u32::from_le_bytes(b[0..4].try_into().expect("4 bytes")),
            count: u64::from_le_bytes(b[4..12].try_into().expect("8 bytes")),
        });
    }
    let data_offset = (fixed.len() + level_count * 12) as u64;
    let expected: u64 = levels
        .iter()
        .map(|l| l.count * u64::from(channels) * 4)
        .sum();
    if f.metadata()?.len() != data_offset + expected {
        return Err(PeakError::Corrupt);
    }
    Ok(Header {
        source_size,
        source_modified,
        summary: PeakSummary {
            sample_rate,
            channels,
            length,
            levels,
        },
        data_offset,
    })
}

/// Raw bytes of one level: per block, per channel, min then max, i16 LE.
pub fn read_level(cache_dir: &Path, source: &Path, level: usize) -> Result<Vec<u8>, PeakError> {
    use std::io::{Seek, SeekFrom};
    let path = cache_path(cache_dir, source);
    let h = read_header(&path)?;
    let ch = u64::from(h.summary.channels);
    let li = h.summary.levels.get(level).ok_or(PeakError::Corrupt)?;
    let offset: u64 = h.data_offset
        + h.summary.levels[..level]
            .iter()
            .map(|l| l.count * ch * 4)
            .sum::<u64>();
    let mut f = fs::File::open(&path)?;
    f.seek(SeekFrom::Start(offset))?;
    let mut out = vec![0u8; (li.count * ch * 4) as usize];
    f.read_exact(&mut out)?;
    Ok(out)
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
    fn finer_levels_only_for_shorter_files() {
        assert_eq!(levels_for(48_000 * 60, 48_000), vec![32, 256, 4096, 65536]);
        assert_eq!(
            levels_for(48_000 * 60 * 30, 48_000),
            vec![64, 256, 4096, 65536]
        );
        assert_eq!(
            levels_for(48_000 * 60 * 120, 48_000),
            vec![256, 4096, 65536]
        );
    }

    #[test]
    fn builds_reads_and_reuses_the_cache() {
        let dir = tempfile::tempdir().unwrap();
        let src = media("stereo_48k_16.wav");
        let mut calls = 0;
        let s = ensure(dir.path(), &src, |_| {
            calls += 1;
            true
        })
        .unwrap();
        assert_eq!((s.sample_rate, s.channels, s.length), (48_000, 2, 24_000));
        // A short file starts with the fine 32-frame level.
        assert_eq!(
            s.levels[0],
            LevelInfo {
                block: 32,
                count: 750
            }
        );
        assert_eq!(
            s.levels[1],
            LevelInfo {
                block: 256,
                count: 94
            }
        );
        assert_eq!(s.levels[2].count, 6);
        assert_eq!(s.levels[3].count, 1);

        let bytes = read_level(dir.path(), &src, 1).unwrap();
        assert_eq!(bytes.len(), 94 * 2 * 4);
        let vals: Vec<i16> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| i16::from_le_bytes([b[0], b[1]]))
            .collect();
        // A sine tone swings both ways in every block.
        assert!(vals[0] < -1000 && vals[1] > 1000, "{:?}", &vals[..4]);

        // Second call uses the cache: no progress reports.
        let mut again = 0;
        let s2 = ensure(dir.path(), &src, |_| {
            again += 1;
            true
        })
        .unwrap();
        assert_eq!(s2, s);
        assert_eq!(again, 0);
        assert!(calls >= 1);
    }

    #[test]
    fn coarse_levels_cover_fine_levels() {
        let dir = tempfile::tempdir().unwrap();
        let src = media("tone.mp3");
        ensure(dir.path(), &src, |_| true).unwrap();
        let fine = read_level(dir.path(), &src, 0).unwrap();
        let coarse = read_level(dir.path(), &src, 2).unwrap();
        let v = |b: &[u8], i: usize| i16::from_le_bytes([b[i * 2], b[i * 2 + 1]]);
        let fine_min = (0..fine.len() / 2)
            .step_by(2)
            .map(|i| v(&fine, i))
            .min()
            .unwrap();
        assert_eq!(v(&coarse, 0), fine_min);
    }

    #[test]
    fn damaged_cache_is_rebuilt() {
        let dir = tempfile::tempdir().unwrap();
        let src = media("tone.flac");
        ensure(dir.path(), &src, |_| true).unwrap();
        fs::write(cache_path(dir.path(), &src), b"ALPKgarbage").unwrap();
        let s = ensure(dir.path(), &src, |_| true).unwrap();
        assert_eq!(s.channels, 1);
        assert!(read_level(dir.path(), &src, 0).is_ok());
    }

    #[test]
    fn merge_takes_extremes() {
        // Two blocks, mono: (-5, 3) and (-2, 9) merge to (-5, 9).
        assert_eq!(merge(&[-5, 3, -2, 9], 1, 2), vec![-5, 9]);
    }
}
