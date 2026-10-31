// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

//! Track Details: a file's tags and its cover picture, read and written
//! the way each format keeps them.
//!
//! - MP3: ID3v2.3 text frames and an `APIC` frame (front cover).
//! - FLAC: Vorbis comments and a `PICTURE` block (front cover).
//! - M4A: the item list (`moov/udta/meta/ilst`) with a `covr` item.
//! - WAV: a `LIST`/`INFO` chunk for the text, and an `id3 ` chunk holding
//!   an ID3v2.3 tag with the text and the picture, which most players
//!   that show WAV covers read.
//!
//! Every cover is stored as a JPEG no larger than [`COVER_MAX`] pixels a
//! side: large pictures are shrunk first, since some players (and some
//! FLAC readers) skip big embedded pictures, and it keeps files small.

use std::fs;
use std::path::Path;

use super::chapters::{
    empty_id3, existing_id3, frame, id3_frames, replace, set_flac_comments, set_flac_picture,
    text_frame, with_frames,
};
use super::tags::{comment_frame, Tags, FIELDS};

/// Longest side of a stored cover, in pixels.
pub const COVER_MAX: u32 = 600;

/// The picture data, shrunk to fit [`COVER_MAX`] and saved as JPEG.
/// Reads JPEG, PNG, WebP, BMP and GIF.
pub fn prepare_cover(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let img = image::load_from_memory(bytes).map_err(|_| {
        "That picture could not be read. Use a JPEG, PNG, WebP, BMP or GIF.".to_string()
    })?;
    let img = if img.width() > COVER_MAX || img.height() > COVER_MAX {
        img.resize(COVER_MAX, COVER_MAX, image::imageops::FilterType::Lanczos3)
    } else {
        img
    };
    let rgb = img.to_rgb8();
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 90)
        .encode_image(&rgb)
        .map_err(|e| e.to_string())?;
    Ok(out)
}

fn ext(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default()
}

/// True when Track Details can write this kind of file.
pub fn writable(path: &Path) -> bool {
    matches!(
        ext(path).as_str(),
        "mp3" | "flac" | "wav" | "wave" | "m4a" | "mp4" | "m4b"
    )
}

/// The picture in an ID3v2.3 `APIC` frame body.
fn apic_picture(body: &[u8]) -> Option<Vec<u8>> {
    let enc = *body.first()?;
    let mut at = 1 + body[1..].iter().position(|b| *b == 0)? + 1; // MIME, zero-ended
    at += 1; // picture type
             // Description, zero-ended (two zeros in UTF-16).
    if enc == 1 || enc == 2 {
        while at + 1 < body.len() && !(body[at] == 0 && body[at + 1] == 0) {
            at += 2;
        }
        at += 2;
    } else {
        at += body.get(at..)?.iter().position(|b| *b == 0)? + 1;
    }
    body.get(at..).filter(|d| !d.is_empty()).map(<[u8]>::to_vec)
}

/// An ID3v2.3 `APIC` frame holding a front cover.
fn apic_frame(pic: &[u8]) -> Vec<u8> {
    let mime: &[u8] = if pic.starts_with(b"\x89PNG") {
        b"image/png"
    } else {
        b"image/jpeg"
    };
    let mut body = vec![0u8];
    body.extend_from_slice(mime);
    body.push(0);
    body.push(3); // front cover
    body.push(0); // no description
    body.extend_from_slice(pic);
    frame(b"APIC", &body)
}

/// The WAV's `id3 ` chunk, if it has one: the tag inside it.
fn wav_id3(data: &[u8]) -> Option<&[u8]> {
    if data.len() < 12 || &data[..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        return None;
    }
    let mut at = 12;
    while at + 8 <= data.len() {
        let len =
            u32::from_le_bytes([data[at + 4], data[at + 5], data[at + 6], data[at + 7]]) as usize;
        let end = (at + 8 + len).min(data.len());
        if data[at..at + 4].eq_ignore_ascii_case(b"id3 ") {
            return Some(&data[at + 8..end]);
        }
        at = end + (len & 1);
    }
    None
}

/// The cover picture in a file: its front cover, or its first picture.
pub fn read_cover(path: &Path) -> Option<Vec<u8>> {
    if matches!(ext(path).as_str(), "wav" | "wave") {
        let data = fs::read(path).ok()?;
        let tag = wav_id3(&data)?;
        return id3_frames(tag)
            .into_iter()
            .find(|(id, _)| id == b"APIC")
            .and_then(|(_, body)| apic_picture(&body));
    }
    use symphonia::core::formats::probe::Hint;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::{MetadataOptions, StandardVisualKey};
    let file = fs::File::open(path).ok()?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    hint.with_extension(&ext(path));
    let mut format = symphonia::default::get_probe()
        .probe(
            &hint,
            mss,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .ok()?;
    let mut md = format.metadata();
    let mut first: Option<Vec<u8>> = None;
    loop {
        if let Some(rev) = md.current() {
            for v in &rev.media.visuals {
                if v.usage == Some(StandardVisualKey::FrontCover) {
                    return Some(v.data.to_vec());
                }
                first.get_or_insert_with(|| v.data.to_vec());
            }
        }
        if md.pop().is_none() {
            break;
        }
    }
    first
}

/// A FLAC `PICTURE` block body holding a front cover.
fn flac_picture(pic: &[u8]) -> Vec<u8> {
    let (mime, w, h) = match image::load_from_memory(pic) {
        Ok(i) => (
            if pic.starts_with(b"\x89PNG") {
                "image/png"
            } else {
                "image/jpeg"
            },
            i.width(),
            i.height(),
        ),
        Err(_) => ("image/jpeg", 0, 0),
    };
    let mut b = Vec::new();
    b.extend_from_slice(&3u32.to_be_bytes());
    b.extend_from_slice(&(mime.len() as u32).to_be_bytes());
    b.extend_from_slice(mime.as_bytes());
    b.extend_from_slice(&0u32.to_be_bytes()); // no description
    b.extend_from_slice(&w.to_be_bytes());
    b.extend_from_slice(&h.to_be_bytes());
    b.extend_from_slice(&24u32.to_be_bytes());
    b.extend_from_slice(&0u32.to_be_bytes());
    b.extend_from_slice(&(pic.len() as u32).to_be_bytes());
    b.extend_from_slice(pic);
    b
}

/// The ID3v2.3 frames for `tags` and `cover`.
fn id3_body(tags: &Tags, cover: Option<&[u8]>) -> Vec<u8> {
    let mut frames = Vec::new();
    for (i, v) in tags.set() {
        let id = FIELDS[i].0;
        frames.extend(if id == b"COMM" {
            comment_frame(v)
        } else {
            text_frame(id, v)
        });
    }
    if let Some(p) = cover {
        frames.extend(apic_frame(p));
    }
    frames
}

fn write_mp3(path: &Path, tags: &Tags, cover: Option<&[u8]>) -> std::io::Result<()> {
    let data = fs::read(path)?;
    let n = existing_id3(&data);
    let mut frames = id3_body(tags, cover);
    // Everything else in the old tag (chapters, other text) stays.
    for (id, body) in id3_frames(&data[..n]) {
        if id != *b"APIC" && !FIELDS.iter().any(|f| f.0 == &id) {
            frames.extend(frame(&id, &body));
        }
    }
    let mut out = with_frames(&empty_id3(), &frames);
    out.extend_from_slice(&data[n..]);
    replace(path, &out)
}

fn write_flac(path: &Path, tags: &Tags, cover: Option<&[u8]>) -> std::io::Result<()> {
    let lines: Vec<String> = tags
        .set()
        .map(|(i, v)| format!("{}={v}", FIELDS[i].1))
        .collect();
    set_flac_comments(path, |old| {
        let mut v: Vec<String> = old
            .into_iter()
            .filter(|c| {
                let key = c.split('=').next().unwrap_or("").to_ascii_uppercase();
                !FIELDS.iter().any(|f| f.1 == key) && key != "YEAR"
            })
            .collect();
        v.extend(lines);
        v
    })?;
    set_flac_picture(path, cover.map(flac_picture))
}

fn write_wav(path: &Path, tags: &Tags, cover: Option<&[u8]>) -> std::io::Result<()> {
    super::tags::write_wav(path, tags)?;
    let data = fs::read(path)?;
    let mut out = data[..12].to_vec();
    let mut at = 12;
    while at + 8 <= data.len() {
        let len =
            u32::from_le_bytes([data[at + 4], data[at + 5], data[at + 6], data[at + 7]]) as usize;
        let end = (at + 8 + len + (len & 1)).min(data.len());
        if !data[at..at + 4].eq_ignore_ascii_case(b"id3 ") {
            out.extend_from_slice(&data[at..end]);
            if (end - at) % 2 == 1 {
                out.push(0);
            }
        }
        at = end;
    }
    let body = id3_body(tags, cover);
    if !body.is_empty() {
        let tag = with_frames(&empty_id3(), &body);
        out.extend_from_slice(b"id3 ");
        out.extend_from_slice(&(tag.len() as u32).to_le_bytes());
        out.extend_from_slice(&tag);
        if tag.len() % 2 == 1 {
            out.push(0);
        }
    }
    let riff = (out.len() - 8) as u32;
    out[4..8].copy_from_slice(&riff.to_le_bytes());
    replace(path, &out)
}

/// Writes the file's tags and cover, replacing what was there: a field
/// left blank is removed, and no cover removes the picture.
pub fn write_details(path: &Path, tags: &Tags, cover: Option<&[u8]>) -> std::io::Result<()> {
    match ext(path).as_str() {
        "mp3" => write_mp3(path, tags, cover),
        "flac" => write_flac(path, tags, cover),
        "wav" | "wave" => write_wav(path, tags, cover),
        "m4a" | "mp4" | "m4b" => super::m4a_tags::write_m4a_with_cover(path, tags, cover),
        _ => Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Track Details writes WAV, FLAC, MP3 and M4A files.",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picture(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbImage::from_fn(w, h, |x, y| {
            image::Rgb([(x % 256) as u8, (y % 256) as u8, 128])
        });
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut out, image::ImageFormat::Png)
            .unwrap();
        out.into_inner()
    }

    #[test]
    fn large_covers_are_shrunk_to_a_jpeg() {
        let jpeg = prepare_cover(&picture(1500, 1000)).unwrap();
        assert!(jpeg.starts_with(&[0xff, 0xd8]));
        let back = image::load_from_memory(&jpeg).unwrap();
        assert_eq!((back.width(), back.height()), (600, 400));
        assert!(prepare_cover(b"not a picture").is_err());
    }

    fn wav(path: &Path) {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 8000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(path, spec).unwrap();
        for i in 0..800 {
            w.write_sample((i % 50) as i16 * 100).unwrap();
        }
        w.finalize().unwrap();
    }

    #[test]
    fn a_wav_keeps_its_tags_and_cover_and_still_plays() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.wav");
        wav(&p);
        let cover = prepare_cover(&picture(40, 40)).unwrap();
        let tags = Tags {
            title: Some("Song".into()),
            artist: Some("Band".into()),
            ..Tags::default()
        };
        write_details(&p, &tags, Some(&cover)).unwrap();
        assert_eq!(read_cover(&p).unwrap(), cover);
        assert_eq!(
            super::super::tags::read_tags(&p).title.as_deref(),
            Some("Song")
        );
        assert_eq!(hound::WavReader::open(&p).unwrap().duration(), 800);
        // Cleared: the title goes, the picture goes.
        write_details(&p, &Tags::default(), None).unwrap();
        assert!(read_cover(&p).is_none());
        assert!(super::super::tags::read_tags(&p).title.is_none());
        assert_eq!(hound::WavReader::open(&p).unwrap().duration(), 800);
    }

    #[test]
    fn an_apic_frame_reads_back() {
        let f = apic_frame(b"\xff\xd8abc");
        assert_eq!(apic_picture(&f[10..]).unwrap(), b"\xff\xd8abc");
    }
}
