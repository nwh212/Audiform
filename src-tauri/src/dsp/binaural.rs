// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Binaural listening for 3D Space sessions: each speaker of the layout
//! heard as if it stood around a head, for checking a mix on headphones.
//!
//! The head is a sphere (the textbook spherical-head model): a speaker's
//! sound reaches the far ear later, by the extra path around the head,
//! and duller, through a head-shadow filter that cuts highs more the
//! further the ear faces away. Raised speakers get a little extra air on
//! top. No measured head responses are used, so nothing here is anyone
//! else's data. The low-frequency channel goes to both ears.

use crate::dsp::space::speakers;
use crate::session::model::SpaceLayout;

/// Head radius, meters, and the speed of sound.
const HEAD: f32 = 0.0875;
const SOUND: f32 = 343.0;
/// How far the shadow filter's zero moves, at its brightest and dullest.
const ALPHA_MIN: f32 = 0.1;
const THETA_MIN: f32 = 150.0;

/// A one-pole, one-zero filter.
#[derive(Debug, Clone, Copy, Default)]
struct Shelf {
    b0: f32,
    b1: f32,
    a1: f32,
    x1: f32,
    y1: f32,
}

impl Shelf {
    /// The head shadow for an ear facing `theta` degrees away from the
    /// sound, at `rate`, by the bilinear transform of
    /// (1 + s * alpha / (2 w0)) / (1 + s / (2 w0)).
    fn shadow(theta: f32, rate: u32) -> Shelf {
        let alpha = (1.0 + ALPHA_MIN / 2.0)
            + (1.0 - ALPHA_MIN / 2.0) * (theta / THETA_MIN * std::f32::consts::PI).cos();
        let w0 = SOUND / HEAD;
        let k = 2.0 * rate as f32;
        let (n0, n1) = (1.0, alpha / (2.0 * w0));
        let (d0, d1) = (1.0, 1.0 / (2.0 * w0));
        let a0 = d0 + d1 * k;
        Shelf {
            b0: (n0 + n1 * k) / a0,
            b1: (n0 - n1 * k) / a0,
            a1: (d0 - d1 * k) / a0,
            ..Shelf::default()
        }
    }

    #[inline]
    fn run(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.b1 * self.x1 - self.a1 * self.y1;
        self.x1 = x;
        self.y1 = y;
        y
    }
}

/// One speaker heard at one ear.
#[derive(Debug, Clone)]
struct Path {
    delay: usize,
    gain: f32,
    shadow: Shelf,
    /// Raised speakers: a touch more top, as the outer ear gives.
    air: Option<Air>,
}

/// A gentle high lift: the sound plus some of what a low pass removes.
#[derive(Debug, Clone, Copy)]
struct Air {
    /// Low-pass pole.
    c: f32,
    amount: f32,
    lp: f32,
}

impl Air {
    #[inline]
    fn run(&mut self, x: f32) -> f32 {
        self.lp = (1.0 - self.c) * x + self.c * self.lp;
        x + self.amount * (x - self.lp)
    }
}

/// Renders a layout's speaker channels to two ears.
pub struct Binaural {
    channels: usize,
    /// Per speaker: left ear, right ear.
    paths: Vec<[Path; 2]>,
    /// Per speaker: the last samples, for the delays.
    hist: Vec<Vec<f32>>,
    at: usize,
    len: usize,
}

impl Binaural {
    pub fn new(layout: SpaceLayout, rate: u32) -> Binaural {
        let sp = speakers(layout);
        let mut longest = 1;
        let paths: Vec<[Path; 2]> = sp
            .iter()
            .map(|s| {
                let Some([x, y, z]) = s.at else {
                    // Low-frequency channel: both ears, no cues.
                    let p = Path {
                        delay: 0,
                        gain: 0.5,
                        shadow: Shelf {
                            b0: 1.0,
                            ..Shelf::default()
                        },
                        air: None,
                    };
                    return [p.clone(), p];
                };
                // Speakers stand on the room's walls: the room's height is
                // about half its width, so raised ones sit at about 35 degrees.
                let v = [x, y, z * 0.7];
                let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-6);
                let v = [v[0] / len, v[1] / len, v[2] / len];
                let mut ear = |side: f32| {
                    // The angle between the sound and the ear's axis.
                    let cos = (v[0] * side).clamp(-1.0, 1.0);
                    let theta = cos.acos();
                    let extra = if theta < std::f32::consts::FRAC_PI_2 {
                        -HEAD / SOUND * theta.cos()
                    } else {
                        HEAD / SOUND * (theta - std::f32::consts::FRAC_PI_2)
                    };
                    let delay = ((HEAD / SOUND + extra) * rate as f32).round().max(0.0) as usize;
                    longest = longest.max(delay + 1);
                    Path {
                        delay,
                        gain: 0.7,
                        shadow: Shelf::shadow(theta.to_degrees(), rate),
                        air: (z > 0.5).then(|| Air {
                            c: (-2.0 * std::f32::consts::PI * 6_000.0 / rate as f32).exp(),
                            amount: 0.35,
                            lp: 0.0,
                        }),
                    }
                };
                [ear(-1.0), ear(1.0)]
            })
            .collect();
        let len = longest.next_power_of_two().max(64);
        Binaural {
            channels: sp.len(),
            hist: vec![vec![0.0; len]; sp.len()],
            paths,
            at: 0,
            len,
        }
    }

    pub fn channels(&self) -> usize {
        self.channels
    }

    pub fn reset(&mut self) {
        for h in &mut self.hist {
            h.fill(0.0);
        }
        for p in &mut self.paths {
            for e in p.iter_mut() {
                e.shadow.x1 = 0.0;
                e.shadow.y1 = 0.0;
                if let Some(a) = e.air.as_mut() {
                    a.lp = 0.0;
                }
            }
        }
    }

    /// `bed` holds `channels` interleaved; writes stereo into `out` (its
    /// length is twice the frames).
    pub fn process(&mut self, bed: &[f32], out: &mut [f32]) {
        let n = self.channels;
        let mask = self.len - 1;
        for (f, o) in out.chunks_exact_mut(2).enumerate() {
            let frame = &bed[f * n..(f + 1) * n];
            let mut lr = [0f32; 2];
            for (c, &x) in frame.iter().enumerate() {
                let h = &mut self.hist[c];
                h[self.at] = x;
                for (e, path) in self.paths[c].iter_mut().enumerate() {
                    let d = h[(self.at + self.len - path.delay) & mask];
                    let mut y = path.shadow.run(d) * path.gain;
                    if let Some(a) = path.air.as_mut() {
                        y = a.run(y);
                    }
                    lr[e] += y;
                }
            }
            o[0] = lr[0];
            o[1] = lr[1];
            self.at = (self.at + 1) & mask;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn impulse_at(layout: SpaceLayout, speaker: usize) -> (Vec<f32>, Vec<f32>) {
        let mut b = Binaural::new(layout, 48_000);
        let n = b.channels();
        let frames = 256;
        let mut bed = vec![0f32; frames * n];
        bed[speaker] = 1.0;
        let mut out = vec![0f32; frames * 2];
        b.process(&bed, &mut out);
        let l: Vec<f32> = out.iter().step_by(2).copied().collect();
        let r: Vec<f32> = out.iter().skip(1).step_by(2).copied().collect();
        (l, r)
    }

    fn first(x: &[f32]) -> usize {
        x.iter().position(|v| v.abs() > 1e-4).unwrap()
    }

    #[test]
    fn a_left_speaker_reaches_the_left_ear_first_and_louder() {
        // 5.1's left surround (channel 4) sits back left.
        let (l, r) = impulse_at(SpaceLayout::Surround51, 4);
        assert!(first(&l) < first(&r), "{} {}", first(&l), first(&r));
        let e = |x: &[f32]| x.iter().map(|v| v * v).sum::<f32>();
        assert!(e(&l) > e(&r) * 1.5);
    }

    #[test]
    fn the_center_speaker_reaches_both_ears_alike() {
        let (l, r) = impulse_at(SpaceLayout::Surround51, 2);
        assert_eq!(first(&l), first(&r));
        for (a, b) in l.iter().zip(&r) {
            assert!((a - b).abs() < 1e-6);
        }
    }
}
