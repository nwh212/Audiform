// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Stereo Expander and Distortion. All original code from textbook
//! techniques: mid/side width control, and waveshaping with the tone
//! filtered after it.

use std::f64::consts::TAU;

use super::{choice, num, EffectInfo, Params, Processor};

const SHAPES: &[&str] = &["Soft clip", "Hard clip", "Tube", "Fold"];

pub fn expander_info() -> EffectInfo {
    EffectInfo {
        id: "altaudio.stereo-expander",
        name: "Stereo Expander",
        category: "Stereo",
        description: "Widens or narrows the stereo picture. Below 100% it pulls the sound toward the middle, above it spreads it out. Keeping the low end in the middle avoids a thin or wobbly bass.",
        params: vec![
            num("width", "Width", "%", 0.0, 300.0, 150.0, 1.0),
            num("center", "Center level", "dB", -12.0, 12.0, 0.0, 0.1),
            num("bass", "Keep bass centered below", "Hz", 0.0, 500.0, 120.0, 1.0),
        ],
    }
}

pub fn distortion_info() -> EffectInfo {
    EffectInfo {
        id: "altaudio.distortion",
        name: "Distortion",
        category: "Amplitude",
        description: "Pushes the sound into a curve that rounds, flattens or folds the peaks, adding harmonics. Soft clip is warm, hard clip is harsh, Tube is lopsided and Fold is wild. Tone darkens the result.",
        params: vec![
            choice("shape", "Type", SHAPES, 0),
            num("drive", "Drive", "dB", 0.0, 40.0, 12.0, 0.1),
            num("tone", "Tone", "Hz", 500.0, 20000.0, 12000.0, 1.0).log(),
            num("output", "Output", "dB", -24.0, 12.0, -6.0, 0.1),
            num("mix", "Mix", "%", 0.0, 100.0, 100.0, 1.0),
        ],
    }
}

fn db(x: f64) -> f32 {
    10f64.powf(x / 20.0) as f32
}

// --- Stereo Expander ---

pub struct Expander {
    width: f32,
    center: f32,
    /// One-pole coefficient for the bass split; 0 turns the split off.
    coef: f32,
    low: f32,
}

impl Expander {
    pub fn new(p: &Params, rate: u32) -> Self {
        let hz = p.get("bass");
        let coef = if hz < 1.0 {
            0.0
        } else {
            (1.0 - (-TAU * hz / f64::from(rate)).exp()) as f32
        };
        Self {
            width: (p.get("width") / 100.0) as f32,
            center: db(p.get("center")),
            coef,
            low: 0.0,
        }
    }
}

impl Processor for Expander {
    fn process(&mut self, buf: &mut [f32]) {
        for f in buf.as_chunks_mut::<2>().0 {
            let mid = (f[0] + f[1]) * 0.5;
            let side = (f[0] - f[1]) * 0.5;
            // Only the side above the split is widened; the bass side
            // keeps its own level, so low notes stay where they were.
            let side = if self.coef > 0.0 {
                self.low += self.coef * (side - self.low);
                if self.low.abs() < 1e-20 {
                    self.low = 0.0;
                }
                self.low + (side - self.low) * self.width
            } else {
                side * self.width
            };
            let mid = mid * self.center;
            f[0] = mid + side;
            f[1] = mid - side;
        }
    }

    fn reset(&mut self) {
        self.low = 0.0;
    }
}

// --- Distortion ---

pub struct Distortion {
    shape: usize,
    drive: f32,
    out: f32,
    mix: f32,
    coef: f32,
    prev: [f32; 2],
    tone: [f32; 2],
}

/// The curve: input in, output in about -1..1.
fn shaped(shape: usize, x: f32) -> f32 {
    match shape {
        0 => x.tanh(),
        1 => x.clamp(-1.0, 1.0),
        // Lopsided: the positive half bends harder than the negative.
        2 => {
            if x >= 0.0 {
                x.tanh()
            } else {
                (x / 0.9).tanh() * 0.9
            }
        }
        // Folds back on itself past full scale.
        _ => {
            let t = (x + 1.0).rem_euclid(4.0);
            if t < 2.0 {
                t - 1.0
            } else {
                3.0 - t
            }
        }
    }
}

impl Distortion {
    pub fn new(p: &Params, rate: u32) -> Self {
        let hz = p.get("tone").min(f64::from(rate) * 0.45);
        Self {
            shape: p.index("shape").min(SHAPES.len() - 1),
            drive: db(p.get("drive")),
            out: db(p.get("output")),
            mix: (p.get("mix") / 100.0) as f32,
            coef: (1.0 - (-TAU * hz / f64::from(rate)).exp()) as f32,
            prev: [0.0; 2],
            tone: [0.0; 2],
        }
    }
}

impl Processor for Distortion {
    fn process(&mut self, buf: &mut [f32]) {
        for f in buf.as_chunks_mut::<2>().0 {
            for (c, s) in f.iter_mut().enumerate() {
                let dry = *s;
                let x = dry * self.drive;
                // Run at twice the rate (a midpoint between frames) and
                // average, which keeps the extra harmonics from folding
                // back as harshly.
                let mid = (self.prev[c] + x) * 0.5;
                let y = (shaped(self.shape, mid) + shaped(self.shape, x)) * 0.5;
                self.prev[c] = x;
                self.tone[c] += self.coef * (y - self.tone[c]);
                if self.tone[c].abs() < 1e-20 {
                    self.tone[c] = 0.0;
                }
                let wet = self.tone[c] * self.out;
                *s = if self.mix >= 1.0 {
                    wet
                } else {
                    dry * (1.0 - self.mix) + wet * self.mix
                };
            }
        }
    }

    fn reset(&mut self) {
        self.prev = [0.0; 2];
        self.tone = [0.0; 2];
    }
}

#[cfg(test)]
mod tests {
    use super::super::build;
    use super::super::tests::slot;
    use std::f32::consts::PI;

    fn sine(freq: f32, amp: f32, secs: f32) -> Vec<f32> {
        let n = (48_000.0 * secs) as usize;
        (0..n)
            .flat_map(|i| {
                let v = (2.0 * PI * freq * i as f32 / 48_000.0).sin() * amp;
                [v, v]
            })
            .collect()
    }

    fn run(effect: &str, params: &[(&str, f64)], mut b: Vec<f32>) -> Vec<f32> {
        let mut p = build(&slot(effect, params), 48_000).unwrap();
        for chunk in b.chunks_mut(2 * 777) {
            p.process(chunk);
        }
        b
    }

    fn peak(b: &[f32]) -> f32 {
        b.iter().fold(0f32, |m, s| m.max(s.abs()))
    }

    /// Level of the left channel at `freq`, by a single-bin transform.
    fn bin(b: &[f32], freq: f32) -> f32 {
        let (mut re, mut im) = (0f64, 0f64);
        let n = b.len() / 2;
        for i in 0..n {
            let a = 2.0 * std::f64::consts::PI * f64::from(freq) * i as f64 / 48_000.0;
            re += f64::from(b[2 * i]) * a.cos();
            im += f64::from(b[2 * i]) * a.sin();
        }
        (2.0 * (re * re + im * im).sqrt() / n as f64) as f32
    }

    #[test]
    fn expander_at_100_percent_leaves_the_sound_alone() {
        let input: Vec<f32> = (0..4000)
            .flat_map(|i| {
                [
                    ((i % 13) as f32 - 6.0) * 0.05,
                    ((i % 5) as f32 - 2.0) * 0.07,
                ]
            })
            .collect();
        let out = run(
            "altaudio.stereo-expander",
            &[("width", 100.0), ("center", 0.0)],
            input.clone(),
        );
        for (a, b) in input.iter().zip(&out) {
            assert!((a - b).abs() < 1e-6);
        }
    }

    #[test]
    fn expander_narrows_to_mono_and_widens_the_sides() {
        let wide: Vec<f32> = [0.6f32, 0.2].into_iter().cycle().take(2000).collect();
        let mono = run(
            "altaudio.stereo-expander",
            &[("width", 0.0), ("bass", 0.0)],
            wide.clone(),
        );
        for f in mono.chunks(2) {
            assert!((f[0] - f[1]).abs() < 1e-6, "mono");
            assert!((f[0] - 0.4).abs() < 1e-6, "keeps the middle");
        }
        let more = run(
            "altaudio.stereo-expander",
            &[("width", 200.0), ("bass", 0.0)],
            wide,
        );
        // Side is 0.2, doubled to 0.4 around the 0.4 middle.
        assert!((more[0] - 0.8).abs() < 1e-6 && more[1].abs() < 1e-6);
    }

    #[test]
    fn expander_keeps_the_bass_side_in_place() {
        // A 40 Hz tone that is only on the left stays on the left at
        // high width when the bass is held, and spreads when it is not.
        let tone: Vec<f32> = sine(40.0, 0.5, 1.0)
            .chunks(2)
            .flat_map(|f| [f[0], 0.0])
            .collect();
        let held = run(
            "altaudio.stereo-expander",
            &[("width", 300.0), ("bass", 300.0)],
            tone.clone(),
        );
        let free = run(
            "altaudio.stereo-expander",
            &[("width", 300.0), ("bass", 0.0)],
            tone,
        );
        let tail = |b: &[f32]| b[b.len() / 2..].to_vec();
        assert!(bin(&tail(&held), 40.0) < 0.55, "bass level kept");
        assert!(peak(&tail(&free)) > peak(&tail(&held)) * 1.5);
    }

    #[test]
    fn distortion_with_no_mix_is_the_dry_sound_and_clips_stay_in_bounds() {
        let input = sine(220.0, 0.8, 0.3);
        assert_eq!(
            run("altaudio.distortion", &[("mix", 0.0)], input.clone()),
            input
        );
        for shape in 0..4 {
            let out = run(
                "altaudio.distortion",
                &[
                    ("shape", f64::from(shape)),
                    ("drive", 40.0),
                    ("output", 0.0),
                ],
                input.clone(),
            );
            assert!(peak(&out) <= 1.05, "shape {shape}: {}", peak(&out));
            assert!(out.iter().all(|s| s.is_finite()));
        }
    }

    #[test]
    fn distortion_adds_odd_harmonics_to_a_clean_tone() {
        let input = sine(500.0, 0.8, 0.5);
        let clean = run(
            "altaudio.distortion",
            &[
                ("drive", 0.0),
                ("output", 0.0),
                ("tone", 20000.0),
                ("mix", 100.0),
            ],
            input.clone(),
        );
        let hot = run(
            "altaudio.distortion",
            &[("drive", 24.0), ("output", -12.0), ("tone", 20000.0)],
            input,
        );
        let third = |b: &[f32]| bin(&b[b.len() / 2..], 1500.0) / bin(&b[b.len() / 2..], 500.0);
        assert!(third(&hot) > third(&clean) * 5.0 && third(&hot) > 0.05);
    }
}
