// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.
//
// A port to Rust of the DTS Coherent Acoustics core decoder in FFmpeg's
// libavcodec (dca_core.c, dcadsp.c, dcadct.c, synth_filter.c;
// Copyright (C) 2016 foo86, LGPL-2.1-or-later), fixed-point path only,
// without the 96 kHz and extended channel set extensions. See
// ATTRIBUTION.md.

//! The DTS core: up to 5.1 channels (plus the 6.1 back center from the
//! XCh extension), 16 to 24 bits, up to 48 kHz. This is what DTS music
//! CDs, DTS WAV files and the core of every DTS stream carry.

// Ported numeric code keeps FFmpeg's indexed loops so it reads side by side.
#![allow(clippy::needless_range_loop)]

use std::sync::OnceLock;

use super::tables::*;

const CHANNELS: usize = 7;
const SUBBANDS: usize = 32;
const SUBFRAMES: usize = 16;
const SUBBAND_SAMPLES: usize = 8;
pub const PCMBLOCK_SAMPLES: usize = 32;
const LFE_HISTORY: usize = 8;
const ABITS_MAX: i32 = 26;
const ADPCM_COEFFS: usize = 4;
const CODE_BOOKS: usize = 10;

const SYNC_CORE: u32 = 0x7FFE_8001;
const SYNC_XCH: u32 = 0x5A5A_5A5A;
const SYNC_REV1AUX: u32 = 0x9A11_05A0;

const SAMPLE_RATES: [u32; 16] = [
    0, 8000, 16000, 32000, 0, 0, 11025, 22050, 44100, 0, 0, 12000, 24000, 48000, 96000, 192000,
];
const BITS_PER_SAMPLE: [u8; 8] = [16, 16, 20, 20, 0, 24, 24, 0];
const BLOCK_CODE_NBITS: [u32; 7] = [7, 10, 12, 13, 15, 17, 19];

// Speakers, in DTS numbering.
pub const SPK_C: usize = 0;
pub const SPK_L: usize = 1;
pub const SPK_R: usize = 2;
pub const SPK_LS: usize = 3;
pub const SPK_RS: usize = 4;
pub const SPK_LFE: usize = 5;
pub const SPK_CS: usize = 6;
pub const SPEAKERS: usize = 7;

/// Each audio mode's primary channels, as speakers.
const PRM_CH_TO_SPKR: [&[usize]; 10] = [
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
const AMODE_STEREO_SUMDIFF: usize = 3;
const AMODE_2F2R: usize = 8;

/// Why a frame could not be decoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DtsError(pub &'static str);

impl std::fmt::Display for DtsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

type Res<T> = Result<T, DtsError>;

fn bad<T>(why: &'static str) -> Res<T> {
    Err(DtsError(why))
}

// ---- Fixed-point helpers (dcamath.h) ----

#[inline]
fn norm(a: i64, bits: i32) -> i32 {
    if bits > 0 {
        ((a + (1i64 << (bits - 1))) >> bits) as i32
    } else {
        a as i32
    }
}

#[inline]
fn mul(a: i32, b: i32, bits: i32) -> i32 {
    norm(i64::from(a) * i64::from(b), bits)
}

#[inline]
fn clip23(a: i32) -> i32 {
    a.clamp(-(1 << 23), (1 << 23) - 1)
}

// ---- Bit reader (MSB first; reading past the end gives zeros) ----

pub struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Bits<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn bit(&self, at: usize) -> u32 {
        self.data
            .get(at / 8)
            .map_or(0, |b| u32::from(b >> (7 - at % 8)) & 1)
    }

    pub fn get(&mut self, n: u32) -> u32 {
        let mut v = 0u32;
        for _ in 0..n {
            v = (v << 1) | self.bit(self.pos);
            self.pos += 1;
        }
        v
    }

    fn get_signed(&mut self, n: u32) -> i32 {
        if n == 0 {
            return 0;
        }
        let v = self.get(n);
        ((v << (32 - n)) as i32) >> (32 - n)
    }

    fn skip(&mut self, n: usize) {
        self.pos += n;
    }

    fn count(&self) -> usize {
        self.pos
    }

    fn left(&self) -> isize {
        (self.data.len() * 8) as isize - self.pos as isize
    }
}

// ---- Huffman codes ----

/// A code tree: each node holds its two children; a leaf is `!symbol`.
struct Vlc {
    nodes: Vec<[i32; 2]>,
}

impl Vlc {
    /// Builds from (symbol, length) pairs listed in code order, as FFmpeg
    /// lists them; `offset` is added to each symbol.
    fn new(src: &[(u8, u8)], offset: i32) -> Self {
        let mut nodes = vec![[0i32; 2]];
        let mut code: u64 = 0;
        for &(sym, len) in src {
            let len = u32::from(len);
            let mut node = 0usize;
            for b in (0..len).rev() {
                let bit = ((code >> (32 - len + b)) & 1) as usize;
                if b == 0 {
                    nodes[node][bit] = !(i32::from(sym) + offset + 1024);
                } else {
                    if nodes[node][bit] <= 0 {
                        nodes.push([0, 0]);
                        nodes[node][bit] = (nodes.len() - 1) as i32;
                    }
                    node = nodes[node][bit] as usize;
                }
            }
            code += 1u64 << (32 - len);
        }
        Self { nodes }
    }

    fn read(&self, bits: &mut Bits) -> Res<i32> {
        let mut node = 0usize;
        for _ in 0..32 {
            let next = self.nodes[node][bits.get(1) as usize];
            if next < 0 {
                return Ok(!next - 1024);
            }
            if next == 0 {
                return bad("an invalid Huffman code");
            }
            node = next as usize;
        }
        bad("an invalid Huffman code")
    }
}

struct Vlcs {
    quant_index: Vec<Vec<Vlc>>,
    bit_allocation: Vec<Vlc>,
    scale_factor: Vec<Vlc>,
    transition_mode: Vec<Vlc>,
}

const BITALLOC_SIZES: [usize; CODE_BOOKS] = [3, 5, 7, 9, 13, 17, 25, 33, 65, 129];
const BITALLOC_OFFSETS: [i32; CODE_BOOKS] = [-1, -2, -3, -4, -6, -8, -12, -16, -32, -64];

fn vlcs() -> &'static Vlcs {
    static V: OnceLock<Vlcs> = OnceLock::new();
    V.get_or_init(|| {
        let mut at = 0usize;
        let mut take = |n: usize, offset: i32| {
            let v = Vlc::new(&VLC_SRC[at..at + n], offset);
            at += n;
            v
        };
        let quant_index = (0..CODE_BOOKS)
            .map(|i| {
                (0..usize::from(QUANT_INDEX_GROUP_SIZE[i]))
                    .map(|_| take(BITALLOC_SIZES[i], BITALLOC_OFFSETS[i]))
                    .collect()
            })
            .collect();
        let bit_allocation = (0..5).map(|_| take(12, 1)).collect();
        let scale_factor = (0..5).map(|_| take(129, -64)).collect();
        let transition_mode = (0..4).map(|_| take(4, 0)).collect();
        Vlcs {
            quant_index,
            bit_allocation,
            scale_factor,
            transition_mode,
        }
    })
}

// ---- Frame header ----

/// What a core frame header says.
#[derive(Debug, Clone, Copy)]
pub struct Header {
    pub crc_present: bool,
    pub npcmblocks: usize,
    pub frame_size: usize,
    pub audio_mode: usize,
    pub sample_rate: u32,
    pub bit_rate: u32,
    pub drc_present: bool,
    pub ts_present: bool,
    pub aux_present: bool,
    pub ext_audio_type: u32,
    pub ext_audio_present: bool,
    pub sync_ssf: bool,
    pub lfe_present: u32,
    pub predictor_history: bool,
    pub filter_perfect: bool,
    pub es_format: bool,
    pub sumdiff_front: bool,
    pub sumdiff_surround: bool,
}

impl Header {
    /// PCM samples per channel in a frame.
    pub fn samples(&self) -> usize {
        self.npcmblocks * PCMBLOCK_SAMPLES
    }
}

/// Reads a core frame header (big-endian, 16-bit words).
pub fn parse_header(b: &mut Bits) -> Res<Header> {
    if b.get(32) != SYNC_CORE {
        return bad("no DTS sync word");
    }
    let _normal_frame = b.get(1);
    if b.get(5) + 1 != PCMBLOCK_SAMPLES as u32 {
        return bad("deficit samples are not supported");
    }
    let crc_present = b.get(1) == 1;
    let npcmblocks = b.get(7) as usize + 1;
    if !npcmblocks.is_multiple_of(SUBBAND_SAMPLES) {
        return bad("an unsupported number of PCM blocks");
    }
    let frame_size = b.get(14) as usize + 1;
    if frame_size < 96 {
        return bad("a core frame that is too small");
    }
    let audio_mode = b.get(6) as usize;
    if audio_mode >= PRM_CH_TO_SPKR.len() {
        return bad("an unsupported channel arrangement");
    }
    let sample_rate = SAMPLE_RATES[b.get(4) as usize];
    if sample_rate == 0 {
        return bad("an invalid sample rate");
    }
    let bit_rate = BIT_RATES[b.get(5) as usize];
    if b.get(1) == 1 {
        return bad("a reserved bit is set");
    }
    let drc_present = b.get(1) == 1;
    let ts_present = b.get(1) == 1;
    let aux_present = b.get(1) == 1;
    let _hdcd = b.get(1);
    let ext_audio_type = b.get(3);
    let ext_audio_present = b.get(1) == 1;
    let sync_ssf = b.get(1) == 1;
    let lfe_present = b.get(2);
    if lfe_present == 3 {
        return bad("an invalid LFE flag");
    }
    let predictor_history = b.get(1) == 1;
    if crc_present {
        b.skip(16);
    }
    let filter_perfect = b.get(1) == 1;
    let _encoder_rev = b.get(4);
    let _copy_hist = b.get(2);
    let pcmr_code = b.get(3) as usize;
    if BITS_PER_SAMPLE[pcmr_code] == 0 {
        return bad("an invalid source resolution");
    }
    let sumdiff_front = b.get(1) == 1;
    let sumdiff_surround = b.get(1) == 1;
    let _dn_code = b.get(4);
    Ok(Header {
        crc_present,
        npcmblocks,
        frame_size,
        audio_mode,
        sample_rate,
        bit_rate,
        drc_present,
        ts_present,
        aux_present,
        ext_audio_type,
        ext_audio_present,
        sync_ssf,
        lfe_present,
        predictor_history,
        filter_perfect,
        es_format: pcmr_code & 1 == 1,
        sumdiff_front,
        sumdiff_surround,
    })
}

// ---- The decoder ----

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Core,
    Xch,
}

/// One channel's synthesis filter memory.
#[derive(Clone)]
struct Synth {
    hist1: Vec<i32>,
    offset: usize,
    hist2: [i32; 32],
}

impl Default for Synth {
    fn default() -> Self {
        Self {
            hist1: vec![0; 512],
            offset: 0,
            hist2: [0; 32],
        }
    }
}

/// A decoded frame: one buffer per speaker present (`None` if absent), in
/// DTS speaker numbering, as 24-bit values (full scale is 2^23).
pub struct Decoded {
    pub speakers: [Option<Vec<i32>>; SPEAKERS],
}

pub struct CoreDecoder {
    h: Header,
    nsubframes: usize,
    nchannels: usize,
    has_xch: bool,
    nsubbands: [usize; CHANNELS],
    subband_vq_start: [usize; CHANNELS],
    joint_intensity_index: [usize; CHANNELS],
    transition_mode_sel: [usize; CHANNELS],
    scale_factor_sel: [usize; CHANNELS],
    bit_allocation_sel: [usize; CHANNELS],
    quant_index_sel: [[usize; CODE_BOOKS]; CHANNELS],
    scale_factor_adj: [[i32; CODE_BOOKS]; CHANNELS],

    nsubsubframes: [usize; SUBFRAMES],
    prediction_mode: [[bool; SUBBANDS]; CHANNELS],
    prediction_vq_index: [[usize; SUBBANDS]; CHANNELS],
    bit_allocation: [[i32; SUBBANDS]; CHANNELS],
    transition_mode: [[[usize; SUBBANDS]; CHANNELS]; SUBFRAMES],
    scale_factors: [[[i32; 2]; SUBBANDS]; CHANNELS],
    joint_scale_sel: [usize; CHANNELS],
    joint_scale_factors: [[i32; SUBBANDS]; CHANNELS],

    /// Per channel and subband: 4 samples of ADPCM history, then the frame.
    subband: Vec<Vec<Vec<i32>>>,
    /// LFE history (8), then this frame's decimated LFE samples.
    lfe: Vec<i32>,
    synth: Vec<Synth>,
}

impl Default for CoreDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl CoreDecoder {
    pub fn new() -> Self {
        Self {
            h: Header {
                crc_present: false,
                npcmblocks: 0,
                frame_size: 0,
                audio_mode: 0,
                sample_rate: 0,
                bit_rate: 0,
                drc_present: false,
                ts_present: false,
                aux_present: false,
                ext_audio_type: 0,
                ext_audio_present: false,
                sync_ssf: false,
                lfe_present: 0,
                predictor_history: false,
                filter_perfect: false,
                es_format: false,
                sumdiff_front: false,
                sumdiff_surround: false,
            },
            nsubframes: 0,
            nchannels: 0,
            has_xch: false,
            nsubbands: [0; CHANNELS],
            subband_vq_start: [0; CHANNELS],
            joint_intensity_index: [0; CHANNELS],
            transition_mode_sel: [0; CHANNELS],
            scale_factor_sel: [0; CHANNELS],
            bit_allocation_sel: [0; CHANNELS],
            quant_index_sel: [[0; CODE_BOOKS]; CHANNELS],
            scale_factor_adj: [[0; CODE_BOOKS]; CHANNELS],
            nsubsubframes: [0; SUBFRAMES],
            prediction_mode: [[false; SUBBANDS]; CHANNELS],
            prediction_vq_index: [[0; SUBBANDS]; CHANNELS],
            bit_allocation: [[0; SUBBANDS]; CHANNELS],
            transition_mode: [[[0; SUBBANDS]; CHANNELS]; SUBFRAMES],
            scale_factors: [[[0; 2]; SUBBANDS]; CHANNELS],
            joint_scale_sel: [0; CHANNELS],
            joint_scale_factors: [[0; SUBBANDS]; CHANNELS],
            subband: vec![vec![vec![0; ADPCM_COEFFS]; SUBBANDS]; CHANNELS],
            lfe: vec![0; LFE_HISTORY],
            synth: vec![Synth::default(); CHANNELS],
        }
    }

    /// Forgets all history, as after a seek.
    pub fn reset(&mut self) {
        for ch in &mut self.subband {
            for band in ch.iter_mut() {
                band.iter_mut().for_each(|v| *v = 0);
            }
        }
        self.lfe.iter_mut().for_each(|v| *v = 0);
        self.synth = vec![Synth::default(); CHANNELS];
    }

    /// Decodes one core frame (big-endian 16-bit words, from its sync word).
    pub fn decode(&mut self, data: &[u8]) -> Res<Decoded> {
        let mut b = Bits::new(data);
        self.h = parse_header(&mut b)?;
        self.has_xch = false;
        let n = self.h.npcmblocks;
        // Size the sample buffers, keeping the ADPCM history.
        for ch in &mut self.subband {
            for band in ch.iter_mut() {
                band.resize(ADPCM_COEFFS + n, 0);
            }
        }
        if !self.h.predictor_history {
            for ch in &mut self.subband {
                for band in ch.iter_mut() {
                    band[..ADPCM_COEFFS].iter_mut().for_each(|v| *v = 0);
                }
            }
        }
        self.lfe.resize(LFE_HISTORY + n / 2, 0);

        self.parse_frame_data(&mut b, Kind::Core, 0)?;
        let xch_pos = self.parse_optional_info(&mut b, data)?;
        if let Some(pos) = xch_pos {
            // A broken XCh falls back to the primary channels.
            let primary = self.nchannels;
            let mut x = Bits::new(&data[..self.h.frame_size.min(data.len())]);
            x.skip(pos);
            if self.parse_frame_data(&mut x, Kind::Xch, primary).is_ok() {
                self.has_xch = true;
            } else {
                self.nchannels = primary;
            }
        }
        Ok(self.filter())
    }

    // 5.3.2 Primary audio coding header
    fn parse_coding_header(&mut self, b: &mut Bits, kind: Kind, base: usize) -> Res<()> {
        if b.left() < 0 {
            return bad("a frame cut short");
        }
        match kind {
            Kind::Core => {
                self.nsubframes = b.get(4) as usize + 1;
                self.nchannels = b.get(3) as usize + 1;
                if self.nchannels != usize::from(CHANNELS_TABLE[self.h.audio_mode]) {
                    return bad("a channel count that does not match the arrangement");
                }
            }
            Kind::Xch => {
                self.nchannels = usize::from(CHANNELS_TABLE[self.h.audio_mode]) + 1;
            }
        }
        for ch in base..self.nchannels {
            self.nsubbands[ch] = b.get(5) as usize + 2;
            if self.nsubbands[ch] > SUBBANDS {
                return bad("an invalid subband count");
            }
        }
        for ch in base..self.nchannels {
            self.subband_vq_start[ch] = b.get(5) as usize + 1;
        }
        for ch in base..self.nchannels {
            let n = b.get(3) as usize;
            if n > self.nchannels {
                return bad("an invalid joint intensity index");
            }
            self.joint_intensity_index[ch] = n;
        }
        for ch in base..self.nchannels {
            self.transition_mode_sel[ch] = b.get(2) as usize;
        }
        for ch in base..self.nchannels {
            self.scale_factor_sel[ch] = b.get(3) as usize;
            if self.scale_factor_sel[ch] == 7 {
                return bad("an invalid scale factor code book");
            }
        }
        for ch in base..self.nchannels {
            self.bit_allocation_sel[ch] = b.get(3) as usize;
            if self.bit_allocation_sel[ch] == 7 {
                return bad("an invalid bit allocation select");
            }
        }
        for n in 0..CODE_BOOKS {
            for ch in base..self.nchannels {
                self.quant_index_sel[ch][n] = b.get(u32::from(QUANT_INDEX_SEL_NBITS[n])) as usize;
            }
        }
        for n in 0..CODE_BOOKS {
            for ch in base..self.nchannels {
                if self.quant_index_sel[ch][n] < usize::from(QUANT_INDEX_GROUP_SIZE[n]) {
                    self.scale_factor_adj[ch][n] = SCALE_FACTOR_ADJ[b.get(2) as usize];
                }
            }
        }
        if self.h.crc_present {
            b.skip(16);
        }
        Ok(())
    }

    fn parse_scale(&self, b: &mut Bits, index: &mut i32, sel: usize) -> Res<i32> {
        let table: &[i32] = if sel > 5 {
            &SCALE_FACTOR_QUANT7
        } else {
            &SCALE_FACTOR_QUANT6
        };
        if sel < 5 {
            *index += vlcs().scale_factor[sel].read(b)?;
        } else {
            *index = b.get(sel as u32 + 1) as i32;
        }
        usize::try_from(*index)
            .ok()
            .and_then(|i| table.get(i).copied())
            .ok_or(DtsError("an invalid scale factor index"))
    }

    fn parse_joint_scale(&self, b: &mut Bits, sel: usize) -> Res<i32> {
        let index = if sel < 5 {
            vlcs().scale_factor[sel].read(b)?
        } else {
            b.get(sel as u32 + 1) as i32
        } + 64;
        usize::try_from(index)
            .ok()
            .and_then(|i| JOINT_SCALE_FACTORS.get(i).copied())
            .ok_or(DtsError("an invalid joint scale factor index"))
    }

    // 5.4.1 Primary audio coding side information
    fn parse_subframe_header(
        &mut self,
        b: &mut Bits,
        sf: usize,
        kind: Kind,
        base: usize,
    ) -> Res<()> {
        if b.left() < 0 {
            return bad("a frame cut short");
        }
        if kind == Kind::Core {
            self.nsubsubframes[sf] = b.get(2) as usize + 1;
            b.skip(3);
        }
        for ch in base..self.nchannels {
            for band in 0..self.nsubbands[ch] {
                self.prediction_mode[ch][band] = b.get(1) == 1;
            }
        }
        for ch in base..self.nchannels {
            for band in 0..self.nsubbands[ch] {
                if self.prediction_mode[ch][band] {
                    self.prediction_vq_index[ch][band] = b.get(12) as usize;
                }
            }
        }
        for ch in base..self.nchannels {
            let sel = self.bit_allocation_sel[ch];
            for band in 0..self.subband_vq_start[ch].min(SUBBANDS) {
                let abits = if sel < 5 {
                    vlcs().bit_allocation[sel].read(b)?
                } else {
                    b.get(sel as u32 - 1) as i32
                };
                if abits > ABITS_MAX {
                    return bad("an invalid bit allocation");
                }
                self.bit_allocation[ch][band] = abits;
            }
        }
        for ch in base..self.nchannels {
            self.transition_mode[sf][ch] = [0; SUBBANDS];
            if self.nsubsubframes[sf] > 1 {
                let sel = self.transition_mode_sel[ch];
                for band in 0..self.subband_vq_start[ch].min(SUBBANDS) {
                    if self.bit_allocation[ch][band] != 0 {
                        self.transition_mode[sf][ch][band] =
                            vlcs().transition_mode[sel].read(b)? as usize;
                    }
                }
            }
        }
        for ch in base..self.nchannels {
            let sel = self.scale_factor_sel[ch];
            let mut index = 0i32;
            let vq = self.subband_vq_start[ch].min(SUBBANDS);
            for band in 0..vq {
                if self.bit_allocation[ch][band] != 0 {
                    self.scale_factors[ch][band][0] = self.parse_scale(b, &mut index, sel)?;
                    if self.transition_mode[sf][ch][band] != 0 {
                        self.scale_factors[ch][band][1] = self.parse_scale(b, &mut index, sel)?;
                    }
                } else {
                    self.scale_factors[ch][band][0] = 0;
                }
            }
            for band in vq..self.nsubbands[ch] {
                self.scale_factors[ch][band][0] = self.parse_scale(b, &mut index, sel)?;
            }
        }
        for ch in base..self.nchannels {
            if self.joint_intensity_index[ch] != 0 {
                self.joint_scale_sel[ch] = b.get(3) as usize;
                if self.joint_scale_sel[ch] == 7 {
                    return bad("an invalid joint scale code book");
                }
            }
        }
        for ch in base..self.nchannels {
            if let Some(src) = self.joint_intensity_index[ch].checked_sub(1) {
                let sel = self.joint_scale_sel[ch];
                for band in self.nsubbands[ch]..self.nsubbands[src] {
                    self.joint_scale_factors[ch][band] = self.parse_joint_scale(b, sel)?;
                }
            }
        }
        if self.h.drc_present && kind == Kind::Core {
            b.skip(8);
        }
        if self.h.crc_present {
            b.skip(16);
        }
        Ok(())
    }

    fn extract_audio(
        &self,
        b: &mut Bits,
        audio: &mut [i32; 16],
        abits: i32,
        ch: usize,
    ) -> Res<bool> {
        if abits == 0 {
            audio[..SUBBAND_SAMPLES].iter_mut().for_each(|v| *v = 0);
            return Ok(false);
        }
        let ab = abits as usize;
        if ab <= CODE_BOOKS {
            let sel = self.quant_index_sel[ch][ab - 1];
            if sel < usize::from(QUANT_INDEX_GROUP_SIZE[ab - 1]) {
                let vlc = &vlcs().quant_index[ab - 1][sel];
                for v in audio.iter_mut().take(SUBBAND_SAMPLES) {
                    *v = vlc.read(b)?;
                }
                return Ok(true);
            }
            if ab <= 7 {
                let nbits = BLOCK_CODE_NBITS[ab - 1];
                let mut code1 = b.get(nbits) as i32;
                let mut code2 = b.get(nbits) as i32;
                let levels = QUANT_LEVELS[ab];
                let offset = (levels - 1) / 2;
                for (n, v) in audio.iter_mut().take(SUBBAND_SAMPLES).enumerate() {
                    let code = if n < SUBBAND_SAMPLES / 2 {
                        &mut code1
                    } else {
                        &mut code2
                    };
                    let div = *code / levels;
                    *v = *code - div * levels - offset;
                    *code = div;
                }
                if code1 | code2 != 0 {
                    return bad("an invalid block code");
                }
                return Ok(false);
            }
        }
        for v in audio.iter_mut().take(SUBBAND_SAMPLES) {
            *v = b.get_signed(ab as u32 - 3);
        }
        Ok(false)
    }

    // 5.5 Primary audio data arrays
    fn parse_subframe_audio(
        &mut self,
        b: &mut Bits,
        sf: usize,
        kind: Kind,
        base: usize,
        sub_pos: &mut usize,
        lfe_pos: &mut usize,
    ) -> Res<()> {
        let nsamples = self.nsubsubframes[sf] * SUBBAND_SAMPLES;
        if *sub_pos + nsamples > self.h.npcmblocks {
            return bad("too many subband samples");
        }
        if b.left() < 0 {
            return bad("a frame cut short");
        }
        // High frequency VQ subbands.
        for ch in base..self.nchannels {
            let start = self.subband_vq_start[ch];
            for band in start..self.nsubbands[ch] {
                let row = b.get(10) as usize * 32;
                let scale = self.scale_factors[ch][band][0];
                for j in 0..nsamples {
                    let c = i32::from(HIGH_FREQ_VQ[row + j]);
                    self.subband[ch][band][ADPCM_COEFFS + *sub_pos + j] =
                        clip23((c * scale + (1 << 3)) >> 4);
                }
            }
        }
        // LFE.
        if self.h.lfe_present != 0 && kind == Kind::Core {
            let nlfe = 2 * self.h.lfe_present as usize * self.nsubsubframes[sf];
            let mut audio = [0i32; 16];
            for v in audio.iter_mut().take(nlfe) {
                *v = b.get_signed(8);
            }
            let index = b.get(8) as usize;
            let Some(&scale) = SCALE_FACTOR_QUANT7.get(index) else {
                return bad("an invalid LFE scale factor");
            };
            // The quantizer step is 0.035.
            let scale = mul(4_697_620, scale, 23);
            for (n, v) in audio.iter().take(nlfe).enumerate() {
                let x = (i64::from(*v) * i64::from(scale)) >> 4;
                if let Some(slot) = self.lfe.get_mut(*lfe_pos + n) {
                    *slot = clip23(x as i32);
                }
            }
            *lfe_pos += nlfe;
        }
        // Audio data.
        let mut ofs = *sub_pos;
        for ssf in 0..self.nsubsubframes[sf] {
            for ch in base..self.nchannels {
                if b.left() < 0 {
                    return bad("a frame cut short");
                }
                for band in 0..self.subband_vq_start[ch].min(SUBBANDS) {
                    let abits = self.bit_allocation[ch][band];
                    let mut audio = [0i32; 16];
                    let huff = self.extract_audio(b, &mut audio, abits, ch)?;
                    let step_size = if self.h.bit_rate == 3 {
                        LOSSLESS_QUANT[abits as usize]
                    } else {
                        LOSSY_QUANT[abits as usize]
                    };
                    let trans = self.transition_mode[sf][ch][band];
                    let mut scale = if trans == 0 || ssf < trans {
                        self.scale_factors[ch][band][0]
                    } else {
                        self.scale_factors[ch][band][1]
                    };
                    if huff {
                        let adj = i64::from(self.scale_factor_adj[ch][abits as usize - 1]);
                        scale = clip23(((adj * i64::from(scale)) >> 22) as i32);
                    }
                    dequantize(
                        &mut self.subband[ch][band]
                            [ADPCM_COEFFS + ofs..ADPCM_COEFFS + ofs + SUBBAND_SAMPLES],
                        &audio[..SUBBAND_SAMPLES],
                        step_size,
                        scale,
                    );
                }
            }
            if (ssf == self.nsubsubframes[sf] - 1 || self.h.sync_ssf) && b.get(16) != 0xffff {
                return bad("a lost sync inside a frame");
            }
            ofs += SUBBAND_SAMPLES;
        }
        // Inverse ADPCM.
        for ch in base..self.nchannels {
            for band in 0..self.nsubbands[ch] {
                if !self.prediction_mode[ch][band] {
                    continue;
                }
                let coeff = &ADPCM_VB[self.prediction_vq_index[ch][band] * 4..][..4];
                let s = &mut self.subband[ch][band];
                for j in 0..nsamples {
                    let at = ADPCM_COEFFS + *sub_pos + j;
                    let mut pred = 0i64;
                    for (i, c) in coeff.iter().enumerate() {
                        pred += i64::from(s[at - 1 - i]) * i64::from(*c);
                    }
                    let x = clip23(norm(pred, 13));
                    s[at] = clip23(s[at].wrapping_add(x));
                }
            }
        }
        // Joint subband coding.
        for ch in base..self.nchannels {
            if let Some(src) = self.joint_intensity_index[ch].checked_sub(1) {
                for band in self.nsubbands[ch]..self.nsubbands[src] {
                    let scale = self.joint_scale_factors[ch][band];
                    for j in 0..nsamples {
                        let at = ADPCM_COEFFS + *sub_pos + j;
                        self.subband[ch][band][at] =
                            clip23(mul(self.subband[src][band][at], scale, 17));
                    }
                }
            }
        }
        *sub_pos = ofs;
        Ok(())
    }

    fn parse_frame_data(&mut self, b: &mut Bits, kind: Kind, base: usize) -> Res<()> {
        self.parse_coding_header(b, kind, base)?;
        let mut sub_pos = 0usize;
        let mut lfe_pos = LFE_HISTORY;
        for sf in 0..self.nsubframes {
            self.parse_subframe_header(b, sf, kind, base)?;
            self.parse_subframe_audio(b, sf, kind, base, &mut sub_pos, &mut lfe_pos)?;
        }
        let n = self.h.npcmblocks;
        for ch in base..self.nchannels {
            let mut active = self.nsubbands[ch];
            if let Some(src) = self.joint_intensity_index[ch].checked_sub(1) {
                active = active.max(self.nsubbands[src]);
            }
            for band in 0..SUBBANDS {
                let s = &mut self.subband[ch][band];
                if band < active {
                    // Keep the last 4 samples as the next frame's history.
                    s.copy_within(n..n + ADPCM_COEFFS, 0);
                } else {
                    s.iter_mut().for_each(|v| *v = 0);
                }
            }
        }
        Ok(())
    }

    /// Skips the time code and aux data, and finds an XCh extension.
    /// Returns the bit position of the XCh data, if there is one.
    fn parse_optional_info(&mut self, b: &mut Bits, data: &[u8]) -> Res<Option<usize>> {
        if self.h.ts_present {
            b.skip(32);
        }
        if self.h.aux_present {
            // Aux data (the dynamic downmix) is not used; just step over it.
            b.skip(6);
            let pad = (32 - b.count() % 32) % 32;
            b.skip(pad);
            if b.get(32) == SYNC_REV1AUX {
                if b.get(1) == 1 {
                    b.skip(47);
                }
                if b.get(1) == 1 {
                    let kind = b.get(3) as usize;
                    const DMIX_NCH: [usize; 8] = [1, 2, 2, 3, 3, 4, 4, 0];
                    let m = DMIX_NCH.get(kind).copied().unwrap_or(0);
                    let n = usize::from(CHANNELS_TABLE[self.h.audio_mode])
                        + usize::from(self.h.lfe_present != 0);
                    b.skip(9 * m * n);
                }
            }
        }
        // XCh: search back from the end of the core frame, on 4-byte steps.
        if !(self.h.ext_audio_present && self.h.ext_audio_type == 0) {
            return Ok(None);
        }
        let frame_size = self.h.frame_size.min(data.len());
        let words = frame_size / 4;
        if words == 0 {
            return Ok(None);
        }
        let last = b.count() / 32;
        let rb32 = |i: usize| {
            data.get(i * 4..i * 4 + 4)
                .map_or(0, |w| u32::from_be_bytes([w[0], w[1], w[2], w[3]]))
        };
        let mut pos = words - 1;
        let mut w2 = 0u32;
        while pos >= last && pos > 0 {
            let w1 = rb32(pos);
            if w1 == SYNC_XCH {
                let size = (w2 >> 22) as usize + 1;
                let dist = frame_size - pos * 4;
                if size >= 96 && (size == dist || size - 1 == dist) && (w2 >> 15 & 0x7f) == 0x08 {
                    return Ok(Some(pos * 32 + 49));
                }
            }
            w2 = w1;
            pos -= 1;
        }
        Ok(None)
    }

    fn speaker_of(&self, ch: usize) -> Option<usize> {
        let primary = PRM_CH_TO_SPKR[self.h.audio_mode];
        if ch < primary.len() {
            return Some(primary[ch]);
        }
        (self.has_xch && ch == primary.len()).then_some(SPK_CS)
    }

    /// The synthesis filter bank, the LFE interpolator and the sum and
    /// difference decoding, giving PCM per speaker.
    fn filter(&mut self) -> Decoded {
        let n = self.h.npcmblocks;
        let ns = n * PCMBLOCK_SAMPLES;
        let mut speakers: [Option<Vec<i32>>; SPEAKERS] = Default::default();
        let window: &[i32; 512] = if self.h.filter_perfect {
            &FIR_32BANDS_PERFECT
        } else {
            &FIR_32BANDS_NONPERFECT
        };
        for ch in 0..self.nchannels {
            let Some(spkr) = self.speaker_of(ch) else {
                continue;
            };
            let mut out = vec![0i32; ns];
            let mut input = [0i32; 32];
            for j in 0..n {
                for (i, v) in input.iter_mut().enumerate() {
                    *v = self.subband[ch][i][ADPCM_COEFFS + j];
                }
                let block: &mut [i32] = &mut out[j * 32..j * 32 + 32];
                synth_filter(&mut self.synth[ch], window, block, &input);
            }
            speakers[spkr] = Some(out);
        }
        if self.h.lfe_present != 0 {
            let mut out = vec![0i32; ns];
            if self.h.lfe_present == 2 {
                lfe_fir_64(&mut out, &self.lfe, n);
            } else {
                lfe_fir_128(&mut out, &self.lfe, n);
            }
            let used = n >> (if self.h.lfe_present == 1 { 2 } else { 1 });
            for k in (0..LFE_HISTORY).rev() {
                self.lfe[k] = self.lfe[used + k];
            }
            speakers[SPK_LFE] = Some(out);
        }
        // An embedded XCh downmix in the surrounds is taken back out.
        if self.h.es_format && self.has_xch && self.h.audio_mode >= AMODE_2F2R {
            if let Some(cs) = speakers[SPK_CS].clone() {
                for spk in [SPK_LS, SPK_RS] {
                    if let Some(s) = speakers[spk].as_mut() {
                        for (v, c) in s.iter_mut().zip(&cs) {
                            *v = v.wrapping_sub(mul(*c, 5_931_520, 23));
                        }
                    }
                }
            }
        }
        if !self.has_xch {
            let butterfly = |sp: &mut [Option<Vec<i32>>; SPEAKERS], a: usize, b: usize| {
                if let (Some(x), Some(y)) = (sp[a].take(), sp[b].take()) {
                    let (mut x, mut y) = (x, y);
                    for (p, q) in x.iter_mut().zip(y.iter_mut()) {
                        let (s, d) = (p.wrapping_add(*q), p.wrapping_sub(*q));
                        *p = s;
                        *q = d;
                    }
                    sp[a] = Some(x);
                    sp[b] = Some(y);
                }
            };
            if (self.h.sumdiff_front && self.h.audio_mode > 0)
                || self.h.audio_mode == AMODE_STEREO_SUMDIFF
            {
                butterfly(&mut speakers, SPK_L, SPK_R);
            }
            if self.h.sumdiff_surround && self.h.audio_mode >= AMODE_2F2R {
                butterfly(&mut speakers, SPK_LS, SPK_RS);
            }
        }
        Decoded { speakers }
    }
}

/// The table of primary channel counts per audio mode.
const CHANNELS_TABLE: [u8; 16] = super::tables::CHANNELS;

fn dequantize(out: &mut [i32], input: &[i32], step_size: i32, scale: i32) {
    let mut step_scale = i64::from(step_size) * i64::from(scale);
    let mut shift = 0i32;
    if step_scale > (1 << 23) {
        shift = 63 - (step_scale >> 23).leading_zeros() as i32 + 1;
        step_scale >>= shift;
    }
    for (o, i) in out.iter_mut().zip(input) {
        *o = clip23(norm(i64::from(*i) * step_scale, 22 - shift));
    }
}

fn synth_filter(s: &mut Synth, window: &[i32; 512], out: &mut [i32], input: &[i32; 32]) {
    let off = s.offset;
    {
        let buf = &mut s.hist1[off..off + 32];
        imdct_half_32(buf, input);
    }
    let h = &s.hist1;
    let at = |k: isize| -> i64 {
        let k = (off as isize + k).rem_euclid(512) as usize;
        i64::from(h[k])
    };
    for i in 0..16 {
        let mut a = i64::from(s.hist2[i]) << 21;
        let mut b = i64::from(s.hist2[i + 16]) << 21;
        let mut c = 0i64;
        let mut d = 0i64;
        let mut j = 0usize;
        while j < 512 {
            let (ii, jj) = (i as isize, j as isize);
            a += i64::from(window[i + j]) * at(ii + jj);
            b += i64::from(window[i + j + 16]) * at(15 - ii + jj);
            c += i64::from(window[i + j + 32]) * at(16 + ii + jj);
            d += i64::from(window[i + j + 48]) * at(31 - ii + jj);
            j += 64;
        }
        out[i] = clip23(norm(a, 21));
        out[i + 16] = clip23(norm(b, 21));
        s.hist2[i] = norm(c, 21);
        s.hist2[i + 16] = norm(d, 21);
    }
    s.offset = (s.offset + 512 - 32) & 511;
}

fn lfe_fir_64(out: &mut [i32], lfe: &[i32], npcmblocks: usize) {
    let nlfe = npcmblocks >> 1;
    for i in 0..nlfe {
        let base = LFE_HISTORY + i;
        for j in 0..32 {
            let mut a = 0i64;
            let mut b = 0i64;
            for k in 0..8 {
                let x = i64::from(lfe[base - k]);
                a += i64::from(LFE_FIR_64[j * 8 + k]) * x;
                b += i64::from(LFE_FIR_64[255 - j * 8 - k]) * x;
            }
            out[i * 64 + j] = clip23(norm(a, 23));
            out[i * 64 + 32 + j] = clip23(norm(b, 23));
        }
    }
}

/// The 128-times LFE interpolator. FFmpeg only has it in floating point;
/// its coefficients here are unscaled, so the result is in the same
/// 24-bit units as everything else.
fn lfe_fir_128(out: &mut [i32], lfe: &[i32], npcmblocks: usize) {
    let nlfe = npcmblocks >> 2;
    for i in 0..nlfe {
        let base = LFE_HISTORY + i;
        for j in 0..64 {
            let mut a = 0f64;
            let mut b = 0f64;
            for k in 0..4 {
                let x = f64::from(lfe[base - k]);
                a += f64::from(LFE_FIR_128[j * 4 + k]) * x;
                b += f64::from(LFE_FIR_128[255 - j * 4 - k]) * x;
            }
            out[i * 128 + j] = clip23(a.round() as i32);
            out[i * 128 + 64 + j] = clip23(b.round() as i32);
        }
    }
}

// ---- The fixed-point half IMDCT (dcadct.c) ----

fn sum_a(input: &[i32], output: &mut [i32], len: usize) {
    for i in 0..len {
        output[i] = input[2 * i].wrapping_add(input[2 * i + 1]);
    }
}

fn sum_b(input: &[i32], output: &mut [i32], len: usize) {
    output[0] = input[0];
    for i in 1..len {
        output[i] = input[2 * i].wrapping_add(input[2 * i - 1]);
    }
}

fn sum_c(input: &[i32], output: &mut [i32], len: usize) {
    for i in 0..len {
        output[i] = input[2 * i];
    }
}

fn sum_d(input: &[i32], output: &mut [i32], len: usize) {
    output[0] = input[1];
    for i in 1..len {
        output[i] = input[2 * i - 1].wrapping_add(input[2 * i + 1]);
    }
}

const DCT_A: [[i32; 8]; 8] = [
    [
        8348215, 8027397, 7398092, 6484482, 5321677, 3954362, 2435084, 822227,
    ],
    [
        8027397, 5321677, 822227, -3954362, -7398092, -8348215, -6484482, -2435084,
    ],
    [
        7398092, 822227, -6484482, -8027397, -2435084, 5321677, 8348215, 3954362,
    ],
    [
        6484482, -3954362, -8027397, 822227, 8348215, 2435084, -7398092, -5321677,
    ],
    [
        5321677, -7398092, -2435084, 8348215, -822227, -8027397, 3954362, 6484482,
    ],
    [
        3954362, -8348215, 5321677, 2435084, -8027397, 6484482, 822227, -7398092,
    ],
    [
        2435084, -6484482, 8348215, -7398092, 3954362, 822227, -5321677, 8027397,
    ],
    [
        822227, -2435084, 3954362, -5321677, 6484482, -7398092, 8027397, -8348215,
    ],
];

const DCT_B: [[i32; 7]; 8] = [
    [
        8227423, 7750063, 6974873, 5931642, 4660461, 3210181, 1636536,
    ],
    [
        6974873, 3210181, -1636536, -5931642, -8227423, -7750063, -4660461,
    ],
    [
        4660461, -3210181, -8227423, -5931642, 1636536, 7750063, 6974873,
    ],
    [
        1636536, -7750063, -4660461, 5931642, 6974873, -3210181, -8227423,
    ],
    [
        -1636536, -7750063, 4660461, 5931642, -6974873, -3210181, 8227423,
    ],
    [
        -4660461, -3210181, 8227423, -5931642, -1636536, 7750063, -6974873,
    ],
    [
        -6974873, 3210181, 1636536, -5931642, 8227423, -7750063, 4660461,
    ],
    [
        -8227423, 7750063, -6974873, 5931642, -4660461, 3210181, -1636536,
    ],
];

fn dct_a(input: &[i32], output: &mut [i32]) {
    for i in 0..8 {
        let mut res = 0i64;
        for j in 0..8 {
            res += i64::from(DCT_A[i][j]) * i64::from(input[j]);
        }
        output[i] = norm(res, 23);
    }
}

fn dct_b(input: &[i32], output: &mut [i32]) {
    for i in 0..8 {
        let mut res = i64::from(input[0]) << 23;
        for j in 0..7 {
            res += i64::from(DCT_B[i][j]) * i64::from(input[1 + j]);
        }
        output[i] = norm(res, 23);
    }
}

fn mod_a(input: &[i32], output: &mut [i32]) {
    const C: [i32; 16] = [
        4199362, 4240198, 4323885, 4454708, 4639772, 4890013, 5221943, 5660703, -6245623, -7040975,
        -8158494, -9809974, -12450076, -17261920, -28585092, -85479984,
    ];
    for i in 0..8 {
        output[i] = mul(C[i], input[i].wrapping_add(input[8 + i]), 23);
    }
    for (i, k) in (8..16).zip((0..8).rev()) {
        output[i] = mul(C[i], input[k].wrapping_sub(input[8 + k]), 23);
    }
}

fn mod_b(input: &mut [i32], output: &mut [i32]) {
    const C: [i32; 8] = [
        4214598, 4383036, 4755871, 5425934, 6611520, 8897610, 14448934, 42791536,
    ];
    for i in 0..8 {
        input[8 + i] = mul(C[i], input[8 + i], 23);
    }
    for i in 0..8 {
        output[i] = input[i].wrapping_add(input[8 + i]);
    }
    for (i, k) in (8..16).zip((0..8).rev()) {
        output[i] = input[k].wrapping_sub(input[8 + k]);
    }
}

fn mod_c(input: &[i32], output: &mut [i32]) {
    const C: [i32; 32] = [
        1048892, 1051425, 1056522, 1064244, 1074689, 1087987, 1104313, 1123884, 1146975, 1173922,
        1205139, 1241133, 1282529, 1330095, 1384791, 1447815, -1520688, -1605358, -1704360,
        -1821051, -1959964, -2127368, -2332183, -2587535, -2913561, -3342802, -3931480, -4785806,
        -6133390, -8566050, -14253820, -42727120,
    ];
    for i in 0..16 {
        output[i] = mul(C[i], input[i].wrapping_add(input[16 + i]), 23);
    }
    for (i, k) in (16..32).zip((0..16).rev()) {
        output[i] = mul(C[i], input[k].wrapping_sub(input[16 + k]), 23);
    }
}

fn clp_v(v: &mut [i32]) {
    v.iter_mut().for_each(|x| *x = clip23(*x));
}

fn imdct_half_32(output: &mut [i32], input: &[i32; 32]) {
    let mut a = [0i32; 32];
    let mut b = [0i32; 32];
    let mag: i64 = input.iter().map(|v| i64::from(*v).abs()).sum();
    let shift = if mag > 0x40_0000 { 2 } else { 0 };
    let round = if shift > 0 { 1 << (shift - 1) } else { 0 };
    for i in 0..32 {
        a[i] = (input[i] + round) >> shift;
    }
    sum_a(&a, &mut b[0..], 16);
    sum_b(&a, &mut b[16..], 16);
    clp_v(&mut b);

    sum_a(&b[0..], &mut a[0..], 8);
    sum_b(&b[0..], &mut a[8..], 8);
    sum_c(&b[16..], &mut a[16..], 8);
    sum_d(&b[16..], &mut a[24..], 8);
    clp_v(&mut a);

    dct_a(&a[0..], &mut b[0..]);
    dct_b(&a[8..], &mut b[8..]);
    dct_b(&a[16..], &mut b[16..]);
    dct_b(&a[24..], &mut b[24..]);
    clp_v(&mut b);

    {
        let (lo, hi) = a.split_at_mut(16);
        mod_a(&b[0..16], lo);
        let mut tmp = [0i32; 16];
        tmp.copy_from_slice(&b[16..32]);
        mod_b(&mut tmp, hi);
    }
    clp_v(&mut a);

    mod_c(&a, &mut b);

    for v in b.iter_mut() {
        *v = clip23(v.wrapping_mul(1 << shift));
    }
    for (i, k) in (0..16).zip((16..32).rev()) {
        output[i] = clip23(b[i].wrapping_sub(b[k]));
        output[16 + i] = clip23(b[i].wrapping_add(b[k]));
    }
}
