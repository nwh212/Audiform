// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Chapters written into the exported file itself, so podcast apps show
//! them without a separate chapters file.
//!
//! MP3: an ID3v2.3 tag at the front with one CHAP frame per chapter and a
//! CTOC table of contents, as the ID3 chapter addendum describes. Most
//! podcast apps read these.
//!
//! FLAC: Vorbis comments, `CHAPTER001=00:00:00.000` and
//! `CHAPTER001NAME=Intro`, in a VORBIS_COMMENT block after STREAMINFO.
//!
//! Both are added after the audio is written, by rewriting the file once.

use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::Path;

use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Chapter {
    pub title: String,
    pub start_ms: u64,
    /// The next chapter's start, or the end of the file, when not given.
    pub end_ms: Option<u64>,
}

/// ID3v2 sizes are 7 bits per byte.
pub(crate) fn syncsafe(n: u32) -> [u8; 4] {
    [
        ((n >> 21) & 0x7f) as u8,
        ((n >> 14) & 0x7f) as u8,
        ((n >> 7) & 0x7f) as u8,
        (n & 0x7f) as u8,
    ]
}

/// One ID3v2.3 frame: id, 32-bit size, no flags, body.
pub(crate) fn frame(id: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut f = Vec::with_capacity(10 + body.len());
    f.extend_from_slice(id);
    f.extend_from_slice(&(body.len() as u32).to_be_bytes());
    f.extend_from_slice(&[0, 0]);
    f.extend_from_slice(body);
    f
}

/// A title frame in UTF-16 with a byte order mark, which ID3v2.3 readers all accept.
fn title(text: &str) -> Vec<u8> {
    text_frame(b"TIT2", text)
}

/// Any ID3v2.3 text frame (TIT2, TPE1 and so on), UTF-16 with a BOM.
pub(crate) fn text_frame(id: &[u8; 4], text: &str) -> Vec<u8> {
    let mut body = vec![1u8, 0xff, 0xfe];
    for u in text.encode_utf16() {
        body.extend_from_slice(&u.to_le_bytes());
    }
    body.extend_from_slice(&[0, 0]);
    frame(id, &body)
}

/// The ID3v2.3 tag holding the chapters. `total_ms` ends the last one.
pub fn id3_tag(chapters: &[Chapter], total_ms: u64) -> Vec<u8> {
    let mut frames = Vec::new();
    let ids: Vec<String> = (1..=chapters.len()).map(|i| format!("chp{i}")).collect();
    // Table of contents: top level, ordered, every chapter.
    let mut toc = b"toc\0".to_vec();
    toc.push(0b11);
    toc.push(chapters.len().min(255) as u8);
    for id in ids.iter().take(255) {
        toc.extend_from_slice(id.as_bytes());
        toc.push(0);
    }
    frames.extend(frame(b"CTOC", &toc));
    for (i, c) in chapters.iter().enumerate() {
        let end = c
            .end_ms
            .or_else(|| chapters.get(i + 1).map(|n| n.start_ms))
            .unwrap_or(total_ms)
            .max(c.start_ms);
        let mut body = ids[i].as_bytes().to_vec();
        body.push(0);
        body.extend_from_slice(&(c.start_ms.min(u64::from(u32::MAX)) as u32).to_be_bytes());
        body.extend_from_slice(&(end.min(u64::from(u32::MAX)) as u32).to_be_bytes());
        // No byte offsets: readers use the times.
        body.extend_from_slice(&[0xff; 8]);
        body.extend(title(&c.title));
        frames.extend(frame(b"CHAP", &body));
    }
    let mut tag = b"ID3".to_vec();
    tag.extend_from_slice(&[3, 0, 0]);
    tag.extend_from_slice(&syncsafe(frames.len() as u32));
    tag.extend(frames);
    tag
}

/// Length of an ID3v2 tag at the start of `data`, if there is one.
pub(crate) fn existing_id3(data: &[u8]) -> usize {
    if data.len() >= 10 && &data[..3] == b"ID3" {
        let size = data[6..10]
            .iter()
            .fold(0usize, |n, b| (n << 7) | usize::from(b & 0x7f));
        let footer = if data[5] & 0x10 != 0 { 10 } else { 0 };
        (10 + size + footer).min(data.len())
    } else {
        0
    }
}

/// Puts the chapters at the front of an MP3 file. Other frames in an
/// existing ID3v2.3 tag (title, artist and so on) are kept.
pub fn embed_mp3(path: &Path, chapters: &[Chapter], total_ms: u64) -> std::io::Result<()> {
    let data = fs::read(path)?;
    let n = existing_id3(&data);
    let mut tag = id3_tag(chapters, total_ms);
    let kept: Vec<u8> = id3_frames(&data[..n])
        .into_iter()
        .filter(|(id, _)| id != b"CHAP" && id != b"CTOC")
        .flat_map(|(id, body)| frame(&id, &body))
        .collect();
    tag = with_frames(&tag, &kept);
    tag.extend_from_slice(&data[n..]);
    replace(path, &tag)
}

/// The frames of an ID3v2.3 tag, as (id, body). Other versions give none:
/// their frames do not carry over to a 2.3 tag as they are.
pub(crate) fn id3_frames(tag: &[u8]) -> Vec<([u8; 4], Vec<u8>)> {
    let mut out = Vec::new();
    if tag.len() < 10 || &tag[..3] != b"ID3" || tag[3] != 3 {
        return out;
    }
    let mut at = 10;
    // An extended header, when flagged, comes first.
    if tag[5] & 0x40 != 0 && tag.len() >= 14 {
        at += 4 + u32::from_be_bytes([tag[10], tag[11], tag[12], tag[13]]) as usize;
    }
    while at + 10 <= tag.len() && tag[at] != 0 {
        let id = [tag[at], tag[at + 1], tag[at + 2], tag[at + 3]];
        let len = u32::from_be_bytes([tag[at + 4], tag[at + 5], tag[at + 6], tag[at + 7]]) as usize;
        let Some(body) = tag.get(at + 10..at + 10 + len) else {
            break;
        };
        out.push((id, body.to_vec()));
        at += 10 + len;
    }
    out
}

/// Adds raw frames to the end of a tag built here, fixing its size.
pub(crate) fn with_frames(tag: &[u8], frames: &[u8]) -> Vec<u8> {
    let mut out = tag[..6].to_vec();
    out.extend_from_slice(&syncsafe((tag.len() - 10 + frames.len()) as u32));
    out.extend_from_slice(&tag[10..]);
    out.extend_from_slice(frames);
    out
}

/// An empty ID3v2.3 tag header.
pub(crate) fn empty_id3() -> Vec<u8> {
    let mut tag = b"ID3".to_vec();
    tag.extend_from_slice(&[3, 0, 0, 0, 0, 0, 0]);
    tag
}

fn clock(ms: u64) -> String {
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        ms / 3_600_000,
        (ms / 60_000) % 60,
        (ms / 1000) % 60,
        ms % 1000
    )
}

/// The chapters as Vorbis comment lines.
pub fn chapter_comments(chapters: &[Chapter]) -> Vec<String> {
    let mut items: Vec<String> = Vec::new();
    for (i, c) in chapters.iter().enumerate() {
        items.push(format!("CHAPTER{:03}={}", i + 1, clock(c.start_ms)));
        items.push(format!("CHAPTER{:03}NAME={}", i + 1, c.title));
    }
    items
}

/// A VORBIS_COMMENT block body holding `items` ("KEY=value" lines).
pub fn vorbis_block(items: &[String]) -> Vec<u8> {
    let vendor = b"Audiform";
    let mut b = Vec::new();
    b.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
    b.extend_from_slice(vendor);
    b.extend_from_slice(&(items.len() as u32).to_le_bytes());
    for s in items {
        b.extend_from_slice(&(s.len() as u32).to_le_bytes());
        b.extend_from_slice(s.as_bytes());
    }
    b
}

/// The comment lines in a VORBIS_COMMENT block body.
pub(crate) fn vorbis_items(body: &[u8]) -> Vec<String> {
    let u32_at = |i: usize| {
        body.get(i..i + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
    };
    let mut out = Vec::new();
    let Some(vendor) = u32_at(0) else {
        return out;
    };
    let mut at = 4 + vendor;
    let Some(count) = u32_at(at) else {
        return out;
    };
    at += 4;
    for _ in 0..count {
        let Some(len) = u32_at(at) else {
            break;
        };
        let Some(text) = body.get(at + 4..at + 4 + len) else {
            break;
        };
        out.push(String::from_utf8_lossy(text).into_owned());
        at += 4 + len;
    }
    out
}

/// Adds the chapters to the FLAC file's Vorbis comments, replacing old
/// chapters and keeping every other comment.
pub fn embed_flac(path: &Path, chapters: &[Chapter]) -> std::io::Result<()> {
    let ours = chapter_comments(chapters);
    set_flac_comments(path, |old| {
        let mut v: Vec<String> = old
            .into_iter()
            .filter(|c| !c.to_ascii_uppercase().starts_with("CHAPTER"))
            .collect();
        v.extend(ours);
        v
    })
}

/// Rewrites the FLAC file's Vorbis comments: `edit` gets the old lines and
/// returns the new ones. The comment block goes straight after STREAMINFO.
pub(crate) fn set_flac_comments(
    path: &Path,
    edit: impl FnOnce(Vec<String>) -> Vec<String>,
) -> std::io::Result<()> {
    let bad = || std::io::Error::new(std::io::ErrorKind::InvalidData, "Not a FLAC file.");
    let mut src = BufReader::new(File::open(path)?);
    let mut magic = [0u8; 4];
    src.read_exact(&mut magic).map_err(|_| bad())?;
    if &magic != b"fLaC" {
        return Err(bad());
    }
    // Read only the metadata blocks, keeping all but the old Vorbis comment. The audio
    // after them can be hours long, so it is copied across and never held in memory.
    let mut blocks: Vec<(u8, Vec<u8>)> = Vec::new();
    let mut old = Vec::new();
    loop {
        let mut head = [0u8; 4];
        src.read_exact(&mut head).map_err(|_| bad())?;
        let len = (usize::from(head[1]) << 16) | (usize::from(head[2]) << 8) | usize::from(head[3]);
        let mut body = vec![0u8; len];
        src.read_exact(&mut body).map_err(|_| bad())?;
        let kind = head[0] & 0x7f;
        if kind == 4 {
            old.extend(vorbis_items(&body));
        } else {
            blocks.push((kind, body));
        }
        if head[0] & 0x80 != 0 {
            break;
        }
    }
    let comments = vorbis_block(&edit(old));
    blocks.insert(1.min(blocks.len()), (4, comments));
    // Written beside the file, then swapped in, so a failure never leaves half a file.
    let tmp = path.with_extension("chapters.tmp");
    let written = write_flac_copy(&tmp, &blocks, &mut src);
    // Closed before the swap: Windows will not replace a file that is still open.
    drop(src);
    match written.and_then(|_| fs::rename(&tmp, path)) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            Err(e)
        }
    }
}

/// A new FLAC file at `tmp`: the marker, `blocks` (the last flagged as last), then
/// everything left in `rest`, which is the audio.
fn write_flac_copy(
    tmp: &Path,
    blocks: &[(u8, Vec<u8>)],
    rest: &mut impl Read,
) -> std::io::Result<()> {
    let mut out = BufWriter::new(File::create(tmp)?);
    out.write_all(b"fLaC")?;
    let last = blocks.len() - 1;
    for (i, (kind, body)) in blocks.iter().enumerate() {
        out.write_all(&[if i == last { kind | 0x80 } else { *kind }])?;
        out.write_all(&(body.len() as u32).to_be_bytes()[1..])?;
        out.write_all(body)?;
    }
    std::io::copy(rest, &mut out)?;
    out.flush()
}

/// Adds the chapters to a WAV file as cue points with labels (`cue ` and
/// `LIST`/`adtl`), which audio editors and broadcast tools read. The audio
/// is not rewritten: the chunks go on the end and the RIFF size is fixed.
/// Plain RIFF only; a file that already has cue points is left alone.
pub fn embed_wav(path: &Path, chapters: &[Chapter]) -> std::io::Result<()> {
    use crate::codecs::cue::{chunks, Cue};
    let bad = |m: &str| std::io::Error::new(std::io::ErrorKind::InvalidData, m.to_string());
    if chapters.is_empty() {
        return Ok(());
    }
    let mut f = fs::OpenOptions::new().read(true).write(true).open(path)?;
    let mut head = [0u8; 12];
    f.read_exact(&mut head)
        .map_err(|_| bad("Not a WAV file."))?;
    if &head[..4] == b"RF64" {
        return Err(bad("Markers cannot go into RF64 files (over 4 GB) yet."));
    }
    if &head[..4] != b"RIFF" || &head[8..12] != b"WAVE" {
        return Err(bad("Not a WAV file."));
    }
    let len = f.metadata()?.len();
    let mut rate = 0u32;
    let mut at = 12u64;
    while at + 8 <= len {
        f.seek(SeekFrom::Start(at))?;
        let mut h = [0u8; 8];
        f.read_exact(&mut h)?;
        let size = u64::from(u32::from_le_bytes([h[4], h[5], h[6], h[7]]));
        match &h[..4] {
            b"fmt " => {
                let mut fmt = [0u8; 8];
                f.read_exact(&mut fmt)?;
                rate = u32::from_le_bytes([fmt[4], fmt[5], fmt[6], fmt[7]]);
            }
            b"cue " => return Err(bad("This WAV file already has markers.")),
            _ => {}
        }
        at += 8 + size + (size & 1);
    }
    if rate == 0 {
        return Err(bad("Not a WAV file."));
    }
    let cues: Vec<Cue> = chapters
        .iter()
        .map(|c| Cue {
            position: c.start_ms * u64::from(rate) / 1000,
            length: 0,
            label: c.title.clone(),
        })
        .collect();
    let add = chunks(&cues);
    let padded = len + (len & 1);
    let riff = u32::try_from(padded + add.len() as u64 - 8)
        .map_err(|_| bad("The file is too big to hold markers."))?;
    f.seek(SeekFrom::Start(len))?;
    if len & 1 == 1 {
        f.write_all(&[0])?;
    }
    f.write_all(&add)?;
    f.seek(SeekFrom::Start(4))?;
    f.write_all(&riff.to_le_bytes())?;
    Ok(())
}

/// Embeds `chapters` in an exported file of any format that holds them
/// (WAV, FLAC, MP3, M4A). False for other formats; they are left as they are.
pub fn embed_chapters(path: &Path, chapters: &[Chapter]) -> std::io::Result<bool> {
    if chapters.is_empty() {
        return Ok(false);
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "mp3" => {
            let total_ms = crate::codecs::probe(path)
                .map(|i| (i.duration_seconds() * 1000.0) as u64)
                .unwrap_or_else(|_| chapters.last().map_or(0, |c| c.start_ms));
            embed_mp3(path, chapters, total_ms).map(|_| true)
        }
        "flac" => embed_flac(path, chapters).map(|_| true),
        "wav" | "wave" => embed_wav(path, chapters).map(|_| true),
        "m4a" => super::m4a_tags::write_m4a_chapters(path, chapters).map(|_| true),
        _ => Ok(false),
    }
}

/// Chapters in an audio file: MP3 (ID3 chapters), FLAC (`CHAPTERnnn`
/// comments) and M4A (`chpl`). Empty for other files or when there are none.
pub fn read_chapters(path: &Path) -> Vec<Chapter> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let mut list = match ext.as_str() {
        "mp3" => read_mp3(path).unwrap_or_default(),
        "flac" => read_flac(path).unwrap_or_default(),
        "m4a" | "mp4" => super::m4a_tags::read_m4a_chapters(path),
        _ => Vec::new(),
    };
    list.sort_by_key(|c| c.start_ms);
    list
}

/// A text frame's text: the encoding byte, then the text.
fn id3_text(body: &[u8]) -> String {
    let Some((&enc, rest)) = body.split_first() else {
        return String::new();
    };
    let utf16 = |bytes: &[u8], big: bool| {
        let units: Vec<u16> = bytes
            .iter()
            .step_by(2)
            .zip(bytes.iter().skip(1).step_by(2))
            .map(|(&a, &b)| {
                if big {
                    u16::from_be_bytes([a, b])
                } else {
                    u16::from_le_bytes([a, b])
                }
            })
            .collect();
        String::from_utf16_lossy(&units)
    };
    let text = match enc {
        1 | 2 => {
            let (big, bytes) = match rest {
                [0xff, 0xfe, r @ ..] => (false, r),
                [0xfe, 0xff, r @ ..] => (true, r),
                r => (enc == 2, r),
            };
            utf16(bytes, big)
        }
        3 => String::from_utf8_lossy(rest).into_owned(),
        _ => rest.iter().map(|&b| b as char).collect(),
    };
    text.trim_end_matches('\0').to_string()
}

fn read_mp3(path: &Path) -> std::io::Result<Vec<Chapter>> {
    let mut f = File::open(path)?;
    let mut head = [0u8; 10];
    f.read_exact(&mut head)?;
    if &head[..3] != b"ID3" {
        return Ok(Vec::new());
    }
    let size = head[6..10]
        .iter()
        .fold(0usize, |n, b| (n << 7) | usize::from(b & 0x7f));
    let mut tag = head.to_vec();
    tag.resize(10 + size.min(16 << 20), 0);
    f.read_exact(&mut tag[10..])?;
    let mut out = Vec::new();
    for (id, body) in id3_frames(&tag) {
        if &id != b"CHAP" {
            continue;
        }
        // Element id, a zero, start and end in ms, two byte offsets, then sub-frames.
        let Some(z) = body.iter().position(|&b| b == 0) else {
            continue;
        };
        let rest = &body[z + 1..];
        if rest.len() < 16 {
            continue;
        }
        let start = u64::from(u32::from_be_bytes([rest[0], rest[1], rest[2], rest[3]]));
        let mut at = 16;
        let mut name = String::new();
        while at + 10 <= rest.len() {
            let n = u32::from_be_bytes([rest[at + 4], rest[at + 5], rest[at + 6], rest[at + 7]])
                as usize;
            let Some(sub) = rest.get(at + 10..at + 10 + n) else {
                break;
            };
            if &rest[at..at + 4] == b"TIT2" {
                name = id3_text(sub);
                break;
            }
            at += 10 + n;
        }
        out.push(Chapter {
            title: name,
            start_ms: start,
            end_ms: None,
        });
    }
    Ok(out)
}

/// `HH:MM:SS.mmm` to milliseconds.
fn parse_clock(text: &str) -> Option<u64> {
    let mut parts = text.trim().split(':');
    let h: u64 = parts.next()?.parse().ok()?;
    let m: u64 = parts.next()?.parse().ok()?;
    let sec = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    let (s, ms) = sec.split_once('.').unwrap_or((sec, "0"));
    let s: u64 = s.parse().ok()?;
    let ms: u64 = format!("{ms:0<3}")[..3].parse().ok()?;
    Some(((h * 60 + m) * 60 + s) * 1000 + ms)
}

fn read_flac(path: &Path) -> std::io::Result<Vec<Chapter>> {
    let mut src = BufReader::new(File::open(path)?);
    let mut magic = [0u8; 4];
    src.read_exact(&mut magic)?;
    if &magic != b"fLaC" {
        return Ok(Vec::new());
    }
    let mut items = Vec::new();
    loop {
        let mut head = [0u8; 4];
        src.read_exact(&mut head)?;
        let len = (usize::from(head[1]) << 16) | (usize::from(head[2]) << 8) | usize::from(head[3]);
        if head[0] & 0x7f == 4 {
            let mut body = vec![0u8; len];
            src.read_exact(&mut body)?;
            items.extend(vorbis_items(&body));
        } else {
            src.seek(SeekFrom::Current(len as i64))?;
        }
        if head[0] & 0x80 != 0 {
            break;
        }
    }
    let mut times: Vec<(String, u64)> = Vec::new();
    for line in &items {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.to_ascii_uppercase();
        if let Some(n) = key.strip_prefix("CHAPTER") {
            if n.len() == 3 && n.bytes().all(|b| b.is_ascii_digit()) {
                if let Some(ms) = parse_clock(value) {
                    times.push((n.to_string(), ms));
                }
            }
        }
    }
    Ok(times
        .into_iter()
        .map(|(n, ms)| {
            let want = format!("CHAPTER{n}NAME");
            let title = items
                .iter()
                .filter_map(|l| l.split_once('='))
                .find(|(k, _)| k.to_ascii_uppercase() == want)
                .map(|(_, v)| v.to_string())
                .unwrap_or_default();
            Chapter {
                title,
                start_ms: ms,
                end_ms: None,
            }
        })
        .collect())
}

/// Writes beside the file, then swaps it in, so a failure never leaves half a file.
pub(crate) fn replace(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("chapters.tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ch(title: &str, start_ms: u64) -> Chapter {
        Chapter {
            title: title.into(),
            start_ms,
            end_ms: None,
        }
    }

    #[test]
    fn id3_tag_has_toc_and_chapters_with_times() {
        let tag = id3_tag(&[ch("Intro", 0), ch("Main topic", 95_000)], 600_000);
        assert_eq!(&tag[..5], b"ID3\x03\x00");
        assert_eq!(existing_id3(&tag), tag.len(), "size field matches");
        let find = |needle: &[u8]| tag.windows(needle.len()).position(|w| w == needle);
        assert!(find(b"CTOC").is_some());
        // The last "chp2" is the chapter frame itself (the first is in the table of contents).
        let second = tag.windows(5).rposition(|w| w == b"chp2\0").unwrap();
        // Start 95,000 ms, end at the file end, 600,000 ms.
        assert_eq!(&tag[second + 5..second + 9], &95_000u32.to_be_bytes());
        assert_eq!(&tag[second + 9..second + 13], &600_000u32.to_be_bytes());
    }

    #[test]
    fn mp3_tag_replaces_any_old_one() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.mp3");
        let mut old = id3_tag(&[ch("Old", 0)], 1_000);
        old.extend_from_slice(&[0xff, 0xfb, 1, 2, 3]);
        fs::write(&p, &old).unwrap();
        embed_mp3(&p, &[ch("New", 0)], 1_000).unwrap();
        let data = fs::read(&p).unwrap();
        let n = existing_id3(&data);
        assert_eq!(&data[n..], &[0xff, 0xfb, 1, 2, 3], "audio kept intact");
        assert!(data.windows(6).any(|w| w == "N\0e\0w\0".as_bytes()));
        assert!(!data.windows(6).any(|w| w == "O\0l\0d\0".as_bytes()));
    }

    #[test]
    fn flac_comments_go_after_streaminfo() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.flac");
        // fLaC, a last STREAMINFO block of 34 bytes, then "audio".
        let mut f = b"fLaC".to_vec();
        f.extend_from_slice(&[0x80, 0, 0, 34]);
        f.extend_from_slice(&[7u8; 34]);
        f.extend_from_slice(b"audio");
        fs::write(&p, &f).unwrap();
        embed_flac(&p, &[ch("Intro", 0), ch("Ad", 65_250)]).unwrap();
        let d = fs::read(&p).unwrap();
        assert_eq!(d[4], 0, "STREAMINFO is no longer last");
        assert_eq!(d[4 + 4 + 34], 0x84, "comments block is last");
        let text = String::from_utf8_lossy(&d);
        assert!(text.contains("CHAPTER002=00:01:05.250"));
        assert!(text.contains("CHAPTER002NAME=Ad"));
        assert!(d.ends_with(b"audio"));
        // Running it again replaces the comments rather than adding more.
        embed_flac(&p, &[ch("Only", 0)]).unwrap();
        let again = String::from_utf8_lossy(&fs::read(&p).unwrap()).to_string();
        assert!(again.contains("CHAPTER001NAME=Only") && !again.contains("CHAPTER002"));
    }

    fn three() -> Vec<Chapter> {
        vec![
            ch("Intro", 0),
            ch("Café talk", 61_500),
            ch("Wrap up", 3_723_004),
        ]
    }

    #[test]
    fn mp3_chapters_read_back() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.mp3");
        fs::write(&p, [0xffu8, 0xfb, 0x90, 0x00, 1, 2, 3, 4]).unwrap();
        embed_mp3(&p, &three(), 4_000_000).unwrap();
        let got = read_chapters(&p);
        assert_eq!(
            got.iter()
                .map(|c| (c.title.as_str(), c.start_ms))
                .collect::<Vec<_>>(),
            vec![("Intro", 0), ("Café talk", 61_500), ("Wrap up", 3_723_004)]
        );
    }

    #[test]
    fn flac_chapters_read_back() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.flac");
        // STREAMINFO (last block) then fake audio: enough for the comment code.
        let mut f = b"fLaC".to_vec();
        f.extend_from_slice(&[0x80, 0, 0, 34]);
        f.extend_from_slice(&[0u8; 34]);
        f.extend_from_slice(&[1, 2, 3, 4]);
        fs::write(&p, f).unwrap();
        embed_flac(&p, &three()).unwrap();
        let got = read_chapters(&p);
        assert_eq!(
            got.iter()
                .map(|c| (c.title.as_str(), c.start_ms))
                .collect::<Vec<_>>(),
            vec![("Intro", 0), ("Café talk", 61_500), ("Wrap up", 3_723_004)]
        );
    }

    #[test]
    fn wav_chapters_become_cue_points_and_the_audio_still_reads() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.wav");
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 8000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(&p, spec).unwrap();
        for i in 0..8000 * 70 {
            w.write_sample((i % 50) as i16 * 100).unwrap();
        }
        w.finalize().unwrap();
        let ch = vec![ch("Intro", 0), ch("Café talk", 61_500)];
        embed_wav(&p, &ch).unwrap();
        let cues = crate::codecs::cue::read_cues(&p);
        assert_eq!(cues.len(), 2);
        assert_eq!(
            (cues[1].position, cues[1].label.as_str()),
            (61_500 * 8, "Café talk")
        );
        let r = hound::WavReader::open(&p).unwrap();
        assert_eq!(r.len(), 8000 * 70);
        // Writing twice is refused, not doubled.
        assert!(embed_wav(&p, &ch).is_err());
        assert_eq!(crate::codecs::cue::read_cues(&p).len(), 2);
    }

    #[test]
    fn clocks_parse() {
        assert_eq!(parse_clock("01:02:03.004"), Some(3_723_004));
        assert_eq!(parse_clock("00:00:01.5"), Some(1500));
        assert_eq!(parse_clock("nonsense"), None);
    }
}
