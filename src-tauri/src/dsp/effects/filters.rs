// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! More filter and EQ effects: Band EQ (10, 20 or 30 fixed bands),
//! Notch Filter (several narrow cuts), Precision Filter (classic filter
//! designs at a chosen order), Curve EQ (a smooth response through a few
//! points, applied in the spectrum) and Center Remover. Original code
//! from textbook filter design; coefficients use the same published
//! cookbook formulas as the Parametric EQ.

use super::{choice, db_to_gain, num, toggle, EffectInfo, ParamInfo, Params, Processor};
use crate::dsp::biquad::{butterworth_qs, Coeffs, Shape, StereoBiquad};
use crate::dsp::stft::{Mask, Stft};

/// Octave centres for the 10-band EQ.
const OCTAVE: [f64; 10] = [
    31.5, 63.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16_000.0,
];
/// Third-octave centres for the 30-band EQ.
const THIRD: [f64; 30] = [
    25.0, 31.5, 40.0, 50.0, 63.0, 80.0, 100.0, 125.0, 160.0, 200.0, 250.0, 315.0, 400.0, 500.0,
    630.0, 800.0, 1000.0, 1250.0, 1600.0, 2000.0, 2500.0, 3150.0, 4000.0, 5000.0, 6300.0, 8000.0,
    10_000.0, 12_500.0, 16_000.0, 20_000.0,
];

/// Band centres for an EQ with `n` bands (10, 20 or 30).
pub fn band_centres(n: usize) -> Vec<f64> {
    match n {
        10 => OCTAVE.to_vec(),
        30 => THIRD.to_vec(),
        // Half-octave steps from 31.5 Hz.
        _ => (0..20).map(|k| 31.5 * 2f64.powf(k as f64 / 2.0)).collect(),
    }
}

fn band_label(f: f64) -> String {
    if f >= 1000.0 {
        let k = f / 1000.0;
        if (k - k.round()).abs() < 1e-9 {
            format!("{} kHz", k.round())
        } else {
            format!("{k:.2}")
                .trim_end_matches('0')
                .trim_end_matches('.')
                .to_string()
                + " kHz"
        }
    } else if (f - f.round()).abs() < 1e-9 {
        format!("{} Hz", f.round())
    } else {
        format!("{f:.1} Hz")
    }
}

/// Parameter keys and labels per band, made once: the catalog needs
/// `'static` text and is asked for often.
fn band_names(n: usize) -> &'static [(&'static str, &'static str)] {
    use std::sync::OnceLock;
    static NAMES: [OnceLock<Vec<(&'static str, &'static str)>>; 3] =
        [OnceLock::new(), OnceLock::new(), OnceLock::new()];
    let slot = match n {
        10 => 0,
        30 => 2,
        _ => 1,
    };
    NAMES[slot].get_or_init(|| {
        band_centres(n)
            .into_iter()
            .enumerate()
            .map(|(i, f)| {
                let key: &'static str = Box::leak(format!("g{:02}", i + 1).into_boxed_str());
                let label: &'static str = Box::leak(band_label(f).into_boxed_str());
                (key, label)
            })
            .collect()
    })
}

fn band_key(n: usize, i: usize) -> &'static str {
    band_names(n)[i].0
}

fn band_eq_info(n: usize) -> EffectInfo {
    let mut params: Vec<ParamInfo> = band_names(n)
        .iter()
        .map(|(key, label)| num(key, label, "dB", -15.0, 15.0, 0.0, 0.1).group("Bands"))
        .collect();
    params.push(num("output", "Output", "dB", -15.0, 15.0, 0.0, 0.1));
    let (id, name) = match n {
        10 => ("altaudio.band_eq_10", "Band EQ (10 bands)"),
        20 => ("altaudio.band_eq_20", "Band EQ (20 bands)"),
        _ => ("altaudio.band_eq_30", "Band EQ (30 bands)"),
    };
    EffectInfo {
        id,
        name,
        category: "Filter and EQ",
        description:
            "Fixed bands across the range, each turned up or down: quick broad tone shaping.",
        params,
    }
}

pub fn band_infos() -> [EffectInfo; 3] {
    [band_eq_info(10), band_eq_info(20), band_eq_info(30)]
}

pub struct BandEq {
    filters: Vec<StereoBiquad>,
    gain: f32,
}

impl BandEq {
    pub fn new(n: usize, p: &Params, rate: u32) -> Self {
        // Q for the band spacing: octave 1.41, half-octave 2.87, third 4.32.
        let q = match n {
            10 => 1.41,
            30 => 4.32,
            _ => 2.87,
        };
        let nyquist = f64::from(rate) * 0.45;
        let filters = band_centres(n)
            .into_iter()
            .enumerate()
            .filter(|(i, f)| *f < nyquist && p.get(band_key(n, *i)).abs() > 1e-3)
            .map(|(i, f)| {
                StereoBiquad::new(Coeffs::new(Shape::Peak, rate, f, q, p.get(band_key(n, i))))
            })
            .collect();
        Self {
            filters,
            gain: db_to_gain(p.get("output")),
        }
    }
}

impl Processor for BandEq {
    fn process(&mut self, buf: &mut [f32]) {
        for f in &mut self.filters {
            f.process(buf);
        }
        if self.gain != 1.0 {
            for s in buf.iter_mut() {
                *s *= self.gain;
            }
        }
    }
    fn reset(&mut self) {
        for f in &mut self.filters {
            f.reset();
        }
    }
}

// --- Notch Filter ---

const NOTCHES: usize = 6;
const NOTCH_KEYS: [[&str; 3]; NOTCHES] = [
    ["n1_on", "n1_freq", "n1_depth"],
    ["n2_on", "n2_freq", "n2_depth"],
    ["n3_on", "n3_freq", "n3_depth"],
    ["n4_on", "n4_freq", "n4_depth"],
    ["n5_on", "n5_freq", "n5_depth"],
    ["n6_on", "n6_freq", "n6_depth"],
];
const NOTCH_GROUPS: [&str; NOTCHES] = [
    "Notch 1", "Notch 2", "Notch 3", "Notch 4", "Notch 5", "Notch 6",
];

pub fn notch_info() -> EffectInfo {
    let defaults = [1000.0, 2000.0, 3000.0, 4000.0, 5000.0, 6000.0];
    let mut params = Vec::new();
    for (k, keys) in NOTCH_KEYS.iter().enumerate() {
        let g = NOTCH_GROUPS[k];
        params.push(toggle(keys[0], "On", k == 0).group(g));
        params.push(
            num(keys[1], "Frequency", "Hz", 20.0, 20_000.0, defaults[k], 1.0)
                .log()
                .group(g),
        );
        params.push(num(keys[2], "Depth", "dB", -60.0, 0.0, -30.0, 0.5).group(g));
    }
    params.push(num("width", "Width (Q)", "", 1.0, 100.0, 20.0, 0.5).log());
    EffectInfo {
        id: "altaudio.notch",
        name: "Notch Filter",
        category: "Filter and EQ",
        description:
            "Up to six narrow cuts at exact frequencies: a whine, a feedback ring, a buzz.",
        params,
    }
}

pub struct Notches(Vec<StereoBiquad>);

impl Notches {
    pub fn new(p: &Params, rate: u32) -> Self {
        let q = p.get("width");
        Self(
            NOTCH_KEYS
                .iter()
                .filter(|k| p.on(k[0]))
                .map(|k| {
                    StereoBiquad::new(Coeffs::new(Shape::Peak, rate, p.get(k[1]), q, p.get(k[2])))
                })
                .collect(),
        )
    }
}

impl Processor for Notches {
    fn process(&mut self, buf: &mut [f32]) {
        for f in &mut self.0 {
            f.process(buf);
        }
    }
    fn reset(&mut self) {
        for f in &mut self.0 {
            f.reset();
        }
    }
}

// --- Precision Filter ---

const DESIGNS: &[&str] = &[
    "Butterworth (flattest)",
    "Chebyshev (steepest, some ripple)",
    "Bessel (gentlest, cleanest timing)",
    "Elliptic (steepest of all, ripple in both bands)",
];
const MODES: &[&str] = &["Low-pass", "High-pass", "Band-pass", "Band-stop"];
const ORDERS: &[&str] = &[
    "2nd order (12 dB/oct)",
    "4th order (24 dB/oct)",
    "6th order (36 dB/oct)",
    "8th order (48 dB/oct)",
];

pub fn precision_info() -> EffectInfo {
    EffectInfo {
        id: "altaudio.precision_filter",
        name: "Precision Filter",
        category: "Filter and EQ",
        description:
            "Classic filter designs at an exact order, for when the slope and shape matter.",
        params: vec![
            choice("design", "Design", DESIGNS, 0),
            choice("mode", "Type", MODES, 1),
            choice("order", "Order", ORDERS, 1),
            num("freq", "Frequency", "Hz", 10.0, 20_000.0, 100.0, 1.0).log(),
            num(
                "freq2",
                "Upper frequency (bands)",
                "Hz",
                20.0,
                20_000.0,
                4000.0,
                1.0,
            )
            .log(),
            num(
                "ripple",
                "Ripple (Chebyshev, Elliptic)",
                "dB",
                0.1,
                3.0,
                0.5,
                0.05,
            ),
            num(
                "stop",
                "Stopband depth (Elliptic)",
                "dB",
                20.0,
                120.0,
                60.0,
                1.0,
            ),
        ],
    }
}

/// One second-order section of the analog prototype: frequency scale
/// relative to the cutoff, and Q.
fn sections(design: usize, order: usize, ripple: f64) -> Vec<(f64, f64)> {
    let pairs = order / 2;
    match design {
        1 => {
            // Chebyshev type I poles; each pair gives a section's
            // natural frequency and Q.
            let eps = (10f64.powf(ripple / 10.0) - 1.0).sqrt();
            let mu = (1.0 / eps).asinh() / order as f64;
            (1..=pairs)
                .map(|k| {
                    let th = std::f64::consts::PI * (2 * k - 1) as f64 / (2 * order) as f64;
                    let s = mu.sinh() * th.sin();
                    let w = mu.cosh() * th.cos();
                    let w0 = (s * s + w * w).sqrt();
                    (w0, w0 / (2.0 * s))
                })
                .collect()
        }
        2 => match order {
            // Bessel sections, normalised to -3 dB at the cutoff.
            2 => vec![(1.2736, 0.5773)],
            4 => vec![(1.4192, 0.5219), (1.5912, 0.8055)],
            6 => vec![(1.6060, 0.5103), (1.6913, 0.6112), (1.9071, 1.0234)],
            _ => vec![
                (1.7837, 0.5060),
                (1.8376, 0.5596),
                (1.9591, 0.7109),
                (2.1953, 1.2257),
            ],
        },
        _ => butterworth_qs(pairs)
            .into_iter()
            .map(|q| (1.0, q))
            .collect(),
    }
}

/// The analog design and its depth for one Precision Filter.
#[derive(Clone, Copy)]
struct Design {
    kind: usize,
    order: usize,
    ripple: f64,
    stop: f64,
}

fn cascade(shape: Shape, d: Design, freq: f64, rate: u32) -> Vec<Coeffs> {
    if d.kind == 3 {
        return crate::dsp::elliptic::elliptic(
            shape == Shape::HighPass,
            d.order,
            d.ripple,
            d.stop,
            freq,
            rate,
        );
    }
    sections(d.kind, d.order, d.ripple)
        .into_iter()
        .map(|(scale, q)| {
            let f = if shape == Shape::LowPass {
                freq * scale
            } else {
                freq / scale
            };
            Coeffs::new(shape, rate, f, q, 0.0)
        })
        .collect()
}

pub struct Precision {
    /// Filters in series.
    chain: Vec<StereoBiquad>,
    /// Band-stop: a low-pass and a high-pass in parallel, summed.
    split: Option<(Vec<StereoBiquad>, Vec<StereoBiquad>)>,
    scratch: Vec<f32>,
}

impl Precision {
    pub fn new(p: &Params, rate: u32) -> Self {
        let design = Design {
            kind: p.index("design").min(3),
            order: [2, 4, 6, 8][p.index("order").min(3)],
            ripple: p.get("ripple"),
            stop: p.get("stop"),
        };
        let lo = p.get("freq").min(p.get("freq2"));
        let hi = p.get("freq").max(p.get("freq2"));
        let mk = |v: Vec<Coeffs>| v.into_iter().map(StereoBiquad::new).collect::<Vec<_>>();
        let (chain, split) = match p.index("mode") {
            0 => (
                mk(cascade(Shape::LowPass, design, p.get("freq"), rate)),
                None,
            ),
            1 => (
                mk(cascade(Shape::HighPass, design, p.get("freq"), rate)),
                None,
            ),
            2 => {
                let mut c = mk(cascade(Shape::HighPass, design, lo, rate));
                c.extend(mk(cascade(Shape::LowPass, design, hi, rate)));
                (c, None)
            }
            _ => (
                Vec::new(),
                Some((
                    mk(cascade(Shape::LowPass, design, lo, rate)),
                    mk(cascade(Shape::HighPass, design, hi, rate)),
                )),
            ),
        };
        Self {
            chain,
            split,
            scratch: Vec::new(),
        }
    }
}

impl Processor for Precision {
    fn process(&mut self, buf: &mut [f32]) {
        for f in &mut self.chain {
            f.process(buf);
        }
        if let Some((low, high)) = self.split.as_mut() {
            self.scratch.clear();
            self.scratch.extend_from_slice(buf);
            for f in low.iter_mut() {
                f.process(buf);
            }
            for f in high.iter_mut() {
                f.process(&mut self.scratch);
            }
            for (a, b) in buf.iter_mut().zip(&self.scratch) {
                *a += b;
            }
        }
    }
    fn reset(&mut self) {
        for f in &mut self.chain {
            f.reset();
        }
        if let Some((a, b)) = self.split.as_mut() {
            for f in a.iter_mut().chain(b.iter_mut()) {
                f.reset();
            }
        }
    }
}

// --- Curve EQ ---

const POINTS: usize = 8;
const CURVE_KEYS: [[&str; 2]; POINTS] = [
    ["p1_freq", "p1_gain"],
    ["p2_freq", "p2_gain"],
    ["p3_freq", "p3_gain"],
    ["p4_freq", "p4_gain"],
    ["p5_freq", "p5_gain"],
    ["p6_freq", "p6_gain"],
    ["p7_freq", "p7_gain"],
    ["p8_freq", "p8_gain"],
];
const CURVE_GROUPS: [&str; POINTS] = [
    "Point 1", "Point 2", "Point 3", "Point 4", "Point 5", "Point 6", "Point 7", "Point 8",
];

pub fn curve_info() -> EffectInfo {
    let freqs = [40.0, 100.0, 250.0, 600.0, 1500.0, 3500.0, 8000.0, 16_000.0];
    let mut params = Vec::new();
    for (k, keys) in CURVE_KEYS.iter().enumerate() {
        let g = CURVE_GROUPS[k];
        params.push(
            num(keys[0], "Frequency", "Hz", 20.0, 20_000.0, freqs[k], 1.0)
                .log()
                .group(g),
        );
        params.push(num(keys[1], "Gain", "dB", -40.0, 20.0, 0.0, 0.1).group(g));
    }
    EffectInfo {
        id: "altaudio.curve_eq",
        name: "Curve EQ",
        category: "Filter and EQ",
        description: "A smooth response through eight points, with no phase shift: surgical shaping and steep cuts.",
        params,
    }
}

/// A fixed gain per bin, from the points joined smoothly on a log scale.
pub struct CurveMask(Vec<f32>);

impl Mask for CurveMask {
    fn mask(&mut self, _power: &[f32], gains: &mut [f32]) {
        gains.copy_from_slice(&self.0);
    }
    fn reset(&mut self) {}
}

pub fn curve_gains(points: &[(f64, f64)], bins: usize, rate: u32) -> Vec<f32> {
    let mut pts: Vec<(f64, f64)> = points
        .iter()
        .map(|(f, g)| (f.max(1.0).log2(), *g))
        .collect();
    pts.sort_by(|a, b| a.0.total_cmp(&b.0));
    let n = (bins - 1) * 2;
    (0..bins)
        .map(|k| {
            let f = (k as f64 * f64::from(rate) / n as f64).max(1.0).log2();
            let db = match pts.iter().position(|p| p.0 >= f) {
                None => pts.last().map_or(0.0, |p| p.1),
                Some(0) => pts[0].1,
                Some(i) => {
                    let (a, b) = (pts[i - 1], pts[i]);
                    let t = if b.0 > a.0 {
                        (f - a.0) / (b.0 - a.0)
                    } else {
                        0.0
                    };
                    // Smoothstep between points so the curve has no corners.
                    let t = t * t * (3.0 - 2.0 * t);
                    a.1 + (b.1 - a.1) * t
                }
            };
            db_to_gain(db)
        })
        .collect()
}

pub struct CurveEq(Stft<CurveMask>);

impl CurveEq {
    pub fn new(p: &Params, rate: u32) -> Self {
        let n = Stft::<CurveMask>::size_for(80.0, rate);
        let pts: Vec<(f64, f64)> = CURVE_KEYS
            .iter()
            .map(|k| (p.get(k[0]), p.get(k[1])))
            .collect();
        Self(Stft::new(n, CurveMask(curve_gains(&pts, n / 2 + 1, rate))))
    }
}

impl Processor for CurveEq {
    fn process(&mut self, buf: &mut [f32]) {
        self.0.process(buf);
    }
    fn latency(&self) -> usize {
        self.0.latency()
    }
    fn reset(&mut self) {
        self.0.reset();
    }
}

// --- Center Remover ---

pub fn center_info() -> EffectInfo {
    EffectInfo {
        id: "altaudio.center_remover",
        name: "Center Remover",
        category: "Filter and EQ",
        description: "Removes what sits in the middle of a stereo mix, usually the lead vocal, and keeps the bass.",
        params: vec![
            num("amount", "Amount", "%", 0.0, 100.0, 100.0, 1.0),
            num("keep_below", "Keep bass below", "Hz", 20.0, 500.0, 150.0, 1.0).log(),
        ],
    }
}

pub struct CenterRemover {
    amount: f32,
    low: [StereoBiquad; 2],
}

impl CenterRemover {
    pub fn new(p: &Params, rate: u32) -> Self {
        let qs = butterworth_qs(2);
        let f = p.get("keep_below");
        Self {
            amount: (p.get("amount") / 100.0) as f32,
            low: [
                StereoBiquad::new(Coeffs::new(Shape::LowPass, rate, f, qs[0], 0.0)),
                StereoBiquad::new(Coeffs::new(Shape::LowPass, rate, f, qs[1], 0.0)),
            ],
        }
    }
}

impl Processor for CenterRemover {
    fn process(&mut self, buf: &mut [f32]) {
        for fr in buf.as_chunks_mut::<2>().0 {
            let (l, r) = (fr[0], fr[1]);
            let mid = (l + r) * 0.5;
            // The bass of the middle is kept, so kick and bass guitar stay.
            let first = self.low[0].l.run(mid);
            let keep = self.low[1].l.run(first);
            let cut = (mid - keep) * self.amount;
            fr[0] = l - cut;
            fr[1] = r - cut;
        }
    }
    fn reset(&mut self) {
        for f in &mut self.low {
            f.reset();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::build;
    use super::super::tests::slot;
    use std::f64::consts::PI;

    fn level(effect: &str, params: &[(&str, f64)], freq: f64) -> f64 {
        let rate = 48_000;
        let mut p = build(&slot(effect, params), rate).unwrap();
        let n = rate as usize;
        let mut b: Vec<f32> = (0..n)
            .flat_map(|i| {
                let v = (2.0 * PI * freq * i as f64 / f64::from(rate)).sin() as f32 * 0.5;
                [v, v]
            })
            .collect();
        for c in b.chunks_mut(2 * 1000) {
            p.process(c);
        }
        let peak = b[n..].iter().fold(0f32, |m, s| m.max(s.abs()));
        20.0 * (f64::from(peak) / 0.5).log10()
    }

    #[test]
    fn band_eq_moves_each_band() {
        assert!(
            level("altaudio.band_eq_10", &[], 1000.0).abs() < 0.05,
            "flat by default"
        );
        assert!((level("altaudio.band_eq_10", &[("g06", 6.0)], 1000.0) - 6.0).abs() < 0.5);
        assert!((level("altaudio.band_eq_30", &[("g17", -9.0)], 1000.0) + 9.0).abs() < 0.6);
        assert!(
            level("altaudio.band_eq_30", &[("g17", -9.0)], 4000.0).abs() < 0.5,
            "other bands stay"
        );
        assert!((level("altaudio.band_eq_20", &[("output", -3.0)], 1000.0) + 3.0).abs() < 0.05);
    }

    #[test]
    fn notches_cut_only_their_frequencies() {
        let p = [("n1_on", 1.0), ("n1_freq", 2000.0), ("n1_depth", -40.0)];
        assert!(level("altaudio.notch", &p, 2000.0) < -35.0);
        assert!(level("altaudio.notch", &p, 1000.0).abs() < 0.5);
    }

    #[test]
    fn precision_designs_have_their_slopes() {
        let lp = |design: f64, order: f64, f: f64| {
            level(
                "altaudio.precision_filter",
                &[
                    ("design", design),
                    ("mode", 0.0),
                    ("order", order),
                    ("freq", 1000.0),
                ],
                f,
            )
        };
        // An octave above the cutoff: 4th-order Butterworth is about -24 dB.
        let b4 = lp(0.0, 1.0, 2000.0);
        assert!((b4 + 24.0).abs() < 2.0, "butterworth {b4}");
        // Chebyshev is steeper, Bessel gentler, at the same order.
        assert!(lp(1.0, 1.0, 2000.0) < b4 - 3.0);
        assert!(lp(2.0, 1.0, 2000.0) > b4 + 3.0);
        // Passband stays near 0 dB for all three.
        for d in [0.0, 1.0, 2.0] {
            assert!(lp(d, 1.0, 200.0).abs() < 1.0, "design {d}");
        }
        let band = |mode: f64, f: f64| {
            level(
                "altaudio.precision_filter",
                &[
                    ("mode", mode),
                    ("order", 1.0),
                    ("freq", 300.0),
                    ("freq2", 3000.0),
                ],
                f,
            )
        };
        assert!(
            band(2.0, 1000.0).abs() < 1.0 && band(2.0, 50.0) < -30.0 && band(2.0, 15_000.0) < -30.0
        );
        assert!(band(3.0, 1000.0) < -20.0 && band(3.0, 50.0).abs() < 1.0);
    }

    #[test]
    fn curve_eq_follows_its_points() {
        let mut p = vec![];
        let keys = [
            "p1_gain", "p2_gain", "p3_gain", "p4_gain", "p5_gain", "p6_gain", "p7_gain", "p8_gain",
        ];
        for k in keys {
            p.push((k, 0.0));
        }
        p[4] = ("p5_gain", -12.0); // 1500 Hz point
        assert!((level("altaudio.curve_eq", &p, 1500.0) + 12.0).abs() < 1.0);
        assert!(level("altaudio.curve_eq", &p, 100.0).abs() < 0.5);
    }

    #[test]
    fn center_remover_takes_out_the_middle_and_keeps_the_bass() {
        // A centered 1 kHz tone (same on both sides) is removed.
        assert!(level("altaudio.center_remover", &[], 1000.0) < -30.0);
        // A centered 50 Hz bass stays.
        assert!(level("altaudio.center_remover", &[], 50.0) > -3.0);
    }
}
