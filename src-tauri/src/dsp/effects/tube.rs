// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Two effects in the manner of classic tube studio gear, with our own
//! names and maths:
//!
//! * Opto Leveler: a leveling amplifier whose gain cell behaves like a
//!   light-dependent resistor. It reacts in about 10 ms and lets go in
//!   two stages, quickly at first and then slowly, more slowly the longer
//!   it has been working (program-dependent release). Peak Reduction sets
//!   how hard it works, Gain makes up the level, Compress or Limit sets
//!   the ratio and knee, Emphasis makes it listen harder to high
//!   frequencies (taming sibilance), and Tube adds the warm, mostly even
//!   harmonic color of a tube output stage.
//! * Tube Preamp: input gain into a tube stage (asymmetric, so mostly
//!   even harmonics, run at twice the sample rate to keep aliasing down),
//!   an output transformer (a little low-end weight and a soft top), and
//!   broad Low and High tone controls, for a warm vintage vocal sound.
//!
//! Both report gain reduction and output level to their windows' meters.

use std::sync::Arc;

use super::{choice, db_to_gain, num, toggle, EffectInfo, Params, Processor};
use crate::dsp::biquad::{butterworth_qs, Biquad, Coeffs, Shape};
use crate::dsp::meters::SlotMeter;

pub const OPTO: &str = "altaudio.opto_leveler";
pub const PREAMP: &str = "altaudio.tube_preamp";

pub fn opto_info() -> EffectInfo {
    EffectInfo {
        id: OPTO,
        name: "Opto Leveler",
        category: "Dynamics",
        description: "A smooth leveling amplifier with an optical-style gain cell: quick to catch, slow and gentle to let go. Turn up Peak Reduction until the meter shows a few dB of leveling, then set Gain for the level you want.",
        params: vec![
            num("peak", "Peak Reduction", "", 0.0, 100.0, 40.0, 0.5),
            num("gain", "Gain", "", 0.0, 100.0, 40.0, 0.5),
            choice("mode", "Mode", &["Compress", "Limit"], 0),
            num("emphasis", "Emphasis", "%", 0.0, 100.0, 0.0, 1.0),
            num("tube", "Tube", "%", 0.0, 100.0, 30.0, 1.0),
            num("mix", "Mix", "%", 0.0, 100.0, 100.0, 1.0),
            choice("meter", "Meter", &["Gain reduction", "Output"], 0),
        ],
    }
}

pub fn preamp_info() -> EffectInfo {
    EffectInfo {
        id: PREAMP,
        name: "Tube Preamp",
        category: "Amplitude",
        description: "A warm vintage tube preamp: drive it for rich, mostly even harmonics, with a transformer's weight and soft top and broad Low and High tone controls.",
        params: vec![
            num("input", "Input", "dB", -12.0, 24.0, 0.0, 0.1),
            num("drive", "Drive", "%", 0.0, 100.0, 35.0, 1.0),
            num("low", "Low", "dB", -12.0, 12.0, 0.0, 0.1),
            num("high", "High", "dB", -12.0, 12.0, 0.0, 0.1),
            toggle("transformer", "Transformer", true),
            num("output", "Output", "dB", -24.0, 12.0, 0.0, 0.1),
            num("mix", "Mix", "%", 0.0, 100.0, 100.0, 1.0),
        ],
    }
}

/// A tube stage: soft, asymmetric saturation with unity gain for small
/// signals. `k` is how hard it is driven (1 is clean), `bias` how lopsided
/// (more even harmonics).
#[inline]
fn tube(x: f32, k: f32, bias: f32) -> f32 {
    let a = k * (x + bias * x * x);
    let y = if a >= 0.0 {
        a.tanh()
    } else {
        (a * 1.25).tanh() / 1.25
    };
    y / k
}

/// Removes the offset the lopsided tube stage adds.
#[derive(Clone, Copy)]
struct DcBlock {
    x1: f32,
    y1: f32,
    r: f32,
}

impl DcBlock {
    fn new(rate: u32) -> Self {
        Self {
            x1: 0.0,
            y1: 0.0,
            r: (-std::f32::consts::TAU * 8.0 / rate as f32).exp(),
        }
    }
    #[inline]
    fn run(&mut self, x: f32) -> f32 {
        let y = x - self.x1 + self.r * self.y1;
        self.x1 = x;
        self.y1 = y;
        y
    }
    fn reset(&mut self) {
        self.x1 = 0.0;
        self.y1 = 0.0;
    }
}

/// Output level for the meter: peak with a VU-like fall, in dBFS.
#[derive(Clone, Copy)]
struct Follower {
    v: f32,
    fall: f32,
}

impl Follower {
    fn new(rate: u32) -> Self {
        Self {
            v: 0.0,
            fall: (-1.0 / (0.3 * rate as f32)).exp(),
        }
    }
    #[inline]
    fn run(&mut self, x: f32) {
        let a = x.abs();
        self.v = if a > self.v { a } else { self.v * self.fall };
    }
    fn db(&self) -> f32 {
        20.0 * self.v.max(1e-6).log10()
    }
}

pub struct Opto {
    rate: f32,
    thr_db: f32,
    ratio_term: f32,
    knee: f32,
    makeup: f32,
    drive: f32,
    mix: f32,
    emph: [Biquad; 1],
    det: f32,
    det_coef: f32,
    gr: f32,
    /// Gain reduction when release began: the first half of it goes quickly.
    held: f32,
    att: f32,
    memory: f32,
    dc: [DcBlock; 2],
    follow: Follower,
    meter: Arc<SlotMeter>,
    tick: u32,
}

impl Opto {
    pub fn new(p: &Params, rate: u32, meter: Arc<SlotMeter>) -> Self {
        let r = rate as f32;
        let mut o = Self {
            rate: r,
            thr_db: 0.0,
            ratio_term: 0.0,
            knee: 0.0,
            makeup: 1.0,
            drive: 1.0,
            mix: 1.0,
            emph: [Biquad::new(Coeffs::FLAT)],
            det: 0.0,
            det_coef: (-1.0 / (0.005 * r)).exp(),
            gr: 0.0,
            held: 0.0,
            att: (-1.0 / (0.010 * r)).exp(),
            memory: 0.0,
            dc: [DcBlock::new(rate); 2],
            follow: Follower::new(rate),
            meter,
            tick: 0,
        };
        o.set(p, rate);
        o
    }

    fn set(&mut self, p: &Params, rate: u32) {
        // Peak Reduction lowers the point where leveling starts: 0 hardly
        // touches anything, 100 sits 60 dB down.
        self.thr_db = -(p.get("peak") as f32) * 0.6;
        let limit = p.get("mode").round() as i64 == 1;
        let ratio = if limit { 20.0 } else { 3.0 };
        self.ratio_term = 1.0 - 1.0 / ratio;
        self.knee = if limit { 3.0 } else { 12.0 };
        // Gain 40 is unity; each step is half a dB.
        self.makeup = db_to_gain((p.get("gain") - 40.0) * 0.5);
        self.drive = 1.0 + (p.get("tube") as f32 / 100.0).powf(1.5) * 3.0;
        self.mix = (p.get("mix") / 100.0) as f32;
        let e = p.get("emphasis");
        self.emph[0].set(Coeffs::new(
            Shape::HighShelf,
            rate,
            2500.0,
            0.7,
            e / 100.0 * 12.0,
        ));
    }

    /// Gain reduction (dB, 0 or more) for a detector level in dB.
    #[inline]
    fn target(&self, level_db: f32) -> f32 {
        let over = level_db - self.thr_db;
        let w = self.knee;
        if over <= -w / 2.0 {
            0.0
        } else if over >= w / 2.0 {
            over * self.ratio_term
        } else {
            let t = over + w / 2.0;
            t * t / (2.0 * w) * self.ratio_term
        }
    }
}

impl Processor for Opto {
    fn process(&mut self, buf: &mut [f32]) {
        let mem_up = 1.0 - (-1.0 / (2.0 * self.rate)).exp();
        let mem_down = 1.0 - (-1.0 / (6.0 * self.rate)).exp();
        for f in buf.as_chunks_mut::<2>().0 {
            let side = self.emph[0].run((f[0] + f[1]) * 0.5);
            self.det = side * side + (self.det - side * side) * self.det_coef;
            let level = 10.0 * self.det.max(1e-12).log10() + 3.0;
            let want = self.target(level);
            if want > self.gr {
                self.gr = want + (self.gr - want) * self.att;
                self.held = self.gr;
            } else {
                // Two-stage release: the first half lets go in about 60 ms,
                // the rest in 0.5 to 3 s, slower the longer it has worked.
                let fast = self.gr > 0.5 * self.held;
                let tau = if fast { 0.06 } else { 0.5 + 2.5 * self.memory };
                let rel = (-1.0 / (tau * self.rate)).exp();
                self.gr = want + (self.gr - want) * rel;
            }
            let busy = if self.gr > 1.0 { 1.0 } else { 0.0 };
            self.memory +=
                (busy - self.memory) * if busy > self.memory { mem_up } else { mem_down };
            let g = 10f32.powf(-self.gr / 20.0) * self.makeup;
            for (v, dc) in f.iter_mut().zip(&mut self.dc) {
                let dry = *v;
                let wet = dc.run(tube(dry * g, self.drive, 0.12));
                *v = dry + (wet - dry) * self.mix;
            }
            self.follow.run(f[0].abs().max(f[1].abs()));
        }
        // Meter readings about 60 times a second is plenty.
        self.tick += (buf.len() / 2) as u32;
        if self.tick as f32 > self.rate / 60.0 {
            self.tick = 0;
            self.meter.store(self.gr, self.follow.db());
        }
    }
    fn reset(&mut self) {
        self.det = 0.0;
        self.gr = 0.0;
        self.held = 0.0;
        self.memory = 0.0;
        self.emph[0].reset();
        for d in &mut self.dc {
            d.reset();
        }
    }
    fn update(&mut self, p: &Params) -> bool {
        self.set(p, self.rate as u32);
        true
    }
}

/// Twice-the-rate filter for the tube stage: a 4th-order low-pass.
#[derive(Clone, Copy)]
struct Half([Biquad; 2]);

impl Half {
    fn new(rate: u32) -> Self {
        let qs = butterworth_qs(2);
        let f = (f64::from(rate) * 0.45).min(20_000.0);
        Self([
            Biquad::new(Coeffs::new(Shape::LowPass, rate * 2, f, qs[0], 0.0)),
            Biquad::new(Coeffs::new(Shape::LowPass, rate * 2, f, qs[1], 0.0)),
        ])
    }
    #[inline]
    fn run(&mut self, x: f32) -> f32 {
        let y = self.0[0].run(x);
        self.0[1].run(y)
    }
    fn reset(&mut self) {
        for b in &mut self.0 {
            b.reset();
        }
    }
}

pub struct Preamp {
    rate: u32,
    input: f32,
    k: f32,
    output: f32,
    mix: f32,
    transformer: bool,
    up: [Half; 2],
    down: [Half; 2],
    dc: [DcBlock; 2],
    tone: [[Biquad; 4]; 2],
    follow: Follower,
    meter: Arc<SlotMeter>,
    tick: u32,
}

impl Preamp {
    pub fn new(p: &Params, rate: u32, meter: Arc<SlotMeter>) -> Self {
        let mut s = Self {
            rate,
            input: 1.0,
            k: 1.0,
            output: 1.0,
            mix: 1.0,
            transformer: true,
            up: [Half::new(rate); 2],
            down: [Half::new(rate); 2],
            dc: [DcBlock::new(rate); 2],
            tone: [[Biquad::new(Coeffs::FLAT); 4]; 2],
            follow: Follower::new(rate),
            meter,
            tick: 0,
        };
        s.set(p);
        s
    }

    fn set(&mut self, p: &Params) {
        let r = self.rate;
        self.input = db_to_gain(p.get("input"));
        self.k = 1.0 + (p.get("drive") as f32 / 100.0).powf(1.3) * 7.0;
        self.output = db_to_gain(p.get("output"));
        self.mix = (p.get("mix") / 100.0) as f32;
        self.transformer = p.get("transformer") >= 0.5;
        let nyq = f64::from(r) / 2.0;
        let cs = [
            Coeffs::new(Shape::LowShelf, r, 110.0, 0.6, p.get("low")),
            Coeffs::new(
                Shape::HighShelf,
                r,
                7000.0_f64.min(nyq * 0.8),
                0.6,
                p.get("high"),
            ),
            // The transformer: a little weight around 70 Hz and a soft top.
            if self.transformer {
                Coeffs::new(Shape::Peak, r, 70.0, 0.8, 1.5)
            } else {
                Coeffs::FLAT
            },
            if self.transformer {
                Coeffs::new(Shape::LowPass, r, 16_000.0_f64.min(nyq * 0.9), 0.6, 0.0)
            } else {
                Coeffs::FLAT
            },
        ];
        for ch in &mut self.tone {
            for (b, c) in ch.iter_mut().zip(cs) {
                b.set(c);
            }
        }
    }
}

impl Processor for Preamp {
    fn process(&mut self, buf: &mut [f32]) {
        let bias = if self.transformer { 0.35 } else { 0.25 };
        for f in buf.as_chunks_mut::<2>().0 {
            for (c, v) in f.iter_mut().enumerate() {
                let dry = *v;
                let x = dry * self.input;
                // Twice the rate: the sample and a zero, filtered, shaped, filtered, every other one kept.
                let a = self.up[c].run(x * 2.0);
                let b = self.up[c].run(0.0);
                let ya = self.down[c].run(tube(a, self.k, bias));
                let _ = self.down[c].run(tube(b, self.k, bias));
                let mut y = self.dc[c].run(ya);
                for t in &mut self.tone[c] {
                    y = t.run(y);
                }
                let wet = y * self.output;
                *v = dry + (wet - dry) * self.mix;
            }
            self.follow.run(f[0].abs().max(f[1].abs()));
        }
        self.tick += (buf.len() / 2) as u32;
        if self.tick as f32 > self.rate as f32 / 60.0 {
            self.tick = 0;
            self.meter.store(0.0, self.follow.db());
        }
    }
    fn reset(&mut self) {
        for c in 0..2 {
            self.up[c].reset();
            self.down[c].reset();
            self.dc[c].reset();
            for t in &mut self.tone[c] {
                t.reset();
            }
        }
    }
    fn update(&mut self, p: &Params) -> bool {
        self.set(p);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::effects::{build, info};
    use crate::session::model::EffectSlot;
    use std::collections::BTreeMap;

    fn slot(effect: &str, params: &[(&str, f64)]) -> EffectSlot {
        EffectSlot {
            id: crate::session::model::new_id(),
            effect: effect.into(),
            enabled: true,
            params: params
                .iter()
                .map(|(k, v)| (k.to_string(), *v))
                .collect::<BTreeMap<_, _>>(),
            preset: None,
            sidechain: None,
            plugin_state: None,
            data: None,
            position: None,
            routing: None,
        }
    }
    fn tone(frames: usize, level: f32, hz: f32) -> Vec<f32> {
        (0..frames)
            .flat_map(|i| {
                let v = (std::f32::consts::TAU * hz * i as f32 / 48_000.0).sin() * level;
                [v, v]
            })
            .collect()
    }
    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
    }

    #[test]
    fn the_leveler_evens_out_loud_and_quiet() {
        let s = slot(OPTO, &[("peak", 60.0), ("tube", 0.0)]);
        let mut fx = build(&s, 48_000).unwrap();
        let mut loud = tone(48_000, 0.5, 300.0);
        fx.process(&mut loud);
        let mut fx = build(&s, 48_000).unwrap();
        let mut quiet = tone(48_000, 0.05, 300.0);
        fx.process(&mut quiet);
        let tail = 24_000 * 2..;
        // 20 dB apart going in, much closer coming out.
        let ratio = rms(&loud[tail.clone()]) / rms(&quiet[tail]);
        assert!(ratio < 4.0, "loud / quiet = {ratio}");
        let m = crate::dsp::meters::read(s.id).unwrap();
        assert!(m[0] > 1.0, "gain reduction shows: {m:?}");
    }

    #[test]
    fn the_leveler_lets_go_slowly_after_working_long() {
        let s = slot(OPTO, &[("peak", 60.0), ("tube", 0.0)]);
        let mut fx = build(&s, 48_000).unwrap();
        let mut a = tone(96_000, 0.5, 300.0);
        a.extend(tone(48_000, 0.05, 300.0));
        fx.process(&mut a);
        // Right after the loud part, the quiet part is still held down; a second later, less so.
        let early = rms(&a[(96_000 + 2_400) * 2..(96_000 + 7_200) * 2]);
        let late = rms(&a[(96_000 + 40_000) * 2..(96_000 + 48_000) * 2]);
        assert!(late > early * 1.2, "early {early}, late {late}");
    }

    #[test]
    fn the_preamp_is_clean_when_quiet_and_warm_when_driven() {
        let clean = slot(PREAMP, &[("drive", 0.0), ("transformer", 0.0)]);
        let mut fx = build(&clean, 48_000).unwrap();
        let mut x = tone(48_000, 0.01, 1000.0);
        let before = rms(&x[24_000..]);
        fx.process(&mut x);
        assert!((rms(&x[24_000..]) / before - 1.0).abs() < 0.05);

        // Driven hard, a pure tone grows a second harmonic.
        let hot = slot(PREAMP, &[("drive", 50.0)]);
        let mut fx = build(&hot, 48_000).unwrap();
        let mut y = tone(48_000, 0.5, 1000.0);
        fx.process(&mut y);
        let mono: Vec<f32> = y.as_chunks::<2>().0[24_000..]
            .iter()
            .map(|f| f[0])
            .collect();
        let h = |hz: f32| {
            let (mut re, mut im) = (0f32, 0f32);
            for (i, v) in mono.iter().enumerate() {
                let a = std::f32::consts::TAU * hz * i as f32 / 48_000.0;
                re += v * a.cos();
                im += v * a.sin();
            }
            (re * re + im * im).sqrt()
        };
        let (h1, h2) = (h(1000.0), h(2000.0));
        assert!(h2 > h1 * 0.02, "second harmonic {h2} vs {h1}");
        assert!(info(PREAMP).is_some());
    }
}
