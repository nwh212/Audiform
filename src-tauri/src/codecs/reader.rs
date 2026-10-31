// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Streams decoded audio as interleaved f32, optionally converted to a
//! different channel count and sample rate. Used by preview playback
//! now and by the peak cache and engine later.
//!
//! Never call this from the real-time audio thread: it reads files and
//! allocates. Run it on a worker thread and hand samples over through a
//! lock-free queue.

use std::path::Path;

use audioadapter_buffers::owned::InterleavedOwned;
use rubato::{Async, FixedAsync, Resampler, SincInterpolationParameters};
use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo, TrackType};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::units::Time;

use super::probe::ProbeError;

/// Frames fed to the resampler per call.
const RESAMPLE_CHUNK: usize = 1024;

/// Where decoded samples come from.
enum Backend {
    Symphonia {
        format: Box<dyn FormatReader>,
        decoder: Box<dyn AudioDecoder>,
        track_id: u32,
    },
    /// A DTS stream (in a WAV or a raw `.dts` file), decoded here.
    Dts(Box<super::dts::DtsStream>),
    /// A decoder built into the operating system, for what Symphonia
    /// cannot read (HE-AAC, Dolby Digital and Dolby Digital Plus).
    Os(audiform_os_decode::OsReader),
}

pub struct AudioReader {
    backend: Backend,
    src_rate: u32,
    src_channels: usize,
    out_channels: usize,
    out_rate: u32,
    resampler: Option<Async<f32>>,
    /// Leading resampler output still to drop, in samples. The
    /// resampler delays its output a little; dropping that keeps
    /// resampled files lined up with files played at their own rate.
    skip: usize,
    /// Decoded source frames to drop after a seek, so playback starts at
    /// exactly the requested time rather than the start of a packet.
    skip_src: usize,
    /// Decoded, channel-mapped samples waiting for the resampler.
    pending: Vec<f32>,
    scratch: Vec<f32>,
    /// Source Channels: `[left, right]` per source channel, mixing to stereo
    /// instead of the plain channel mapping.
    mix: Option<Vec<[f32; 2]>>,
    ended: bool,
    flushed: bool,
}

impl AudioReader {
    /// Mixes the file's channels to stereo by these amounts (one
    /// `[left, right]` per source channel). Only for a reader opened for
    /// two channels; call before reading.
    pub fn set_mix(&mut self, gains: Vec<[f32; 2]>) {
        self.mix = Some(gains);
        // A format that only revealed its channels by decoding has one
        // packet mapped the plain way already: read it again.
        if !self.pending.is_empty() {
            let _ = self.seek(0.0);
        }
    }

    /// Changes the channel mix while playing (None maps the channels the
    /// plain way). The packet already decoded keeps the old mix.
    pub fn change_mix(&mut self, gains: Option<Vec<[f32; 2]>>) {
        self.mix = gains;
    }

    /// Opens a file. `out_rate` of None keeps the file's own rate.
    pub fn open(
        path: &Path,
        out_channels: usize,
        out_rate: Option<u32>,
    ) -> Result<Self, ProbeError> {
        let (backend, src_rate, src_channels) = if let Some(dts) = super::dts::sniff(path) {
            let (rate, ch) = (dts.sample_rate, dts.channels());
            (
                Backend::Dts(Box::new(super::dts::DtsStream::open(path, dts)?)),
                rate,
                ch,
            )
        } else {
            match Self::open_symphonia(path).map_err(|e| super::probe::name_hidden_codec(path, e)) {
                Ok(b) => b,
                Err(e) => match Self::open_os(path, &e) {
                    Some(b) => b,
                    None => return Err(e),
                },
            }
        };
        match Self::finish(backend, src_rate, src_channels, out_channels, out_rate) {
            Ok(r) => Ok(r),
            // A codec variant Symphonia only refuses once it decodes (such
            // as HE-AAC that signals itself inside the stream).
            Err(e) => match Self::open_os(path, &e) {
                Some((b, rate, ch)) => Self::finish(b, rate, ch, out_channels, out_rate),
                None => Err(e),
            },
        }
    }

    /// Opens a file through the system's decoders after `why` kept the
    /// app's own from reading it. None where that cannot help.
    fn open_os(path: &Path, why: &ProbeError) -> Option<(Backend, u32, usize)> {
        if !audiform_os_decode::AVAILABLE
            || !matches!(
                why,
                ProbeError::UnsupportedCodec(_) | ProbeError::UnknownFormat
            )
        {
            return None;
        }
        let os = audiform_os_decode::OsReader::open(path).ok()?;
        let (rate, ch) = (os.info().sample_rate, os.info().channels);
        Some((Backend::Os(os), rate, ch))
    }

    fn finish(
        backend: Backend,
        src_rate: u32,
        src_channels: usize,
        out_channels: usize,
        out_rate: Option<u32>,
    ) -> Result<Self, ProbeError> {
        let mut reader = Self {
            backend,
            src_rate,
            src_channels,
            out_channels: out_channels.max(1),
            out_rate: 0,
            resampler: None,
            pending: Vec::new(),
            scratch: Vec::new(),
            mix: None,
            ended: false,
            flushed: false,
            skip: 0,
            skip_src: 0,
        };
        reader.finish_open(out_rate)?;
        Ok(reader)
    }

    /// Opens a file through Symphonia: its backend, rate and channels
    /// (0 when only the first decode will tell).
    fn open_symphonia(path: &Path) -> Result<(Backend, u32, usize), ProbeError> {
        let (mss, ext) = super::source::open(path)?;
        let mut hint = Hint::new();
        if let Some(ext) = ext.as_deref() {
            hint.with_extension(ext);
        }
        let format = symphonia::default::get_probe()
            .probe(
                &hint,
                mss,
                FormatOptions::default(),
                MetadataOptions::default(),
            )
            .map_err(|_| ProbeError::UnknownFormat)?;
        let track = format
            .default_track(TrackType::Audio)
            .ok_or(ProbeError::NoAudio)?;
        let track_id = track.id;
        let params = track
            .codec_params
            .as_ref()
            .and_then(|p| p.audio())
            .cloned()
            .ok_or(ProbeError::NoAudio)?;
        let decoder = super::opus::codecs()
            .make_audio_decoder(&params, &AudioDecoderOptions::default())
            .map_err(|e| ProbeError::UnsupportedCodec(e.to_string()))?;

        Ok((
            Backend::Symphonia {
                format,
                decoder,
                track_id,
            },
            params.sample_rate.unwrap_or(0),
            params.channels.as_ref().map(|c| c.count()).unwrap_or(0),
        ))
    }

    fn finish_open(&mut self, out_rate: Option<u32>) -> Result<(), ProbeError> {
        let reader = self;
        // Some formats only reveal rate and channels after the first decode,
        // and some codec variants are only refused then: decode the first
        // packet now, so a refusal shows here and not mid-playback.
        reader.decode_next()?;
        if reader.src_rate == 0 || reader.src_channels == 0 {
            return Err(ProbeError::NoAudio);
        }
        reader.out_rate = out_rate.unwrap_or(reader.src_rate);
        if reader.out_rate != reader.src_rate {
            let ratio = f64::from(reader.out_rate) / f64::from(reader.src_rate);
            let resampler = Async::<f32>::new_sinc(
                ratio,
                1.0,
                &SincInterpolationParameters::default(),
                RESAMPLE_CHUNK,
                reader.out_channels,
                FixedAsync::Input,
            )
            .map_err(|e| ProbeError::Damaged(e.to_string()))?;
            reader.skip = resampler.output_delay() * reader.out_channels;
            reader.resampler = Some(resampler);
        }
        Ok(())
    }

    pub fn source_rate(&self) -> u32 {
        self.src_rate
    }

    pub fn source_channels(&self) -> usize {
        self.src_channels
    }

    pub fn out_rate(&self) -> u32 {
        self.out_rate
    }

    pub fn out_channels(&self) -> usize {
        self.out_channels
    }

    /// Moves to a time in seconds. Returns the time actually reached.
    pub fn seek(&mut self, seconds: f64) -> Result<f64, ProbeError> {
        let (format, decoder, track_id) = match &mut self.backend {
            Backend::Symphonia {
                format,
                decoder,
                track_id,
            } => (format, decoder, *track_id),
            Backend::Os(os) => {
                let rate = f64::from(self.src_rate);
                let target = (seconds.max(0.0) * rate).round();
                os.seek(target as u64).map_err(ProbeError::Damaged)?;
                if let Some(r) = self.resampler.as_mut() {
                    r.reset();
                    self.skip = r.output_delay() * self.out_channels;
                }
                self.pending.clear();
                self.ended = false;
                self.flushed = false;
                self.skip_src = 0;
                return Ok(target / rate.max(1.0));
            }
            Backend::Dts(d) => {
                let rate = f64::from(self.src_rate);
                let target = (seconds.max(0.0) * rate).round();
                let landed = d.seek(target as u64) as f64;
                if let Some(r) = self.resampler.as_mut() {
                    r.reset();
                    self.skip = r.output_delay() * self.out_channels;
                }
                self.pending.clear();
                self.ended = false;
                self.flushed = false;
                self.skip_src = (target - landed).max(0.0) as usize;
                return Ok((landed + self.skip_src as f64) / rate.max(1.0));
            }
        };
        let time = Time::try_from_secs_f64(seconds.max(0.0)).unwrap_or(Time::from_nanos(0));
        let seeked = format
            .seek(
                SeekMode::Accurate,
                SeekTo::Time {
                    time,
                    track_id: Some(track_id),
                },
            )
            .map_err(|e| ProbeError::Damaged(e.to_string()))?;
        decoder.reset();
        if let Some(r) = self.resampler.as_mut() {
            r.reset();
            self.skip = r.output_delay() * self.out_channels;
        }
        self.pending.clear();
        self.skip_src = 0;
        self.ended = false;
        self.flushed = false;
        let tb = format
            .tracks()
            .iter()
            .find(|t| t.id == track_id)
            .and_then(|t| t.time_base);
        let secs = |ts| tb.and_then(|tb| tb.calc_time(ts)).map(|t| t.as_secs_f64());
        let actual = secs(seeked.actual_ts).unwrap_or(seconds);
        // Count in whole frames so the result is exact, not rounded twice.
        let rate = f64::from(self.src_rate);
        let target = (seconds.max(0.0) * rate).round();
        let landed = (actual * rate).round();
        self.skip_src = (target - landed).max(0.0) as usize;
        Ok((landed + self.skip_src as f64) / rate.max(1.0))
    }

    /// Appends the next block of interleaved samples to `out`. Returns
    /// false once the file is finished and nothing was added.
    pub fn read_into(&mut self, out: &mut Vec<f32>) -> Result<bool, ProbeError> {
        loop {
            let Some(resampler) = self.resampler.as_mut() else {
                // No rate change: hand over decoded samples directly.
                if !self.pending.is_empty() {
                    out.append(&mut self.pending);
                    return Ok(true);
                }
                if self.ended {
                    return Ok(false);
                }
                self.decode_next()?;
                continue;
            };

            let need = resampler.input_frames_next() * self.out_channels;
            if self.pending.len() >= need {
                let chunk: Vec<f32> = self.pending.drain(..need).collect();
                let frames = need / self.out_channels;
                let input = InterleavedOwned::new_from(chunk, self.out_channels, frames)
                    .map_err(|e| ProbeError::Damaged(e.to_string()))?;
                let result = resampler
                    .process(&input, None)
                    .map_err(|e| ProbeError::Damaged(e.to_string()))?;
                let data = result.take_data();
                let drop = self.skip.min(data.len());
                self.skip -= drop;
                if drop == data.len() {
                    continue;
                }
                out.extend_from_slice(&data[drop..]);
                return Ok(true);
            }
            if self.ended {
                if self.flushed {
                    return Ok(false);
                }
                // Pad the tail with silence so the last real samples
                // make it through the resampler's delay line.
                let have = self.pending.len() / self.out_channels;
                let pad = (need / self.out_channels).saturating_sub(have) + 2 * RESAMPLE_CHUNK;
                self.pending
                    .resize(self.pending.len() + pad * self.out_channels, 0.0);
                self.flushed = true;
                continue;
            }
            self.decode_next()?;
        }
    }

    /// Decodes one packet into `pending`, mapped to the output channels.
    fn decode_next(&mut self) -> Result<(), ProbeError> {
        let (format, decoder, track_id) = match &mut self.backend {
            Backend::Symphonia {
                format,
                decoder,
                track_id,
            } => (format, decoder, *track_id),
            Backend::Os(os) => {
                self.scratch.clear();
                if !os.read(&mut self.scratch).map_err(ProbeError::Damaged)? {
                    self.ended = true;
                    return Ok(());
                }
                let channels = self.src_channels.max(1);
                self.push_scratch(channels);
                return Ok(());
            }
            Backend::Dts(d) => {
                self.scratch.clear();
                loop {
                    if !d.next_into(&mut self.scratch)? {
                        self.ended = true;
                        return Ok(());
                    }
                    let channels = self.src_channels.max(1);
                    if self.skip_src > 0 {
                        let frames = self.scratch.len() / channels;
                        let drop = self.skip_src.min(frames);
                        self.skip_src -= drop;
                        self.scratch.drain(..drop * channels);
                        if self.scratch.is_empty() {
                            continue;
                        }
                    }
                    self.push_scratch(channels);
                    return Ok(());
                }
            }
        };
        loop {
            let packet = match format.next_packet() {
                Ok(Some(p)) => p,
                Ok(None) => {
                    self.ended = true;
                    return Ok(());
                }
                Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    self.ended = true;
                    return Ok(());
                }
                Err(SymError::ResetRequired) => {
                    decoder.reset();
                    continue;
                }
                Err(e) => return Err(ProbeError::Damaged(e.to_string())),
            };
            if packet.track_id != track_id {
                continue;
            }
            let buf = match decoder.decode(&packet) {
                Ok(b) => b,
                Err(SymError::DecodeError(_)) => continue,
                Err(SymError::Unsupported(what)) => {
                    return Err(ProbeError::UnsupportedCodec(what.to_string()))
                }
                Err(e) => return Err(ProbeError::Damaged(e.to_string())),
            };
            let spec = buf.spec();
            if self.src_rate == 0 {
                self.src_rate = spec.rate();
            }
            let channels = spec.channels().count();
            if self.src_channels == 0 {
                self.src_channels = channels;
            }
            buf.copy_to_vec_interleaved(&mut self.scratch);
            if self.skip_src > 0 && channels > 0 {
                let frames = self.scratch.len() / channels;
                let drop = self.skip_src.min(frames);
                self.skip_src -= drop;
                self.scratch.drain(..drop * channels);
                if self.scratch.is_empty() {
                    continue;
                }
            }
            self.push_scratch(channels);
            return Ok(());
        }
    }

    /// Maps the decoded samples in `scratch` to the output channels.
    fn push_scratch(&mut self, channels: usize) {
        match self.mix.as_deref() {
            Some(g) if self.out_channels == 2 => {
                crate::dsp::channels::mix_into(&self.scratch, channels, g, &mut self.pending);
            }
            _ => map_channels(
                &self.scratch,
                channels,
                self.out_channels,
                &mut self.pending,
            ),
        }
    }
}

/// Converts interleaved samples between channel counts. Mono feeds both
/// sides of a stereo output; stereo folds to mono by averaging; extra
/// source channels are dropped; extra output channels stay silent.
pub fn map_channels(input: &[f32], in_ch: usize, out_ch: usize, out: &mut Vec<f32>) {
    if in_ch == 0 {
        return;
    }
    let frames = input.len() / in_ch;
    out.reserve(frames * out_ch);
    for f in input.chunks_exact(in_ch) {
        match (in_ch, out_ch) {
            (a, b) if a == b => out.extend_from_slice(f),
            (1, _) => {
                for c in 0..out_ch {
                    out.push(if c < 2 { f[0] } else { 0.0 });
                }
            }
            (_, 1) => out.push((f[0] + f[1]) * 0.5),
            _ => {
                for c in 0..out_ch {
                    out.push(f.get(c).copied().unwrap_or(0.0));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_channels() {
        let mut out = Vec::new();
        map_channels(&[0.5, -0.5], 1, 2, &mut out);
        assert_eq!(out, [0.5, 0.5, -0.5, -0.5]);

        out.clear();
        map_channels(&[1.0, 0.0, 0.2, 0.4], 2, 1, &mut out);
        assert_eq!(out, [0.5, 0.3]);

        out.clear();
        map_channels(&[1.0, 2.0, 3.0], 3, 2, &mut out);
        assert_eq!(out, [1.0, 2.0]);

        out.clear();
        map_channels(&[1.0, 2.0], 2, 4, &mut out);
        assert_eq!(out, [1.0, 2.0, 0.0, 0.0]);

        out.clear();
        map_channels(&[0.7], 1, 4, &mut out);
        assert_eq!(out, [0.7, 0.7, 0.0, 0.0]);
    }

    #[test]
    fn mixes_a_surround_file_to_stereo() {
        // Six channels, each holding its own steady value.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("surround.wav");
        let spec = hound::WavSpec {
            channels: 6,
            sample_rate: 48_000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut w = hound::WavWriter::create(&path, spec).unwrap();
        for _ in 0..4_800 {
            for c in 0..6 {
                w.write_sample(0.1 * (c + 1) as f32).unwrap();
            }
        }
        w.finalize().unwrap();
        let mut r = AudioReader::open(&path, 2, None).unwrap();
        // Ls only, to both sides at half.
        let mut g = vec![[0.0f32; 2]; 6];
        g[4] = [0.5, 0.5];
        r.set_mix(g);
        let mut out = Vec::new();
        assert!(r.read_into(&mut out).unwrap());
        assert!((out[0] - 0.25).abs() < 1e-5 && (out[1] - 0.25).abs() < 1e-5);
    }
}
