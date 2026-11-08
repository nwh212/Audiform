// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform.

//! Save Space: smaller copies of a session's audio that sound exactly the
//! same. Uncompressed files (WAV, AIFF) become FLAC at its smallest
//! setting, which is lossless; stereo files whose two sides are the same
//! become mono, which a stereo track plays on both sides as before.
//!
//! Every copy is read back and compared with the original, sample for
//! sample, before it is offered; a copy that differs at all (beyond the
//! change the user allowed, such as 32-bit float stored as 24-bit) is
//! deleted and the original kept. A copy that is not smaller is dropped
//! too. Originals are never touched here.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::encode::{FormatKind, Mp3Mode, Resolved, Writer, FLAC_MAX_LEVEL};
use crate::codecs::{probe, AudioReader};

/// What Save Space may do.
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Options {
    /// Store WAV and AIFF files as FLAC.
    pub flac: bool,
    /// Also 32-bit (float or whole-number) files, stored as 24-bit, which
    /// FLAC holds at most. Not bit-exact, though the change is far below hearing.
    pub to_24: bool,
    /// Make stereo files whose sides are the same into mono files.
    pub mono: bool,
    /// Count sides as the same when they differ by less than `NEAR_DB`
    /// (the mono file is then their average). Off: only exact copies.
    pub near: bool,
}

/// How close two sides must be for `near`: -90 dB below full scale.
pub const NEAR_DB: f32 = -90.0;

/// What happened to one file.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub from: PathBuf,
    /// The new, smaller file; None when the file was left as it is.
    pub to: Option<PathBuf>,
    pub bytes_before: u64,
    pub bytes_after: u64,
    pub flac: bool,
    pub mono: bool,
    /// Channels of the new file.
    pub channels: u16,
    /// Why nothing was made.
    pub skipped: Option<String>,
}

impl Outcome {
    fn skip(from: &Path, bytes: u64, why: impl Into<String>) -> Self {
        Outcome {
            from: from.to_path_buf(),
            to: None,
            bytes_before: bytes,
            bytes_after: bytes,
            flac: false,
            mono: false,
            channels: 0,
            skipped: Some(why.into()),
        }
    }
}

fn uncompressed(path: &Path, codec: &str) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    ["wav", "wave", "bwf", "aif", "aiff", "aifc"].contains(&ext.as_str())
        && codec.starts_with("PCM")
}

/// Reads a whole file through `f`, chunk by chunk, at its own rate and channels.
fn each_chunk(
    path: &Path,
    channels: usize,
    mut f: impl FnMut(&[f32]) -> bool,
) -> Result<(), String> {
    let mut r = AudioReader::open(path, channels, None).map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    while r.read_into(&mut buf).map_err(|e| e.to_string())? {
        if !f(&buf) {
            return Ok(());
        }
        buf.clear();
    }
    Ok(())
}

/// Whether a stereo file's sides are the same: exactly, or within `NEAR_DB`.
pub fn sides_match(path: &Path, near: bool) -> Result<bool, String> {
    let limit = if near {
        10f32.powf(NEAR_DB / 20.0)
    } else {
        0.0
    };
    let mut same = true;
    each_chunk(path, 2, |buf| {
        same = buf
            .as_chunks::<2>()
            .0
            .iter()
            .all(|f| (f[0] - f[1]).abs() <= limit);
        same
    })?;
    Ok(same)
}

/// A float sample as a whole number at `bits`, the way decoders read one
/// (divided by 2 to the power `bits - 1`), so a whole-number file comes back exact.
fn to_int(v: f32, bits: u16) -> i32 {
    let scale = (1i64 << (bits - 1)) as f64;
    (f64::from(v) * scale).round().clamp(-scale, scale - 1.0) as i32
}

/// A file name in `dir` from `stem` and `ext` that is not taken.
fn free_name(dir: &Path, stem: &str, ext: &str, avoid: &Path) -> PathBuf {
    let first = dir.join(format!("{stem}.{ext}"));
    if !first.exists() && first != avoid {
        return first;
    }
    (2..)
        .map(|n| dir.join(format!("{stem} ({n}).{ext}")))
        .find(|p| !p.exists())
        .unwrap_or(first)
}

/// Makes the smaller copy of `path`, if one can be made. `mono_ok` is false
/// when the session uses the file in a way a mono file would change (a
/// clip taking one side only, or a 3D Space session).
pub fn shrink(path: &Path, opts: Options, mono_ok: bool) -> Outcome {
    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let info = match probe(path) {
        Ok(i) => i,
        Err(e) => return Outcome::skip(path, size, format!("could not be read: {e}")),
    };
    let raw = uncompressed(path, &info.codec);
    let is_flac = info.codec == "FLAC";
    if !raw && !is_flac {
        return Outcome::skip(
            path,
            size,
            format!(
                "{} is already compressed; making it again would lose quality",
                info.codec
            ),
        );
    }
    let float = info.codec.contains("float");
    let bits = info.bit_depth.unwrap_or(16).clamp(8, 32) as u16;
    let deep = float || bits > 24;
    let out_bits = if deep { 24 } else { bits };
    let exact = !deep;
    if deep && !opts.to_24 && !(opts.mono && info.channels == 2) {
        return Outcome::skip(path, size, format!("{} audio; FLAC holds at most 24-bit (tick the 32-bit choice to store it as 24-bit)", info.codec));
    }
    let flac = opts.flac && raw && (!deep || opts.to_24);
    let mono = opts.mono
        && mono_ok
        && info.channels == 2
        && matches!(sides_match(path, opts.near), Ok(true));
    if !flac && !mono {
        let why = if opts.flac && raw && deep {
            "32-bit audio; FLAC holds at most 24-bit (tick the 32-bit choice to store it as 24-bit)"
        } else if opts.mono && info.channels == 2 && !mono_ok {
            "a clip uses one side of it, or the session is in 3D Space, so it stays stereo"
        } else if opts.mono && info.channels == 2 {
            "its two sides differ, and it is already as small as it gets"
        } else if is_flac {
            "already FLAC"
        } else {
            "nothing to change"
        };
        return Outcome::skip(path, size, why);
    }
    // A stereo float file made mono without FLAC stays a WAV; FLAC needs 24 bits or fewer.
    let kind = if flac || is_flac {
        FormatKind::Flac
    } else {
        FormatKind::Wav
    };
    if kind == FormatKind::Wav && deep {
        return Outcome::skip(
            path,
            size,
            "32-bit audio stays as it is unless it may be stored as 24-bit",
        );
    }
    let in_ch = usize::from(info.channels);
    let out_ch: u16 = if mono { 1 } else { info.channels };
    let dir = path.parent().unwrap_or(Path::new("."));
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Audio")
        .to_string();
    let ext = if kind == FormatKind::Flac {
        "flac"
    } else {
        "wav"
    };
    let stem = if mono { format!("{stem} (mono)") } else { stem };
    let out = free_name(dir, &stem, ext, path);

    let fmt = Resolved {
        kind,
        bits: out_bits,
        rate: info.sample_rate,
        channels: out_ch,
        mp3_mode: Mp3Mode::Cbr,
        mp3_bitrate: 192,
        mp3_quality: 2,
        aac_bitrate: 128,
        flac_level: FLAC_MAX_LEVEL,
        dither: false,
        mix: [0.5, 0.5],
    };
    let written = (|| -> Result<(), String> {
        let mut w =
            Writer::create(&out, &fmt, info.sample_rate, out_ch).map_err(|e| e.to_string())?;
        let mut ints = Vec::new();
        let mut err = None;
        each_chunk(path, in_ch, |buf| {
            ints.clear();
            if mono {
                ints.extend(buf.as_chunks::<2>().0.iter().map(|f| {
                    to_int(
                        if exact && !opts.near {
                            f[0]
                        } else {
                            (f[0] + f[1]) * 0.5
                        },
                        out_bits,
                    )
                }));
            } else {
                ints.extend(buf.iter().map(|v| to_int(*v, out_bits)));
            }
            if let Err(e) = w.write_int(&ints) {
                err = Some(e.to_string());
                return false;
            }
            true
        })?;
        if let Some(e) = err {
            return Err(e);
        }
        w.finish().map_err(|e| e.to_string())?;
        Ok(())
    })();
    let fail = |why: String| {
        let _ = std::fs::remove_file(&out);
        Outcome::skip(path, size, why)
    };
    if let Err(e) = written {
        return fail(format!("the smaller copy could not be written: {e}"));
    }
    // Read both back and compare. Exact files must match to the bit; a
    // 32-bit file stored as 24-bit, or a near-mono average, within one step.
    let step = if exact && !(mono && opts.near) {
        0.0
    } else {
        1.0 / (1u32 << (out_bits - 1)) as f32 * 1.01
            + if mono && opts.near {
                10f32.powf(NEAR_DB / 20.0)
            } else {
                0.0
            }
    };
    match verify(path, &out, in_ch, mono, step) {
        Ok(true) => {}
        Ok(false) => {
            return fail(
                "the copy did not match the original exactly, so the original is kept".into(),
            )
        }
        Err(e) => return fail(format!("the copy could not be checked: {e}")),
    }
    let after = std::fs::metadata(&out).map(|m| m.len()).unwrap_or(u64::MAX);
    if after >= size {
        return fail("the copy was not smaller".into());
    }
    Outcome {
        from: path.to_path_buf(),
        to: Some(out),
        bytes_before: size,
        bytes_after: after,
        flac: kind == FormatKind::Flac && !is_flac,
        mono,
        channels: out_ch,
        skipped: None,
    }
}

/// Whether `copy` holds what `original` does: the same samples (within
/// `step`), the same length, and for a mono copy, each side of the original.
fn verify(
    original: &Path,
    copy: &Path,
    in_ch: usize,
    mono: bool,
    step: f32,
) -> Result<bool, String> {
    let mut a = AudioReader::open(original, in_ch, None).map_err(|e| e.to_string())?;
    let out_ch = if mono { 1 } else { in_ch };
    let mut b = AudioReader::open(copy, out_ch, None).map_err(|e| e.to_string())?;
    let (mut abuf, mut bbuf) = (Vec::new(), Vec::new());
    let (mut apos, mut bpos) = (0usize, 0usize);
    let mut a_done = false;
    let mut b_done = false;
    loop {
        if apos >= abuf.len() && !a_done {
            abuf.clear();
            apos = 0;
            a_done = !a.read_into(&mut abuf).map_err(|e| e.to_string())?;
        }
        if bpos >= bbuf.len() && !b_done {
            bbuf.clear();
            bpos = 0;
            b_done = !b.read_into(&mut bbuf).map_err(|e| e.to_string())?;
        }
        let a_left = (abuf.len() - apos) / in_ch;
        let b_left = (bbuf.len() - bpos) / out_ch;
        if a_left == 0 || b_left == 0 {
            if a_left == 0 && b_left == 0 && a_done && b_done {
                return Ok(true);
            }
            if (a_left == 0 && a_done) || (b_left == 0 && b_done) {
                // One ended before the other.
                return Ok(false);
            }
            continue;
        }
        let n = a_left.min(b_left);
        for k in 0..n {
            let fa = &abuf[apos + k * in_ch..apos + (k + 1) * in_ch];
            let fb = &bbuf[bpos + k * out_ch..bpos + (k + 1) * out_ch];
            let ok = if mono {
                (fa[0] - fb[0]).abs() <= step && (fa[1] - fb[0]).abs() <= step
            } else {
                fa.iter().zip(fb).all(|(x, y)| (x - y).abs() <= step)
            };
            if !ok {
                return Ok(false);
            }
        }
        apos += n * in_ch;
        bpos += n * out_ch;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav(path: &Path, ch: u16, bits: u16, frames: usize, f: impl Fn(usize, usize) -> i32) {
        let spec = hound::WavSpec {
            channels: ch,
            sample_rate: 48_000,
            bits_per_sample: bits,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(path, spec).unwrap();
        for i in 0..frames {
            for c in 0..usize::from(ch) {
                if bits == 16 {
                    w.write_sample(f(i, c) as i16).unwrap();
                } else {
                    w.write_sample(f(i, c)).unwrap();
                }
            }
        }
        w.finalize().unwrap();
    }

    fn tone(i: usize, amp: f64) -> i32 {
        ((i as f64 * 0.013).sin() * amp + (i as f64 * 0.31).sin() * amp * 0.1) as i32
    }

    fn all_opts() -> Options {
        Options {
            flac: true,
            to_24: false,
            mono: true,
            near: false,
        }
    }

    #[test]
    fn a_wav_becomes_a_smaller_exact_flac() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("voice.wav");
        wav(&p, 2, 24, 48_000, |i, c| tone(i + c * 7, 3_000_000.0));
        let o = shrink(&p, all_opts(), true);
        assert_eq!(o.skipped, None, "{o:?}");
        assert!(o.flac && !o.mono);
        assert!(o.bytes_after < o.bytes_before);
        assert_eq!(
            o.to.as_deref(),
            Some(dir.path().join("voice.flac").as_path())
        );
        // The original is untouched.
        assert!(p.is_file());
    }

    #[test]
    fn identical_sides_become_mono() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("mono in stereo.wav");
        wav(&p, 2, 16, 48_000, |i, _| tone(i, 12_000.0));
        let o = shrink(&p, all_opts(), true);
        assert_eq!(o.skipped, None, "{o:?}");
        assert!(o.mono && o.flac);
        assert_eq!(o.channels, 1);
        assert_eq!(probe(o.to.as_ref().unwrap()).unwrap().channels, 1);
        // Not when the session needs it stereo.
        let o = shrink(
            &p,
            Options {
                flac: false,
                ..all_opts()
            },
            false,
        );
        assert!(o.skipped.is_some());
    }

    #[test]
    fn sides_differing_by_one_step_count_only_when_near_is_on() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("almost.wav");
        wav(&p, 2, 16, 4_800, |i, c| {
            tone(i, 12_000.0) + i32::from(c == 1 && i % 3 == 0)
        });
        assert!(!sides_match(&p, false).unwrap());
        assert!(sides_match(&p, true).unwrap());
        let o = shrink(
            &p,
            Options {
                near: true,
                ..all_opts()
            },
            true,
        );
        assert!(o.mono, "{o:?}");
    }

    #[test]
    fn mono_only_makes_a_mono_wav() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("m.wav");
        wav(&p, 2, 16, 48_000, |i, _| tone(i, 9_000.0));
        let o = shrink(
            &p,
            Options {
                flac: false,
                ..all_opts()
            },
            true,
        );
        assert_eq!(o.skipped, None, "{o:?}");
        assert!(o.mono && !o.flac);
        assert!(o
            .to
            .as_ref()
            .unwrap()
            .to_string_lossy()
            .ends_with("m (mono).wav"));
    }

    #[test]
    fn lossy_and_float_files_are_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("f.wav");
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 48_000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut w = hound::WavWriter::create(&p, spec).unwrap();
        for i in 0..9_600 {
            w.write_sample((i as f32 * 0.01).sin() * 0.3).unwrap();
            w.write_sample((i as f32 * 0.017).sin() * 0.3).unwrap();
        }
        w.finalize().unwrap();
        let o = shrink(&p, all_opts(), true);
        assert!(o.skipped.unwrap().contains("24-bit"));
        // Allowed: stored as 24-bit, within one step.
        let o = shrink(
            &p,
            Options {
                to_24: true,
                ..all_opts()
            },
            true,
        );
        assert_eq!(o.skipped, None, "{o:?}");
        assert!(o.flac);
    }

    #[test]
    fn whole_numbers_round_trip() {
        for bits in [8u16, 16, 24] {
            let scale = (1i64 << (bits - 1)) as f32;
            for x in [-(scale as i32), -1, 0, 1, scale as i32 - 1] {
                assert_eq!(to_int(x as f32 / scale, bits), x);
            }
        }
    }
}
