// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! 4-bit IMA ADPCM in a WAV file: about a quarter the size of 16-bit
//! PCM, lossy, and readable by nearly every player and phone system.
//! The format is the public IMA (Interactive Multimedia Association)
//! recommendation: each block starts with the exact first sample and a
//! step index per channel, then 4-bit codes, low nibble first, in groups
//! of eight samples per channel.

use std::fs::File;
use std::io::{BufWriter, Seek, SeekFrom, Write};
use std::path::Path;

const STEPS: [i32; 89] = [
    7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 19, 21, 23, 25, 28, 31, 34, 37, 41, 45, 50, 55, 60, 66,
    73, 80, 88, 97, 107, 118, 130, 143, 157, 173, 190, 209, 230, 253, 279, 307, 337, 371, 408, 449,
    494, 544, 598, 658, 724, 796, 876, 963, 1060, 1166, 1282, 1411, 1552, 1707, 1878, 2066, 2272,
    2499, 2749, 3024, 3327, 3660, 4026, 4428, 4871, 5358, 5894, 6484, 7132, 7845, 8630, 9493,
    10442, 11487, 12635, 13899, 15289, 16818, 18500, 20350, 22385, 24623, 27086, 29794, 32767,
];
const INDEX_ADJUST: [i32; 8] = [-1, -1, -1, -1, 2, 4, 6, 8];
/// RIFF header, fmt (20 bytes of fields), fact and the data chunk header.
const HEADER_LEN: u64 = 12 + 8 + 20 + 8 + 4 + 8;

/// Running state of one channel.
#[derive(Clone, Copy, Default)]
pub struct Channel {
    pred: i32,
    index: i32,
}

impl Channel {
    /// The 4-bit code for one sample, updating the prediction.
    pub fn encode(&mut self, sample: i16) -> u8 {
        let step = STEPS[self.index as usize];
        let mut diff = i32::from(sample) - self.pred;
        let mut code = 0u8;
        if diff < 0 {
            code = 8;
            diff = -diff;
        }
        let mut delta = step >> 3;
        if diff >= step {
            code |= 4;
            diff -= step;
            delta += step;
        }
        if diff >= step >> 1 {
            code |= 2;
            diff -= step >> 1;
            delta += step >> 1;
        }
        if diff >= step >> 2 {
            code |= 1;
            delta += step >> 2;
        }
        self.pred = if code & 8 != 0 {
            self.pred - delta
        } else {
            self.pred + delta
        }
        .clamp(-32_768, 32_767);
        self.index = (self.index + INDEX_ADJUST[usize::from(code & 7)]).clamp(0, 88);
        code
    }

    /// What a decoder rebuilds from a code. Used by the tests.
    pub fn decode(&mut self, code: u8) -> i16 {
        let step = STEPS[self.index as usize];
        let mut delta = step >> 3;
        if code & 4 != 0 {
            delta += step;
        }
        if code & 2 != 0 {
            delta += step >> 1;
        }
        if code & 1 != 0 {
            delta += step >> 2;
        }
        self.pred = if code & 8 != 0 {
            self.pred - delta
        } else {
            self.pred + delta
        }
        .clamp(-32_768, 32_767);
        self.index = (self.index + INDEX_ADJUST[usize::from(code & 7)]).clamp(0, 88);
        self.pred as i16
    }
}

/// Bytes per block for a rate and channel count, as common encoders choose.
pub fn block_align(rate: u32, channels: u16) -> u16 {
    let per = if rate <= 11_025 {
        256
    } else if rate <= 22_050 {
        512
    } else {
        1024
    };
    per * channels
}

/// Sample frames in one block.
pub fn samples_per_block(block_align: u16, channels: u16) -> u16 {
    (block_align / channels - 4) * 2 + 1
}

/// Encodes one block of `spb` interleaved frames (padded by the caller).
pub fn encode_block(
    frames: &[i16],
    ch: usize,
    spb: usize,
    state: &mut [Channel],
    out: &mut Vec<u8>,
) {
    for (c, st) in state.iter_mut().enumerate() {
        let first = frames[c];
        st.pred = i32::from(first);
        out.extend_from_slice(&first.to_le_bytes());
        out.push(st.index as u8);
        out.push(0);
    }
    let groups = (spb - 1) / 8;
    for g in 0..groups {
        for (c, st) in state.iter_mut().enumerate() {
            for pair in 0..4 {
                let i = 1 + g * 8 + pair * 2;
                let lo = st.encode(frames[i * ch + c]);
                let hi = st.encode(frames[(i + 1) * ch + c]);
                out.push(lo | (hi << 4));
            }
        }
    }
}

pub struct AdpcmWriter {
    file: BufWriter<File>,
    ch: usize,
    rate: u32,
    align: u16,
    spb: usize,
    pending: Vec<i16>,
    state: Vec<Channel>,
    frames: u64,
    data: u64,
    block: Vec<u8>,
}

impl AdpcmWriter {
    pub fn create(path: &Path, rate: u32, channels: u16) -> std::io::Result<Self> {
        let align = block_align(rate, channels);
        let mut w = Self {
            file: BufWriter::new(File::create(path)?),
            ch: usize::from(channels),
            rate,
            align,
            spb: usize::from(samples_per_block(align, channels)),
            pending: Vec::new(),
            state: vec![Channel::default(); usize::from(channels)],
            frames: 0,
            data: 0,
            block: Vec::with_capacity(usize::from(align)),
        };
        w.header()?;
        Ok(w)
    }

    fn header(&mut self) -> std::io::Result<()> {
        let ch = self.ch as u16;
        let byte_rate = (u64::from(self.rate) * u64::from(self.align) / self.spb as u64) as u32;
        let f = &mut self.file;
        f.write_all(b"RIFF")?;
        f.write_all(&((HEADER_LEN - 8 + self.data) as u32).to_le_bytes())?;
        f.write_all(b"WAVEfmt ")?;
        f.write_all(&20u32.to_le_bytes())?;
        f.write_all(&0x0011u16.to_le_bytes())?;
        f.write_all(&ch.to_le_bytes())?;
        f.write_all(&self.rate.to_le_bytes())?;
        f.write_all(&byte_rate.to_le_bytes())?;
        f.write_all(&self.align.to_le_bytes())?;
        f.write_all(&4u16.to_le_bytes())?;
        f.write_all(&2u16.to_le_bytes())?;
        f.write_all(&(self.spb as u16).to_le_bytes())?;
        f.write_all(b"fact")?;
        f.write_all(&4u32.to_le_bytes())?;
        f.write_all(&(self.frames as u32).to_le_bytes())?;
        f.write_all(b"data")?;
        f.write_all(&(self.data as u32).to_le_bytes())?;
        Ok(())
    }

    /// Adds interleaved 16-bit samples.
    pub fn write(&mut self, samples: &[i16]) -> std::io::Result<()> {
        self.pending.extend_from_slice(samples);
        self.frames += (samples.len() / self.ch) as u64;
        let need = self.spb * self.ch;
        let mut at = 0;
        while self.pending.len() - at >= need {
            self.block.clear();
            encode_block(
                &self.pending[at..at + need],
                self.ch,
                self.spb,
                &mut self.state,
                &mut self.block,
            );
            self.file.write_all(&self.block)?;
            self.data += self.block.len() as u64;
            at += need;
        }
        self.pending.drain(..at);
        Ok(())
    }

    /// Pads and writes the last block, then fills in the header.
    pub fn finish(mut self) -> std::io::Result<()> {
        if !self.pending.is_empty() {
            let need = self.spb * self.ch;
            let last: Vec<i16> = self.pending[self.pending.len() - self.ch..].to_vec();
            while self.pending.len() < need {
                self.pending.extend_from_slice(&last);
            }
            self.block.clear();
            let frames = std::mem::take(&mut self.pending);
            encode_block(&frames, self.ch, self.spb, &mut self.state, &mut self.block);
            self.file.write_all(&self.block)?;
            self.data += self.block.len() as u64;
        }
        self.file.seek(SeekFrom::Start(0))?;
        self.header()?;
        self.file.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_follow_a_sine_closely() {
        let mut enc = Channel::default();
        let mut dec = Channel::default();
        let mut worst = 0i32;
        for i in 0..4_000 {
            let s = ((i as f32 * 0.05).sin() * 12_000.0) as i16;
            let code = enc.encode(s);
            let back = dec.decode(code);
            if i > 50 {
                worst = worst.max((i32::from(s) - i32::from(back)).abs());
            }
        }
        assert!(worst < 600, "worst error {worst}");
    }

    #[test]
    fn block_sizes_match_common_encoders() {
        assert_eq!(block_align(44_100, 1), 1024);
        assert_eq!(samples_per_block(1024, 1), 2041);
        assert_eq!(samples_per_block(2048, 2), 2041);
        assert_eq!(block_align(8_000, 1), 256);
    }
}
