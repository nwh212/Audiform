// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

//! Noise Print Reducer and Sound Eraser. The Noise Print Reducer learns a noise from a stretch of audio picked
//! by the person (a gap with only the hum, fan or hiss in it), then
//! turns that noise down wherever it plays, voice or not.
//!
//! Unlike the Noise Reducer, which guesses the background as it plays,
//! the print is fixed, so it works from the first word and never
//! mistakes a long held note for noise. The print is a level per band
//! (128 bands, evenly spaced in pitch), stored in the effect's own
//! settings, so it saves with the session and travels with presets.

use std::sync::OnceLock;

use super::restore::Spectral;
use super::{db_to_gain, num, EffectInfo, Params};
use crate::dsp::stft::{Mask, Stft};

/// Bands in a print.
pub const PRINT_BANDS: usize = 128;
/// The lowest band starts here; the highest ends at half the sample rate.
const LOW_HZ: f64 = 20.0;
/// Stored level of a band with nothing learned.
pub const SILENT_DB: f64 = -200.0;

/// Keys and names of the print bands, made once (the catalog needs
/// `'static` text).
fn print_keys() -> &'static [&'static str] {
    static KEYS: OnceLock<Vec<&'static str>> = OnceLock::new();
    KEYS.get_or_init(|| {
        (0..PRINT_BANDS)
            .map(|i| &*Box::leak(format!("print{i:03}").into_boxed_str()))
            .collect()
    })
}

/// The learned print's settings, hidden from the controls.
fn print_settings() -> Vec<super::ParamInfo> {
    let mut params = vec![num("printlen", "Learned", "s", 0.0, 3600.0, 0.0, 0.01).hidden()];
    params.extend(
        print_keys()
            .iter()
            .map(|k| num(k, k, "dB", SILENT_DB, 50.0, SILENT_DB, 0.01).hidden()),
    );
    params
}

pub fn eraser_info() -> EffectInfo {
    let mut params = vec![
        num("reduction", "Reduction", "dB", 0.0, 60.0, 30.0, 0.5),
        num("strength", "Strength", "", 0.5, 4.0, 2.0, 0.1),
        num("sensitivity", "Sensitivity", "%", 0.0, 100.0, 50.0, 1.0),
        num("smoothing", "Smoothing", "%", 0.0, 100.0, 40.0, 1.0),
    ];
    params.extend(print_settings());
    EffectInfo {
        id: "altaudio.sound_eraser",
        name: "Sound Eraser",
        category: "Restoration",
        description: "Learns one sound from a selection of it (a phone chime, a siren, a squeaky chair), then takes it out wherever it plays, louder or softer, leaving the rest.",
        params,
    }
}

pub fn print_info() -> EffectInfo {
    let mut params = vec![
        num("reduction", "Reduction", "dB", 0.0, 40.0, 18.0, 0.5),
        num("strength", "Strength", "", 0.5, 4.0, 1.5, 0.1),
        num("smoothing", "Smoothing", "%", 0.0, 100.0, 50.0, 1.0),
    ];
    params.extend(print_settings());
    EffectInfo {
        id: "altaudio.noise_print",
        name: "Noise Print Reducer",
        category: "Restoration",
        description: "Learns a noise from a selection with only that noise in it (Learn from Selection), then turns it down everywhere. Put it first in the chain.",
        params,
    }
}

/// The band edges for `rate`, in Hz: `PRINT_BANDS + 1` of them.
fn edges(rate: u32) -> Vec<f64> {
    let top = f64::from(rate) / 2.0;
    (0..=PRINT_BANDS)
        .map(|b| LOW_HZ * (top / LOW_HZ).powf(b as f64 / PRINT_BANDS as f64))
        .collect()
}

/// Collects the average power per bin, changing nothing.
struct Learner {
    sum: Vec<f64>,
    frames: u64,
}

/// Frames at the start whose window still holds the silence before the audio.
const PARTIAL: u64 = 3;

impl Mask for Learner {
    fn mask(&mut self, power: &[f32], gains: &mut [f32]) {
        self.frames += 1;
        if self.frames > PARTIAL {
            for (s, p) in self.sum.iter_mut().zip(power) {
                *s += f64::from(*p);
            }
        }
        gains.fill(1.0);
    }
    fn reset(&mut self) {
        self.sum.fill(0.0);
        self.frames = 0;
    }
}

fn size(rate: u32) -> usize {
    Stft::<Learner>::size_for(40.0, rate)
}

/// Learns a print from `audio` (stereo interleaved at `rate`): one
/// level per band in dB, `PRINT_BANDS` of them. None when the audio is
/// too short to learn from (about a tenth of a second at least).
pub fn learn(audio: &[f32], rate: u32) -> Option<Vec<f64>> {
    let n = size(rate);
    let bins = n / 2 + 1;
    let mut stft = Stft::new(
        n,
        Learner {
            sum: vec![0.0; bins],
            frames: 0,
        },
    );
    let mut buf = audio.to_vec();
    stft.process(&mut buf);
    let l = &stft.mask;
    if l.frames < PARTIAL + 2 {
        return None;
    }
    let count = (l.frames - PARTIAL) as f64;
    let bin_hz = f64::from(rate) / n as f64;
    let e = edges(rate);
    let mean = |k: usize| l.sum[k] / count / n as f64;
    Some(
        (0..PRINT_BANDS)
            .map(|b| {
                let lo = ((e[b] / bin_hz).ceil() as usize).min(bins - 1);
                let hi = ((e[b + 1] / bin_hz).floor() as usize).min(bins - 1);
                let p = if hi >= lo {
                    (lo..=hi).map(mean).sum::<f64>() / (hi - lo + 1) as f64
                } else {
                    // A band narrower than a bin: the bin nearest its middle.
                    mean((((e[b] * e[b + 1]).sqrt() / bin_hz).round() as usize).min(bins - 1))
                };
                (10.0 * p.max(1e-30).log10()).clamp(SILENT_DB, 50.0)
            })
            .collect(),
    )
}

/// Turns each bin down by how far it sits above the learned print.
pub struct PrintMask {
    /// Noise power per bin, scaled like the frame power.
    noise: Vec<f32>,
    floor: f32,
    strength: f32,
    smooth: f32,
    last: Vec<f32>,
    /// Nothing learned: the audio passes untouched.
    empty: bool,
}

impl Mask for PrintMask {
    fn mask(&mut self, power: &[f32], gains: &mut [f32]) {
        if self.empty {
            gains.fill(1.0);
            return;
        }
        let last = power.len() - 1;
        for k in 0..power.len() {
            let lo = k.saturating_sub(1);
            let hi = (k + 1).min(last);
            let p = (power[lo] + power[k] + power[hi]) / 3.0 + 1e-20;
            let want = (1.0 - self.strength * self.noise[k] / p)
                .max(0.0)
                .sqrt()
                .max(self.floor);
            // Open fast, close gently, as the Noise Reducer does.
            let g = if want > self.last[k] {
                want
            } else {
                want + (self.last[k] - want) * self.smooth
            };
            self.last[k] = g;
            gains[k] = g;
        }
    }
    fn reset(&mut self) {
        self.last.fill(1.0);
    }
}

/// The learned print spread over the bins of an `n`-point frame, scaled
/// like the frame power, and whether nothing was learned.
fn print_bins(p: &Params, rate: u32, n: usize) -> (Vec<f32>, bool) {
    let bins = n / 2 + 1;
    let levels: Vec<f64> = print_keys().iter().map(|k| p.get(k)).collect();
    let empty = p.get("printlen") <= 0.0 || levels.iter().all(|v| *v <= SILENT_DB);
    // Band centres in log frequency; each bin reads between its two nearest.
    let e = edges(rate);
    let centres: Vec<f64> = (0..PRINT_BANDS)
        .map(|b| (e[b] * e[b + 1]).sqrt().ln())
        .collect();
    let bin_hz = f64::from(rate) / n as f64;
    let noise = (0..bins)
        .map(|k| {
            let f = (k as f64 * bin_hz).max(LOW_HZ).ln();
            let j = centres.partition_point(|c| *c <= f);
            let db = if j == 0 {
                levels[0]
            } else if j >= PRINT_BANDS {
                levels[PRINT_BANDS - 1]
            } else {
                let t = (f - centres[j - 1]) / (centres[j] - centres[j - 1]);
                levels[j - 1] + (levels[j] - levels[j - 1]) * t
            };
            (10f64.powf(db / 10.0) * n as f64) as f32
        })
        .collect();
    (noise, empty)
}

pub fn print_reducer(p: &Params, rate: u32) -> Spectral<PrintMask> {
    let n = size(rate);
    let bins = n / 2 + 1;
    let (noise, empty) = print_bins(p, rate, n);
    let mask = PrintMask {
        noise,
        floor: db_to_gain(-p.get("reduction")),
        strength: p.get("strength") as f32,
        smooth: (p.get("smoothing") / 100.0 * 0.9) as f32,
        last: vec![1.0; bins],
        empty,
    };
    Spectral(Stft::new(n, mask))
}

/// Takes out a learned sound at whatever level it plays. Works band by
/// band: each frame, how loud the sound is (as a multiple of the print)
/// is read from the bands where the sound is strong, taking one of the
/// lower readings since other sound only adds to a band; then each band
/// loses that much of the print.
pub struct EraserMask {
    /// The print per band, scaled like the frame power.
    print: Vec<f32>,
    /// The band each bin belongs to (None below the lowest band).
    band_of: Vec<Option<usize>>,
    /// Bins in each band.
    counts: Vec<u32>,
    /// Bands where the sound is strong.
    strong: Vec<usize>,
    share: f32,
    floor: f32,
    strength: f32,
    smooth: f32,
    bands: Vec<f32>,
    ratios: Vec<f32>,
    last: Vec<f32>,
    empty: bool,
}

impl Mask for EraserMask {
    fn mask(&mut self, power: &[f32], gains: &mut [f32]) {
        if self.empty || self.strong.is_empty() {
            gains.fill(1.0);
            return;
        }
        self.bands.fill(0.0);
        for (p, b) in power.iter().zip(&self.band_of) {
            if let Some(b) = b {
                self.bands[*b] += *p;
            }
        }
        for (v, c) in self.bands.iter_mut().zip(&self.counts) {
            *v /= (*c).max(1) as f32;
        }
        self.ratios.clear();
        self.ratios.extend(
            self.strong
                .iter()
                .map(|&b| self.bands[b] / self.print[b].max(1e-30)),
        );
        let len = self.ratios.len();
        let at = ((len - 1) as f32 * self.share) as usize;
        self.ratios.sort_unstable_by(f32::total_cmp);
        let low = self.ratios[at];
        let mid = self.ratios[(len - 1) / 2].max(1e-30);
        // When the sound is there, it shapes every strong band alike, so
        // the low and middle readings agree. Other sound (a voice) gives
        // scattered readings: then little is taken out.
        let agree = (low / mid).clamp(0.0, 1.0);
        let scale = low * agree * agree;
        for (k, g) in gains.iter_mut().enumerate() {
            let want = match self.band_of[k] {
                Some(b) => (1.0 - self.strength * scale * self.print[b] / (self.bands[b] + 1e-20))
                    .max(0.0)
                    .sqrt()
                    .max(self.floor),
                None => 1.0,
            };
            let v = if want > self.last[k] {
                want
            } else {
                want + (self.last[k] - want) * self.smooth
            };
            self.last[k] = v;
            *g = v;
        }
    }
    fn reset(&mut self) {
        self.last.fill(1.0);
    }
}

pub fn sound_eraser(p: &Params, rate: u32) -> Spectral<EraserMask> {
    let n = size(rate);
    let bins = n / 2 + 1;
    let levels: Vec<f64> = print_keys().iter().map(|k| p.get(k)).collect();
    let empty = p.get("printlen") <= 0.0 || levels.iter().all(|v| *v <= SILENT_DB);
    let e = edges(rate);
    let bin_hz = f64::from(rate) / n as f64;
    let band_of: Vec<Option<usize>> = (0..bins)
        .map(|k| {
            let f = k as f64 * bin_hz;
            (f >= LOW_HZ).then(|| {
                e.partition_point(|x| *x <= f)
                    .saturating_sub(1)
                    .min(PRINT_BANDS - 1)
            })
        })
        .collect();
    let mut counts = vec![0u32; PRINT_BANDS];
    for b in band_of.iter().flatten() {
        counts[*b] += 1;
    }
    let print: Vec<f32> = levels
        .iter()
        .map(|db| (10f64.powf(db / 10.0) * n as f64) as f32)
        .collect();
    // Strong bands: within 12 dB of the sound's loudest, and wide enough to hold a bin.
    let top = print
        .iter()
        .zip(&counts)
        .filter(|(_, c)| **c > 0)
        .fold(0f32, |m, (v, _)| m.max(*v));
    let strong: Vec<usize> = (0..PRINT_BANDS)
        .filter(|&b| counts[b] > 0 && print[b] > top * 0.063)
        .collect();
    let mask = EraserMask {
        print,
        band_of,
        counts,
        ratios: Vec::with_capacity(strong.len()),
        strong,
        share: (0.1 + p.get("sensitivity") / 100.0 * 0.5) as f32,
        floor: db_to_gain(-p.get("reduction")),
        strength: p.get("strength") as f32,
        smooth: (p.get("smoothing") / 100.0 * 0.9) as f32,
        bands: vec![0.0; PRINT_BANDS],
        last: vec![1.0; bins],
        empty,
    };
    Spectral(Stft::new(n, mask))
}

/// The print as effect settings: `printlen` plus one value per band.
pub fn print_params(bands: &[f64], seconds: f64) -> Vec<(&'static str, f64)> {
    let mut out = vec![("printlen", seconds)];
    out.extend(print_keys().iter().copied().zip(bands.iter().copied()));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::effects::build;
    use crate::session::model::EffectSlot;

    /// Steady noise, from a fixed seed.
    fn noise(frames: usize, level: f32, seed: &mut u32) -> Vec<f32> {
        (0..frames * 2)
            .map(|_| {
                *seed ^= *seed << 13;
                *seed ^= *seed >> 17;
                *seed ^= *seed << 5;
                (*seed as f32 / u32::MAX as f32 * 2.0 - 1.0) * level
            })
            .collect()
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
    }

    #[test]
    fn learns_noise_and_turns_it_down_but_keeps_a_tone() {
        let rate = 48_000;
        let mut seed = 7;
        let print = learn(&noise(rate as usize, 0.05, &mut seed), rate).expect("a print");
        assert_eq!(print.len(), PRINT_BANDS);
        // White noise: about the same level in every band.
        let spread = print.iter().fold(f64::MIN, |m, v| m.max(*v))
            - print.iter().fold(f64::MAX, |m, v| m.min(*v));
        assert!(spread < 6.0, "spread {spread} dB");

        let mut slot = EffectSlot {
            id: crate::session::model::new_id(),
            effect: "altaudio.noise_print".into(),
            enabled: true,
            params: Default::default(),
            preset: None,
            sidechain: None,
            plugin_state: None,
            position: None,
        };
        for (k, v) in print_params(&print, 1.0) {
            slot.params.insert(k.to_string(), v);
        }
        let mut fx = build(&slot, rate).unwrap();
        let lat = fx.latency();
        // Fresh noise of the same kind, then the same noise with a tone over it.
        let mut quiet = noise(rate as usize, 0.05, &mut seed);
        fx.process(&mut quiet);
        let before = rms(&noise(rate as usize, 0.05, &mut seed));
        let after = rms(&quiet[(lat + rate as usize / 4) * 2..]);
        let cut = 20.0 * (before / after).log10();
        assert!(cut > 10.0, "noise down only {cut} dB");

        fx.reset();
        let mut mixed = noise(rate as usize, 0.05, &mut seed);
        for (i, f) in mixed.as_chunks_mut::<2>().0.iter_mut().enumerate() {
            let t = (std::f32::consts::TAU * 440.0 * i as f32 / rate as f32).sin() * 0.5;
            f[0] += t;
            f[1] += t;
        }
        fx.process(&mut mixed);
        let tone_left = rms(&mixed[(lat + rate as usize / 4) * 2..]);
        assert!(
            (tone_left - 0.5 / 2f32.sqrt()).abs() < 0.05,
            "tone kept: {tone_left}"
        );
    }

    #[test]
    fn erases_a_learned_chime_at_any_level_and_keeps_others() {
        let rate = 48_000;
        let tone = |f: f32, a: f32, frames: usize| -> Vec<f32> {
            (0..frames)
                .flat_map(|i| {
                    let v = (std::f32::consts::TAU * f * i as f32 / rate as f32).sin() * a;
                    [v, v]
                })
                .collect()
        };
        // A two-tone chime.
        let chime = |a: f32, frames: usize| {
            let x = tone(1200.0, a, frames);
            let y = tone(3100.0, a * 0.7, frames);
            x.iter().zip(&y).map(|(p, q)| p + q).collect::<Vec<f32>>()
        };
        let print = learn(&chime(0.3, rate as usize / 2), rate).unwrap();
        let mut slot = EffectSlot {
            id: crate::session::model::new_id(),
            effect: "altaudio.sound_eraser".into(),
            enabled: true,
            params: Default::default(),
            preset: None,
            sidechain: None,
            plugin_state: None,
            position: None,
        };
        for (k, v) in print_params(&print, 0.5) {
            slot.params.insert(k.to_string(), v);
        }
        let mut fx = build(&slot, rate).unwrap();
        let lat = fx.latency();
        let settle = (lat + rate as usize / 4) * 2;
        // The chime at a third of the level it was learned at, over a 300 Hz tone.
        let voice = tone(300.0, 0.3, rate as usize);
        let soft = chime(0.1, rate as usize);
        let mut both: Vec<f32> = voice.iter().zip(&soft).map(|(a, b)| a + b).collect();
        fx.process(&mut both);
        // What is left minus the tone (delayed by the latency) is the chime's remains.
        let left: Vec<f32> = both[settle..]
            .iter()
            .zip(&voice[settle - lat * 2..])
            .map(|(o, v)| o - v)
            .collect();
        let cut = 20.0 * (rms(&soft[settle..]) / rms(&left)).log10();
        assert!(cut > 12.0, "chime down only {cut} dB");
        // And the other sound is kept.
        fx.reset();
        let mut alone = voice.clone();
        fx.process(&mut alone);
        let kept = rms(&alone[settle..]) / rms(&voice[settle..]);
        assert!(kept > 0.9, "tone kept {kept}");
        // Broadband sound over the chime's bands, with no chime: mostly kept.
        fx.reset();
        let mut seed = 5;
        let hiss = noise(rate as usize, 0.1, &mut seed);
        let mut h = hiss.clone();
        fx.process(&mut h);
        let kept = rms(&h[settle..]) / rms(&hiss[settle..]);
        assert!(kept > 0.8, "broadband kept {kept}");
    }

    #[test]
    fn with_nothing_learned_it_changes_nothing() {
        let slot = EffectSlot {
            id: crate::session::model::new_id(),
            effect: "altaudio.noise_print".into(),
            enabled: true,
            params: Default::default(),
            preset: None,
            sidechain: None,
            plugin_state: None,
            position: None,
        };
        let mut fx = build(&slot, 48_000).unwrap();
        let lat = fx.latency();
        let mut seed = 3;
        let src = noise(20_000, 0.2, &mut seed);
        let mut out = src.clone();
        fx.process(&mut out);
        for i in (lat * 2..out.len()).step_by(97) {
            assert!((out[i] - src[i - lat * 2]).abs() < 1e-4);
        }
        assert!(learn(&src[..200], 48_000).is_none(), "too short");
    }
}
