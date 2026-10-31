// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

//! Vox Strip: a whole vocal chain in one effect, in four modules that
//! each pick a style and are run by one or two controls.
//!
//! Signal path: Input, Low Cut, Dynamics, Tone, Effects, Output.
//!
//! * Dynamics: a compressor whose style sets how it reacts (Smooth +
//!   Fast: a slow leveler into a fast peak catcher; Clean: one even
//!   compressor; Warm: a leveler that also adds tube color), plus a
//!   split-band de-esser that only turns down the top when "s" sounds
//!   are loud. Comp and De-ess are one-knob controls.
//! * Tone: Low, Mid and High, voiced by the style (Clean, Vintage,
//!   Modern), and Air.
//! * Effects: one of Chorus, Doubler, Saturate, Radio or Widen, with
//!   one Amount control.
//!
//! There is no reverb or delay module: the strip is meant to go before
//! the Reverb and Delay and Echo effects, which do that job better.
//!
//! Air is not a plain treble boost. It does three things at once:
//! 1. a very wide, gentle shelf that starts around 10 kHz and keeps
//!    rising to the top of the range, so it opens the sound without a
//!    bump you can point at;
//! 2. an exciter: the presence band (about 3 to 7 kHz) is lightly
//!    saturated to make new harmonics, and only those above about 9 kHz
//!    are kept and mixed in, so breath and shimmer appear even where the
//!    recording had little up there;
//! 3. both are ducked while "s" sounds are loud (when the sibilance
//!    band jumps above the voice's body), so the top gets airy without
//!    the sibilance getting harsh.
//!
//! Meters: gain reduction, output level and de-essing.

use std::sync::Arc;

use super::{choice, db_to_gain, num, toggle, EffectInfo, Params, Processor};
use crate::dsp::biquad::{Biquad, Coeffs, Shape};
use crate::dsp::meters::SlotMeter;

pub const VOCAL: &str = "altaudio.vox_strip";

pub const DYN_STYLES: &[&str] = &["Smooth + Fast", "Clean", "Warm"];
pub const TONE_STYLES: &[&str] = &["Clean", "Vintage", "Modern"];
pub const FX_STYLES: &[&str] = &["Chorus", "Doubler", "Saturate", "Radio", "Widen"];

pub fn vocal_info() -> EffectInfo {
    EffectInfo {
        id: VOCAL,
        name: "Vox Strip",
        category: "Vocal",
        description: "A full vocal chain in one: Dynamics (compressor and de-esser), Tone (with Air) and Effects, each with its own style and one-knob controls.",
        params: vec![
            num("input", "Input", "dB", -24.0, 24.0, 0.0, 0.1),
            num("output", "Output", "dB", -24.0, 24.0, 0.0, 0.1),
            toggle("low_cut", "Low Cut", false),
            toggle("dyn_on", "Dynamics", true),
            choice("dyn_style", "Dynamics Style", DYN_STYLES, 0),
            num("comp", "Comp", "%", 0.0, 100.0, 30.0, 1.0),
            num("deess", "De-ess", "%", 0.0, 100.0, 20.0, 1.0),
            num("dyn_trim", "Dynamics Trim", "dB", -12.0, 12.0, 0.0, 0.1),
            toggle("tone_on", "Tone", true),
            choice("tone_style", "Tone Style", TONE_STYLES, 0),
            num("low", "Low", "dB", -12.0, 12.0, 0.0, 0.1),
            num("mid", "Mid", "dB", -12.0, 12.0, 0.0, 0.1),
            num("high", "High", "dB", -12.0, 12.0, 0.0, 0.1),
            num("air", "Air", "%", 0.0, 100.0, 0.0, 1.0),
            num("tone_trim", "Tone Trim", "dB", -12.0, 12.0, 0.0, 0.1),
            toggle("fx_on", "Effects", false),
            choice("fx_style", "Effects Style", FX_STYLES, 0),
            num("fx", "Amount", "%", 0.0, 100.0, 30.0, 1.0),
            num("fx_trim", "Effects Trim", "dB", -12.0, 12.0, 0.0, 0.1),
        ],
    }
}

// ---- Small building blocks ----

#[inline]
fn coef(seconds: f32, rate: f32) -> f32 {
    (-1.0 / (seconds.max(1e-5) * rate)).exp()
}

#[inline]
fn soft(x: f32, k: f32) -> f32 {
    (x * k).tanh() / k
}

/// A feed-forward compressor gain computer with a soft knee.
#[derive(Clone, Copy)]
struct Comp {
    thr: f32,
    ratio_term: f32,
    knee: f32,
    att: f32,
    rel: f32,
    rms: bool,
    det: f32,
    det_c: f32,
    gr: f32,
}

impl Comp {
    fn new() -> Self {
        Self {
            thr: 0.0,
            ratio_term: 0.0,
            knee: 6.0,
            att: 0.0,
            rel: 0.0,
            rms: true,
            det: 0.0,
            det_c: 0.0,
            gr: 0.0,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn set(&mut self, rate: f32, thr: f32, ratio: f32, knee: f32, att: f32, rel: f32, rms: bool) {
        self.thr = thr;
        self.ratio_term = 1.0 - 1.0 / ratio;
        self.knee = knee;
        self.att = coef(att, rate);
        self.rel = coef(rel, rate);
        self.rms = rms;
        self.det_c = coef(if rms { 0.010 } else { 0.0005 }, rate);
    }

    /// Gain reduction in dB for this input sample.
    #[inline]
    fn run(&mut self, x: f32) -> f32 {
        let level = if self.rms {
            self.det = x * x + (self.det - x * x) * self.det_c;
            10.0 * self.det.max(1e-12).log10()
        } else {
            let a = x.abs();
            self.det = if a > self.det {
                a
            } else {
                a + (self.det - a) * self.det_c
            };
            20.0 * self.det.max(1e-6).log10()
        };
        let over = level - self.thr;
        let w = self.knee;
        let want = if over <= -w / 2.0 {
            0.0
        } else if over >= w / 2.0 {
            over * self.ratio_term
        } else {
            let t = over + w / 2.0;
            t * t / (2.0 * w) * self.ratio_term
        };
        let c = if want > self.gr { self.att } else { self.rel };
        self.gr = want + (self.gr - want) * c;
        self.gr
    }

    fn reset(&mut self) {
        self.det = 0.0;
        self.gr = 0.0;
    }
}

/// A modulated delay line for one channel.
struct Line {
    buf: Vec<f32>,
    at: usize,
}

impl Line {
    fn new(len: usize) -> Self {
        Self {
            buf: vec![0.0; len.max(4)],
            at: 0,
        }
    }
    #[inline]
    fn push(&mut self, x: f32) {
        self.at = (self.at + 1) % self.buf.len();
        self.buf[self.at] = x;
    }
    /// The sample `d` samples ago (fractional, linear).
    #[inline]
    fn tap(&self, d: f32) -> f32 {
        let n = self.buf.len();
        let d = d.clamp(1.0, (n - 2) as f32);
        let i = d.floor() as usize;
        let f = d - i as f32;
        let a = self.buf[(self.at + n - i) % n];
        let b = self.buf[(self.at + n - i - 1) % n];
        a + (b - a) * f
    }
    fn clear(&mut self) {
        self.buf.iter_mut().for_each(|v| *v = 0.0);
    }
}

// ---- The strip ----

pub struct VoxStrip {
    rate: f32,
    p: Params,
    input: f32,
    output: f32,
    low_cut: [Biquad; 2],
    low_cut_on: bool,
    // Dynamics
    dyn_on: bool,
    dyn_style: usize,
    level: Comp,
    peak: Comp,
    makeup: f32,
    warm: f32,
    ds_on: bool,
    ds_band: Biquad,
    ds_body: Biquad,
    ds_split: [Biquad; 2],
    ds_env: f32,
    ds_body_env: f32,
    ds_thr: f32,
    /// Share of the overshoot taken off (0 to 1).
    ds_ratio: f32,
    ds_gr: f32,
    dyn_trim: f32,
    // Tone
    tone_on: bool,
    eq: [[Biquad; 3]; 2],
    air_amt: f32,
    air_shelf: [[Biquad; 2]; 2],
    exc_in: [[Biquad; 2]; 2],
    exc_out: [[Biquad; 2]; 2],
    air_det: Biquad,
    air_body: Biquad,
    air_env: f32,
    air_body_env: f32,
    air_duck: f32,
    vintage: bool,
    tone_trim: f32,
    // Effects
    fx_on: bool,
    fx_style: usize,
    fx_amt: f32,
    fx_lines: [Line; 2],
    fx_phase: [f32; 2],
    fx_band: [[Biquad; 2]; 2],
    fx_hp: Biquad,
    fx_trim: f32,
    // Meters
    out_env: f32,
    out_fall: f32,
    gr_show: f32,
    meter: Arc<SlotMeter>,
    tick: u32,
}

/// Biquads for a stereo pair, all set to one shape.
fn pair(c: Coeffs) -> [Biquad; 2] {
    [Biquad::new(c), Biquad::new(c)]
}

impl VoxStrip {
    pub fn new(p: &Params, rate: u32, meter: Arc<SlotMeter>) -> Self {
        let r = rate as f32;
        let flat = Coeffs::FLAT;
        let mut s = Self {
            rate: r,
            p: p.clone(),
            input: 1.0,
            output: 1.0,
            low_cut: pair(flat),
            low_cut_on: false,
            dyn_on: true,
            dyn_style: 0,
            level: Comp::new(),
            peak: Comp::new(),
            makeup: 1.0,
            warm: 0.0,
            ds_on: false,
            ds_band: Biquad::new(flat),
            ds_body: Biquad::new(flat),
            ds_split: pair(flat),
            ds_env: 0.0,
            ds_body_env: 0.0,
            ds_thr: 0.0,
            ds_ratio: 0.5,
            ds_gr: 0.0,
            dyn_trim: 1.0,
            tone_on: true,
            eq: [[Biquad::new(flat); 3]; 2],
            air_amt: 0.0,
            air_shelf: [[Biquad::new(flat); 2]; 2],
            exc_in: [[Biquad::new(flat); 2]; 2],
            exc_out: [[Biquad::new(flat); 2]; 2],
            air_det: Biquad::new(flat),
            air_body: Biquad::new(flat),
            air_env: 0.0,
            air_body_env: 0.0,
            air_duck: 1.0,
            vintage: false,
            tone_trim: 1.0,
            fx_on: false,
            fx_style: 0,
            fx_amt: 0.0,
            fx_lines: [
                Line::new((0.06 * r) as usize),
                Line::new((0.06 * r) as usize),
            ],
            fx_phase: [0.0, 0.25],
            fx_band: [[Biquad::new(flat); 2]; 2],
            fx_hp: Biquad::new(flat),
            fx_trim: 1.0,
            out_env: 0.0,
            out_fall: coef(0.3, r),
            gr_show: 0.0,
            meter,
            tick: 0,
        };
        s.set(p);
        s
    }

    fn set(&mut self, p: &Params) {
        self.p = p.clone();
        let rate = self.rate as u32;
        let r = self.rate;
        let nyq = f64::from(rate) / 2.0;
        let top = |f: f64| f.min(nyq * 0.9);
        let on = |k: &str| p.get(k) >= 0.5;
        let pc = |k: &str| (p.get(k) / 100.0) as f32;
        self.input = db_to_gain(p.get("input"));
        self.output = db_to_gain(p.get("output"));
        self.low_cut_on = on("low_cut");
        for b in &mut self.low_cut {
            b.set(Coeffs::new(Shape::HighPass, rate, 80.0, 0.707, 0.0));
        }

        // Dynamics.
        self.dyn_on = on("dyn_on");
        self.dyn_style = p.get("dyn_style").round() as usize;
        let c = pc("comp");
        match self.dyn_style {
            // Smooth + Fast: a slow, soft leveler, then a fast peak catcher.
            0 => {
                self.level
                    .set(r, -8.0 - 30.0 * c, 3.0, 12.0, 0.010, 0.35, true);
                self.peak
                    .set(r, -2.0 - 16.0 * c, 8.0, 3.0, 0.0008, 0.06, false);
                self.makeup = db_to_gain(f64::from(c * 13.0));
                self.warm = 0.0;
            }
            // Clean: one even compressor.
            1 => {
                self.level
                    .set(r, -6.0 - 30.0 * c, 4.0, 6.0, 0.004, 0.12, true);
                self.peak.set(r, 0.0, 1.0, 0.0, 0.001, 0.05, false);
                self.makeup = db_to_gain(f64::from(c * 12.0));
                self.warm = 0.0;
            }
            // Warm: a slow leveler with a little tube color.
            _ => {
                self.level
                    .set(r, -8.0 - 28.0 * c, 2.5, 14.0, 0.015, 0.6, true);
                self.peak
                    .set(r, -3.0 - 10.0 * c, 4.0, 6.0, 0.002, 0.1, false);
                self.makeup = db_to_gain(f64::from(c * 11.0));
                self.warm = 0.6 + 1.6 * c;
            }
        }
        let d = pc("deess");
        self.ds_on = d > 0.0;
        // The more De-ess, the lower the point where "s" sounds are turned down.
        // The more De-ess, the lower the point where "s" sounds are turned
        // down (on the sibilance band's own level) and the harder.
        self.ds_thr = -14.0 - 30.0 * d;
        self.ds_ratio = 0.55 + 0.4 * d;
        self.ds_band
            .set(Coeffs::new(Shape::BandPass, rate, top(6500.0), 0.7, 0.0));
        self.ds_body
            .set(Coeffs::new(Shape::BandPass, rate, 700.0, 0.6, 0.0));
        for b in &mut self.ds_split {
            b.set(Coeffs::new(Shape::LowPass, rate, top(4500.0), 0.707, 0.0));
        }
        self.dyn_trim = db_to_gain(p.get("dyn_trim"));

        // Tone.
        self.tone_on = on("tone_on");
        let style = p.get("tone_style").round() as usize;
        self.vintage = style == 1;
        let (lf, lq, mf, mq, hf, hq) = match style {
            1 => (100.0, 0.5, 1600.0, 0.6, 8000.0, 0.5),
            2 => (80.0, 0.8, 2500.0, 1.0, 6000.0, 0.8),
            _ => (120.0, 0.7, 1200.0, 0.8, 5000.0, 0.7),
        };
        let cs = [
            Coeffs::new(Shape::LowShelf, rate, lf, lq, p.get("low")),
            Coeffs::new(Shape::Peak, rate, mf, mq, p.get("mid")),
            Coeffs::new(Shape::HighShelf, rate, top(hf), hq, p.get("high")),
        ];
        for ch in &mut self.eq {
            for (b, c) in ch.iter_mut().zip(cs) {
                b.set(c);
            }
        }
        self.air_amt = pc("air");
        let air = f64::from(self.air_amt);
        // A very wide shelf in two stages, so the lift keeps rising to the top.
        let shelf = [
            Coeffs::new(Shape::HighShelf, rate, top(10_000.0), 0.4, air * 4.0),
            Coeffs::new(Shape::HighShelf, rate, top(16_000.0), 0.5, air * 3.0),
        ];
        let exc_in = [
            Coeffs::new(Shape::HighPass, rate, top(3000.0), 0.707, 0.0),
            Coeffs::new(Shape::LowPass, rate, top(7500.0), 0.707, 0.0),
        ];
        let exc_out = [
            Coeffs::new(Shape::HighPass, rate, top(9000.0), 0.6, 0.0),
            Coeffs::new(Shape::HighPass, rate, top(9000.0), 1.2, 0.0),
        ];
        for c in 0..2 {
            for (b, k) in self.air_shelf[c].iter_mut().zip(shelf) {
                b.set(k);
            }
            for (b, k) in self.exc_in[c].iter_mut().zip(exc_in) {
                b.set(k);
            }
            for (b, k) in self.exc_out[c].iter_mut().zip(exc_out) {
                b.set(k);
            }
        }
        self.air_det
            .set(Coeffs::new(Shape::BandPass, rate, top(6500.0), 1.2, 0.0));
        self.air_body
            .set(Coeffs::new(Shape::BandPass, rate, 600.0, 0.5, 0.0));
        self.tone_trim = db_to_gain(p.get("tone_trim"));

        // Effects.
        self.fx_on = on("fx_on");
        self.fx_style = p.get("fx_style").round() as usize;
        self.fx_amt = pc("fx");
        let band = [
            Coeffs::new(Shape::HighPass, rate, 400.0, 0.9, 0.0),
            Coeffs::new(Shape::LowPass, rate, top(3200.0), 0.9, 0.0),
        ];
        for ch in &mut self.fx_band {
            for (b, k) in ch.iter_mut().zip(band) {
                b.set(k);
            }
        }
        self.fx_hp
            .set(Coeffs::new(Shape::HighPass, rate, 300.0, 0.707, 0.0));
        self.fx_trim = db_to_gain(p.get("fx_trim"));
    }

    #[inline]
    fn dynamics(&mut self, f: &mut [f32; 2]) {
        let mono = (f[0] + f[1]) * 0.5;
        let mut gr = self.level.run(mono);
        if self.dyn_style != 1 {
            gr += self.peak.run(mono * db_to_gain(f64::from(-gr)));
        }
        self.gr_show = gr;
        let g = 10f32.powf(-gr / 20.0) * self.makeup;
        for v in f.iter_mut() {
            *v *= g;
            if self.warm > 0.0 {
                let k = self.warm;
                let a = k * (*v + 0.15 * *v * *v);
                *v = if a >= 0.0 {
                    a.tanh()
                } else {
                    (a * 1.2).tanh() / 1.2
                } / k;
            }
        }
        // De-esser: when the sibilance band rises above the voice's body,
        // turn down only what lies above 4.5 kHz.
        if self.ds_on {
            let m = (f[0] + f[1]) * 0.5;
            let s = self.ds_band.run(m);
            let b = self.ds_body.run(m);
            let fast = coef(0.002, self.rate);
            let slow = coef(0.06, self.rate);
            let s2 = s * s;
            self.ds_env = s2 + (self.ds_env - s2) * if s2 > self.ds_env { fast } else { slow };
            let b2 = b * b;
            self.ds_body_env = b2 + (self.ds_body_env - b2) * coef(0.03, self.rate);
            let s_db = 10.0 * self.ds_env.max(1e-12).log10();
            let b_db = 10.0 * self.ds_body_env.max(1e-12).log10();
            // Over the threshold: full strength when the "s" band also
            // stands out from the voice's body (a real "s"), a third of it
            // otherwise, so bright vowels are left mostly alone.
            let over = (s_db - self.ds_thr).max(0.0);
            let hiss = if s_db > b_db - 6.0 { 1.0 } else { 0.33 };
            let want = (over * self.ds_ratio * hiss).min(24.0);
            let c = if want > self.ds_gr { fast } else { slow };
            self.ds_gr = want + (self.ds_gr - want) * c;
            let g = 10f32.powf(-self.ds_gr / 20.0);
            // A complementary split: what is left after the low part is the
            // top, so turning only the top down rebuilds exactly at 0 dB.
            for (v, lp) in f.iter_mut().zip(&mut self.ds_split) {
                let lo = lp.run(*v);
                let hi = *v - lo;
                *v = lo + hi * g;
            }
        } else {
            self.ds_gr = 0.0;
        }
        for v in f.iter_mut() {
            *v *= self.dyn_trim;
        }
    }

    #[inline]
    fn tone(&mut self, f: &mut [f32; 2]) {
        // Air's sibilance watch: how far the "s" band sits above the body.
        let duck = if self.air_amt > 0.0 {
            let m = (f[0] + f[1]) * 0.5;
            let s = self.air_det.run(m);
            let b = self.air_body.run(m);
            let (s2, b2) = (s * s, b * b);
            self.air_env = s2 + (self.air_env - s2) * coef(0.004, self.rate);
            self.air_body_env = b2 + (self.air_body_env - b2) * coef(0.04, self.rate);
            let ratio_db = 10.0 * (self.air_env.max(1e-12) / self.air_body_env.max(1e-12)).log10();
            // Up to 0 dB difference: full air. Above +6 dB: mostly held back.
            let want = 1.0 - ((ratio_db + 0.0) / 6.0).clamp(0.0, 0.8);
            let c = if want < self.air_duck {
                coef(0.002, self.rate)
            } else {
                coef(0.08, self.rate)
            };
            self.air_duck = want + (self.air_duck - want) * c;
            self.air_duck
        } else {
            1.0
        };
        for (c, v) in f.iter_mut().enumerate() {
            let mut x = *v;
            for b in &mut self.eq[c] {
                x = b.run(x);
            }
            if self.vintage {
                x = soft(x, 1.4);
            }
            if self.air_amt > 0.0 {
                // 1. The wide shelf, ducked on sibilance.
                let mut sh = x;
                for b in &mut self.air_shelf[c] {
                    sh = b.run(sh);
                }
                let lift = sh - x;
                // 2. Harmonics made from the presence band, top kept.
                let mut e = x;
                for b in &mut self.exc_in[c] {
                    e = b.run(e);
                }
                let k = 4.0;
                let h = (e * k + 0.3 * (e * k).abs()).tanh();
                let mut top = h;
                for b in &mut self.exc_out[c] {
                    top = b.run(top);
                }
                x += (lift + top * 0.12 * self.air_amt) * duck;
            }
            *v = x * self.tone_trim;
        }
    }

    #[inline]
    #[allow(clippy::needless_range_loop)]
    fn effects(&mut self, f: &mut [f32; 2]) {
        let a = self.fx_amt;
        let r = self.rate;
        match self.fx_style {
            // Chorus: two swept voices, opposite on each side.
            0 => {
                let rate = 0.9 / r;
                for c in 0..2 {
                    self.fx_lines[c].push(f[c]);
                    self.fx_phase[c] = (self.fx_phase[c] + rate).fract();
                    let lfo = (self.fx_phase[c] * std::f32::consts::TAU).sin();
                    let d = (0.018 + 0.004 * lfo) * r;
                    let wet = self.fx_lines[c].tap(d);
                    f[c] += wet * a * 0.7;
                }
            }
            // Doubler: two slowly drifting copies, one each side.
            1 => {
                let m = (f[0] + f[1]) * 0.5;
                for (c, (base, speed)) in [(0.019f32, 0.31f32), (0.026, 0.23)].iter().enumerate() {
                    self.fx_lines[c].push(m);
                    self.fx_phase[c] = (self.fx_phase[c] + speed / r).fract();
                    let lfo = (self.fx_phase[c] * std::f32::consts::TAU).sin();
                    let d = (base + 0.0012 * lfo) * r;
                    f[c] += self.fx_lines[c].tap(d) * a * 0.75;
                }
            }
            // Saturate: drive into a soft clipper, level kept.
            2 => {
                let k = 1.0 + 8.0 * a;
                for v in f.iter_mut() {
                    *v = soft(*v * k, 1.0) / k.sqrt();
                }
            }
            // Radio: a narrow, gritty band.
            3 => {
                for (c, v) in f.iter_mut().enumerate() {
                    let mut x = *v;
                    for b in &mut self.fx_band[c] {
                        x = b.run(x);
                    }
                    x = soft(x * 3.0, 1.0) * 0.7;
                    *v += (x - *v) * a;
                }
            }
            // Widen: a short-delayed, high-passed copy pushes the sides apart.
            _ => {
                let m = (f[0] + f[1]) * 0.5;
                let hp = self.fx_hp.run(m);
                self.fx_lines[0].push(hp);
                let s = self.fx_lines[0].tap(0.012 * r) * a * 0.6;
                f[0] += s;
                f[1] -= s;
            }
        }
        for v in f.iter_mut() {
            *v *= self.fx_trim;
        }
    }
}

impl Processor for VoxStrip {
    fn process(&mut self, buf: &mut [f32]) {
        for f in buf.as_chunks_mut::<2>().0 {
            for v in f.iter_mut() {
                *v *= self.input;
            }
            if self.low_cut_on {
                for (v, b) in f.iter_mut().zip(&mut self.low_cut) {
                    *v = b.run(*v);
                }
            }
            if self.dyn_on {
                self.dynamics(f);
            } else {
                self.gr_show = 0.0;
                self.ds_gr = 0.0;
            }
            if self.tone_on {
                self.tone(f);
            }
            if self.fx_on {
                self.effects(f);
            }
            for v in f.iter_mut() {
                *v *= self.output;
            }
            let a = f[0].abs().max(f[1].abs());
            self.out_env = if a > self.out_env {
                a
            } else {
                self.out_env * self.out_fall
            };
        }
        self.tick += (buf.len() / 2) as u32;
        if self.tick as f32 > self.rate / 60.0 {
            self.tick = 0;
            self.meter
                .store(self.gr_show, 20.0 * self.out_env.max(1e-6).log10());
            self.meter.store_aux(self.ds_gr);
        }
    }

    fn reset(&mut self) {
        self.level.reset();
        self.peak.reset();
        self.ds_env = 0.0;
        self.ds_body_env = 0.0;
        self.ds_gr = 0.0;
        self.air_env = 0.0;
        self.air_body_env = 0.0;
        self.air_duck = 1.0;
        let p = self.p.clone();
        // Rebuilding the filters clears them along with their settings.
        for b in self.low_cut.iter_mut().chain(self.ds_split.iter_mut()) {
            b.reset();
        }
        for b in self
            .eq
            .iter_mut()
            .flatten()
            .chain(self.air_shelf.iter_mut().flatten())
        {
            b.reset();
        }

        for b in self
            .exc_in
            .iter_mut()
            .flatten()
            .chain(self.exc_out.iter_mut().flatten())
            .chain(self.fx_band.iter_mut().flatten())
        {
            b.reset();
        }
        for b in [
            &mut self.ds_band,
            &mut self.ds_body,
            &mut self.air_det,
            &mut self.air_body,
            &mut self.fx_hp,
        ] {
            b.reset();
        }
        for l in self.fx_lines.iter_mut() {
            l.clear();
        }
        self.out_env = 0.0;
        self.set(&p);
    }

    fn update(&mut self, p: &Params) -> bool {
        self.set(p);
        true
    }
}

/// The Tone module's EQ curve (without Air's dynamics or harmonics), in
/// dB at `points` frequencies from 20 Hz to 20 kHz, for the window.
pub fn tone_curve(p: &Params, points: usize) -> Vec<[f64; 2]> {
    let rate = 48_000;
    let style = p.get("tone_style").round() as usize;
    let (lf, lq, mf, mq, hf, hq) = match style {
        1 => (100.0, 0.5, 1600.0, 0.6, 8000.0, 0.5),
        2 => (80.0, 0.8, 2500.0, 1.0, 6000.0, 0.8),
        _ => (120.0, 0.7, 1200.0, 0.8, 5000.0, 0.7),
    };
    let air = p.get("air") / 100.0;
    let mut cs = vec![
        Coeffs::new(Shape::LowShelf, rate, lf, lq, p.get("low")),
        Coeffs::new(Shape::Peak, rate, mf, mq, p.get("mid")),
        Coeffs::new(Shape::HighShelf, rate, hf, hq, p.get("high")),
        Coeffs::new(Shape::HighShelf, rate, 10_000.0, 0.4, air * 4.0),
        Coeffs::new(Shape::HighShelf, rate, 16_000.0, 0.5, air * 3.0),
    ];
    if p.get("low_cut") >= 0.5 {
        cs.push(Coeffs::new(Shape::HighPass, rate, 80.0, 0.707, 0.0));
    }
    let n = points.max(2);
    (0..n)
        .map(|i| {
            let f = 20.0 * 1000f64.powf(i as f64 / (n - 1) as f64);
            [f, cs.iter().map(|c| c.response_db(rate, f)).sum()]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::effects::{build, info};
    use crate::session::model::EffectSlot;
    use std::collections::BTreeMap;

    fn slot(params: &[(&str, f64)]) -> EffectSlot {
        EffectSlot {
            id: crate::session::model::new_id(),
            effect: VOCAL.into(),
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
        (x.iter().map(|v| v * v).sum::<f32>() / x.len().max(1) as f32).sqrt()
    }

    /// Energy of the left channel at `hz`.
    fn at(x: &[f32], hz: f32) -> f32 {
        let (mut re, mut im) = (0f32, 0f32);
        for (i, f) in x.as_chunks::<2>().0.iter().enumerate() {
            let a = std::f32::consts::TAU * hz * i as f32 / 48_000.0;
            re += f[0] * a.cos();
            im += f[0] * a.sin();
        }
        (re * re + im * im).sqrt()
    }

    const ALL_OFF: [(&str, f64); 3] = [("dyn_on", 0.0), ("tone_on", 0.0), ("fx_on", 0.0)];

    #[test]
    fn with_every_module_off_it_passes_sound_unchanged() {
        let mut fx = build(&slot(&ALL_OFF), 48_000).unwrap();
        let mut x = tone(4800, 0.3, 440.0);
        let y = x.clone();
        fx.process(&mut x);
        assert!(x.iter().zip(&y).all(|(a, b)| (a - b).abs() < 1e-6));
        assert!(info(VOCAL).is_some());
    }

    #[test]
    fn the_compressor_evens_out_level_and_reports_it() {
        let s = slot(&[("comp", 80.0), ("deess", 0.0), ("tone_on", 0.0)]);
        let mut fx = build(&s, 48_000).unwrap();
        let mut loud = tone(48_000, 0.5, 300.0);
        fx.process(&mut loud);
        let mut fx = build(&s, 48_000).unwrap();
        let mut quiet = tone(48_000, 0.05, 300.0);
        fx.process(&mut quiet);
        let ratio = rms(&loud[48_000..]) / rms(&quiet[48_000..]);
        assert!(ratio < 4.0, "20 dB in, {ratio} out");
        assert!(crate::dsp::meters::read(s.id).unwrap()[0] > 1.0);
    }

    #[test]
    fn air_adds_top_end_that_was_not_there() {
        // A 5 kHz tone has nothing at 10 kHz. Air makes some (its
        // harmonics), where a shelf alone could only lift what exists.
        let off = slot(&[("dyn_on", 0.0), ("air", 0.0)]);
        let on = slot(&[("dyn_on", 0.0), ("air", 100.0)]);
        let mut a = tone(24_000, 0.2, 5000.0);
        let mut b = a.clone();
        build(&off, 48_000).unwrap().process(&mut a);
        build(&on, 48_000).unwrap().process(&mut b);
        let tail = 12_000..;
        let before = at(&a[tail.clone()], 10_000.0);
        let after = at(&b[tail], 10_000.0);
        assert!(after > before * 20.0 + 1.0, "10 kHz: {before} -> {after}");
    }

    #[test]
    fn air_holds_back_on_sibilance() {
        // Loud "s"-band energy with little body: Air's top lift is ducked
        // compared with a steady, voiced sound with the same top.
        let s = slot(&[("dyn_on", 0.0), ("air", 100.0)]);
        let voiced: Vec<f32> = tone(24_000, 0.3, 500.0)
            .iter()
            .zip(tone(24_000, 0.02, 12_000.0))
            .map(|(a, b)| a + b)
            .collect();
        let hiss: Vec<f32> = tone(24_000, 0.3, 6_500.0)
            .iter()
            .zip(tone(24_000, 0.02, 12_000.0))
            .map(|(a, b)| a + b)
            .collect();
        let gain_at_12k = |x: &[f32]| {
            let mut y = x.to_vec();
            build(&s, 48_000).unwrap().process(&mut y);
            at(&y[24_000..], 12_000.0) / at(&x[24_000..], 12_000.0)
        };
        let (g_voiced, g_hiss) = (gain_at_12k(&voiced), gain_at_12k(&hiss));
        assert!(
            g_voiced > g_hiss * 1.3,
            "voiced {g_voiced}, sibilant {g_hiss}"
        );
    }

    #[test]
    fn the_de_esser_takes_loud_s_sounds_down_hard() {
        // A loud "s" burst (6.5 kHz) with a little voice under it.
        let s = slot(&[("comp", 0.0), ("deess", 60.0), ("tone_on", 0.0)]);
        let x: Vec<f32> = tone(24_000, 0.4, 6_500.0)
            .iter()
            .zip(tone(24_000, 0.05, 300.0))
            .map(|(a, b)| a + b)
            .collect();
        let mut y = x.clone();
        build(&s, 48_000).unwrap().process(&mut y);
        let cut = at(&x[24_000..], 6_500.0) / at(&y[24_000..], 6_500.0);
        let db = 20.0 * cut.log10();
        assert!(db > 8.0, "only {db} dB off the s");
        // The voice below 4.5 kHz is kept.
        let body = at(&y[24_000..], 300.0) / at(&x[24_000..], 300.0);
        assert!(body > 0.8, "voice kept: {body}");
        assert!(crate::dsp::meters::read(s.id).unwrap()[2] > 6.0);
    }

    #[test]
    fn old_space_settings_add_nothing() {
        // Sessions saved with the Space module on still load; it is gone,
        // so the sound has no tail after the input stops.
        let s = slot(&[
            ("dyn_on", 0.0),
            ("tone_on", 0.0),
            ("space_on", 1.0),
            ("reverb", 80.0),
        ]);
        let mut fx = build(&s, 48_000).unwrap();
        let mut x = tone(4800, 0.5, 440.0);
        x.extend(vec![0.0; 48_000]);
        fx.process(&mut x);
        let tail = rms(&x[(4800 + 9600) * 2..(4800 + 24_000) * 2]);
        assert!(tail < 1e-6, "tail {tail}");
    }

    #[test]
    fn every_style_runs_clean() {
        for (k, n) in [("dyn_style", 3), ("tone_style", 3), ("fx_style", 5)] {
            for i in 0..n {
                let s = slot(&[(k, i as f64), ("fx_on", 1.0), ("air", 50.0), ("fx", 80.0)]);
                let mut fx = build(&s, 44_100).unwrap();
                let mut x = tone(44_100, 0.5, 220.0);
                fx.process(&mut x);
                assert!(x.iter().all(|v| v.is_finite() && v.abs() < 8.0), "{k} {i}");
            }
        }
    }

    #[test]
    fn the_tone_curve_shows_the_settings() {
        let i = vocal_info();
        let mut saved = BTreeMap::new();
        saved.insert("low".to_string(), 6.0);
        let p = Params::resolve(&i, &saved);
        let c = tone_curve(&p, 50);
        assert!(c[0][1] > 4.0 && c[49][1].abs() < 1.0);
    }
}
