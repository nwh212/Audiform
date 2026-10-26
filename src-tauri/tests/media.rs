// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Probes one small file per supported format. The files in
//! tests/media are half-second 440 Hz tones made with ffmpeg.

use std::path::PathBuf;

use audiform_lib::codecs::{probe, AudioInfo, ProbeError};

fn media(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("media")
        .join(name)
}

fn info(name: &str) -> AudioInfo {
    probe(&media(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// Lossy codecs add encoder delay and padding, so allow a little slack.
fn assert_about_half_second(i: &AudioInfo, slack: f64) {
    let secs = i.duration_seconds();
    assert!((secs - 0.5).abs() <= slack, "{}: {secs} s", i.name);
}

#[test]
fn wav_16_bit_stereo() {
    let i = info("stereo_48k_16.wav");
    assert_eq!((i.format.as_str(), i.codec.as_str()), ("WAV", "PCM 16-bit"));
    assert_eq!(
        (i.sample_rate, i.channels, i.bit_depth),
        (48_000, 2, Some(16))
    );
    assert_eq!(i.length, 24_000);
    assert!(i.file_size > 0);
}

#[test]
fn wav_24_bit_mono() {
    let i = info("mono_44k_24.wav");
    assert_eq!(i.codec, "PCM 24-bit");
    assert_eq!(
        (i.sample_rate, i.channels, i.bit_depth),
        (44_100, 1, Some(24))
    );
    assert_eq!(i.length, 22_050);
}

#[test]
fn aiff() {
    let i = info("tone.aiff");
    assert_eq!(i.format, "AIFF");
    assert_eq!((i.sample_rate, i.channels), (48_000, 1));
    assert_eq!(i.length, 24_000);
}

#[test]
fn flac() {
    let i = info("tone.flac");
    assert_eq!((i.format.as_str(), i.codec.as_str()), ("FLAC", "FLAC"));
    assert_eq!((i.sample_rate, i.channels), (48_000, 1));
    assert_eq!(i.length, 24_000);
}

#[test]
fn mp3_with_length_header() {
    let i = info("tone.mp3");
    assert_eq!((i.format.as_str(), i.codec.as_str()), ("MP3", "MP3"));
    assert_eq!((i.sample_rate, i.channels, i.bit_depth), (48_000, 2, None));
    assert_about_half_second(&i, 0.06);
}

#[test]
fn mp3_without_length_header_is_measured_by_scanning() {
    let i = info("tone_noxing.mp3");
    assert_eq!(i.sample_rate, 48_000);
    assert_about_half_second(&i, 0.06);
}

#[test]
fn m4a_aac() {
    let i = info("tone.m4a");
    assert_eq!((i.format.as_str(), i.codec.as_str()), ("M4A", "AAC"));
    assert_eq!((i.sample_rate, i.channels), (48_000, 1));
    assert_about_half_second(&i, 0.06);
}

#[test]
fn ogg_vorbis() {
    let i = info("tone.ogg");
    assert_eq!((i.format.as_str(), i.codec.as_str()), ("OGG", "Vorbis"));
    assert_eq!((i.sample_rate, i.channels), (48_000, 1));
    assert_about_half_second(&i, 0.06);
}

#[test]
fn ogg_opus() {
    let i = info("tone.opus");
    assert_eq!(i.codec.as_str(), "Opus");
    assert_eq!((i.sample_rate, i.channels), (48_000, 1));
    assert_about_half_second(&i, 0.06);
}

#[test]
fn truncated_file_is_reported_not_crashed() {
    let dir = tempfile::tempdir().unwrap();
    let cut = dir.path().join("cut.flac");
    let bytes = std::fs::read(media("tone.flac")).unwrap();
    std::fs::write(&cut, &bytes[..60]).unwrap();
    match probe(&cut) {
        Err(ProbeError::Damaged(_)) | Err(ProbeError::UnknownFormat) => {}
        other => panic!("expected an error, got {other:?}"),
    }
}

mod reader {
    use super::media;
    use audiform_lib::codecs::AudioReader;

    fn read_all(r: &mut AudioReader) -> Vec<f32> {
        let mut out = Vec::new();
        while r.read_into(&mut out).unwrap() {}
        out
    }

    #[test]
    fn reads_every_sample_at_the_file_rate() {
        let mut r = AudioReader::open(&media("stereo_48k_16.wav"), 2, None).unwrap();
        assert_eq!(
            (r.source_rate(), r.source_channels(), r.out_rate()),
            (48_000, 2, 48_000)
        );
        let samples = read_all(&mut r);
        assert_eq!(samples.len(), 24_000 * 2);
        let peak = samples.iter().fold(0f32, |m, s| m.max(s.abs()));
        assert!(peak > 0.05 && peak <= 1.0, "peak {peak}");
    }

    #[test]
    fn upmixes_mono_and_resamples() {
        let mut r = AudioReader::open(&media("mono_44k_24.wav"), 2, Some(48_000)).unwrap();
        let samples = read_all(&mut r);
        let frames = samples.len() / 2;
        // Half a second at 48 kHz plus the padded tail.
        assert!(
            (24_000..24_000 + 4_096).contains(&frames),
            "{frames} frames"
        );
        // Mono feeds both channels equally.
        let mid = &samples[20_000..20_010];
        for pair in mid.as_chunks::<2>().0 {
            assert!((pair[0] - pair[1]).abs() < 1e-6);
        }
    }

    #[test]
    fn decodes_every_lossy_format() {
        for name in [
            "tone.mp3",
            "tone.m4a",
            "tone.ogg",
            "tone.opus",
            "tone.flac",
            "tone.aiff",
        ] {
            let mut r = AudioReader::open(&media(name), 2, Some(48_000)).unwrap();
            let samples = read_all(&mut r);
            let secs = samples.len() as f64 / 2.0 / 48_000.0;
            assert!(secs > 0.45 && secs < 0.7, "{name}: {secs} s");
        }
    }

    /// Opus decodes to the right tone (440 Hz at 1/16 scale, as made by
    /// ffmpeg's sine source at half volume), not just the right length.
    #[test]
    fn opus_decodes_the_tone() {
        let mut r = AudioReader::open(&media("tone.opus"), 1, None).unwrap();
        let s = read_all(&mut r);
        let mid = &s[4_800..19_200];
        let rms = (mid.iter().map(|v| v * v).sum::<f32>() / mid.len() as f32).sqrt();
        assert!((rms - 0.0442).abs() < 0.004, "rms {rms}");
        let crossings = mid
            .windows(2)
            .filter(|w| (w[0] < 0.0) != (w[1] < 0.0))
            .count();
        // 0.3 s of 440 Hz crosses zero 264 times.
        assert!((250..=280).contains(&crossings), "{crossings} crossings");
    }

    #[test]
    fn seeks() {
        let mut r = AudioReader::open(&media("stereo_48k_16.wav"), 2, None).unwrap();
        let at = r.seek(0.25).unwrap();
        assert!((at - 0.25).abs() < 0.01, "{at}");
        let rest = read_all(&mut r);
        let frames = rest.len() / 2;
        assert!((11_500..=12_500).contains(&frames), "{frames}");
    }
}

mod alignment {
    use audiform_lib::codecs::AudioReader;

    /// A click at 0.1 s stays at 0.1 s after resampling to 48 kHz.
    #[test]
    fn resampling_keeps_events_in_place() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("click.wav");
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 44_100,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(&path, spec).unwrap();
        for i in 0..22_050 {
            w.write_sample(if i == 4_410 { i16::MAX } else { 0 })
                .unwrap();
        }
        w.finalize().unwrap();

        let mut r = AudioReader::open(&path, 1, Some(48_000)).unwrap();
        let mut out = Vec::new();
        while r.read_into(&mut out).unwrap() {}
        let peak = out
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
            .map(|(i, _)| i)
            .unwrap();
        assert!(
            (4_797..=4_803).contains(&peak),
            "click at {peak}, expected 4800"
        );

        // Seeking lands on the exact frame, not the start of a packet.
        let mut r = AudioReader::open(&path, 1, None).unwrap();
        r.seek(0.05).unwrap();
        let mut out = Vec::new();
        while r.read_into(&mut out).unwrap() {}
        let at = out.iter().position(|v| *v > 0.5).unwrap();
        assert_eq!(at, 2_205);
    }
}
