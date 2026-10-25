// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

//! Shaped Noise Reducer. Learns a noise print from a selection, like the
//! Noise Print Reducer, then works as a gate on every frequency: a bin
//! louder than its threshold passes, a quieter one is turned down by up
//! to Reduction. The threshold follows the learned noise, moved up or
//! down by Threshold and by a curve of up to five shaping bands (a bell,
//! low shelf or high shelf each), so the person can remove more in one
//! range (rumble, hiss) and less in another (the voice). Knee softens
//! the step at the threshold; Attack and Release set how fast each bin
//! opens and closes. Adaptive lets the print follow a noise that drifts,
//! learning from frames that sound like noise alone. Output can be the
//! cleaned audio, or only what is being removed, to hear what is lost.

use super::print::{print_bins, print_settings, size};
use super::restore::Spectral;
use super::{choice, num, toggle, EffectInfo, ParamInfo, Params};
use crate::dsp::stft::{Mask, Stft};

pub const SHAPED_NOISE: &str = "altaudio.shaped_noise";
/// Shaping bands.
pub const BANDS: usize = 5;
const GROUPS: [&str; BANDS] = ["Shape 1", "Shape 2", "Shape 3", "Shape 4", "Shape 5"];
const ON_KEYS: [&str; BANDS] = ["s1_on", "s2_on", "s3_on", "s4_on", "s5_on"];
const KIND_KEYS: [&str; BANDS] = ["s1_kind", "s2_kind", "s3_kind", "s4_kind", "s5_kind"];
const FREQ_KEYS: [&str; BANDS] = ["s1_freq", "s2_freq", "s3_freq", "s4_freq", "s5_freq"];
const GAIN_KEYS: [&str; BANDS] = ["s1_gain", "s2_gain", "s3_gain", "s4_gain", "s5_gain"];
const Q_KEYS: [&str; BANDS] = ["s1_q", "s2_q", "s3_q", "s4_q", "s5_q"];
const KINDS: &[&str] = &["Bell", "Low shelf", "High shelf"];
/// Where the bands start: low shelf, three bells, high shelf.
const DEFAULT_FREQ: [f64; BANDS] = [80.0, 250.0, 1000.0, 4000.0, 10_000.0];
const DEFAULT_KIND: [usize; BANDS] = [1, 0, 0, 0, 2];

pub fn shaped_info() -> EffectInfo {
    let mut params: Vec<ParamInfo> = vec![
        num("threshold", "Threshold", "dB", -20.0, 30.0, 6.0, 0.1),
        num("reduction", "Reduction", "dB", 0.0, 60.0, 15.0, 0.5),
        num("knee", "Knee", "dB", 0.0, 24.0, 6.0, 0.5),
        num("attack", "Attack", "ms", 1.0, 200.0, 10.0, 1.0).log(),
        num("release", "Release", "ms", 10.0, 2000.0, 150.0, 1.0).log(),
        toggle("adaptive", "Adaptive", false),
        choice("monitor", "Output", &["Cleaned audio", "Removed noise"], 0),
    ];
    for b in 0..BANDS {
        let g = GROUPS[b];
        params.push(toggle(ON_KEYS[b], "On", true).group(g));
        params.push(choice(KIND_KEYS[b], "Shape", KINDS, DEFAULT_KIND[b]).group(g));
        params.push(
            num(
                FREQ_KEYS[b],
                "Frequency",
                "Hz",
                20.0,
                20_000.0,
                DEFAULT_FREQ[b],
                1.0,
            )
            .log()
            .group(g),
        );
        params.push(num(GAIN_KEYS[b], "Gain", "dB", -24.0, 24.0, 0.0, 0.1).group(g));
        params.push(
            num(Q_KEYS[b], "Width (Q)", "", 0.1, 10.0, 1.0, 0.01)
                .log()
                .group(g),
        );
    }
    params.extend(print_settings());
    EffectInfo {
        id: SHAPED_NOISE,
        name: "Shaped Noise Reducer",
        category: "Restoration",
        description: "Learns a noise from a selection with only that noise in it, then turns down every frequency that sits below its threshold. Shape the threshold with five bands to remove more noise in one range and less in another.",
        params,
    }
}

/// A shaping band's pull on the threshold at `hz`, in dB.
fn band_db(kind: usize, fc: f64, gain: f64, q: f64, hz: f64) -> f64 {
    let x = (hz.max(1.0) / fc.max(1.0)).log2();
    match kind {
        // Width at half the gain is the band's width in octaves for that Q.
        0 => {
            let oct = (2.0 / std::f64::consts::LN_2) * (1.0 / (2.0 * q.max(1e-3))).asinh();
            gain * (-std::f64::consts::LN_2 * (2.0 * x / oct.max(1e-3)).powi(2)).exp()
        }
        // Shelves: halfway at the frequency, steeper with a higher Q.
        1 => gain / (1.0 + (4.0 * q * x).exp()),
        _ => gain / (1.0 + (-4.0 * q * x).exp()),
    }
}

/// The shaping curve at `hz`: every band that is on, added up, in dB.
pub fn shape_db(p: &Params, hz: f64) -> f64 {
    (0..BANDS)
        .filter(|&b| p.get(ON_KEYS[b]) >= 0.5)
        .map(|b| {
            band_db(
                p.get(KIND_KEYS[b]).round() as usize,
                p.get(FREQ_KEYS[b]),
                p.get(GAIN_KEYS[b]),
                p.get(Q_KEYS[b]),
                hz,
            )
        })
        .sum()
}

/// The threshold at `hz` above the learned noise, in dB: Threshold plus the shaping curve.
pub fn threshold_db(p: &Params, hz: f64) -> f64 {
    p.get("threshold") + shape_db(p, hz)
}

pub struct ShapedMask {
    /// Learned noise per bin, scaled like the frame power.
    noise: Vec<f32>,
    /// Threshold over the noise per bin, as a power ratio.
    over: Vec<f32>,
    /// Reduction floor as a power ratio in dB terms: gain in dB at the bottom.
    floor_db: f32,
    knee_db: f32,
    attack: f32,
    release: f32,
    adapt: Option<f32>,
    removed: bool,
    last: Vec<f32>,
    empty: bool,
}

impl ShapedMask {
    /// The gain for a bin `r_db` above (or below) its threshold.
    fn want(&self, r_db: f32) -> f32 {
        let half = self.knee_db / 2.0;
        let db = if r_db >= half {
            0.0
        } else if r_db <= -half {
            self.floor_db
        } else {
            // Through the knee, straight in dB from no cut to the full cut.
            self.floor_db * (half - r_db) / self.knee_db.max(1e-6)
        };
        10f32.powf(db / 20.0)
    }
}

impl Mask for ShapedMask {
    fn mask(&mut self, power: &[f32], gains: &mut [f32]) {
        if self.empty {
            gains.fill(1.0);
            return;
        }
        if let Some(a) = self.adapt {
            // A frame no louder than about twice the noise is taken as noise alone.
            let (p, n): (f32, f32) = power
                .iter()
                .zip(&self.noise)
                .fold((0.0, 0.0), |(p, n), (x, y)| (p + x, n + y));
            if p < n * 2.0 {
                for (n, x) in self.noise.iter_mut().zip(power) {
                    *n += (*x - *n) * a;
                }
            }
        }
        let last = power.len() - 1;
        for k in 0..power.len() {
            let lo = k.saturating_sub(1);
            let hi = (k + 1).min(last);
            let p = (power[lo] + power[k] + power[hi]) / 3.0 + 1e-20;
            let thr = self.noise[k] * self.over[k] + 1e-20;
            let want = self.want(10.0 * (p / thr).log10());
            let prev = self.last[k];
            let g = if want > prev {
                want + (prev - want) * self.attack
            } else {
                want + (prev - want) * self.release
            };
            self.last[k] = g;
            gains[k] = if self.removed { 1.0 - g } else { g };
        }
    }
    fn reset(&mut self) {
        self.last.fill(1.0);
    }
}

pub fn shaped_reducer(p: &Params, rate: u32) -> Spectral<ShapedMask> {
    let n = size(rate);
    let bins = n / 2 + 1;
    let (noise, empty) = print_bins(p, rate, n);
    let bin_hz = f64::from(rate) / n as f64;
    let over = (0..bins)
        .map(|k| 10f64.powf(threshold_db(p, (k as f64 * bin_hz).max(20.0)) / 10.0) as f32)
        .collect();
    let fps = f64::from(rate) / (n / 4) as f64;
    // Per-frame smoothing for a time in ms: the share of the old gain kept each frame.
    let keep = |ms: f64| (-1000.0 / (ms.max(0.1) * fps)).exp() as f32;
    let mask = ShapedMask {
        noise,
        over,
        floor_db: -(p.get("reduction") as f32),
        knee_db: p.get("knee") as f32,
        attack: keep(p.get("attack")),
        release: keep(p.get("release")),
        // About two seconds to follow a drifting noise.
        adapt: (p.get("adaptive") >= 0.5).then(|| (1.0 - (-1.0 / (2.0 * fps)).exp()) as f32),
        removed: p.get("monitor").round() as i64 == 1,
        last: vec![1.0; bins],
        empty,
    };
    Spectral(Stft::new(n, mask))
}

/// For the graph: the learned noise and the threshold, in dB per
/// frequency, at `points` frequencies from 20 Hz to 20 kHz, on the scale
/// of a `scope`-point spectrum from `engine::scope::spectrum` (so a steady
/// noise reads the same on both). The print is empty when nothing is
/// learned yet; the threshold is then drawn from -60 dB.
pub fn curves(
    p: &Params,
    rate: u32,
    points: usize,
    scope: usize,
) -> (Vec<[f64; 2]>, Vec<[f64; 2]>) {
    // A stereo noise of power P per channel reads P / 2 per bin here, and
    // 6 (P / 2) / N on an N-point spectrum view (Hann window, mono mix).
    let offset = 10.0 * (6.0 / scope.max(64) as f64).log10();
    let n = size(rate);
    let (noise, empty) = print_bins(p, rate, n);
    let bin_hz = f64::from(rate) / n as f64;
    let mut print = Vec::new();
    let mut thr = Vec::new();
    for i in 0..points.max(2) {
        let f = 20.0 * 1000f64.powf(i as f64 / (points.max(2) - 1) as f64);
        let k = ((f / bin_hz).round() as usize).min(noise.len() - 1);
        // Back to dB per bin, as the learned levels are stored.
        let db = if empty {
            None
        } else {
            Some(10.0 * (f64::from(noise[k]) / n as f64).max(1e-30).log10() + offset)
        };
        if let Some(d) = db {
            print.push([f, d]);
        }
        thr.push([f, db.unwrap_or(-60.0) + threshold_db(p, f)]);
    }
    (print, thr)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::effects::{build, info, print::print_params};
    use crate::session::model::EffectSlot;
    use std::collections::BTreeMap;

    fn params(extra: &[(&str, f64)]) -> Params {
        let saved: BTreeMap<String, f64> = extra.iter().map(|(k, v)| (k.to_string(), *v)).collect();
        Params::resolve(&info(SHAPED_NOISE).unwrap(), &saved)
    }

    #[test]
    fn the_print_lines_up_with_the_spectrum_view() {
        let rate = 48_000;
        let mut seed = 3u32;
        let audio: Vec<f32> = (0..rate as usize * 2)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                (seed as f32 / u32::MAX as f32 * 2.0 - 1.0) * 0.1
            })
            .collect();
        let learned = crate::dsp::effects::print::learn(&audio, rate).unwrap();
        let saved: BTreeMap<String, f64> = print_params(&learned, 1.0)
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        let p = Params::resolve(&info(SHAPED_NOISE).unwrap(), &saved);
        let (print, _) = curves(&p, rate, 50, 4096);
        let frames: Vec<[f32; 2]> = audio.as_chunks::<2>().0[..4096].to_vec();
        let scope = crate::engine::scope::spectrum(&frames, rate, 50);
        // Middle of the range: the spectrum view takes the loudest bin per point, so it reads a little higher.
        let (a, b) = (print[25][1], f64::from(scope[25]));
        assert!(
            (b - a) > -3.0 && (b - a) < 12.0,
            "print {a:.1} dB, spectrum {b:.1} dB"
        );
    }

    #[test]
    fn the_shape_moves_the_threshold_where_it_is_set() {
        let p = params(&[("s3_gain", 12.0), ("threshold", 0.0)]);
        assert!((threshold_db(&p, 1000.0) - 12.0).abs() < 0.01);
        assert!(threshold_db(&p, 100.0).abs() < 0.5);
        // A low shelf pulls the lows and leaves the highs.
        let s = params(&[("s1_gain", -10.0), ("threshold", 0.0)]);
        assert!(threshold_db(&s, 20.0) < -9.0);
        assert!(threshold_db(&s, 2000.0).abs() < 0.1);
    }

    #[test]
    fn removes_learned_noise_and_keeps_a_loud_tone() {
        let rate = 48_000;
        let mut seed = 7u32;
        let mut noise = |frames: usize, level: f32| -> Vec<f32> {
            (0..frames * 2)
                .map(|_| {
                    seed ^= seed << 13;
                    seed ^= seed >> 17;
                    seed ^= seed << 5;
                    (seed as f32 / u32::MAX as f32 * 2.0 - 1.0) * level
                })
                .collect()
        };
        let learned = crate::dsp::effects::print::learn(&noise(rate as usize, 0.05), rate).unwrap();
        let mut saved: BTreeMap<String, f64> = print_params(&learned, 1.0)
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        saved.insert("reduction".into(), 30.0);
        let slot = |monitor: f64| EffectSlot {
            id: crate::session::model::new_id(),
            effect: SHAPED_NOISE.into(),
            enabled: true,
            params: {
                let mut s = saved.clone();
                s.insert("monitor".into(), monitor);
                s
            },
            preset: None,
            sidechain: None,
            plugin_state: None,
            position: None,
        };
        let rms = |x: &[f32]| (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt();
        let frames = rate as usize * 2;
        let input = noise(frames, 0.05);
        let mut fx = build(&slot(0.0), rate).unwrap();
        let mut out = input.clone();
        fx.process(&mut out);
        let tail = frames / 2 * 2..;
        assert!(
            rms(&out[tail.clone()]) < rms(&input[tail.clone()]) * 0.2,
            "noise should drop by more than 14 dB"
        );

        // A tone well above the noise passes.
        let tone: Vec<f32> = (0..frames)
            .flat_map(|i| {
                let v = (std::f32::consts::TAU * 1000.0 * i as f32 / rate as f32).sin() * 0.5;
                [v, v]
            })
            .zip(input.iter())
            .map(|(t, n)| t + n)
            .collect();
        let mut fx = build(&slot(0.0), rate).unwrap();
        let mut out = tone.clone();
        fx.process(&mut out);
        let lat = fx.latency() * 2;
        let a = rms(&out[lat + rate as usize..lat + rate as usize + 20_000]);
        assert!(a > 0.3, "tone kept: {a}");

        // Listening to what is removed gives the noise back, not silence.
        let mut fx = build(&slot(1.0), rate).unwrap();
        let mut out = input.clone();
        fx.process(&mut out);
        assert!(rms(&out[tail.clone()]) > rms(&input[tail]) * 0.5);
    }
}
