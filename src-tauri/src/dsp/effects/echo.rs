// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Delay and Echo: repeats of the sound that fade away, with a tone
//! control that darkens the repeats and an optional ping-pong that
//! bounces them between the left and the right.
//!
//! Each side keeps a ring of the last two seconds. The repeats are read
//! back `time` ago, darkened by a one-pole low-pass, and fed back in,
//! so each repeat is `feedback` as loud as the one before. Changing the
//! time while playing glides the read point, so it bends the repeats
//! like tape instead of clicking.

use super::{num, toggle, EffectInfo, Params, Processor};

/// The longest delay, in seconds.
const MAX_SECONDS: f64 = 2.0;

/// How fast the read point glides to a new time, per frame (about 40 ms).
const GLIDE: f64 = 0.0005;

pub fn echo_info() -> EffectInfo {
    EffectInfo {
        id: "altaudio.echo",
        name: "Delay and Echo",
        category: "Delay and Reverb",
        description: "Repeats the sound after a set time, each repeat quieter than the last. Tone darkens the repeats, and Ping-pong bounces them between left and right.",
        params: vec![
            num("time", "Delay time", "ms", 1.0, 2000.0, 350.0, 1.0).log(),
            num("feedback", "Feedback", "%", 0.0, 95.0, 35.0, 1.0),
            num("tone", "Tone", "Hz", 500.0, 16_000.0, 6000.0, 10.0).log(),
            num("wet", "Echo level", "%", 0.0, 100.0, 30.0, 1.0),
            toggle("pingpong", "Ping-pong", false),
        ],
    }
}

pub struct Echo {
    rate: f64,
    ring: [Vec<f32>; 2],
    at: usize,
    /// The delay in frames the read point is at now, and the one it
    /// glides toward.
    cur: f64,
    want: f64,
    feedback: f32,
    wet: f32,
    pingpong: bool,
    /// The low-pass on the repeats: its coefficient and each side's memory.
    lp: f32,
    lp_state: [f32; 2],
}

/// Reads between two neighbouring frames of a ring.
fn tap(ring: &[f32], i0: usize, i1: usize, frac: f32) -> f32 {
    ring[i0] * (1.0 - frac) + ring[i1] * frac
}

/// Keeps tiny leftovers from slowing the processor down.
fn flush(x: f32) -> f32 {
    if x.abs() < 1e-20 {
        0.0
    } else {
        x
    }
}

impl Echo {
    pub fn new(p: &Params, rate: u32) -> Self {
        let n = (MAX_SECONDS * f64::from(rate)) as usize + 4;
        let mut e = Self {
            rate: f64::from(rate),
            ring: [vec![0.0; n], vec![0.0; n]],
            at: 0,
            cur: 0.0,
            want: 0.0,
            feedback: 0.0,
            wet: 0.0,
            pingpong: false,
            lp: 0.0,
            lp_state: [0.0; 2],
        };
        e.set(p);
        e.cur = e.want;
        e
    }

    fn set(&mut self, p: &Params) {
        let longest = (self.ring[0].len() - 2) as f64;
        self.want = (p.get("time") / 1000.0 * self.rate).clamp(1.0, longest);
        self.feedback = (p.get("feedback") / 100.0) as f32;
        self.wet = (p.get("wet") / 100.0) as f32;
        self.pingpong = p.on("pingpong");
        self.lp = (-std::f64::consts::TAU * p.get("tone") / self.rate).exp() as f32;
    }
}

impl Processor for Echo {
    fn process(&mut self, buf: &mut [f32]) {
        let n = self.ring[0].len();
        for f in buf.as_chunks_mut::<2>().0 {
            self.cur += (self.want - self.cur) * GLIDE;
            if (self.want - self.cur).abs() < 0.01 {
                self.cur = self.want;
            }
            let pos = self.at as f64 + n as f64 - self.cur;
            let whole = pos.floor();
            let frac = (pos - whole) as f32;
            let i0 = whole as usize % n;
            let i1 = (i0 + 1) % n;
            let dl = tap(&self.ring[0], i0, i1, frac);
            let dr = tap(&self.ring[1], i0, i1, frac);
            // The repeats that go back in are darkened; the ones you hear
            // are not, so the first repeat is a clean copy.
            self.lp_state[0] = dl + self.lp * (self.lp_state[0] - dl);
            self.lp_state[1] = dr + self.lp * (self.lp_state[1] - dr);
            let (fl, fr) = (self.lp_state[0], self.lp_state[1]);
            let (xl, xr) = (f[0], f[1]);
            let (wl, wr) = if self.pingpong {
                // The input enters on the left; each repeat crosses sides.
                ((xl + xr) * 0.5 + self.feedback * fr, self.feedback * fl)
            } else {
                (xl + self.feedback * fl, xr + self.feedback * fr)
            };
            self.ring[0][self.at] = flush(wl);
            self.ring[1][self.at] = flush(wr);
            f[0] = xl + self.wet * dl;
            f[1] = xr + self.wet * dr;
            self.at = (self.at + 1) % n;
        }
    }

    fn reset(&mut self) {
        for r in &mut self.ring {
            r.fill(0.0);
        }
        self.lp_state = [0.0; 2];
        self.at = 0;
        self.cur = self.want;
    }

    fn update(&mut self, p: &Params) -> bool {
        self.set(p);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    const R: u32 = 48_000;

    fn params(over: &[(&str, f64)]) -> Params {
        let saved: BTreeMap<String, f64> =
            over.iter().map(|(k, v)| ((*k).to_string(), *v)).collect();
        Params::resolve(&echo_info(), &saved)
    }

    /// `frames` of silence with one frame of sound at the start.
    fn impulse(frames: usize, l: f32, r: f32) -> Vec<f32> {
        let mut b = vec![0.0; frames * 2];
        b[0] = l;
        b[1] = r;
        b
    }

    fn run(p: &Params, mut b: Vec<f32>) -> Vec<f32> {
        let mut e = Echo::new(p, R);
        for chunk in b.chunks_mut(2 * 333) {
            e.process(chunk);
        }
        b
    }

    /// The largest value of one side near `frame` (a frame either way).
    fn near(b: &[f32], frame: usize, side: usize) -> f32 {
        (frame - 2..=frame + 2)
            .map(|f| b[f * 2 + side].abs())
            .fold(0.0, f32::max)
    }

    #[test]
    fn repeats_come_after_the_delay_time_and_fade() {
        let p = params(&[
            ("time", 100.0),
            ("feedback", 50.0),
            ("tone", 16_000.0),
            ("wet", 100.0),
        ]);
        let out = run(&p, impulse(20_000, 1.0, 1.0));
        assert!((out[0] - 1.0).abs() < 1e-6, "the dry sound passes");
        let (e1, e2, e3) = (
            near(&out, 4800, 0),
            near(&out, 9600, 0),
            near(&out, 14_400, 0),
        );
        assert!(e1 > 0.99, "the first repeat is a clean copy: {e1}");
        assert!(e2 > 0.3 && e2 < 0.5, "the second is about half: {e2}");
        assert!(e3 > 0.0 && e3 < e2, "each repeat is quieter: {e3} {e2}");
        assert!(near(&out, 2400, 0) < 1e-6, "nothing between the repeats");
    }

    #[test]
    fn no_echo_level_leaves_the_sound_alone() {
        let p = params(&[("wet", 0.0), ("feedback", 80.0)]);
        let input: Vec<f32> = (0..10_000)
            .flat_map(|i| {
                let v = (i as f32 * 0.05).sin() * 0.4;
                [v, -v * 0.5]
            })
            .collect();
        let out = run(&p, input.clone());
        for (a, b) in out.iter().zip(&input) {
            assert!((a - b).abs() < 1e-6);
        }
    }

    #[test]
    fn ping_pong_bounces_between_the_sides() {
        let p = params(&[
            ("time", 50.0),
            ("feedback", 80.0),
            ("tone", 16_000.0),
            ("wet", 100.0),
            ("pingpong", 1.0),
        ]);
        let out = run(&p, impulse(12_000, 1.0, 0.0));
        assert!(near(&out, 2400, 0) > 0.3, "first repeat on the left");
        assert!(near(&out, 2400, 1) < 1e-4, "and not on the right");
        assert!(near(&out, 4800, 1) > 0.1, "second repeat on the right");
        assert!(near(&out, 4800, 0) < 1e-4, "and not on the left");
    }

    #[test]
    fn a_new_time_glides_there_and_the_tail_stays_finite() {
        let mut e = Echo::new(&params(&[("time", 100.0), ("feedback", 95.0)]), R);
        let mut b = impulse(100_000, 1.0, 1.0);
        for chunk in b.chunks_mut(2 * 500) {
            e.process(chunk);
        }
        assert!(e.update(&params(&[("time", 200.0), ("feedback", 95.0)])));
        let mut c = impulse(100_000, 1.0, 1.0);
        for chunk in c.chunks_mut(2 * 500) {
            e.process(chunk);
        }
        assert!((e.cur - 9600.0).abs() < 0.5, "{}", e.cur);
        assert!(b.iter().chain(&c).all(|v| v.is_finite() && v.abs() < 4.0));
        e.reset();
        let mut quiet = vec![0.0; 2000];
        e.process(&mut quiet);
        assert!(quiet.iter().all(|v| *v == 0.0), "reset empties the tail");
    }
}
