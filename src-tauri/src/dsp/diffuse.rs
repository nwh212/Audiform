// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Diffuse sound for 3D Space sessions: Spread and Spatialize.
//!
//! A decorrelator turns one signal into four that sound the same but no
//! longer line up sample for sample: each goes through its own short
//! delay and a chain of all-pass filters (which change phase, not level).
//! Played from different speakers, such copies are heard as a wide,
//! enveloping sound instead of one phantom point between them.
//!
//! Spread sends part of a source through one, to every speaker of the
//! layers in use. Spatialize takes what differs between a stereo source's
//! sides (left minus right) through another, to the surround and height
//! speakers (left ones in phase, right ones inverted), and feeds what the
//! sides share to the center. Our own method, built from textbook parts.

use crate::dsp::space::speakers;
use crate::session::model::SpaceLayout;

/// Copies a decorrelator makes.
pub const TAPS: usize = 4;

/// One Schroeder all-pass section: flat level, smeared phase.
struct AllPass {
    buf: Vec<f32>,
    at: usize,
    g: f32,
}

impl AllPass {
    fn new(len: usize, g: f32) -> Self {
        Self {
            buf: vec![0.0; len.max(1)],
            at: 0,
            g,
        }
    }

    fn run(&mut self, x: f32) -> f32 {
        let d = self.buf[self.at];
        let v = x + self.g * d;
        self.buf[self.at] = v;
        self.at = (self.at + 1) % self.buf.len();
        d - self.g * v
    }
}

/// A pre-delay then three all-pass sections.
struct Path {
    pre: Vec<f32>,
    at: usize,
    ap: [AllPass; 3],
}

impl Path {
    fn run(&mut self, x: f32) -> f32 {
        let d = self.pre[self.at];
        self.pre[self.at] = x;
        self.at = (self.at + 1) % self.pre.len();
        self.ap.iter_mut().fold(d, |v, a| a.run(v))
    }
}

/// Four decorrelated copies of one signal. All buffers are made up front,
/// so running it never allocates.
pub struct Decorrelator {
    paths: [Path; TAPS],
}

impl Decorrelator {
    /// `seed` picks a different set of delays, so two decorrelators on the
    /// same source do not match each other either.
    pub fn new(rate: u32, seed: usize) -> Self {
        let ms = |v: f32| ((v * rate as f32) / 1000.0).round().max(1.0) as usize;
        // Pre-delays 5 to 19 ms and all-pass lengths at prime-ish spacings.
        const PRE: [f32; 8] = [5.3, 9.7, 13.1, 17.9, 7.1, 11.3, 15.7, 19.3];
        const AP: [[f32; 3]; 8] = [
            [1.31, 3.71, 7.93],
            [1.79, 4.33, 8.69],
            [2.11, 5.03, 9.41],
            [2.53, 5.77, 10.13],
            [1.13, 3.97, 8.27],
            [1.97, 4.61, 9.07],
            [2.39, 5.39, 9.83],
            [2.71, 6.11, 10.61],
        ];
        let path = |k: usize| {
            let i = (k + seed * TAPS) % PRE.len();
            Path {
                pre: vec![0.0; ms(PRE[i])],
                at: 0,
                ap: AP[i].map(|l| AllPass::new(ms(l), 0.6)),
            }
        };
        Self {
            paths: [path(0), path(1), path(2), path(3)],
        }
    }

    pub fn run(&mut self, x: f32) -> [f32; TAPS] {
        let mut out = [0.0; TAPS];
        for (o, p) in out.iter_mut().zip(self.paths.iter_mut()) {
            *o = p.run(x);
        }
        out
    }
}

/// What Spread and Spatialize send to each speaker, worked out from a spot.
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    /// Spread: per speaker, the gain for its decorrelated copy (0 off).
    pub spread: Vec<f32>,
    /// Spatialize: per speaker, the gain for the decorrelated side sound
    /// (positive on the left, negative on the right, 0 off).
    pub side: Vec<f32>,
    /// Spatialize: the center speaker and the gain for the shared sound.
    pub center: Option<(usize, f32)>,
    /// Each speaker folded to stereo for listening (left, right).
    pub fold: Vec<[f32; 2]>,
    /// What the direct (placed) sound keeps, so the total stays as loud.
    pub direct: f32,
    /// Something to add: Spread, Spatialize, or both.
    pub on: bool,
    pub spreads: bool,
    pub sides: bool,
}

/// The field for a source at height `z` with `spread` and `spatialize`
/// (0 to 1 each). Spatialize applies only to stereo sources.
pub fn field(layout: SpaceLayout, z: f32, spread: f32, spatialize: f32, stereo: bool) -> Field {
    let sp = speakers(layout);
    let n = sp.len();
    let spread = spread.clamp(0.0, 1.0);
    let spat = if stereo {
        spatialize.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let has_top = sp.iter().any(|s| s.at.is_some_and(|a| a[2] > 0.5));
    let fold: Vec<[f32; 2]> = sp
        .iter()
        .map(|s| match s.at {
            Some(a) => {
                let t = ((a[0] + 1.0) / 2.0).clamp(0.0, 1.0) * std::f32::consts::FRAC_PI_2;
                [t.cos(), t.sin()]
            }
            None => [0.0, 0.0],
        })
        .collect();
    let mut f = Field {
        spread: vec![0.0; n],
        side: vec![0.0; n],
        center: None,
        fold,
        direct: 1.0,
        on: spread > 0.0 || spat > 0.0,
        spreads: spread > 0.0,
        sides: spat > 0.0,
    };
    if spread > 0.0 {
        // Every placed speaker of the layers the height uses: the floor
        // always, the top as the sound rises (and both fully at mid height).
        let lift = if has_top { z.clamp(0.0, 1.0) } else { 0.0 };
        let weight = |a: [f32; 3]| {
            if a[2] > 0.5 {
                lift.min(0.5) * 2.0
            } else {
                1.0 - (lift - 0.5).max(0.0)
            }
        };
        let w: Vec<f32> = sp.iter().map(|s| s.at.map_or(0.0, weight)).collect();
        let p = w.iter().map(|v| v * v).sum::<f32>().sqrt().max(1e-6);
        for (o, v) in f.spread.iter_mut().zip(&w) {
            *o = spread.sqrt() * v / p;
        }
        f.direct *= (1.0 - spread).sqrt();
    }
    if spat > 0.0 {
        // The side sound to every speaker not in the front row, left ones in
        // phase and right ones inverted; the center ones stay out of it.
        let w: Vec<f32> = sp
            .iter()
            .map(|s| match s.at {
                Some(a) if a[1] < 0.5 || a[2] > 0.5 => {
                    if a[0] < -0.1 {
                        1.0
                    } else if a[0] > 0.1 {
                        -1.0
                    } else {
                        0.0
                    }
                }
                _ => 0.0,
            })
            .collect();
        let p = w.iter().map(|v| v * v).sum::<f32>().sqrt();
        if p > 0.0 {
            for (o, v) in f.side.iter_mut().zip(&w) {
                *o = spat * 0.8 * v / p;
            }
        }
        f.center = sp
            .iter()
            .position(|s| {
                s.at.is_some_and(|a| a[0].abs() < 0.1 && a[1] > 0.5 && a[2] < 0.5)
            })
            .map(|c| (c, spat * 0.5));
        // The front pair gives up a little of what moves to the center and around.
        f.direct *= 1.0 - 0.3 * spat;
    }
    f
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_keep_level_but_differ() {
        let rate = 48_000;
        let mut d = Decorrelator::new(rate, 0);
        let n = rate as usize;
        let mut e_in = 0.0f64;
        let mut e_out = [0.0f64; TAPS];
        let mut cross = 0.0f64;
        let mut s = 1u32;
        for _ in 0..n {
            // White noise, the same every run.
            s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let x = (s >> 8) as f32 / (1u32 << 24) as f32 - 0.5;
            let y = d.run(x);
            e_in += f64::from(x * x);
            for (e, v) in e_out.iter_mut().zip(y) {
                *e += f64::from(v * v);
            }
            cross += f64::from(y[0] * y[1]);
        }
        for e in e_out {
            assert!((e / e_in - 1.0).abs() < 0.1, "level kept: {}", e / e_in);
        }
        let corr = cross / (e_out[0] * e_out[1]).sqrt();
        assert!(corr.abs() < 0.1, "copies differ: {corr}");
    }

    #[test]
    fn fields_keep_power_and_spread_where_they_should() {
        let f = field(SpaceLayout::Surround714, 0.0, 1.0, 0.0, false);
        let p: f32 = f.spread.iter().map(|v| v * v).sum();
        assert!((p - 1.0).abs() < 1e-4, "{p}");
        assert_eq!(f.direct, 0.0);
        // At ear level the height speakers stay out of it; the LFE always does.
        assert_eq!(f.spread[8], 0.0);
        assert_eq!(f.spread[3], 0.0);
        let f = field(SpaceLayout::Surround51, 0.0, 0.0, 1.0, true);
        // Rear left in phase, rear right inverted, front pair out of it, center fed.
        assert!(f.side[4] > 0.0 && f.side[5] < 0.0);
        assert_eq!(f.side[0], 0.0);
        assert_eq!(f.center.map(|c| c.0), Some(2));
        // Mono sources have no sides to spatialize.
        assert!(!field(SpaceLayout::Surround51, 0.0, 0.0, 1.0, false).on);
    }
}
