// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

//! A check of a file just exported: does it open, is it as long as the
//! mix, does every sample decode, and are the chapters really in it.
//! A bad export is the worst surprise a podcaster gets, found after
//! upload; this finds it while the session is still open.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::chapters::{read_chapters, Chapter};
use crate::codecs::{self, AudioReader};

/// What the export should have produced.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Expect {
    /// Length in frames at the file's rate.
    pub frames: u64,
    pub sample_rate: u32,
    /// Chapters that were written; empty when none were.
    #[serde(default)]
    pub chapters: Vec<Chapter>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Check {
    pub name: String,
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub ok: bool,
    pub checks: Vec<Check>,
}

impl Report {
    fn add(&mut self, name: &str, ok: bool, detail: impl Into<String>) {
        self.ok &= ok;
        self.checks.push(Check {
            name: name.into(),
            ok,
            detail: detail.into(),
        });
    }
}

fn clock(frames: u64, rate: u32) -> String {
    let s = frames / u64::from(rate.max(1));
    format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
}

/// Lossy codecs pad and trim by a few frames; allow a tenth of a second.
fn tolerance(ext: &str, rate: u32) -> u64 {
    match ext {
        "mp3" | "m4a" | "aac" => u64::from(rate) / 10,
        _ => 0,
    }
}

/// Checks `path` against what `expect` says it should be. Decodes the
/// whole file, so it takes about as long as reading it through once.
pub fn verify(path: &Path, expect: &Expect) -> Report {
    let mut r = Report {
        ok: true,
        checks: Vec::new(),
    };
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    r.add("File", size > 0, format!("{} bytes", size));
    if size == 0 {
        return r;
    }

    if ext == "wav" {
        let head = std::fs::File::open(path).ok().and_then(|mut f| {
            use std::io::Read;
            let mut h = [0u8; 8];
            f.read_exact(&mut h).ok().map(|_| h)
        });
        if let Some(h) = head.filter(|h| &h[..4] == b"RIFF") {
            let riff = u64::from(u32::from_le_bytes([h[4], h[5], h[6], h[7]])) + 8;
            let ok = riff == size || riff + 1 == size;
            r.add(
                "WAV header",
                ok,
                if ok {
                    "Size in the header matches the file.".to_string()
                } else {
                    format!("Header says {riff} bytes, the file has {size}.")
                },
            );
        }
    }

    let info = match codecs::probe(path) {
        Ok(i) => i,
        Err(e) => {
            r.add("Opens", false, e.to_string());
            return r;
        }
    };
    r.add("Opens", true, format!("{} {}", info.format, info.codec));
    r.add(
        "Sample rate",
        info.sample_rate == expect.sample_rate,
        format!("{} Hz (expected {})", info.sample_rate, expect.sample_rate),
    );

    let tol = tolerance(&ext, expect.sample_rate);
    let mut frames = 0u64;
    let mut peak = 0f32;
    let mut bad: Option<String> = None;
    match AudioReader::open(path, usize::from(info.channels).max(1), None) {
        Ok(mut rd) => {
            let ch = rd.out_channels().max(1) as u64;
            let mut buf = Vec::new();
            loop {
                buf.clear();
                match rd.read_into(&mut buf) {
                    Ok(true) => {
                        frames += buf.len() as u64 / ch;
                        for &v in &buf {
                            if !v.is_finite() {
                                bad = Some("A sample is not a number.".into());
                            }
                            peak = peak.max(v.abs());
                        }
                    }
                    Ok(false) => break,
                    Err(e) => {
                        bad = Some(e.to_string());
                        break;
                    }
                }
            }
        }
        Err(e) => bad = Some(e.to_string()),
    }
    match bad {
        Some(m) => r.add("Decodes", false, m),
        None => r.add("Decodes", true, "Every sample reads back."),
    }
    let diff = frames.abs_diff(expect.frames);
    r.add(
        "Length",
        diff <= tol,
        format!(
            "{} (expected {}){}",
            clock(frames, expect.sample_rate),
            clock(expect.frames, expect.sample_rate),
            if diff <= tol {
                String::new()
            } else {
                format!(", {diff} frames off")
            }
        ),
    );
    if peak == 0.0 && frames > 0 {
        r.add("Sound", false, "The file is completely silent.");
    }

    if !expect.chapters.is_empty() {
        let got = if ext == "wav" {
            codecs::cue::read_cues(path)
                .into_iter()
                .map(|c| Chapter {
                    title: c.label,
                    start_ms: c.position * 1000 / u64::from(info.sample_rate.max(1)),
                    end_ms: None,
                    url: None,
                })
                .collect()
        } else {
            read_chapters(path)
        };
        let same = got.len() == expect.chapters.len()
            && got
                .iter()
                .zip(&expect.chapters)
                .all(|(a, b)| a.title == b.title && a.start_ms.abs_diff(b.start_ms) <= 2);
        r.add(
            "Chapters",
            same,
            if same {
                format!("{} chapters read back.", got.len())
            } else {
                format!("Wrote {}, read back {}.", expect.chapters.len(), got.len())
            },
        );
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav(path: &Path, frames: u32) {
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 8000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(path, spec).unwrap();
        for i in 0..frames {
            let v = ((i % 40) as i16 - 20) * 300;
            w.write_sample(v).unwrap();
            w.write_sample(v).unwrap();
        }
        w.finalize().unwrap();
    }

    fn expect(frames: u64) -> Expect {
        Expect {
            frames,
            sample_rate: 8000,
            chapters: Vec::new(),
        }
    }

    #[test]
    fn a_good_file_passes_every_check() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.wav");
        wav(&p, 16_000);
        let r = verify(&p, &expect(16_000));
        assert!(r.ok, "{r:?}");
        assert!(r.checks.iter().any(|c| c.name == "Length" && c.ok));
    }

    #[test]
    fn a_short_file_is_caught() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.wav");
        wav(&p, 16_000);
        let r = verify(&p, &expect(20_000));
        assert!(!r.ok);
        assert!(r.checks.iter().any(|c| c.name == "Length" && !c.ok));
    }

    #[test]
    fn a_cut_off_file_and_a_missing_one_are_caught() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.wav");
        wav(&p, 16_000);
        let d = std::fs::read(&p).unwrap();
        std::fs::write(&p, &d[..d.len() / 2]).unwrap();
        assert!(!verify(&p, &expect(16_000)).ok);
        assert!(!verify(&dir.path().join("none.wav"), &expect(1)).ok);
    }

    #[test]
    fn chapters_are_compared_with_what_was_written() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.wav");
        wav(&p, 16_000);
        let ch = vec![Chapter {
            title: "Intro".into(),
            start_ms: 0,
            end_ms: None,
            url: None,
        }];
        super::super::chapters::embed_wav(&p, &ch).unwrap();
        let mut e = expect(16_000);
        e.chapters = ch;
        assert!(verify(&p, &e).ok);
        e.chapters[0].title = "Other".into();
        assert!(!verify(&p, &e).ok);
    }
}
