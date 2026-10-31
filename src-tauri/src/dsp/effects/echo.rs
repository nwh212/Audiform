// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Delay and Echo: repeats of the sound that fade away, with a tone
//! control that darkens the repeats and an optional ping-pong that
//! bounces them between the left and the right.
//!
//! Three kinds: Digital repeats cleanly; Tape wobbles (wow and
//! flutter), saturates and darkens each pass the way a tape loop does;
//! Analog is the darker, softly clipping sound of a bucket-brigade
//! delay. The time can follow the tempo as a note value. Low cut and
//! Tone filter the repeats inside the loop, Modulation sways the time,
//! and Drive pushes the repeats into saturation.
//!
//! Each side keeps a ring of the last two seconds. The repeats are read
//! back `time` ago, darkened by a one-pole low-pass, and fed back in,
//! so each repeat is `feedback` as loud as the one before. Changing the
//! time while playing glides the read point, so it bends the repeats
//! like tape instead of clicking.
//!
//! On a speaker bed every speaker has its own ring. Room spread passes a
//! share of each repeat on to the speakers either side in the room's
//! ring, and pulls their times a little apart, so the repeats spread out
//! around the listener. Ping-pong sends each repeat on to the next
//! speaker, so the echoes circle the room.

use super::{choice, num, toggle, EffectInfo, Params, Processor};

/// The longest delay, in seconds.
const MAX_SECONDS: f64 = 4.0;

pub const KINDS: [&str; 4] = ["Digital", "Tape", "Analog", "Simple"];
/// The Simple kind: each side's own time, early or late, by this much at most.
pub const SIMPLE_MS: f64 = 500.0;
/// Note values for a time that follows the tempo, and their length in beats.
pub const NOTES: [&str; 16] = [
    "1/64", "1/32T", "1/32", "1/16T", "1/16", "1/16D", "1/8T", "1/8", "1/8D", "1/4T", "1/4",
    "1/4D", "1/2", "1/2D", "1 bar", "2 bars",
];
const BEATS: [f64; 16] = [
    1.0 / 16.0,
    1.0 / 12.0,
    1.0 / 8.0,
    1.0 / 6.0,
    0.25,
    0.375,
    1.0 / 3.0,
    0.5,
    0.75,
    2.0 / 3.0,
    1.0,
    1.5,
    2.0,
    3.0,
    4.0,
    8.0,
];

/// The most a bed's rings' times are pulled apart by the spread (a share).
const MAX_APART: f64 = 0.06;

/// How fast the read point glides to a new time, per frame (about 40 ms).
const GLIDE: f64 = 0.0005;

pub fn echo_info() -> EffectInfo {
    EffectInfo {
        id: "altaudio.echo",
        name: "Delay and Echo",
        category: "Delay and Reverb",
        description: "Repeats the sound after a set time, each repeat quieter than the last. Digital, Tape or Analog; the time can follow the tempo. Tone darkens the repeats, and Ping-pong bounces them between left and right. Simple moves each side by its own time, early or late, with its own mix and polarity, for lining up or widening a sound.",
        params: vec![
            choice("kind", "Type", &KINDS, 0),
            num("time", "Delay time", "ms", 1.0, MAX_SECONDS * 1000.0, 350.0, 1.0).log().group("Time"),
            toggle("sync", "Follow tempo", false).group("Time"),
            choice("note", "Note", &NOTES, 7).group("Time"),
            num("tempo", "Tempo", "BPM", 30.0, 300.0, 120.0, 0.1).group("Time"),
            num("feedback", "Feedback", "%", 0.0, 120.0, 35.0, 1.0).group("Repeats"),
            toggle("pingpong", "Ping-pong", false).group("Repeats"),
            num("spread", "Room spread", "%", 0.0, 100.0, 50.0, 1.0).group("Repeats"),
            num("low_cut", "Low cut", "Hz", 20.0, 2000.0, 20.0, 1.0).log().group("Repeats"),
            num("tone", "Tone", "Hz", 500.0, 16_000.0, 6000.0, 10.0).log().group("Repeats"),
            num("depth", "Modulation", "%", 0.0, 100.0, 0.0, 1.0).group("Color"),
            num("rate", "Rate", "Hz", 0.05, 10.0, 0.6, 0.01).log().group("Color"),
            num("drive", "Drive", "%", 0.0, 100.0, 0.0, 1.0).group("Color"),
            num("dry", "Dry", "%", 0.0, 100.0, 100.0, 1.0).group("Output"),
            num("wet", "Echo level", "%", 0.0, 100.0, 30.0, 1.0).group("Output"),
            num("l_time", "Left time", "ms", -SIMPLE_MS, SIMPLE_MS, 0.0, 0.01).group("Simple"),
            num("l_mix", "Left mix", "%", 0.0, 100.0, 100.0, 1.0).group("Simple"),
            toggle("l_invert", "Left invert", false).group("Simple"),
            num("r_time", "Right time", "ms", -SIMPLE_MS, SIMPLE_MS, 0.0, 0.01).group("Simple"),
            num("r_mix", "Right mix", "%", 0.0, 100.0, 100.0, 1.0).group("Simple"),
            toggle("r_invert", "Right invert", false).group("Simple"),
            choice("units", "Time in", &["ms", "Samples"], 0).group("Simple"),
        ],
    }
}

pub struct Echo {
    rate: f64,
    ring: Vec<Vec<f32>>,
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
    lp_state: Vec<f32>,
    /// The low cut on the repeats (a one-pole high-pass): coefficient and memory.
    hp: f32,
    hp_state: Vec<f32>,
    kind: usize,
    dry: f32,
    /// Modulation: depth in frames, phase and step of the slow sway (wow),
    /// and of the fast one (flutter, tape only).
    depth: f64,
    phase: f64,
    step: f64,
    flutter: f64,
    drive: f32,
    spread: f32,
    /// On a speaker bed; None for stereo.
    bed: Option<Bed>,
    /// The Simple kind's settings, when it is picked.
    simple: Option<Simple>,
}

/// The Simple kind: per side, how far back to read (frames, the shared
/// hold-back included), how much of that to hear, and the polarity.
#[derive(Clone, Copy, PartialEq)]
struct Simple {
    /// Frames everything is held back so a side can come early.
    base: usize,
    read: [usize; 2],
    mix: [f32; 2],
    sign: [f32; 2],
}

/// The speakers a bed's rings work on.
struct Bed {
    names: Vec<String>,
    /// The channels in ring order (no LFE); ring `a` works on `chans[a]`.
    chans: Vec<usize>,
    /// Each ring's time, as a share of the set time: pulled apart by the spread.
    stretch: Vec<f64>,
    /// This frame's repeat per ring, before and after the filters.
    heard: Vec<f32>,
    back: Vec<f32>,
}

/// Soft saturation that leaves quiet sound almost as it is.
fn soft(x: f32, drive: f32) -> f32 {
    if drive <= 0.0 {
        return x;
    }
    let k = 1.0 + 6.0 * drive;
    (x * k).tanh() / k.tanh().max(1e-6) * (1.0 / (1.0 + 0.3 * drive))
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
        Self::with_rings(p, rate, 2)
    }

    /// A Delay and Echo over a speaker bed with channels named `names`.
    pub fn wide(p: &Params, rate: u32, names: &[String]) -> Self {
        let chans = super::wide::ring(names);
        let n = chans.len().max(1);
        let mut e = Self::with_rings(p, rate, n);
        e.bed = Some(Bed {
            names: names.to_vec(),
            chans,
            stretch: vec![1.0; n],
            heard: vec![0.0; n],
            back: vec![0.0; n],
        });
        e.set(p);
        e
    }

    fn with_rings(p: &Params, rate: u32, rings: usize) -> Self {
        let n = (MAX_SECONDS * f64::from(rate)) as usize + 4;
        let mut e = Self {
            rate: f64::from(rate),
            ring: vec![vec![0.0; n]; rings],
            at: 0,
            cur: 0.0,
            want: 0.0,
            feedback: 0.0,
            wet: 0.0,
            pingpong: false,
            lp: 0.0,
            lp_state: vec![0.0; rings],
            hp: 0.0,
            hp_state: vec![0.0; rings],
            kind: 0,
            dry: 1.0,
            depth: 0.0,
            phase: 0.0,
            step: 0.0,
            flutter: 0.0,
            drive: 0.0,
            spread: 0.5,
            bed: None,
            simple: None,
        };
        e.set(p);
        e.cur = e.want;
        e
    }

    fn set(&mut self, p: &Params) {
        let longest = (self.ring[0].len() - 2) as f64;
        self.kind = p.index("kind").min(KINDS.len() - 1);
        let ms = if p.on("sync") {
            let beats = BEATS[p.index("note").min(BEATS.len() - 1)];
            beats * 60_000.0 / p.get("tempo").clamp(30.0, 300.0)
        } else {
            p.get("time")
        };
        // Room for the sway on both sides, and on a bed for the spread.
        let sway =
            p.get("depth") / 100.0 * if self.kind == 1 { 8.0 } else { 4.0 } / 1000.0 * self.rate;
        self.spread = (p.get("spread") / 100.0).clamp(0.0, 1.0) as f32;
        let apart = if self.bed.is_some() {
            1.0 + MAX_APART * f64::from(self.spread)
        } else {
            1.0
        };
        self.want = (ms / 1000.0 * self.rate).clamp(1.0 + sway, (longest - sway - 2.0) / apart);
        if let Some(bed) = self.bed.as_mut() {
            let n = bed.stretch.len();
            for (a, t) in bed.stretch.iter_mut().enumerate() {
                // Spread evenly from one end of the range to the other,
                // in an order that keeps neighbours apart.
                let at = if n > 1 {
                    ((a * 5) % n) as f64 / (n - 1) as f64
                } else {
                    0.0
                };
                *t = 1.0 + MAX_APART * f64::from(self.spread) * at;
            }
        }
        self.feedback = (p.get("feedback") / 100.0) as f32;
        self.wet = (p.get("wet") / 100.0) as f32;
        self.dry = (p.get("dry") / 100.0) as f32;
        self.pingpong = p.on("pingpong");
        // Analog repeats are darker still.
        let tone = if self.kind == 2 {
            p.get("tone").min(5500.0)
        } else {
            p.get("tone")
        };
        self.lp = (-std::f64::consts::TAU * tone / self.rate).exp() as f32;
        self.hp = (-std::f64::consts::TAU * p.get("low_cut") / self.rate).exp() as f32;
        self.depth = sway;
        self.step = std::f64::consts::TAU * p.get("rate") / self.rate;
        let base = (p.get("drive") / 100.0) as f32;
        // Tape and analog always saturate a little.
        self.drive = match self.kind {
            1 => 0.2 + 0.8 * base,
            2 => 0.1 + 0.9 * base,
            _ => base,
        };
        self.simple = (self.kind == 3).then(|| {
            let frames = |ms: f64| (ms / 1000.0 * self.rate).round() as i64;
            let (l, r) = (frames(p.get("l_time")), frames(p.get("r_time")));
            // On a bed nothing is held back (the other speakers would fall
            // out of time), so a side can only come late there.
            let base = if self.bed.is_some() {
                0
            } else {
                (-l.min(r)).max(0)
            };
            let read = |t: i64| (t + base).max(0) as usize;
            let mix = |k: &str| (p.get(k) / 100.0).clamp(0.0, 1.0) as f32;
            let sign = |k: &str| if p.on(k) { -1.0 } else { 1.0 };
            Simple {
                base: base as usize,
                read: [read(l), read(r)],
                mix: [mix("l_mix"), mix("r_mix")],
                sign: [sign("l_invert"), sign("r_invert")],
            }
        });
    }
}

impl Echo {
    /// This frame's delay in frames, gliding to a new time, with the
    /// modulation: a slow sway, and for tape a fast flutter too.
    fn sway(&mut self) -> f64 {
        self.cur += (self.want - self.cur) * GLIDE;
        if (self.want - self.cur).abs() < 0.01 {
            self.cur = self.want;
        }
        let mut d = self.cur;
        if self.depth > 0.0 {
            self.phase = (self.phase + self.step) % std::f64::consts::TAU;
            let mut m = self.phase.sin();
            if self.kind == 1 {
                self.flutter = (self.flutter + self.step * 11.0) % std::f64::consts::TAU;
                m = 0.8 * m + 0.2 * self.flutter.sin();
            }
            d += self.depth * m;
        }
        d
    }

    /// Processes interleaved frames over the bed's speakers.
    fn process_bed(&mut self, buf: &mut [f32]) {
        let Some(mut bed) = self.bed.take() else {
            return;
        };
        let ch = bed.names.len().max(1);
        let rings = bed.chans.len();
        let n = self.ring[0].len();
        let drive = if self.drive > 0.0 || self.feedback > 0.95 {
            self.drive.max(0.05)
        } else {
            0.0
        };
        let side = 0.5 * self.spread;
        for f in buf.chunks_exact_mut(ch) {
            let d = self.sway();
            for a in 0..rings {
                let pos = self.at as f64 + n as f64 - d * bed.stretch[a];
                let whole = pos.floor();
                let frac = (pos - whole) as f32;
                let i0 = whole as usize % n;
                let y = tap(&self.ring[a], i0, (i0 + 1) % n, frac);
                bed.heard[a] = y;
                self.lp_state[a] = y + self.lp * (self.lp_state[a] - y);
                let mut v = self.lp_state[a];
                if self.hp > 0.0 && self.hp < 0.9999 {
                    self.hp_state[a] = v + self.hp * (self.hp_state[a] - v);
                    v -= self.hp_state[a];
                }
                bed.back[a] = if drive > 0.0 { soft(v, drive) } else { v };
            }
            for a in 0..rings {
                let prev = bed.back[(a + rings - 1) % rings];
                let next = bed.back[(a + 1) % rings];
                // Ping-pong: each repeat moves on one speaker round the room.
                let back = if self.pingpong {
                    prev
                } else if rings > 1 {
                    bed.back[a] * (1.0 - self.spread) + (prev + next) * side
                } else {
                    bed.back[a]
                };
                let x = f[bed.chans[a]];
                self.ring[a][self.at] = flush(x + self.feedback * back);
                f[bed.chans[a]] = self.dry * x + self.wet * bed.heard[a];
            }
            self.at = (self.at + 1) % n;
        }
        self.bed = Some(bed);
    }
}

impl Echo {
    /// The Simple kind: each side read back by its own time, mixed with
    /// the sound held back by the shared hold-back, its polarity flipped
    /// when asked. On a bed, the speakers named L... take the left
    /// settings and R... the right; the others pass as they are.
    fn process_simple(&mut self, buf: &mut [f32], sm: Simple) {
        let n = self.ring[0].len();
        let (ch, sides): (usize, Vec<(usize, usize, Option<usize>)>) = match &self.bed {
            Some(b) => (
                b.names.len().max(1),
                b.chans
                    .iter()
                    .enumerate()
                    .map(|(a, &c)| {
                        let side = match b.names[c].chars().next() {
                            Some('L') => Some(0),
                            Some('R') => Some(1),
                            _ => None,
                        };
                        (a, c, side)
                    })
                    .collect(),
            ),
            None => (2, vec![(0, 0, Some(0)), (1, 1, Some(1))]),
        };
        for f in buf.chunks_exact_mut(ch) {
            for &(a, c, side) in &sides {
                let Some(k) = side else { continue };
                self.ring[a][self.at] = f[c];
                let back = |d: usize| (self.at + n - d.min(n - 1)) % n;
                let dry = self.ring[a][back(sm.base)];
                let wet = self.ring[a][back(sm.read[k])];
                f[c] = (dry * (1.0 - sm.mix[k]) + wet * sm.mix[k]) * sm.sign[k];
            }
            self.at = (self.at + 1) % n;
        }
    }
}

impl Processor for Echo {
    fn process(&mut self, buf: &mut [f32]) {
        if let Some(sm) = self.simple {
            self.process_simple(buf, sm);
            return;
        }
        if self.bed.is_some() {
            self.process_bed(buf);
            return;
        }
        let n = self.ring[0].len();
        for f in buf.as_chunks_mut::<2>().0 {
            let d = self.sway();
            let pos = self.at as f64 + n as f64 - d;
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
            let (mut fl, mut fr) = (self.lp_state[0], self.lp_state[1]);
            if self.hp > 0.0 && self.hp < 0.9999 {
                // Low cut: what the high-pass memory tracks is taken away.
                self.hp_state[0] = fl + self.hp * (self.hp_state[0] - fl);
                self.hp_state[1] = fr + self.hp * (self.hp_state[1] - fr);
                fl -= self.hp_state[0];
                fr -= self.hp_state[1];
            }
            if self.drive > 0.0 || self.feedback > 0.95 {
                // Saturation keeps feedback past 100% from running away.
                let dr_ = self.drive.max(0.05);
                fl = soft(fl, dr_);
                fr = soft(fr, dr_);
            }
            let (xl, xr) = (f[0], f[1]);
            let (wl, wr) = if self.pingpong {
                // The input enters on the left; each repeat crosses sides.
                ((xl + xr) * 0.5 + self.feedback * fr, self.feedback * fl)
            } else {
                (xl + self.feedback * fl, xr + self.feedback * fr)
            };
            self.ring[0][self.at] = flush(wl);
            self.ring[1][self.at] = flush(wr);
            f[0] = self.dry * xl + self.wet * dl;
            f[1] = self.dry * xr + self.wet * dr;
            self.at = (self.at + 1) % n;
        }
    }

    fn reset(&mut self) {
        for r in &mut self.ring {
            r.fill(0.0);
        }
        self.lp_state.fill(0.0);
        self.hp_state.fill(0.0);
        self.at = 0;
        self.cur = self.want;
    }

    fn update(&mut self, p: &Params) -> bool {
        let was = self.latency();
        self.set(p);
        // A new hold-back changes the latency: built again.
        self.latency() == was
    }

    fn latency(&self) -> usize {
        self.simple.map_or(0, |s| s.base)
    }

    fn channels(&self) -> (Vec<String>, Vec<String>) {
        match &self.bed {
            Some(b) => (b.names.clone(), b.names.clone()),
            None => (super::routing::stereo(), super::routing::stereo()),
        }
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
    fn follows_the_tempo_and_every_kind_stays_finite() {
        // A quarter note at 120 BPM is half a second.
        let p = params(&[
            ("sync", 1.0),
            ("note", 10.0),
            ("tempo", 120.0),
            ("wet", 100.0),
            ("feedback", 0.0),
        ]);
        let out = run(&p, impulse(30_000, 1.0, 1.0));
        assert!(near(&out, 24_000, 0) > 0.9, "the repeat lands on the beat");
        for (kind, name) in KINDS.iter().enumerate() {
            let p = params(&[
                ("kind", kind as f64),
                ("feedback", 120.0),
                ("depth", 60.0),
                ("drive", 80.0),
                ("wet", 100.0),
            ]);
            let out = run(&p, impulse(200_000, 1.0, 1.0));
            assert!(
                out.iter().all(|v| v.is_finite() && v.abs() < 3.0),
                "{} runs away",
                name
            );
        }
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

    #[test]
    fn on_a_bed_ping_pong_circles_the_room_and_spread_reaches_neighbours() {
        let names: Vec<String> = ["L", "R", "C", "LFE", "Ls", "Rs"]
            .iter()
            .map(|x| (*x).to_string())
            .collect();
        let at = |b: &[f32], frame: usize, c: usize| {
            (frame - 400..frame + 400)
                .map(|f| b[f * 6 + c].abs())
                .fold(0.0, f32::max)
        };
        // Ring order from the front, clockwise: C, R, Rs, Ls, L.
        let p = params(&[
            ("time", 100.0),
            ("feedback", 90.0),
            ("tone", 16_000.0),
            ("wet", 100.0),
            ("dry", 0.0),
            ("pingpong", 1.0),
            ("spread", 0.0),
        ]);
        let mut e = Echo::wide(&p, R, &names);
        let mut b = vec![0.0f32; 30_000 * 6];
        b[2] = 1.0;
        for c in b.chunks_mut(6 * 300) {
            e.process(c);
        }
        assert!(at(&b, 4800, 2) > 0.5, "first repeat in the center");
        assert!(
            at(&b, 9600, 1) > 0.3 && at(&b, 9600, 2) < 1e-4,
            "then right"
        );
        assert!(at(&b, 14_400, 5) > 0.2, "then right surround");
        assert!(at(&b, 19_200, 4) > 0.1, "then left surround");
        assert!(
            b.as_chunks::<6>().0.iter().all(|f| f[3] == 0.0),
            "LFE left alone"
        );

        let p = params(&[
            ("time", 100.0),
            ("feedback", 80.0),
            ("wet", 100.0),
            ("dry", 0.0),
            ("spread", 100.0),
        ]);
        let mut e = Echo::wide(&p, R, &names);
        let mut b = vec![0.0f32; 40_000 * 6];
        b[2] = 1.0;
        for c in b.chunks_mut(6 * 300) {
            e.process(c);
        }
        let energy = |c: usize| {
            b.as_chunks::<6>()
                .0
                .iter()
                .map(|f| f[c] * f[c])
                .sum::<f32>()
        };
        assert!(
            energy(0) > 0.01 && energy(1) > 0.01,
            "repeats spread to L and R"
        );
        assert!(
            energy(4) > 0.001 && energy(5) > 0.001,
            "and on round the room"
        );
        assert!(b.iter().all(|v| v.is_finite() && v.abs() < 3.0));
    }

    #[test]
    fn simple_moves_each_side_early_or_late_and_flips_polarity() {
        // Left 2 ms late, right 1 ms early: everything is held back 1 ms
        // (48 frames), so right comes out at 0 and left at 96 + 48 = 144.
        let p = params(&[
            ("kind", 3.0),
            ("l_time", 2.0),
            ("r_time", -1.0),
            ("r_invert", 1.0),
        ]);
        let mut e = Echo::new(&p, R);
        assert_eq!(super::super::Processor::latency(&e), 48);
        let mut b = impulse(400, 1.0, 1.0);
        e.process(&mut b);
        assert!((b[144 * 2] - 1.0).abs() < 1e-6, "left late");
        assert!((b[1] + 1.0).abs() < 1e-6, "right early and flipped");
        let rest: f32 = b.iter().map(|v| v.abs()).sum();
        assert!((rest - 2.0).abs() < 1e-5, "nothing else");
        // Half mix: half held-back dry, half moved.
        let p = params(&[("kind", 3.0), ("l_time", 1.0), ("l_mix", 50.0)]);
        let mut e = Echo::new(&p, R);
        let mut b = impulse(200, 1.0, 0.0);
        e.process(&mut b);
        assert!((b[0] - 0.5).abs() < 1e-6 && (b[48 * 2] - 0.5).abs() < 1e-6);
    }
}
