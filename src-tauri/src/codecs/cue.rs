// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Markers stored inside WAV files as cue points, the way most audio
//! editors and broadcast tools keep them: a `cue ` chunk with each
//! point's position, and a `LIST`/`adtl` chunk with a `labl` (name) per
//! point and an `ltxt` (length) for ranges. Positions are sample frames.

use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cue {
    /// Sample frame where the marker starts.
    pub position: u64,
    /// Length in frames; 0 for a point.
    pub length: u64,
    pub label: String,
}

fn u32le(b: &[u8], at: usize) -> Option<u32> {
    b.get(at..at + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

/// The cue points in a WAV file. Empty for other files or when there are none.
pub fn read_cues(path: &Path) -> Vec<Cue> {
    fs::read(path).map(|d| parse(&d)).unwrap_or_default()
}

pub fn parse(d: &[u8]) -> Vec<Cue> {
    if d.len() < 12 || &d[..4] != b"RIFF" || &d[8..12] != b"WAVE" {
        return Vec::new();
    }
    let mut points: Vec<(u32, u64)> = Vec::new();
    let mut labels: Vec<(u32, String)> = Vec::new();
    let mut lengths: Vec<(u32, u64)> = Vec::new();
    let mut at = 12;
    while at + 8 <= d.len() {
        let id = &d[at..at + 4];
        let Some(size) = u32le(d, at + 4).map(|n| n as usize) else {
            break;
        };
        let body = &d[(at + 8).min(d.len())..(at + 8 + size).min(d.len())];
        if id == b"cue " {
            let n = u32le(body, 0).unwrap_or(0) as usize;
            for i in 0..n {
                let p = 4 + i * 24;
                if let (Some(name), Some(offset)) = (u32le(body, p), u32le(body, p + 20)) {
                    points.push((name, u64::from(offset)));
                }
            }
        } else if id == b"LIST" && body.get(..4) == Some(b"adtl") {
            let mut q = 4;
            while q + 8 <= body.len() {
                let sid = &body[q..q + 4];
                let Some(ssize) = u32le(body, q + 4).map(|n| n as usize) else {
                    break;
                };
                let sb = &body[(q + 8).min(body.len())..(q + 8 + ssize).min(body.len())];
                if let Some(cue) = u32le(sb, 0) {
                    if sid == b"labl" {
                        let text = &sb[4.min(sb.len())..];
                        let end = text.iter().position(|c| *c == 0).unwrap_or(text.len());
                        labels.push((cue, String::from_utf8_lossy(&text[..end]).into_owned()));
                    } else if sid == b"ltxt" {
                        lengths.push((cue, u64::from(u32le(sb, 4).unwrap_or(0))));
                    }
                }
                q += 8 + ssize + (ssize & 1);
            }
        }
        at += 8 + size + (size & 1);
    }
    let mut out: Vec<Cue> = points
        .into_iter()
        .map(|(name, position)| Cue {
            position,
            length: lengths
                .iter()
                .find(|(c, _)| *c == name)
                .map(|(_, l)| *l)
                .unwrap_or(0),
            label: labels
                .iter()
                .find(|(c, _)| *c == name)
                .map(|(_, l)| l.clone())
                .unwrap_or_else(|| format!("Marker {name}")),
        })
        .collect();
    out.sort_by_key(|c| c.position);
    out
}

fn chunk(id: &[u8; 4], body: &[u8], out: &mut Vec<u8>) {
    out.extend_from_slice(id);
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(body);
    if body.len() % 2 == 1 {
        out.push(0);
    }
}

/// The `cue ` and `LIST`/`adtl` chunks for `cues`.
pub fn chunks(cues: &[Cue]) -> Vec<u8> {
    let mut out = Vec::new();
    if cues.is_empty() {
        return out;
    }
    let mut cue = (cues.len() as u32).to_le_bytes().to_vec();
    for (i, c) in cues.iter().enumerate() {
        let pos = c.position.min(u64::from(u32::MAX)) as u32;
        cue.extend_from_slice(&(i as u32 + 1).to_le_bytes());
        cue.extend_from_slice(&pos.to_le_bytes());
        cue.extend_from_slice(b"data");
        cue.extend_from_slice(&0u32.to_le_bytes());
        cue.extend_from_slice(&0u32.to_le_bytes());
        cue.extend_from_slice(&pos.to_le_bytes());
    }
    chunk(b"cue ", &cue, &mut out);
    let mut adtl = b"adtl".to_vec();
    for (i, c) in cues.iter().enumerate() {
        let id = (i as u32 + 1).to_le_bytes();
        let mut labl = id.to_vec();
        labl.extend_from_slice(c.label.as_bytes());
        labl.push(0);
        chunk(b"labl", &labl, &mut adtl);
        if c.length > 0 {
            let mut ltxt = id.to_vec();
            ltxt.extend_from_slice(&(c.length.min(u64::from(u32::MAX)) as u32).to_le_bytes());
            ltxt.extend_from_slice(b"rgn ");
            ltxt.extend_from_slice(&[0u8; 8]);
            chunk(b"ltxt", &ltxt, &mut adtl);
        }
    }
    chunk(b"LIST", &adtl, &mut out);
    out
}

/// Adds cue points to the end of a WAV file and fixes the RIFF size.
pub fn write_cues(path: &Path, cues: &[Cue]) -> std::io::Result<()> {
    if cues.is_empty() {
        return Ok(());
    }
    let mut d = fs::read(path)?;
    if d.len() < 12 || &d[..4] != b"RIFF" || &d[8..12] != b"WAVE" {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Not a WAV file.",
        ));
    }
    if d.len() % 2 == 1 {
        d.push(0);
    }
    d.extend(chunks(cues));
    let riff = (d.len() - 8) as u32;
    d[4..8].copy_from_slice(&riff.to_le_bytes());
    fs::write(path, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_and_reads_back_points_and_ranges() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.wav");
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 48_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(&p, spec).unwrap();
        for _ in 0..48_000 {
            w.write_sample(0i16).unwrap();
        }
        w.finalize().unwrap();
        let cues = vec![
            Cue {
                position: 1_000,
                length: 0,
                label: "Breath".into(),
            },
            Cue {
                position: 24_000,
                length: 4_800,
                label: "Retake this".into(),
            },
        ];
        write_cues(&p, &cues).unwrap();
        assert_eq!(read_cues(&p), cues);
        // The file still reads as audio of the same length.
        let r = hound::WavReader::open(&p).unwrap();
        assert_eq!(r.duration(), 48_000);
    }

    #[test]
    fn other_files_have_no_cues() {
        assert!(parse(b"fLaC....").is_empty());
    }
}
