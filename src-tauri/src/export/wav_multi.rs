// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Multichannel WAV for 3D Space exports: the extensible WAV header
//! with its channel mask, so other software knows which channel is
//! which speaker (left, right, center, low frequency, surrounds,
//! heights). 16 or 24-bit PCM, or 32-bit float.

use std::fs::File;
use std::io::{BufWriter, Seek, SeekFrom, Write};
use std::path::Path;

/// The format GUIDs' shared tail.
const GUID_TAIL: [u8; 14] = [
    0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0xAA, 0x00, 0x38, 0x9B, 0x71,
];

pub struct MultiWav {
    out: BufWriter<File>,
    channels: u16,
    bits: u16,
    frames: u64,
    buf: Vec<u8>,
    /// Triangular dither for integer output.
    seed: u32,
}

impl MultiWav {
    pub fn create(
        path: &Path,
        rate: u32,
        channels: u16,
        mask: u32,
        bits: u16,
    ) -> std::io::Result<MultiWav> {
        let bits = if [16, 24, 32].contains(&bits) {
            bits
        } else {
            24
        };
        let mut out = BufWriter::new(File::create(path)?);
        let block = channels * (bits / 8);
        let float = bits == 32;
        out.write_all(b"RIFF")?;
        out.write_all(&0u32.to_le_bytes())?; // filled in at the end
        out.write_all(b"WAVE")?;
        out.write_all(b"fmt ")?;
        out.write_all(&40u32.to_le_bytes())?;
        out.write_all(&0xFFFEu16.to_le_bytes())?; // extensible
        out.write_all(&channels.to_le_bytes())?;
        out.write_all(&rate.to_le_bytes())?;
        out.write_all(&(rate * u32::from(block)).to_le_bytes())?;
        out.write_all(&block.to_le_bytes())?;
        out.write_all(&bits.to_le_bytes())?;
        out.write_all(&22u16.to_le_bytes())?;
        out.write_all(&bits.to_le_bytes())?; // valid bits
        out.write_all(&mask.to_le_bytes())?;
        out.write_all(&(if float { 3u16 } else { 1u16 }).to_le_bytes())?;
        out.write_all(&GUID_TAIL)?;
        if float {
            // The frame count, which the format asks of float files: some
            // players look for it to read the samples as float at all.
            out.write_all(b"fact")?;
            out.write_all(&4u32.to_le_bytes())?;
            out.write_all(&0u32.to_le_bytes())?; // filled in at the end
        }
        out.write_all(b"data")?;
        out.write_all(&0u32.to_le_bytes())?; // filled in at the end
        Ok(MultiWav {
            out,
            channels,
            bits,
            frames: 0,
            buf: Vec::new(),
            seed: 0x9E37_79B9,
        })
    }

    fn noise(&mut self) -> f32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        self.seed as f32 / u32::MAX as f32
    }

    /// Interleaved samples, `channels` to a frame.
    pub fn write(&mut self, samples: &[f32]) -> std::io::Result<()> {
        self.buf.clear();
        for &v in samples {
            match self.bits {
                32 => self.buf.extend_from_slice(&v.to_le_bytes()),
                24 => {
                    let d = (self.noise() - self.noise()) / 8_388_608.0;
                    let s = ((v + d).clamp(-1.0, 1.0) * 8_388_607.0).round() as i32;
                    self.buf.extend_from_slice(&s.to_le_bytes()[..3]);
                }
                _ => {
                    let d = (self.noise() - self.noise()) / 32_768.0;
                    let s = ((v + d).clamp(-1.0, 1.0) * 32_767.0).round() as i16;
                    self.buf.extend_from_slice(&s.to_le_bytes());
                }
            }
        }
        self.frames += (samples.len() / usize::from(self.channels)) as u64;
        self.out.write_all(&self.buf)
    }

    /// Writes the sizes into the header. Returns the frames written.
    pub fn finish(mut self) -> std::io::Result<u64> {
        let data = self.frames * u64::from(self.channels) * u64::from(self.bits / 8);
        if data % 2 == 1 {
            self.out.write_all(&[0])?;
        }
        let data = u32::try_from(data)
            .map_err(|_| std::io::Error::other("The file is too long for WAV (over 4 GB)."))?;
        self.out.flush()?;
        let mut f = self.out.into_inner().map_err(|e| e.into_error())?;
        let fact: u32 = if self.bits == 32 { 12 } else { 0 };
        let riff = 4 + 8 + 40 + fact + 8 + data + (data % 2);
        f.seek(SeekFrom::Start(4))?;
        f.write_all(&riff.to_le_bytes())?;
        if fact > 0 {
            f.seek(SeekFrom::Start(4 + 4 + 4 + 8 + 40 + 8))?;
            f.write_all(&(self.frames.min(u64::from(u32::MAX)) as u32).to_le_bytes())?;
        }
        f.seek(SeekFrom::Start(4 + 4 + 4 + 8 + 40 + u64::from(fact) + 4))?;
        f.write_all(&data.to_le_bytes())?;
        f.flush()?;
        Ok(self.frames)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_a_readable_multichannel_file_with_its_mask() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bed.wav");
        let mut w = MultiWav::create(&path, 48_000, 12, 0x2D63F, 32).unwrap();
        let frame: Vec<f32> = (0..12).map(|c| c as f32 / 20.0).collect();
        for _ in 0..100 {
            w.write(&frame).unwrap();
        }
        assert_eq!(w.finish().unwrap(), 100);
        let r = hound::WavReader::open(&path).unwrap();
        assert_eq!(r.spec().channels, 12);
        assert_eq!(r.len(), 1_200);
        let s: Vec<f32> = r.into_samples::<f32>().map(Result::unwrap).collect();
        assert_eq!(s[13], 0.05);
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(
            u32::from_le_bytes(bytes[40..44].try_into().unwrap()),
            0x2D63F
        );
        // Float files carry their frame count in a fact chunk.
        assert_eq!(&bytes[60..64], b"fact");
        assert_eq!(u32::from_le_bytes(bytes[68..72].try_into().unwrap()), 100);
    }
}
