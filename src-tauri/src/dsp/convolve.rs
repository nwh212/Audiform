// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Convolution with an impulse response, for the Amp Model's cabinet and
//! the Convolution Reverb, and resampling a response to the session rate.

use std::sync::Arc;

use realfft::num_complex::Complex32;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};

/// A response at another rate, by windowed-sinc interpolation (done once,
/// when the effect is built).
pub fn resample(x: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || from == 0 || to == 0 {
        return x.to_vec();
    }
    let ratio = f64::from(to) / f64::from(from);
    let n = ((x.len() as f64) * ratio).round() as usize;
    // Lowpass at the lower of the two Nyquists.
    let cut = ratio.min(1.0);
    const A: f64 = 16.0;
    (0..n)
        .map(|j| {
            let t = j as f64 / ratio;
            let lo = (t - A / cut).floor().max(0.0) as usize;
            let hi = ((t + A / cut).ceil() as usize).min(x.len().saturating_sub(1));
            let mut s = 0.0;
            for (i, v) in x.iter().enumerate().take(hi + 1).skip(lo) {
                // Lanczos: a sinc, windowed by a wider sinc.
                let d = (t - i as f64) * cut;
                if d.abs() < A {
                    s += f64::from(*v) * sinc(d) * sinc(d / A) * cut;
                }
            }
            s as f32
        })
        .collect()
}

fn sinc(x: f64) -> f64 {
    if x.abs() < 1e-9 {
        1.0
    } else {
        let px = std::f64::consts::PI * x;
        px.sin() / px
    }
}

/// Uniformly partitioned convolution: the response cut into blocks of
/// `part` frames, each multiplied in the frequency domain with the input's recent
/// blocks. Adds `part` frames of delay.
pub struct Convolver {
    part: usize,
    fwd: Arc<dyn RealToComplex<f32>>,
    inv: Arc<dyn ComplexToReal<f32>>,
    /// The response's blocks, transformed.
    parts: Vec<Vec<Complex32>>,
    /// The input's recent blocks, transformed, newest at `head`.
    history: Vec<Vec<Complex32>>,
    head: usize,
    /// Last block and this block of input (2 * part).
    frame: Vec<f32>,
    fill: usize,
    out: Vec<f32>,
    acc: Vec<Complex32>,
    time: Vec<f32>,
    scratch_f: Vec<Complex32>,
    scratch_i: Vec<Complex32>,
}

impl Convolver {
    /// `part` must be a power of two.
    pub fn new(ir: &[f32], part: usize) -> Convolver {
        let n = 2 * part;
        let mut planner = RealFftPlanner::<f32>::new();
        let fwd = planner.plan_fft_forward(n);
        let inv = planner.plan_fft_inverse(n);
        let bins = n / 2 + 1;
        let mut scratch_f = fwd.make_scratch_vec();
        let parts: Vec<Vec<Complex32>> = ir
            .chunks(part)
            .map(|c| {
                let mut t = vec![0f32; n];
                t[..c.len()].copy_from_slice(c);
                let mut f = vec![Complex32::new(0.0, 0.0); bins];
                let _ = fwd.process_with_scratch(&mut t, &mut f, &mut scratch_f);
                f
            })
            .collect();
        let count = parts.len().max(1);
        Convolver {
            part,
            history: vec![vec![Complex32::new(0.0, 0.0); bins]; count],
            head: 0,
            frame: vec![0.0; n],
            fill: 0,
            out: vec![0.0; part],
            acc: vec![Complex32::new(0.0, 0.0); bins],
            time: vec![0.0; n],
            scratch_i: inv.make_scratch_vec(),
            scratch_f,
            fwd,
            inv,
            parts,
        }
    }

    pub fn reset(&mut self) {
        for h in &mut self.history {
            h.fill(Complex32::new(0.0, 0.0));
        }
        self.frame.fill(0.0);
        self.out.fill(0.0);
        self.fill = 0;
    }

    /// One sample in, one out, `part` frames late.
    #[inline]
    pub fn run(&mut self, x: f32) -> f32 {
        let y = self.out[self.fill];
        self.frame[self.part + self.fill] = x;
        self.fill += 1;
        if self.fill == self.part {
            self.fill = 0;
            self.block();
        }
        y
    }

    fn block(&mut self) {
        let n = 2 * self.part;
        self.head = (self.head + 1) % self.history.len();
        self.time.copy_from_slice(&self.frame);
        let _ = self.fwd.process_with_scratch(
            &mut self.time,
            &mut self.history[self.head],
            &mut self.scratch_f,
        );
        self.acc.fill(Complex32::new(0.0, 0.0));
        let count = self.history.len();
        for (p, part) in self.parts.iter().enumerate() {
            let x = &self.history[(self.head + count - p) % count];
            for ((a, h), v) in self.acc.iter_mut().zip(part).zip(x) {
                *a += h * v;
            }
        }
        // The inverse wants a real first and last bin.
        self.acc[0].im = 0.0;
        self.acc[n / 2].im = 0.0;
        let _ = self
            .inv
            .process_with_scratch(&mut self.acc, &mut self.time, &mut self.scratch_i);
        let scale = 1.0 / n as f32;
        for (o, t) in self.out.iter_mut().zip(&self.time[self.part..]) {
            *o = t * scale;
        }
        self.frame.copy_within(self.part.., 0);
    }
}
