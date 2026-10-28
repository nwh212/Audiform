// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

//! Tests for the `model` module.

use super::*;

#[test]
fn space_settings_round_trip_and_stay_out_of_plain_files() {
    let mut s = Session::new("t", 48_000);
    s.tracks.push(Track::new("A", ChannelLayout::Stereo));
    let plain = serde_json::to_string(&s).unwrap();
    assert!(!plain.contains("space"));
    s.space = Some(Space {
        layout: SpaceLayout::Surround712,
        listen: SpaceListen::Binaural,
    });
    s.tracks[0].space = Some(SpacePos {
        x: -0.5,
        y: 0.25,
        z: 0.75,
        size: 0.1,
        width: 0.5,
        rotation: 30.0,
        lfe: 0.25,
        spread: 0.4,
        spatialize: 0.6,
        spatialize_mode: SpatializeMode::Ambience,
    });
    let text = serde_json::to_string(&s).unwrap();
    assert!(text.contains(r#""layout":"7.1.2""#));
    let back: Session = serde_json::from_str(&text).unwrap();
    assert_eq!(back, s);
}

#[test]
fn tempo_map_follows_changes_at_bars() {
    // Two bars of 4/4 at 120 (4 s), then 6/8 at 60.
    let t = Tempo {
        changes: vec![TempoChange {
            id: new_id(),
            bar: 2,
            bpm: 60.0,
            beats_per_bar: 6,
            beat_unit: 8,
        }],
        ..Tempo::default()
    };
    let segs: Vec<_> = t.segments().map(|s| (s.bar, s.beat, s.seconds)).collect();
    assert_eq!(segs, vec![(0, 0.0, 0.0), (2, 8.0, 4.0)]);
    assert_eq!(t.samples(8.0, 1_000), 4_000);
    // A quarter note at 60 is a second.
    assert_eq!(t.samples(11.0, 1_000), 7_000);
    assert!((t.beats(7_000, 1_000) - 11.0).abs() < 1e-9);
    assert_eq!(t.segment_at_seconds(3.9).bpm, 120.0);
    assert_eq!(t.segment_at_seconds(4.0).beat_unit, 8);
    // Files from before time signatures read as 4/4 with no changes.
    let old: Tempo = serde_json::from_str(r#"{"bpm":90,"beats_per_bar":3}"#).unwrap();
    assert_eq!((old.beat_unit, old.changes.len()), (4, 0));
}

#[test]
fn tidy_tempo_keeps_one_change_per_bar_in_order() {
    let c = |bar, bpm| TempoChange {
        id: new_id(),
        bar,
        bpm,
        beats_per_bar: 4,
        beat_unit: 4,
    };
    let t = Tempo {
        changes: vec![c(5, 90.0), c(0, 70.0), c(2, 100.0), c(5, 95.0)],
        beat_unit: 3,
        ..Tempo::default()
    }
    .tidy();
    let got: Vec<_> = t.changes.iter().map(|c| (c.bar, c.bpm)).collect();
    assert_eq!(got, vec![(2, 100.0), (5, 95.0)]);
    assert_eq!(t.beat_unit, 4);
    assert_eq!(tidy_beat_unit(6), 8);
    assert_eq!(tidy_beat_unit(64), 32);
}

#[test]
fn strip_effects_clears_every_chain() {
    let mut s = Session::new("t", 48_000);
    s.tracks.push(Track::new("A", ChannelLayout::Stereo));
    s.tracks[0].effects.input_gain_db = 6.0;
    s.master.effects.input_gain_db = 3.0;
    s.strip_effects();
    assert_eq!(s.tracks[0].effects, EffectChain::default());
    assert_eq!(s.master.effects, EffectChain::default());
}
