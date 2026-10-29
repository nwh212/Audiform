// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

//! Import of `.aup3` project files.
//!
//! A `.aup3` file is an SQLite database. Its `project` table holds the
//! track layout as a compact binary tree of names and values, and its
//! `sampleblocks` table holds the audio in blocks. This reads the file
//! (never writes to it), brings tracks, clips (position, trims, time
//! stretch, pitch, volume line) over, and writes each distinct piece of
//! audio to a WAV file in a folder beside the project, so the new session
//! plays from ordinary files. The project itself is left untouched.
//!
//! This reads the user's own project files for interoperability. It
//! parses the stored layout by example; nothing from the other
//! application is reused.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};
use thiserror::Error;

use crate::session::model::{
    new_id, ChannelLayout, Clip, ClipStretch, EffectChain, Envelope, EnvelopeCurve, EnvelopePoint,
    EnvelopeTarget, Fade, MediaRef, Session, Track, STRETCH_MAX, STRETCH_MIN,
};
use crate::sesx::{ImportReport, Imported};

#[derive(Debug, Error)]
pub enum Aup3Error {
    #[error("Could not read the project: {0}")]
    Db(String),
    #[error("This is not a readable .aup3 project: {0}")]
    Format(String),
    #[error("Could not write the audio files: {0}")]
    Io(#[from] std::io::Error),
    #[error("This project has no audio tracks in it.")]
    NoTracks,
}

impl From<rusqlite::Error> for Aup3Error {
    fn from(e: rusqlite::Error) -> Self {
        Aup3Error::Db(e.to_string())
    }
}

/// A stored value.
#[derive(Debug, Clone, PartialEq)]
enum Val {
    Text(String),
    Num(f64),
}

/// One node of the stored layout.
#[derive(Debug, Default)]
struct El {
    name: String,
    attrs: HashMap<String, Val>,
    kids: Vec<El>,
}

impl El {
    fn num(&self, k: &str) -> Option<f64> {
        match self.attrs.get(k)? {
            Val::Num(n) => Some(*n),
            Val::Text(t) => t.trim().parse().ok(),
        }
    }
    fn text(&self, k: &str) -> Option<&str> {
        match self.attrs.get(k)? {
            Val::Text(t) => Some(t),
            Val::Num(_) => None,
        }
    }
    fn each<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a El> {
        self.kids.iter().filter(move |k| k.name == name)
    }
}

fn bad(msg: &str) -> Aup3Error {
    Aup3Error::Format(msg.to_string())
}

/// Reads `N` bytes at `at`, or fails as a cut-off file.
fn take<const N: usize>(b: &[u8], at: usize) -> Result<[u8; N], Aup3Error> {
    b.get(at..at + N)
        .and_then(|s| s.try_into().ok())
        .ok_or_else(|| bad("the project data ends too soon"))
}

fn utf16(b: &[u8], at: usize, len: usize) -> Result<String, Aup3Error> {
    let raw = b
        .get(at..at + len)
        .ok_or_else(|| bad("the project data ends too soon"))?;
    let units: Vec<u16> = raw
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .collect();
    Ok(String::from_utf16_lossy(&units))
}

/// The table of names the layout refers to by number.
fn names(dict: &[u8]) -> Result<HashMap<u16, String>, Aup3Error> {
    let mut out = HashMap::new();
    let mut i = 2;
    while i < dict.len() {
        let id = u16::from_le_bytes(take(dict, i + 1)?);
        let len = u16::from_le_bytes(take(dict, i + 3)?) as usize;
        out.insert(id, utf16(dict, i + 5, len)?);
        i += 5 + len;
    }
    Ok(out)
}

/// Builds the node tree from the stored layout.
fn tree(names: &HashMap<u16, String>, doc: &[u8]) -> Result<El, Aup3Error> {
    let name_of = |id: u16| names.get(&id).cloned().unwrap_or_default();
    let mut stack: Vec<El> = Vec::new();
    let mut root: Option<El> = None;
    let mut i = 0;
    while i < doc.len() {
        let tag = doc[i];
        let id = |at| -> Result<u16, Aup3Error> { Ok(u16::from_le_bytes(take(doc, at)?)) };
        match tag {
            0x0c => {
                let len = u32::from_le_bytes(take(doc, i + 1)?) as usize;
                i += 5 + len;
            }
            0x01 => {
                stack.push(El {
                    name: name_of(id(i + 1)?),
                    ..El::default()
                });
                i += 3;
            }
            0x02 => {
                let done = stack.pop().ok_or_else(|| bad("unbalanced project data"))?;
                match stack.last_mut() {
                    Some(parent) => parent.kids.push(done),
                    None => root = Some(done),
                }
                i += 3;
            }
            _ => {
                let key = name_of(id(i + 1)?);
                let (val, used) = match tag {
                    0x03 => {
                        let len = u32::from_le_bytes(take(doc, i + 3)?) as usize;
                        (Val::Text(utf16(doc, i + 7, len)?), 7 + len)
                    }
                    0x04 | 0x06 => (Val::Num(f64::from(i32::from_le_bytes(take(doc, i + 3)?))), 7),
                    0x05 => (Val::Num(f64::from(doc.get(i + 3).copied().unwrap_or(0))), 4),
                    0x07 => (Val::Num(i64::from_le_bytes(take(doc, i + 3)?) as f64), 11),
                    0x08 => (Val::Num(f64::from(u32::from_le_bytes(take(doc, i + 3)?))), 7),
                    0x09 => (Val::Num(f64::from(f32::from_le_bytes(take(doc, i + 3)?))), 11),
                    0x0a => (Val::Num(f64::from_le_bytes(take(doc, i + 3)?)), 15),
                    _ => return Err(bad("unknown value kind in the project data")),
                };
                if let Some(top) = stack.last_mut() {
                    top.attrs.insert(key, val);
                }
                i += used;
            }
        }
    }
    root.ok_or_else(|| bad("the project data is empty"))
}

/// A run of audio blocks: one channel of one recording.
#[derive(Debug, Clone, PartialEq)]
struct Blocks {
    ids: Vec<i64>,
    frames: u64,
}

/// A clip as stored, before its audio is placed.
struct Raw {
    name: String,
    offset: f64,
    trim_left: f64,
    trim_right: f64,
    ratio: f64,
    cents: f64,
    blocks: Blocks,
    points: Vec<(f64, f64)>,
}

struct RawTrack {
    name: String,
    channel: i32,
    linked: bool,
    muted: bool,
    solo: bool,
    rate: u32,
    gain: f64,
    pan: f64,
    clips: Vec<Raw>,
}

fn raw_clip(c: &El, track: &str) -> Option<Raw> {
    let seq = c.each("sequence").next()?;
    let mut blocks: Vec<(f64, i64)> = seq
        .each("waveblock")
        .filter_map(|b| Some((b.num("start")?, b.num("blockid")? as i64)))
        .collect();
    blocks.sort_by(|a, b| a.0.total_cmp(&b.0));
    let points = c
        .each("envelope")
        .next()
        .map(|e| {
            e.each("controlpoint")
                .filter_map(|p| Some((p.num("t")?, p.num("val")?)))
                .collect()
        })
        .unwrap_or_default();
    let name = c.text("name").filter(|n| !n.is_empty()).unwrap_or(track);
    Some(Raw {
        name: name.to_string(),
        offset: c.num("offset").unwrap_or(0.0),
        trim_left: c.num("trimLeft").unwrap_or(0.0),
        trim_right: c.num("trimRight").unwrap_or(0.0),
        ratio: c.num("clipStretchRatio").unwrap_or(1.0),
        cents: c.num("centShift").unwrap_or(0.0),
        blocks: Blocks {
            ids: blocks.into_iter().map(|b| b.1).collect(),
            frames: seq.num("numsamples").unwrap_or(0.0).max(0.0) as u64,
        },
        points,
    })
}

fn raw_tracks(root: &El) -> Vec<RawTrack> {
    root.each("wavetrack")
        .map(|t| {
            let name = t.text("name").unwrap_or("Track").to_string();
            let mut clips: Vec<Raw> = t.each("waveclip").filter_map(|c| raw_clip(c, &name)).collect();
            clips.sort_by(|a, b| (a.offset + a.trim_left).total_cmp(&(b.offset + b.trim_left)));
            RawTrack {
                channel: t.num("channel").unwrap_or(2.0) as i32,
                linked: t.num("linked").unwrap_or(0.0) != 0.0,
                muted: t.num("mute").unwrap_or(0.0) != 0.0,
                solo: t.num("solo").unwrap_or(0.0) != 0.0,
                rate: t.num("rate").unwrap_or(44100.0).max(1.0) as u32,
                gain: t.num("gain").unwrap_or(1.0),
                pan: t.num("pan").unwrap_or(0.0),
                name,
                clips,
            }
        })
        .collect()
}

/// How the stored samples are laid out.
#[derive(Clone, Copy)]
enum Format {
    Float,
    Int16,
    Int24,
}

impl Format {
    fn of(code: i64) -> Option<Self> {
        match code {
            0x4000F => Some(Self::Float),
            0x20001 => Some(Self::Int16),
            0x40001 => Some(Self::Int24),
            _ => None,
        }
    }
    fn width(self) -> usize {
        match self {
            Self::Float | Self::Int24 => 4,
            Self::Int16 => 2,
        }
    }
    fn decode(self, b: &[u8], out: &mut Vec<f32>) {
        for c in b.chunks_exact(self.width()) {
            out.push(match self {
                Self::Float => f32::from_le_bytes([c[0], c[1], c[2], c[3]]),
                Self::Int16 => f32::from(i16::from_le_bytes([c[0], c[1]])) / 32768.0,
                // 24-bit audio is kept in a 32-bit integer.
                Self::Int24 => i32::from_le_bytes([c[0], c[1], c[2], c[3]]) as f32 / 8_388_608.0,
            });
        }
    }
}

/// Reads one channel's blocks in order.
struct Reader<'a> {
    db: &'a Connection,
    ids: std::vec::IntoIter<i64>,
    left: u64,
    pending: Vec<f32>,
    at: usize,
}

impl<'a> Reader<'a> {
    fn new(db: &'a Connection, b: &Blocks) -> Self {
        Self {
            db,
            ids: b.ids.clone().into_iter(),
            left: b.frames,
            pending: Vec::new(),
            at: 0,
        }
    }

    /// Up to `n` samples; fewer only at the end. Missing blocks are silence.
    fn read(&mut self, n: usize, out: &mut Vec<f32>) -> Result<(), Aup3Error> {
        let want = (n as u64).min(self.left) as usize;
        let mut got = 0;
        while got < want {
            if self.at >= self.pending.len() {
                self.pending.clear();
                self.at = 0;
                match self.ids.next() {
                    Some(id) => {
                        let row: Option<(i64, Vec<u8>)> = self
                            .db
                            .query_row(
                                "SELECT sampleformat, samples FROM sampleblocks WHERE blockid = ?1",
                                [id],
                                |r| Ok((r.get(0)?, r.get(1)?)),
                            )
                            .ok();
                        if let Some((code, bytes)) = row {
                            let f = Format::of(code).ok_or_else(|| bad("an unsupported sample format"))?;
                            f.decode(&bytes, &mut self.pending);
                        }
                    }
                    None => break,
                }
                if self.pending.is_empty() {
                    continue;
                }
            }
            let take = (want - got).min(self.pending.len() - self.at);
            out.extend_from_slice(&self.pending[self.at..self.at + take]);
            self.at += take;
            got += take;
        }
        out.resize(out.len() + (want - got), 0.0);
        self.left -= want as u64;
        Ok(())
    }
}

/// One distinct recording to write out: one or two channels of blocks.
struct Source {
    channels: Vec<Blocks>,
    rate: u32,
    media: Option<MediaRef>,
}

/// Writes a source to a WAV file (32-bit float) and returns its length.
fn write_source(db: &Connection, s: &Source, path: &Path) -> Result<u64, Aup3Error> {
    let spec = hound::WavSpec {
        channels: s.channels.len() as u16,
        sample_rate: s.rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut w = hound::WavWriter::create(path, spec).map_err(|e| std::io::Error::other(e.to_string()))?;
    let mut readers: Vec<Reader> = s.channels.iter().map(|b| Reader::new(db, b)).collect();
    let frames = s.channels.iter().map(|b| b.frames).min().unwrap_or(0);
    let mut done = 0u64;
    const CHUNK: usize = 1 << 16;
    let mut cols: Vec<Vec<f32>> = vec![Vec::new(); readers.len()];
    while done < frames {
        let n = CHUNK.min((frames - done) as usize);
        for (r, c) in readers.iter_mut().zip(cols.iter_mut()) {
            c.clear();
            r.read(n, c)?;
        }
        for i in 0..n {
            for c in &cols {
                w.write_sample(c[i]).map_err(|e| std::io::Error::other(e.to_string()))?;
            }
        }
        done += n as u64;
    }
    w.finalize().map_err(|e| std::io::Error::other(e.to_string()))?;
    Ok(frames)
}

fn to_db(gain: f64) -> f64 {
    if gain <= 0.000_01 {
        -96.0
    } else {
        ((20.0 * gain.log10()) * 10.0).round() / 10.0
    }
}

/// The line's points as seconds from the clip's left edge, cut to the
/// clip, with points added where it crosses the ends.
fn cut_line(points: &[(f64, f64)], origin: f64, length: f64) -> Vec<(f64, f64)> {
    let moved: Vec<(f64, f64)> = points.iter().map(|&(t, v)| (t - origin, v)).collect();
    if moved.is_empty() {
        return moved;
    }
    let at = |x: f64| -> f64 {
        let (first, last) = (moved[0], moved[moved.len() - 1]);
        if x <= first.0 {
            return first.1;
        }
        if x >= last.0 {
            return last.1;
        }
        let k = (1..moved.len()).find(|&i| x <= moved[i].0).unwrap_or(1);
        let (a, b) = (moved[k - 1], moved[k]);
        if b.0 - a.0 <= 0.0 {
            b.1
        } else {
            a.1 + (b.1 - a.1) * (x - a.0) / (b.0 - a.0)
        }
    };
    let mut out = vec![(0.0, at(0.0))];
    out.extend(moved.iter().copied().filter(|&(p, _)| p > 0.0 && p < length));
    if length > 0.0 {
        out.push((length, at(length)));
    }
    out
}

/// Where the audio of an import goes: a folder beside the project.
pub fn audio_folder(project: &Path) -> PathBuf {
    let stem = project.file_stem().map_or_else(|| "Project".into(), |s| s.to_string_lossy().into_owned());
    project.with_file_name(format!("{stem} audio"))
}

/// Reads a `.aup3` project and builds a session from it, writing the
/// audio to `audio_dir`.
pub fn import(path: &Path, audio_dir: &Path) -> Result<Imported, Aup3Error> {
    let db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let (dict, doc): (Vec<u8>, Vec<u8>) = db
        .query_row("SELECT dict, doc FROM project LIMIT 1", [], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(|_| bad("it has no saved project data (it may only hold an autosave)"))?;
    let root = tree(&names(&dict)?, &doc)?;
    if root.name != "project" {
        return Err(bad("the project data does not start as expected"));
    }
    let project_rate = root.num("rate").unwrap_or(44100.0).clamp(8000.0, 384_000.0) as u32;
    let stem = path.file_stem().map_or_else(|| "Project".into(), |s| s.to_string_lossy().into_owned());
    let mut session = Session::new(stem.clone(), project_rate);
    let mut report = ImportReport::default();
    let raw = raw_tracks(&root);

    // Left and right tracks of a stereo pair become one stereo track.
    let mut layout: Vec<(usize, Option<usize>)> = Vec::new();
    let mut i = 0;
    while i < raw.len() {
        let t = &raw[i];
        match raw.get(i + 1) {
            Some(r) if t.channel == 0 && t.linked && r.channel == 1 && r.clips.len() == t.clips.len() => {
                layout.push((i, Some(i + 1)));
                i += 2;
            }
            _ => {
                layout.push((i, None));
                i += 1;
            }
        }
    }
    if layout.iter().all(|&(i, _)| raw[i].clips.is_empty()) && layout.is_empty() {
        return Err(Aup3Error::NoTracks);
    }

    // Clips often share one recording (a split keeps the same blocks):
    // one source per distinct start, long enough for every clip using it.
    let mut sources: Vec<Source> = Vec::new();
    let mut lookup: HashMap<Vec<i64>, usize> = HashMap::new();
    let mut source_of = |chs: Vec<&Blocks>, rate: u32| -> usize {
        let key: Vec<i64> = chs.iter().map(|b| b.ids.first().copied().unwrap_or(-1)).collect();
        match lookup.get(&key) {
            Some(&k) => {
                for (have, new) in sources[k].channels.iter_mut().zip(chs) {
                    if new.ids.len() > have.ids.len() && new.ids.starts_with(&have.ids) {
                        *have = new.clone();
                    }
                }
                k
            }
            None => {
                sources.push(Source {
                    channels: chs.into_iter().cloned().collect(),
                    rate,
                    media: None,
                });
                lookup.insert(key, sources.len() - 1);
                sources.len() - 1
            }
        }
    };
    struct Plan {
        track: usize,
        clip: usize,
        source: usize,
    }
    let mut plans: Vec<Plan> = Vec::new();
    for (n, &(l, r)) in layout.iter().enumerate() {
        for (k, c) in raw[l].clips.iter().enumerate() {
            if c.blocks.ids.is_empty() {
                continue;
            }
            let chs = match r {
                Some(r) => vec![&c.blocks, &raw[r].clips[k].blocks],
                None => vec![&c.blocks],
            };
            plans.push(Plan { track: n, clip: k, source: source_of(chs, raw[l].rate) });
        }
    }

    std::fs::create_dir_all(audio_dir)?;
    for (n, s) in sources.iter_mut().enumerate() {
        let file = audio_dir.join(format!("{stem} {:02}.wav", n + 1));
        let frames = write_source(&db, s, &file)?;
        s.media = Some(MediaRef {
            id: new_id(),
            path: file,
            relative_path: None,
            sample_rate: s.rate,
            channels: s.channels.len() as u16,
            length: frames,
        });
    }
    session.media = sources.iter().filter_map(|s| s.media.clone()).collect();

    for (n, &(l, r)) in layout.iter().enumerate() {
        let t = &raw[l];
        let mut track = Track::new(
            t.name.clone(),
            if r.is_some() { ChannelLayout::Stereo } else { ChannelLayout::Mono },
        );
        track.volume_db = to_db(t.gain).max(-60.0);
        track.pan = (t.pan * 100.0).clamp(-100.0, 100.0);
        track.muted = t.muted;
        track.solo = t.solo;
        let sr = f64::from(project_rate);
        for p in plans.iter().filter(|p| p.track == n) {
            let c = &t.clips[p.clip];
            let src = &sources[p.source];
            let media = src.media.as_ref().ok_or_else(|| bad("missing audio"))?;
            let mrate = f64::from(media.sample_rate);
            let ratio = if c.ratio.is_finite() && c.ratio > 0.0 { c.ratio.clamp(STRETCH_MIN, STRETCH_MAX) } else { 1.0 };
            let natural = c.blocks.frames as f64 / mrate;
            let mut left = c.trim_left.max(0.0);
            let mut at = c.offset + left;
            // A clip that starts before zero loses its head.
            if at < 0.0 {
                left -= at;
                at = 0.0;
            }
            let seen = (natural * ratio - left - c.trim_right.max(0.0)).max(0.0);
            let source_in = ((left / ratio) * mrate).round() as u64;
            let source_out = (source_in + ((seen / ratio) * mrate).round() as u64).min(c.blocks.frames.min(media.length));
            if source_out <= source_in {
                continue;
            }
            let stretched = (ratio - 1.0).abs() > 1e-6 || c.cents.abs() > 0.5;
            if stretched {
                report.stretched += 1;
            }
            let mut envelopes = Vec::new();
            let line = cut_line(&c.points, left, seen);
            if line.iter().any(|&(_, v)| (v - 1.0).abs() > 0.001) {
                envelopes.push(Envelope {
                    id: new_id(),
                    target: EnvelopeTarget::Volume,
                    enabled: true,
                    points: line
                        .iter()
                        .map(|&(s, v)| EnvelopePoint {
                            position: (s * sr).round() as u64,
                            value: to_db(v),
                            curve: EnvelopeCurve::Linear,
                        })
                        .collect(),
                    height: None,
                });
            }
            track.clips.push(Clip {
                id: new_id(),
                name: c.name.clone(),
                media_id: media.id,
                start: (at * sr).round() as u64,
                source_in,
                source_out,
                gain_db: 0.0,
                muted: false,
                locked: false,
                color: None,
                fade_in: Fade::default(),
                fade_out: Fade::default(),
                envelopes,
                effects: EffectChain::default(),
                group: None,
                looped: None,
                stretch: stretched.then(|| ClipStretch {
                    ratio,
                    keep_pitch: true,
                    pitch: (c.cents / 100.0).clamp(-24.0, 24.0),
                    polyphonic: true,
                    rendered: false,
                    advanced: None,
                }),
                audio_type: None,
                channel_mix: None,
                reversed: false,
                inverted: false,
                invert_channel: None,
            });
            report.clips += 1;
        }
        session.tracks.push(track);
        report.tracks += 1;
    }
    Ok(Imported { session, report, missing: Vec::new() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_is_cut_to_the_clip() {
        let l = cut_line(&[(0.0, 1.0), (10.0, 0.0)], 5.0, 2.0);
        assert_eq!(l.len(), 2);
        assert!((l[0].1 - 0.5).abs() < 1e-9);
        assert!((l[1].1 - 0.3).abs() < 1e-9);
    }

    #[test]
    fn gain_in_decibels() {
        assert_eq!(to_db(1.0), 0.0);
        assert_eq!(to_db(0.0), -96.0);
        assert!((to_db(0.5) + 6.0).abs() < 0.01);
    }

    /// Set `AUP3_TEST` to a project to try the real thing.
    #[test]
    #[ignore]
    fn real_project() {
        let Ok(p) = std::env::var("AUP3_TEST") else { return };
        let out = std::env::temp_dir().join("aup3-test-audio");
        let got = import(Path::new(&p), &out).unwrap();
        eprintln!("{:?}", got.report);
        assert!(got.report.clips > 0);
    }
}
