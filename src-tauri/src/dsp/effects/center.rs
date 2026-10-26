// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

//! Center Channel Extractor: finds the sound at one place in a stereo mix
//! (the center, usually the lead vocal; hard left or right; or out of
//! phase "surround" sound) and turns it up or down apart from the rest.
//! Center at its lowest takes the vocal out; Side at its lowest keeps
//! only the vocal.
//!
//! The work is done on the spectrum. For every frequency in each frame,
//! the left and right parts are compared with the target: their phase
//! difference (the target's phase angle, plus the turn a delay between
//! the channels gives at that frequency) and their level difference (the
//! target's pan). Within the discrimination limits a frequency counts as
//! the target; past them it fades out to side sound over a span the
//! crossover bleed sets. Amplitude bandwidth averages the levels over
//! neighbouring frequencies first, and spectral decay lets a frequency
//! that was the target fade out slowly rather than flicker. Only the
//! frequency range chosen takes part. The frames overlap by 75%; their
//! length is the FFT size, reported as latency.

use std::sync::Arc;

use realfft::num_complex::Complex32;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};

use super::{choice, num, EffectInfo, Params, Processor};

pub const ID: &str = "altaudio.center_extractor";

const EXTRACT: &[&str] = &["Center", "Left", "Right", "Surround", "Custom"];
const RANGES: &[&str] = &[
    "Full spectrum",
    "Bass",
    "Male voice",
    "Female voice",
    "Custom",
];
/// The frequency ranges by name: low and high (Hz).
const RANGE_HZ: [(f64, f64); 4] = [
    (20.0, 20_000.0),
    (20.0, 250.0),
    (90.0, 10_000.0),
    (160.0, 12_000.0),
];
pub const FFT_SIZES: [usize; 5] = [1024, 2048, 4096, 8192, 16384];
const FFT_NAMES: &[&str] = &["1024", "2048", "4096", "8192", "16384"];
/// Center and Side at their lowest are silent.
pub const LEVEL_FLOOR: f64 = -48.0;

pub fn center_extractor_info() -> EffectInfo {
    EffectInfo {
        id: ID,
        name: "Center Channel Extractor",
        category: "Stereo",
        description: "Turns the sound at one place in a stereo mix (the center, usually the vocal) up or down apart from the rest: take a vocal out, or keep only the vocal.",
        params: vec![
            choice("extract", "Extract", EXTRACT, 0).group("Extraction"),
            num("phase", "Phase angle", "°", -180.0, 180.0, 0.0, 1.0).group("Extraction"),
            num("pan", "Pan", "%", -100.0, 100.0, 0.0, 1.0).group("Extraction"),
            num("delay", "Delay", "ms", -10.0, 10.0, 0.0, 0.01).group("Extraction"),
            choice("range", "Frequency range", RANGES, 0).group("Extraction"),
            num("center_freq", "Center frequency", "Hz", 20.0, 20_000.0, 1_000.0, 1.0)
                .log()
                .group("Extraction"),
            num("width", "Width", "oct", 0.5, 10.0, 4.0, 0.1).group("Extraction"),
            num("bleed", "Crossover bleed", "%", 0.0, 100.0, 20.0, 1.0).group("Discrimination"),
            num("phase_disc", "Phase discrimination", "°", 1.0, 90.0, 15.0, 0.5)
                .group("Discrimination"),
            num("amp_disc", "Amplitude discrimination", "dB", 0.5, 20.0, 3.0, 0.1)
                .group("Discrimination"),
            num("amp_band", "Amplitude bandwidth", "bins", 0.0, 20.0, 2.0, 1.0)
                .group("Discrimination"),
            num("decay", "Spectral decay", "%", 0.0, 100.0, 0.0, 1.0).group("Discrimination"),
            num("center", "Center level", "dB", LEVEL_FLOOR, 12.0, 0.0, 0.1),
            num("side", "Side level", "dB", LEVEL_FLOOR, 12.0, 0.0, 0.1),
            choice("fft", "FFT size", FFT_NAMES, 2).group("Advanced"),
        ],
    }
}

/// What to look for, from the settings.
#[derive(Debug, Clone, Copy)]
struct Target {
    /// Left's phase ahead of right's (radians).
    phase: f32,
    /// Delay of the right channel against the left (seconds).
    delay: f32,
    /// Right's level over left's (dB).
    ratio: f32,
    /// Hard left or right: the phase cannot be measured, so it is not used.
    hard: bool,
}

fn target(p: &Params) -> Target {
    let (phase, pan, delay) = match p.index("extract") {
        0 => (0.0, 0.0, 0.0),
        1 => (0.0, -100.0, 0.0),
        2 => (0.0, 100.0, 0.0),
        3 => (180.0, 0.0, 0.0),
        _ => (p.get("phase"), p.get("pan"), p.get("delay")),
    };
    // Constant-power pan law: the level each side gets at that pan.
    let theta = (pan.clamp(-100.0, 100.0) / 100.0 + 1.0) * std::f64::consts::FRAC_PI_4;
    let (l, r) = (theta.cos().max(1e-3), theta.sin().max(1e-3));
    Target {
        phase: phase.to_radians() as f32,
        delay: (delay / 1000.0) as f32,
        ratio: (20.0 * (r / l).log10()) as f32,
        hard: pan.abs() >= 99.0,
    }
}

/// Low and high edges of the frequency range taking part (Hz).
fn band(p: &Params) -> (f64, f64) {
    match RANGE_HZ.get(p.index("range")) {
        Some(r) => *r,
        None => {
            let c = p.get("center_freq").max(20.0);
            let half = 2f64.powf(p.get("width").max(0.1) / 2.0);
            (c / half, c * half)
        }
    }
}

fn level(db: f64) -> f32 {
    if db <= LEVEL_FLOOR + 1e-9 {
        0.0
    } else {
        super::db_to_gain(db)
    }
}

/// 1 within `tol`, fading to 0 over the span past it.
fn within(x: f32, tol: f32, span: f32) -> f32 {
    if x <= tol {
        1.0
    } else {
        (1.0 - (x - tol) / span).max(0.0)
    }
}

pub struct CenterExtractor {
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
    spec: [Vec<Complex32>; 2],
    scratch: Vec<Complex32>,
    rate: f32,
    /// Per bin: last frame's share of target sound (0 to 1).
    mask: Vec<f32>,
    /// Per bin: the levels, before and after averaging over neighbours.
    mags: [Vec<f32>; 2],
    smooth: [Vec<f32>; 2],
    /// Per bin: how much it takes part, from the frequency range (0 to 1).
    reach: Vec<f32>,
    target: Target,
    phase_tol: f32,
    amp_tol: f32,
    /// The fade past the limits, as a multiple of each limit.
    bleed: f32,
    amp_band: usize,
    decay: f32,
    center: f32,
    side: f32,
}

impl CenterExtractor {
    pub fn new(p: &Params, rate: u32) -> Self {
        let n = FFT_SIZES[p.index("fft").min(FFT_SIZES.len() - 1)];
        let hop = n / 4;
        let bins = n / 2 + 1;
        let mut planner = RealFftPlanner::<f32>::new();
        let fwd = planner.plan_fft_forward(n);
        let inv = planner.plan_fft_inverse(n);
        let scratch_len = fwd.get_scratch_len().max(inv.get_scratch_len());
        // Square-root Hann on both sides: a Hann window in all, which
        // adds up to 2 at 75% overlap.
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
            spec: [
                vec![Complex32::default(); bins],
                vec![Complex32::default(); bins],
            ],
            scratch: vec![Complex32::default(); scratch_len],
            rate: rate.max(1) as f32,
            mask: vec![0.0; bins],
            mags: [vec![0.0; bins], vec![0.0; bins]],
            smooth: [vec![0.0; bins], vec![0.0; bins]],
            reach: vec![0.0; bins],
            target: target(p),
            phase_tol: 0.0,
            amp_tol: 0.0,
            bleed: 0.0,
            amp_band: 0,
            decay: 0.0,
            center: 1.0,
            side: 1.0,
        };
        s.set_params(p);
        s
    }

    fn set_params(&mut self, p: &Params) {
        self.target = target(p);
        self.phase_tol = p.get("phase_disc").to_radians() as f32;
        self.amp_tol = p.get("amp_disc") as f32;
        self.bleed = 0.25 + (p.get("bleed") / 100.0) as f32 * 3.75;
        self.amp_band = p.get("amp_band").round().max(0.0) as usize;
        self.decay = (p.get("decay") / 100.0 * 0.95) as f32;
        self.center = level(p.get("center"));
        self.side = level(p.get("side"));
        // A quarter-octave fade at each edge of the range, so its edges do not ring.
        let (lo, hi) = band(p);
        let bin_hz = f64::from(self.rate) / self.n as f64;
        let edge = 2f64.powf(0.25);
        for (k, r) in self.reach.iter_mut().enumerate() {
            let f = (k as f64 * bin_hz).max(1.0);
            let a = ((f / (lo / edge)).log2() / edge.log2()).clamp(0.0, 1.0);
            let b = (((hi * edge) / f).log2() / edge.log2()).clamp(0.0, 1.0);
            *r = (a.min(b)) as f32;
        }
    }

    fn frame(&mut self) {
        let n = self.n;
        let hop = self.hop;
        for c in 0..2 {
            for (o, (x, w)) in self
                .frame
                .iter_mut()
                .zip(self.input[c].iter().zip(&self.win))
            {
                *o = x * w;
            }
            let _ = self.fwd.process_with_scratch(
                &mut self.frame,
                &mut self.spec[c],
                &mut self.scratch,
            );
            for (m, s) in self.mags[c].iter_mut().zip(&self.spec[c]) {
                *m = s.norm();
            }
            // Amplitude bandwidth: each level averaged with its neighbours.
            let b = self.amp_band;
            let bins = self.mags[c].len();
            let mut sum: f32 = self.mags[c][..b.min(bins)].iter().sum();
            for k in 0..bins {
                if k + b < bins {
                    sum += self.mags[c][k + b];
                }
                if k > b {
                    sum -= self.mags[c][k - b - 1];
                }
                let count = (k + b).min(bins - 1) + 1 - k.saturating_sub(b);
                self.smooth[c][k] = sum / count as f32;
            }
        }

        let t = self.target;
        let bin_hz = self.rate / n as f32;
        let floor = 1e-9;
        for k in 0..self.mask.len() {
            let mut m = 0.0;
            if self.reach[k] > 0.0 {
                let (l, r) = (self.spec[0][k], self.spec[1][k]);
                let ratio =
                    20.0 * ((self.smooth[1][k] + floor) / (self.smooth[0][k] + floor)).log10();
                let amp = within(
                    (ratio.clamp(-60.0, 60.0) - t.ratio).abs(),
                    self.amp_tol,
                    self.amp_tol * self.bleed,
                );
                let ph = if t.hard {
                    1.0
                } else {
                    let want = t.phase + std::f32::consts::TAU * k as f32 * bin_hz * t.delay;
                    let d = (l * r.conj()).arg() - want;
                    let d = (d + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
                        - std::f32::consts::PI;
                    within(d.abs(), self.phase_tol, self.phase_tol * self.bleed)
                };
                m = amp * ph * self.reach[k];
            }
            // Spectral decay: a share that falls, falls slowly.
            let prev = self.mask[k];
            if m < prev {
                m = prev * self.decay + m * (1.0 - self.decay);
            }
            self.mask[k] = m;
        }

        let scale = 1.0 / (n as f32 * 2.0);
        let (gc, gs) = (self.center, self.side);
        for c in 0..2 {
            for (s, m) in self.spec[c].iter_mut().zip(&self.mask) {
                *s *= gs + m * (gc - gs);
            }
            let last = self.spec[c].len() - 1;
            self.spec[c][0].im = 0.0;
            self.spec[c][last].im = 0.0;
            let _ = self.inv.process_with_scratch(
                &mut self.spec[c],
                &mut self.frame,
                &mut self.scratch,
            );
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

impl Processor for CenterExtractor {
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
        self.mask.fill(0.0);
        self.pos = 0;
    }

    /// Everything but the FFT size can change while it plays.
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
    use std::f32::consts::TAU;

    fn params(list: &[(&str, f64)]) -> Params {
        let info = center_extractor_info();
        let map = list.iter().map(|(k, v)| (k.to_string(), *v)).collect();
        Params::resolve(&info, &map)
    }

    /// Output level (dB) of a 1 kHz tone with these left and right gains.
    fn run(list: &[(&str, f64)], gl: f32, gr: f32) -> (f32, f32) {
        let rate = 48_000;
        let mut fx = CenterExtractor::new(&params(list), rate);
        let frames = rate as usize;
        let mut buf: Vec<f32> = (0..frames)
            .flat_map(|i| {
                let s = (TAU * 1000.0 * i as f32 / rate as f32).sin() * 0.5;
                [s * gl, s * gr]
            })
            .collect();
        fx.process(&mut buf);
        let tail = &buf[frames..];
        let rms = |c: usize| {
            let (sum, n) = tail
                .iter()
                .skip(c)
                .step_by(2)
                .fold((0.0, 0), |(s, n), v| (s + v * v, n + 1));
            let r = (sum / n as f32).sqrt();
            20.0 * (r / (0.5 / 2f32.sqrt())).max(1e-9).log10()
        };
        (rms(0), rms(1))
    }

    #[test]
    fn passes_the_sound_through_at_zero_levels() {
        let (l, r) = run(&[], 1.0, 0.6);
        assert!(l.abs() < 0.2, "{l}");
        assert!((r - 20.0 * 0.6f32.log10()).abs() < 0.2, "{r}");
    }

    #[test]
    fn takes_out_the_center_and_keeps_the_sides() {
        let (l, _) = run(&[("center", LEVEL_FLOOR)], 1.0, 1.0);
        assert!(l < -40.0, "center left in: {l}");
        let (l, r) = run(&[("center", LEVEL_FLOOR)], 1.0, 0.0);
        assert!(l > -1.0, "a hard-left sound was cut: {l}");
        assert!(r < -40.0);
    }

    #[test]
    fn keeps_only_the_center() {
        let (l, _) = run(&[("side", LEVEL_FLOOR)], 1.0, 1.0);
        assert!(l.abs() < 0.3, "{l}");
        let (l, _) = run(&[("side", LEVEL_FLOOR)], 1.0, 0.1);
        assert!(l < -40.0, "{l}");
    }

    #[test]
    fn finds_out_of_phase_sound_as_surround() {
        let (l, _) = run(&[("extract", 3.0), ("center", LEVEL_FLOOR)], 1.0, -1.0);
        assert!(l < -40.0, "{l}");
        let (l, _) = run(&[("extract", 3.0), ("center", LEVEL_FLOOR)], 1.0, 1.0);
        assert!(l > -1.0, "{l}");
    }

    #[test]
    fn leaves_frequencies_outside_the_range_alone() {
        let (l, _) = run(&[("range", 1.0), ("center", LEVEL_FLOOR)], 1.0, 1.0);
        assert!(l > -1.0, "a 1 kHz tone is not bass: {l}");
    }
}
