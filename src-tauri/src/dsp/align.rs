// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Phase Align: finds how far one recording of a sound trails another
//! (two mics on one voice, a camera track and a recorder), by
//! cross-correlation, so the later one can be moved into line. Also
//! says when the best match is upside down (polarity inverted).

use realfft::num_complex::Complex32;
use realfft::RealFftPlanner;

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Alignment {
    /// Samples `other` trails `reference` by (negative: it is early).
    pub lag: i64,
    /// How alike they are once lined up, 0 to 1.
    pub score: f32,
    /// The best match has the polarity flipped.
    pub inverted: bool,
}

/// Cross-correlates `other` against `reference` (mono, same rate, same
/// start time) for lags up to `max_lag` either way.
pub fn align(reference: &[f32], other: &[f32], max_lag: usize) -> Option<Alignment> {
    let len = reference.len().min(other.len());
    if len < 64 {
        return None;
    }
    let (a, b) = (&reference[..len], &other[..len]);
    let energy = |x: &[f32]| x.iter().map(|v| f64::from(*v) * f64::from(*v)).sum::<f64>();
    let norm = (energy(a) * energy(b)).sqrt();
    if norm <= 1e-12 {
        return None;
    }
    let n = (len + max_lag + 1).next_power_of_two() * 2;
    let mut planner = RealFftPlanner::<f32>::new();
    let fwd = planner.plan_fft_forward(n);
    let inv = planner.plan_fft_inverse(n);
    let spectrum = |x: &[f32]| {
        let mut buf = vec![0f32; n];
        buf[..x.len()].copy_from_slice(x);
        let mut out = fwd.make_output_vec();
        let _ = fwd.process(&mut buf, &mut out);
        out
    };
    let fa = spectrum(a);
    let fb = spectrum(b);
    let mut prod: Vec<Complex32> = fa.iter().zip(&fb).map(|(x, y)| x.conj() * y).collect();
    prod[0].im = 0.0;
    let last = prod.len() - 1;
    prod[last].im = 0.0;
    let mut corr = vec![0f32; n];
    let _ = inv.process(&mut prod, &mut corr);
    // corr[k] = sum of a[t] * b[t + k]; negative lags wrap to the end.
    let at = |k: i64| {
        corr[if k >= 0 {
            k as usize
        } else {
            (n as i64 + k) as usize
        }] / n as f32
    };
    let max = max_lag.min(len - 1) as i64;
    let best = (-max..=max).max_by(|x, y| at(*x).abs().total_cmp(&at(*y).abs()))?;
    let v = at(best);
    // Score against the overlap actually compared at that lag.
    let overlap = (len - best.unsigned_abs() as usize).max(1) as f64 / len as f64;
    Some(Alignment {
        lag: best,
        score: ((f64::from(v.abs()) / norm / overlap).min(1.0)) as f32,
        inverted: v < 0.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn voice(frames: usize) -> Vec<f32> {
        let mut seed = 11u32;
        (0..frames)
            .map(|i| {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                let n = seed as f32 / u32::MAX as f32 - 0.5;
                n * 0.3 + (i as f32 * 0.05).sin() * 0.2
            })
            .collect()
    }

    #[test]
    fn finds_a_delay_and_a_flip() {
        let a = voice(48_000);
        // The second mic is 2.5 ms (120 samples) further away and quieter.
        let mut b = vec![0f32; 120];
        b.extend(a.iter().map(|v| v * 0.6));
        b.truncate(a.len());
        let r = align(&a, &b, 2_400).unwrap();
        assert_eq!(r.lag, 120);
        assert!(!r.inverted);
        assert!(r.score > 0.9, "score {}", r.score);
        // Early and flipped.
        let c: Vec<f32> = a[50..].iter().map(|v| -v).collect();
        let r = align(&a, &c, 2_400).unwrap();
        assert_eq!(r.lag, -50);
        assert!(r.inverted);
    }

    #[test]
    fn silence_has_no_answer() {
        assert!(align(&[0.0; 1000], &[0.0; 1000], 100).is_none());
        assert!(align(&[0.1; 10], &[0.1; 10], 5).is_none());
    }
}
