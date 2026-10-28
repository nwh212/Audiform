// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

//! Tests for the `transport` module.

use super::Jump;

#[test]
fn position_follows_loops_and_skips() {
    let lp = Some(Jump::Loop {
        start: 100,
        end: 200,
    });
    assert_eq!(Jump::position(lp, 50, 40), 90);
    assert_eq!(Jump::position(lp, 50, 150), 100);
    assert_eq!(Jump::position(lp, 50, 275), 125);
    // Starting after the loop plays straight on.
    assert_eq!(Jump::position(lp, 300, 10), 310);
    let sk = Some(Jump::Skip {
        start: 100,
        end: 200,
    });
    assert_eq!(Jump::position(sk, 50, 40), 90);
    assert_eq!(Jump::position(sk, 50, 60), 210);
    assert_eq!(Jump::position(None, 50, 60), 110);
    // Starting inside a skipped range starts after it.
    assert_eq!(
        Jump::Skip {
            start: 100,
            end: 200
        }
        .first(150),
        200
    );
}

use super::*;

#[test]
fn a_clip_level_is_heard_on_its_track_until_the_edit_takes_over() {
    let shared = Shared::new(2, &[[1.0, 1.0], [1.0, 1.0]], 1.0);
    let (mut p0, c0) = RingBuffer::<f32>::new(16);
    let (mut p1, c1) = RingBuffer::<f32>::new(16);
    for _ in 0..8 {
        p0.push(0.25).unwrap();
        p1.push(1.0).unwrap();
    }
    // Track 1 at half level over timeline frames 1..4; audio from
    // output frame 3 on was rendered with the edit already.
    shared.audition.set(0.5, 1, 4, 1);
    shared.audition.until.store(3, Ordering::Relaxed);
    let mut consumers = vec![c0, c1];
    let mut out = [0.0f32; 8];
    mix(
        &mut out,
        2,
        &mut consumers,
        &mut quiet(),
        &shared,
        &mut [[0.0; 2]; 2],
        &mut [[0.0; 2]; 2],
        &mut Routing::stereo(),
    );
    let left: Vec<f32> = out.iter().step_by(2).copied().collect();
    // The other track is untouched; the clip's track is scaled only
    // inside its span and before the edit's audio.
    assert_eq!(left, [1.25, 0.75, 0.75, 1.25]);
}

#[test]
fn mix_applies_gains_meters_and_counts_frames() {
    let shared = Shared::new(2, &[[1.0, 0.5], [0.0, 0.0]], 1.0);
    let (mut p0, c0) = RingBuffer::<f32>::new(16);
    let (mut p1, c1) = RingBuffer::<f32>::new(16);
    for v in [0.5, 0.5, -0.25, -0.25] {
        p0.push(v).unwrap();
    }
    for v in [0.9, 0.9, 0.9, 0.9] {
        p1.push(v).unwrap();
    }
    let mut consumers = vec![c0, c1];
    let mut gains = vec![[0.0; 2]; 2];
    let mut peaks = vec![[0.0; 2]; 2];
    let mut out = [9.0f32; 6];
    mix(
        &mut out,
        2,
        &mut consumers,
        &mut quiet(),
        &shared,
        &mut gains,
        &mut peaks,
        &mut Routing::stereo(),
    );
    // Two frames mixed, the third is silence because the rings ran dry.
    assert_eq!(out, [0.5, 0.25, -0.25, -0.125, 0.0, 0.0]);
    assert_eq!(shared.played.load(Ordering::Relaxed), 2);
    assert_eq!(shared.underruns.load(Ordering::Relaxed), 1);
    assert_eq!(bits_f32(shared.meters[0][0].load(Ordering::Relaxed)), 0.5);
    assert_eq!(
        bits_f32(shared.meters[1][0].load(Ordering::Relaxed)),
        0.0,
        "muted track shows no level"
    );
    assert_eq!(
        bits_f32(shared.master_meter[1].load(Ordering::Relaxed)),
        0.25
    );
}

/// A metronome ring with silence waiting.
fn quiet() -> Consumer<f32> {
    let (mut p, c) = RingBuffer::<f32>::new(64);
    while p.push(0.0).is_ok() {}
    c
}

#[test]
fn the_click_plays_after_the_master_fader_and_meters() {
    let shared = Shared::new(1, &[[1.0, 1.0]], 0.5);
    let (mut p, c) = RingBuffer::<f32>::new(4);
    p.push(0.4).unwrap();
    p.push(0.4).unwrap();
    let (mut cp, cc) = RingBuffer::<f32>::new(4);
    cp.push(0.3).unwrap();
    cp.push(0.3).unwrap();
    let mut out = [0f32; 2];
    mix(
        &mut out,
        2,
        &mut [c],
        &mut { cc },
        &shared,
        &mut [[0.0; 2]],
        &mut [[0.0; 2]],
        &mut Routing::stereo(),
    );
    assert_eq!(out, [0.5, 0.5]);
    assert_eq!(
        bits_f32(shared.master_meter[0].load(Ordering::Relaxed)),
        0.2
    );
}

#[test]
fn gain_updates_apply_without_restart() {
    let shared = Shared::new(1, &[[1.0, 1.0]], 1.0);
    shared.set_gains(&[[0.25, 0.0]], 2.0);
    let (mut p, c) = RingBuffer::<f32>::new(4);
    p.push(1.0).unwrap();
    p.push(1.0).unwrap();
    let mut consumers = vec![c];
    let mut out = [0f32; 2];
    mix(
        &mut out,
        2,
        &mut consumers,
        &mut quiet(),
        &shared,
        &mut [[0.0; 2]],
        &mut [[0.0; 2]],
        &mut Routing::stereo(),
    );
    assert_eq!(out, [0.5, 0.0]);
}

#[test]
fn mono_check_folds_what_plays() {
    let shared = Shared::new(1, &[[1.0, 1.0]], 1.0);
    let (mut p, c) = RingBuffer::<f32>::new(4);
    p.push(1.0).unwrap();
    p.push(-0.5).unwrap();
    let mut consumers = vec![c];
    let mut out = [0f32; 2];
    set_mono_check(true);
    mix(
        &mut out,
        2,
        &mut consumers,
        &mut quiet(),
        &shared,
        &mut [[0.0; 2]],
        &mut [[0.0; 2]],
        &mut Routing::stereo(),
    );
    set_mono_check(false);
    assert_eq!(out, [0.25, 0.25]);
}

#[test]
fn speakers_play_on_their_mapped_outputs() {
    let shared = Shared::new(1, &[[1.0, 1.0]], 1.0);
    let (mut p, c) = RingBuffer::<f32>::new(4);
    p.push(0.9).unwrap();
    p.push(0.9).unwrap();
    // Three speakers: L on output 3, R on 1, C on 2; output 4 unused.
    let (mut bp, bc) = RingBuffer::<f32>::new(6);
    for v in [0.1, 0.2, 0.3] {
        bp.push(v).unwrap();
    }
    let mut routing = Routing {
        stereo: [0, 1],
        bed: Some((bc, vec![2, 0, 1])),
        frame: vec![0.0; 3],
    };
    let mut out = [7f32; 4];
    mix(
        &mut out,
        4,
        &mut [c],
        &mut quiet(),
        &shared,
        &mut [[0.0; 2]],
        &mut [[0.0; 2]],
        &mut routing,
    );
    assert_eq!(out, [0.2, 0.3, 0.1, 0.0], "the stereo fold is not played");
}

#[test]
fn stereo_plays_on_the_picked_pair() {
    let shared = Shared::new(1, &[[1.0, 1.0]], 1.0);
    let (mut p, c) = RingBuffer::<f32>::new(4);
    p.push(0.3).unwrap();
    p.push(0.6).unwrap();
    let mut routing = Routing {
        stereo: [3, 2],
        ..Routing::default()
    };
    let mut out = [7f32; 4];
    mix(
        &mut out,
        4,
        &mut [c],
        &mut quiet(),
        &shared,
        &mut [[0.0; 2]],
        &mut [[0.0; 2]],
        &mut routing,
    );
    assert_eq!(out, [0.0, 0.0, 0.6, 0.3]);
}

#[test]
fn extra_device_channels_stay_silent() {
    let shared = Shared::new(1, &[[1.0, 1.0]], 1.0);
    let (mut p, c) = RingBuffer::<f32>::new(4);
    p.push(0.3).unwrap();
    p.push(0.6).unwrap();
    let mut out = [7f32; 4];
    mix(
        &mut out,
        4,
        &mut [c],
        &mut quiet(),
        &shared,
        &mut [[0.0; 2]],
        &mut [[0.0; 2]],
        &mut Routing::stereo(),
    );
    assert_eq!(out, [0.3, 0.6, 0.0, 0.0]);
}

#[test]
fn render_meters_wait_for_the_device_to_play_them() {
    let shared = Shared::new(2, &[[1.0, 1.0], [1.0, 1.0]], 1.0);
    let reading = |c: [f32; 4], bus: f32| GraphMeters {
        track_chains: vec![Some(c), None],
        buses: vec![[bus, bus]],
        ..GraphMeters::default()
    };
    shared.chain_meters.lock().unwrap().extend([
        (100, reading([0.5, 0.4, 0.2, 0.1], 0.3)),
        (200, reading([0.9, 0.9, 0.3, 0.3], 0.1)),
    ]);
    assert_eq!(
        shared.played_meters(),
        GraphMeters::default(),
        "nothing played yet"
    );
    shared.played.store(150, Ordering::Relaxed);
    assert_eq!(shared.played_meters(), reading([0.5, 0.4, 0.2, 0.1], 0.3));
    shared.played.store(400, Ordering::Relaxed);
    let m = shared.played_meters();
    assert_eq!(m.track_chains[0], Some([0.9, 0.9, 0.3, 0.3]));
    assert_eq!(m.buses[0], [0.1, 0.1]);
    assert_eq!(
        shared.played_meters(),
        m,
        "with nothing newly played, the last reading holds (no blink to silence)"
    );
}

#[test]
fn chain_updates_merge_until_picked_up() {
    let shared = Shared::new(1, &[[1.0, 1.0]], 1.0);
    let id = crate::session::model::new_id();
    let a = EffectChain {
        input_gain_db: 1.0,
        ..Default::default()
    };
    let b = EffectChain {
        input_gain_db: 2.0,
        ..Default::default()
    };
    shared.send_chains(ChainUpdate {
        tracks: vec![(id, a.clone())],
        master: Some(a),
        ..ChainUpdate::default()
    });
    shared.send_chains(ChainUpdate {
        tracks: vec![(id, b.clone())],
        buses: vec![(id, b.clone())],
        ..ChainUpdate::default()
    });
    let got = shared.take_chains().unwrap();
    assert_eq!(got.tracks.len(), 1);
    assert_eq!(got.tracks[0].1.input_gain_db, 2.0);
    assert_eq!(got.buses.len(), 1);
    assert_eq!(
        got.master.unwrap().input_gain_db,
        1.0,
        "kept until replaced"
    );
    assert!(shared.take_chains().is_none());
}

#[test]
fn idle_status() {
    let t = Transport::default();
    assert!(!t.status().playing);
    assert_eq!(t.stop(), None);
}
