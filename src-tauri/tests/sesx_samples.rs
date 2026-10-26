// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Imports every real `.sesx` in the repo's `samples/` folder (kept out
//! of git) and prints what came across. Run on demand:
//! `cargo test --test sesx_samples -- --ignored --nocapture`

use std::path::PathBuf;

use audiform_lib::session::validate::validate;
use audiform_lib::sesx::import;

#[test]
#[ignore = "needs real sessions in samples/"]
fn import_real_samples() {
    let dir = std::env::var_os("SESX_SAMPLES")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../samples"));
    let Ok(entries) = std::fs::read_dir(&dir) else {
        eprintln!("no samples folder at {}", dir.display());
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("sesx") {
            continue;
        }
        let out = import(&p).unwrap_or_else(|err| panic!("{}: {err}", p.display()));
        let s = &out.session;
        println!(
            "{}: {} Hz, {} tracks, {} clips, {} effects skipped, {} missing files",
            p.display(),
            s.sample_rate,
            out.report.tracks,
            out.report.clips,
            out.report.skipped_effects,
            out.report.missing_files.len()
        );
        for t in &s.tracks {
            println!(
                "  {:<14} {:?} {:>5.1} dB pan {:>4} mute {} solo {} clips {}",
                t.name,
                t.channels,
                t.volume_db,
                t.pan,
                t.muted,
                t.solo,
                t.clips.len()
            );
        }
        let issues = validate(s);
        assert!(issues.is_empty(), "{}: {issues:?}", p.display());
    }
}
