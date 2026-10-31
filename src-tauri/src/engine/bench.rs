// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

//! A dense session rendered as fast as the engine can, timed stage by
//! stage. Run by hand, in release mode, to see where the time goes:
//!
//! ```text
//! cargo test --release --lib bench_dense -- --ignored --nocapture
//! ```
//!
//! `BENCH_TRACKS` (48), `BENCH_SECS` (30) and `BENCH_FX` (1) change the
//! session; `BENCH_BLOCK` (2048) the block size, as playback uses.

use std::time::{Duration, Instant};

use crate::engine::graph::Graph;
use crate::engine::plan;
use crate::engine::render::Renderer;
use crate::session::model::{
    Bus, ChannelLayout, EffectChain, EffectSlot, MediaRef, Output, Send, Session, Track,
};
use crate::session::ops::{apply, ClipSpec, Op};

fn env(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// A minute of stereo music-like sound: a few tones and noise.
fn music_file(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("bench.wav");
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: 48_000,
        bits_per_sample: 24,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(&path, spec).unwrap();
    let mut seed = 1u32;
    for i in 0..48_000 * 60 {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let noise = (seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5;
        let t = i as f32 / 48_000.0;
        let v = 0.2 * (t * 220.0 * std::f32::consts::TAU).sin()
            + 0.1 * (t * 331.0 * std::f32::consts::TAU).sin()
            + 0.05 * noise;
        let s = (v * 8_388_607.0) as i32;
        w.write_sample(s).unwrap();
        w.write_sample((s as f32 * 0.9) as i32).unwrap();
    }
    w.finalize().unwrap();
    path
}

fn slot(effect: &str, params: &[(&str, f64)]) -> EffectSlot {
    EffectSlot {
        id: crate::session::model::new_id(),
        effect: effect.into(),
        enabled: true,
        params: params.iter().map(|(k, v)| ((*k).to_string(), *v)).collect(),
        preset: None,
        sidechain: None,
        plugin_state: None,
        data: None,
        position: None,
        routing: None,
    }
}

/// `tracks` stereo tracks, each with clips every 8 s (10 s long, so
/// neighbours crossfade), an EQ, a compressor and a reverb when `fx`,
/// and a send to one of two reverb buses.
pub fn dense_session(path: &std::path::Path, tracks: usize, secs: u64, fx: bool) -> Session {
    let mut s = Session::new("bench", 48_000);
    let m = MediaRef {
        id: crate::session::model::new_id(),
        path: path.to_path_buf(),
        relative_path: None,
        sample_rate: 48_000,
        channels: 2,
        length: 48_000 * 60,
    };
    let mid = m.id;
    s.media.push(m);
    let mut buses = Vec::new();
    for k in 0..2 {
        let mut b = Bus::new(format!("Verb {k}"), ChannelLayout::Stereo);
        if fx {
            b.effects
                .slots
                .push(slot("altaudio.reverb", &[("mix", 1.0)]));
        }
        buses.push(b.id);
        s.buses.push(b);
    }
    for t in 0..tracks {
        let mut tr = Track::new(format!("Track {t}"), ChannelLayout::Stereo);
        tr.volume_db = -6.0;
        tr.pan = (t % 5) as f64 * 0.4 - 0.8;
        if fx {
            tr.effects.slots.push(slot(
                "altaudio.parametric_eq",
                &[("low_gain", 2.0), ("high_gain", -1.5)],
            ));
            tr.effects.slots.push(slot(
                "altaudio.dynamics",
                &[("threshold", -18.0), ("ratio", 3.0)],
            ));
            if t % 3 == 0 {
                tr.effects
                    .slots
                    .push(slot("altaudio.reverb", &[("mix", 0.2)]));
            }
        }
        tr.sends.push(Send {
            id: crate::session::model::new_id(),
            bus_id: buses[t % 2],
            level_db: -12.0,
            pan: 0.0,
            pre_fader: false,
            enabled: true,
        });
        if t % 7 == 6 {
            tr.output = Output::Bus {
                bus_id: buses[t % 2],
            };
        }
        let tid = tr.id;
        s.tracks.push(tr);
        let mut clips = Vec::new();
        let mut at = (t as u64 % 4) * 12_000;
        while at < secs * 48_000 {
            let src = (t as u64 * 7_919) % (48_000 * 40);
            clips.push(ClipSpec {
                id: crate::session::model::new_id(),
                name: "c".into(),
                media_id: mid,
                start: at,
                source_in: src,
                source_out: src + 48_000 * 10,
                channel_mix: None,
            });
            at += 48_000 * 8;
        }
        apply(
            &mut s,
            &Op::AddClips {
                track_id: tid,
                clips,
            },
        )
        .unwrap();
        for c in &mut s.tracks.last_mut().unwrap().clips {
            c.fade_in.length = 48_000;
            c.fade_out.length = 48_000 * 2;
            c.gain_db = -1.0;
        }
    }
    let _ = EffectChain::default();
    s
}

/// Renders `secs` seconds of `s` and reports the time per stage.
pub fn run(s: &Session, secs: u64, block: usize) -> (Duration, Duration) {
    let rate = s.sample_rate;
    let s = crate::engine::speaker_feed::expanded(s);
    let mut renderer = Renderer::new(plan::build(&s, rate), 0);
    let mut graph = Graph::new(&s, rate, true);
    let mut raw = vec![Vec::new(); renderer.track_count()];
    let mut out = vec![Vec::new(); graph.outputs()];
    let total = secs * u64::from(rate);
    let (mut t_render, mut t_graph) = (Duration::ZERO, Duration::ZERO);
    let mut done = 0u64;
    while done < total {
        let n = block.min((total - done) as usize);
        graph.set_position(renderer.position());
        let a = Instant::now();
        renderer.render(n, &mut raw);
        let b = Instant::now();
        graph.process(&raw, &mut out);
        t_graph += b.elapsed();
        t_render += b - a;
        done += raw[0].len() as u64 / 2;
    }
    (t_render, t_graph)
}

#[test]
#[ignore]
fn bench_dense() {
    let dir = tempfile::tempdir().unwrap();
    let path = music_file(dir.path());
    let tracks = env("BENCH_TRACKS", 48);
    let secs = env("BENCH_SECS", 30) as u64;
    let fx = env("BENCH_FX", 1) == 1;
    let block = env("BENCH_BLOCK", 2048);
    crate::engine::host::init_threads();
    let s = dense_session(&path, tracks, secs, fx);
    let clips: usize = s.tracks.iter().map(|t| t.clips.len()).sum();
    let start = Instant::now();
    let (r, g) = run(&s, secs, block);
    let all = start.elapsed();
    println!(
        "{tracks} tracks, {clips} clips, fx {fx}, block {block}: {secs} s rendered in {:.2} s ({:.1}x real time)",
        all.as_secs_f64(),
        secs as f64 / all.as_secs_f64()
    );
    println!(
        "  clips and fades {:.2} s, graph (chains, sends, mix) {:.2} s, other {:.2} s",
        r.as_secs_f64(),
        g.as_secs_f64(),
        (all - r - g).as_secs_f64()
    );
    println!(
        "  per 2048-frame block at 48 kHz: {:.2} ms of the {:.2} ms available",
        all.as_secs_f64() * 1000.0 / (secs as f64 * 48_000.0 / 2048.0),
        2048.0 / 48.0
    );
}

/// Each built-in effect alone over 30 s of stereo sound: the time it takes.
#[test]
#[ignore]
fn bench_effects() {
    use crate::dsp::effects::ChainProcessor;
    let rate = 48_000;
    let secs = 30;
    let mut seed = 1u32;
    let src: Vec<f32> = (0..rate as usize * secs * 2)
        .map(|i| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let t = (i / 2) as f32 / rate as f32;
            0.3 * (t * 220.0 * std::f32::consts::TAU).sin()
                + 0.05 * ((seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5)
        })
        .collect();
    let all: Vec<String> = crate::dsp::effects::catalog()
        .iter()
        .map(|i| i.id.to_string())
        .collect();
    let only = std::env::var("BENCH_EFFECT").ok();
    for id in all {
        if only.as_ref().is_some_and(|o| !id.contains(o.as_str())) {
            continue;
        }
        let mut chain = EffectChain::default();
        chain.slots.push(slot(&id, &[]));
        let Some(mut p) = ChainProcessor::new(&chain, rate) else {
            continue;
        };
        let mut buf = src.clone();
        let start = Instant::now();
        for b in buf.chunks_mut(2048 * 2) {
            p.process(b);
        }
        let t = start.elapsed().as_secs_f64();
        println!(
            "{id:<32} {:7.1} ms per 30 s  ({:5.2}% of one core at 48 kHz)",
            t * 1000.0,
            t / secs as f64 * 100.0
        );
    }
}

/// Many copies of one effect run side by side: how the engine's threads
/// scale on this computer. `BENCH_EFFECT` picks it (reverb).
#[test]
#[ignore]
fn bench_parallel() {
    use crate::dsp::effects::ChainProcessor;
    use rayon::prelude::*;
    crate::engine::host::init_threads();
    let id = std::env::var("BENCH_EFFECT").unwrap_or_else(|_| "altaudio.reverb".into());
    let copies = env("BENCH_COPIES", 48);
    let rate = 48_000;
    let mut chain = EffectChain::default();
    chain.slots.push(slot(&id, &[]));
    let mut procs: Vec<(ChainProcessor, Vec<f32>)> = (0..copies)
        .map(|k| {
            (
                ChainProcessor::new(&chain, rate).unwrap(),
                (0..4096)
                    .map(|i| ((i * (k + 1)) as f32 * 0.01).sin() * 0.3)
                    .collect(),
            )
        })
        .collect();
    let blocks = 700;
    let start = Instant::now();
    for _ in 0..blocks {
        for (p, b) in procs.iter_mut() {
            p.process(b);
        }
    }
    let seq = start.elapsed().as_secs_f64();
    let start = Instant::now();
    for _ in 0..blocks {
        procs.par_iter_mut().for_each(|(p, b)| p.process(b));
    }
    let par = start.elapsed().as_secs_f64();
    println!(
        "{copies} x {id}: one thread {seq:.2} s, {} threads {par:.2} s ({:.2}x)",
        rayon::current_num_threads(),
        seq / par
    );
}
