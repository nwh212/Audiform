// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

//! The last moments of what is playing, for the Phase Wheel.
//!
//! The audio thread writes each output frame (left and right, after the
//! master fader and the mono check) into a fixed ring of atomics: two
//! relaxed stores a frame, no allocation, no locks. The interface asks
//! for an analysis now and then; it reads the newest frames, which may
//! tear against a write in progress. For a picture that is fine.
//!
//! The analysis compares two signals (left, right, mid or side) bin by
//! bin: for each frequency, the phase difference between them (0 when
//! they move together, half a turn when they are opposite) and how loud
//! it is. Plus the correlation of the two over the whole window, from
//! +1 (the same) through 0 (unrelated) to -1 (opposite).

use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

use realfft::RealFftPlanner;

/// Frames kept: enough for the largest analysis window.
pub const CAPACITY: usize = 1 << 15;

/// Analysis window sizes offered, smallest (lightest) first.
pub const SIZES: [usize; 5] = [1024, 2048, 4096, 8192, 16384];

static RING: [AtomicU32; CAPACITY * 2] = [const { AtomicU32::new(0) }; CAPACITY * 2];
/// Frames written so far (wrapping), so the newest frame is `WRITTEN - 1`.
static WRITTEN: AtomicUsize = AtomicUsize::new(0);

/// Records one output frame. Audio thread: two stores, nothing else.
#[inline]
pub fn push(l: f32, r: f32) {
    let w = WRITTEN.load(Ordering::Relaxed);
    let at = (w % CAPACITY) * 2;
    RING[at].store(l.to_bits(), Ordering::Relaxed);
    RING[at + 1].store(r.to_bits(), Ordering::Relaxed);
    WRITTEN.store(w.wrapping_add(1), Ordering::Release);
}

/// The newest `n` frames (n at most CAPACITY), oldest first, as (left, right).
pub fn latest(n: usize) -> Vec<[f32; 2]> {
    let n = n.min(CAPACITY);
    let end = WRITTEN.load(Ordering::Acquire);
    (0..n)
        .map(|k| {
            let at = (end.wrapping_sub(n - k) % CAPACITY) * 2;
            [
                f32::from_bits(RING[at].load(Ordering::Relaxed)),
                f32::from_bits(RING[at + 1].load(Ordering::Relaxed)),
            ]
        })
        .collect()
}

/// Which signal a side of the comparison reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Signal {
    Left,
    Right,
    /// (L + R) / 2
    Mid,
    /// (L - R) / 2
    Side,
}

impl Signal {
    fn of(self, f: [f32; 2]) -> f32 {
        match self {
            Signal::Left => f[0],
            Signal::Right => f[1],
            Signal::Mid => (f[0] + f[1]) * 0.5,
            Signal::Side => (f[0] - f[1]) * 0.5,
        }
    }
}

/// One frequency bin of the comparison.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bin {
    /// Phase of `a` relative to `b`, -pi to pi (0 is in phase).
    pub phase: f32,
    /// Level, 0 (at or below -72 dBFS) to 1 (full scale).
    pub level: f32,
    /// Where the bin sits, 0 (lowest) to 1 (highest), on a log scale.
    pub pitch: f32,
}

pub struct Phase {
    /// -1 to 1; 0 when either side is silent.
    pub correlation: f32,
    pub bins: Vec<Bin>,
}

/// Level range shown: this many dB below full scale is the center.
const FLOOR_DB: f32 = 72.0;

/// Compares `a` and `b` over `frames` (each [left, right]).
pub fn analyse(frames: &[[f32; 2]], a: Signal, b: Signal) -> Phase {
    let n = frames.len();
    let mut sa: Vec<f32> = frames.iter().map(|f| a.of(*f)).collect();
    let mut sb: Vec<f32> = frames.iter().map(|f| b.of(*f)).collect();
    let (mut ab, mut aa, mut bb) = (0f64, 0f64, 0f64);
    for (x, y) in sa.iter().zip(&sb) {
        ab += f64::from(*x) * f64::from(*y);
        aa += f64::from(*x) * f64::from(*x);
        bb += f64::from(*y) * f64::from(*y);
    }
    let correlation = if aa > 1e-12 && bb > 1e-12 {
        (ab / (aa * bb).sqrt()) as f32
    } else {
        0.0
    };
    if n < 4 {
        return Phase {
            correlation,
            bins: Vec::new(),
        };
    }
    // Hann window; its sum sets full scale.
    let mut sum = 0f32;
    for i in 0..n {
        let w = 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / n as f32).cos();
        sa[i] *= w;
        sb[i] *= w;
        sum += w;
    }
    let fft = RealFftPlanner::<f32>::new().plan_fft_forward(n);
    let mut fa = fft.make_output_vec();
    let mut fb = fft.make_output_vec();
    if fft.process(&mut sa, &mut fa).is_err() || fft.process(&mut sb, &mut fb).is_err() {
        return Phase {
            correlation,
            bins: Vec::new(),
        };
    }
    let full = sum / 2.0;
    let top = ((n / 2) as f32).ln();
    let mut bins = Vec::with_capacity(n / 2);
    for k in 1..n / 2 {
        let (x, y) = (fa[k], fb[k]);
        let mag = (x.norm() + y.norm()) * 0.5 / full;
        let db = 20.0 * mag.max(1e-9).log10();
        let level = ((db + FLOOR_DB) / FLOOR_DB).clamp(0.0, 1.0);
        if level <= 0.0 {
            continue;
        }
        let rel = x * y.conj();
        bins.push(Bin {
            phase: rel.im.atan2(rel.re),
            level,
            pitch: (k as f32).ln() / top,
        });
    }
    Phase { correlation, bins }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(n: usize, shift: f32) -> Vec<[f32; 2]> {
        // 64 cycles over the window: right is left shifted by `shift` radians.
        (0..n)
            .map(|i| {
                let t = std::f32::consts::TAU * 64.0 * i as f32 / n as f32;
                [0.5 * t.sin(), 0.5 * (t - shift).sin()]
            })
            .collect()
    }

    fn loudest(p: &Phase) -> Bin {
        *p.bins
            .iter()
            .max_by(|a, b| a.level.total_cmp(&b.level))
            .unwrap()
    }

    #[test]
    fn in_phase_opposite_and_a_quarter_turn() {
        let same = analyse(&tone(4096, 0.0), Signal::Left, Signal::Right);
        assert!((same.correlation - 1.0).abs() < 1e-3);
        assert!(loudest(&same).phase.abs() < 1e-3);
        // -6 dBFS sine: level 1 - 6/72.
        assert!((loudest(&same).level - (1.0 - 6.02 / 72.0)).abs() < 0.01);

        let flipped = analyse(
            &tone(4096, std::f32::consts::PI),
            Signal::Left,
            Signal::Right,
        );
        assert!((flipped.correlation + 1.0).abs() < 1e-3);
        assert!((loudest(&flipped).phase.abs() - std::f32::consts::PI).abs() < 1e-3);

        let quarter = analyse(
            &tone(4096, std::f32::consts::FRAC_PI_2),
            Signal::Left,
            Signal::Right,
        );
        assert!(quarter.correlation.abs() < 0.01);
        assert!((loudest(&quarter).phase - std::f32::consts::FRAC_PI_2).abs() < 1e-2);

        // Mid against side of an in-phase signal: side is silent.
        let ms = analyse(&tone(4096, 0.0), Signal::Mid, Signal::Side);
        assert_eq!(ms.correlation, 0.0);
    }

    #[test]
    fn the_ring_returns_the_newest_frames_in_order() {
        for i in 0..(CAPACITY + 10) {
            push(i as f32, -(i as f32));
        }
        let got = latest(3);
        let last = (CAPACITY + 9) as f32;
        assert_eq!(
            got,
            vec![
                [last - 2.0, 2.0 - last],
                [last - 1.0, 1.0 - last],
                [last, -last]
            ]
        );
    }
}
