// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Copies part of an audio file, untouched, to a new WAV. Used by Make
//! Unique Copy: the clip gets its own file, so later changes to it never
//! reach other clips that share the original.

use std::path::Path;

use crate::codecs::AudioReader;

#[derive(Debug, thiserror::Error)]
pub enum RegionError {
    #[error("Could not read the source: {0}")]
    Read(String),
    #[error("Could not write the copy: {0}")]
    Write(String),
    #[error("The source ends before the clip does.")]
    Short,
}

/// Copies source frames `from..to` of `src` to `dest` as a 32-bit float
/// WAV at the source's own rate and channel count. Returns the frames
/// written. A copy that would come up short is deleted, not left half
/// written.
pub fn copy_region(src: &Path, from: u64, to: u64, dest: &Path) -> Result<u64, RegionError> {
    let read = |e: crate::codecs::ProbeError| RegionError::Read(e.to_string());
    let probe = AudioReader::open(src, 1, None).map_err(read)?;
    let channels = probe.source_channels().max(1);
    let rate = probe.source_rate();
    drop(probe);
    let mut r = AudioReader::open(src, channels, None).map_err(read)?;
    if from > 0 {
        r.seek(from as f64 / f64::from(rate)).map_err(read)?;
    }
    let spec = hound::WavSpec {
        channels: channels as u16,
        sample_rate: rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let write = |e: hound::Error| RegionError::Write(e.to_string());
    let mut w = hound::WavWriter::create(dest, spec).map_err(write)?;
    let want = to.saturating_sub(from) * channels as u64;
    let mut done = 0u64;
    let mut buf = Vec::new();
    let result = (|| {
        while done < want {
            buf.clear();
            if !r.read_into(&mut buf).map_err(read)? {
                return Err(RegionError::Short);
            }
            let take = buf.len().min((want - done) as usize);
            for v in &buf[..take] {
                w.write_sample(*v).map_err(write)?;
            }
            done += take as u64;
        }
        Ok(())
    })();
    match result {
        Ok(()) => {
            w.finalize().map_err(write)?;
            Ok(done / channels as u64)
        }
        Err(e) => {
            drop(w);
            let _ = std::fs::remove_file(dest);
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_exactly_the_region_at_the_source_format() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("ramp.wav");
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 44_100,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(&src, spec).unwrap();
        for i in 0..44_100i32 {
            let v = (i % 1000) as i16 * 16;
            w.write_sample(v).unwrap();
            w.write_sample(-v).unwrap();
        }
        w.finalize().unwrap();

        let dest = dir.path().join("copy.wav");
        let n = copy_region(&src, 10_000, 30_000, &dest).unwrap();
        assert_eq!(n, 20_000);
        let mut rd = hound::WavReader::open(&dest).unwrap();
        assert_eq!(rd.spec().sample_rate, 44_100);
        assert_eq!(rd.spec().channels, 2);
        let v: Vec<f32> = rd.samples::<f32>().map(|x| x.unwrap()).collect();
        assert_eq!(v.len(), 40_000);
        // Frame 10_000 of the source is (10_000 % 1000) * 16 = 0, frame 10_500 is 500 * 16.
        assert!(v[0].abs() < 1e-4);
        assert!((v[1_000] - 8_000.0 / 32_768.0).abs() < 1e-4, "{}", v[1_000]);
        assert!((v[1_001] + 8_000.0 / 32_768.0).abs() < 1e-4);

        let short = dir.path().join("short.wav");
        assert!(matches!(
            copy_region(&src, 40_000, 50_000, &short),
            Err(RegionError::Short)
        ));
        assert!(!short.exists(), "a failed copy leaves nothing behind");
    }
}
