// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Filter and EQ effects: Parametric EQ and Hum Cut.

use super::{choice, num, toggle, EffectInfo, Params, Processor};
use crate::dsp::biquad::{butterworth_qs, Coeffs, Shape, StereoBiquad};

const SLOPES: &[&str] = &["12 dB/oct", "24 dB/oct", "48 dB/oct"];

pub fn parametric_info() -> EffectInfo {
    let band = |n: &'static str, f: f64| {
        [
            num(
                match n {
                    "1" => "b1_freq",
                    "2" => "b2_freq",
                    _ => "b3_freq",
                },
                "Frequency",
                "Hz",
                20.0,
                20_000.0,
                f,
                1.0,
            )
            .log(),
            num(
                match n {
                    "1" => "b1_gain",
                    "2" => "b2_gain",
                    _ => "b3_gain",
                },
                "Gain",
                "dB",
                -24.0,
                24.0,
                0.0,
                0.1,
            ),
            num(
                match n {
                    "1" => "b1_q",
                    "2" => "b2_q",
                    _ => "b3_q",
                },
                "Width (Q)",
                "",
                0.1,
                12.0,
                1.0,
                0.01,
            )
            .log(),
        ]
    };
    let mut params = vec![
        toggle("hp_on", "On", false).group("High-pass"),
        num("hp_freq", "Frequency", "Hz", 10.0, 2_000.0, 80.0, 1.0)
            .log()
            .group("High-pass"),
        choice("hp_slope", "Slope", SLOPES, 1).group("High-pass"),
        num("ls_freq", "Frequency", "Hz", 20.0, 2_000.0, 120.0, 1.0)
            .log()
            .group("Low shelf"),
        num("ls_gain", "Gain", "dB", -24.0, 24.0, 0.0, 0.1).group("Low shelf"),
    ];
    for (n, f, g) in [
        ("1", 250.0, "Band 1"),
        ("2", 1_000.0, "Band 2"),
        ("3", 4_000.0, "Band 3"),
    ] {
        params.extend(band(n, f).map(|p| p.group(g)));
    }
    params.extend([
        num(
            "hs_freq",
            "Frequency",
            "Hz",
            1_000.0,
            20_000.0,
            8_000.0,
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
            "High-pass and low-pass filters, two shelves and three bands to shape the tone.",
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

/// Up to 17 filters in series. Bands at 0 dB are left out.
pub struct ParametricEq {
    filters: Vec<StereoBiquad>,
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
        if gain("ls_gain") {
            c.push(Coeffs::new(
                Shape::LowShelf,
                rate,
                p.get("ls_freq"),
                1.0,
                p.get("ls_gain"),
            ));
        }
        for b in ["b1", "b2", "b3"] {
            let g = format!("{b}_gain");
            if gain(&g) {
                c.push(Coeffs::new(
                    Shape::Peak,
                    rate,
                    p.get(&format!("{b}_freq")),
                    p.get(&format!("{b}_q")),
                    p.get(&g),
                ));
            }
        }
        if gain("hs_gain") {
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
        }
    }
}

impl Processor for ParametricEq {
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
        assert!((level(eq, &[("b2_gain", 6.0)], 1000.0) - 6.0).abs() < 0.3);
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
