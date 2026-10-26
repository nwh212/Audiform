// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! File tags (title, artist, album and so on): read from any file the
//! app opens, written into WAV, FLAC and MP3 files.
//!
//! - MP3: ID3v2.3 text frames at the front. Chapters already there stay.
//! - FLAC: Vorbis comments. Chapters already there stay.
//! - WAV: a `LIST` chunk of type `INFO`, before the audio, where readers
//!   that stream the file see it.
//!
//! - M4A: an item list in the movie header (see [`super::m4a_tags`]).

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::{MetadataOptions, MetadataRevision, StandardTag};

use super::chapters::{
    empty_id3, existing_id3, frame, id3_frames, replace, set_flac_comments, text_frame, with_frames,
};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tags {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artist: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub album: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub year: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub track: Option<String>,
}

/// Each field with its ID3 frame, Vorbis key and RIFF INFO id.
const FIELDS: [(&[u8; 4], &str, &[u8; 4]); 7] = [
    (b"TIT2", "TITLE", b"INAM"),
    (b"TPE1", "ARTIST", b"IART"),
    (b"TALB", "ALBUM", b"IPRD"),
    (b"TYER", "DATE", b"ICRD"),
    (b"TCON", "GENRE", b"IGNR"),
    (b"COMM", "COMMENT", b"ICMT"),
    (b"TRCK", "TRACKNUMBER", b"ITRK"),
];

impl Tags {
    fn values(&self) -> [&Option<String>; 7] {
        [
            &self.title,
            &self.artist,
            &self.album,
            &self.year,
            &self.genre,
            &self.comment,
            &self.track,
        ]
    }

    /// Fields that are set and not blank.
    fn set(&self) -> impl Iterator<Item = (usize, &str)> {
        self.values().into_iter().enumerate().filter_map(|(i, v)| {
            v.as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| (i, s))
        })
    }

    pub fn is_empty(&self) -> bool {
        self.set().next().is_none()
    }

    /// These tags with every field `over` sets written on top.
    pub fn overlay(&self, over: &Tags) -> Tags {
        let pick = |a: &Option<String>, b: &Option<String>| match b {
            Some(s) if !s.trim().is_empty() => Some(s.clone()),
            _ => a.clone(),
        };
        Tags {
            title: pick(&self.title, &over.title),
            artist: pick(&self.artist, &over.artist),
            album: pick(&self.album, &over.album),
            year: pick(&self.year, &over.year),
            genre: pick(&self.genre, &over.genre),
            comment: pick(&self.comment, &over.comment),
            track: pick(&self.track, &over.track),
        }
    }

    fn absorb(&mut self, rev: &MetadataRevision) {
        for tag in &rev.media.tags {
            let (slot, value) = match &tag.std {
                Some(StandardTag::TrackTitle(v)) => (&mut self.title, v.to_string()),
                Some(StandardTag::Artist(v)) => (&mut self.artist, v.to_string()),
                Some(StandardTag::Album(v)) => (&mut self.album, v.to_string()),
                Some(StandardTag::RecordingDate(v)) | Some(StandardTag::ReleaseDate(v)) => {
                    (&mut self.year, v.to_string())
                }
                Some(StandardTag::Genre(v)) => (&mut self.genre, v.to_string()),
                Some(StandardTag::Comment(v)) => (&mut self.comment, v.to_string()),
                Some(StandardTag::TrackNumber(n)) => (&mut self.track, n.to_string()),
                _ => continue,
            };
            if slot.is_none() && !value.trim().is_empty() {
                *slot = Some(value);
            }
        }
    }
}

/// Tags from a WAV's `LIST`/`INFO` chunk, wherever in the file it sits.
fn read_wav_info(data: &[u8]) -> Tags {
    let mut tags = Tags::default();
    if data.len() < 12 || &data[..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        return tags;
    }
    let le = |i: usize| {
        data.get(i..i + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
    };
    let mut at = 12;
    while let Some(len) = le(at + 4) {
        let end = (at + 8 + len).min(data.len());
        if &data[at..at + 4] == b"LIST" && data.get(at + 8..at + 12) == Some(b"INFO") {
            let mut i = at + 12;
            while i + 8 <= end {
                let id = &data[i..i + 4];
                let n = le(i + 4).unwrap_or(0);
                let raw = data.get(i + 8..(i + 8 + n).min(end)).unwrap_or(&[]);
                let text = String::from_utf8_lossy(raw)
                    .trim_end_matches('\0')
                    .trim()
                    .to_string();
                if let Some(k) = FIELDS.iter().position(|f| f.2 == id) {
                    if !text.is_empty() {
                        let slot = match k {
                            0 => &mut tags.title,
                            1 => &mut tags.artist,
                            2 => &mut tags.album,
                            3 => &mut tags.year,
                            4 => &mut tags.genre,
                            5 => &mut tags.comment,
                            _ => &mut tags.track,
                        };
                        slot.get_or_insert(text);
                    }
                }
                i += 8 + n + (n & 1);
            }
        }
        at = end + (len & 1);
        if at + 8 > data.len() {
            break;
        }
    }
    tags
}

/// The tags in an audio file. Unreadable files give no tags.
pub fn read_tags(path: &Path) -> Tags {
    let is_wav = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("wav") || e.eq_ignore_ascii_case("wave"));
    let mut tags = if is_wav {
        fs::read(path)
            .map(|d| read_wav_info(&d))
            .unwrap_or_default()
    } else {
        Tags::default()
    };
    let Ok(file) = fs::File::open(path) else {
        return tags;
    };
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let Ok(mut format) = symphonia::default::get_probe().probe(
        &hint,
        mss,
        FormatOptions::default(),
        MetadataOptions::default(),
    ) else {
        return tags;
    };
    let mut md = format.metadata();
    loop {
        if let Some(rev) = md.current() {
            tags.absorb(rev);
        }
        if md.pop().is_none() {
            break;
        }
    }
    tags
}

/// Writes `tags` into the file, by its extension. Formats without tag
/// support here are left as they are; the result says whether tags went in.
pub fn write_tags(path: &Path, tags: &Tags) -> std::io::Result<bool> {
    if tags.is_empty() {
        return Ok(false);
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    match ext.as_str() {
        "mp3" => write_mp3(path, tags).map(|_| true),
        "flac" => write_flac(path, tags).map(|_| true),
        "wav" | "wave" => write_wav(path, tags).map(|_| true),
        "m4a" | "mp4" | "m4b" => super::m4a_tags::write_m4a(path, tags).map(|_| true),
        _ => Ok(false),
    }
}

/// An ID3v2.3 comment frame: encoding, language, empty description, text.
fn comment_frame(text: &str) -> Vec<u8> {
    let mut body = vec![1u8];
    body.extend_from_slice(b"eng");
    body.extend_from_slice(&[0xff, 0xfe, 0, 0]);
    body.extend_from_slice(&[0xff, 0xfe]);
    for u in text.encode_utf16() {
        body.extend_from_slice(&u.to_le_bytes());
    }
    body.extend_from_slice(&[0, 0]);
    frame(b"COMM", &body)
}

fn write_mp3(path: &Path, tags: &Tags) -> std::io::Result<()> {
    let data = fs::read(path)?;
    let n = existing_id3(&data);
    let ours: Vec<&[u8; 4]> = tags.set().map(|(i, _)| FIELDS[i].0).collect();
    let mut frames: Vec<u8> = Vec::new();
    for (i, v) in tags.set() {
        let id = FIELDS[i].0;
        frames.extend(if id == b"COMM" {
            comment_frame(v)
        } else {
            text_frame(id, v)
        });
    }
    for (id, body) in id3_frames(&data[..n]) {
        if !ours.contains(&&id) {
            frames.extend(frame(&id, &body));
        }
    }
    let mut out = with_frames(&empty_id3(), &frames);
    out.extend_from_slice(&data[n..]);
    replace(path, &out)
}

fn write_flac(path: &Path, tags: &Tags) -> std::io::Result<()> {
    let keys: Vec<&str> = tags.set().map(|(i, _)| FIELDS[i].1).collect();
    let lines: Vec<String> = tags
        .set()
        .map(|(i, v)| format!("{}={v}", FIELDS[i].1))
        .collect();
    set_flac_comments(path, |old| {
        let mut v: Vec<String> = old
            .into_iter()
            .filter(|c| {
                let key = c.split('=').next().unwrap_or("").to_ascii_uppercase();
                !keys.contains(&key.as_str())
            })
            .collect();
        v.extend(lines);
        v
    })
}

/// The `LIST`/`INFO` chunk for the tags. Text is UTF-8, which current
/// players read; each entry ends with a zero and pads to an even size.
pub fn info_chunk(tags: &Tags) -> Vec<u8> {
    let mut body = b"INFO".to_vec();
    for (i, v) in tags.set() {
        let mut text = v.as_bytes().to_vec();
        text.push(0);
        body.extend_from_slice(FIELDS[i].2);
        body.extend_from_slice(&(text.len() as u32).to_le_bytes());
        body.extend_from_slice(&text);
        if text.len() % 2 == 1 {
            body.push(0);
        }
    }
    let mut chunk = b"LIST".to_vec();
    chunk.extend_from_slice(&(body.len() as u32).to_le_bytes());
    chunk.extend(body);
    chunk
}

/// Replaces any `LIST`/`INFO` chunk in the WAV with the tags, just before
/// the `data` chunk.
fn write_wav(path: &Path, tags: &Tags) -> std::io::Result<()> {
    let bad = || std::io::Error::new(std::io::ErrorKind::InvalidData, "Not a WAV file.");
    let data = fs::read(path)?;
    if data.len() < 12 || &data[..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        return Err(bad());
    }
    let mut out = data[..12].to_vec();
    let mut at = 12;
    let mut placed = false;
    while at + 8 <= data.len() {
        let id = &data[at..at + 4];
        let len =
            u32::from_le_bytes([data[at + 4], data[at + 5], data[at + 6], data[at + 7]]) as usize;
        let end = (at + 8 + len + (len & 1)).min(data.len());
        let info = id == b"LIST" && data.get(at + 8..at + 12) == Some(b"INFO");
        if id == b"data" && !placed {
            out.extend(info_chunk(tags));
            placed = true;
        }
        if !info {
            out.extend_from_slice(&data[at..end]);
            // A last chunk that ends without its pad byte gets one.
            if (end - at) % 2 == 1 {
                out.push(0);
            }
        }
        at = end;
    }
    if !placed {
        out.extend(info_chunk(tags));
    }
    let riff = (out.len() - 8) as u32;
    out[4..8].copy_from_slice(&riff.to_le_bytes());
    replace(path, &out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Tags {
        Tags {
            title: Some("Episode 12".into()),
            artist: Some("The Show".into()),
            comment: Some("Notes".into()),
            ..Tags::default()
        }
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
    fn overlay_keeps_old_values_under_blank_ones() {
        let base = sample();
        let over = Tags {
            title: Some("New".into()),
            artist: Some("  ".into()),
            album: Some("Season 2".into()),
            ..Tags::default()
        };
        let t = base.overlay(&over);
        assert_eq!(t.title.as_deref(), Some("New"));
        assert_eq!(t.artist.as_deref(), Some("The Show"));
        assert_eq!(t.album.as_deref(), Some("Season 2"));
        assert!(Tags::default().is_empty());
    }

    #[test]
    fn wav_tags_round_trip_and_audio_still_reads() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.wav");
        wav(&p);
        assert!(write_tags(&p, &sample()).unwrap());
        // Writing again replaces, never stacks.
        assert!(write_tags(&p, &sample()).unwrap());
        let data = fs::read(&p).unwrap();
        assert_eq!(data.windows(4).filter(|w| w == b"INFO").count(), 1);
        let info = data.windows(4).position(|w| w == b"INFO").unwrap();
        let audio = data.windows(4).position(|w| w == b"data").unwrap();
        assert!(info < audio, "tags sit before the audio");
        let r = hound::WavReader::open(&p).unwrap();
        assert_eq!(r.len(), 800);
        let t = read_tags(&p);
        assert_eq!(t.title.as_deref(), Some("Episode 12"));
        assert_eq!(t.artist.as_deref(), Some("The Show"));
    }

    #[test]
    fn mp3_tags_keep_chapters_and_chapters_keep_tags() {
        use super::super::chapters::{embed_mp3, Chapter};
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.mp3");
        // Not real audio: the tag code only touches the front of the file.
        fs::write(&p, [0xffu8, 0xfb, 0x90, 0x00, 1, 2, 3, 4]).unwrap();
        let ch = [Chapter {
            title: "Intro".into(),
            start_ms: 0,
            end_ms: None,
        }];
        embed_mp3(&p, &ch, 1000).unwrap();
        write_tags(&p, &sample()).unwrap();
        let data = fs::read(&p).unwrap();
        let n = existing_id3(&data);
        let ids: Vec<[u8; 4]> = id3_frames(&data[..n]).into_iter().map(|(i, _)| i).collect();
        assert!(ids.contains(b"CHAP") && ids.contains(b"CTOC") && ids.contains(b"TIT2"));
        assert_eq!(&data[n..], &[0xff, 0xfb, 0x90, 0x00, 1, 2, 3, 4]);
        embed_mp3(&p, &ch, 1000).unwrap();
        let data = fs::read(&p).unwrap();
        let ids: Vec<[u8; 4]> = id3_frames(&data[..existing_id3(&data)])
            .into_iter()
            .map(|(i, _)| i)
            .collect();
        assert!(ids.contains(b"TPE1") && ids.contains(b"COMM"));
        assert_eq!(ids.iter().filter(|i| *i == b"CHAP").count(), 1);
    }

    #[test]
    fn flac_comments_merge() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.flac");
        // fLaC + a last STREAMINFO block of 34 zero bytes + "audio".
        let mut f = b"fLaC".to_vec();
        f.extend_from_slice(&[0x80, 0, 0, 34]);
        f.extend_from_slice(&[0; 34]);
        f.extend_from_slice(b"audio");
        fs::write(&p, &f).unwrap();
        use super::super::chapters::{embed_flac, Chapter};
        embed_flac(
            &p,
            &[Chapter {
                title: "Intro".into(),
                start_ms: 0,
                end_ms: None,
            }],
        )
        .unwrap();
        write_tags(&p, &sample()).unwrap();
        let data = fs::read(&p).unwrap();
        let text = String::from_utf8_lossy(&data);
        assert!(text.contains("CHAPTER001NAME=Intro") && text.contains("TITLE=Episode 12"));
        assert!(data.ends_with(b"audio"));
    }
}
