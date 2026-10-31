// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

//! Tests for the `mixdown` module.

use super::*;
use crate::session::model::{new_id, ChannelLayout, MediaRef, Track};
use crate::session::ops::{apply, ClipSpec, Op};

fn tone(dir: &Path, name: &str, frames: u32, value: i16) -> std::path::PathBuf {
    let path = dir.join(name);
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 48_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(&path, spec).unwrap();
    for _ in 0..frames {
        w.write_sample(value).unwrap();
    }
    w.finalize().unwrap();
    path
}

#[test]
fn a_3d_session_writes_every_speaker_or_stereo_as_asked() {
    use crate::session::model::{Space, SpaceLayout, SpaceListen, SpacePos};
    let dir = tempfile::tempdir().unwrap();
    let (mut s, _, _) = session_with(dir.path());
    s.space = Some(Space {
        layout: SpaceLayout::Surround51,
        listen: SpaceListen::Stereo,
    });
    // A at the front left speaker, B at the back center.
    s.tracks[0].space = Some(SpacePos {
        x: -1.0,
        y: 1.0,
        lfe: 1.0,
        ..SpacePos::default()
    });
    s.tracks[1].space = Some(SpacePos {
        x: 0.0,
        y: -1.0,
        ..SpacePos::default()
    });
    let path = dir.path().join("bed.wav");
    let req = MixdownRequest {
        bit_depth: Some(32),
        ..MixdownRequest::default()
    };
    mixdown(&s, &req, &path, &AtomicBool::new(false), |_, _| {}).unwrap();
    let r = hound::WavReader::open(&path).unwrap();
    assert_eq!(r.spec().channels, 6);
    let x: Vec<f32> = r.into_samples::<f32>().map(Result::unwrap).collect();
    let at = |frame: usize, ch: usize| (x[frame * 6 + ch] * 1_000.0).round() / 1_000.0;
    assert_eq!((at(1_000, 0), at(1_000, 1), at(1_000, 2)), (0.25, 0.0, 0.0));
    // A's level is steady (no frequency at all), and the low-frequency
    // channel's 20 Hz high pass keeps such offsets out of the subwoofer.
    assert_eq!(at(10_000, 3), 0.0);
    assert_eq!(at(100_000, 3), 0.0);
    // B shares the two surrounds at -3 dB each.
    assert_eq!((at(100_000, 4), at(100_000, 5)), (0.177, 0.177));
    assert_eq!(at(100_000, 0), 0.0);
    // The same session to 7.1.4, then to stereo, then for headphones.
    for (layout, binaural, ch) in [
        (Some(SpaceLayout::Surround714), false, 12),
        (Some(SpaceLayout::Stereo20), false, 2),
        (None, true, 2),
    ] {
        let p = dir.path().join(format!("o{ch}{binaural}.wav"));
        let req = MixdownRequest {
            space_layout: layout,
            binaural,
            ..MixdownRequest::default()
        };
        mixdown(&s, &req, &p, &AtomicBool::new(false), |_, _| {}).unwrap();
        let r = hound::WavReader::open(&p).unwrap();
        assert_eq!(r.spec().channels, ch);
    }
}

#[test]
fn a_speaker_layout_can_be_flac_when_flac_can_carry_it() {
    use crate::export::encode::{FormatKind, OutputFormat};
    use crate::session::model::{Space, SpaceLayout, SpaceListen};
    let dir = tempfile::tempdir().unwrap();
    let (mut s, _, _) = session_with(dir.path());
    s.space = Some(Space {
        layout: SpaceLayout::Surround51,
        listen: SpaceListen::Stereo,
    });
    let flac = |layout| MixdownRequest {
        space_layout: Some(layout),
        format: Some(OutputFormat {
            kind: FormatKind::Flac,
            ..OutputFormat::default()
        }),
        ..MixdownRequest::default()
    };
    for (layout, ch) in [(SpaceLayout::Surround51, 6), (SpaceLayout::Surround71, 8)] {
        let p = dir.path().join(format!("o{ch}.flac"));
        mixdown(&s, &flac(layout), &p, &AtomicBool::new(false), |_, _| {}).unwrap();
        assert_eq!(crate::codecs::probe(&p).unwrap().channels, ch);
    }
    // Height speakers pass FLAC's 8 channels: refused with a reason.
    let p = dir.path().join("o12.flac");
    let e = mixdown(
        &s,
        &flac(SpaceLayout::Surround714),
        &p,
        &AtomicBool::new(false),
        |_, _| {},
    );
    assert!(e.unwrap_err().to_string().contains("WAV"));
    assert!(!p.exists());
}

#[test]
fn a_surround_file_plays_each_channel_from_its_speaker() {
    use crate::session::model::{Space, SpaceLayout, SpaceListen, Track};
    let dir = tempfile::tempdir().unwrap();
    // Six channels, each its own steady level.
    let path = dir.path().join("film.wav");
    let spec = hound::WavSpec {
        channels: 6,
        sample_rate: 48_000,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut w = hound::WavWriter::create(&path, spec).unwrap();
    for _ in 0..48_000 {
        for c in 0..6 {
            w.write_sample(0.1 * (c + 1) as f32).unwrap();
        }
    }
    w.finalize().unwrap();
    let mut s = Session::new("t", 48_000);
    s.space = Some(Space {
        layout: SpaceLayout::Surround51,
        listen: SpaceListen::Stereo,
    });
    let m = new_id();
    s.media.push(MediaRef {
        id: m,
        path,
        relative_path: None,
        sample_rate: 48_000,
        channels: 6,
        length: 48_000,
        linked: None,
    });
    let mut t = Track::new("Film", ChannelLayout::Stereo);
    t.clips.push(
        crate::session::ops::ClipSpec {
            id: new_id(),
            name: "film".into(),
            media_id: m,
            start: 0,
            source_in: 0,
            source_out: 48_000,
            channel_mix: None,
        }
        .build(),
    );
    s.tracks.push(t);
    let out = dir.path().join("bed.wav");
    let req = MixdownRequest {
        bit_depth: Some(32),
        ..MixdownRequest::default()
    };
    mixdown(&s, &req, &out, &AtomicBool::new(false), |_, _| {}).unwrap();
    let r = hound::WavReader::open(&out).unwrap();
    assert_eq!(r.spec().channels, 6);
    let x: Vec<f32> = r.into_samples::<f32>().map(Result::unwrap).collect();
    let at = |ch: usize| (x[20_000 * 6 + ch] * 1_000.0).round() / 1_000.0;
    // Each speaker holds its own channel and nothing else (the steady
    // LFE level is kept out by the subwoofer's 20 Hz high pass).
    assert_eq!(
        (at(0), at(1), at(2), at(3), at(4), at(5)),
        (0.1, 0.2, 0.3, 0.0, 0.5, 0.6)
    );
}

fn session_with(dir: &Path) -> (Session, Id, Id) {
    let mut s = Session::new("t", 48_000);
    s.bit_depth = 24;
    let path = tone(dir, "a.wav", 48_000, 8_192);
    let m = MediaRef {
        id: new_id(),
        path,
        relative_path: None,
        sample_rate: 48_000,
        channels: 1,
        length: 48_000,
        linked: None,
    };
    let mid = m.id;
    s.media.push(m);
    s.tracks.push(Track::new("A", ChannelLayout::Mono));
    s.tracks.push(Track::new("B", ChannelLayout::Mono));
    let (a, b) = (new_id(), new_id());
    for (ti, (id, start)) in [(a, 0u64), (b, 96_000u64)].into_iter().enumerate() {
        let tid = s.tracks[ti].id;
        apply(
            &mut s,
            &Op::AddClips {
                track_id: tid,
                clips: vec![ClipSpec {
                    id,
                    name: "c".into(),
                    media_id: mid,
                    start,
                    source_in: 0,
                    source_out: 48_000,
                    channel_mix: None,
                }],
            },
        )
        .unwrap();
    }
    (s, a, b)
}

#[test]
fn mixes_the_whole_session_at_the_session_depth() {
    let dir = tempfile::tempdir().unwrap();
    let (mut s, _, _) = session_with(dir.path());
    s.tracks[1].volume_db = -6.0;
    let out = dir.path().join("mix.wav");
    let mut last = (0, 0);
    let r = mixdown(
        &s,
        &MixdownRequest::default(),
        &out,
        &AtomicBool::new(false),
        |d, t| last = (d, t),
    )
    .unwrap();
    assert_eq!(r.frames, 144_000, "ends where the last clip ends");
    assert_eq!(last, (144_000, 144_000));
    let mut rd = hound::WavReader::open(&out).unwrap();
    assert_eq!(rd.spec().bits_per_sample, 24);
    assert_eq!(rd.spec().channels, 2);
    let v: Vec<i32> = rd.samples::<i32>().map(|x| x.unwrap()).collect();
    let at = |frame: usize| v[frame * 2] as f32 / 8_388_607.0;
    assert!((at(1_000) - 0.25).abs() < 0.01, "track A at full level");
    assert!(at(60_000).abs() < 0.001, "silence between clips");
    assert!((at(100_000) - 0.125).abs() < 0.01, "track B at -6 dB");
}

/// A mono 997 Hz sine at `peak`, five seconds long, on one track.
fn sine_session(dir: &Path, peak: f32) -> Session {
    let path = dir.join("sine.wav");
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 48_000,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut w = hound::WavWriter::create(&path, spec).unwrap();
    for i in 0..48_000 * 5 {
        let t = i as f32 / 48_000.0;
        w.write_sample(peak * (2.0 * std::f32::consts::PI * 997.0 * t).sin())
            .unwrap();
    }
    w.finalize().unwrap();
    let mut s = Session::new("t", 48_000);
    let m = MediaRef {
        id: new_id(),
        path,
        relative_path: None,
        sample_rate: 48_000,
        channels: 1,
        length: 240_000,
        linked: None,
    };
    let mid = m.id;
    s.media.push(m);
    s.tracks.push(Track::new("A", ChannelLayout::Mono));
    let tid = s.tracks[0].id;
    apply(
        &mut s,
        &Op::AddClips {
            track_id: tid,
            clips: vec![ClipSpec {
                id: new_id(),
                name: "c".into(),
                media_id: mid,
                start: 0,
                source_in: 0,
                source_out: 240_000,
                channel_mix: None,
            }],
        },
    )
    .unwrap();
    s
}

#[test]
fn normalizing_reaches_the_target_and_respects_the_peak_limit() {
    let dir = tempfile::tempdir().unwrap();
    let s = sine_session(dir.path(), 0.1);
    let run = |name: &str, normalize: Option<Normalize>| {
        mixdown(
            &s,
            &MixdownRequest {
                normalize,
                ..Default::default()
            },
            &dir.path().join(name),
            &AtomicBool::new(false),
            |_, _| {},
        )
        .unwrap()
    };
    let plain = run("plain.wav", None);
    let measured = crate::export::loudness::measure(
        &s,
        &MixdownRequest::default(),
        &AtomicBool::new(false),
        |_, _| {},
    )
    .unwrap();
    // Room to spare under the peak limit: the loudness reaches the target.
    let to_target = run(
        "a.wav",
        Some(Normalize {
            lufs: -16.0,
            peak: 0.0,
        }),
    );
    let expect = 10f32.powf(((-16.0 - measured.integrated) / 20.0) as f32);
    let got = to_target.peak / plain.peak;
    assert!((got - expect).abs() < expect * 0.02, "{got} vs {expect}");
    // A low peak limit holds the gain back, so the target is not reached.
    let limited = run(
        "b.wav",
        Some(Normalize {
            lufs: -3.0,
            peak: -12.0,
        }),
    );
    assert!(limited.peak <= 0.2515, "{}", limited.peak);
    assert!(limited.peak > plain.peak, "it still gets louder");
}

#[test]
fn a_realtime_mixdown_takes_as_long_as_the_audio() {
    let dir = tempfile::tempdir().unwrap();
    let (s, _a, _b) = session_with(dir.path());
    let req = MixdownRequest {
        range: Some((0, 24_000)),
        realtime: true,
        ..Default::default()
    };
    let at = std::time::Instant::now();
    let r = render_mix(&s, &req, &AtomicBool::new(false), |_, _| {}, |_| Ok(())).unwrap();
    assert_eq!(r.frames, 24_000);
    // Paced a block at a time: done no sooner than the audio, less the last block.
    assert!(
        at.elapsed() >= std::time::Duration::from_millis(250),
        "{:?}",
        at.elapsed()
    );
}

#[test]
fn mixes_a_range_or_selected_clips_and_can_cancel() {
    let dir = tempfile::tempdir().unwrap();
    let (s, _a, b) = session_with(dir.path());
    let out = dir.path().join("part.wav");
    let req = MixdownRequest {
        range: Some((90_000, 100_000)),
        clips: Some(vec![b]),
        bit_depth: Some(32),
        track: None,
        clip_only: false,
        dry: false,
        channels: None,
        format: None,
        realtime: false,
        space_layout: None,
        binaural: false,
        normalize: None,
    };
    let r = mixdown(&s, &req, &out, &AtomicBool::new(false), |_, _| {}).unwrap();
    assert_eq!(r.frames, 10_000);
    let v: Vec<f32> = hound::WavReader::open(&out)
        .unwrap()
        .samples::<f32>()
        .map(|x| x.unwrap())
        .collect();
    assert!(v[0].abs() < 1e-6 && (v[2 * 9_000] - 0.25).abs() < 0.01);

    let gone = dir.path().join("gone.wav");
    let r = mixdown(
        &s,
        &MixdownRequest::default(),
        &gone,
        &AtomicBool::new(true),
        |_, _| {},
    )
    .unwrap();
    assert!(r.cancelled);
    assert!(!gone.exists());
    assert!(matches!(
        mixdown(
            &s,
            &MixdownRequest {
                range: Some((5, 5)),
                ..Default::default()
            },
            &gone,
            &AtomicBool::new(false),
            |_, _| {}
        ),
        Err(MixdownError::Empty)
    ));
}

#[test]
fn a_stem_is_one_track_for_the_whole_session() {
    let dir = tempfile::tempdir().unwrap();
    let (mut s, _, _) = session_with(dir.path());
    s.tracks[0].muted = true;
    s.tracks[1].solo = true;
    let out = dir.path().join("stem.wav");
    let req = MixdownRequest {
        track: Some(s.tracks[0].id),
        bit_depth: Some(32),
        ..Default::default()
    };
    let r = mixdown(&s, &req, &out, &AtomicBool::new(false), |_, _| {}).unwrap();
    assert_eq!(r.frames, 144_000, "as long as the whole session");
    let v: Vec<f32> = hound::WavReader::open(&out)
        .unwrap()
        .samples::<f32>()
        .map(|x| x.unwrap())
        .collect();
    assert!((v[2 * 1_000] - 0.25).abs() < 0.01, "plays though muted");
    assert!(v[2 * 100_000].abs() < 1e-6, "other tracks left out");
}

#[test]
fn mixes_through_buses_and_their_chains() {
    let dir = tempfile::tempdir().unwrap();
    let (mut s, _, _) = session_with(dir.path());
    let mut bus = crate::session::model::Bus::new("Voices", ChannelLayout::Stereo);
    bus.volume_db = -6.0206;
    bus.effects.output_gain_db = -6.0206;
    let bid = bus.id;
    s.buses.push(bus);
    s.tracks[0].output = crate::session::model::Output::Bus { bus_id: bid };
    let out = dir.path().join("bus.wav");
    let req = MixdownRequest {
        range: Some((0, 48_000)),
        bit_depth: Some(32),
        ..MixdownRequest::default()
    };
    mixdown(&s, &req, &out, &AtomicBool::new(false), |_, _| {}).unwrap();
    let v: Vec<f32> = hound::WavReader::open(&out)
        .unwrap()
        .into_samples::<f32>()
        .map(|x| x.unwrap())
        .collect();
    // 0.25 through the bus chain (-6 dB) and the bus fader (-6 dB).
    assert!((v[2_000] - 0.0625).abs() < 0.002, "got {}", v[2_000]);
    // A stem of the routed track still goes through its bus.
    let req = MixdownRequest {
        range: Some((0, 48_000)),
        bit_depth: Some(32),
        track: Some(s.tracks[0].id),
        ..MixdownRequest::default()
    };
    s.buses[0].muted = true;
    mixdown(&s, &req, &out, &AtomicBool::new(false), |_, _| {}).unwrap();
    let v: Vec<f32> = hound::WavReader::open(&out)
        .unwrap()
        .into_samples::<f32>()
        .map(|x| x.unwrap())
        .collect();
    assert!((v[2_000] - 0.0625).abs() < 0.002, "stem got {}", v[2_000]);
}

#[test]
fn clip_only_leaves_out_track_and_master_settings() {
    let dir = tempfile::tempdir().unwrap();
    let (mut s, a, _) = session_with(dir.path());
    s.tracks[0].volume_db = -12.0;
    s.tracks[0].muted = true;
    s.master.volume_db = -6.0;
    s.tracks[0].clips[0].gain_db = -6.0;
    let out = dir.path().join("merged.wav");
    let req = MixdownRequest {
        range: Some((0, 48_000)),
        clips: Some(vec![a]),
        bit_depth: Some(32),
        track: Some(s.tracks[0].id),
        clip_only: true,
        dry: false,
        channels: Some(1),
        format: None,
        realtime: false,
        space_layout: None,
        binaural: false,
        normalize: None,
    };
    mixdown(&s, &req, &out, &AtomicBool::new(false), |_, _| {}).unwrap();
    let rd = hound::WavReader::open(&out).unwrap();
    assert_eq!(rd.spec().channels, 1, "mono on request");
    let v: Vec<f32> = rd.into_samples::<f32>().map(|x| x.unwrap()).collect();
    assert_eq!(v.len(), 48_000);
    // 0.25 with the clip's -6 dB only: about 0.125.
    assert!((v[1_000] - 0.125).abs() < 0.005, "got {}", v[1_000]);
}

#[test]
fn unique_path_counts_up_and_cleans_names() {
    let dir = tempfile::tempdir().unwrap();
    let first = unique_path(dir.path(), "Intro: Take/1", "wav");
    assert_eq!(first.file_name().unwrap(), "Intro_ Take_1.wav");
    std::fs::write(&first, b"x").unwrap();
    let second = unique_path(dir.path(), "Intro: Take/1", "wav");
    assert_eq!(second.file_name().unwrap(), "Intro_ Take_1 2.wav");
    assert_eq!(
        unique_path(dir.path(), " . ", "wav").file_name().unwrap(),
        "Merged.wav"
    );
}

#[test]
fn mixes_to_flac_and_mp3_at_another_rate() {
    use crate::export::encode::{FormatKind, OutputFormat};
    let dir = tempfile::tempdir().unwrap();
    let (s, _, _) = session_with(dir.path());
    for kind in [FormatKind::Flac, FormatKind::Mp3] {
        let out = dir.path().join(format!("mix.{}", kind.extension()));
        let req = MixdownRequest {
            format: Some(OutputFormat {
                kind,
                sample_rate: Some(44_100),
                ..Default::default()
            }),
            ..Default::default()
        };
        let r = mixdown(&s, &req, &out, &AtomicBool::new(false), |_, _| {}).unwrap();
        assert_eq!(r.frames, 144_000, "frames rendered at the session rate");
        let info = crate::codecs::probe(&out).unwrap();
        assert_eq!(info.sample_rate, 44_100);
        let secs = info.length as f64 / 44_100.0;
        assert!((secs - 3.0).abs() < 0.1, "{kind:?}: {secs} s");
    }
}

/// Runs an outside tool. None when it is not installed, unless the CI
/// run says it must be.
fn tool(name: &str, args: &[&std::ffi::OsStr]) -> Option<std::process::Output> {
    match std::process::Command::new(name).args(args).output() {
        Ok(o) => Some(o),
        Err(_) => {
            assert!(
                std::env::var_os("AUDIFORM_REQUIRE_TOOLS").is_none(),
                "{name} must be installed for this test"
            );
            None
        }
    }
}

#[test]
fn every_format_passes_our_check_and_the_outside_tools() {
    use crate::export::chapters::{embed_flac, embed_mp3, embed_wav, Chapter};
    use crate::export::encode::{FormatKind, OutputFormat};
    use crate::export::m4a_tags::write_m4a_chapters;
    use crate::export::verify::{verify, Expect};
    use std::ffi::OsStr;
    let dir = tempfile::tempdir().unwrap();
    let (s, _, _) = session_with(dir.path());
    let chapters = vec![
        Chapter {
            title: "Intro".into(),
            start_ms: 0,
            end_ms: None,
            url: None,
        },
        Chapter {
            title: "Middle part".into(),
            start_ms: 1_500,
            end_ms: None,
            url: None,
        },
        Chapter {
            title: "Outro".into(),
            start_ms: 2_700,
            end_ms: None,
            url: None,
        },
    ];
    for kind in [
        FormatKind::Wav,
        FormatKind::Flac,
        FormatKind::Mp3,
        FormatKind::M4a,
    ] {
        // The Windows AAC encoder crashed the test process on a three second file
        // (access violation), so it only runs when asked: AUDIFORM_TEST_AAC=1.
        if kind == FormatKind::M4a
            && cfg!(windows)
            && std::env::var_os("AUDIFORM_TEST_AAC").is_none()
        {
            continue;
        }
        let out = dir.path().join(format!("mix.{}", kind.extension()));
        let req = MixdownRequest {
            format: Some(OutputFormat {
                kind,
                ..Default::default()
            }),
            ..Default::default()
        };
        let r = match mixdown(&s, &req, &out, &AtomicBool::new(false), |_, _| {}) {
            Ok(r) => r,
            // AAC is written by the operating system's encoder; not every system has it.
            Err(_) if kind == FormatKind::M4a => continue,
            Err(e) => panic!("{kind:?}: {e}"),
        };
        match kind {
            FormatKind::Wav => embed_wav(&out, &chapters).unwrap(),
            FormatKind::Flac => embed_flac(&out, &chapters).unwrap(),
            FormatKind::Mp3 => embed_mp3(&out, &chapters, 3_000).unwrap(),
            FormatKind::M4a => write_m4a_chapters(&out, &chapters).unwrap(),
        }
        let rep = verify(
            &out,
            &Expect {
                frames: r.frames,
                sample_rate: r.sample_rate,
                chapters: chapters.clone(),
            },
        );
        assert!(rep.ok, "{kind:?}: {rep:?}");

        if kind == FormatKind::Flac {
            if let Some(o) = tool(
                "flac",
                &[OsStr::new("-t"), OsStr::new("--silent"), out.as_os_str()],
            ) {
                assert!(
                    o.status.success(),
                    "flac -t: {}",
                    String::from_utf8_lossy(&o.stderr)
                );
            }
        }
        let args = [
            OsStr::new("-v"),
            OsStr::new("error"),
            OsStr::new("-show_entries"),
            OsStr::new("format=duration:stream=codec_type"),
            OsStr::new("-show_chapters"),
            OsStr::new("-of"),
            OsStr::new("json"),
            out.as_os_str(),
        ];
        if let Some(o) = tool("ffprobe", &args) {
            let text = String::from_utf8_lossy(&o.stdout).into_owned();
            assert!(
                o.status.success() && o.stderr.is_empty(),
                "{kind:?} ffprobe: {} {text}",
                String::from_utf8_lossy(&o.stderr)
            );
            assert!(text.contains("audio"), "{kind:?}: {text}");
            // ffprobe reads chapters from MP3, FLAC and M4A, but takes no cue points as chapters.
            if kind != FormatKind::Wav {
                assert_eq!(
                    text.matches("\"start_time\"").count(),
                    3,
                    "{kind:?}: {text}"
                );
                assert!(text.contains("Middle part"), "{kind:?}: {text}");
            }
        }
    }
}
