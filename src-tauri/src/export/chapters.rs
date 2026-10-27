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
use std::io::{BufReader, BufWriter, Read, Write};
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
fn vorbis_items(body: &[u8]) -> Vec<String> {
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
}
