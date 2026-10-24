// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

//! Phase Rotate: turns the phase of every frequency by the same angle,
//! flips either channel's polarity, and delays the signal by a fraction
//! of a millisecond up to 20 ms. For lining up music or a second mic that
//! sits partly out of phase with another track. Every control can change
//! while it plays (automation): the angle moves frame by frame, the delay
//! glides and polarity flips fade over 5 ms, so nothing clicks.
//!
//! The rotation works on the spectrum: each frame is windowed, every
//! positive-frequency bin is multiplied by e^(i angle), and the frames are
//! added back with 75% overlap. At 0 degrees it passes the audio through
//! unchanged apart from the frame's delay, which is reported as latency.

use std::sync::Arc;

use realfft::num_complex::Complex32;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};

use super::{choice, num, toggle, EffectInfo, Params, Processor};

pub fn phase_info() -> EffectInfo {
    EffectInfo {
        id: "altaudio.phase_rotate",
        name: "Phase Rotate",
        category: "Filter and EQ",
        description: "Turns the phase of the whole signal by an angle, flips either channel's polarity, and adds a fine delay, to line up tracks that partly cancel each other.",
        params: vec![
            num("angle", "Angle", "°", -180.0, 180.0, 0.0, 1.0),
            choice("channels", "Rotate", &["Both channels", "Left only", "Right only"], 0),
            toggle("invert_left", "Flip left polarity", false).group("Polarity"),
            toggle("invert_right", "Flip right polarity", false).group("Polarity"),
            num("delay", "Delay", "ms", 0.0, 20.0, 0.0, 0.01).group("Timing"),
        ],
    }
}

/// Largest delay, in milliseconds.
const MAX_DELAY_MS: f64 = 20.0;

pub struct PhaseRotate {
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
    /// Rotation in radians, used from the next frame.
    angle: f32,
    /// Which channels rotate: [left, right].
    rotate: [bool; 2],
    /// Polarity target and current gain per channel.
    sign: [f32; 2],
    gain: [f32; 2],
    gain_step: f32,
    /// Delay line, interleaved stereo.
    line: Vec<f32>,
    at: usize,
    delay: f64,
    delay_target: f64,
    glide: f64,
}

impl PhaseRotate {
    pub fn new(p: &Params, rate: u32) -> Self {
        let n = crate::dsp::stft::Stft::<NoMask>::size_for(40.0, rate).max(512);
        let hop = n / 4;
        let mut planner = RealFftPlanner::<f32>::new();
        let fwd = planner.plan_fft_forward(n);
        let inv = planner.plan_fft_inverse(n);
        let scratch_len = fwd.get_scratch_len().max(inv.get_scratch_len());
        let win = (0..n)
            .map(|i| (0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / n as f32).cos()).sqrt())
            .collect();
        let rate = f64::from(rate.max(1));
        let line_frames = (MAX_DELAY_MS / 1000.0 * rate).ceil() as usize + 4;
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
            rate,
            angle: 0.0,
            rotate: [true, true],
            sign: [1.0, 1.0],
            gain: [1.0, 1.0],
            gain_step: (1.0 / (0.005 * rate)) as f32,
            line: vec![0.0; line_frames * 2],
            at: 0,
            delay: 0.0,
            delay_target: 0.0,
            // About 30 ms to settle on a new delay.
            glide: 1.0 - (-1.0 / (0.03 * rate)).exp(),
        };
        s.set_params(p);
        s.gain = s.sign;
        s.delay = s.delay_target;
        s
    }

    fn set_params(&mut self, p: &Params) {
        self.angle = (p.get("angle") as f32).to_radians();
        self.rotate = match p.index("channels") {
            1 => [true, false],
            2 => [false, true],
            _ => [true, true],
        };
        self.sign = [
            if p.on("invert_left") { -1.0 } else { 1.0 },
            if p.on("invert_right") { -1.0 } else { 1.0 },
        ];
        self.delay_target = p.get("delay").clamp(0.0, MAX_DELAY_MS) / 1000.0 * self.rate;
    }

    fn frame(&mut self) {
        let n = self.n;
        let hop = self.hop;
        let (sin, cos) = self.angle.sin_cos();
        let turn = Complex32::new(cos, sin);
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
            if self.rotate[c] && self.angle != 0.0 {
                let last = self.spec.len() - 1;
                for s in &mut self.spec[1..last] {
                    *s *= turn;
                }
                // DC and Nyquist are real: they can only scale.
                self.spec[0] *= cos;
                self.spec[last] *= cos;
            }
            self.spec[0].im = 0.0;
            let last = self.spec.len() - 1;
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

/// Only for asking the shared code for a frame size.
struct NoMask;
impl crate::dsp::stft::Mask for NoMask {
    fn mask(&mut self, _: &[f32], _: &mut [f32]) {}
    fn reset(&mut self) {}
}

impl Processor for PhaseRotate {
    fn process(&mut self, buf: &mut [f32]) {
        let start = self.n - self.hop;
        let frames = self.line.len() / 2;
        for f in buf.as_chunks_mut::<2>().0 {
            // Rotation (delayed by one frame, the reported latency).
            for (c, v) in f.iter_mut().enumerate() {
                self.input[c][start + self.pos] = *v;
                *v = self.out[c][self.pos];
            }
            self.pos += 1;
            if self.pos == self.hop {
                self.pos = 0;
                self.frame();
            }
            // Fine delay, read with linear interpolation while it glides.
            self.line[self.at * 2] = f[0];
            self.line[self.at * 2 + 1] = f[1];
            self.delay += (self.delay_target - self.delay) * self.glide;
            let d = self.delay.clamp(0.0, (frames - 2) as f64);
            let whole = d.floor() as usize;
            let t = (d - whole as f64) as f32;
            let a = (self.at + frames - whole) % frames;
            let b = (a + frames - 1) % frames;
            for (c, v) in f.iter_mut().enumerate() {
                let x = self.line[a * 2 + c] * (1.0 - t) + self.line[b * 2 + c] * t;
                // Polarity flips fade instead of jumping.
                let g = &mut self.gain[c];
                let target = self.sign[c];
                if *g < target {
                    *g = (*g + self.gain_step).min(target);
                } else if *g > target {
                    *g = (*g - self.gain_step).max(target);
                }
                *v = x * *g;
            }
            self.at = (self.at + 1) % frames;
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
        self.line.fill(0.0);
        self.at = 0;
        self.delay = self.delay_target;
        self.gain = self.sign;
    }

    fn update(&mut self, p: &Params) -> bool {
        self.set_params(p);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::model::{new_id, EffectSlot};

    fn make(params: &[(&str, f64)]) -> PhaseRotate {
        let info = phase_info();
        let slot = EffectSlot {
            id: new_id(),
            effect: info.id.into(),
            enabled: true,
            params: params.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
            preset: None,
            sidechain: None,
            plugin_state: None,
            position: None,
        };
        PhaseRotate::new(&Params::resolve(&info, &slot.params), 48_000)
    }

    fn sine(freq: f64, frames: usize) -> Vec<f32> {
        (0..frames)
            .flat_map(|i| {
                let v = (std::f64::consts::TAU * freq * i as f64 / 48_000.0).sin() as f32 * 0.5;
                [v, v]
            })
            .collect()
    }

    /// Correlation of a and b (interleaved left channel) over the tail.
    fn corr(a: &[f32], b: &[f32]) -> f32 {
        let (mut ab, mut aa, mut bb) = (0.0f64, 0.0f64, 0.0f64);
        for (x, y) in a.iter().step_by(2).zip(b.iter().step_by(2)) {
            ab += f64::from(*x) * f64::from(*y);
            aa += f64::from(*x) * f64::from(*x);
            bb += f64::from(*y) * f64::from(*y);
        }
        (ab / (aa.sqrt() * bb.sqrt()).max(1e-12)) as f32
    }

    #[test]
    fn zero_degrees_passes_through_after_the_latency() {
        let mut p = make(&[]);
        let lat = p.latency();
        let input = sine(440.0, 48_000);
        let mut buf = input.clone();
        p.process(&mut buf);
        let out = &buf[(lat + 4_000) * 2..];
        let reference = &input[4_000 * 2..input.len() - lat * 2];
        assert!(corr(out, reference) > 0.999, "{}", corr(out, reference));
    }

    #[test]
    fn rotation_by_180_inverts_and_by_90_is_orthogonal() {
        let input = sine(440.0, 48_000);
        for (angle, want) in [(180.0, -1.0f32), (90.0, 0.0)] {
            let mut p = make(&[("angle", angle)]);
            let lat = p.latency();
            let mut buf = input.clone();
            p.process(&mut buf);
            let out = &buf[(lat + 4_000) * 2..];
            let reference = &input[4_000 * 2..input.len() - lat * 2];
            let c = corr(out, reference);
            assert!((c - want).abs() < 0.02, "{angle}: {c}");
        }
    }

    #[test]
    fn delay_and_polarity_apply_per_channel() {
        let mut p = make(&[("invert_right", 1.0), ("delay", 1.0)]);
        let lat = p.latency();
        let input = sine(200.0, 24_000);
        let mut buf = input.clone();
        p.process(&mut buf);
        let shift = lat + 48;
        let out_l: Vec<f32> = buf[shift * 2..].iter().step_by(2).copied().collect();
        let out_r: Vec<f32> = buf[shift * 2 + 1..].iter().step_by(2).copied().collect();
        let src: Vec<f32> = input.iter().step_by(2).copied().collect();
        let n = 10_000;
        let err_l: f32 = (2_000..n)
            .map(|i| (out_l[i] - src[i]).abs())
            .fold(0.0, f32::max);
        let err_r: f32 = (2_000..n)
            .map(|i| (out_r[i] + src[i]).abs())
            .fold(0.0, f32::max);
        assert!(err_l < 0.01 && err_r < 0.01, "{err_l} {err_r}");
    }
}
