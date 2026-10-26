// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Pitch-preserving time stretch for stereo audio, streamed.
//!
//! The method is a phase vocoder with phase locking. The input is cut
//! into overlapping windowed frames, taken closer together than they are
//! placed in the output to make the sound longer (further apart to make
//! it shorter). Each frame's spectrum keeps its levels, and its phases
//! are moved on by what each frequency would have turned through over
//! the output spacing, so the pieces join smoothly and the pitch stays.
//!
//! Three things keep it clean rather than smeared or "phasey":
//! - **Phase locking:** only the peaks of the spectrum (the partials)
//!   get their phase worked out; the frequencies around each peak keep
//!   their phase relation to it, as in the original frame.
//! - **Attacks:** a frame where the spectrum suddenly gains energy (a
//!   drum hit, a consonant) takes its phases straight from the input, so
//!   attacks stay sharp instead of washing out.
//! - **Stereo:** the phase change is worked out once, on the sum of the
//!   channels (or their difference, where that is louder), and applied
//!   to both, so the stereo image stays put.
//!
//! The output starts in step with the input: frames before the start
//! read the input mirrored, so the clip starts at full level.

use std::sync::Arc;

use realfft::num_complex::Complex32;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};

/// Frame length in seconds for a single voice or instrument.
const FRAME_SECONDS: f64 = 0.03;

/// The FFT size for a frame length: the power of two nearest
/// 1.4 times that many frames (the window's useful middle is shorter
/// than the window), 512 to 16384.
fn fft_size(rate: u32, frame_seconds: f64) -> usize {
    let want = (f64::from(rate) * frame_seconds * 1.4).max(1.0);
    let n = 2f64.powf(want.log2().round()) as usize;
    n.clamp(512, 16_384)
}

fn princarg(x: f32) -> f32 {
    let t = std::f32::consts::TAU;
    x - t * ((x + std::f32::consts::PI) / t).floor()
}

pub struct Stretcher {
    /// Output length over input length.
    ratio: f64,
    n: usize,
    /// Output spacing of frames (a quarter frame).
    hop: usize,
    win: Vec<f32>,
    fwd: Arc<dyn RealToComplex<f32>>,
    inv: Arc<dyn ComplexToReal<f32>>,
    scratch: Vec<Complex32>,
    /// Input not yet dropped, stereo interleaved, starting at frame `base`.
    input: Vec<f32>,
    base: usize,
    ended: bool,
    flushed: bool,
    /// The next frame's number (they start at -1).
    frame: i64,
    /// Where the last frame was taken from (input frame), for the phase step.
    prev_center: i64,
    first: bool,
    /// Per bin: the reference's phase in the last frame, and the phase given out.
    prev_phase: Vec<f32>,
    out_phase: Vec<f32>,
    /// Per bin: the reference level in the last frame, for finding attacks.
    prev_mag: Vec<f32>,
    flux_avg: f32,
    /// Overlap-add buffer, `n` stereo frames, starting at output frame `ola_at`.
    ola: Vec<f32>,
    ola_at: i64,
    time: [Vec<f32>; 2],
    spec: [Vec<Complex32>; 2],
    mag: Vec<f32>,
    phase: Vec<f32>,
    peaks: Vec<usize>,
}

impl Stretcher {
    /// `ratio` above 1 makes the audio longer (slower), below 1 shorter.
    pub fn new(ratio: f64, rate: u32) -> Self {
        Self::with_frame(ratio, rate, FRAME_SECONDS)
    }

    /// The same with a chosen frame length in seconds: longer suits music
    /// with several notes at once, shorter a single voice.
    pub fn with_frame(ratio: f64, rate: u32, frame_seconds: f64) -> Self {
        let n = fft_size(rate, frame_seconds);
        let hop = n / 4;
        let mut planner = RealFftPlanner::<f32>::new();
        let fwd = planner.plan_fft_forward(n);
        let inv = planner.plan_fft_inverse(n);
        let scratch = vec![Complex32::default(); fwd.get_scratch_len().max(inv.get_scratch_len())];
        // Periodic Hann, used going in and coming out: at a quarter-frame
        // spacing the squares add up to 1.5 everywhere.
        let win = (0..n)
            .map(|i| (0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / n as f64).cos()) as f32)
            .collect();
        let bins = n / 2 + 1;
        Self {
            // Pitch changes stretch further than the clip itself (see the renderer).
            ratio: ratio.clamp(0.125, 8.0),
            n,
            hop,
            win,
            fwd,
            inv,
            scratch,
            input: Vec::new(),
            base: 0,
            ended: false,
            flushed: false,
            frame: -1,
            prev_center: 0,
            first: true,
            prev_phase: vec![0.0; bins],
            out_phase: vec![0.0; bins],
            prev_mag: vec![0.0; bins],
            flux_avg: 0.0,
            ola: vec![0.0; n * 2],
            ola_at: -(3 * hop as i64),
            time: [vec![0.0; n], vec![0.0; n]],
            spec: [
                vec![Complex32::default(); bins],
                vec![Complex32::default(); bins],
            ],
            mag: vec![0.0; bins],
            phase: vec![0.0; bins],
            peaks: Vec::with_capacity(bins / 2),
        }
    }

    /// Adds stereo interleaved input.
    pub fn push(&mut self, samples: &[f32]) {
        self.input.extend_from_slice(samples);
    }

    /// No more input will come; `pull` then drains what is left.
    pub fn finish(&mut self) {
        self.ended = true;
    }

    fn end(&self) -> usize {
        self.base + self.input.len() / 2
    }

    /// Input sample `idx` of channel `ch`; before the start, the input mirrored.
    fn at(&self, idx: i64, ch: usize) -> f32 {
        let i = idx.unsigned_abs() as usize;
        if i < self.base {
            return 0.0;
        }
        self.input
            .get((i - self.base) * 2 + ch)
            .copied()
            .unwrap_or(0.0)
    }

    /// Where frame `k` is taken from (its middle, as an input frame).
    fn center(&self, k: i64) -> i64 {
        let out = k * self.hop as i64;
        if k < 0 {
            out
        } else {
            (out as f64 / self.ratio).round() as i64
        }
    }

    /// Appends all output that the input so far allows (stereo interleaved).
    pub fn pull(&mut self, out: &mut Vec<f32>) {
        while !self.flushed {
            let total = self
                .ended
                .then(|| (self.end() as f64 * self.ratio).round() as i64);
            if let Some(t) = total {
                if self.ola_at >= t {
                    self.flushed = true;
                    return;
                }
            }
            let c = self.center(self.frame);
            if !self.ended && c + (self.n / 2) as i64 > self.end() as i64 {
                return;
            }
            self.analyse_and_place(c);
            // The first quarter of the buffer is now final.
            let from = (-self.ola_at).clamp(0, self.hop as i64) as usize;
            let to = match total {
                Some(t) => (t - self.ola_at).clamp(0, self.hop as i64) as usize,
                None => self.hop,
            };
            if to > from {
                out.extend_from_slice(&self.ola[from * 2..to * 2]);
            }
            self.ola.copy_within(self.hop * 2.., 0);
            let tail = self.ola.len() - self.hop * 2;
            self.ola[tail..].fill(0.0);
            self.ola_at += self.hop as i64;
            self.frame += 1;
            // Drop input no later frame will read.
            let keep = (self.center(self.frame) - (self.n / 2) as i64).max(0) as usize;
            if keep > self.base {
                let drop = (keep - self.base).min(self.input.len() / 2);
                self.input.drain(..drop * 2);
                self.base += drop;
            }
        }
    }

    /// One frame: taken from the input around `c`, its phases moved on,
    /// and added into the output buffer.
    fn analyse_and_place(&mut self, c: i64) {
        let n = self.n;
        let start = c - (n / 2) as i64;
        for ch in 0..2 {
            for j in 0..n {
                self.time[ch][j] = self.at(start + j as i64, ch) * self.win[j];
            }
            let _ = self.fwd.process_with_scratch(
                &mut self.time[ch],
                &mut self.spec[ch],
                &mut self.scratch,
            );
        }

        // The reference per bin: the sum of the channels, or their
        // difference where the sound is mostly out of phase.
        let bins = self.mag.len();
        let mut total = 0.0f32;
        let mut rise = 0.0f32;
        for k in 0..bins {
            let (l, r) = (self.spec[0][k], self.spec[1][k]);
            let (m, s) = (l + r, l - r);
            let refc = if m.norm_sqr() >= 0.1 * s.norm_sqr() {
                m
            } else {
                s
            };
            self.mag[k] = refc.norm();
            self.phase[k] = refc.arg();
            total += self.mag[k];
            rise += (self.mag[k] - self.prev_mag[k]).max(0.0);
        }
        // An attack: the spectrum gains much more than it usually does.
        let flux = rise / (total + 1e-9);
        let attack = !self.first && flux > 0.3 && flux > 2.5 * self.flux_avg;
        self.flux_avg = 0.85 * self.flux_avg + 0.15 * flux;

        if self.first || attack {
            self.out_phase.copy_from_slice(&self.phase);
        } else {
            let step = (c - self.prev_center).max(1) as f32;
            let hop = self.hop as f32;
            let w = std::f32::consts::TAU / n as f32;
            // Peaks: bins louder than both neighbours, and not lost in the floor.
            let top = self.mag.iter().fold(0.0f32, |a, b| a.max(*b));
            self.peaks.clear();
            for k in 0..bins {
                let m = self.mag[k];
                let left = if k > 0 { self.mag[k - 1] } else { 0.0 };
                let right = if k + 1 < bins { self.mag[k + 1] } else { 0.0 };
                if m > left && m >= right && m > top * 1e-5 {
                    self.peaks.push(k);
                }
            }
            if self.peaks.is_empty() {
                for k in 0..bins {
                    let omega = w * k as f32;
                    let dev = princarg(self.phase[k] - self.prev_phase[k] - omega * step);
                    self.out_phase[k] += (omega + dev / step) * hop;
                }
            } else {
                // Each bin follows the nearest peak, keeping its phase relation to it.
                let mut lo = 0;
                for (p, &k) in self.peaks.iter().enumerate() {
                    let hi = match self.peaks.get(p + 1) {
                        Some(&next) => (k + next) / 2 + 1,
                        None => bins,
                    };
                    let omega = w * k as f32;
                    let dev = princarg(self.phase[k] - self.prev_phase[k] - omega * step);
                    let at_peak = princarg(self.out_phase[k] + (omega + dev / step) * hop);
                    let turn = at_peak - self.phase[k];
                    for b in lo..hi {
                        self.out_phase[b] = self.phase[b] + turn;
                    }
                    lo = hi;
                }
            }
        }
        self.prev_phase.copy_from_slice(&self.phase);
        self.prev_mag.copy_from_slice(&self.mag);
        self.prev_center = c;
        self.first = false;

        // Both channels turned by the reference's change: their relation stays.
        let scale = 1.0 / (n as f32 * 1.5);
        for ch in 0..2 {
            for k in 0..bins {
                let turn = self.out_phase[k] - self.phase[k];
                self.spec[ch][k] *= Complex32::new(turn.cos(), turn.sin());
            }
            self.spec[ch][0].im = 0.0;
            self.spec[ch][bins - 1].im = 0.0;
            let _ = self.inv.process_with_scratch(
                &mut self.spec[ch],
                &mut self.time[ch],
                &mut self.scratch,
            );
            for j in 0..n {
                self.ola[j * 2 + ch] += self.time[ch][j] * self.win[j] * scale;
            }
        }
        // Keep the running phases small.
        for p in &mut self.out_phase {
            *p = princarg(*p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f32, rate: u32, secs: f32) -> Vec<f32> {
        let n = (rate as f32 * secs) as usize;
        (0..n)
            .flat_map(|i| {
                let v = (i as f32 * freq * std::f32::consts::TAU / rate as f32).sin() * 0.5;
                [v, v]
            })
            .collect()
    }

    fn run_with(mut s: Stretcher, input: &[f32]) -> Vec<f32> {
        let mut out = Vec::new();
        for block in input.chunks(4_096) {
            s.push(block);
            s.pull(&mut out);
        }
        s.finish();
        s.pull(&mut out);
        out
    }

    fn run(input: &[f32], ratio: f64, rate: u32) -> Vec<f32> {
        run_with(Stretcher::new(ratio, rate), input)
    }

    /// Rising zero crossings per second in the left channel.
    fn freq(stereo: &[f32], rate: u32) -> f32 {
        let left: Vec<f32> = stereo.iter().step_by(2).copied().collect();
        let skip = left.len() / 10;
        let body = &left[skip..left.len() - skip];
        let ups = body
            .windows(2)
            .filter(|w| w[0] < 0.0 && w[1] >= 0.0)
            .count();
        ups as f32 * rate as f32 / body.len() as f32
    }

    #[test]
    fn longer_and_shorter_keep_the_pitch_and_length() {
        let rate = 48_000;
        let input = sine(220.0, rate, 2.0);
        for ratio in [1.25, 0.8, 1.1, 2.0, 0.5, 4.0] {
            let out = run(&input, ratio, rate);
            let want = input.len() as f64 / 2.0 * ratio;
            let got = out.len() as f64 / 2.0;
            assert!(
                (got - want).abs() <= 1.0,
                "ratio {ratio}: {got} frames, want {want}"
            );
            let f = freq(&out, rate);
            assert!((f - 220.0).abs() < 3.0, "ratio {ratio}: pitch {f} Hz");
        }
    }

    #[test]
    fn steady_tone_stays_steady() {
        let rate = 48_000;
        for ratio in [1.2, 0.7, 2.5] {
            let out = run(&sine(220.0, rate, 1.0), ratio, rate);
            // No dropouts or bursts: the level of each 10 ms stays near the input's.
            let left: Vec<f32> = out.iter().step_by(2).copied().collect();
            for chunk in left[4_800..left.len() - 4_800].chunks(480) {
                let rms = (chunk.iter().map(|v| v * v).sum::<f32>() / chunk.len() as f32).sqrt();
                assert!((rms - 0.3536).abs() < 0.02, "ratio {ratio}: rms {rms}");
            }
        }
    }

    #[test]
    fn a_chord_keeps_its_notes_steady() {
        // Two notes close together beat against each other in a plain
        // vocoder's smear; with phase locking each stays at its level.
        let rate = 48_000;
        let a = sine(440.0, rate, 1.5);
        let b = sine(554.37, rate, 1.5);
        let input: Vec<f32> = a.iter().zip(&b).map(|(x, y)| (x + y) * 0.5).collect();
        let out = run_with(Stretcher::with_frame(1.5, rate, 0.08), &input);
        let left: Vec<f32> = out.iter().step_by(2).copied().collect();
        let rms = |s: &[f32]| (s.iter().map(|v| v * v).sum::<f32>() / s.len() as f32).sqrt();
        let whole = rms(&left[9_600..left.len() - 9_600]);
        for chunk in left[9_600..left.len() - 9_600].chunks(4_800) {
            assert!(
                (rms(chunk) - whole).abs() / whole < 0.12,
                "{} vs {whole}",
                rms(chunk)
            );
        }
    }

    #[test]
    fn keeps_the_stereo_image() {
        // Left and right a quarter turn apart stay a quarter turn apart.
        let rate = 48_000;
        let n = rate as usize;
        let input: Vec<f32> = (0..n)
            .flat_map(|i| {
                let t = i as f32 * 300.0 * std::f32::consts::TAU / rate as f32;
                [t.sin() * 0.5, t.cos() * 0.5]
            })
            .collect();
        let out = run(&input, 1.3, rate);
        let body = &out[out.len() / 4..out.len() * 3 / 4];
        let (mut lr, mut ll) = (0.0f32, 0.0f32);
        for f in body.chunks(2) {
            lr += f[0] * f[1];
            ll += f[0] * f[0];
        }
        assert!((lr / ll).abs() < 0.05, "correlation {}", lr / ll);
    }

    #[test]
    fn an_attack_stays_sharp() {
        // Silence, then a sudden tone: it should be at full level soon after.
        let rate = 48_000;
        let mut input = vec![0.0f32; rate as usize];
        input.extend(sine(1000.0, rate, 0.5));
        let out = run(&input, 1.5, rate);
        let onset = (rate as f64 * 0.5 * 1.5) as usize;
        let left: Vec<f32> = out.iter().step_by(2).copied().collect();
        // Little sound well before the onset.
        let pre = &left[onset - 4_000..onset - 2_000];
        assert!(pre.iter().all(|v| v.abs() < 0.05), "pre-echo");
        let after = &left[onset + 1_500..onset + 2_500];
        let rms = (after.iter().map(|v| v * v).sum::<f32>() / after.len() as f32).sqrt();
        assert!(rms > 0.3, "attack came in slowly: {rms}");
    }

    #[test]
    fn starts_at_full_level() {
        let out = run(&vec![0.5; 48_000 * 2], 1.2, 48_000);
        assert!(
            (out[0] - 0.5).abs() < 0.01 && (out[200] - 0.5).abs() < 0.01,
            "{} {}",
            out[0],
            out[200]
        );
    }
}
