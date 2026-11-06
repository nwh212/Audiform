// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Expander / Gate: turns down what falls below a threshold. As an
//! expander the cut grows gently with ratio the further the sound sits
//! below the threshold; as a gate it drops to the closed level once the
//! sound has been under the threshold for the hold time. Original code
//! from the textbook downward-expander curve.

use super::{choice, num, EffectInfo, Params, Processor};

const MODES: &[&str] = &["Expander", "Gate"];

pub fn info() -> EffectInfo {
    EffectInfo {
        id: "altaudio.expander",
        name: "Expander / Gate",
        category: "Dynamics",
        description: "Turns down quiet sound. Expander mode fades noise away gently the further it sits below the threshold; Gate mode shuts it off once the sound has stayed below the threshold. Can listen to another track as a sidechain.",
        params: vec![
            choice("mode", "Mode", MODES, 0),
            num("threshold", "Threshold", "dB", -80.0, 0.0, -45.0, 0.1),
            num("ratio", "Ratio", ":1", 1.0, 20.0, 2.5, 0.1).log(),
            num("range", "Max reduction", "dB", 0.0, 80.0, 40.0, 0.5),
            num("attack", "Attack", "ms", 0.1, 100.0, 3.0, 0.1).log(),
            num("hold", "Hold", "ms", 0.0, 1000.0, 60.0, 1.0),
            num("release", "Release", "ms", 5.0, 2000.0, 150.0, 1.0).log(),
        ],
    }
}

fn smooth(ms: f64, rate: u32) -> f32 {
    (1.0 - (-1.0 / (ms / 1000.0 * f64::from(rate)).max(1.0)).exp()) as f32
}

pub struct Expander {
    gate: bool,
    threshold_db: f32,
    slope: f32,
    range_db: f32,
    open: f32,
    close: f32,
    hold: u32,
    env_decay: f32,
    env: f32,
    held: u32,
    /// Current cut in dB, 0 or negative.
    cut: f32,
}

impl Expander {
    pub fn new(p: &Params, rate: u32) -> Self {
        let per_ms = f64::from(rate) / 1000.0;
        Self {
            gate: p.index("mode") == 1,
            threshold_db: p.get("threshold") as f32,
            slope: (p.get("ratio") - 1.0) as f32,
            range_db: p.get("range") as f32,
            open: smooth(p.get("attack"), rate),
            close: smooth(p.get("release"), rate),
            hold: (p.get("hold") * per_ms) as u32,
            env_decay: (-1.0 / (0.005 * f64::from(rate))).exp() as f32,
            env: 0.0,
            held: 0,
            cut: 0.0,
        }
    }

    fn run(&mut self, buf: &mut [f32], key: Option<&[f32]>) {
        for (i, f) in buf.as_chunks_mut::<2>().0.iter_mut().enumerate() {
            let x = match key {
                Some(k) => k[i * 2].abs().max(k[i * 2 + 1].abs()),
                None => f[0].abs().max(f[1].abs()),
            };
            self.env = x.max(self.env * self.env_decay);
            if self.env < 1e-20 {
                self.env = 0.0;
            }
            let level_db = if self.env > 1e-9 {
                20.0 * self.env.log10()
            } else {
                -180.0
            };
            let under = self.threshold_db - level_db;
            let target = if under <= 0.0 {
                self.held = self.hold;
                0.0
            } else if self.held > 0 {
                self.held -= 1;
                0.0
            } else if self.gate {
                -self.range_db
            } else {
                (-under * self.slope).max(-self.range_db)
            };
            // Opening (less cut) uses the attack, closing the release.
            let coef = if target > self.cut {
                self.open
            } else {
                self.close
            };
            self.cut += (target - self.cut) * coef;
            if self.cut.abs() < 1e-6 {
                self.cut = 0.0;
            }
            let g = 10f32.powf(self.cut / 20.0);
            f[0] *= g;
            f[1] *= g;
        }
    }
}

impl Processor for Expander {
    fn process(&mut self, buf: &mut [f32]) {
        self.run(buf, None);
    }

    fn process_keyed(&mut self, buf: &mut [f32], key: &[f32]) {
        self.run(buf, Some(key));
    }

    fn reset(&mut self) {
        self.env = 0.0;
        self.held = 0;
        self.cut = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::super::build;
    use super::super::tests::slot;
    use std::f32::consts::PI;

    fn sine(amp: f32, secs: f32) -> Vec<f32> {
        (0..(48_000.0 * secs) as usize)
            .flat_map(|i| {
                let v = (2.0 * PI * 300.0 * i as f32 / 48_000.0).sin() * amp;
                [v, v]
            })
            .collect()
    }

    fn run(params: &[(&str, f64)], mut b: Vec<f32>) -> Vec<f32> {
        let mut p = build(&slot("altaudio.expander", params), 48_000).unwrap();
        for chunk in b.chunks_mut(2 * 777) {
            p.process(chunk);
        }
        b
    }

    fn peak(b: &[f32]) -> f32 {
        b.iter().fold(0f32, |m, s| m.max(s.abs()))
    }

    fn db(x: f32) -> f32 {
        20.0 * x.log10()
    }

    #[test]
    fn sound_above_the_threshold_passes_unchanged() {
        let input = sine(0.5, 0.5);
        let out = run(&[("threshold", -40.0)], input.clone());
        // Past the first few milliseconds, while the detector catches up.
        let from = 2 * 48 * 20;
        for (a, b) in input[from..].iter().zip(&out[from..]) {
            assert!((a - b).abs() < 1e-4);
        }
    }

    #[test]
    fn the_expander_cuts_by_the_ratio_below_the_threshold() {
        // 10 dB under the threshold at 2:1 gives a 10 dB cut.
        let amp = 0.01f32; // about -40 dB
        let out = run(
            &[("threshold", -30.0), ("ratio", 2.0), ("range", 60.0)],
            sine(amp, 1.0),
        );
        let tail = &out[out.len() / 2..];
        let cut = db(peak(tail)) - db(amp);
        assert!((cut + 10.0).abs() < 1.5, "cut {cut}");
    }

    #[test]
    fn the_cut_never_passes_the_max_reduction() {
        let amp = 0.001f32;
        let out = run(
            &[("threshold", -20.0), ("ratio", 10.0), ("range", 12.0)],
            sine(amp, 1.0),
        );
        let cut = db(peak(&out[out.len() / 2..])) - db(amp);
        assert!((cut + 12.0).abs() < 1.0, "cut {cut}");
    }

    #[test]
    fn the_gate_waits_out_its_hold_then_closes() {
        let mut input = sine(0.3, 0.3);
        input.extend(sine(0.0005, 0.7));
        let out = run(
            &[
                ("mode", 1.0),
                ("threshold", -40.0),
                ("range", 60.0),
                ("hold", 100.0),
                ("release", 20.0),
            ],
            input,
        );
        let at = |ms: usize| peak(&out[2 * 48 * ms..2 * 48 * (ms + 10)]);
        // Just after the loud part ends the quiet tone still passes (hold) ...
        assert!(at(310) > 0.0004, "{}", at(310));
        // ... and well after, it is shut.
        assert!(at(900) < 0.0005 * 0.1, "{}", at(900));
    }

    #[test]
    fn silence_stays_silent_and_finite() {
        let out = run(&[("mode", 1.0)], vec![0.0; 20_000]);
        assert!(out.iter().all(|s| *s == 0.0));
    }
}
