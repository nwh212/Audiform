// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Filter and EQ effects: Parametric EQ and Hum Cut.

use super::{choice, num, toggle, EffectInfo, Params, Processor};
use crate::dsp::biquad::{butterworth_qs, Coeffs, Shape, StereoBiquad};

const SLOPES: &[&str] = &["12 dB/oct", "24 dB/oct", "48 dB/oct"];

/// The peaking bands: key prefix, default frequency.
const BANDS: [(&str, f64); 5] = [
    ("b1", 50.0),
    ("b2", 200.0),
    ("b3", 800.0),
    ("b4", 3_200.0),
    ("b5", 12_800.0),
];
/// Each band's group and keys: on, frequency, gain, width.
const BAND_KEYS: [[&str; 5]; 5] = [
    ["Band 1", "b1_on", "b1_freq", "b1_gain", "b1_q"],
    ["Band 2", "b2_on", "b2_freq", "b2_gain", "b2_q"],
    ["Band 3", "b3_on", "b3_freq", "b3_gain", "b3_q"],
    ["Band 4", "b4_on", "b4_freq", "b4_gain", "b4_q"],
    ["Band 5", "b5_on", "b5_freq", "b5_gain", "b5_q"],
];

pub fn parametric_info() -> EffectInfo {
    let mut params = vec![
        num("gain", "Gain", "dB", -80.0, 40.0, 0.0, 0.1),
        toggle("hp_on", "On", false).group("High-pass"),
        num("hp_freq", "Frequency", "Hz", 10.0, 2_000.0, 40.0, 1.0)
            .log()
            .group("High-pass"),
        choice("hp_slope", "Slope", SLOPES, 1).group("High-pass"),
        toggle("ls_on", "On", true).group("Low shelf"),
        num("ls_freq", "Frequency", "Hz", 20.0, 2_000.0, 40.0, 1.0)
            .log()
            .group("Low shelf"),
        num("ls_gain", "Gain", "dB", -24.0, 24.0, 0.0, 0.1).group("Low shelf"),
    ];
    for ([g, on, freq, gain, q], (_, f)) in BAND_KEYS.into_iter().zip(BANDS) {
        params.extend([
            toggle(on, "On", true).group(g),
            num(freq, "Frequency", "Hz", 20.0, 20_000.0, f, 1.0)
                .log()
                .group(g),
            num(gain, "Gain", "dB", -24.0, 24.0, 0.0, 0.1).group(g),
            num(q, "Width (Q)", "", 0.1, 100.0, 2.0, 0.01)
                .log()
                .group(g),
        ]);
    }
    params.extend([
        toggle("hs_on", "On", true).group("High shelf"),
        num(
            "hs_freq",
            "Frequency",
            "Hz",
            1_000.0,
            20_000.0,
            18_000.0,
            1.0,
        )
        .log()
        .group("High shelf"),
        num("hs_gain", "Gain", "dB", -24.0, 24.0, 0.0, 0.1).group("High shelf"),
        toggle("lp_on", "On", false).group("Low-pass"),
        num(
            "lp_freq",
            "Frequency",
            "Hz",
            1_000.0,
            22_000.0,
            18_000.0,
            1.0,
        )
        .log()
        .group("Low-pass"),
        choice("lp_slope", "Slope", SLOPES, 1).group("Low-pass"),
    ]);
    EffectInfo {
        id: "altaudio.parametric_eq",
        name: "Parametric EQ",
        category: "Filter and EQ",
        description:
            "High-pass and low-pass filters, two shelves and five bands to shape the tone, and an overall gain.",
        params,
    }
}

pub fn hum_info() -> EffectInfo {
    EffectInfo {
        id: "altaudio.hum_cut",
        name: "Hum Cut",
        category: "Filter and EQ",
        description: "Removes electrical hum and its overtones with narrow notches.",
        params: vec![
            choice(
                "mains",
                "Mains",
                &["60 Hz (Americas)", "50 Hz (most other places)"],
                0,
            ),
            num("harmonics", "Overtones", "", 1.0, 10.0, 4.0, 1.0),
            num("q", "Narrowness", "", 2.0, 60.0, 25.0, 1.0).log(),
        ],
    }
}

/// Up to 19 filters in series, then the overall gain. Bands at 0 dB are left out.
pub struct ParametricEq {
    filters: Vec<StereoBiquad>,
    gain: f32,
}

impl ParametricEq {
    pub fn new(p: &Params, rate: u32) -> Self {
        let mut c: Vec<Coeffs> = Vec::new();
        let sections = |slope: usize| [1usize, 2, 4][slope.min(2)];
        if p.on("hp_on") {
            for q in butterworth_qs(sections(p.index("hp_slope"))) {
                c.push(Coeffs::new(Shape::HighPass, rate, p.get("hp_freq"), q, 0.0));
            }
        }
        let gain = |k: &str| p.get(k).abs() > 1e-3;
        // Band switches are on unless turned off (older settings have none).
        let on = |k: &str| p.get(k) >= 0.5;
        if on("ls_on") && gain("ls_gain") {
            c.push(Coeffs::new(
                Shape::LowShelf,
                rate,
                p.get("ls_freq"),
                1.0,
                p.get("ls_gain"),
            ));
        }
        for (b, _) in BANDS {
            let g = format!("{b}_gain");
            if on(&format!("{b}_on")) && gain(&g) {
                c.push(Coeffs::new(
                    Shape::Peak,
                    rate,
                    p.get(&format!("{b}_freq")),
                    p.get(&format!("{b}_q")),
                    p.get(&g),
                ));
            }
        }
        if on("hs_on") && gain("hs_gain") {
            c.push(Coeffs::new(
                Shape::HighShelf,
                rate,
                p.get("hs_freq"),
                1.0,
                p.get("hs_gain"),
            ));
        }
        if p.on("lp_on") {
            for q in butterworth_qs(sections(p.index("lp_slope"))) {
                c.push(Coeffs::new(Shape::LowPass, rate, p.get("lp_freq"), q, 0.0));
            }
        }
        Self {
            filters: c.into_iter().map(StereoBiquad::new).collect(),
            gain: 10f64.powf(p.get("gain").clamp(-80.0, 40.0) / 20.0) as f32,
        }
    }
}

impl Processor for ParametricEq {
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

/// Notches at the mains frequency and its overtones.
pub struct HumCut {
    filters: Vec<StereoBiquad>,
}

impl HumCut {
    pub fn new(p: &Params, rate: u32) -> Self {
        let base = if p.index("mains") == 1 { 50.0 } else { 60.0 };
        let q = p.get("q");
        let n = p.index("harmonics").max(1);
        let nyquist = f64::from(rate) * 0.45;
        Self {
            filters: (1..=n)
                .map(|k| base * k as f64)
                .filter(|f| *f < nyquist)
                .map(|f| StereoBiquad::new(Coeffs::new(Shape::Notch, rate, f, q, 0.0)))
                .collect(),
        }
    }
}

impl Processor for HumCut {
    fn process(&mut self, buf: &mut [f32]) {
        for f in &mut self.filters {
            f.process(buf);
        }
    }
    fn reset(&mut self) {
        for f in &mut self.filters {
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
        p.process(&mut b);
        let peak = b[n..].iter().fold(0f32, |m, s| m.max(s.abs()));
        20.0 * (f64::from(peak) / 0.5).log10()
    }

    #[test]
    fn eq_bands_and_filters() {
        let eq = "altaudio.parametric_eq";
        assert!(level(eq, &[], 1000.0).abs() < 0.05, "flat by default");
        assert!((level(eq, &[("b2_gain", 6.0), ("b2_freq", 1000.0)], 1000.0) - 6.0).abs() < 0.3);
        assert!(level(eq, &[("b2_gain", 6.0), ("b2_freq", 1000.0), ("b2_on", 0.0)], 1000.0).abs() < 0.05, "a band switched off");
        assert!((level(eq, &[("gain", -6.0)], 1000.0) + 6.0).abs() < 0.05, "overall gain");
        assert!(
            level(
                eq,
                &[("hp_on", 1.0), ("hp_freq", 100.0), ("hp_slope", 2.0)],
                40.0
            ) < -40.0
        );
        assert!(level(eq, &[("lp_on", 1.0), ("lp_freq", 3000.0)], 12_000.0) < -30.0);
    }

    #[test]
    fn hum_cut_removes_hum_and_keeps_voice() {
        let h = "altaudio.hum_cut";
        assert!(level(h, &[], 60.0) < -25.0);
        assert!(level(h, &[], 180.0) < -25.0);
        assert!(level(h, &[("mains", 1.0)], 100.0) < -25.0);
        assert!(level(h, &[], 1000.0).abs() < 0.3);
    }
}
