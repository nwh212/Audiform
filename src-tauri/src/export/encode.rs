// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Writing audio files in the formats people deliver: WAV, FLAC and MP3.
//!
//! One `Writer` takes interleaved float samples at the source rate and
//! channel count, converts channels and sample rate when the output asks
//! for it, and encodes. Mixdowns, Batch Convert and saving in the
//! Waveform editor all go through it, so a format setting means the same
//! thing everywhere.
//!
//! FLAC uses the pure-Rust `flacenc` crate (Apache-2.0). MP3 uses LAME
//! (LGPL) through `mp3lame-encoder`; MP3 is lossy, so the app warns
//! before saving over an original with it. AAC (M4A) uses the encoder
//! built into the operating system; see `aac.rs`.

use std::fs::File;
use std::io::{BufWriter, Seek, SeekFrom, Write};
use std::num::NonZeroU32;
use std::path::Path;

use audioadapter_buffers::owned::InterleavedOwned;
use rubato::{Async, FixedAsync, Resampler, SincInterpolationParameters};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::codecs::map_channels;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FormatKind {
    #[default]
    Wav,
    Flac,
    Mp3,
    /// AAC in an M4A file.
    M4a,
}

impl FormatKind {
    pub fn extension(self) -> &'static str {
        match self {
            FormatKind::Wav => "wav",
            FormatKind::Flac => "flac",
            FormatKind::Mp3 => "mp3",
            FormatKind::M4a => "m4a",
        }
    }

    /// The format a file extension saves as, if it is one we write.
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            "wav" | "wave" => Some(FormatKind::Wav),
            "flac" => Some(FormatKind::Flac),
            "mp3" => Some(FormatKind::Mp3),
            "m4a" => Some(FormatKind::M4a),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Mp3Mode {
    /// Constant bit rate: `mp3_bitrate` kbps. What podcast hosts expect.
    #[default]
    Cbr,
    /// Variable bit rate at `mp3_quality` (0 best, 9 smallest).
    Vbr,
}

/// What to write. Unset fields keep the source's value.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct OutputFormat {
    pub kind: FormatKind,
    /// WAV: 16, 24 or 32 (float). FLAC: 16 or 24. Ignored for MP3.
    pub bit_depth: Option<u16>,
    pub sample_rate: Option<u32>,
    /// 1 or 2. MP3 is always 1 or 2; WAV and FLAC keep up to 8.
    pub channels: Option<u16>,
    pub mp3_mode: Mp3Mode,
    /// kbps for constant bit rate.
    pub mp3_bitrate: u16,
    /// 0 (best) to 9 for variable bit rate.
    pub mp3_quality: u8,
    /// AAC bit rate in kbps: 96, 128, 160 or 192.
    pub aac_bitrate: u16,
    /// Dither when writing fewer bits than the audio has. On unless turned off.
    pub dither: bool,
    /// Going from stereo to mono: how much of the left and right goes in,
    /// 0 to 1 each.
    pub mix_left: f32,
    pub mix_right: f32,
}

impl Default for OutputFormat {
    fn default() -> Self {
        Self {
            kind: FormatKind::Wav,
            bit_depth: None,
            sample_rate: None,
            channels: None,
            mp3_mode: Mp3Mode::Cbr,
            mp3_bitrate: 192,
            mp3_quality: 2,
            aac_bitrate: 128,
            dither: true,
            mix_left: 0.5,
            mix_right: 0.5,
        }
    }
}

/// Constant bit rates LAME accepts, in kbps.
pub const MP3_BITRATES: [u16; 16] = [
    8, 16, 24, 32, 40, 48, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320,
];
/// Rates an MP3 can carry.
const MP3_RATES: [u32; 9] = [
    8_000, 11_025, 12_000, 16_000, 22_050, 24_000, 32_000, 44_100, 48_000,
];

/// An `OutputFormat` with every choice made for one source.
#[derive(Debug, Clone, PartialEq)]
pub struct Resolved {
    pub kind: FormatKind,
    pub bits: u16,
    pub rate: u32,
    pub channels: u16,
    pub mp3_mode: Mp3Mode,
    pub mp3_bitrate: u16,
    pub mp3_quality: u8,
    pub aac_bitrate: u16,
    pub dither: bool,
    /// Left and right weights for a stereo-to-mono fold.
    pub mix: [f32; 2],
}

#[derive(Debug, Error)]
pub enum EncodeError {
    #[error("{0}")]
    Settings(String),
    #[error("Could not write the file: {0}")]
    Write(String),
}

fn werr(e: impl std::fmt::Display) -> EncodeError {
    EncodeError::Write(e.to_string())
}

impl OutputFormat {
    /// Fills in the source's rate, channels and depth where unset, and
    /// checks the result can be written.
    pub fn resolve(
        &self,
        src_rate: u32,
        src_channels: u16,
        src_bits: u16,
    ) -> Result<Resolved, EncodeError> {
        let mut rate = self.sample_rate.unwrap_or(src_rate);
        let mut channels = self.channels.unwrap_or(src_channels).max(1);
        let bits = match self.kind {
            FormatKind::Wav => match self.bit_depth {
                // Chosen outright: 4 (ADPCM) and 8 only ever come from here.
                Some(b) => b,
                None => match src_bits {
                    0..=16 => 16,
                    17..=24 => 24,
                    _ => 32,
                },
            },
            FormatKind::Flac => match self.bit_depth.unwrap_or(src_bits) {
                0..=16 => 16,
                _ => 24,
            },
            FormatKind::Mp3 | FormatKind::M4a => 16,
        };
        if let Some(b) = self.bit_depth {
            let ok = match self.kind {
                FormatKind::Wav => [4, 8, 16, 24, 32].contains(&b),
                FormatKind::Flac => [16, 24].contains(&b),
                FormatKind::Mp3 | FormatKind::M4a => true,
            };
            if !ok {
                return Err(EncodeError::Settings(format!(
                    "{} can't be written at {b}-bit.",
                    self.kind.extension().to_uppercase()
                )));
            }
        }
        if self.kind == FormatKind::Mp3 {
            channels = channels.min(2);
            // An MP3 tops out at 48 kHz: higher rates come down to the nearest it carries.
            if !MP3_RATES.contains(&rate) {
                rate = MP3_RATES
                    .iter()
                    .copied()
                    .filter(|r| *r <= rate)
                    .max()
                    .unwrap_or(8_000);
            }
            if self.mp3_mode == Mp3Mode::Cbr && !MP3_BITRATES.contains(&self.mp3_bitrate) {
                return Err(EncodeError::Settings(format!(
                    "{} kbps isn't an MP3 bit rate.",
                    self.mp3_bitrate
                )));
            }
        }
        if self.kind == FormatKind::M4a {
            if !super::aac::AVAILABLE {
                return Err(EncodeError::Settings(super::aac::UNAVAILABLE.into()));
            }
            channels = channels.min(2);
            rate = super::aac::pick_rate(rate);
        }
        if !(1..=8).contains(&channels) {
            return Err(EncodeError::Settings(
                "Channels must be between 1 and 8.".into(),
            ));
        }
        if !(8_000..=384_000).contains(&rate) {
            return Err(EncodeError::Settings(format!(
                "{rate} Hz isn't a sample rate we can write."
            )));
        }
        Ok(Resolved {
            kind: self.kind,
            bits,
            rate,
            channels,
            mp3_mode: self.mp3_mode,
            mp3_bitrate: self.mp3_bitrate,
            mp3_quality: self.mp3_quality.min(9),
            aac_bitrate: super::aac::nearest_bitrate(self.aac_bitrate),
            dither: self.dither,
            mix: [
                self.mix_left.clamp(0.0, 1.0),
                self.mix_right.clamp(0.0, 1.0),
            ],
        })
    }
}

impl Resolved {
    /// Short description, such as "MP3 192 kbps CBR, 44100 Hz stereo".
    pub fn describe(&self) -> String {
        let ch = match self.channels {
            1 => "mono".to_string(),
            2 => "stereo".to_string(),
            n => format!("{n} ch"),
        };
        match self.kind {
            FormatKind::Mp3 => match self.mp3_mode {
                Mp3Mode::Cbr => format!("MP3 {} kbps CBR, {} Hz {ch}", self.mp3_bitrate, self.rate),
                Mp3Mode::Vbr => format!(
                    "MP3 VBR quality {}, {} Hz {ch}",
                    self.mp3_quality, self.rate
                ),
            },
            FormatKind::Wav if self.bits == 4 => {
                format!("WAV 4-bit IMA ADPCM, {} Hz {ch}", self.rate)
            }
            FormatKind::Wav if self.bits == 32 => {
                format!("WAV 32-bit float, {} Hz {ch}", self.rate)
            }
            FormatKind::Wav => format!("WAV {}-bit, {} Hz {ch}", self.bits, self.rate),
            FormatKind::Flac => format!("FLAC {}-bit, {} Hz {ch}", self.bits, self.rate),
            FormatKind::M4a => format!("AAC {} kbps, {} Hz {ch}", self.aac_bitrate, self.rate),
        }
    }
}

/// Triangular dither of one step at the target depth. Off for float.
pub(crate) struct Dither {
    on: bool,
    state: u32,
}

impl Dither {
    /// Dither that can be turned off; float output never dithers.
    pub(crate) fn with(bits: u16, on: bool) -> Self {
        Self {
            on: on && bits < 32,
            state: 0x1234_5678,
        }
    }

    fn uniform(&mut self) -> f32 {
        // xorshift32: fast, repeatable, good enough for dither.
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        (x as f32 / u32::MAX as f32) - 0.5
    }

    pub(crate) fn next(&mut self) -> f32 {
        if self.on {
            self.uniform() + self.uniform()
        } else {
            0.0
        }
    }

    /// One float sample as a dithered integer at `bits` (8, 16 or 24).
    pub(crate) fn int(&mut self, v: f32, bits: u16) -> i32 {
        let max = match bits {
            8 => 127.0,
            16 => 32_767.0,
            _ => 8_388_607.0,
        };
        (v * max + self.next()).round().clamp(-max - 1.0, max) as i32
    }
}

/// Streaming sample-rate conversion that keeps the length exact.
struct Resample {
    r: Async<f32>,
    ch: usize,
    pending: Vec<f32>,
    skip: usize,
    ratio: f64,
    frames_in: u64,
    frames_out: u64,
}

const RESAMPLE_CHUNK: usize = 1_024;

impl Resample {
    fn new(from: u32, to: u32, ch: usize) -> Result<Self, EncodeError> {
        let ratio = f64::from(to) / f64::from(from);
        let r = Async::<f32>::new_sinc(
            ratio,
            1.0,
            &SincInterpolationParameters::default(),
            RESAMPLE_CHUNK,
            ch,
            FixedAsync::Input,
        )
        .map_err(|e| EncodeError::Settings(e.to_string()))?;
        Ok(Self {
            skip: r.output_delay() * ch,
            r,
            ch,
            pending: Vec::new(),
            ratio,
            frames_in: 0,
            frames_out: 0,
        })
    }

    fn push(&mut self, input: &[f32], out: &mut Vec<f32>) -> Result<(), EncodeError> {
        self.frames_in += (input.len() / self.ch) as u64;
        self.pending.extend_from_slice(input);
        self.drain(out, u64::MAX)
    }

    fn drain(&mut self, out: &mut Vec<f32>, limit: u64) -> Result<(), EncodeError> {
        loop {
            let need = self.r.input_frames_next() * self.ch;
            if self.pending.len() < need || self.frames_out >= limit {
                return Ok(());
            }
            let chunk: Vec<f32> = self.pending.drain(..need).collect();
            let input = InterleavedOwned::new_from(chunk, self.ch, need / self.ch).map_err(werr)?;
            let data = self.r.process(&input, None).map_err(werr)?.take_data();
            let drop = self.skip.min(data.len());
            self.skip -= drop;
            let room = (limit.saturating_sub(self.frames_out) as usize).saturating_mul(self.ch);
            let take = (data.len() - drop).min(room);
            out.extend_from_slice(&data[drop..drop + take]);
            self.frames_out += (take / self.ch) as u64;
        }
    }

    /// Pushes the tail through and stops at exactly the converted length.
    fn finish(&mut self, out: &mut Vec<f32>) -> Result<(), EncodeError> {
        let target = (self.frames_in as f64 * self.ratio).round() as u64;
        while self.frames_out < target {
            let pad = self.r.input_frames_next() * self.ch;
            self.pending.resize(self.pending.len() + pad, 0.0);
            self.drain(out, target)?;
        }
        Ok(())
    }
}

enum Inner {
    Aac {
        w: super::aac::AacWriter,
        dither: Dither,
        buf: Vec<i16>,
    },
    Adpcm {
        w: super::adpcm::AdpcmWriter,
        dither: Dither,
        buf: Vec<i16>,
    },
    Wav {
        w: hound::WavWriter<BufWriter<File>>,
        bits: u16,
        dither: Dither,
    },
    Flac(Box<FlacOut>),
    Mp3 {
        enc: mp3lame_encoder::Encoder,
        file: BufWriter<File>,
        buf: Vec<u8>,
        mono: bool,
        vbr: bool,
    },
}

struct FlacOut {
    file: BufWriter<File>,
    cfg: flacenc::error::Verified<flacenc::config::Encoder>,
    info: flacenc::component::StreamInfo,
    ctx: flacenc::source::Context,
    fb: flacenc::source::FrameBuf,
    pending: Vec<i32>,
    frame: usize,
    ch: usize,
    bits: u16,
    dither: Dither,
}

const FLAC_BLOCK: usize = 4_096;

impl FlacOut {
    fn header(info: &flacenc::component::StreamInfo) -> Result<Vec<u8>, EncodeError> {
        use flacenc::component::BitRepr;
        let stream = flacenc::component::Stream::with_stream_info(info.clone());
        let mut sink = flacenc::bitsink::ByteSink::new();
        stream
            .write(&mut sink)
            .map_err(|e| werr(format!("{e:?}")))?;
        Ok(sink.as_slice().to_vec())
    }

    fn encode_block(&mut self, samples: &[i32]) -> Result<(), EncodeError> {
        use flacenc::component::BitRepr;
        use flacenc::source::Fill;
        self.fb
            .fill_interleaved(samples)
            .map_err(|e| werr(format!("{e:?}")))?;
        self.ctx
            .fill_interleaved(samples)
            .map_err(|e| werr(format!("{e:?}")))?;
        let frame = flacenc::encode_fixed_size_frame(&self.cfg, &self.fb, self.frame, &self.info)
            .map_err(|e| werr(format!("{e:?}")))?;
        self.info.update_frame_info(&frame);
        let mut sink = flacenc::bitsink::ByteSink::new();
        frame.write(&mut sink).map_err(|e| werr(format!("{e:?}")))?;
        self.file.write_all(sink.as_slice()).map_err(werr)?;
        self.frame += 1;
        Ok(())
    }
}

/// Writes one file. Feed it interleaved floats with `write`, then `finish`.
pub struct Writer {
    inner: Inner,
    in_ch: usize,
    out_ch: usize,
    mix: [f32; 2],
    resample: Option<Resample>,
    mapped: Vec<f32>,
    converted: Vec<f32>,
    frames: u64,
}

impl Writer {
    /// Creates `path` for samples arriving at `in_rate` with `in_ch` channels.
    pub fn create(
        path: &Path,
        fmt: &Resolved,
        in_rate: u32,
        in_ch: u16,
    ) -> Result<Self, EncodeError> {
        let out_ch = usize::from(fmt.channels);
        let inner = match fmt.kind {
            FormatKind::M4a => Inner::Aac {
                w: super::aac::AacWriter::create(path, fmt.rate, fmt.channels, fmt.aac_bitrate)
                    .map_err(EncodeError::Write)?,
                dither: Dither::with(16, fmt.dither),
                buf: Vec::new(),
            },
            FormatKind::Wav if fmt.bits == 4 => Inner::Adpcm {
                w: super::adpcm::AdpcmWriter::create(path, fmt.rate, fmt.channels).map_err(werr)?,
                dither: Dither::with(16, fmt.dither),
                buf: Vec::new(),
            },
            FormatKind::Wav => {
                let spec = hound::WavSpec {
                    channels: fmt.channels,
                    sample_rate: fmt.rate,
                    bits_per_sample: fmt.bits,
                    sample_format: if fmt.bits == 32 {
                        hound::SampleFormat::Float
                    } else {
                        hound::SampleFormat::Int
                    },
                };
                Inner::Wav {
                    w: hound::WavWriter::create(path, spec).map_err(werr)?,
                    bits: fmt.bits,
                    dither: Dither::with(fmt.bits, fmt.dither),
                }
            }
            FormatKind::Flac => {
                use flacenc::error::Verify;
                let mut cfg = flacenc::config::Encoder::default();
                cfg.block_size = FLAC_BLOCK;
                let cfg = cfg
                    .into_verified()
                    .map_err(|e| EncodeError::Settings(format!("{e:?}")))?;
                let info = flacenc::component::StreamInfo::new(
                    fmt.rate as usize,
                    out_ch,
                    usize::from(fmt.bits),
                )
                .map_err(|e| EncodeError::Settings(format!("{e:?}")))?;
                let mut file = BufWriter::new(File::create(path).map_err(werr)?);
                // The header is written again at the end, once the length and checksum are known.
                file.write_all(&FlacOut::header(&info)?).map_err(werr)?;
                Inner::Flac(Box::new(FlacOut {
                    file,
                    cfg,
                    info,
                    ctx: flacenc::source::Context::new(usize::from(fmt.bits), out_ch),
                    fb: flacenc::source::FrameBuf::with_size(out_ch, FLAC_BLOCK)
                        .map_err(|e| werr(format!("{e:?}")))?,
                    pending: Vec::new(),
                    frame: 0,
                    ch: out_ch,
                    bits: fmt.bits,
                    dither: Dither::with(fmt.bits, fmt.dither),
                }))
            }
            FormatKind::Mp3 => {
                use mp3lame_encoder::{Bitrate, Builder, Mode, Quality};
                let mut b = Builder::new().ok_or_else(|| {
                    EncodeError::Settings("The MP3 encoder could not start.".into())
                })?;
                let s = |e: mp3lame_encoder::BuildError| {
                    EncodeError::Settings(format!("MP3 settings: {e:?}"))
                };
                b.set_num_channels(fmt.channels as u8).map_err(s)?;
                b.set_sample_rate(fmt.rate).map_err(s)?;
                b.set_output_sample_rate(NonZeroU32::new(fmt.rate))
                    .map_err(s)?;
                b.set_mode(if fmt.channels == 1 {
                    Mode::Mono
                } else {
                    Mode::JointStereo
                })
                .map_err(s)?;
                b.set_quality(Quality::NearBest).map_err(s)?;
                let q = |n: u8| match n {
                    0 => Quality::Best,
                    1 => Quality::SecondBest,
                    2 => Quality::NearBest,
                    3 => Quality::VeryNice,
                    4 => Quality::Nice,
                    5 => Quality::Good,
                    6 => Quality::Decent,
                    7 => Quality::Ok,
                    8 => Quality::SecondWorst,
                    _ => Quality::Worst,
                };
                let vbr = fmt.mp3_mode == Mp3Mode::Vbr;
                if vbr {
                    b.set_vbr_mode(mp3lame_encoder::VbrMode::Mtrh).map_err(s)?;
                    b.set_vbr_quality(q(fmt.mp3_quality)).map_err(s)?;
                    b.set_to_write_vbr_tag(true).map_err(s)?;
                } else {
                    let rate = match fmt.mp3_bitrate {
                        8 => Bitrate::Kbps8,
                        16 => Bitrate::Kbps16,
                        24 => Bitrate::Kbps24,
                        32 => Bitrate::Kbps32,
                        40 => Bitrate::Kbps40,
                        48 => Bitrate::Kbps48,
                        64 => Bitrate::Kbps64,
                        80 => Bitrate::Kbps80,
                        96 => Bitrate::Kbps96,
                        112 => Bitrate::Kbps112,
                        128 => Bitrate::Kbps128,
                        160 => Bitrate::Kbps160,
                        224 => Bitrate::Kbps224,
                        256 => Bitrate::Kbps256,
                        320 => Bitrate::Kbps320,
                        _ => Bitrate::Kbps192,
                    };
                    b.set_vbr_mode(mp3lame_encoder::VbrMode::Off).map_err(s)?;
                    b.set_brate(rate).map_err(s)?;
                    b.set_to_write_vbr_tag(false).map_err(s)?;
                }
                Inner::Mp3 {
                    enc: b.build().map_err(s)?,
                    file: BufWriter::new(File::create(path).map_err(werr)?),
                    buf: Vec::new(),
                    mono: fmt.channels == 1,
                    vbr,
                }
            }
        };
        let resample = if in_rate != fmt.rate {
            Some(Resample::new(in_rate, fmt.rate, out_ch)?)
        } else {
            None
        };
        Ok(Self {
            inner,
            in_ch: usize::from(in_ch.max(1)),
            out_ch,
            mix: fmt.mix,
            resample,
            mapped: Vec::new(),
            converted: Vec::new(),
            frames: 0,
        })
    }

    /// Adds interleaved samples at the source rate and channel count.
    pub fn write(&mut self, interleaved: &[f32]) -> Result<(), EncodeError> {
        self.mapped.clear();
        let mapped: &[f32] = if self.in_ch == self.out_ch {
            interleaved
        } else if self.out_ch == 1 && self.in_ch >= 2 {
            // Stereo to mono with the chosen left and right amounts.
            let [l, r] = self.mix;
            self.mapped.extend(
                interleaved
                    .chunks_exact(self.in_ch)
                    .map(|f| f[0] * l + f[1] * r),
            );
            &self.mapped
        } else {
            map_channels(interleaved, self.in_ch, self.out_ch, &mut self.mapped);
            &self.mapped
        };
        match self.resample.as_mut() {
            None => {
                let block = mapped.to_vec();
                self.encode(&block)
            }
            Some(r) => {
                self.converted.clear();
                let mut out = std::mem::take(&mut self.converted);
                r.push(mapped, &mut out)?;
                let res = self.encode(&out);
                self.converted = out;
                res
            }
        }
    }

    fn encode(&mut self, data: &[f32]) -> Result<(), EncodeError> {
        if data.is_empty() {
            return Ok(());
        }
        self.frames += (data.len() / self.out_ch) as u64;
        match &mut self.inner {
            Inner::Aac { w, dither, buf } => {
                buf.clear();
                buf.extend(data.iter().map(|v| dither.int(*v, 16) as i16));
                w.write(buf).map_err(EncodeError::Write)?;
            }
            Inner::Adpcm { w, dither, buf } => {
                buf.clear();
                buf.extend(data.iter().map(|v| dither.int(*v, 16) as i16));
                w.write(buf).map_err(werr)?;
            }
            Inner::Wav { w, bits, dither } => {
                if *bits == 32 {
                    for v in data {
                        w.write_sample(*v).map_err(werr)?;
                    }
                } else {
                    for v in data {
                        let x = dither.int(*v, *bits);
                        if *bits == 8 {
                            w.write_sample(x as i8).map_err(werr)?;
                        } else if *bits == 16 {
                            w.write_sample(x as i16).map_err(werr)?;
                        } else {
                            w.write_sample(x).map_err(werr)?;
                        }
                    }
                }
            }
            Inner::Flac(f) => {
                for v in data {
                    let x = f.dither.int(*v, f.bits);
                    f.pending.push(x);
                }
                let block = FLAC_BLOCK * f.ch;
                let mut at = 0;
                while f.pending.len() - at >= block {
                    let chunk: Vec<i32> = f.pending[at..at + block].to_vec();
                    f.encode_block(&chunk)?;
                    at += block;
                }
                f.pending.drain(..at);
            }
            Inner::Mp3 {
                enc,
                file,
                buf,
                mono,
                ..
            } => {
                buf.clear();
                let frames = data.len() / if *mono { 1 } else { 2 };
                buf.reserve(frames * 5 / 4 + 7_200);
                let r = if *mono {
                    enc.encode_to_vec(mp3lame_encoder::MonoPcm(data), buf)
                } else {
                    enc.encode_to_vec(mp3lame_encoder::InterleavedPcm(data), buf)
                };
                r.map_err(|e| werr(format!("{e:?}")))?;
                file.write_all(buf).map_err(werr)?;
            }
        }
        Ok(())
    }

    /// Finishes the file. Returns the frames written.
    pub fn finish(mut self) -> Result<u64, EncodeError> {
        if let Some(mut r) = self.resample.take() {
            let mut out = Vec::new();
            r.finish(&mut out)?;
            self.encode(&out)?;
        }
        match self.inner {
            Inner::Aac { w, .. } => w.finish().map_err(EncodeError::Write)?,
            Inner::Adpcm { w, .. } => w.finish().map_err(werr)?,
            Inner::Wav { w, .. } => w.finalize().map_err(werr)?,
            Inner::Flac(mut f) => {
                if !f.pending.is_empty() {
                    let rest = std::mem::take(&mut f.pending);
                    f.encode_block(&rest)?;
                }
                // Fixed block size: the shorter last block doesn't count, as with the reference encoder.
                // Readers take differing sizes to mean a variable-size stream.
                f.info
                    .set_block_sizes(FLAC_BLOCK, FLAC_BLOCK)
                    .map_err(|e| werr(format!("{e:?}")))?;
                f.info.set_md5_digest(&f.ctx.md5_digest());
                let header = FlacOut::header(&f.info)?;
                f.file.seek(SeekFrom::Start(0)).map_err(werr)?;
                f.file.write_all(&header).map_err(werr)?;
                f.file.flush().map_err(werr)?;
            }
            Inner::Mp3 {
                mut enc,
                mut file,
                mut buf,
                vbr,
                ..
            } => {
                buf.clear();
                buf.reserve(7_200);
                enc.flush_to_vec::<mp3lame_encoder::FlushGap>(&mut buf)
                    .map_err(|e| werr(format!("{e:?}")))?;
                file.write_all(&buf).map_err(werr)?;
                if vbr {
                    // The first frame holds the length table players use to seek a VBR file.
                    let mut tag = Vec::with_capacity(enc.lame_tag_size().max(1));
                    if enc.lame_tag_encode_to_vec(&mut tag).is_some() {
                        file.seek(SeekFrom::Start(enc.id3v2_tag_size() as u64))
                            .map_err(werr)?;
                        file.write_all(&tag).map_err(werr)?;
                    }
                }
                file.flush().map_err(werr)?;
            }
        }
        Ok(self.frames)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codecs::{probe, AudioReader};

    fn sine(rate: u32, ch: usize, secs: f32) -> Vec<f32> {
        let n = (rate as f32 * secs) as usize;
        let mut v = Vec::with_capacity(n * ch);
        for i in 0..n {
            let x = (i as f32 * 440.0 * std::f32::consts::TAU / rate as f32).sin() * 0.5;
            for _ in 0..ch {
                v.push(x);
            }
        }
        v
    }

    fn write(path: &Path, fmt: OutputFormat, rate: u32, ch: u16, data: &[f32]) -> u64 {
        let r = fmt.resolve(rate, ch, 24).unwrap();
        let mut w = Writer::create(path, &r, rate, ch).unwrap();
        for block in data.chunks(10_000 * usize::from(ch)) {
            w.write(block).unwrap();
        }
        w.finish().unwrap()
    }

    fn read_all(path: &Path) -> (u32, usize, Vec<f32>) {
        let ch = usize::from(probe(path).unwrap().channels);
        let mut r = AudioReader::open(path, ch, None).unwrap();
        let mut out = Vec::new();
        while r.read_into(&mut out).unwrap() {}
        (r.out_rate(), r.out_channels(), out)
    }

    #[test]
    fn flac_round_trips_exactly_enough() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.flac");
        let data = sine(48_000, 2, 1.3);
        let fmt = OutputFormat {
            kind: FormatKind::Flac,
            bit_depth: Some(24),
            ..Default::default()
        };
        assert_eq!(write(&p, fmt, 48_000, 2, &data), (data.len() / 2) as u64);
        let info = probe(&p).unwrap();
        assert_eq!(info.sample_rate, 48_000);
        let (rate, ch, back) = read_all(&p);
        assert_eq!((rate, ch, back.len()), (48_000, 2, data.len()));
        let err = data
            .iter()
            .zip(&back)
            .map(|(a, b)| (a - b).abs())
            .fold(0f32, f32::max);
        assert!(err < 1e-5, "max error {err}");
    }

    #[test]
    fn mp3_writes_a_playable_file_of_the_right_length() {
        let dir = tempfile::tempdir().unwrap();
        for mode in [Mp3Mode::Cbr, Mp3Mode::Vbr] {
            let p = dir.path().join(format!("{mode:?}.mp3"));
            let data = sine(44_100, 1, 2.0);
            let fmt = OutputFormat {
                kind: FormatKind::Mp3,
                mp3_mode: mode,
                channels: Some(2),
                ..Default::default()
            };
            write(&p, fmt, 44_100, 1, &data);
            let (rate, ch, back) = read_all(&p);
            assert_eq!((rate, ch), (44_100, 2));
            let frames = back.len() / 2;
            // MP3 adds a little padding at each end.
            assert!(
                (88_200..88_200 + 4_000).contains(&frames),
                "{mode:?}: {frames} frames"
            );
            let peak = back.iter().fold(0f32, |m, v| m.max(v.abs()));
            assert!((0.4..0.6).contains(&peak), "{mode:?}: peak {peak}");
        }
    }

    #[test]
    fn converts_rate_and_channels_with_exact_length() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("b.wav");
        let data = sine(48_000, 2, 1.0);
        let fmt = OutputFormat {
            kind: FormatKind::Wav,
            sample_rate: Some(44_100),
            channels: Some(1),
            bit_depth: Some(16),
            ..Default::default()
        };
        assert_eq!(write(&p, fmt, 48_000, 2, &data), 44_100);
        let r = hound::WavReader::open(&p).unwrap();
        assert_eq!(
            (
                r.spec().sample_rate,
                r.spec().channels,
                r.spec().bits_per_sample,
                r.len()
            ),
            (44_100, 1, 16, 44_100)
        );
    }

    #[test]
    fn resolves_settings() {
        let mp3 = OutputFormat {
            kind: FormatKind::Mp3,
            ..Default::default()
        }
        .resolve(96_000, 6, 24)
        .unwrap();
        assert_eq!((mp3.rate, mp3.channels), (48_000, 2));
        assert_eq!(mp3.describe(), "MP3 192 kbps CBR, 48000 Hz stereo");
        assert!(OutputFormat {
            kind: FormatKind::Flac,
            bit_depth: Some(32),
            ..Default::default()
        }
        .resolve(48_000, 2, 24)
        .is_err());
        assert!(OutputFormat {
            kind: FormatKind::Mp3,
            mp3_bitrate: 100,
            ..Default::default()
        }
        .resolve(48_000, 2, 24)
        .is_err());
        let w = OutputFormat::default().resolve(44_100, 1, 32).unwrap();
        assert_eq!(w.describe(), "WAV 32-bit float, 44100 Hz mono");
    }

    #[test]
    fn eight_bit_and_adpcm_wav_read_back() {
        let dir = tempfile::tempdir().unwrap();
        let data = sine(44_100, 2, 1.0);
        for bits in [8u16, 4] {
            let path = dir.path().join(format!("t{bits}.wav"));
            let fmt = OutputFormat {
                kind: FormatKind::Wav,
                bit_depth: Some(bits),
                ..Default::default()
            };
            let frames = write(&path, fmt, 44_100, 2, &data);
            assert_eq!(frames, 44_100);
            let (rate, ch, back) = read_all(&path);
            assert_eq!((rate, ch), (44_100, 2));
            // ADPCM pads its last block; the decoder may return the padding.
            let n = back.len() / 2;
            assert!(
                (44_100..44_100 + 2_100).contains(&n),
                "{bits}-bit: {n} frames"
            );
            let err = data
                .iter()
                .zip(&back)
                .skip(2_000)
                .map(|(a, b)| (a - b).abs())
                .fold(0.0f32, f32::max);
            assert!(err < 0.05, "{bits}-bit: worst error {err}");
        }
    }
}

#[cfg(test)]
mod aac_tests {
    use super::*;

    #[test]
    fn aac_settings_resolve_to_what_the_encoder_takes() {
        let fmt = OutputFormat {
            kind: FormatKind::M4a,
            aac_bitrate: 256,
            ..Default::default()
        };
        let r = fmt.resolve(96_000, 6, 24);
        if super::super::aac::AVAILABLE {
            let r = r.unwrap();
            assert_eq!((r.rate, r.channels, r.aac_bitrate), (48_000, 2, 192));
            assert_eq!(r.describe(), "AAC 192 kbps, 48000 Hz stereo");
        } else {
            assert!(r.is_err(), "says plainly that AAC is not available here");
        }
        assert_eq!(FormatKind::from_extension("M4A"), Some(FormatKind::M4a));
    }
}
