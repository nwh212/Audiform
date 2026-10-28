// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

//! Tests for the `record` module.

use super::*;

#[test]
fn routes_follow_the_track_input() {
    assert_eq!(
        route(None, 1),
        Some(Route {
            device: None,
            channels: vec![0]
        })
    );
    assert_eq!(
        route(None, 2),
        Some(Route {
            device: None,
            channels: vec![0, 1]
        })
    );
    assert_eq!(route(Some("none"), 2), None);
    assert_eq!(
        route(Some("Mono: Yeti Stereo Microphone 2"), 1),
        Some(Route {
            device: Some("Yeti Stereo Microphone".into()),
            channels: vec![1]
        })
    );
    assert_eq!(
        route(Some("Stereo: Scarlett 2i2 USB 1-2"), 2),
        Some(Route {
            device: Some("Scarlett 2i2 USB".into()),
            channels: vec![0, 1]
        })
    );
    // Garbage falls back to the default input.
    assert_eq!(
        route(Some("Mono: nothing"), 1),
        Some(Route {
            device: None,
            channels: vec![0]
        })
    );
}

#[test]
fn monitor_mixes_only_marked_targets_to_stereo() {
    let mk = |key: &str, monitor: bool| RecordTarget {
        key: key.into(),
        input: None,
        channels: 1,
        name: key.into(),
        monitor,
        chain: None,
    };
    assert!(MonitorMix::new(&[(mk("a", false), vec![0])], 3, 48_000).is_none());
    // Device has 3 channels: a mono mic on 0 (monitored), a stereo pair
    // on 1-2 (monitored), and nothing else.
    let mut m = MonitorMix::new(
        &[
            (mk("mic", true), vec![0]),
            (mk("pair", true), vec![1, 2]),
            (mk("quiet", false), vec![0]),
        ],
        3,
        48_000,
    )
    .unwrap();
    let mut out = Vec::new();
    m.mix(&[0.1, 0.2, 0.3, 0.9, 0.9, 0.9], &mut out);
    assert_eq!(out.len(), 4);
    assert!((out[0] - 0.3).abs() < 1e-6 && (out[1] - 0.4).abs() < 1e-6);
    assert_eq!(out[2], 1.0, "sums are clamped");
}

#[test]
fn monitor_applies_the_tracks_built_in_effects() {
    let chain = EffectChain {
        output_gain_db: -6.0206,
        ..EffectChain::default()
    };
    let target = RecordTarget {
        key: "mic".into(),
        input: None,
        channels: 1,
        name: "mic".into(),
        monitor: true,
        chain: Some(chain),
    };
    let mut m = MonitorMix::new(&[(target, vec![0])], 1, 48_000).unwrap();
    let mut out = Vec::new();
    m.mix(&[0.8, 0.8], &mut out);
    assert_eq!(out.len(), 4);
    assert!((out[0] - 0.4).abs() < 1e-3 && (out[1] - 0.4).abs() < 1e-3);
}

#[test]
fn live_tap_measures_each_target() {
    let mk = |key: &str| RecordTarget {
        key: key.into(),
        input: None,
        channels: 1,
        name: key.into(),
        monitor: false,
        chain: None,
    };
    let mut tap = LiveTap::new(&[(mk("a"), vec![0]), (mk("b"), vec![1])], 2);
    let mut live = HashMap::new();
    let mut frames = Vec::new();
    for i in 0..LIVE_BLOCK + 10 {
        frames.extend_from_slice(&[if i == 5 { 0.8 } else { 0.1 }, -0.25]);
    }
    tap.push(&frames, &mut live);
    assert!((live["a"].peak - 0.8).abs() < 1e-6);
    // One block of 32 frames per channel (min, max), as the peak cache would store it.
    assert_eq!(live["a"].channels, 1);
    assert_eq!(live["a"].blocks, vec![3277, 26214]);
    assert_eq!(live["b"].blocks, vec![-8192, -8192]);
}

#[test]
fn writer_splits_device_channels_into_takes() {
    let dir = tempfile::tempdir().unwrap();
    let mk = |key: &str| RecordTarget {
        key: key.into(),
        input: None,
        channels: 1,
        name: key.into(),
        monitor: false,
        chain: None,
    };
    let mut w = TakeWriter::create(
        dir.path(),
        &[
            (mk("left"), vec![0]),
            (mk("right"), vec![1]),
            (mk("both"), vec![0, 1]),
            (mk("missing"), vec![5]),
        ],
        48_000,
        2,
    )
    .unwrap();
    // Three stereo frames: (0.1, -0.1), (0.2, -0.2), (0.3, -0.3).
    w.push(&[0.1, -0.1, 0.2, -0.2, 0.3, -0.3]).unwrap();
    let takes = w.finish().unwrap();
    assert_eq!(takes.len(), 4);
    assert!(takes
        .iter()
        .all(|t| t.length == 3 && t.sample_rate == 48_000));
    let read = |t: &Take| -> Vec<f32> {
        hound::WavReader::open(&t.path)
            .unwrap()
            .samples::<f32>()
            .map(|s| s.unwrap())
            .collect()
    };
    assert_eq!(read(&takes[0]), vec![0.1, 0.2, 0.3]);
    assert_eq!(read(&takes[1]), vec![-0.1, -0.2, -0.3]);
    assert_eq!(read(&takes[2]), vec![0.1, -0.1, 0.2, -0.2, 0.3, -0.3]);
    assert_eq!(read(&takes[3]), vec![0.0, 0.0, 0.0]);
    assert_eq!(takes[2].channels, 2);
}

#[test]
fn tracks_armed_while_recording_get_their_own_takes() {
    let dir = tempfile::tempdir().unwrap();
    let mk = |key: &str| RecordTarget {
        key: key.into(),
        input: None,
        channels: 1,
        name: "Track".into(),
        monitor: false,
        chain: None,
    };
    let mut w = TakeWriter::create(dir.path(), &[], 48_000, 1).unwrap();
    w.push(&[0.1, 0.2]).unwrap();
    // Two tracks armed on the same input after two frames.
    w.add(&mk("a"), &[0], 2).unwrap();
    w.add(&mk("b"), &[0], 2).unwrap();
    w.push(&[0.3, 0.4]).unwrap();
    // One disarmed; the other keeps going.
    w.close("a").unwrap();
    assert!(!w.is_recording("a"));
    w.push(&[0.5]).unwrap();
    let takes = w.finish().unwrap();
    assert_eq!(takes.len(), 2);
    let a = takes.iter().find(|t| t.key == "a").unwrap();
    let b = takes.iter().find(|t| t.key == "b").unwrap();
    assert_ne!(a.path, b.path, "separate files for the same source");
    assert_eq!((a.start, a.length), (2, 2));
    assert_eq!((b.start, b.length), (2, 3));
}

#[test]
fn pads_system_audio_only_when_well_behind() {
    assert_eq!(silence_to_add(48_000, 48_000, 48_000), 0);
    assert_eq!(
        silence_to_add(52_000, 48_000, 48_000),
        0,
        "under 100 ms behind"
    );
    assert_eq!(silence_to_add(96_000, 48_000, 48_000), 48_000 - 2_400);
    assert_eq!(silence_to_add(0, 10, 48_000), 0);
}
