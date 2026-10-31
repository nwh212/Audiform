// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

//! Tests for the `graph` module.

use super::*;
use crate::engine::plan;
use crate::session::model::{new_id, Bus, EffectSlot, MediaRef, Output, Send, Track};
use crate::session::ops::{apply, ClipSpec, Op};
use std::path::{Path, PathBuf};

const RATE: u32 = 48_000;

/// Steady-state gain of the low-frequency channel's filters for a tone.
fn lfe_gain(freq: f64) -> f64 {
    let mut f = lfe_filters(RATE);
    let n = RATE as usize;
    let mut peak = 0.0f64;
    for i in 0..n {
        let x = (2.0 * std::f64::consts::PI * freq * i as f64 / RATE as f64).sin() as f32;
        let y = f.iter_mut().fold(x, |v, b| b.run(v));
        if i > n / 2 {
            peak = peak.max(f64::from(y.abs()));
        }
    }
    peak
}

#[test]
fn lfe_keeps_only_the_low_band() {
    assert!(lfe_gain(60.0) > 0.9, "60 Hz passes");
    assert!(lfe_gain(500.0) < 0.001, "500 Hz is cut by about 60 dB");
    assert!(
        lfe_gain(240.0) < 0.01,
        "an octave above is down about 48 dB"
    );
    assert!(lfe_gain(8.0) < 0.05, "rumble below hearing is cut");
}

/// One second, mono: `f(i)` for each frame.
fn wav(dir: &Path, name: &str, f: impl Fn(usize) -> f32) -> PathBuf {
    let path = dir.join(name);
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(&path, spec).unwrap();
    for i in 0..RATE as usize {
        w.write_sample((f(i).clamp(-1.0, 1.0) * 32_767.0) as i16)
            .unwrap();
    }
    w.finalize().unwrap();
    path
}

fn click(dir: &Path) -> PathBuf {
    wav(dir, "click.wav", |i| if i == 1_000 { 1.0 } else { 0.0 })
}

/// Adds a track playing `path` from source frame `source_in`, placed at `start`.
fn track(s: &mut Session, path: &Path, start: u64, source_in: u64, len: u64) -> usize {
    let m = MediaRef {
        id: new_id(),
        path: path.into(),
        relative_path: None,
        sample_rate: RATE,
        channels: 1,
        length: u64::from(RATE),
        linked: None,
    };
    let mid = m.id;
    s.media.push(m);
    s.tracks.push(Track::new("t", ChannelLayout::Stereo));
    let tid = s.tracks.last().unwrap().id;
    apply(
        s,
        &Op::AddClips {
            track_id: tid,
            clips: vec![ClipSpec {
                id: new_id(),
                name: "c".into(),
                media_id: mid,
                start,
                source_in,
                source_out: source_in + len,
                channel_mix: None,
            }],
        },
    )
    .unwrap();
    s.tracks.len() - 1
}

fn chain(effect: &str, params: &[(&str, f64)]) -> EffectChain {
    let mut c = EffectChain::default();
    c.slots.push(EffectSlot {
        id: new_id(),
        effect: effect.into(),
        enabled: true,
        params: params.iter().map(|(k, v)| ((*k).to_string(), *v)).collect(),
        preset: None,
        sidechain: None,
        plugin_state: None,
        data: None,
        position: None,
        routing: None,
    });
    c
}

fn limiter(ms: f64) -> EffectChain {
    chain("altaudio.limiter", &[("ceiling", 0.0), ("lookahead", ms)])
}

fn mix(s: &Session, routed: bool, from: u64) -> Mix {
    Mix::new(
        Renderer::new(plan::build(s, RATE), from),
        Graph::new(s, RATE, routed),
    )
}

/// Renders `frames` frames from `from`; one buffer per output.
fn render(s: &Session, routed: bool, from: u64, frames: usize) -> Vec<Vec<f32>> {
    let mut m = mix(s, routed, from);
    let mut out = vec![Vec::new(); m.outputs()];
    let mut all = vec![Vec::new(); m.outputs()];
    let mut done = 0;
    while done < frames {
        let got = m.render(1_024.min(frames - done), &mut out);
        for (a, o) in all.iter_mut().zip(&out) {
            a.extend_from_slice(o);
        }
        done += got;
    }
    all
}

fn clicks(buf: &[f32], over: f32) -> Vec<usize> {
    buf.as_chunks::<2>()
        .0
        .iter()
        .enumerate()
        .filter(|(_, f)| f[0].abs() > over)
        .map(|(i, _)| i)
        .collect()
}

fn peak(buf: &[f32], from: usize, to: usize) -> f32 {
    buf[from * 2..to * 2]
        .iter()
        .fold(0f32, |m, v| m.max(v.abs()))
}

#[test]
fn track_chains_keep_time_with_look_ahead() {
    let dir = tempfile::tempdir().unwrap();
    let path = click(dir.path());
    for (a, b) in [
        (limiter(5.0), EffectChain::default()),
        (limiter(5.0), limiter(2.0)),
    ] {
        let mut s = Session::new("t", RATE);
        // Click at timeline 5,500 on both tracks.
        track(&mut s, &path, 5_000, 500, 10_000);
        track(&mut s, &path, 5_000, 500, 10_000);
        s.tracks[0].effects = a;
        s.tracks[1].effects = b;
        s.tracks[1].clips[0].effects = limiter(1.0);
        for routed in [false, true] {
            let out = render(&s, routed, 0, 12_000);
            for o in &out {
                assert_eq!(clicks(o, 0.5), vec![5_500], "routed {routed}");
            }
            let out = render(&s, routed, 5_200, 1_024);
            assert_eq!(clicks(&out[0], 0.5), vec![300], "starting mid-clip");
        }
    }
    // A gain effect on the track halves the click.
    let mut s = Session::new("t", RATE);
    track(&mut s, &path, 5_000, 500, 10_000);
    s.tracks[0].effects = chain("altaudio.gain", &[("gain", -6.0206)]);
    let out = render(&s, false, 5_000, 1_024);
    assert!((peak(&out[0], 0, 1_024) - 0.5).abs() < 0.01);
}

#[test]
fn buses_sum_their_inputs_in_step_with_direct_tracks() {
    let dir = tempfile::tempdir().unwrap();
    let path = click(dir.path());
    let mut s = Session::new("t", RATE);
    let direct = track(&mut s, &path, 5_000, 500, 10_000);
    let routed = track(&mut s, &path, 5_000, 500, 10_000);
    let sent = track(&mut s, &path, 5_000, 500, 10_000);
    s.tracks[direct].effects = limiter(1.0);
    // A bus with 5 ms of look-ahead and -6 dB, and a send-only reverb bus.
    let mut bus = Bus::new("Voices", ChannelLayout::Stereo);
    bus.effects = limiter(5.0);
    bus.volume_db = -6.0206;
    let verb = Bus::new("Reverb", ChannelLayout::Stereo);
    let (bus_id, verb_id) = (bus.id, verb.id);
    s.buses.push(bus);
    s.buses.push(verb);
    s.tracks[routed].output = Output::Bus { bus_id };
    s.tracks[sent].volume_db = -96.0;
    s.tracks[sent].sends.push(Send {
        id: new_id(),
        bus_id: verb_id,
        level_db: -6.0206,
        pan: 100.0,
        pre_fader: true,
        enabled: true,
    });
    assert!(crate::session::routing::is_routed(&s));
    let out = render(&s, true, 0, 12_000);
    // Every path lands on frame 5,500: 1 (direct) + 0.5 (bus) +
    // 0.5 on the right only (pre-fader send, panned right).
    let f = |i: usize| (out[0][i * 2], out[0][i * 2 + 1]);
    let (l, r) = f(5_500);
    assert!((l - 1.5).abs() < 0.01, "left {l}");
    assert!((r - 2.0).abs() < 0.01, "right {r}");
    assert_eq!(clicks(&out[0], 0.01), vec![5_500]);

    // Meters: the bus after its fader.
    let mut m = mix(&s, true, 5_000);
    let mut o = vec![Vec::new()];
    m.render(1_024, &mut o);
    let meters = m.graph().take_meters();
    assert!((meters.buses[0][0] - 0.5).abs() < 0.01, "{meters:?}");
    assert!(meters.bus_chains[0].is_some());
}

#[test]
fn soloing_a_bus_plays_what_feeds_it() {
    let mut s = Session::new("t", RATE);
    for _ in 0..3 {
        s.tracks.push(Track::new("t", ChannelLayout::Stereo));
    }
    s.buses.push(Bus::new("b", ChannelLayout::Stereo));
    s.buses.push(Bus::new("verb", ChannelLayout::Stereo));
    let (b, verb) = (s.buses[0].id, s.buses[1].id);
    s.tracks[0].output = Output::Bus { bus_id: b };
    s.buses[0].solo = true;
    let (t, bs) = audible(&s);
    assert_eq!(t, vec![true, false, false]);
    assert_eq!(bs, vec![true, false]);
    // A soloed track is heard through its send's bus.
    s.buses[0].solo = false;
    s.tracks[1].solo = true;
    s.tracks[1].sends.push(Send {
        id: new_id(),
        bus_id: verb,
        level_db: 0.0,
        pan: 0.0,
        pre_fader: false,
        enabled: true,
    });
    let (t, bs) = audible(&s);
    assert_eq!(t, vec![false, true, false]);
    assert_eq!(bs, vec![false, true]);
    s.tracks[1].muted = true;
    assert_eq!(audible(&s).0, vec![false, false, false]);
    // Levels: muted means silent sends too, polarity flips both.
    s.tracks[1].muted = false;
    s.tracks[1].polarity_inverted = true;
    s.tracks[1].volume_db = -6.0206;
    let lv = levels(&s);
    assert!((lv.tracks[1].main[0] + 0.5).abs() < 1e-3);
    assert!(
        (lv.tracks[1].sends[0][0] + 0.5).abs() < 1e-3,
        "post-fader send"
    );
    assert_eq!(lv.tracks[0].main, [0.0, 0.0]);
}

#[test]
fn sidechain_ducks_music_under_the_voice() {
    let dir = tempfile::tempdir().unwrap();
    let tone = wav(dir.path(), "tone.wav", |i| {
        (i as f32 * 220.0 * std::f32::consts::TAU / RATE as f32).sin() * 0.5
    });
    let burst = wav(dir.path(), "burst.wav", |i| {
        if (24_000..36_000).contains(&i) {
            0.5
        } else {
            0.0
        }
    });
    let mut s = Session::new("t", RATE);
    let voice = track(&mut s, &burst, 0, 0, 48_000);
    let music = track(&mut s, &tone, 0, 0, 48_000);
    let mut duck = chain(
        "altaudio.dynamics",
        &[
            ("threshold", -30.0),
            ("ratio", 20.0),
            ("attack", 0.1),
            ("release", 20.0),
            ("knee", 0.0),
        ],
    );
    duck.slots[0].sidechain = Some(s.tracks[voice].id);
    s.tracks[music].effects = duck;
    // A look-ahead on the voice: the key must still line up.
    s.tracks[voice].effects = limiter(5.0);
    // The key is taken before the fader, so a muted voice still ducks.
    s.tracks[voice].muted = true;
    assert!(crate::session::routing::is_routed(&s));
    let out = &render(&s, true, 0, 48_000)[0];
    let before = peak(out, 10_000, 20_000);
    let during = peak(out, 25_000, 35_000);
    let after = peak(out, 42_000, 47_000);
    assert!((before - 0.5).abs() < 0.02, "untouched before: {before}");
    assert!(during < 0.06, "ducked under the voice: {during}");
    assert!((after - 0.5).abs() < 0.02, "back after: {after}");
    // The duck starts with the voice, not before it.
    assert!(peak(out, 23_000, 23_990) > 0.45);
}

#[test]
fn levels_change_live_and_ramp() {
    let dir = tempfile::tempdir().unwrap();
    let dc = wav(dir.path(), "dc.wav", |_| 0.5);
    let mut s = Session::new("t", RATE);
    track(&mut s, &dc, 0, 0, 48_000);
    s.buses.push(Bus::new("b", ChannelLayout::Stereo));
    s.tracks[0].output = Output::Bus {
        bus_id: s.buses[0].id,
    };
    let mut m = mix(&s, true, 0);
    let mut o = vec![Vec::new()];
    m.render(1_000, &mut o);
    assert!((o[0][1_998] - 0.5).abs() < 1e-3);
    s.buses[0].volume_db = -6.0206;
    m.graph().set_levels(levels(&s));
    m.render(1_000, &mut o);
    assert!((o[0][998] - 0.375).abs() < 0.01, "halfway down the ramp");
    assert!((o[0][1_998] - 0.25).abs() < 1e-3);
    m.render(1_000, &mut o);
    assert!((o[0][0] - 0.25).abs() < 1e-3, "and it stays there");
}

#[test]
fn mono_folds_a_track_after_its_chain_and_before_its_pan() {
    let dir = tempfile::tempdir().unwrap();
    let dc = wav(dir.path(), "dc.wav", |_| 0.5);
    let mut s = Session::new("t", RATE);
    track(&mut s, &dc, 0, 0, 48_000);
    // Hard left before the fold: the right side is silent.
    s.tracks[0].effects = chain("altaudio.gain", &[("gain", 0.0)]);
    let mut g = Graph::new(&s, RATE, true);
    let raw = vec![[0.5f32, 0.0].repeat(64)];
    let mut out = vec![Vec::new()];
    g.process(&raw, &mut out);
    assert_eq!((out[0][10], out[0][11]), (0.5, 0.0));
    s.tracks[0].mono = true;
    g.set_levels(levels(&s));
    g.process(&raw, &mut out);
    assert_eq!(
        (out[0][10], out[0][11]),
        (0.25, 0.25),
        "folded live, no rebuild"
    );
    // Pan still places the folded track.
    s.tracks[0].pan = -100.0;
    g.set_levels(levels(&s));
    g.process(&raw, &mut out);
    g.process(&raw, &mut out);
    assert_eq!((out[0][10], out[0][11]), (0.25, 0.0));
}

#[test]
fn chains_swap_in_while_playing() {
    let dir = tempfile::tempdir().unwrap();
    let path = click(dir.path());
    let mut s = Session::new("t", RATE);
    track(&mut s, &path, 5_000, 500, 10_000);
    s.buses.push(Bus::new("b", ChannelLayout::Stereo));
    let (tid, bid) = (s.tracks[0].id, s.buses[0].id);
    s.tracks[0].output = Output::Bus { bus_id: bid };
    s.tracks[0].effects = limiter(5.0);
    s.buses[0].effects = limiter(5.0);
    let lim = |ceiling: f64| {
        chain(
            "altaudio.limiter",
            &[("ceiling", ceiling), ("lookahead", 5.0)],
        )
    };
    let play = |swap: Option<ChainUpdate>| {
        let mut m = mix(&s, true, 0);
        let mut out = vec![Vec::new()];
        let mut all = Vec::new();
        for block in 0..12 {
            if block == 5 {
                if let Some(u) = &swap {
                    assert!(m.set_chains(u));
                }
            }
            m.render(1_024, &mut out);
            all.extend_from_slice(&out[0]);
        }
        all
    };
    let plain = play(None);
    let same = play(Some(ChainUpdate {
        tracks: vec![(tid, lim(0.0))],
        buses: vec![(bid, lim(0.0))],
        ..ChainUpdate::default()
    }));
    assert_eq!(
        plain, same,
        "the same chains pick up exactly where the old ones were"
    );
    let lower = play(Some(ChainUpdate {
        buses: vec![(bid, lim(-6.0))],
        ..ChainUpdate::default()
    }));
    assert_eq!(clicks(&lower, 0.4), vec![5_500]);
    assert!((peak(&lower, 0, 12_000) - 0.5).abs() < 0.02);
    // A different look-ahead cannot swap in place.
    let mut m = mix(&s, true, 0);
    assert!(!m.set_chains(&ChainUpdate {
        tracks: vec![(tid, limiter(10.0))],
        ..ChainUpdate::default()
    }));
}

#[test]
fn track_chain_reaches_every_clip() {
    let dir = tempfile::tempdir().unwrap();
    let tone = wav(dir.path(), "tone.wav", |i| {
        (i as f32 * 220.0 * std::f32::consts::TAU / RATE as f32).sin() * 0.5
    });
    for fx in [
        chain("altaudio.gain", &[("gain", -12.0)]),
        chain(
            "altaudio.limiter",
            &[("ceiling", -12.0), ("lookahead", 5.0)],
        ),
    ] {
        let mut s = Session::new("t", RATE);
        let t = track(&mut s, &tone, 0, 0, 12_000);
        for start in [30_000, 60_000] {
            let mid = s.media[0].id;
            let tid = s.tracks[t].id;
            apply(
                &mut s,
                &Op::AddClips {
                    track_id: tid,
                    clips: vec![ClipSpec {
                        id: new_id(),
                        name: "c".into(),
                        media_id: mid,
                        start,
                        source_in: 0,
                        source_out: 12_000,
                        channel_mix: None,
                    }],
                },
            )
            .unwrap();
        }
        s.tracks[t].effects = fx;
        let out = &render(&s, false, 0, 80_000)[0];
        let (a, b, c) = (
            peak(out, 4_000, 11_000),
            peak(out, 34_000, 41_000),
            peak(out, 64_000, 71_000),
        );
        assert!((a - b).abs() < 0.03 && (a - c).abs() < 0.03, "{a} {b} {c}");
        assert!(a < 0.35, "first clip {a} not processed");
    }
}

#[test]
fn loops_play_through_the_lead() {
    let dir = tempfile::tempdir().unwrap();
    let path = click(dir.path());
    let mut s = Session::new("t", RATE);
    track(&mut s, &path, 5_000, 500, 10_000);
    s.buses.push(Bus::new("b", ChannelLayout::Stereo));
    s.tracks[0].output = Output::Bus {
        bus_id: s.buses[0].id,
    };
    s.buses[0].effects = limiter(5.0);
    // Loop 5,000..6,000 from 4,000: click at 1,500 rendered frames, then every 1,000.
    let r = Renderer::new(plan::build(&s, RATE), 4_000).with_jump(6_000, 5_000);
    let mut m = Mix::new(r, Graph::new(&s, RATE, true));
    let mut out = vec![Vec::new()];
    let mut all = Vec::new();
    while all.len() < 4_600 * 2 {
        m.render(1_000, &mut out);
        all.extend_from_slice(&out[0]);
    }
    assert_eq!(clicks(&all, 0.5), vec![1_500, 2_500, 3_500, 4_500]);
}

#[test]
fn a_damaged_loop_still_plays() {
    let mut s = Session::new("t", RATE);
    s.tracks.push(Track::new("t", ChannelLayout::Stereo));
    s.buses.push(Bus::new("a", ChannelLayout::Stereo));
    s.buses.push(Bus::new("b", ChannelLayout::Stereo));
    let (a, b) = (s.buses[0].id, s.buses[1].id);
    s.tracks[0].output = Output::Bus { bus_id: a };
    s.buses[0].output = Output::Bus { bus_id: b };
    s.buses[1].output = Output::Bus { bus_id: a };
    let mut g = Graph::new(&s, RATE, true);
    let raw = vec![vec![0.25f32; 64]];
    let mut out = vec![Vec::new()];
    g.process(&raw, &mut out);
    assert_eq!(out[0].len(), 64);
    assert!(
        (out[0][10] - 0.25).abs() < 1e-6,
        "the loop's back edge goes to the master"
    );
}

/// Renders a 3D Space session, returning each block's bed (one channel per speaker).
fn render_bed(s: &Session, frames: usize) -> Vec<f32> {
    let mut m = mix(s, true, 0);
    m.graph().set_want_bed(true);
    let mut out = vec![Vec::new(); m.outputs()];
    let mut bed = Vec::new();
    let mut done = 0;
    while done < frames {
        done += m.render(1_024.min(frames - done), &mut out);
        bed.extend_from_slice(m.graph().bed());
    }
    bed
}

#[test]
fn a_3d_bus_runs_its_effects_on_the_whole_room() {
    use crate::session::model::{Space, SpaceLayout, SpacePos};
    let dir = tempfile::tempdir().unwrap();
    let path = click(dir.path());
    let mut s = Session::new("t", RATE);
    s.space = Some(Space {
        layout: SpaceLayout::Surround51,
        listen: Default::default(),
    });
    let t = track(&mut s, &path, 0, 0, 20_000);
    s.tracks[t].space = Some(SpacePos {
        x: -1.0,
        y: 1.0,
        ..SpacePos::default()
    });
    let mut bus = Bus::new("Room", ChannelLayout::Stereo);
    bus.room = true;
    bus.effects = chain("altaudio.reverb", &[("dry", 0.0), ("wet", 100.0)]);
    let bus_id = bus.id;
    s.buses.push(bus);
    s.tracks[t].output = Output::Bus { bus_id };
    let bed = render_bed(&s, 20_000);
    let energy = |c: usize| {
        bed.as_chunks::<6>()
            .0
            .iter()
            .map(|f| f[c] * f[c])
            .sum::<f32>()
    };
    // The click sits front left; the bus's reverb reaches every speaker
    // (L R C LFE Ls Rs), the rear right too, but not the LFE.
    for c in [0, 1, 2, 4, 5] {
        assert!(energy(c) > 1e-4, "speaker {c}: {}", energy(c));
    }
    assert!(energy(3) < 1e-6);

    // Without 3D Space it is a plain stereo bus and still plays.
    s.space = None;
    let out = render(&s, true, 0, 20_000);
    assert!(peak(&out[0], 0, 20_000) > 1e-3);
}
