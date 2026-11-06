// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

//! Zero crossings: places where the sound passes through silence, so a
//! cut or a selection edge there does not click. From Altered Audio Editor.

/// The sample nearest `at` where the sound crosses zero, looking up to `reach` samples
/// either way. A crossing sits between two samples with opposite signs (or on an exact
/// zero); the one of the pair closer to zero is returned. None when the sound never
/// crosses inside the reach, such as silence or a held offset.
pub fn nearest_zero_crossing(mono: &[f32], at: usize, reach: usize) -> Option<usize> {
    if mono.is_empty() {
        return None;
    }
    let at = at.min(mono.len() - 1);
    let crosses = |i: usize| -> Option<usize> {
        let v = mono[i];
        if v == 0.0 {
            // A zero between two non-zero samples is a crossing only when it is not a flat run.
            return Some(i);
        }
        let prev = *mono.get(i.checked_sub(1)?)?;
        if prev != 0.0 && (prev < 0.0) != (v < 0.0) {
            Some(if prev.abs() < v.abs() { i - 1 } else { i })
        } else {
            None
        }
    };
    // Silence is not a crossing: ask for at least some movement around the point.
    let moving = |i: usize| {
        let lo = i.saturating_sub(2);
        let hi = (i + 2).min(mono.len() - 1);
        mono[lo..=hi].iter().any(|v| *v != 0.0)
    };
    for d in 0..=reach {
        for i in [at.checked_sub(d), Some(at + d)].into_iter().flatten() {
            if i < mono.len() && moving(i) {
                if let Some(c) = crosses(i) {
                    return Some(c);
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_crossings_are_found_nearest_first() {
        // A 100 Hz sine at 8 kHz crosses zero every 40 samples, near 0, 40, 80...
        let s: Vec<f32> = (0..400)
            .map(|i| (i as f32 * 100.0 * std::f32::consts::TAU / 8_000.0 + 0.3).sin())
            .collect();
        for at in [57, 100, 213] {
            let c = nearest_zero_crossing(&s, at, 60).unwrap();
            assert!(c.abs_diff(at) <= 21, "{at} -> {c}");
            let flips = |i: usize| (s[i] < 0.0) != (s[i + 1] < 0.0);
            assert!(
                s[c].abs() < 0.2 && (flips(c) || (c > 0 && flips(c - 1))),
                "{at} -> {c}"
            );
        }
        assert_eq!(nearest_zero_crossing(&[0.0; 50], 25, 40), None, "silence");
        assert_eq!(
            nearest_zero_crossing(&[0.5; 50], 25, 40),
            None,
            "held offset"
        );
        assert_eq!(nearest_zero_crossing(&[], 0, 10), None);
    }
}
