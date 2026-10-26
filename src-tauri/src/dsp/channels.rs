// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

//! The Source Channels: how a file's channels reach a track's left and
//! right. Each source channel goes to the left and the right by its own
//! amount (negative flips its polarity), and the sides are the sums.
//!
//! Channel names come from the channel count, in the standard order of
//! multichannel WAV files: a 6-channel file is L, R, C, LFE, Ls, Rs.

use crate::session::model::{ChannelMix, ChannelSend};

const MINUS_3DB: f32 = std::f32::consts::FRAC_1_SQRT_2;

/// Speaker names for a file with `n` channels, in file order.
pub fn labels(n: usize) -> Vec<String> {
    let named: &[&str] = match n {
        1 => &["M"],
        2 => &["L", "R"],
        3 => &["L", "R", "C"],
        4 => &["L", "R", "Ls", "Rs"],
        5 => &["L", "R", "C", "Ls", "Rs"],
        6 => &["L", "R", "C", "LFE", "Ls", "Rs"],
        7 => &["L", "R", "C", "LFE", "Cs", "Ls", "Rs"],
        8 => &["L", "R", "C", "LFE", "Lrs", "Rrs", "Lss", "Rss"],
        10 => &["L", "R", "C", "LFE", "Ls", "Rs", "Ltf", "Rtf", "Ltr", "Rtr"],
        12 => &[
            "L", "R", "C", "LFE", "Lrs", "Rrs", "Lss", "Rss", "Ltf", "Rtf", "Ltr", "Rtr",
        ],
        _ => &[],
    };
    if named.len() == n {
        named.iter().map(|s| (*s).to_string()).collect()
    } else {
        (1..=n).map(|k| k.to_string()).collect()
    }
}

/// The standard fold-down for `n` channels: fronts straight through,
/// center and surrounds 3 dB down, the low-frequency channel left out.
/// Mono feeds both sides; stereo is left to left and right to right.
/// Files with an unknown order play their first two channels.
pub fn default_mix(n: usize) -> ChannelMix {
    let names = labels(n);
    let sends = names
        .iter()
        .enumerate()
        .map(|(k, name)| {
            let (left, right) = match name.as_str() {
                "M" => (1.0, 1.0),
                "L" => (1.0, 0.0),
                "R" => (0.0, 1.0),
                "C" => (MINUS_3DB, MINUS_3DB),
                "Cs" => (0.5, 0.5),
                "LFE" => (0.0, 0.0),
                s if s.starts_with('L') => (MINUS_3DB, 0.0),
                s if s.starts_with('R') => (0.0, MINUS_3DB),
                _ => match k {
                    0 => (1.0, 0.0),
                    1 => (0.0, 1.0),
                    _ => (0.0, 0.0),
                },
            };
            ChannelSend { left, right }
        })
        .collect();
    ChannelMix { sends }
}

/// What playback uses for a clip of an `n`-channel file: the clip's own
/// mix, or the default. None when the default is the plain mono or
/// stereo path (nothing to mix).
pub fn effective(own: Option<&ChannelMix>, n: usize) -> Option<Vec<[f32; 2]>> {
    if own.is_none() && n <= 2 {
        return None;
    }
    let mix = own.cloned().unwrap_or_else(|| default_mix(n));
    Some(
        (0..n.max(1))
            .map(|k| {
                mix.sends.get(k).map_or([0.0, 0.0], |s| {
                    [s.left.clamp(-1.0, 1.0), s.right.clamp(-1.0, 1.0)]
                })
            })
            .collect(),
    )
}

/// Sums interleaved `input` of `in_ch` channels into stereo by `gains`
/// (one `[left, right]` per source channel), appending to `out`.
pub fn mix_into(input: &[f32], in_ch: usize, gains: &[[f32; 2]], out: &mut Vec<f32>) {
    if in_ch == 0 {
        return;
    }
    out.reserve(input.len() / in_ch * 2);
    for f in input.chunks_exact(in_ch) {
        let (mut l, mut r) = (0.0f32, 0.0f32);
        for (v, g) in f.iter().zip(gains) {
            l += v * g[0];
            r += v * g[1];
        }
        out.push(l);
        out.push(r);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_follow_the_wav_order() {
        assert_eq!(labels(6), ["L", "R", "C", "LFE", "Ls", "Rs"]);
        assert_eq!(labels(12)[11], "Rtr");
        assert_eq!(labels(9), ["1", "2", "3", "4", "5", "6", "7", "8", "9"]);
    }

    #[test]
    fn fold_down_leaves_out_the_lfe() {
        let m = effective(None, 6).unwrap();
        assert_eq!(m[0], [1.0, 0.0]);
        assert_eq!(m[3], [0.0, 0.0]);
        assert!((m[2][0] - MINUS_3DB).abs() < 1e-6 && (m[2][1] - MINUS_3DB).abs() < 1e-6);
        assert_eq!(m[5], [0.0, MINUS_3DB]);
    }

    #[test]
    fn plain_files_need_no_mix() {
        assert!(effective(None, 1).is_none());
        assert!(effective(None, 2).is_none());
        // Unknown orders play their first two channels.
        let m = effective(None, 9).unwrap();
        assert_eq!(&m[..3], &[[1.0, 0.0], [0.0, 1.0], [0.0, 0.0]]);
    }

    #[test]
    fn sums_by_percent() {
        // Center only, to both sides; left flipped into the right.
        let mix = ChannelMix {
            sends: vec![
                ChannelSend {
                    left: 0.0,
                    right: -1.0,
                },
                ChannelSend {
                    left: 0.0,
                    right: 0.0,
                },
                ChannelSend {
                    left: 0.5,
                    right: 0.5,
                },
            ],
        };
        let g = effective(Some(&mix), 3).unwrap();
        let mut out = Vec::new();
        mix_into(&[0.2, 0.9, 0.4], 3, &g, &mut out);
        assert!((out[0] - 0.2).abs() < 1e-6);
        assert!((out[1] - 0.0).abs() < 1e-6);
    }
}
