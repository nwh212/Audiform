// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

//! DTS audio: WAV files whose samples are really a DTS stream (as on DTS
//! music CDs, often 5.1), and raw `.dts` files.
//!
//! A DTS WAV looks like plain 16-bit stereo, so without this it plays as
//! loud noise. We look for DTS sync words at the start of the audio data
//! and, if they line up frame after frame, decode the stream instead and
//! show the file with its real channels and rate.
//!
//! The stream may be stored in 16-bit words or packed in 14 of every 16
//! bits (the CD form), either byte order. The core decoder (`core.rs`)
//! takes it as big-endian 16-bit words.

mod core;
mod tables;

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use self::core::{
    CoreDecoder, Header, SPEAKERS, SPK_C, SPK_CS, SPK_L, SPK_LFE, SPK_LS, SPK_R, SPK_RS,
};
use super::ProbeError;

/// How the stream's words are stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Packing {
    Be16,
    Le16,
    Be14,
    Le14,
}

/// How far into the audio data to look for the first sync word.
const SNIFF_BYTES: usize = 64 * 1024;
/// Consecutive frames that must line up before a WAV counts as DTS.
const CONFIRM_FRAMES: usize = 3;

impl Packing {
    /// The packing whose sync word starts at `b`, if any.
    fn at(b: &[u8]) -> Option<Self> {
        match b {
            [0x7F, 0xFE, 0x80, 0x01, ..] => Some(Self::Be16),
            [0xFE, 0x7F, 0x01, 0x80, ..] => Some(Self::Le16),
            [0x1F, 0xFF, 0xE8, 0x00, 0x07, x, ..] if x & 0xF0 == 0xF0 => Some(Self::Be14),
            [0xFF, 0x1F, 0x00, 0xE8, x, 0x07, ..] if x & 0xF0 == 0xF0 => Some(Self::Le14),
            _ => None,
        }
    }

    /// Stored bytes needing `n` stream bytes.
    fn raw_len(self, n: usize) -> usize {
        match self {
            Self::Be16 | Self::Le16 => n.div_ceil(2) * 2,
            Self::Be14 | Self::Le14 => (n * 8).div_ceil(14) * 2,
        }
    }

    /// Converts stored bytes to big-endian 16-bit stream words.
    fn convert(self, raw: &[u8], out: &mut Vec<u8>) {
        out.clear();
        match self {
            Self::Be16 => out.extend_from_slice(raw),
            Self::Le16 => {
                for w in raw.as_chunks::<2>().0 {
                    out.extend_from_slice(&[w[1], w[0]]);
                }
            }
            Self::Be14 | Self::Le14 => {
                let mut acc: u32 = 0;
                let mut bits = 0u32;
                for w in raw.as_chunks::<2>().0 {
                    let v = if self == Self::Be14 {
                        u16::from_be_bytes([w[0], w[1]])
                    } else {
                        u16::from_le_bytes([w[0], w[1]])
                    };
                    acc = (acc << 14) | u32::from(v & 0x3FFF);
                    bits += 14;
                    while bits >= 8 {
                        bits -= 8;
                        out.push((acc >> bits) as u8);
                    }
                    acc &= (1 << bits) - 1;
                }
                if bits > 0 {
                    out.push((acc << (8 - bits)) as u8);
                }
            }
        }
    }
}

/// Where each output channel's sound comes from.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Src {
    Speaker(usize),
    /// A single surround channel, fed to both sides at -3 dB.
    HalfSurround,
    Silent,
}

/// A DTS stream found in a file, with its frames indexed.
#[derive(Debug, Clone)]
pub struct DtsFile {
    /// Where the stream's bytes start in the file.
    data_start: u64,
    packing: Packing,
    /// Each frame's offset from `data_start` and stored length.
    frames: Vec<(u64, usize)>,
    pub sample_rate: u32,
    /// Samples per channel in each frame.
    frame_samples: usize,
    layout: Vec<Src>,
    /// Is the outer file a WAV (else a raw `.dts`)?
    pub in_wav: bool,
}

impl DtsFile {
    pub fn channels(&self) -> usize {
        self.layout.len()
    }

    /// Length in sample frames.
    pub fn length(&self) -> u64 {
        (self.frames.len() * self.frame_samples) as u64
    }
}

/// The audio data of a WAV file: its start and length, if `head` is a
/// RIFF WAVE header with a `data` chunk.
fn wav_data(file: &mut File) -> Option<(u64, u64)> {
    let mut riff = [0u8; 12];
    file.seek(SeekFrom::Start(0)).ok()?;
    file.read_exact(&mut riff).ok()?;
    if &riff[0..4] != b"RIFF" || &riff[8..12] != b"WAVE" {
        return None;
    }
    let mut at = 12u64;
    let mut fmt_ok = false;
    for _ in 0..64 {
        let mut head = [0u8; 8];
        file.seek(SeekFrom::Start(at)).ok()?;
        file.read_exact(&mut head).ok()?;
        let size = u64::from(u32::from_le_bytes([head[4], head[5], head[6], head[7]]));
        match &head[0..4] {
            b"fmt " => {
                let mut fmt = [0u8; 16];
                file.read_exact(&mut fmt).ok()?;
                let tag = u16::from_le_bytes([fmt[0], fmt[1]]);
                let bits = u16::from_le_bytes([fmt[14], fmt[15]]);
                // Plain 16-bit PCM, the DTS tag, or the extensible form.
                fmt_ok = (tag == 1 && bits == 16) || tag == 0x2001 || tag == 0xFFFE;
            }
            b"data" => {
                let len = file.metadata().ok()?.len();
                let size = if size == 0 || size == 0xFFFF_FFFF {
                    len.saturating_sub(at + 8)
                } else {
                    size.min(len.saturating_sub(at + 8))
                };
                return fmt_ok.then_some((at + 8, size));
            }
            _ => {}
        }
        at += 8 + size + (size & 1);
    }
    None
}

fn is_dts_ext(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("dts") || e.eq_ignore_ascii_case("cpt"))
}

/// Reads one frame's header at `pos` of `data`, if a sync word is there.
fn header_at(data: &[u8], pos: usize, packing: Packing, scratch: &mut Vec<u8>) -> Option<Header> {
    let raw = data.get(pos..pos + packing.raw_len(32).min(data.len() - pos))?;
    packing.convert(raw, scratch);
    core::parse_header(&mut core::Bits::new(scratch)).ok()
}

/// Looks for a DTS stream in a WAV file or a raw `.dts` file. Returns
/// None (quickly) for anything else.
pub fn sniff(path: &Path) -> Option<DtsFile> {
    let mut file = File::open(path).ok()?;
    let (start, len, in_wav) = match wav_data(&mut file) {
        Some((s, l)) => (s, l, true),
        None if is_dts_ext(path) => (0, file.metadata().ok()?.len(), false),
        None => return None,
    };
    // A first look at the start of the data.
    let mut head = vec![0u8; SNIFF_BYTES.min(len as usize)];
    file.seek(SeekFrom::Start(start)).ok()?;
    file.read_exact(&mut head).ok()?;
    let mut scratch = Vec::new();
    let (first, packing) = (0..head.len().saturating_sub(8))
        .step_by(2)
        .find_map(|i| Packing::at(&head[i..]).map(|p| (i, p)))?;
    let first_header = header_at(&head, first, packing, &mut scratch)?;
    // Now the whole stream, frame by frame.
    let mut data = Vec::with_capacity(len as usize);
    file.seek(SeekFrom::Start(start)).ok()?;
    file.take(len).read_to_end(&mut data).ok()?;
    let mut frames = Vec::new();
    let mut pos = first;
    while pos + 8 <= data.len() {
        let Some(h) = header_at(&data, pos, packing, &mut scratch) else {
            break;
        };
        let raw = packing.raw_len(h.frame_size);
        frames.push((pos as u64, raw.min(data.len() - pos)));
        // The next frame follows, perhaps after some padding.
        let from = pos + raw.max(2);
        let limit = (from + SNIFF_BYTES).min(data.len().saturating_sub(4));
        let next = (from..limit)
            .step_by(2)
            .find(|&i| Packing::at(&data[i..]) == Some(packing));
        match next {
            Some(n) => pos = n,
            None => break,
        }
    }
    // Real PCM can hold a sync word by chance; DTS has many in a row.
    if frames.len() < CONFIRM_FRAMES.min(if in_wav { CONFIRM_FRAMES } else { 1 }) {
        return None;
    }
    let mut layout = layout_for(&first_header);
    if layout.is_empty() {
        layout = vec![Src::Speaker(SPK_L), Src::Speaker(SPK_R)];
    }
    Some(DtsFile {
        data_start: start,
        packing,
        frames,
        sample_rate: first_header.sample_rate,
        frame_samples: first_header.samples(),
        layout,
        in_wav,
    })
}

/// The channels to show, in the multichannel WAV order the app uses
/// (L, R, C, LFE, Ls, Rs; 6.1 adds the back center before the sides).
fn layout_for(h: &Header) -> Vec<Src> {
    const PRIMARY: [&[usize]; 10] = [
        &[SPK_C],
        &[SPK_L, SPK_R],
        &[SPK_L, SPK_R],
        &[SPK_L, SPK_R],
        &[SPK_L, SPK_R],
        &[SPK_C, SPK_L, SPK_R],
        &[SPK_L, SPK_R, SPK_CS],
        &[SPK_C, SPK_L, SPK_R, SPK_CS],
        &[SPK_L, SPK_R, SPK_LS, SPK_RS],
        &[SPK_C, SPK_L, SPK_R, SPK_LS, SPK_RS],
    ];
    let mut has = [false; SPEAKERS];
    for &s in PRIMARY[h.audio_mode] {
        has[s] = true;
    }
    has[SPK_LFE] = h.lfe_present != 0;
    let xch = h.ext_audio_present && h.ext_audio_type == 0;
    let sp = Src::Speaker;
    if xch && has[SPK_LS] {
        return vec![
            sp(SPK_L),
            sp(SPK_R),
            sp(SPK_C),
            sp(SPK_LFE),
            sp(SPK_CS),
            sp(SPK_LS),
            sp(SPK_RS),
        ];
    }
    // One surround channel: it plays from both sides.
    let half = has[SPK_CS];
    let (ls, rs) = if half {
        (Src::HalfSurround, Src::HalfSurround)
    } else {
        (sp(SPK_LS), sp(SPK_RS))
    };
    let surround = half || has[SPK_LS];
    match (has[SPK_C], has[SPK_L], surround, has[SPK_LFE]) {
        (true, false, false, false) => vec![sp(SPK_C)],
        (false, true, false, false) => vec![sp(SPK_L), sp(SPK_R)],
        (true, true, false, false) => vec![sp(SPK_L), sp(SPK_R), sp(SPK_C)],
        (false, true, true, false) => vec![sp(SPK_L), sp(SPK_R), ls, rs],
        (true, true, true, false) => vec![sp(SPK_L), sp(SPK_R), sp(SPK_C), ls, rs],
        (c, l, _, _) => vec![
            if l { sp(SPK_L) } else { Src::Silent },
            if l { sp(SPK_R) } else { Src::Silent },
            if c { sp(SPK_C) } else { Src::Silent },
            if has[SPK_LFE] {
                sp(SPK_LFE)
            } else {
                Src::Silent
            },
            if surround { ls } else { Src::Silent },
            if surround { rs } else { Src::Silent },
        ],
    }
}

/// Decodes a DTS file frame by frame, as interleaved f32.
pub struct DtsStream {
    file: File,
    info: DtsFile,
    decoder: CoreDecoder,
    next: usize,
    raw: Vec<u8>,
    stream: Vec<u8>,
}

impl DtsStream {
    pub fn open(path: &Path, info: DtsFile) -> Result<Self, ProbeError> {
        Ok(Self {
            file: File::open(path)?,
            info,
            decoder: CoreDecoder::new(),
            next: 0,
            raw: Vec::new(),
            stream: Vec::new(),
        })
    }

    /// Moves to sample frame `at`. Returns the frame actually reached (the
    /// start of the DTS frame holding it).
    pub fn seek(&mut self, at: u64) -> u64 {
        let per = self.info.frame_samples.max(1) as u64;
        // Start one frame early so the filters have settled by `at`.
        let target = (at / per) as usize;
        let warm = target.saturating_sub(1);
        self.decoder.reset();
        self.next = warm;
        if warm < target {
            let mut scratch = Vec::new();
            let _ = self.next_into(&mut scratch);
        }
        self.next = target.min(self.info.frames.len());
        self.next as u64 * per
    }

    /// Appends the next frame's samples. Returns false at the end.
    pub fn next_into(&mut self, out: &mut Vec<f32>) -> Result<bool, ProbeError> {
        let Some(&(offset, len)) = self.info.frames.get(self.next) else {
            return Ok(false);
        };
        self.next += 1;
        self.raw.resize(len, 0);
        self.file
            .seek(SeekFrom::Start(self.info.data_start + offset))?;
        self.file.read_exact(&mut self.raw)?;
        self.info.packing.convert(&self.raw, &mut self.stream);
        let n = self.info.frame_samples;
        let channels = self.info.layout.len();
        let start = out.len();
        out.resize(start + n * channels, 0.0);
        // A damaged frame plays as silence rather than stopping the file.
        let Ok(frame) = self.decoder.decode(&self.stream) else {
            return Ok(true);
        };
        const SCALE: f32 = 1.0 / (1 << 23) as f32;
        const HALF: f32 = std::f32::consts::FRAC_1_SQRT_2 * SCALE;
        for (c, src) in self.info.layout.iter().enumerate() {
            let (buf, gain) = match *src {
                Src::Speaker(s) => (frame.speakers[s].as_ref(), SCALE),
                Src::HalfSurround => (frame.speakers[SPK_CS].as_ref(), HALF),
                Src::Silent => (None, 0.0),
            };
            if let Some(buf) = buf {
                for (i, v) in buf.iter().take(n).enumerate() {
                    out[start + i * channels + c] = *v as f32 * gain;
                }
            }
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fourteen_bit_words_unpack() {
        // Two 14-bit words: all ones then a pattern.
        let raw = [0x3F, 0xFF, 0x00, 0x01];
        let mut out = Vec::new();
        Packing::Be14.convert(&raw, &mut out);
        // 11111111 111111 00 000000 0001 + pad
        assert_eq!(out, [0xFF, 0xFC, 0x00, 0x10]);
        let le = [0xFF, 0x3F, 0x01, 0x00];
        let mut out2 = Vec::new();
        Packing::Le14.convert(&le, &mut out2);
        assert_eq!(out, out2);
    }

    #[test]
    fn sync_words_name_their_packing() {
        assert_eq!(Packing::at(&[0x7F, 0xFE, 0x80, 0x01]), Some(Packing::Be16));
        assert_eq!(Packing::at(&[0xFE, 0x7F, 0x01, 0x80]), Some(Packing::Le16));
        assert_eq!(
            Packing::at(&[0x1F, 0xFF, 0xE8, 0x00, 0x07, 0xF1]),
            Some(Packing::Be14)
        );
        assert_eq!(
            Packing::at(&[0xFF, 0x1F, 0x00, 0xE8, 0xF1, 0x07]),
            Some(Packing::Le14)
        );
        assert_eq!(Packing::at(&[0, 0, 0, 0, 0, 0]), None);
    }

    fn fixture() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/media/dts_51_14bit.wav")
    }

    /// The strongest frequency in `x` among `candidates`, by correlation.
    fn tone_of(x: &[f32], rate: f32, candidates: &[f32]) -> f32 {
        let power = |f: f32| {
            let (mut re, mut im) = (0f32, 0f32);
            for (n, v) in x.iter().enumerate() {
                let ph = 2.0 * std::f32::consts::PI * f * n as f32 / rate;
                re += v * ph.cos();
                im += v * ph.sin();
            }
            re * re + im * im
        };
        *candidates
            .iter()
            .max_by(|a, b| power(**a).total_cmp(&power(**b)))
            .unwrap()
    }

    #[test]
    fn a_dts_wav_decodes_as_five_point_one() {
        // A 5.1 DTS stream packed in 14-bit words, in a 16-bit stereo WAV,
        // with a different tone on each channel (L R C LFE Ls Rs).
        let info = sniff(&fixture()).expect("a DTS WAV");
        assert!(info.in_wav);
        assert_eq!((info.channels(), info.sample_rate), (6, 44_100));
        let mut s = DtsStream::open(&fixture(), info).unwrap();
        let mut out = Vec::new();
        while s.next_into(&mut out).unwrap() {}
        let tones = [440.0, 550.0, 660.0, 60.0, 770.0, 880.0];
        for (c, want) in tones.iter().enumerate() {
            let ch: Vec<f32> = out
                .iter()
                .skip(c)
                .step_by(6)
                .skip(2048)
                .take(8192)
                .copied()
                .collect();
            let peak = ch.iter().fold(0f32, |a, v| a.max(v.abs()));
            assert!(peak > 0.2 && peak < 0.35, "channel {c} peak {peak}");
            assert_eq!(tone_of(&ch, 44_100.0, &tones), *want, "channel {c}");
        }
    }

    #[test]
    fn dts_files_open_in_the_app() {
        let info = crate::codecs::probe(&fixture()).unwrap();
        assert_eq!((info.codec.as_str(), info.channels), ("DTS", 6));
        let mut r = crate::codecs::AudioReader::open(&fixture(), 6, None).unwrap();
        r.seek(0.2).unwrap();
        let mut buf = Vec::new();
        assert!(r.read_into(&mut buf).unwrap());
        assert!(buf.iter().any(|v| v.abs() > 0.1));
    }

    #[test]
    fn plain_wavs_are_not_dts() {
        let wav = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/media/stereo_48k_16.wav");
        assert!(sniff(&wav).is_none());
    }
}
