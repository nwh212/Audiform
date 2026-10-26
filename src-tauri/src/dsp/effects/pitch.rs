// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Pitch Shift: changes pitch without changing speed. The audio is made
//! longer (or shorter) by the pitch factor with the same pitch-keeping
//! stretch the timeline uses, then played back that much faster (or
//! slower), which brings the length back and moves the pitch. A fixed
//! delay in front keeps the output steady; it is reported as latency so
//! playback stays in time.

use std::collections::VecDeque;

use super::{num, EffectInfo, Params, Processor};
use crate::dsp::stretch::Stretcher;

pub fn pitch_info() -> EffectInfo {
    EffectInfo {
        id: "altaudio.pitch_shift",
        name: "Pitch Shift",
        category: "Time and Pitch",
        description: "Raises or lowers the pitch without changing the speed. Small amounts keep a voice natural.",
        params: vec![
            num("semitones", "Semitones", "st", -12.0, 12.0, 0.0, 1.0),
            num("cents", "Fine tune", "cents", -100.0, 100.0, 0.0, 1.0),
        ],
    }
}

pub struct PitchShift {
    factor: f64,
    rate: u32,
    st: Option<Stretcher>,
    stretched: Vec<f32>,
    pos: f64,
    out: VecDeque<f32>,
    tmp: Vec<f32>,
    latency: usize,
}

impl PitchShift {
    pub fn new(p: &Params, rate: u32) -> Self {
        let semis = p.get("semitones") + p.get("cents") / 100.0;
        let factor = 2f64.powf(semis / 12.0).clamp(0.5, 2.0);
        // Enough to cover the stretch's frame and search, at any factor.
        let latency = if (factor - 1.0).abs() < 1e-6 {
            0
        } else {
            (f64::from(rate) * 0.075) as usize
        };
        let mut me = Self {
            factor,
            rate,
            st: None,
            stretched: Vec::new(),
            pos: 0.0,
            out: VecDeque::new(),
            tmp: Vec::new(),
            latency,
        };
        me.reset();
        me
    }
}

/// Cubic interpolation through four samples, `t` between the middle two.
fn cubic(a: f32, b: f32, c: f32, d: f32, t: f32) -> f32 {
    let m1 = (c - a) * 0.5;
    let m2 = (d - b) * 0.5;
    let t2 = t * t;
    let t3 = t2 * t;
    (2.0 * t3 - 3.0 * t2 + 1.0) * b
        + (t3 - 2.0 * t2 + t) * m1
        + (-2.0 * t3 + 3.0 * t2) * c
        + (t3 - t2) * m2
}

impl Processor for PitchShift {
    fn process(&mut self, buf: &mut [f32]) {
        let Some(st) = self.st.as_mut() else {
            return;
        };
        st.push(buf);
        self.tmp.clear();
        st.pull(&mut self.tmp);
        self.stretched.extend_from_slice(&self.tmp);
        // Read the stretched audio `factor` times faster.
        let frames = self.stretched.len() / 2;
        while (self.pos as usize) + 3 < frames {
            let i = self.pos as usize;
            let t = (self.pos - i as f64) as f32;
            for c in 0..2 {
                let s = |k: usize| self.stretched[(i + k) * 2 + c];
                // One frame of history before `i` is kept for the curve.
                let a = if i > 0 { s(0) } else { s(1) };
                self.out.push_back(cubic(a, s(1), s(2), s(3), t));
            }
            self.pos += self.factor;
        }
        let used = (self.pos as usize).min(frames);
        if used > 0 {
            self.stretched.drain(..used * 2);
            self.pos -= used as f64;
        }
        for v in buf.iter_mut() {
            *v = self.out.pop_front().unwrap_or(0.0);
        }
    }
    fn latency(&self) -> usize {
        self.latency
    }
    fn reset(&mut self) {
        self.st = (self.latency > 0).then(|| Stretcher::new(self.factor, self.rate));
        self.stretched.clear();
        self.pos = 0.0;
        self.out.clear();
        // The stretch's first sample lines up one frame in (see `cubic`).
        let lead = self.latency.saturating_sub(1);
        self.out.extend(std::iter::repeat_n(0.0, lead * 2));
    }
}

#[cfg(test)]
mod tests {
    use super::super::build;
    use super::super::tests::slot;
    use std::f32::consts::TAU;

    fn crossings(semitones: f64) -> f32 {
        let rate = 48_000usize;
        let mut p = build(
            &slot("altaudio.pitch_shift", &[("semitones", semitones)]),
            rate as u32,
        )
        .unwrap();
        let mut b: Vec<f32> = (0..rate * 2)
            .flat_map(|i| {
                let v = (TAU * 440.0 * i as f32 / rate as f32).sin() * 0.5;
                [v, v]
            })
            .collect();
        for c in b.chunks_mut(2 * 512) {
            p.process(c);
        }
        // Rising zero crossings over the middle second.
        let left: Vec<f32> = b.iter().step_by(2).copied().collect();
        let seg = &left[rate / 2..rate * 3 / 2];
        seg.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count() as f32
    }

    #[test]
    fn shifts_pitch_and_keeps_speed() {
        let up = crossings(12.0);
        assert!((up - 880.0).abs() < 15.0, "an octave up: {up}");
        let down = crossings(-12.0);
        assert!((down - 220.0).abs() < 8.0, "an octave down: {down}");
        let third = crossings(4.0);
        assert!((third - 554.4).abs() < 12.0, "a major third up: {third}");
    }

    #[test]
    fn no_shift_passes_audio_untouched() {
        let mut p = build(&slot("altaudio.pitch_shift", &[]), 48_000).unwrap();
        assert_eq!(p.latency(), 0);
        let mut b = vec![0.25f32, -0.5, 0.75, 0.1];
        p.process(&mut b);
        assert_eq!(b, vec![0.25, -0.5, 0.75, 0.1]);
    }

    #[test]
    fn output_keeps_flowing_with_small_blocks() {
        let rate = 48_000usize;
        let mut p = build(
            &slot("altaudio.pitch_shift", &[("semitones", 3.0)]),
            rate as u32,
        )
        .unwrap();
        let mut b: Vec<f32> = (0..rate)
            .flat_map(|i| {
                let v = (TAU * 300.0 * i as f32 / rate as f32).sin() * 0.5;
                [v, v]
            })
            .collect();
        for c in b.chunks_mut(2 * 64) {
            p.process(c);
        }
        // After the delay there are no dropouts: no 10 ms stretch of silence.
        let left: Vec<f32> = b.iter().step_by(2).copied().collect();
        let after = &left[p.latency() + 4800..];
        let silent = after.windows(480).any(|w| w.iter().all(|v| v.abs() < 1e-4));
        assert!(!silent, "no gaps once running");
    }
}
