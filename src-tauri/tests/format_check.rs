// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Manual check: decodes every file in $FORMAT_DIR and reports length and peak.
//! `FORMAT_DIR=/path cargo test --test format_check -- --ignored --nocapture`

use audiform_lib::codecs::{probe, AudioReader};

#[test]
#[ignore]
fn decode_every_file_in_format_dir() {
    let dir = std::env::var("FORMAT_DIR").expect("set FORMAT_DIR");
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .collect();
    entries.sort();
    for path in entries {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        match probe(&path) {
            Err(e) => println!("{name:20} PROBE FAIL {e}"),
            Ok(info) => {
                let mut r = match AudioReader::open(&path, 1, None) {
                    Ok(r) => r,
                    Err(e) => {
                        println!("{name:20} OPEN FAIL {e}");
                        continue;
                    }
                };
                let mut buf = Vec::new();
                let mut total = 0usize;
                let mut peak = 0f32;
                while let Ok(true) = r.read_into(&mut buf) {
                    total += buf.len();
                    peak = buf.iter().fold(peak, |p, v| p.max(v.abs()));
                    buf.clear();
                }
                println!(
                    "{name:20} ok {} {} Hz {} ch, decoded {total} frames, peak {peak:.2}",
                    info.codec, info.sample_rate, info.channels
                );
            }
        }
    }
}
