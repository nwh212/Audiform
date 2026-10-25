// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

//! VST3 plug-ins end to end: scan a real bundle with the editor's own
//! executable as the host, mix through it in a Voice Chain, keep its
//! settings, and carry on when it crashes. Then a MIDI track playing
//! the test instrument.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;

use altered_audio_editor_lib::export::mixdown::{render_mix, MixdownRequest};
use altered_audio_editor_lib::plugins::{self, instance};
use altered_audio_editor_lib::session::model::{
    new_id, ChannelLayout, EffectSlot, MediaRef, MidiClip, Note, Session, Track,
};
use altered_audio_editor_lib::session::ops::{apply, ClipSpec, Op};

/// The id a scanned plug-in gets. The byte order of a class id differs
/// between Windows and elsewhere, so the tests ask the registry by name
/// rather than hard-code it.
fn id_of(name: &str) -> String {
    plugins::snapshot()
        .plugins
        .iter()
        .find(|p| p.info.name == name)
        .map(|p| format!("vst3:{}", p.info.cid))
        .unwrap_or_else(|| format!("vst3:missing-{name}"))
}

fn effect_id() -> String {
    id_of("Altered Test Gain")
}

fn synth_id() -> String {
    id_of("Altered Test Synth")
}

/// Builds the test plug-in and puts it in a bundle inside `dir`.
fn bundle(dir: &Path) -> PathBuf {
    let here = Path::new(env!("CARGO_MANIFEST_DIR")).join("plughost");
    let target = here.join("target").join("testplug");
    let ok = Command::new(env!("CARGO"))
        .args(["build", "--quiet", "--manifest-path"])
        .arg(here.join("testplug").join("Cargo.toml"))
        .arg("--target-dir")
        .arg(&target)
        .status()
        .unwrap()
        .success();
    assert!(ok, "the test plug-in builds");
    let (lib, arch, ext) = if cfg!(windows) {
        ("altered_testplug.dll", "x86_64-win", "vst3")
    } else {
        ("libaltered_testplug.so", "x86_64-linux", "so")
    };
    let b = dir.join("Vendor").join("AlteredTestGain.vst3");
    let d = b.join("Contents").join(arch);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::copy(
        target.join("debug").join(lib),
        d.join(format!("AlteredTestGain.{ext}")),
    )
    .unwrap();
    b
}

fn click(dir: &Path) -> PathBuf {
    let path = dir.join("click.wav");
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 48_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(&path, spec).unwrap();
    for i in 0..48_000 {
        w.write_sample(if i == 1_000 { i16::MAX } else { 0 })
            .unwrap();
    }
    w.finalize().unwrap();
    path
}

fn slot(params: &[(&str, f64)]) -> EffectSlot {
    EffectSlot {
        id: new_id(),
        effect: effect_id(),
        enabled: true,
        params: params.iter().map(|(k, v)| ((*k).to_string(), *v)).collect(),
        preset: None,
        sidechain: None,
        plugin_state: None,
        position: None,
    }
}

/// A session: one track with the click at timeline frame 6,000, through `fx`.
fn session(dir: &Path, fx: EffectSlot) -> Session {
    let mut s = Session::new("t", 48_000);
    s.bit_depth = 32;
    let m = MediaRef {
        id: new_id(),
        path: click(dir),
        relative_path: None,
        sample_rate: 48_000,
        channels: 1,
        length: 48_000,
    };
    let mid = m.id;
    s.media.push(m);
    s.tracks.push(Track::new("A", ChannelLayout::Stereo));
    let tid = s.tracks[0].id;
    apply(
        &mut s,
        &Op::AddClips {
            track_id: tid,
            clips: vec![ClipSpec {
                id: new_id(),
                name: "c".into(),
                media_id: mid,
                start: 5_000,
                source_in: 0,
                source_out: 48_000,
            }],
        },
    )
    .unwrap();
    s.tracks[0].effects.slots.push(fx);
    s
}

fn mix(s: &Session) -> Vec<f32> {
    mix_range(s, 0, 12_000)
}

fn mix_range(s: &Session, from: u64, to: u64) -> Vec<f32> {
    let mut out = Vec::new();
    let req = MixdownRequest {
        range: Some((from, to)),
        ..MixdownRequest::default()
    };
    render_mix(
        s,
        &req,
        &AtomicBool::new(false),
        |_, _| {},
        |b| {
            out.extend_from_slice(b);
            Ok(())
        },
    )
    .unwrap();
    out
}

/// Where the loudest frame is, and its level to 3 decimals.
fn peak(v: &[f32]) -> (usize, f32) {
    let (i, p) = v.chunks(2).enumerate().fold((0, 0.0f32), |m, (i, f)| {
        if f[0].abs() > m.1 {
            (i, f[0].abs())
        } else {
            m
        }
    });
    (i, (p * 1_000.0).round() / 1_000.0)
}

#[test]
fn plug_ins_scan_mix_keep_settings_and_survive_crashes() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("MyPlugins");
    bundle(&folder);
    std::env::set_var(
        "ALTERED_PLUGIN_HOST",
        env!("CARGO_BIN_EXE_altered-audio-editor"),
    );
    plugins::load(dir.path());

    // The user's folder is searched; the plug-in is found and cached.
    plugins::set_folder(&folder, true);
    let report = plugins::scan(true, |_, _, _| {});
    assert_eq!(
        (report.found, report.failed),
        (2, 0),
        "{:?}",
        plugins::snapshot().failed
    );
    assert!(dir.path().join("plugins.json").is_file());
    let info =
        altered_audio_editor_lib::dsp::effects::info(&effect_id()).expect("a plug-in is an effect");
    assert_eq!(info.name, "Altered Test Gain");
    assert_eq!(info.params[0].key, "p0");
    // The synth is an instrument: offered to MIDI tracks, not Voice Chains.
    assert_eq!(
        plugins::instruments()
            .iter()
            .map(|i| i.name)
            .collect::<Vec<_>>(),
        ["Altered Test Synth"]
    );
    assert!(plugins::catalog().iter().all(|i| i.id != synth_id()));
    // A rescan skips the unchanged bundle.
    assert_eq!(plugins::scan(false, |_, _, _| {}).scanned, 0);

    // Mixdown through the plug-in: half gain, and its 32-sample look-ahead
    // compensated, so the click stays at 6,000.
    let s = session(dir.path(), slot(&[("p0", 0.5)]));
    assert_eq!(peak(&mix(&s)), (6_000, 0.5));

    // Playback's pooled host: settings come back out and load into a new one.
    let mut fx = slot(&[("p0", 0.25)]);
    let p = instance::processor(&fx, 48_000).expect("starts");
    drop(p);
    let state = instance::running(fx.id)
        .unwrap()
        .lock()
        .unwrap()
        .state()
        .expect("settings");
    fx.plugin_state = Some(state);
    fx.params.clear();
    let s2 = session(dir.path(), fx.clone());
    assert_eq!(
        peak(&mix(&s2)).1,
        0.25,
        "the saved settings, in a fresh host"
    );
    instance::retain(&Default::default());
    assert!(instance::running(fx.id).is_none(), "unused hosts stop");

    // Two tracks with plug-ins run side by side and still mix in time.
    let mut two = session(dir.path(), slot(&[("p0", 0.5)]));
    let mut other = two.tracks[0].clone();
    other.id = new_id();
    for c in &mut other.clips {
        c.id = new_id();
    }
    other.effects.slots = vec![slot(&[("p0", 0.25)])];
    two.tracks.push(other);
    assert_eq!(peak(&mix(&two)), (6_000, 0.75));

    // The plug-in crashes: the mix carries on with the dry sound, in time.
    let s3 = session(dir.path(), slot(&[("p0", 0.5), ("p1", 1.0)]));
    assert_eq!(peak(&mix(&s3)), (6_000, 1.0));

    // Not installed: the slot is skipped, nothing breaks.
    let mut missing = slot(&[]);
    missing.effect = "vst3:00000000000000000000000000000000".into();
    assert_eq!(peak(&mix(&session(dir.path(), missing))), (6_000, 1.0));

    // A MIDI track: at 120 BPM a beat is 24,000 samples. The clip starts
    // at beat 0.25 (6,000) and holds one note for an eighth of a beat
    // (3,000). Full velocity plays 0.5; the track's chain (the gain
    // plug-in at half, with its look-ahead) brings it to 0.25, in time.
    let mut s = Session::new("m", 48_000);
    s.bit_depth = 32;
    let tid = new_id();
    apply(
        &mut s,
        &Op::AddTrack {
            id: tid,
            name: "Keys".into(),
            stereo: true,
            index: 0,
            midi: true,
        },
    )
    .unwrap();
    let mut synth = slot(&[]);
    synth.effect = synth_id();
    s.tracks[0].instrument = Some(synth);
    s.tracks[0].effects.slots.push(slot(&[("p0", 0.5)]));
    apply(
        &mut s,
        &Op::SetMidiClips {
            track_id: tid,
            clips: vec![MidiClip {
                id: new_id(),
                name: "c".into(),
                start: 0.25,
                length: 1.0,
                muted: false,
                color: None,
                notes: vec![Note {
                    pitch: 60,
                    velocity: 127,
                    start: 0.0,
                    length: 0.125,
                }],
            }],
        },
    )
    .unwrap();
    let y = mix(&s);
    let at = |i: usize| (y[i * 2] * 1_000.0).round() / 1_000.0;
    assert_eq!(
        (at(5_999), at(6_000), at(8_999), at(9_000)),
        (0.0, 0.25, 0.25, 0.0)
    );
    // Playback of a session without buses runs each track on its own
    // (faders on the audio thread): the track's effects still apply.
    {
        use altered_audio_editor_lib::engine::{graph, plan, render::Renderer};
        let mut m = graph::Mix::new(
            Renderer::new(plan::build(&s, 48_000), 0),
            graph::Graph::new(&s, 48_000, false),
        );
        let mut out = vec![Vec::new(); m.outputs()];
        let mut track = Vec::new();
        while track.len() < 12_000 * 2 {
            m.render(1_024, &mut out);
            track.extend_from_slice(&out[0]);
        }
        let at = |i: usize| (track[i * 2] * 1_000.0).round() / 1_000.0;
        assert_eq!(
            (at(6_000), at(8_999)),
            (0.25, 0.25),
            "playback hears the track's effects"
        );
    }
    // Starting in the middle of the note plays it from there.
    let y = mix_range(&s, 7_000, 8_000);
    assert_eq!((y[0] * 1_000.0).round() / 1_000.0, 0.25);
    // The session runs to the clip's end.
    assert_eq!(
        altered_audio_editor_lib::export::mixdown::session_end(&s),
        30_000
    );
    // A slower tempo moves the note: at 60 BPM it starts at 12,000.
    apply(
        &mut s,
        &Op::SetTempo {
            bpm: 60.0,
            beats_per_bar: 4,
        },
    )
    .unwrap();
    let y = mix_range(&s, 0, 20_000);
    let at = |i: usize| (y[i * 2] * 1_000.0).round() / 1_000.0;
    assert_eq!((at(11_999), at(12_000)), (0.0, 0.25));
    // Instruments start loading in the background as soon as they are in
    // the session, so playback never waits on a slow one.
    let inst = s.tracks[0].instrument.clone().unwrap();
    instance::warm(vec![inst.clone()]);
    let t0 = std::time::Instant::now();
    while instance::running(inst.id).is_none() && t0.elapsed() < std::time::Duration::from_secs(20)
    {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(
        instance::running(inst.id).is_some(),
        "warmed in the background"
    );
    // A note heard while editing: it sounds, lets go after the hold, and
    // the preview ends after a short tail.
    let make = instance::audition(&inst, 60, 1.0, 0.1).expect("starts");
    let rate = altered_audio_editor_lib::engine::output::device_rate();
    let mut fill = make(2, rate).unwrap();
    let mut heard = Vec::new();
    let mut block = Vec::new();
    while fill(&mut block) {
        heard.append(&mut block);
        assert!(heard.len() < rate as usize * 2 * 5, "it ends");
    }
    let frames = heard.len() / 2;
    let hold = (rate as usize) / 10;
    assert!(
        heard[..hold * 2].iter().any(|v| v.abs() > 0.1),
        "the note sounds"
    );
    assert!(
        heard[(hold + 1_000) * 2..].iter().all(|v| v.abs() < 1e-6),
        "and lets go"
    );
    assert!(frames >= hold + rate as usize / 3, "with a tail");
    instance::retain(&Default::default());

    // No instrument: silent, nothing breaks.
    s.tracks[0].instrument = None;
    assert!(mix_range(&s, 0, 20_000).iter().all(|v| *v == 0.0));

    // Removing the folder forgets the plug-in at the next scan.
    plugins::set_folder(&folder, false);
    assert_eq!(plugins::scan(true, |_, _, _| {}).found, 0);
}

/// A real reverb (set REVERB_DIR to a folder of VST3 reverbs) after an
/// instrument on a MIDI track: its tail must ring on after the note, in
/// export and in playback alike. Ignored unless asked for.
#[test]
#[ignore]
fn a_real_reverb_rings_on_a_midi_track() {
    let rev = PathBuf::from(std::env::var("REVERB_DIR").expect("REVERB_DIR"));
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("MyPlugins");
    bundle(&folder);
    std::env::set_var(
        "ALTERED_PLUGIN_HOST",
        env!("CARGO_BIN_EXE_altered-audio-editor"),
    );
    plugins::load(dir.path());
    plugins::set_folder(&folder, true);
    plugins::set_folder(&rev, true);
    let report = plugins::scan(true, |_, _, _| {});
    println!("scan {report:?} failed {:?}", plugins::snapshot().failed);
    let info = plugins::catalog()
        .into_iter()
        .find(|i| i.name.contains("Plate"))
        .expect("the reverb is listed");
    println!("reverb {} {} controls", info.id, info.params.len());

    let mut s = Session::new("m", 48_000);
    s.bit_depth = 32;
    let tid = new_id();
    apply(
        &mut s,
        &Op::AddTrack {
            id: tid,
            name: "Keys".into(),
            stereo: true,
            index: 0,
            midi: true,
        },
    )
    .unwrap();
    let mut synth = slot(&[]);
    synth.effect = synth_id();
    s.tracks[0].instrument = Some(synth);
    apply(
        &mut s,
        &Op::SetMidiClips {
            track_id: tid,
            clips: vec![MidiClip {
                id: new_id(),
                name: "c".into(),
                start: 0.25,
                length: 4.0,
                muted: false,
                color: None,
                notes: vec![Note {
                    pitch: 60,
                    velocity: 127,
                    start: 0.0,
                    length: 0.125,
                }],
            }],
        },
    )
    .unwrap();
    // The note plays 6,000 to 9,000; after 10,000 only a tail can sound.
    let tail = |y: &[f32]| -> f32 { y[10_000 * 2..60_000 * 2].iter().map(|v| v * v).sum::<f32>() };
    let dry = mix_range(&s, 0, 60_000);
    // The reverb as the app adds it: every control at its default.
    let mut fx = slot(&[]);
    fx.effect = info.id.to_string();
    fx.params = info
        .params
        .iter()
        .map(|p| (p.key.to_string(), p.default))
        .collect();
    s.tracks[0].effects.slots.push(fx);
    let wet = mix_range(&s, 0, 60_000);
    println!("export: dry tail {} wet tail {}", tail(&dry), tail(&wet));
    // Playback of a session without buses.
    use altered_audio_editor_lib::engine::{graph, plan, render::Renderer};
    let mut m = graph::Mix::new(
        Renderer::new(plan::build(&s, 48_000), 0),
        graph::Graph::new(&s, 48_000, false),
    );
    let mut out = vec![Vec::new(); m.outputs()];
    let mut play = Vec::new();
    while play.len() < 60_000 * 2 {
        m.render(1_024, &mut out);
        play.extend_from_slice(&out[0]);
    }
    println!("playback: wet tail {}", tail(&play));
    assert!(tail(&wet) > tail(&dry) * 10.0 + 1e-3, "export rings");
    assert!(tail(&play) > tail(&dry) * 10.0 + 1e-3, "playback rings");
}
