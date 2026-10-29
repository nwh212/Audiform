// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! End to end: build the test plug-in, put it in a real bundle, and drive
//! it through a host process the way the editor does.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use altered_plughost::protocol::*;

/// The class id a scan reports for a test plug-in. Its first eight bytes
/// are written in a different order on Windows than elsewhere, so the
/// tests ask the scan rather than hard-code it.
fn cid_of(bundle: &Path, name: &str) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_altered-plughost"))
        .arg("--plugin-scan")
        .arg(bundle)
        .output()
        .unwrap();
    scan_result(&out)
        .classes
        .into_iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("no class named {name}"))
        .cid
}

/// Builds the test plug-in once and returns its bundle. Tests run at the
/// same time, so the first builds and copies it and the rest wait, never
/// reading a bundle half copied.
pub fn test_bundle() -> PathBuf {
    static BUNDLE: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    BUNDLE.get_or_init(build_bundle).clone()
}

fn build_bundle() -> PathBuf {
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest = here.join("testplug").join("Cargo.toml");
    let target = here.join("target").join("testplug");
    let status = Command::new(env!("CARGO"))
        .args(["build", "--quiet", "--manifest-path"])
        .arg(&manifest)
        .arg("--target-dir")
        .arg(&target)
        .status()
        .expect("cargo runs");
    assert!(status.success(), "the test plug-in builds");
    let (lib, arch_dir, ext) = if cfg!(windows) {
        ("altered_testplug.dll", "x86_64-win", "vst3")
    } else {
        ("libaltered_testplug.so", "x86_64-linux", "so")
    };
    let bundle = target.join("AlteredTestGain.vst3");
    let dir = bundle.join("Contents").join(arch_dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(
        target.join("debug").join(lib),
        dir.join(format!("AlteredTestGain.{ext}")),
    )
    .unwrap();
    bundle
}

struct Host {
    child: Child,
    stdin: ChildStdin,
    stdout: ChildStdout,
}

impl Host {
    fn start() -> Host {
        Host::start_with(&[])
    }

    fn start_with(env: &[(&str, &str)]) -> Host {
        let mut child = Command::new(env!("CARGO_BIN_EXE_altered-plughost"))
            .envs(env.iter().copied())
            .arg("--plugin-host")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        Host {
            child,
            stdin,
            stdout,
        }
    }

    fn request(&mut self, r: &Request) -> Message {
        write_frame(&mut self.stdin, CONTROL, &serde_json::to_vec(r).unwrap()).unwrap();
        self.next_message()
    }

    fn next_message(&mut self) -> Message {
        let mut buf = Vec::new();
        loop {
            let kind = read_frame(&mut self.stdout, &mut buf)
                .unwrap()
                .expect("host still running");
            assert_eq!(kind, CONTROL);
            let m: Message = serde_json::from_slice(&buf).unwrap();
            if !matches!(m, Message::ParamEdited { .. } | Message::Dirty) {
                return m;
            }
        }
    }

    /// Sends a block; None when the host is gone.
    fn process(&mut self, changes: &[(u32, f64)], samples: &[f32]) -> Option<Vec<f32>> {
        self.play(changes, &[], samples)
    }

    fn play(
        &mut self,
        changes: &[(u32, f64)],
        notes: &[NoteEvent],
        samples: &[f32],
    ) -> Option<Vec<f32>> {
        let mut body = Vec::new();
        encode_audio(&mut body, -1, changes, notes, samples);
        write_frame(&mut self.stdin, AUDIO, &body).ok()?;
        let mut buf = Vec::new();
        loop {
            match read_frame(&mut self.stdout, &mut buf).ok()?? {
                AUDIO => {
                    let mut out = Vec::new();
                    decode_samples(&buf, &mut out);
                    return Some(out);
                }
                _ => continue,
            }
        }
    }

    fn load(&mut self, bundle: &Path, state: Option<String>, params: Vec<(u32, f64)>) -> Message {
        let cid = cid_of(bundle, "Altered Test Gain");
        self.load_class(bundle, &cid, state, params)
    }

    fn load_class(
        &mut self,
        bundle: &Path,
        cid: &str,
        state: Option<String>,
        params: Vec<(u32, f64)>,
    ) -> Message {
        self.request(&Request::Load {
            path: bundle.to_string_lossy().into_owned(),
            cid: cid.into(),
            rate: 48_000.0,
            state,
            params,
            main: None,
        })
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.stdin.flush();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Reads a scan process's output the way the editor does.
fn scan_result(out: &std::process::Output) -> ScanResult {
    let exit = match out.status.code() {
        Some(c) => ScanExit::Code(c),
        None => ScanExit::Killed,
    };
    parse_scan_output(
        &String::from_utf8_lossy(&out.stdout),
        &String::from_utf8_lossy(&out.stderr),
        exit,
    )
}

fn impulse(frames: usize, at: usize) -> Vec<f32> {
    let mut v = vec![0.0; frames * 2];
    v[at * 2] = 1.0;
    v[at * 2 + 1] = 1.0;
    v
}

fn peak_at(v: &[f32]) -> (usize, f32) {
    v.chunks(2).enumerate().fold(
        (0, 0.0),
        |m, (i, f)| if f[0].abs() > m.1 { (i, f[0].abs()) } else { m },
    )
}

#[test]
fn scans_processes_saves_and_survives_a_crash() {
    let bundle = test_bundle();

    // Scan: one effect, its controls and look-ahead.
    let out = Command::new(env!("CARGO_BIN_EXE_altered-plughost"))
        .arg("--plugin-scan")
        .arg(&bundle)
        .output()
        .unwrap();
    let scan = scan_result(&out);
    assert_eq!(scan.error, None);
    assert_eq!(scan.classes.len(), 2);
    let c = &scan.classes[0];
    assert!(!c.instrument);
    // The synth has a note input and no audio input: an instrument.
    assert_eq!(
        (scan.classes[1].name.as_str(), scan.classes[1].instrument),
        ("Altered Test Synth", true)
    );
    assert_eq!((c.name.as_str(), c.latency), ("Altered Test Gain", 32));
    assert!(c.cid.ends_with("8A1B2C3D4E5F6071"), "{}", c.cid);
    assert_eq!(
        c.params
            .iter()
            .map(|p| p.title.as_str())
            .collect::<Vec<_>>(),
        ["Gain", "Crash"]
    );

    // A missing bundle is an error, not a crash.
    let out = Command::new(env!("CARGO_BIN_EXE_altered-plughost"))
        .args(["--plugin-scan", "/no/such/Plugin.vst3"])
        .output()
        .unwrap();
    let scan = scan_result(&out);
    assert!(scan.error.is_some());

    // A plug-in that crashes while starting: still listed by name, with where it died.
    let out = Command::new(env!("CARGO_BIN_EXE_altered-plughost"))
        .arg("--plugin-scan")
        .arg(&bundle)
        .env("ALTERED_TESTPLUG_CRASH_ON_START", "1")
        .output()
        .unwrap();
    let scan = scan_result(&out);
    assert_eq!(scan.classes.len(), 2, "{scan:?}");
    let note = scan.classes[0]
        .note
        .clone()
        .expect("a note on the crashed one");
    assert!(note.contains("crashed while starting it"), "{note}");

    // Process: the impulse comes out 32 samples later, at full level.
    let mut h = Host::start();
    match h.load(&bundle, None, vec![]) {
        Message::Loaded {
            latency, params, ..
        } => {
            assert_eq!(latency, 32);
            assert_eq!(params.len(), 2);
        }
        m => panic!("{m:?}"),
    }
    let y = h.process(&[], &impulse(256, 10)).unwrap();
    assert_eq!(peak_at(&y), (42, 1.0));
    // A control change applies in the same block.
    let y = h.process(&[(0, 0.5)], &impulse(256, 10)).unwrap();
    assert_eq!(peak_at(&y), (42, 0.5));
    // Bigger than the largest block: split, and still continuous.
    let y = h
        .process(&[], &impulse(MAX_BLOCK + 500, MAX_BLOCK + 100))
        .unwrap();
    assert_eq!(peak_at(&y), (MAX_BLOCK + 132, 0.5));

    // Settings come back in a new host.
    let state = match h.request(&Request::GetState) {
        Message::State { data } => data,
        m => panic!("{m:?}"),
    };
    let mut h2 = Host::start();
    assert!(matches!(
        h2.load(&bundle, Some(state), vec![]),
        Message::Loaded { .. }
    ));
    let y = h2.process(&[], &impulse(256, 10)).unwrap();
    assert_eq!(peak_at(&y).1, 0.5, "the saved gain");
    // Controls given at load apply too, and are in the settings at once.
    let mut h3 = Host::start();
    h3.load(&bundle, None, vec![(0, 0.25)]);
    let saved = match h3.request(&Request::GetState) {
        Message::State { data } => data,
        m => panic!("{m:?}"),
    };
    let mut h4 = Host::start();
    h4.load(&bundle, Some(saved), vec![]);
    assert_eq!(
        peak_at(&h4.process(&[], &impulse(256, 10)).unwrap()).1,
        0.25
    );
    let y = h3.process(&[], &impulse(256, 10)).unwrap();
    assert_eq!(peak_at(&y).1, 0.25);

    // No window on this system (or this plug-in): an error, not a hang.
    assert!(matches!(
        h.request(&Request::OpenEditor {
            title: "x".into(),
            owner: 0,
            bypassed: false,
        }),
        Message::Error { .. }
    ));

    // Crash on purpose: the host dies, the caller just sees the pipe close.
    assert_eq!(h.process(&[(1, 1.0)], &impulse(64, 0)), None);
    let status = h.child.wait().unwrap();
    assert!(!status.success());
}

#[test]
fn plays_notes_on_an_instrument_at_their_frames() {
    let bundle = test_bundle();
    let mut h = Host::start();
    match h.load_class(
        &bundle,
        &cid_of(&bundle, "Altered Test Synth"),
        None,
        vec![],
    ) {
        Message::Loaded {
            instrument,
            latency,
            ..
        } => assert_eq!((instrument, latency), (true, 0)),
        m => panic!("{m:?}"),
    }
    assert_eq!(
        h.request(&Request::Tempo {
            bpm: 90.0,
            beats_per_bar: 3,
            beat_unit: 8,
            start: 0,
            start_music: 0.0,
        }),
        Message::Ok
    );
    let on = |offset, pitch, velocity| NoteEvent {
        offset,
        on: true,
        pitch,
        velocity,
        ..Default::default()
    };
    let off = |offset, pitch| NoteEvent {
        offset,
        on: false,
        pitch,
        velocity: 0.0,
        ..Default::default()
    };
    let silent = vec![0.0; 256 * 2];
    // A note from frame 10 to 100.
    let y = h
        .play(&[], &[on(10, 60, 1.0), off(100, 60)], &silent)
        .unwrap();
    assert_eq!(
        (y[9 * 2], y[10 * 2], y[99 * 2], y[100 * 2]),
        (0.0, 0.5, 0.5, 0.0)
    );
    // Held across blocks, softer, through a block bigger than the largest.
    let big = vec![0.0; (MAX_BLOCK + 300) * 2];
    let y = h
        .play(
            &[],
            &[on(5, 64, 0.5), off(MAX_BLOCK as u32 + 200, 64)],
            &big,
        )
        .unwrap();
    assert_eq!(y[4 * 2], 0.0);
    assert_eq!(y[(MAX_BLOCK + 199) * 2], 0.25);
    assert_eq!(y[(MAX_BLOCK + 200) * 2], 0.0);
    // A reset silences a held note.
    h.play(&[], &[on(0, 67, 1.0)], &silent).unwrap();
    assert_eq!(h.request(&Request::Reset), Message::Ok);
    let y = h.play(&[], &[], &silent).unwrap();
    assert_eq!(y[0], 0.0);
}

#[test]
fn one_process_runs_several_plugins_each_on_its_own() {
    let bundle = test_bundle();
    let cid = cid_of(&bundle, "Altered Test Gain");
    let mut h = Host::start();
    let mut buf = Vec::new();
    // The next CONTROL or AUDIO frame, with the plug-in it is for.
    let mut cur = 0u32;
    let mut next = |h: &mut Host| -> (u32, u8, Vec<u8>) {
        loop {
            let kind = read_frame(&mut h.stdout, &mut buf)
                .unwrap()
                .expect("host still running");
            if kind == SELECT {
                cur = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
                continue;
            }
            return (cur, kind, buf.clone());
        }
    };
    let select =
        |h: &mut Host, id: u32| write_frame(&mut h.stdin, SELECT, &id.to_le_bytes()).unwrap();
    for (id, gain) in [(0u32, 0.5f64), (1, 0.25)] {
        select(&mut h, id);
        let load = Request::Load {
            path: bundle.to_string_lossy().into_owned(),
            cid: cid.clone(),
            rate: 48_000.0,
            state: None,
            params: vec![(0, gain)],
            main: None,
        };
        write_frame(&mut h.stdin, CONTROL, &serde_json::to_vec(&load).unwrap()).unwrap();
        let (from, kind, body) = next(&mut h);
        assert_eq!((from, kind), (id, CONTROL));
        assert!(matches!(
            serde_json::from_slice::<Message>(&body).unwrap(),
            Message::Loaded { .. }
        ));
    }
    // Each plug-in keeps its own gain, and each reply says whose it is.
    for (id, want) in [(1u32, 0.25f32), (0, 0.5), (1, 0.25)] {
        select(&mut h, id);
        let mut body = Vec::new();
        encode_audio(&mut body, 0, &[], &[], &impulse(256, 10));
        write_frame(&mut h.stdin, AUDIO, &body).unwrap();
        let (from, kind, out) = next(&mut h);
        assert_eq!((from, kind), (id, AUDIO));
        let mut y = Vec::new();
        decode_samples(&out, &mut y);
        assert_eq!(peak_at(&y), (42, want));
    }
    // Unloading one leaves the other playing.
    select(&mut h, 0);
    write_frame(
        &mut h.stdin,
        CONTROL,
        &serde_json::to_vec(&Request::Unload).unwrap(),
    )
    .unwrap();
    assert_eq!(next(&mut h).1, CONTROL);
    select(&mut h, 1);
    let mut body = Vec::new();
    encode_audio(&mut body, 0, &[], &[], &impulse(256, 10));
    write_frame(&mut h.stdin, AUDIO, &body).unwrap();
    let (from, _, out) = next(&mut h);
    let mut y = Vec::new();
    decode_samples(&out, &mut y);
    assert_eq!((from, peak_at(&y).1), (1, 0.25));
}

#[test]
fn a_plugin_with_a_sidechain_input_hears_the_key() {
    let bundle = test_bundle();
    let env = [("ALTERED_TESTPLUG_SIDECHAIN", "1")];
    // The scan notes the second input.
    let out = Command::new(env!("CARGO_BIN_EXE_altered-plughost"))
        .arg("--plugin-scan")
        .arg(&bundle)
        .envs(env)
        .output()
        .unwrap();
    let scan = scan_result(&out);
    let gain = scan.classes.iter().find(|c| c.name == "Altered Test Gain").unwrap();
    assert!(gain.sidechain);

    let mut h = Host::start_with(&env);
    assert!(matches!(h.load(&bundle, None, vec![]), Message::Loaded { .. }));
    let frames = 64;
    let silence = vec![0.0f32; frames * 2];
    let key: Vec<f32> = (0..frames * 2).map(|i| if i % 2 == 0 { 0.25 } else { -0.5 }).collect();
    let send = |h: &mut Host, key: Option<&[f32]>| -> Vec<f32> {
        if let Some(k) = key {
            let mut body = Vec::new();
            encode_samples(&mut body, k);
            write_frame(&mut h.stdin, KEY, &body).unwrap();
        }
        h.process(&[], &silence).unwrap()
    };
    // The key comes out beside the (silent) main input.
    let y = send(&mut h, Some(&key));
    assert_eq!(y, key);
    // A key counts for one block only.
    let y = send(&mut h, None);
    assert!(y.iter().all(|v| *v == 0.0));
}
