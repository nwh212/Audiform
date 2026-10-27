// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Identifies an audio file and measures it: format, codec, sample
//! rate, channels and exact length. The first packet is decoded so a
//! file we cannot play is caught at import, not later on the timeline.

use std::path::{Path, PathBuf};

use serde::Serialize;
use symphonia::core::codecs::audio::{AudioCodecParameters, AudioDecoderOptions};
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, TrackType};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::units::{Duration, TimeBase};
use thiserror::Error;

use super::scan::is_supported;

/// What the Files panel shows about one audio file.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioInfo {
    pub path: PathBuf,
    /// File name with extension.
    pub name: String,
    /// Container, for example "WAV" or "M4A".
    pub format: String,
    /// Codec, for example "PCM 24-bit" or "MP3".
    pub codec: String,
    pub sample_rate: u32,
    pub channels: u16,
    /// Length in sample frames at the file's own rate.
    pub length: u64,
    /// Bits per sample for lossless files. None for lossy codecs.
    pub bit_depth: Option<u32>,
    /// Size on disk in bytes.
    pub file_size: u64,
}

impl AudioInfo {
    /// Length in seconds.
    pub fn duration_seconds(&self) -> f64 {
        if self.sample_rate == 0 {
            0.0
        } else {
            self.length as f64 / f64::from(self.sample_rate)
        }
    }
}

/// Why a file could not be imported. Messages are shown to the user.
#[derive(Debug, Error)]
pub enum ProbeError {
    #[error("File not found")]
    NotFound,
    #[error("Not an audio file this app can open")]
    UnsupportedType,
    #[error("Could not recognize the audio format")]
    UnknownFormat,
    #[error("No audio in this file")]
    NoAudio,
    #[error("Codec not supported: {0}")]
    UnsupportedCodec(String),
    #[error("File is damaged or incomplete: {0}")]
    Damaged(String),
    #[error("Could not read file: {0}")]
    Io(#[from] std::io::Error),
}

/// Reads enough of the file to describe it. Runs in time roughly
/// proportional to file size only when the container does not store
/// its length (some MP3 files); otherwise it reads just the header and
/// one packet.
pub fn probe(path: &Path) -> Result<AudioInfo, ProbeError> {
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(ProbeError::NotFound),
        Err(e) => return Err(e.into()),
    };
    if !meta.is_file() || !is_supported(path) {
        return Err(ProbeError::UnsupportedType);
    }

    // A WAV holding a DTS stream (or a raw .dts file) is described by
    // what the stream holds, not by its 16-bit stereo wrapper.
    if let Some(dts) = super::dts::sniff(path) {
        return Ok(AudioInfo {
            path: path.to_path_buf(),
            name: path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            format: if dts.in_wav {
                "WAV".into()
            } else {
                "DTS".into()
            },
            codec: "DTS".into(),
            sample_rate: dts.sample_rate,
            channels: dts.channels() as u16,
            length: dts.length(),
            bit_depth: None,
            file_size: meta.len(),
        });
    }

    match probe_symphonia(path, &meta).map_err(|e| name_hidden_codec(path, e)) {
        Err(e @ (ProbeError::UnsupportedCodec(_) | ProbeError::UnknownFormat)) => {
            probe_os(path, &meta).unwrap_or(Err(explain(e)))
        }
        other => other,
    }
}

/// Codecs Symphonia names only by number, and what to call them.
const CODEC_NAMES: &[(&str, &str)] = &[
    ("0x1008", "Dolby Digital (AC-3)"),
    ("0x1009", "Dolby Digital Plus (E-AC-3)"),
];

/// MP4 audio entries Symphonia skips without a word, so a file of them
/// reads as having no audio at all.
const MP4_CODECS: &[(&[u8; 4], &str)] = &[
    (b"ac-4", AC4),
    (b"mha1", "MPEG-H 3D Audio"),
    (b"mha2", "MPEG-H 3D Audio"),
    (b"mhm1", "MPEG-H 3D Audio"),
    (b"mhm2", "MPEG-H 3D Audio"),
];

const AC4: &str = "Dolby AC-4";

/// When Symphonia finds no audio, names the codec of an MP4 or M4A audio
/// track it skipped (Dolby AC-4, as Dolby Atmos music is now sold), so
/// the system decoders get a try and the error says what the file holds.
pub(super) fn name_hidden_codec(path: &Path, e: ProbeError) -> ProbeError {
    if !matches!(e, ProbeError::NoAudio) {
        return e;
    }
    mp4_sample_entries(path)
        .into_iter()
        .find_map(|t| MP4_CODECS.iter().find(|(c, _)| **c == t))
        .map_or(e, |(_, name)| ProbeError::UnsupportedCodec((*name).into()))
}

/// The sample entry types of an MP4 file's tracks (moov, trak, mdia,
/// minf, stbl, stsd). Empty for anything else.
fn mp4_sample_entries(path: &Path) -> Vec<[u8; 4]> {
    use std::io::{Read, Seek, SeekFrom};
    fn walk(f: &mut std::fs::File, start: u64, end: u64, out: &mut Vec<[u8; 4]>, depth: u32) {
        let mut at = start;
        while at + 8 <= end && depth < 8 && out.len() < 16 {
            let mut head = [0u8; 16];
            if f.seek(SeekFrom::Start(at)).is_err() || f.read_exact(&mut head[..8]).is_err() {
                return;
            }
            let mut size = u64::from(u32::from_be_bytes([head[0], head[1], head[2], head[3]]));
            let kind = [head[4], head[5], head[6], head[7]];
            let mut body = at + 8;
            if size == 1 {
                if f.read_exact(&mut head[8..16]).is_err() {
                    return;
                }
                size = u64::from_be_bytes(head[8..16].try_into().unwrap_or_default());
                body += 8;
            } else if size == 0 {
                size = end - at;
            }
            if size < body - at || at + size > end {
                return;
            }
            match &kind {
                b"moov" | b"trak" | b"mdia" | b"minf" | b"stbl" => {
                    walk(f, body, at + size, out, depth + 1)
                }
                // Version and flags, the entry count, then the entries.
                b"stsd" => walk(f, body + 8, at + size, out, 8.min(depth + 1)),
                _ if depth >= 6 => out.push(kind),
                _ => {}
            }
            at += size;
        }
    }
    let Ok(mut f) = std::fs::File::open(path) else {
        return Vec::new();
    };
    let Ok(len) = f.metadata().map(|m| m.len()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    walk(&mut f, 0, len, &mut out, 0);
    out
}

/// Names a refused codec plainly, and says when Windows could open it.
fn explain(e: ProbeError) -> ProbeError {
    let ProbeError::UnsupportedCodec(what) = e else {
        return e;
    };
    let what = CODEC_NAMES
        .iter()
        .find(|(id, _)| what.starts_with(id))
        .map(|(_, name)| (*name).to_string())
        .unwrap_or(what);
    if audiform_os_decode::AVAILABLE {
        if what == AC4 {
            // Windows decodes it only with Dolby's own decoder added.
            return ProbeError::UnsupportedCodec(format!(
                "{what}. Windows opens it once Dolby's AC-4 decoder is installed (it comes with Dolby Access and on many PCs with Dolby Atmos)"
            ));
        }
        ProbeError::UnsupportedCodec(what)
    } else {
        ProbeError::UnsupportedCodec(format!("{what}. {}", audiform_os_decode::UNAVAILABLE))
    }
}

/// Describes a file through the decoders built into the system, for what
/// Symphonia cannot read. None where there are none.
fn probe_os(path: &Path, meta: &std::fs::Metadata) -> Option<Result<AudioInfo, ProbeError>> {
    if !audiform_os_decode::AVAILABLE {
        return None;
    }
    let mut os = audiform_os_decode::OsReader::open(path).ok()?;
    let info = os.info().clone();
    // Without a stated length, count it by decoding.
    let length = match info.frames {
        Some(n) => n,
        None => {
            let mut n = 0u64;
            let mut buf = Vec::new();
            loop {
                buf.clear();
                match os.read(&mut buf) {
                    Ok(true) => n += (buf.len() / info.channels.max(1)) as u64,
                    Ok(false) => break,
                    Err(e) => return Some(Err(ProbeError::Damaged(e))),
                }
            }
            n
        }
    };
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_uppercase();
    Some(Ok(AudioInfo {
        path: path.to_path_buf(),
        name: path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        format: if ext.is_empty() {
            info.codec.clone()
        } else {
            ext
        },
        codec: info.codec,
        sample_rate: info.sample_rate,
        channels: u16::try_from(info.channels).unwrap_or(u16::MAX),
        length,
        bit_depth: None,
        file_size: meta.len(),
    }))
}

/// Describes a file through Symphonia.
fn probe_symphonia(path: &Path, meta: &std::fs::Metadata) -> Result<AudioInfo, ProbeError> {
    let (mss, ext) = super::source::open(path)?;
    let mut hint = Hint::new();
    if let Some(ext) = ext.as_deref() {
        hint.with_extension(ext);
    }

    let mut format = symphonia::default::get_probe()
        .probe(
            &hint,
            mss,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|e| match e {
            SymError::IoError(io) if io.kind() == std::io::ErrorKind::UnexpectedEof => {
                ProbeError::Damaged("the file ends too early".into())
            }
            SymError::IoError(io) => ProbeError::Io(io),
            _ => ProbeError::UnknownFormat,
        })?;

    let container = container_label(format.format_info().short_name, path);

    let track = format
        .default_track(TrackType::Audio)
        .ok_or(ProbeError::NoAudio)?
        .clone();
    let params: AudioCodecParameters = track
        .codec_params
        .as_ref()
        .and_then(|p| p.audio())
        .cloned()
        .ok_or(ProbeError::NoAudio)?;

    let registered = super::opus::codecs()
        .get_audio_decoder(params.codec)
        .ok_or_else(|| ProbeError::UnsupportedCodec(params.codec.to_string()))?;
    let codec_name = registered.codec.info.short_name;

    let mut decoder = super::opus::codecs()
        .make_audio_decoder(&params, &AudioDecoderOptions::default())
        .map_err(|e| ProbeError::UnsupportedCodec(format!("{codec_name} ({e})")))?;

    // Decode the first audio packet. This proves the codec variant is
    // supported and fills in rate and channels when the header lacks them.
    let mut sample_rate = params.sample_rate;
    let mut channels = params.channels.as_ref().map(|c| c.count());
    let mut scanned: u64 = 0;
    loop {
        let packet = match format.next_packet() {
            Ok(Some(p)) => p,
            Ok(None) => return Err(ProbeError::Damaged("no audio data".into())),
            Err(e) => return Err(damaged(e)),
        };
        if packet.track_id != track.id {
            continue;
        }
        scanned += packet.dur.get();
        match decoder.decode(&packet) {
            Ok(buf) => {
                let spec = buf.spec();
                sample_rate = sample_rate.or(Some(spec.rate()));
                channels = channels.or(Some(spec.channels().count()));
                break;
            }
            // A bad first packet is common in streams cut mid-frame.
            Err(SymError::DecodeError(_)) => continue,
            Err(SymError::Unsupported(what)) => {
                return Err(ProbeError::UnsupportedCodec(format!(
                    "{codec_name}, {what}"
                )))
            }
            Err(e) => return Err(damaged(e)),
        }
    }

    let sample_rate = sample_rate.filter(|r| *r > 0).ok_or(ProbeError::NoAudio)?;
    let channels = channels.filter(|c| *c > 0).ok_or(ProbeError::NoAudio)?;
    let time_base = track.time_base;

    // A fragmented MP4 (as most download tools save M4A) states a length
    // of 0 in its header: the real length is only known by counting.
    let length = match (
        track.num_frames.filter(|n| *n > 0),
        track.duration.filter(|d| d.get() > 0),
    ) {
        (Some(n), _) => n,
        (None, Some(d)) => to_frames(d.get(), time_base, sample_rate),
        (None, None) => {
            let ticks = count_remaining(format.as_mut(), track.id, scanned)?;
            to_frames(ticks, time_base, sample_rate)
        }
    };

    Ok(AudioInfo {
        path: path.to_path_buf(),
        name: path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        format: container,
        codec: codec_label(codec_name, params.bits_per_sample),
        sample_rate,
        channels: u16::try_from(channels).unwrap_or(u16::MAX),
        length,
        bit_depth: if is_lossless(codec_name) {
            params.bits_per_sample
        } else {
            None
        },
        file_size: meta.len(),
    })
}

/// Sums packet durations to the end of the stream, without decoding.
fn count_remaining(
    format: &mut dyn FormatReader,
    track_id: u32,
    mut ticks: u64,
) -> Result<u64, ProbeError> {
    loop {
        match format.next_packet() {
            Ok(Some(p)) if p.track_id == track_id => ticks += p.dur.get(),
            Ok(Some(_)) => {}
            Ok(None) => return Ok(ticks),
            // A truncated tail still has a usable length.
            Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                return Ok(ticks)
            }
            Err(e) => return Err(damaged(e)),
        }
    }
}

fn to_frames(ticks: u64, time_base: Option<TimeBase>, sample_rate: u32) -> u64 {
    match time_base.and_then(|tb| tb.calc_duration(Duration::new(ticks))) {
        Some(t) => (t.as_secs_f64() * f64::from(sample_rate)).round() as u64,
        None => ticks,
    }
}

fn damaged(e: SymError) -> ProbeError {
    match e {
        SymError::IoError(io) if io.kind() == std::io::ErrorKind::UnexpectedEof => {
            ProbeError::Damaged("the file ends too early".into())
        }
        SymError::IoError(io) => ProbeError::Io(io),
        other => ProbeError::Damaged(other.to_string()),
    }
}

fn container_label(short_name: &str, path: &Path) -> String {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_uppercase();
    match short_name {
        "wave" => "WAV".into(),
        "aiff" => "AIFF".into(),
        "flac" => "FLAC".into(),
        "ogg" => "OGG".into(),
        "mp1" | "mp2" | "mp3" => "MP3".into(),
        "isomp4" if ext == "MP4" => "MP4".into(),
        "isomp4" => "M4A".into(),
        "adts" | "aac" => "AAC".into(),
        _ if !ext.is_empty() => ext,
        other => other.to_ascii_uppercase(),
    }
}

fn is_lossless(codec: &str) -> bool {
    let c = codec.to_ascii_lowercase();
    c.starts_with("pcm") || c == "flac" || c == "alac"
}

fn codec_label(short_name: &str, bits: Option<u32>) -> String {
    let lower = short_name.to_ascii_lowercase();
    if lower.starts_with("pcm") {
        let float = lower.starts_with("pcm_f");
        return match (bits, float) {
            (Some(b), true) => format!("PCM {b}-bit float"),
            (Some(b), false) => format!("PCM {b}-bit"),
            (None, _) => "PCM".into(),
        };
    }
    match lower.as_str() {
        "mp3" | "mp2" | "mp1" | "flac" | "aac" | "alac" => short_name.to_ascii_uppercase(),
        "vorbis" => "Vorbis".into(),
        "opus" => "Opus".into(),
        _ => short_name.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_an_ac4_track_symphonia_skips() {
        fn mp4box(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
            let mut b = ((body.len() + 8) as u32).to_be_bytes().to_vec();
            b.extend_from_slice(kind);
            b.extend_from_slice(body);
            b
        }
        let entry = mp4box(b"ac-4", &[0; 28]);
        let mut stsd = vec![0, 0, 0, 0, 0, 0, 0, 1];
        stsd.extend(entry);
        let mut tree = mp4box(b"stsd", &stsd);
        for k in [b"stbl", b"minf", b"mdia", b"trak", b"moov"] {
            tree = mp4box(k, &tree);
        }
        let mut file = mp4box(b"ftyp", b"mp42\0\0\0\x01mp42");
        file.extend(tree);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("atmos.m4a");
        std::fs::write(&path, &file).unwrap();
        assert_eq!(mp4_sample_entries(&path), vec![*b"ac-4"]);
        assert!(matches!(
            name_hidden_codec(&path, ProbeError::NoAudio),
            ProbeError::UnsupportedCodec(n) if n == AC4
        ));
        // Other errors pass through.
        assert!(matches!(
            name_hidden_codec(&path, ProbeError::UnknownFormat),
            ProbeError::UnknownFormat
        ));
    }

    #[test]
    fn labels_containers() {
        assert_eq!(container_label("wave", Path::new("a.wav")), "WAV");
        assert_eq!(container_label("isomp4", Path::new("a.m4a")), "M4A");
        assert_eq!(container_label("isomp4", Path::new("a.mp4")), "MP4");
        assert_eq!(container_label("mystery", Path::new("a.xyz")), "XYZ");
    }

    #[test]
    fn labels_codecs() {
        assert_eq!(codec_label("pcm_s24le", Some(24)), "PCM 24-bit");
        assert_eq!(codec_label("pcm_f32le", Some(32)), "PCM 32-bit float");
        assert_eq!(codec_label("vorbis", None), "Vorbis");
        assert_eq!(codec_label("opus", None), "Opus");
        assert_eq!(codec_label("vorbis", None), "Vorbis");
    }

    #[test]
    fn refuses_missing_and_unsupported_files() {
        assert!(matches!(
            probe(Path::new("definitely/not/here.wav")),
            Err(ProbeError::NotFound)
        ));
        let dir = tempfile::tempdir().unwrap();
        let txt = dir.path().join("notes.txt");
        std::fs::write(&txt, b"hello").unwrap();
        assert!(matches!(probe(&txt), Err(ProbeError::UnsupportedType)));
        let fake = dir.path().join("fake.wav");
        std::fs::write(&fake, b"this is not audio at all").unwrap();
        assert!(matches!(probe(&fake), Err(ProbeError::UnknownFormat)));
    }
}
