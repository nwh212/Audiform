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

/// Decodes a whole file at its own channel count and returns each
/// channel's pitch in Hz, counted from upward zero crossings.
fn channel_pitches(name: &str) -> Vec<f64> {
    use audiform_lib::codecs::AudioReader;
    let r = AudioReader::open(&media(name), 0, None).unwrap_or_else(|e| panic!("{name}: {e}"));
    let ch = r.source_channels();
    let mut r = AudioReader::open(&media(name), ch, None).unwrap();
    let (mut all, mut out) = (Vec::new(), Vec::new());
    while r.read_into(&mut out).unwrap() {
        all.append(&mut out);
    }
    let rate = f64::from(r.source_rate());
    let frames = all.len() / ch;
    // Skip the encoder's lead-in and tail.
    let (a, b) = (frames / 8, frames - frames / 8);
    (0..ch)
        .map(|c| {
            let ups = (a + 1..b)
                .filter(|&f| all[(f - 1) * ch + c] <= 0.0 && all[f * ch + c] > 0.0)
                .count();
            ups as f64 * rate / (b - a) as f64
        })
        .collect()
}

fn assert_pitches(name: &str, want: &[f64]) {
    let got = channel_pitches(name);
    assert_eq!(got.len(), want.len(), "{name}: {got:?}");
    for (g, w) in got.iter().zip(want) {
        assert!(
            (g - w).abs() <= w * 0.05 + 5.0,
            "{name}: {got:?}, want {want:?}"
        );
    }
}

// The surround files carry a different tone on each speaker: front left
// 200 Hz, front right 300, center 400, LFE 50, rear left 600, rear right
// 700 (and in 7.1, side left 800, side right 900). Channels come out in
// that order.

#[test]
fn aac_5_1() {
    let i = info("surround_51.m4a");
    assert_eq!((i.codec.as_str(), i.channels), ("AAC", 6));
    assert_about_half_second(&i, 0.05);
    assert_pitches(
        "surround_51.m4a",
        &[200.0, 300.0, 400.0, 50.0, 600.0, 700.0],
    );
}

#[test]
fn aac_7_1() {
    let i = info("surround_71.m4a");
    assert_eq!((i.codec.as_str(), i.channels), ("AAC", 8));
    assert_pitches(
        "surround_71.m4a",
        &[200.0, 300.0, 400.0, 50.0, 600.0, 700.0, 800.0, 900.0],
    );
}

/// Most download tools save M4A as fragmented MP4, whose header gives no
/// length.
#[test]
fn fragmented_m4a_has_its_length() {
    let i = info("fragmented.m4a");
    assert_eq!((i.codec.as_str(), i.channels), ("AAC", 2));
    assert_about_half_second(&i, 0.05);
}

/// Dolby Digital and Dolby Digital Plus (the codec of Dolby Atmos music in
/// M4A) open through the decoder built into Windows. Elsewhere the error
/// names the codec.
#[test]
fn dolby_digital_in_m4a() {
    for name in ["dolby_digital_51.m4a", "dolby_digital_plus_51.m4a"] {
        if cfg!(windows) {
            // Some Windows editions (Server, N) come without these decoders.
            let i = match probe(&media(name)) {
                Ok(i) => i,
                Err(e) => {
                    eprintln!("{name}: skipped, this system cannot decode it ({e})");
                    continue;
                }
            };
            assert_eq!(i.channels, 6, "{name}");
            assert_about_half_second(&i, 0.1);
            assert_pitches(name, &[200.0, 300.0, 400.0, 50.0, 600.0, 700.0]);
        } else {
            match probe(&media(name)) {
                Err(ProbeError::UnsupportedCodec(what)) => {
                    assert!(what.starts_with("Dolby Digital"), "{name}: {what}")
                }
                other => panic!("{name}: {other:?}"),
            }
        }
    }
}
