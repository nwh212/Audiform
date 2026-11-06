// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Dynamics Curve: a level-in to level-out curve through up to eight
//! points, so one effect can compress the loud parts, lift the quiet
//! parts and push the very quiet parts down (a gate) all at once. Each
//! point is two ordinary controls (`p1_in`, `p1_out` and so on), so a
//! point can be automated like any other control.
//!
//! The curve runs straight between points in dB and carries on past the
//! first and last point at the slope of the nearest segment. A knee
//! rounds the corners. The level detector follows the input with its own
//! attack and release (RMS or peak); the gain that the curve asks for is
//! then smoothed with a second attack and release before it is applied.
//! Look-ahead delays the audio so the gain can move before a loud sound
//! arrives. The delay is reported as latency.
//!
//! All original code built from textbook techniques.

use std::collections::VecDeque;

use super::{db_to_gain, num, toggle, EffectInfo, Params, Processor};

/// Most points the curve takes.
pub const MAX_POINTS: usize = 8;

const IN_KEYS: [&str; MAX_POINTS] = [
    "p1_in", "p2_in", "p3_in", "p4_in", "p5_in", "p6_in", "p7_in", "p8_in",
];
const OUT_KEYS: [&str; MAX_POINTS] = [
    "p1_out", "p2_out", "p3_out", "p4_out", "p5_out", "p6_out", "p7_out", "p8_out",
];
const GROUPS: [&str; MAX_POINTS] = [
    "Point 1", "Point 2", "Point 3", "Point 4", "Point 5", "Point 6", "Point 7", "Point 8",
];
/// Where each point starts: a straight line, so a new effect changes nothing.
const START: [f64; MAX_POINTS] = [-60.0, 0.0, -40.0, -30.0, -20.0, -10.0, -5.0, -80.0];

/// The lowest and highest input level the curve is drawn over, in dB.
const LOW: f32 = -120.0;
const HIGH: f32 = 24.0;
/// dB between entries of the lookup table.
const STEP: f32 = 0.25;

pub fn info() -> EffectInfo {
    let mut params = vec![
        num(
            "points",
            "Points in use",
            "",
            2.0,
            MAX_POINTS as f64,
            2.0,
            1.0,
        ),
        num("knee", "Knee", "dB", 0.0, 24.0, 0.0, 0.5),
        num("makeup", "Makeup gain", "dB", -24.0, 24.0, 0.0, 0.1),
        num(
            "detect_attack",
            "Detector attack",
            "ms",
            0.1,
            200.0,
            1.0,
            0.1,
        )
        .log(),
        num(
            "detect_release",
            "Detector release",
            "ms",
            5.0,
            2000.0,
            300.0,
            1.0,
        )
        .log(),
        toggle("rms", "Detect RMS (off: peak)", true),
        num("gain_attack", "Gain attack", "ms", 0.1, 200.0, 0.1, 0.1).log(),
        num(
            "gain_release",
            "Gain release",
            "ms",
            5.0,
            2000.0,
            150.0,
            1.0,
        )
        .log(),
        num("lookahead", "Look-ahead", "ms", 0.0, 20.0, 3.0, 0.5),
        toggle("linked", "Link channels", true),
    ];
    for i in 0..MAX_POINTS {
        params
            .push(num(IN_KEYS[i], "In", "dB", f64::from(LOW), 0.0, START[i], 0.1).group(GROUPS[i]));
        params.push(
            num(
                OUT_KEYS[i],
                "Out",
                "dB",
                f64::from(LOW),
                12.0,
                START[i],
                0.1,
            )
            .group(GROUPS[i]),
        );
    }
    EffectInfo {
        id: "altaudio.dynamics_curve",
        name: "Dynamics Curve",
        category: "Dynamics",
        description: "Draws the whole dynamics response as one curve: squeeze the loud, lift the quiet, close the gate on the very quiet.",
        params,
    }
}

/// Smoothing coefficient for a time constant in milliseconds.
fn coef(ms: f64, rate: u32) -> f32 {
    let n = ms.max(0.01) * f64::from(rate) / 1000.0;
    (-1.0 / n).exp() as f32
}

fn to_db(x: f32) -> f32 {
    20.0 * x.max(1e-9).log10()
}

/// The points in use, lowest input first, with any that sit on top of the
/// one before dropped.
pub fn points_from(p: &Params) -> Vec<(f32, f32)> {
    let n = (p.get("points").round() as usize).clamp(2, MAX_POINTS);
    let mut pts: Vec<(f32, f32)> = (0..n)
        .map(|i| (p.get(IN_KEYS[i]) as f32, p.get(OUT_KEYS[i]) as f32))
        .collect();
    pts.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut out: Vec<(f32, f32)> = Vec::with_capacity(n);
    for pt in pts {
        if out.last().is_none_or(|l| pt.0 - l.0 >= 0.01) {
            out.push(pt);
        }
    }
    out
}

/// The curve's output level in dB for an input level in dB: straight
/// between points, and on past the ends at the slope of the nearest
/// segment. `pts` is sorted by input and has no two inputs closer than 0.01.
pub fn curve_linear(pts: &[(f32, f32)], x: f32) -> f32 {
    let y = match pts {
        [] => x,
        [only] => only.1 + (x - only.0),
        _ => {
            let seg = |a: (f32, f32), b: (f32, f32)| (b.1 - a.1) / (b.0 - a.0);
            let first = pts[0];
            let last = pts[pts.len() - 1];
            if x <= first.0 {
                first.1 + (x - first.0) * seg(first, pts[1])
            } else if x >= last.0 {
                last.1 + (x - last.0) * seg(pts[pts.len() - 2], last)
            } else {
                let i = pts
                    .iter()
                    .rposition(|p| p.0 <= x)
                    .unwrap_or(0)
                    .min(pts.len() - 2);
                pts[i].1 + (x - pts[i].0) * seg(pts[i], pts[i + 1])
            }
        }
    };
    y.clamp(-140.0, 60.0)
}

/// The curve sampled every [`STEP`] dB from [`LOW`] to [`HIGH`], with the
/// corners rounded by averaging over a window `knee` dB wide.
pub fn build_table(pts: &[(f32, f32)], knee: f32) -> Vec<f32> {
    let n = ((HIGH - LOW) / STEP).round() as usize + 1;
    let half = (knee / STEP / 2.0).round() as i32;
    (0..n)
        .map(|i| {
            let x = LOW + i as f32 * STEP;
            if half == 0 {
                curve_linear(pts, x)
            } else {
                let sum: f32 = (-half..=half)
                    .map(|j| curve_linear(pts, x + j as f32 * STEP))
                    .sum();
                sum / (2 * half + 1) as f32
            }
        })
        .collect()
}

pub struct DynamicsCurve {
    rate: u32,
    table: Vec<f32>,
    makeup: f32,
    det_attack: f32,
    det_release: f32,
    gain_attack: f32,
    gain_release: f32,
    rms: bool,
    linked: bool,
    look: usize,
    /// The detector's follower per channel.
    env: [f32; 2],
    /// The smoothed gain in dB per channel.
    gain: [f32; 2],
    delay: VecDeque<[f32; 2]>,
}

impl DynamicsCurve {
    pub fn new(p: &Params, rate: u32) -> Self {
        let mut s = Self {
            rate,
            table: Vec::new(),
            makeup: 0.0,
            det_attack: 0.0,
            det_release: 0.0,
            gain_attack: 0.0,
            gain_release: 0.0,
            rms: true,
            linked: true,
            look: 0,
            env: [0.0; 2],
            gain: [0.0; 2],
            delay: VecDeque::new(),
        };
        s.configure(p);
        s.reset();
        s
    }

    /// Takes the settings, keeping the running state.
    fn configure(&mut self, p: &Params) {
        self.table = build_table(&points_from(p), p.get("knee") as f32);
        self.makeup = p.get("makeup") as f32;
        self.det_attack = coef(p.get("detect_attack"), self.rate);
        self.det_release = coef(p.get("detect_release"), self.rate);
        self.gain_attack = coef(p.get("gain_attack"), self.rate);
        self.gain_release = coef(p.get("gain_release"), self.rate);
        self.rms = p.on("rms");
        self.linked = p.on("linked");
        let look = (p.get("lookahead") * f64::from(self.rate) / 1000.0).round() as usize;
        if look != self.look {
            self.look = look;
            self.delay = VecDeque::from(vec![[0.0; 2]; look]);
        }
    }

    /// The curve's output level for an input level, both in dB.
    fn lookup(&self, x: f32) -> f32 {
        let pos = (x.clamp(LOW, HIGH) - LOW) / STEP;
        let i = (pos.floor() as usize).min(self.table.len() - 2);
        let frac = pos - i as f32;
        self.table[i] + (self.table[i + 1] - self.table[i]) * frac
    }

    /// The gain to apply for one channel's input sample magnitude.
    fn follow(&mut self, ch: usize, level: f32) -> f32 {
        let v = if self.rms { level * level } else { level };
        let c = if v > self.env[ch] {
            self.det_attack
        } else {
            self.det_release
        };
        self.env[ch] = v + (self.env[ch] - v) * c;
        let seen = if self.rms {
            self.env[ch].sqrt()
        } else {
            self.env[ch]
        };
        let x = to_db(seen).clamp(LOW, HIGH);
        let want = (self.lookup(x) - x).clamp(-100.0, 60.0);
        let c = if want < self.gain[ch] {
            self.gain_attack
        } else {
            self.gain_release
        };
        self.gain[ch] = want + (self.gain[ch] - want) * c;
        db_to_gain(f64::from(self.gain[ch] + self.makeup))
    }

    /// Processes `buf`, detecting the level of `key` (or of `buf` itself).
    fn run(&mut self, buf: &mut [f32], key: Option<&[f32]>) {
        for (i, f) in buf.as_chunks_mut::<2>().0.iter_mut().enumerate() {
            let k = key.map_or([f[0], f[1]], |k| [k[i * 2], k[i * 2 + 1]]);
            let g = if self.linked {
                let g = self.follow(0, k[0].abs().max(k[1].abs()));
                [g, g]
            } else {
                [self.follow(0, k[0].abs()), self.follow(1, k[1].abs())]
            };
            let heard = if self.look > 0 {
                self.delay.push_back([f[0], f[1]]);
                self.delay.pop_front().unwrap_or([0.0; 2])
            } else {
                [f[0], f[1]]
            };
            f[0] = heard[0] * g[0];
            f[1] = heard[1] * g[1];
        }
    }
}

impl Processor for DynamicsCurve {
    fn process(&mut self, buf: &mut [f32]) {
        self.run(buf, None);
    }
    fn process_keyed(&mut self, buf: &mut [f32], key: &[f32]) {
        self.run(buf, Some(key));
    }
    fn latency(&self) -> usize {
        self.look
    }
    fn reset(&mut self) {
        self.env = [0.0; 2];
        self.gain = [0.0; 2];
        self.delay = VecDeque::from(vec![[0.0; 2]; self.look]);
    }
    fn update(&mut self, p: &Params) -> bool {
        self.configure(p);
        true
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn params(saved: &[(&str, f64)]) -> Params {
        let saved: BTreeMap<String, f64> =
            saved.iter().map(|(k, v)| ((*k).to_string(), *v)).collect();
        Params::resolve(&info(), &saved)
    }

    #[test]
    fn the_curve_runs_straight_between_points_and_on_past_the_ends() {
        let pts = [(-40.0, -40.0), (-20.0, -20.0), (0.0, -10.0)];
        assert!((curve_linear(&pts, -30.0) + 30.0).abs() < 1e-4);
        // Above -20 dB the slope is 0.5: 0 dB in gives -10 dB out.
        assert!((curve_linear(&pts, -10.0) + 15.0).abs() < 1e-4);
        assert!((curve_linear(&pts, 0.0) + 10.0).abs() < 1e-4);
        // Past the last point the slope carries on; below the first, too.
        assert!((curve_linear(&pts, 10.0) + 5.0).abs() < 1e-4);
        assert!((curve_linear(&pts, -60.0) + 60.0).abs() < 1e-4);
    }

    #[test]
    fn points_are_sorted_and_stacked_inputs_dropped() {
        let p = params(&[
            ("points", 3.0),
            ("p1_in", -10.0),
            ("p1_out", -10.0),
            ("p2_in", -50.0),
            ("p2_out", -60.0),
            ("p3_in", -50.0),
            ("p3_out", -30.0),
        ]);
        let pts = points_from(&p);
        assert_eq!(pts.len(), 2);
        assert!(pts[0].0 < pts[1].0);
    }

    #[test]
    fn a_knee_rounds_a_corner_without_moving_the_ends() {
        let pts = [(-40.0, -40.0), (-20.0, -20.0), (0.0, -10.0)];
        let sharp = build_table(&pts, 0.0);
        let soft = build_table(&pts, 8.0);
        let at = |t: &[f32], x: f32| t[((x - LOW) / STEP).round() as usize];
        // At the corner the rounded curve lies below the straight lines' corner.
        assert!(at(&soft, -20.0) < at(&sharp, -20.0));
        // Far from any corner they agree.
        assert!((at(&soft, -60.0) - at(&sharp, -60.0)).abs() < 1e-3);
        assert!((at(&soft, 10.0) - at(&sharp, 10.0)).abs() < 1e-3);
    }

    #[test]
    fn the_default_curve_changes_nothing() {
        let mut fx = DynamicsCurve::new(&params(&[("lookahead", 0.0)]), 48_000);
        let mut buf: Vec<f32> = (0..9_600)
            .map(|i| 0.3 * ((i / 2) as f32 * 0.05).sin())
            .collect();
        let before = buf.clone();
        fx.process(&mut buf);
        let worst = buf
            .iter()
            .zip(&before)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f32::max);
        assert!(worst < 0.01, "worst difference {worst}");
    }

    #[test]
    fn a_steady_tone_is_turned_down_by_what_the_curve_asks() {
        // The curve takes 10 dB off everything.
        let p = params(&[
            ("points", 2.0),
            ("p1_in", -60.0),
            ("p1_out", -70.0),
            ("p2_in", 0.0),
            ("p2_out", -10.0),
            ("rms", 0.0),
            ("lookahead", 0.0),
        ]);
        let mut fx = DynamicsCurve::new(&p, 48_000);
        let mut buf = vec![0.1_f32; 2 * 24_000];
        fx.process(&mut buf);
        let last = buf[buf.len() - 1];
        let want = 0.1 * 10f32.powf(-10.0 / 20.0);
        assert!(
            (last - want).abs() < want * 0.02,
            "got {last}, wanted {want}"
        );
    }

    #[test]
    fn a_curve_can_lift_the_quiet_and_close_the_gate_on_the_very_quiet() {
        // -50 dB in comes out -40 (lifted); -90 dB in comes out -110 (closed down).
        let p = params(&[
            ("points", 4.0),
            ("p1_in", -100.0),
            ("p1_out", -120.0),
            ("p2_in", -70.0),
            ("p2_out", -90.0),
            ("p3_in", -50.0),
            ("p3_out", -40.0),
            ("p4_in", 0.0),
            ("p4_out", 0.0),
            ("rms", 0.0),
            ("lookahead", 0.0),
        ]);
        let mut fx = DynamicsCurve::new(&p, 48_000);
        let level = 10f32.powf(-50.0 / 20.0);
        let mut buf = vec![level; 2 * 96_000];
        fx.process(&mut buf);
        let out_db = to_db(buf[buf.len() - 1]);
        assert!((out_db + 40.0).abs() < 0.5, "got {out_db} dB");
    }

    #[test]
    fn look_ahead_delays_the_audio_and_says_so() {
        let mut fx = DynamicsCurve::new(&params(&[("lookahead", 5.0)]), 48_000);
        assert_eq!(fx.latency(), 240);
        let mut buf = vec![0.0_f32; 2 * 1_000];
        buf[0] = 0.5;
        buf[1] = 0.5;
        fx.process(&mut buf);
        assert!(buf[..2 * 240].iter().all(|v| v.abs() < 1e-6));
        assert!(buf[2 * 240].abs() > 0.4);
    }

    #[test]
    fn automation_updates_the_curve_and_keeps_running() {
        let mut fx = DynamicsCurve::new(&params(&[("lookahead", 0.0), ("rms", 0.0)]), 48_000);
        let mut buf = vec![0.1_f32; 2 * 4_800];
        fx.process(&mut buf);
        let down = params(&[
            ("points", 2.0),
            ("p1_in", -60.0),
            ("p1_out", -80.0),
            ("p2_in", 0.0),
            ("p2_out", -20.0),
            ("rms", 0.0),
            ("lookahead", 0.0),
        ]);
        assert!(fx.update(&down));
        let mut more = vec![0.1_f32; 2 * 24_000];
        fx.process(&mut more);
        let want = 0.1 * 10f32.powf(-20.0 / 20.0);
        assert!((more[more.len() - 1] - want).abs() < want * 0.02);
    }
}
