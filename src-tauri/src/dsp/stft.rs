// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Streaming short-time Fourier processing for effects that work on the
//! spectrum (noise, hiss and room reduction).
//!
//! Each frame of `n` samples is windowed, transformed, multiplied by a
//! gain per frequency bin that the effect's [`Mask`] chooses, and added
//! back with 75% overlap. Both channels get the same gains, worked out
//! from their average power, so the stereo image stays put. The output
//! is delayed by `n` samples, which the effect reports as its latency.

use std::sync::Arc;

use realfft::num_complex::Complex32;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};

/// Chooses a gain (0 to 1) per bin from each frame's power.
pub trait Mask: Send {
    /// `power` and `gains` have `n / 2 + 1` bins.
    fn mask(&mut self, power: &[f32], gains: &mut [f32]);
    fn reset(&mut self);
}

pub struct Stft<M: Mask> {
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
    power: Vec<f32>,
    gains: Vec<f32>,
    pub mask: M,
}

impl<M: Mask> Stft<M> {
    /// `n` must be a power of two of at least 64.
    pub fn new(n: usize, mask: M) -> Self {
        let hop = n / 4;
        let mut planner = RealFftPlanner::<f32>::new();
        let fwd = planner.plan_fft_forward(n);
        let inv = planner.plan_fft_inverse(n);
        let scratch_len = fwd.get_scratch_len().max(inv.get_scratch_len());
        // Square-root Hann on both sides: their product is a Hann
        // window, which adds up to 2 at 75% overlap.
        let win = (0..n)
            .map(|i| (0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / n as f32).cos()).sqrt())
            .collect();
        let bins = n / 2 + 1;
        Self {
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
            power: vec![0.0; bins],
            gains: vec![1.0; bins],
            mask,
        }
    }

    /// The FFT size for about `ms` milliseconds at `rate`.
    pub fn size_for(ms: f64, rate: u32) -> usize {
        ((ms / 1000.0 * f64::from(rate)) as usize)
            .next_power_of_two()
            .clamp(256, 8192)
    }

    pub fn latency(&self) -> usize {
        self.n
    }

    /// Frames per second the mask is called at.
    pub fn frames_per_second(&self, rate: u32) -> f64 {
        f64::from(rate) / self.hop as f64
    }

    pub fn reset(&mut self) {
        for c in 0..2 {
            self.input[c].fill(0.0);
            self.acc[c].fill(0.0);
            self.out[c].fill(0.0);
        }
        self.pos = 0;
        self.mask.reset();
    }

    pub fn process(&mut self, buf: &mut [f32]) {
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

    fn frame(&mut self) {
        let n = self.n;
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
        }
        for (k, p) in self.power.iter_mut().enumerate() {
            *p = (self.spec[0][k].norm_sqr() + self.spec[1][k].norm_sqr()) * 0.5;
        }
        self.mask.mask(&self.power, &mut self.gains);
        // Inverse scale: 1/n for the transform, 1/2 for the window sum.
        let scale = 1.0 / (n as f32 * 2.0);
        let hop = self.hop;
        for c in 0..2 {
            for (s, g) in self.spec[c].iter_mut().zip(&self.gains) {
                *s *= *g;
            }
            // The transform needs real DC and Nyquist bins.
            self.spec[c][0].im = 0.0;
            let last = self.spec[c].len() - 1;
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

#[cfg(test)]
mod tests {
    use super::*;

    struct Pass;
    impl Mask for Pass {
        fn mask(&mut self, _: &[f32], g: &mut [f32]) {
            g.fill(1.0);
        }
        fn reset(&mut self) {}
    }

    #[test]
    fn unity_mask_gives_back_the_input_delayed() {
        let mut s = Stft::new(1024, Pass);
        let lat = s.latency();
        let input: Vec<f32> = (0..20_000)
            .flat_map(|i| {
                let v = (i as f32 * 0.031).sin() * 0.5 + (i as f32 * 0.0007).cos() * 0.2;
                [v, -v]
            })
            .collect();
        let mut b = input.clone();
        for chunk in b.chunks_mut(2 * 333) {
            s.process(chunk);
        }
        let mut worst = 0f32;
        for i in lat + 2000..10_000 {
            worst = worst.max((b[i * 2] - input[(i - lat) * 2]).abs());
            worst = worst.max((b[i * 2 + 1] - input[(i - lat) * 2 + 1]).abs());
        }
        assert!(worst < 1e-4, "{worst}");
    }
}
