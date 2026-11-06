// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Plosive Tamer: turns down the low-frequency thump of P and B sounds,
//! only while it happens. A pop is a burst of air that makes the low end
//! jump far above the rest of the voice, which an ordinary high-pass
//! filter handles by thinning every word. This watches the low band
//! against the voice's body (everything from 500 Hz to 3 kHz) and, when the
//! low band rises more than the Trigger amount above it, lays a low
//! shelf over the signal that deepens with the excess. Loud vowels lift
//! both bands together, so they pass untouched. Original code from the
//! textbook envelope follower and the shelf in [`crate::dsp::biquad`].

use super::{num, EffectInfo, Params, Processor};
use crate::dsp::biquad::{Coeffs, Shape, StereoBiquad};

pub fn info() -> EffectInfo {
    EffectInfo {
        id: "altaudio.plosive",
        name: "Plosive Tamer",
        category: "Dynamics",
        description: "Turns down the thump of P and B sounds only while it happens, leaving the rest of the voice alone. Raise Trigger if it touches normal words; raise Most reduction if pops still get through.",
        params: vec![
            num("freq", "Corner", "Hz", 60.0, 250.0, 120.0, 1.0).log(),
            num("trigger", "Trigger", "dB", 0.0, 24.0, 6.0, 0.5),
            num("reduction", "Most reduction", "dB", 0.0, 30.0, 18.0, 0.5),
            num("release", "Release", "ms", 20.0, 400.0, 100.0, 1.0).log(),
        ],
    }
}

/// One-pole coefficient for a time in milliseconds.
fn coef(ms: f64, rate: u32) -> f32 {
    let n = ms.max(0.01) * f64::from(rate) / 1000.0;
    (-1.0 / n).exp() as f32
}

fn to_db(x: f32) -> f32 {
    20.0 * x.max(1e-9).log10()
}

/// The shelf is re-aimed every this many frames, not every sample.
const STEP: u32 = 32;

pub struct Plosive {
    low: StereoBiquad,
    body_hp: StereoBiquad,
    body_lp: StereoBiquad,
    shelf: StereoBiquad,
    rate: u32,
    freq: f64,
    trigger: f32,
    most: f32,
    attack: f32,
    release: f32,
    env_low: f32,
    env_body: f32,
    count: u32,
    cut: f32,
}

impl Plosive {
    pub fn new(p: &Params, rate: u32) -> Self {
        let freq = p.get("freq");
        Self {
            low: StereoBiquad::new(Coeffs::new(Shape::LowPass, rate, freq, 0.707, 0.0)),
            body_hp: StereoBiquad::new(Coeffs::new(Shape::HighPass, rate, 500.0, 0.707, 0.0)),
            body_lp: StereoBiquad::new(Coeffs::new(Shape::LowPass, rate, 3000.0, 0.707, 0.0)),
            shelf: StereoBiquad::new(Coeffs::FLAT),
            rate,
            freq,
            trigger: p.get("trigger") as f32,
            most: p.get("reduction") as f32,
            attack: coef(1.0, rate),
            release: coef(p.get("release"), rate),
            env_low: 0.0,
            env_body: 0.0,
            count: 0,
            cut: 0.0,
        }
    }

    fn follow(env: &mut f32, x: f32, attack: f32, release: f32) {
        let c = if x > *env { attack } else { release };
        *env = x + (*env - x) * c;
    }
}

impl Processor for Plosive {
    fn process(&mut self, buf: &mut [f32]) {
        for f in buf.as_chunks_mut::<2>().0 {
            let lo = self.low.l.run(f[0]).abs().max(self.low.r.run(f[1]).abs());
            let ml = self.body_lp.l.run(self.body_hp.l.run(f[0]));
            let mr = self.body_lp.r.run(self.body_hp.r.run(f[1]));
            let mid = ml.abs().max(mr.abs());
            Self::follow(&mut self.env_low, lo, self.attack, self.release);
            Self::follow(&mut self.env_body, mid, self.attack, self.release);
            if self.count == 0 {
                let excess = to_db(self.env_low) - to_db(self.env_body) - self.trigger;
                let cut = excess.clamp(0.0, self.most);
                if (cut - self.cut).abs() > 0.1 {
                    self.cut = cut;
                    let co = if cut < 0.1 {
                        Coeffs::FLAT
                    } else {
                        Coeffs::new(Shape::LowShelf, self.rate, self.freq, 1.0, -f64::from(cut))
                    };
                    self.shelf.l.set(co);
                    self.shelf.r.set(co);
                }
            }
            self.count = (self.count + 1) % STEP;
            f[0] = self.shelf.l.run(f[0]);
            f[1] = self.shelf.r.run(f[1]);
        }
    }

    fn reset(&mut self) {
        self.low.reset();
        self.body_hp.reset();
        self.body_lp.reset();
        self.shelf.reset();
        self.shelf.l.set(Coeffs::FLAT);
        self.shelf.r.set(Coeffs::FLAT);
        self.env_low = 0.0;
        self.env_body = 0.0;
        self.count = 0;
        self.cut = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::super::build;
    use super::super::tests::slot;
    use std::f32::consts::PI;

    /// A mix of sines, each `(frequency, amplitude)`, `secs` long.
    fn tones(parts: &[(f32, f32)], secs: f32) -> Vec<f32> {
        (0..(48_000.0 * secs) as usize)
            .flat_map(|i| {
                let v: f32 = parts
                    .iter()
                    .map(|(f, a)| (2.0 * PI * f * i as f32 / 48_000.0).sin() * a)
                    .sum();
                [v, v]
            })
            .collect()
    }

    fn run(params: &[(&str, f64)], mut b: Vec<f32>) -> Vec<f32> {
        let mut p = build(&slot("altaudio.plosive", params), 48_000).unwrap();
        for chunk in b.chunks_mut(2 * 777) {
            p.process(chunk);
        }
        b
    }

    fn rms(b: &[f32]) -> f32 {
        (b.iter().map(|v| v * v).sum::<f32>() / b.len() as f32).sqrt()
    }

    /// Level of one frequency in a stretch, by correlating with sine and cosine.
    fn level_at(b: &[f32], freq: f32) -> f32 {
        let (mut s, mut c) = (0.0f32, 0.0f32);
        for (i, f) in b.as_chunks::<2>().0.iter().enumerate() {
            let w = 2.0 * PI * freq * i as f32 / 48_000.0;
            s += f[0] * w.sin();
            c += f[0] * w.cos();
        }
        2.0 * (s * s + c * c).sqrt() / (b.len() / 2) as f32
    }

    fn db(x: f32) -> f32 {
        20.0 * x.max(1e-9).log10()
    }

    #[test]
    fn speech_range_sound_passes_unchanged() {
        let input = tones(&[(1000.0, 0.2)], 1.0);
        let out = run(&[], input.clone());
        let d = db(rms(&out[24_000..])) - db(rms(&input[24_000..]));
        assert!(d.abs() < 0.2, "{d}");
    }

    #[test]
    fn a_low_thump_over_the_voice_is_turned_down() {
        // A voice body at 1 kHz with an 80 Hz burst far above it.
        let input = tones(&[(1000.0, 0.1), (80.0, 0.6)], 1.0);
        let out = run(&[], input.clone());
        let tail = 48_000;
        let cut = db(level_at(&out[tail..], 80.0)) - db(level_at(&input[tail..], 80.0));
        assert!(cut < -6.0, "thump cut {cut}");
        // The voice itself stays where it was.
        let voice = db(level_at(&out[tail..], 1000.0)) - db(level_at(&input[tail..], 1000.0));
        assert!(voice.abs() < 0.5, "voice moved {voice}");
    }

    #[test]
    fn low_notes_that_rise_with_the_voice_are_left_alone() {
        // Low and body at the same level: a deep voice, not a pop.
        let input = tones(&[(1000.0, 0.2), (130.0, 0.2)], 1.0);
        let out = run(&[], input.clone());
        let d = db(level_at(&out[24_000 * 2..], 130.0)) - db(level_at(&input[24_000 * 2..], 130.0));
        assert!(d.abs() < 1.0, "{d}");
    }

    #[test]
    fn the_cut_never_passes_the_most_reduction() {
        let input = tones(&[(1000.0, 0.05), (80.0, 0.8)], 1.0);
        let out = run(&[("reduction", 6.0)], input.clone());
        let cut = db(level_at(&out[48_000..], 80.0)) - db(level_at(&input[48_000..], 80.0));
        assert!(cut > -8.0 && cut < -3.0, "{cut}");
    }

    #[test]
    fn it_lets_go_after_the_pop() {
        let mut input = tones(&[(1000.0, 0.1), (80.0, 0.6)], 0.4);
        input.extend(tones(&[(1000.0, 0.1)], 1.0));
        let out = run(&[("release", 50.0)], input.clone());
        let n = out.len();
        let d = db(rms(&out[n - 24_000..])) - db(rms(&input[n - 24_000..]));
        assert!(d.abs() < 0.3, "{d}");
    }
}
