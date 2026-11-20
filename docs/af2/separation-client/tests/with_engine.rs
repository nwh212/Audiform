// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) The Audiform contributors.

//! The client against a real engine. Set `AUDIFORM_SEPARATE_TEST_ENGINE`
//! to a development build of the engine (`cargo build --features any-host`
//! in the engine's folder); without it this test does nothing.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use audiform_separate_client::{Engine, Error, Output, RunEnd};
use serde_json::json;

fn wav(path: &Path, frames: usize) {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: 44100,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut w = hound::WavWriter::create(path, spec).unwrap();
    for i in 0..frames {
        w.write_sample((i as f32 * 0.01).sin() * 0.5).unwrap();
        w.write_sample((i as f32 * 0.02).sin() * 0.5).unwrap();
    }
    w.finalize().unwrap();
}

#[test]
fn separates_through_a_running_engine() {
    let Some(exe) = std::env::var_os("AUDIFORM_SEPARATE_TEST_ENGINE").map(PathBuf::from) else {
        return;
    };
    // An Audiform 1 version is turned away.
    assert!(matches!(
        Engine::start(&exe, "1.0.0"),
        Err(Error::Engine(_))
    ));

    let dir = std::env::temp_dir().join(format!("af-client-run-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let input = dir.join("in.wav");
    wav(&input, 40_000);
    let mut e = Engine::start(&exe, "2.0.0").unwrap();
    assert_eq!(e.version, "1.0.0");
    let spec = json!({"arch": "test", "sample_rate": 44100, "n_fft": 2048, "hop": 0, "win": 2048,
                      "heads": 1, "dim_head": 1, "chunk_size": 4096, "num_overlap": 2});
    let l = e.load("Test", &spec, Path::new("none"), false).unwrap();
    assert_eq!((l.stems, l.sample_rate), (2, 44100));
    let no = AtomicBool::new(false);
    // Two files with one load.
    for k in 0..2 {
        let mut seen = 0.0f32;
        let end = e
            .run(
                &input,
                &[
                    Output::stems(dir.join(format!("v{k}.wav")), &[1]),
                    Output::without(dir.join(format!("r{k}.wav")), &[1]),
                ],
                None,
                |p| seen = p,
                &no,
            )
            .unwrap();
        assert!(matches!(end, RunEnd::Done { .. }));
        assert!((seen - 1.0).abs() < 1e-6);
        assert!(dir.join(format!("r{k}.wav")).is_file());
    }
    // Cancelled before it starts: it stops at the first chunk.
    let yes = AtomicBool::new(true);
    assert_eq!(
        e.run(
            &input,
            &[Output::stems(dir.join("c.wav"), &[0])],
            None,
            |_| {},
            &yes
        )
        .unwrap(),
        RunEnd::Cancelled
    );
    e.unload().unwrap();
    assert!(e.loaded().is_none());
    drop(e);
    let _ = std::fs::remove_dir_all(&dir);
}
