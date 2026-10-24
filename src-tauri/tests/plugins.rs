// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

//! VST3 plug-ins end to end: scan a real bundle with the editor's own
//! executable as the host, mix through it in a Voice Chain, keep its
//! settings, and carry on when it crashes.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;

use altered_audio_editor_lib::export::mixdown::{render_mix, MixdownRequest};
use altered_audio_editor_lib::plugins::{self, instance};
use altered_audio_editor_lib::session::model::{
    new_id, ChannelLayout, EffectSlot, MediaRef, Session, Track,
};
use altered_audio_editor_lib::session::ops::{apply, ClipSpec, Op};

const EFFECT: &str = "vst3:41A17E2D7E5700018A1B2C3D4E5F6071";

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
        effect: EFFECT.into(),
        enabled: true,
        params: params.iter().map(|(k, v)| ((*k).to_string(), *v)).collect(),
        preset: None,
        sidechain: None,
        plugin_state: None,
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
    let mut out = Vec::new();
    let req = MixdownRequest {
        range: Some((0, 12_000)),
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
        (1, 0),
        "{:?}",
        plugins::snapshot().failed
    );
    assert!(dir.path().join("plugins.json").is_file());
    let info =
        altered_audio_editor_lib::dsp::effects::info(EFFECT).expect("a plug-in is an effect");
    assert_eq!(info.name, "Altered Test Gain");
    assert_eq!(info.params[0].key, "p0");
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

    // The plug-in crashes: the mix carries on with the dry sound, in time.
    let s3 = session(dir.path(), slot(&[("p0", 0.5), ("p1", 1.0)]));
    assert_eq!(peak(&mix(&s3)), (6_000, 1.0));

    // Not installed: the slot is skipped, nothing breaks.
    let mut missing = slot(&[]);
    missing.effect = "vst3:00000000000000000000000000000000".into();
    assert_eq!(peak(&mix(&session(dir.path(), missing))), (6_000, 1.0));

    // Removing the folder forgets the plug-in at the next scan.
    plugins::set_folder(&folder, false);
    assert_eq!(plugins::scan(true, |_, _, _| {}).found, 0);
}
