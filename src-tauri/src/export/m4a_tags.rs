// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Tags in M4A (MP4) files, in the item list most players read
//! (`moov/udta/meta/ilst`). Our own writer, from the published MP4 box
//! layout.
//!
//! An MP4 file is a tree of boxes: a 32-bit size, a four-letter type,
//! then the contents. The tags go inside the movie header (`moov`). When
//! that header sits before the audio (`mdat`), making it bigger moves
//! the audio, so every chunk offset (`stco` and `co64`) is moved by the
//! same amount. Other boxes are copied as they are.

use std::fs;
use std::path::Path;

use super::chapters::{replace, Chapter};
use super::tags::Tags;

fn bad() -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, "Not an MP4 file.")
}

/// Where one box sits: start, header length, total length, type.
type BoxAt = (usize, usize, usize, [u8; 4]);

/// Each box in `data`, in order.
fn boxes(data: &[u8]) -> std::io::Result<Vec<BoxAt>> {
    let mut out = Vec::new();
    let mut at = 0;
    while at + 8 <= data.len() {
        let size = u32::from_be_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]]) as u64;
        let kind = [data[at + 4], data[at + 5], data[at + 6], data[at + 7]];
        let (head, len) = match size {
            0 => (8, (data.len() - at) as u64),
            1 => {
                let b = data.get(at + 8..at + 16).ok_or_else(bad)?;
                (
                    16,
                    u64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]),
                )
            }
            n => (8, n),
        };
        let len = len as usize;
        if len < head || at + len > data.len() {
            return Err(bad());
        }
        out.push((at, head, len, kind));
        at += len;
    }
    Ok(out)
}

fn make_box(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut b = Vec::with_capacity(8 + body.len());
    b.extend_from_slice(&((8 + body.len()) as u32).to_be_bytes());
    b.extend_from_slice(kind);
    b.extend_from_slice(body);
    b
}

/// One item: a box named for the field holding a `data` box.
fn text_item(kind: &[u8; 4], text: &str) -> Vec<u8> {
    let mut data = vec![0, 0, 0, 1, 0, 0, 0, 0]; // UTF-8, no locale
    data.extend_from_slice(text.as_bytes());
    make_box(kind, &make_box(b"data", &data))
}

fn track_item(text: &str) -> Option<Vec<u8>> {
    let mut parts = text.split('/');
    let n: u16 = parts.next()?.trim().parse().ok()?;
    let total: u16 = parts
        .next()
        .and_then(|t| t.trim().parse().ok())
        .unwrap_or(0);
    let mut data = vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    data.extend_from_slice(&n.to_be_bytes());
    data.extend_from_slice(&total.to_be_bytes());
    data.extend_from_slice(&[0, 0]);
    Some(make_box(b"trkn", &make_box(b"data", &data)))
}

/// The `meta` box holding the tags.
pub fn meta_box(tags: &Tags) -> Vec<u8> {
    meta_box_with(tags, None)
}

/// The `meta` box holding the tags and, when given, the cover picture
/// (JPEG or PNG, told apart by its first bytes).
pub fn meta_box_with(tags: &Tags, cover: Option<&[u8]>) -> Vec<u8> {
    fn field(v: &Option<String>) -> Option<&str> {
        v.as_deref().map(str::trim).filter(|s| !s.is_empty())
    }
    let mut items = Vec::new();
    for (kind, v) in [
        (b"\xa9nam", &tags.title),
        (b"\xa9ART", &tags.artist),
        (b"\xa9alb", &tags.album),
        (b"\xa9day", &tags.year),
        (b"\xa9gen", &tags.genre),
        (b"\xa9cmt", &tags.comment),
    ] {
        if let Some(t) = field(v) {
            items.extend(text_item(kind, t));
        }
    }
    if let Some(item) = field(&tags.track).and_then(track_item) {
        items.extend(item);
    }
    if let Some(pic) = cover {
        // Data type 13 is JPEG, 14 PNG.
        let kind = if pic.starts_with(b"\x89PNG") {
            14u8
        } else {
            13
        };
        let mut data = vec![0, 0, 0, kind, 0, 0, 0, 0];
        data.extend_from_slice(pic);
        items.extend(make_box(b"covr", &make_box(b"data", &data)));
    }
    // meta is a full box: version and flags first.
    let mut body = vec![0, 0, 0, 0];
    let mut hdlr = vec![0u8; 8];
    hdlr.extend_from_slice(b"mdir");
    hdlr.extend_from_slice(b"appl");
    hdlr.extend_from_slice(&[0; 9]);
    body.extend(make_box(b"hdlr", &hdlr));
    body.extend(make_box(b"ilst", &items));
    make_box(b"meta", &body)
}

/// Boxes inside which chunk offset tables can sit.
const CONTAINERS: [&[u8; 4]; 6] = [b"moov", b"trak", b"mdia", b"minf", b"stbl", b"edts"];

/// Moves every chunk offset at or after `from` by `delta`, in place.
fn shift_offsets(moov: &mut [u8], from: u64, delta: i64) -> std::io::Result<()> {
    for (at, head, len, kind) in boxes(moov)? {
        let body = at + head;
        match &kind {
            k if CONTAINERS.contains(&k) => shift_offsets(&mut moov[body..at + len], from, delta)?,
            b"stco" | b"co64" => {
                let wide = &kind == b"co64";
                let count = u32::from_be_bytes(
                    moov.get(body + 4..body + 8)
                        .ok_or_else(bad)?
                        .try_into()
                        .map_err(|_| bad())?,
                ) as usize;
                let size = if wide { 8 } else { 4 };
                for i in 0..count {
                    let p = body + 8 + i * size;
                    let slot = moov.get_mut(p..p + size).ok_or_else(bad)?;
                    let old = if wide {
                        u64::from_be_bytes(slot.try_into().map_err(|_| bad())?)
                    } else {
                        u64::from(u32::from_be_bytes(slot.try_into().map_err(|_| bad())?))
                    };
                    if old >= from {
                        let new = (old as i64 + delta) as u64;
                        if wide {
                            slot.copy_from_slice(&new.to_be_bytes());
                        } else {
                            let v = u32::try_from(new).map_err(|_| bad())?;
                            slot.copy_from_slice(&v.to_be_bytes());
                        }
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// Writes `tags` and the cover picture into an M4A file, replacing any
/// tags and picture already there (no cover: none).
pub fn write_m4a_with_cover(path: &Path, tags: &Tags, cover: Option<&[u8]>) -> std::io::Result<()> {
    rewrite_udta(path, b"meta", &meta_box_with(tags, cover))
}

/// Writes `tags` into an M4A file, replacing any tags already there.
pub fn write_m4a(path: &Path, tags: &Tags) -> std::io::Result<()> {
    rewrite_udta(path, b"meta", &meta_box(tags))
}

/// Rebuilds the movie header's `udta`: every box in it is kept except the
/// ones of type `drop`, and `add` goes at the end. Audio offsets follow.
fn rewrite_udta(path: &Path, drop: &[u8; 4], add: &[u8]) -> std::io::Result<()> {
    let data = fs::read(path)?;
    let top = boxes(&data)?;
    let &(m_at, m_head, m_len, _) = top.iter().find(|b| &b.3 == b"moov").ok_or_else(bad)?;
    // The new movie header: its children, with udta rebuilt to hold the new box.
    let moov_body = &data[m_at + m_head..m_at + m_len];
    let mut children = Vec::new();
    let mut udta_rest: Vec<u8> = Vec::new();
    for (at, head, len, kind) in boxes(moov_body)? {
        if &kind == b"udta" {
            for (u_at, _, u_len, u_kind) in boxes(&moov_body[at + head..at + len])? {
                if &u_kind != drop {
                    udta_rest
                        .extend_from_slice(&moov_body[at + head + u_at..at + head + u_at + u_len]);
                }
            }
        } else {
            children.extend_from_slice(&moov_body[at..at + len]);
        }
    }
    udta_rest.extend_from_slice(add);
    if !udta_rest.is_empty() {
        children.extend(make_box(b"udta", &udta_rest));
    }
    let mut moov = make_box(b"moov", &children);
    let delta = moov.len() as i64 - m_len as i64;
    // Audio after the header moves; offsets into it move with it.
    if delta != 0 {
        shift_offsets(&mut moov[8..], (m_at + m_len) as u64, delta)?;
    }
    let mut out = Vec::with_capacity(data.len() + moov.len());
    out.extend_from_slice(&data[..m_at]);
    out.extend(moov);
    out.extend_from_slice(&data[m_at + m_len..]);
    replace(path, &out)
}

/// The Nero chapter list (`chpl`): version 1, a reserved word, a count,
/// then each chapter's start in 100 ns units and its title (up to 255 bytes).
fn chpl_box(chapters: &[Chapter]) -> Vec<u8> {
    let list = &chapters[..chapters.len().min(255)];
    let mut body = vec![1, 0, 0, 0, 0, 0, 0, 0, list.len() as u8];
    for c in list {
        body.extend_from_slice(&(c.start_ms * 10_000).to_be_bytes());
        let mut title = c.title.as_bytes();
        if title.len() > 255 {
            // Cut on a character boundary so the title stays valid text.
            let mut n = 255;
            while !c.title.is_char_boundary(n) {
                n -= 1;
            }
            title = &title[..n];
        }
        body.push(title.len() as u8);
        body.extend_from_slice(title);
    }
    make_box(b"chpl", &body)
}

/// Writes chapters into an M4A file (`moov/udta/chpl`, which podcast apps
/// read), replacing old ones and keeping the tags. An empty list removes them.
pub fn write_m4a_chapters(path: &Path, chapters: &[Chapter]) -> std::io::Result<()> {
    let add = if chapters.is_empty() {
        Vec::new()
    } else {
        chpl_box(chapters)
    };
    rewrite_udta(path, b"chpl", &add)
}

/// The chapters in an M4A file's `chpl` box, if it has one.
pub fn read_m4a_chapters(path: &Path) -> Vec<Chapter> {
    read_chpl(path).unwrap_or_default()
}

fn read_chpl(path: &Path) -> std::io::Result<Vec<Chapter>> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path)?;
    let len = f.metadata()?.len();
    // Walk the top-level boxes by their headers; only the movie header is read.
    let mut at = 0u64;
    let mut moov = None;
    while at + 8 <= len {
        f.seek(SeekFrom::Start(at))?;
        let mut h = [0u8; 8];
        f.read_exact(&mut h)?;
        let mut size = u64::from(u32::from_be_bytes([h[0], h[1], h[2], h[3]]));
        let mut head = 8;
        if size == 1 {
            let mut w = [0u8; 8];
            f.read_exact(&mut w)?;
            size = u64::from_be_bytes(w);
            head = 16;
        } else if size == 0 {
            size = len - at;
        }
        if size < head || at + size > len {
            return Err(bad());
        }
        if &h[4..8] == b"moov" {
            if size > 64 << 20 {
                return Ok(Vec::new());
            }
            let mut body = vec![0u8; (size - head) as usize];
            f.read_exact(&mut body)?;
            moov = Some(body);
            break;
        }
        at += size;
    }
    let Some(moov) = moov else {
        return Ok(Vec::new());
    };
    for (at, head, len, kind) in boxes(&moov)? {
        if &kind != b"udta" {
            continue;
        }
        let udta = &moov[at + head..at + len];
        for (u_at, u_head, u_len, u_kind) in boxes(udta)? {
            if &u_kind == b"chpl" {
                return Ok(parse_chpl(&udta[u_at + u_head..u_at + u_len]));
            }
        }
    }
    Ok(Vec::new())
}

fn parse_chpl(body: &[u8]) -> Vec<Chapter> {
    let mut out = Vec::new();
    // Version 1 has a reserved word before the count; version 0 does not.
    let mut at = if body.first() == Some(&1) { 8 } else { 4 };
    let Some(&count) = body.get(at) else {
        return out;
    };
    at += 1;
    for _ in 0..count {
        let Some(start) = body.get(at..at + 8) else {
            break;
        };
        let start = u64::from_be_bytes(start.try_into().unwrap_or([0; 8]));
        let Some(&n) = body.get(at + 8) else {
            break;
        };
        let Some(title) = body.get(at + 9..at + 9 + usize::from(n)) else {
            break;
        };
        out.push(Chapter {
            title: String::from_utf8_lossy(title).into_owned(),
            start_ms: start / 10_000,
            end_ms: None,
            url: None,
        });
        at += 9 + usize::from(n);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny file: ftyp, a movie header with one chunk offset, then the
    /// audio box with eight bytes of "audio".
    fn sample(moov_first: bool) -> Vec<u8> {
        let ftyp = make_box(b"ftyp", b"M4A \0\0\0\0M4A mp42");
        let mdat = make_box(b"mdat", b"AUDIODAT");
        let stco = |off: u32| {
            let mut b = vec![0, 0, 0, 0, 0, 0, 0, 1];
            b.extend_from_slice(&off.to_be_bytes());
            make_box(b"stco", &b)
        };
        let moov = |off: u32| {
            let stbl = make_box(b"stbl", &stco(off));
            let minf = make_box(b"minf", &stbl);
            let mdia = make_box(b"mdia", &minf);
            make_box(b"moov", &make_box(b"trak", &mdia))
        };
        if moov_first {
            let len = moov(0).len();
            let off = (ftyp.len() + len + 8) as u32;
            [ftyp, moov(off), mdat].concat()
        } else {
            let off = (ftyp.len() + 8) as u32;
            [ftyp.clone(), mdat, moov(off)].concat()
        }
    }

    /// The chunk offset in the file, and the bytes it points at.
    fn chunk(data: &[u8]) -> (u32, Vec<u8>) {
        let i = data.windows(4).position(|w| w == b"stco").unwrap();
        let off = u32::from_be_bytes(data[i + 12..i + 16].try_into().unwrap());
        (off, data[off as usize..off as usize + 8].to_vec())
    }

    fn tags() -> Tags {
        Tags {
            title: Some("Episode 12".into()),
            artist: Some("The Show".into()),
            track: Some("12/40".into()),
            ..Tags::default()
        }
    }

    #[test]
    fn tags_go_in_and_audio_offsets_follow() {
        for moov_first in [true, false] {
            let dir = tempfile::tempdir().unwrap();
            let p = dir.path().join("a.m4a");
            fs::write(&p, sample(moov_first)).unwrap();
            write_m4a(&p, &tags()).unwrap();
            // Writing twice replaces, never stacks.
            write_m4a(&p, &tags()).unwrap();
            let data = fs::read(&p).unwrap();
            assert_eq!(
                chunk(&data).1,
                b"AUDIODAT",
                "offset still points at the audio (moov first: {moov_first})"
            );
            assert_eq!(data.windows(4).filter(|w| w == b"ilst").count(), 1);
            let text = String::from_utf8_lossy(&data);
            assert!(text.contains("Episode 12") && text.contains("The Show"));
            // The whole file still parses as boxes end to end.
            assert_eq!(
                boxes(&data).unwrap().iter().map(|b| b.2).sum::<usize>(),
                data.len()
            );
        }
    }

    #[test]
    fn other_udta_contents_are_kept() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.m4a");
        let mut f = sample(false);
        // Add a udta with a chapter list box to the end-of-file movie header.
        let top = boxes(&f).unwrap();
        let (at, _, len, _) = *top.iter().find(|b| &b.3 == b"moov").unwrap();
        let mut body = f[at + 8..at + len].to_vec();
        body.extend(make_box(b"udta", &make_box(b"chpl", b"chapters")));
        f.truncate(at);
        f.extend(make_box(b"moov", &body));
        fs::write(&p, &f).unwrap();
        write_m4a(&p, &tags()).unwrap();
        let data = fs::read(&p).unwrap();
        assert!(
            data.windows(8).any(|w| w == b"chapters"),
            "chapter box kept"
        );
        assert!(data.windows(4).any(|w| w == b"ilst"));
    }

    #[test]
    fn chapters_go_in_read_back_and_keep_the_tags() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.m4a");
        fs::write(&p, sample(true)).unwrap();
        write_m4a(&p, &tags()).unwrap();
        let ch = vec![
            Chapter {
                title: "Intro".into(),
                start_ms: 0,
                end_ms: None,
                url: None,
            },
            Chapter {
                title: "Café talk".into(),
                start_ms: 61_500,
                end_ms: None,
                url: None,
            },
        ];
        write_m4a_chapters(&p, &ch).unwrap();
        assert_eq!(read_m4a_chapters(&p), ch);
        let data = fs::read(&p).unwrap();
        assert!(data.windows(4).any(|w| w == b"ilst"), "tags kept");
        assert_eq!(chunk(&data).1, b"AUDIODAT", "audio offsets follow");
        // Tags written afterwards keep the chapters; new chapters replace old.
        write_m4a(&p, &tags()).unwrap();
        assert_eq!(read_m4a_chapters(&p).len(), 2);
        write_m4a_chapters(&p, &ch[..1]).unwrap();
        assert_eq!(read_m4a_chapters(&p), ch[..1]);
        write_m4a_chapters(&p, &[]).unwrap();
        assert!(read_m4a_chapters(&p).is_empty());
        assert_eq!(chunk(&fs::read(&p).unwrap()).1, b"AUDIODAT");
        // The audio-last layout works too.
        let q = dir.path().join("b.m4a");
        fs::write(&q, sample(false)).unwrap();
        write_m4a_chapters(&q, &ch).unwrap();
        assert_eq!(read_m4a_chapters(&q), ch);
        assert_eq!(chunk(&fs::read(&q).unwrap()).1, b"AUDIODAT");
    }
}
