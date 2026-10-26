// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Phase Curve: turns the phase of each frequency by its own angle, set
//! by a curve through up to eight points (frequency, angle). For
//! restoration: lining up a second mic that is out of phase only in some
//! frequencies, or undoing the phase smear of a filter. The level of
//! every frequency stays the same.
//!
//! Each point is two ordinary numeric controls (`p1_freq`, `p1_phase` and
//! so on), so the points can be automated like any other control. The
//! curve runs straight between points on a log frequency scale, or
//! smoothly through them (a Catmull-Rom spline) with Spline on, and holds
//! level beyond the first and last points.
//!
//! The work is done on the spectrum: each frame is windowed, bin `k` is
//! multiplied by e^(i angle(k)), and the frames are added back with 75%
//! overlap. The frame length is the FFT size; longer frames follow the
//! curve more finely in the low frequencies. The frame's delay is
//! reported as latency.

use std::sync::Arc;

use realfft::num_complex::Complex32;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};

use super::{choice, num, toggle, EffectInfo, ParamInfo, Params, Processor};

/// Most points the curve takes.
pub const MAX_POINTS: usize = 8;

pub const FREQ_KEYS: [&str; MAX_POINTS] = [
    "p1_freq", "p2_freq", "p3_freq", "p4_freq", "p5_freq", "p6_freq", "p7_freq", "p8_freq",
];
pub const PHASE_KEYS: [&str; MAX_POINTS] = [
    "p1_phase", "p2_phase", "p3_phase", "p4_phase", "p5_phase", "p6_phase", "p7_phase", "p8_phase",
];
const GROUPS: [&str; MAX_POINTS] = [
    "Point 1", "Point 2", "Point 3", "Point 4", "Point 5", "Point 6", "Point 7", "Point 8",
];
/// Where unused points start, spread across the range.
const SPREAD: [f64; MAX_POINTS] = [
    20.0, 20_000.0, 100.0, 500.0, 1_000.0, 2_000.0, 5_000.0, 10_000.0,
];

pub const FFT_SIZES: [usize; 7] = [1024, 2048, 4096, 8192, 16384, 32768, 65536];
const FFT_NAMES: [&str; 7] = ["1024", "2048", "4096", "8192", "16384", "32768", "65536"];

pub fn phase_curve_info() -> EffectInfo {
    let mut params: Vec<ParamInfo> = vec![
        num(
            "points",
            "Points in use",
            "",
            2.0,
            MAX_POINTS as f64,
            2.0,
            1.0,
        )
        // Set by adding and removing points on the graph, not as a control.
        .hidden(),
        toggle("spline", "Spline", true),
        choice("range", "Range", &["±180°", "±360°"], 0),
        choice(
            "channels",
            "Channel",
            &["Both channels", "Left only", "Right only"],
            0,
        ),
        choice("fft", "FFT size", &FFT_NAMES, 3),
    ];
    for i in 0..MAX_POINTS {
        params.push(
            num(
                FREQ_KEYS[i],
                "Frequency",
                "Hz",
                10.0,
                24_000.0,
                SPREAD[i],
                1.0,
            )
            .log()
            .group(GROUPS[i]),
        );
        params.push(num(PHASE_KEYS[i], "Phase", "°", -360.0, 360.0, 0.0, 0.1).group(GROUPS[i]));
    }
    EffectInfo {
        id: "altaudio.phase_curve",
        name: "Phase Curve",
        category: "Stereo",
        description: "Turns the phase of each frequency by its own angle, along a curve of points, to line up sound that is out of phase in only some frequencies.",
        params,
    }
}

/// The curve's points in use, by frequency, as (Hz, degrees).
pub fn points(p: &Params) -> Vec<(f64, f64)> {
    let n = (p.get("points").round() as usize).clamp(2, MAX_POINTS);
    let limit = if p.index("range") == 1 { 360.0 } else { 180.0 };
    let mut pts: Vec<(f64, f64)> = (0..n)
        .map(|i| {
            (
                p.get(FREQ_KEYS[i]).max(1.0),
                p.get(PHASE_KEYS[i]).clamp(-limit, limit),
            )
        })
        .collect();
    pts.sort_by(|a, b| a.0.total_cmp(&b.0));
    pts
}

/// The curve's angle in degrees at `hz`: straight between points on a log
/// frequency scale, or a smooth curve through them with `spline`.
pub fn curve_at(pts: &[(f64, f64)], spline: bool, hz: f64) -> f64 {
    let Some(first) = pts.first() else {
        return 0.0;
    };
    let last = pts[pts.len() - 1];
    if hz <= first.0 {
        return first.1;
    }
    if hz >= last.0 {
        return last.1;
    }
    let x = hz.max(1.0).ln();
    let i = pts
        .iter()
        .rposition(|p| p.0 <= hz)
        .unwrap_or(0)
        .min(pts.len() - 2);
    let (x1, y1) = (pts[i].0.ln(), pts[i].1);
    let (x2, y2) = (pts[i + 1].0.ln(), pts[i + 1].1);
    let span = (x2 - x1).max(1e-12);
    let t = ((x - x1) / span).clamp(0.0, 1.0);
    if !spline || pts.len() < 3 {
        return y1 + (y2 - y1) * t;
    }
    // Catmull-Rom through the points, with slopes scaled for uneven spacing.
    let slope = |k: usize| -> f64 {
        let a = pts[k.saturating_sub(1)];
        let b = pts[(k + 1).min(pts.len() - 1)];
        let dx = b.0.ln() - a.0.ln();
        if dx <= 1e-12 {
            0.0
        } else {
            (b.1 - a.1) / dx
        }
    };
    let (m1, m2) = (slope(i) * span, slope(i + 1) * span);
    let t2 = t * t;
    let t3 = t2 * t;
    (2.0 * t3 - 3.0 * t2 + 1.0) * y1
        + (t3 - 2.0 * t2 + t) * m1
        + (-2.0 * t3 + 3.0 * t2) * y2
        + (t3 - t2) * m2
}

pub struct PhaseCurve {
    n: usize,
    hop: usize,
    win: Vec<f32>,
    fwd: Arc<dyn RealToComplex<f32>>,
    inv: Arc<dyn ComplexToReal<f32>>,
    input: [Vec<f32>; 2],
    acc: [Vec<f32>; 2],
    out: [Vec<f32>; 2],
    pos: usize,
    frame: Vec<f32>,
    spec: Vec<Complex32>,
    scratch: Vec<Complex32>,
    rate: f64,
    /// Per bin: the turn to apply (e^(i angle)).
    turns: Vec<Complex32>,
    /// Which channels turn: [left, right].
    rotate: [bool; 2],
}

impl PhaseCurve {
    pub fn new(p: &Params, rate: u32) -> Self {
        let n = FFT_SIZES[p.index("fft").min(FFT_SIZES.len() - 1)];
        let hop = n / 4;
        let mut planner = RealFftPlanner::<f32>::new();
        let fwd = planner.plan_fft_forward(n);
        let inv = planner.plan_fft_inverse(n);
        let scratch_len = fwd.get_scratch_len().max(inv.get_scratch_len());
        let win = (0..n)
            .map(|i| (0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / n as f32).cos()).sqrt())
            .collect();
        let mut s = Self {
            n,
            hop,
            win,
            fwd,
            inv,
            input: [vec![0.0; n], vec![0.0; n]],
            acc: [vec![0.0; n], vec![0.0; n]],
            out: [vec![0.0; hop], vec![0.0; hop]],
            pos: 0,
            frame: vec![0.0; n],
            spec: vec![Complex32::default(); n / 2 + 1],
            scratch: vec![Complex32::default(); scratch_len],
            rate: f64::from(rate.max(1)),
            turns: vec![Complex32::new(1.0, 0.0); n / 2 + 1],
            rotate: [true, true],
        };
        s.set_params(p);
        s
    }

    fn set_params(&mut self, p: &Params) {
        let pts = points(p);
        let spline = p.on("spline");
        let bin_hz = self.rate / self.n as f64;
        for (k, t) in self.turns.iter_mut().enumerate() {
            let a = curve_at(&pts, spline, k as f64 * bin_hz).to_radians() as f32;
            *t = Complex32::new(a.cos(), a.sin());
        }
        self.rotate = match p.index("channels") {
            1 => [true, false],
            2 => [false, true],
            _ => [true, true],
        };
    }

    fn frame(&mut self) {
        let n = self.n;
        let hop = self.hop;
        let scale = 1.0 / (n as f32 * 2.0);
        for c in 0..2 {
            for (o, (x, w)) in self
                .frame
                .iter_mut()
                .zip(self.input[c].iter().zip(&self.win))
            {
                *o = x * w;
            }
            let _ =
                self.fwd
                    .process_with_scratch(&mut self.frame, &mut self.spec, &mut self.scratch);
            let last = self.spec.len() - 1;
            if self.rotate[c] {
                for (s, t) in self.spec[1..last].iter_mut().zip(&self.turns[1..last]) {
                    *s *= *t;
                }
                // DC and Nyquist are real: they can only scale.
                self.spec[0] *= self.turns[0].re;
                self.spec[last] *= self.turns[last].re;
            }
            self.spec[0].im = 0.0;
            self.spec[last].im = 0.0;
            let _ =
                self.inv
                    .process_with_scratch(&mut self.spec, &mut self.frame, &mut self.scratch);
            for ((a, x), w) in self.acc[c].iter_mut().zip(&self.frame).zip(&self.win) {
                *a += x * w * scale;
            }
            self.out[c].copy_from_slice(&self.acc[c][..hop]);
            self.acc[c].copy_within(hop.., 0);
            self.acc[c][n - hop..].fill(0.0);
            self.input[c].copy_within(hop.., 0);
        }
    }
}

impl Processor for PhaseCurve {
    fn process(&mut self, buf: &mut [f32]) {
        let start = self.n - self.hop;
        for f in buf.as_chunks_mut::<2>().0 {
            for (c, v) in f.iter_mut().enumerate() {
                self.input[c][start + self.pos] = *v;
                *v = self.out[c][self.pos];
            }
            self.pos += 1;
            if self.pos == self.hop {
                self.pos = 0;
                self.frame();
            }
        }
    }

    fn latency(&self) -> usize {
        self.n
    }

    fn reset(&mut self) {
        for c in 0..2 {
            self.input[c].fill(0.0);
            self.acc[c].fill(0.0);
            self.out[c].fill(0.0);
        }
        self.pos = 0;
    }

    /// The points, spline, range and channel can change while it plays
    /// (automation); a new FFT size needs a rebuild.
    fn update(&mut self, p: &Params) -> bool {
        if FFT_SIZES[p.index("fft").min(FFT_SIZES.len() - 1)] != self.n {
            return false;
        }
        self.set_params(p);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::model::{new_id, EffectSlot};

    fn params(list: &[(&str, f64)]) -> Params {
        let info = phase_curve_info();
        let slot = EffectSlot {
            id: new_id(),
            effect: info.id.into(),
            enabled: true,
            params: list.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
            preset: None,
            sidechain: None,
            plugin_state: None,
            data: None,
            position: None,
            routing: None,
        };
        Params::resolve(&info, &slot.params)
    }

    #[test]
    fn the_curve_goes_through_its_points() {
        let pts = [(100.0, 0.0), (1_000.0, 90.0), (10_000.0, -90.0)];
        for spline in [false, true] {
            assert!((curve_at(&pts, spline, 1_000.0) - 90.0).abs() < 1e-9);
            assert!(
                (curve_at(&pts, spline, 50.0) - 0.0).abs() < 1e-9,
                "holds below"
            );
            assert!(
                (curve_at(&pts, spline, 20_000.0) + 90.0).abs() < 1e-9,
                "holds above"
            );
        }
        // Straight on a log scale: halfway in pitch is halfway in angle.
        assert!((curve_at(&pts, false, 316.227_766) - 45.0).abs() < 1e-3);
    }

    #[test]
    fn points_sort_and_respect_the_range() {
        let p = params(&[
            ("points", 3.0),
            ("p1_freq", 5_000.0),
            ("p1_phase", 300.0),
            ("p2_freq", 100.0),
            ("p3_freq", 1_000.0),
        ]);
        let pts = points(&p);
        assert_eq!(
            pts.iter().map(|x| x.0).collect::<Vec<_>>(),
            [100.0, 1_000.0, 5_000.0]
        );
        assert_eq!(pts[2].1, 180.0, "±180° by default");
    }

    /// A tone turned by the curve's angle at its frequency, at its level.
    #[test]
    fn turns_a_tone_by_the_curves_angle() {
        let rate = 48_000u32;
        let hz = 1_500.0f32;
        // A flat curve at +90°: the output is the input a quarter turn on.
        let p = params(&[
            ("p1_freq", 20.0),
            ("p1_phase", 90.0),
            ("p2_freq", 20_000.0),
            ("p2_phase", 90.0),
            ("fft", 1.0),
        ]);
        let mut fx = PhaseCurve::new(&p, rate);
        let n = 48_000;
        let mut buf: Vec<f32> = (0..n)
            .flat_map(|i| {
                let v = (std::f32::consts::TAU * hz * i as f32 / rate as f32).sin() * 0.5;
                [v, v]
            })
            .collect();
        fx.process(&mut buf);
        let lat = fx.latency();
        // Compare with the input shifted by the latency: expect -cos (sin turned +90°).
        let mut err = 0f32;
        let mut peak = 0f32;
        for i in 20_000..30_000 {
            let t = (i - lat) as f32;
            let want = (std::f32::consts::TAU * hz * t / rate as f32).sin() * 0.5;
            let turned =
                (std::f32::consts::TAU * hz * t / rate as f32 + std::f32::consts::FRAC_PI_2).sin()
                    * 0.5;
            let _ = want;
            err = err.max((buf[i * 2] - turned).abs());
            peak = peak.max(buf[i * 2].abs());
        }
        assert!((peak - 0.5).abs() < 0.02, "level kept: {peak}");
        assert!(err < 0.02, "a quarter turn: error {err}");
    }

    #[test]
    fn a_flat_zero_curve_passes_the_sound_through() {
        let p = params(&[("fft", 0.0)]);
        let mut fx = PhaseCurve::new(&p, 48_000);
        let mut buf: Vec<f32> = (0..20_000)
            .map(|i| ((i as f32) * 0.01).sin() * 0.3)
            .collect();
        let dry = buf.clone();
        fx.process(&mut buf);
        let lat = fx.latency() * 2;
        let err = buf[lat + 4_000..]
            .iter()
            .zip(&dry[4_000..])
            .fold(0f32, |m, (a, b)| m.max((a - b).abs()));
        assert!(err < 1e-3, "error {err}");
    }
}
