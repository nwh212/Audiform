// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

//! Modulation: four effects that move a copy of the sound against itself,
//! picked from tabs in one window.
//!
//! - Chorus: a few copies, each delayed 5 to 40 ms by a slowly swinging
//!   amount, so they drift in pitch and time like several players.
//! - Flanger: one copy delayed under 10 ms, swept, fed back: a comb of
//!   notches that sweeps up and down (the jet sound).
//! - Phaser: the sound through a chain of all-pass filters whose turning
//!   point sweeps, mixed with the dry sound: moving notches without delay.
//! - Double Track: a tape-style doubler. The source and its copy each have
//!   their own pan, drive, level, mute and polarity; the copy plays up to
//!   20 ms early or late (Offset), its time swaying by an LFO (range, rate,
//!   shape: sine, triangle or random), as a second take or a second tape
//!   machine would. The input can be the left, both or the right side, and
//!   the result can be heard in mono. The source is held back 20 ms so the
//!   copy can come early (the effect's latency).
//!
//! Rate, depth, feedback, stereo width and the dry and wet levels are
//! shared; each kind has its own time setting.
//!
//! On a speaker bed every speaker has its own moving copy, its sweep a
//! step further round the turn than the speaker before it in the room's
//! ring, so the movement travels around the listener. Room spread feeds
//! each copy a share of the neighbouring speakers' sound, so a part in
//! front shimmers out into the sides and rear too.

use super::{choice, db_to_gain, num, toggle, EffectInfo, Params, Processor};

pub const ID: &str = "altaudio.modulation";
pub const KINDS: [&str; 4] = ["Chorus", "Flanger", "Phaser", "Double Track"];
const STAGES: [&str; 6] = ["2", "4", "6", "8", "10", "12"];
/// Longest delay any kind reads, with room for its swing.
const MAX_MS: f64 = 80.0;
/// Double Track: how early or late the copy can be, and its LFO shapes.
pub const ADT_MS: f64 = 20.0;
const LFO_SHAPES: [&str; 3] = ["Sine", "Triangle", "Random"];

pub fn modulation_info() -> EffectInfo {
    EffectInfo {
        id: ID,
        name: "Modulation",
        category: "Modulation",
        description: "Chorus, Flanger, Phaser and Double Track: a moving copy of the sound mixed back with it, for width, shimmer, sweeping notches or a second take.",
        params: vec![
            choice("kind", "Type", &KINDS, 0),
            num("rate", "Rate", "Hz", 0.02, 10.0, 0.6, 0.01).log(),
            num("depth", "Depth", "%", 0.0, 100.0, 50.0, 1.0),
            num("feedback", "Feedback", "%", -95.0, 95.0, 0.0, 1.0),
            num("width", "Stereo width", "%", 0.0, 100.0, 100.0, 1.0),
            num("spread", "Room spread", "%", 0.0, 100.0, 50.0, 1.0),
            num("voices", "Voices", "", 2.0, 6.0, 3.0, 1.0).group("Chorus"),
            num("chorus_delay", "Delay", "ms", 5.0, 40.0, 18.0, 0.1).group("Chorus"),
            num("flange_delay", "Delay", "ms", 0.1, 10.0, 1.5, 0.01).log().group("Flanger"),
            choice("stages", "Stages", &STAGES, 2).group("Phaser"),
            num("center", "Center", "Hz", 100.0, 5000.0, 700.0, 1.0).log().group("Phaser"),
            num("adt_offset", "Offset", "ms", -ADT_MS, ADT_MS, 12.0, 0.01).group("Double Track"),
            num("lfo_range", "LFO range", "ms", 0.0, 5.0, 1.0, 0.01).group("Double Track"),
            num("lfo_rate", "LFO rate", "Hz", 0.05, 10.0, 0.4, 0.01).log().group("Double Track"),
            choice("lfo_shape", "LFO shape", &LFO_SHAPES, 2).group("Double Track"),
            choice("adt_input", "Input", &["Left", "Left + Right", "Right"], 1).group("Double Track"),
            toggle("adt_mono", "Mono", false).group("Double Track"),
            num("src_pan", "Source pan", "", -100.0, 100.0, 0.0, 1.0).group("Source"),
            num("src_drive", "Source drive", "%", 0.0, 100.0, 0.0, 1.0).group("Source"),
            num("src_level", "Source level", "dB", -48.0, 12.0, 0.0, 0.1).group("Source"),
            toggle("src_mute", "Source mute", false).group("Source"),
            toggle("src_invert", "Source invert", false).group("Source"),
            num("adt_pan", "Copy pan", "", -100.0, 100.0, 0.0, 1.0).group("Copy"),
            num("adt_drive", "Copy drive", "%", 0.0, 100.0, 0.0, 1.0).group("Copy"),
            num("adt_level", "Copy level", "dB", -48.0, 12.0, 0.0, 0.1).group("Copy"),
            toggle("adt_mute", "Copy mute", false).group("Copy"),
            toggle("adt_invert", "Copy invert", false).group("Copy"),
            num("dry", "Dry", "%", 0.0, 100.0, 100.0, 1.0).group("Output"),
            num("wet", "Wet", "%", 0.0, 100.0, 50.0, 1.0).group("Output"),
        ],
    }
}

/// A delay line read at a fractional point.
struct Line {
    buf: Vec<f32>,
    at: usize,
}

/// A sine for the sweeps, a few thousandths off at most (far under what
/// a sweep's shape shows) and several times quicker than the exact one,
/// which was taken once per voice per sample.
fn lfo_sin(x: f64) -> f64 {
    use std::f64::consts::{PI, TAU};
    // Into -PI..PI (phases here stay within a few turns, so a loop beats
    // a remainder), then fold onto -PI/2..PI/2 where the polynomial holds.
    let mut x = x;
    while x > PI {
        x -= TAU;
    }
    while x < -PI {
        x += TAU;
    }
    if x > PI / 2.0 {
        x = PI - x;
    } else if x < -PI / 2.0 {
        x = -PI - x;
    }
    let x2 = x * x;
    x * (1.0 + x2 * (-1.0 / 6.0 + x2 * (1.0 / 120.0 + x2 * (-1.0 / 5040.0 + x2 / 362_880.0))))
}

impl Line {
    fn new(len: usize) -> Self {
        Self {
            buf: vec![0.0; len.max(4)],
            at: 0,
        }
    }
    fn push(&mut self, x: f32) {
        self.buf[self.at] = x;
        self.at += 1;
        if self.at == self.buf.len() {
            self.at = 0;
        }
    }
    /// The sound `d` frames ago (at least 1), between samples by a cubic.
    fn read(&self, d: f64) -> f32 {
        let n = self.buf.len();
        let d = d.clamp(1.0, (n - 3) as f64);
        let pos = self.at as f64 + n as f64 - d;
        let i = pos.floor();
        let t = (pos - i) as f32;
        // `pos` is in `at + 3..at + n`, so `i - 1 + k` stays under 2n.
        let i = i as usize + n - 1;
        let s = |k: usize| {
            let mut j = i + k;
            if j >= n {
                j -= n;
            }
            if j >= n {
                j -= n;
            }
            self.buf[j]
        };
        let (y0, y1, y2, y3) = (s(0), s(1), s(2), s(3));
        // Catmull-Rom between y1 and y2.
        y1 + 0.5
            * t
            * (y2 - y0
                + t * (2.0 * y0 - 5.0 * y1 + 4.0 * y2 - y3 + t * (3.0 * (y1 - y2) + y3 - y0)))
    }
    fn clear(&mut self) {
        self.buf.fill(0.0);
    }
}

/// A first-order all-pass with a moving coefficient.
#[derive(Clone, Copy, Default)]
struct Stage {
    x1: f32,
    y1: f32,
}

impl Stage {
    fn run(&mut self, x: f32, a: f32) -> f32 {
        let y = a * x + self.x1 - a * self.y1;
        self.x1 = x;
        self.y1 = if y.abs() < 1e-20 { 0.0 } else { y };
        y
    }
}

/// Smooth random movement for Double Track: a slow random walk, low-passed.
struct Wander {
    seed: u32,
    target: f64,
    value: f64,
    smooth: f64,
    left: usize,
    every: usize,
}

impl Wander {
    fn new(seed: u32, rate: f64, sr: f64) -> Self {
        let mut w = Self {
            seed: seed.max(1),
            target: 0.0,
            value: 0.0,
            smooth: 0.0,
            left: 0,
            every: 1,
        };
        w.set_rate(rate, sr);
        w
    }
    fn set_rate(&mut self, rate: f64, sr: f64) {
        // A new aim a few times per cycle; glide toward it.
        self.every = ((sr / (rate.max(0.02) * 4.0)) as usize).max(1);
        self.smooth = 1.0 - (-std::f64::consts::TAU * rate.max(0.02) * 2.0 / sr).exp();
    }
    fn next(&mut self) -> f64 {
        if self.left == 0 {
            self.left = self.every;
            self.seed ^= self.seed << 13;
            self.seed ^= self.seed >> 17;
            self.seed ^= self.seed << 5;
            self.target = f64::from(self.seed) / f64::from(u32::MAX) * 2.0 - 1.0;
        }
        self.left -= 1;
        self.value += (self.target - self.value) * self.smooth;
        self.value
    }
}

pub struct Modulation {
    sr: f64,
    kind: usize,
    lines: Vec<Line>,
    stages: Vec<[Stage; 12]>,
    n_stages: usize,
    phase: f64,
    step: f64,
    depth: f64,
    feedback: f32,
    width: f32,
    voices: usize,
    chorus_ms: f64,
    flange_ms: f64,
    center: f64,
    dt_ms: f64,
    wander: f64,
    walk: Vec<Wander>,
    fb: Vec<f32>,
    dry: f32,
    wet: f32,
    spread: f32,
    /// On a speaker bed; None for stereo.
    bed: Option<Bed>,
    /// Double Track's settings, when it is the kind.
    adt: Option<AdtSet>,
    /// Double Track's random LFO.
    adt_walk: Wander,
    adt_step: f64,
    adt_phase: f64,
}

/// One side of Double Track (the source or its copy): pan gains, drive,
/// and level with mute and polarity folded in.
#[derive(Clone, Copy)]
struct AdtSide {
    pan: [f32; 2],
    drive: f32,
    gain: f32,
}

impl AdtSide {
    fn from(p: &Params, k: &str) -> Self {
        let pan = (p.get(&format!("{k}_pan")) / 100.0).clamp(-1.0, 1.0);
        let a = (pan + 1.0) * std::f64::consts::FRAC_PI_4;
        let mut gain = db_to_gain(p.get(&format!("{k}_level")));
        if p.on(&format!("{k}_mute")) {
            gain = 0.0;
        }
        if p.on(&format!("{k}_invert")) {
            gain = -gain;
        }
        Self {
            pan: [a.cos() as f32, a.sin() as f32],
            drive: (p.get(&format!("{k}_drive")) / 100.0) as f32,
            gain,
        }
    }

    /// Soft tape saturation, the same level at low drive.
    fn color(&self, x: f32) -> f32 {
        if self.drive <= 0.0 {
            return x;
        }
        let k = 1.0 + 5.0 * self.drive;
        (x * k).tanh() / k.tanh()
    }
}

#[derive(Clone, Copy)]
struct AdtSet {
    src: AdtSide,
    copy: AdtSide,
    offset_ms: f64,
    range_ms: f64,
    shape: usize,
    input: usize,
    mono: bool,
}

impl AdtSet {
    fn from(p: &Params) -> Self {
        Self {
            src: AdtSide::from(p, "src"),
            copy: AdtSide::from(p, "adt"),
            offset_ms: p.get("adt_offset").clamp(-ADT_MS, ADT_MS),
            range_ms: p.get("lfo_range").clamp(0.0, 5.0),
            shape: p.index("lfo_shape").min(LFO_SHAPES.len() - 1),
            input: p.index("adt_input").min(2),
            mono: p.on("adt_mono"),
        }
    }
}

/// The speakers a bed's copies work on.
struct Bed {
    names: Vec<String>,
    /// The channels in ring order (no LFE); copy `a` works on `chans[a]`.
    chans: Vec<usize>,
    /// This frame's input and wet sound per copy.
    x: Vec<f32>,
    w: Vec<f32>,
}

/// A seed for copy `k`'s random wander.
fn seed(k: usize) -> u32 {
    let k = k as u32;
    0x9e37_79b9 ^ k.wrapping_mul(0x2545_f491) ^ (k << 7)
}

impl Modulation {
    pub fn new(p: &Params, rate: u32) -> Self {
        let sr = f64::from(rate.max(1));
        Self::with_copies(p, sr, 2)
    }

    /// A Modulation over a speaker bed with channels named `names`.
    pub fn wide(p: &Params, rate: u32, names: &[String]) -> Self {
        let chans = super::wide::ring(names);
        let n = chans.len().max(1);
        let mut m = Self::with_copies(p, f64::from(rate.max(1)), n);
        m.bed = Some(Bed {
            names: names.to_vec(),
            chans,
            x: vec![0.0; n],
            w: vec![0.0; n],
        });
        m
    }

    fn with_copies(p: &Params, sr: f64, n: usize) -> Self {
        let len = (MAX_MS / 1000.0 * sr) as usize + 8;
        let mut m = Self {
            sr,
            kind: p.index("kind").min(KINDS.len() - 1),
            lines: (0..n).map(|_| Line::new(len)).collect(),
            stages: vec![[Stage::default(); 12]; n],
            n_stages: 6,
            phase: 0.0,
            step: 0.0,
            depth: 0.0,
            feedback: 0.0,
            width: 1.0,
            voices: 3,
            chorus_ms: 18.0,
            flange_ms: 1.5,
            center: 700.0,
            dt_ms: 28.0,
            wander: 0.4,
            walk: (0..n)
                .map(|k| match k {
                    0 => Wander::new(0x9e37_79b9, 0.6, sr),
                    1 => Wander::new(0x85eb_ca6b, 0.6, sr),
                    _ => Wander::new(seed(k), 0.6, sr),
                })
                .collect(),
            fb: vec![0.0; n],
            dry: 1.0,
            wet: 0.5,
            spread: 0.5,
            bed: None,
            adt: None,
            adt_walk: Wander::new(0x51ed_270b, 0.4, sr),
            adt_step: 0.0,
            adt_phase: 0.0,
        };
        m.set(p);
        m
    }

    fn set(&mut self, p: &Params) {
        let rate = p.get("rate").clamp(0.02, 10.0);
        self.step = std::f64::consts::TAU * rate / self.sr;
        self.depth = (p.get("depth") / 100.0).clamp(0.0, 1.0);
        self.feedback = (p.get("feedback") / 100.0).clamp(-0.95, 0.95) as f32;
        self.width = (p.get("width") / 100.0).clamp(0.0, 1.0) as f32;
        self.voices = (p.get("voices").round() as usize).clamp(2, 6);
        self.chorus_ms = p.get("chorus_delay").clamp(5.0, 40.0);
        self.flange_ms = p.get("flange_delay").clamp(0.1, 10.0);
        self.n_stages = (p.index("stages").min(STAGES.len() - 1) + 1) * 2;
        self.center = p.get("center").clamp(100.0, 5000.0);
        // On a bed Double Track keeps the plain doubler: a copy late by
        // the offset (at least 10 ms), wandering by the LFO range.
        self.dt_ms = (10.0 + p.get("adt_offset").abs()).clamp(10.0, 60.0);
        self.wander = (p.get("lfo_range") / 5.0).clamp(0.0, 1.0);
        self.adt = (self.kind == 3).then(|| AdtSet::from(p));
        for w in &mut self.walk {
            w.set_rate(rate, self.sr);
        }
        let lfo = p.get("lfo_rate").clamp(0.05, 10.0);
        self.adt_step = std::f64::consts::TAU * lfo / self.sr;
        self.adt_walk.set_rate(lfo, self.sr);
        self.dry = (p.get("dry") / 100.0) as f32;
        self.wet = (p.get("wet") / 100.0) as f32;
        self.spread = (p.get("spread") / 100.0).clamp(0.0, 1.0) as f32;
    }

    /// Processes interleaved frames over the bed's speakers.
    fn process_bed(&mut self, buf: &mut [f32]) {
        let Some(mut bed) = self.bed.take() else {
            return;
        };
        let ch = bed.names.len().max(1);
        let n = bed.chans.len();
        // Copies spread evenly round the turn (chorus a whole turn, the
        // others half), narrowed by the width.
        let turn = f64::from(self.width) * if self.kind == 0 { 1.0 } else { 0.5 };
        // A share of each neighbour's sound, the same loudness overall.
        let side = 0.5 * self.spread;
        for f in buf.chunks_exact_mut(ch) {
            for (a, c) in bed.chans.iter().enumerate() {
                bed.x[a] = f[*c];
            }
            for a in 0..n {
                let (prev, next) = (bed.x[(a + n - 1) % n], bed.x[(a + 1) % n]);
                let x = if n > 1 {
                    bed.x[a] * (1.0 - self.spread) + (prev + next) * side
                } else {
                    bed.x[a]
                };
                bed.w[a] = self.wet_of(a, x, turn * a as f64 / n as f64);
            }
            self.phase += self.step;
            if self.phase > std::f64::consts::TAU {
                self.phase -= std::f64::consts::TAU;
            }
            // Width: 0 is the same moving copy in every speaker.
            let mid = bed.w.iter().sum::<f32>() / n.max(1) as f32;
            for (a, c) in bed.chans.iter().enumerate() {
                let w = mid + (bed.w[a] - mid) * self.width;
                f[*c] = f[*c] * self.dry + w * self.wet;
            }
        }
        self.bed = Some(bed);
    }

    fn ms(&self, v: f64) -> f64 {
        v / 1000.0 * self.sr
    }

    /// This frame's wet sound for channel `c` (0 left, 1 right), whose
    /// sweep runs `offset` of a turn behind the left's.
    fn wet_of(&mut self, c: usize, x: f32, offset: f64) -> f32 {
        let ph = self.phase + offset * std::f64::consts::TAU;
        match self.kind {
            // Chorus: voices spread around the turn, each its own swing.
            0 => {
                let base = self.ms(self.chorus_ms);
                let swing = self.ms(self.chorus_ms.min(12.0) * 0.45) * self.depth;
                let n = self.voices;
                let mut sum = 0.0f32;
                for v in 0..n {
                    let p = ph + std::f64::consts::TAU * v as f64 / n as f64;
                    sum += self.lines[c].read(base + swing * lfo_sin(p));
                }
                let y = sum / (n as f32).sqrt();
                self.lines[c].push(x + self.fb[c] * self.feedback);
                self.fb[c] = y / (n as f32).sqrt();
                y
            }
            // Flanger: a short delay swept between almost nothing and twice the setting.
            1 => {
                let base = self.ms(self.flange_ms);
                let d = base * (1.0 + 0.95 * self.depth * lfo_sin(ph)) + 1.0;
                let y = self.lines[c].read(d);
                self.lines[c].push(x + y * self.feedback);
                y
            }
            // Phaser: the all-passes' turning point sweeps up to 3 octaves each way.
            2 => {
                let f =
                    (self.center * 2f64.powf(self.depth * 3.0 * lfo_sin(ph))).min(self.sr * 0.45);
                let t = (std::f64::consts::PI * f / self.sr).tan();
                let a = ((t - 1.0) / (t + 1.0)) as f32;
                let mut y = x + self.fb[c] * self.feedback;
                for s in self.stages[c].iter_mut().take(self.n_stages) {
                    y = s.run(y, a);
                }
                self.fb[c] = y;
                y
            }
            // Double Track: a second take, late by a wandering amount.
            _ => {
                let drift = self.walk[c].next();
                let d = self.ms(self.dt_ms + drift * 3.0 * self.wander) + self.ms(0.0);
                let y = self.lines[c].read(d);
                self.lines[c].push(x + y * self.feedback * 0.5);
                y
            }
        }
    }
}

impl Modulation {
    /// Double Track on stereo: the source held back ADT_MS, the copy read
    /// earlier or later by the offset and the LFO, each colored, leveled
    /// and panned, then summed (and folded to mono when asked).
    fn process_adt(&mut self, buf: &mut [f32], a: AdtSet) {
        let hold = self.ms(ADT_MS);
        for f in buf.as_chunks_mut::<2>().0 {
            let x = match a.input {
                0 => f[0],
                2 => f[1],
                _ => 0.5 * (f[0] + f[1]),
            };
            self.lines[0].push(x);
            let lfo = match a.shape {
                0 => self.adt_phase.sin(),
                1 => 1.0 - 2.0 * (self.adt_phase / std::f64::consts::PI - 1.0).abs(),
                _ => self.adt_walk.next(),
            };
            self.adt_phase = (self.adt_phase + self.adt_step) % std::f64::consts::TAU;
            let src = self.lines[0].read(hold + 1.0);
            let copy = self.lines[0].read(hold + 1.0 + self.ms(a.offset_ms + a.range_ms * lfo));
            let s = a.src.color(src) * a.src.gain;
            let c = a.copy.color(copy) * a.copy.gain;
            let mut l = s * a.src.pan[0] + c * a.copy.pan[0];
            let mut r = s * a.src.pan[1] + c * a.copy.pan[1];
            if a.mono {
                let m = 0.5 * (l + r);
                (l, r) = (m, m);
            }
            f[0] = l;
            f[1] = r;
        }
    }
}

impl Processor for Modulation {
    fn process(&mut self, buf: &mut [f32]) {
        if self.bed.is_some() {
            self.process_bed(buf);
            return;
        }
        if let Some(a) = self.adt {
            self.process_adt(buf, a);
            return;
        }
        // The right side's sweep runs up to a quarter turn behind (half for chorus).
        let offset = f64::from(self.width) * if self.kind == 0 { 0.5 } else { 0.25 };
        for f in buf.as_chunks_mut::<2>().0 {
            let (l, r) = (f[0], f[1]);
            let wl = self.wet_of(0, l, 0.0);
            let wr = self.wet_of(1, r, offset);
            self.phase += self.step;
            if self.phase > std::f64::consts::TAU {
                self.phase -= std::f64::consts::TAU;
            }
            // Width: 0 puts the wet sound in the middle.
            let mid = 0.5 * (wl + wr);
            let side = 0.5 * (wl - wr) * self.width;
            f[0] = l * self.dry + (mid + side) * self.wet;
            f[1] = r * self.dry + (mid - side) * self.wet;
        }
    }

    fn latency(&self) -> usize {
        if self.adt.is_some() && self.bed.is_none() {
            self.ms(ADT_MS).round() as usize
        } else {
            0
        }
    }

    fn reset(&mut self) {
        for l in &mut self.lines {
            l.clear();
        }
        self.stages.fill([Stage::default(); 12]);
        self.fb.fill(0.0);
    }

    fn channels(&self) -> (Vec<String>, Vec<String>) {
        match &self.bed {
            Some(b) => (b.names.clone(), b.names.clone()),
            None => (super::routing::stereo(), super::routing::stereo()),
        }
    }

    fn vary(&mut self, k: usize) {
        // An odd fraction of a turn, so no two copies line up.
        self.phase = (k as f64 * 0.382).fract() * std::f64::consts::TAU;
        let sr = self.sr;
        let rate = self.step * sr / std::f64::consts::TAU;
        let n = self.walk.len();
        self.walk = (0..n)
            .map(|c| Wander::new(seed(k * n + c + 2), rate, sr))
            .collect();
    }

    fn update(&mut self, p: &Params) -> bool {
        if p.index("kind").min(KINDS.len() - 1) != self.kind {
            return false;
        }
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
        Params::resolve(&modulation_info(), &saved)
    }

    fn tone(hz: f32, frames: usize) -> Vec<f32> {
        (0..frames)
            .flat_map(|i| {
                let v = (i as f32 * hz * std::f32::consts::TAU / R as f32).sin() * 0.5;
                [v, v]
            })
            .collect()
    }

    #[test]
    fn every_kind_changes_the_sound_and_stays_sane() {
        for (k, name) in KINDS.iter().enumerate() {
            let mut m = Modulation::new(&params(&[("kind", k as f64), ("feedback", 90.0)]), R);
            let input = tone(440.0, R as usize * 2);
            let mut b = input.clone();
            for c in b.chunks_mut(512) {
                m.process(c);
            }
            assert!(
                b.iter().all(|v| v.is_finite() && v.abs() < 8.0),
                "{name} stays sane"
            );
            let diff: f32 = b.iter().zip(&input).map(|(a, c)| (a - c).abs()).sum();
            assert!(diff > 100.0, "{name} changes the sound: {diff}");
        }
    }

    #[test]
    fn wet_off_leaves_the_sound_alone() {
        // Double Track has its own levels for the source and the copy.
        for k in 0..KINDS.len() - 1 {
            let mut m = Modulation::new(&params(&[("kind", k as f64), ("wet", 0.0)]), R);
            let input = tone(300.0, 4800);
            let mut b = input.clone();
            m.process(&mut b);
            assert!(b.iter().zip(&input).all(|(a, c)| (a - c).abs() < 1e-6));
        }
    }

    #[test]
    fn width_zero_puts_the_wet_sound_in_the_middle() {
        let mut m = Modulation::new(&params(&[("width", 0.0), ("dry", 0.0), ("wet", 100.0)]), R);
        let mut b = tone(500.0, 9600);
        m.process(&mut b);
        assert!(b
            .as_chunks::<2>()
            .0
            .iter()
            .all(|f| (f[0] - f[1]).abs() < 1e-5));
    }

    #[test]
    fn on_a_bed_a_sound_in_front_spreads_round_the_room() {
        let names: Vec<String> = ["L", "R", "C", "LFE", "Ls", "Rs"]
            .iter()
            .map(|x| (*x).to_string())
            .collect();
        for k in 0..KINDS.len() {
            let p = params(&[("kind", k as f64), ("dry", 0.0), ("wet", 100.0)]);
            let mut m = Modulation::wide(&p, R, &names);
            let frames = R as usize;
            let mut b = vec![0.0f32; frames * 6];
            for (i, f) in b.as_chunks_mut::<6>().0.iter_mut().enumerate() {
                f[2] = (i as f32 * 440.0 * std::f32::consts::TAU / R as f32).sin() * 0.5;
            }
            for c in b.chunks_mut(6 * 256) {
                m.process(c);
            }
            let energy = |c: usize| {
                b.as_chunks::<6>()
                    .0
                    .iter()
                    .map(|f| f[c] * f[c])
                    .sum::<f32>()
            };
            assert!(b.iter().all(|v| v.is_finite()));
            assert!(energy(0) > 1.0 && energy(1) > 1.0, "{k}: reaches L and R");
            assert!(energy(3) == 0.0, "{k}: LFE left alone");
            // The copies differ: the room moves, not one copy everywhere.
            let diff: f32 = b
                .as_chunks::<6>()
                .0
                .iter()
                .map(|f| (f[0] - f[1]).abs())
                .sum();
            assert!(diff > 10.0, "{k}: copies move on their own: {diff}");
        }
    }

    #[test]
    fn double_track_offsets_the_copy_and_pans_each_side() {
        // Source hard left, copy hard right 5 ms late, no LFO.
        let p = params(&[
            ("kind", 3.0),
            ("adt_offset", 5.0),
            ("lfo_range", 0.0),
            ("src_pan", -100.0),
            ("adt_pan", 100.0),
        ]);
        let mut m = Modulation::new(&p, R);
        let hold = 960; // 20 ms at 48 kHz
        assert_eq!(super::super::Processor::latency(&m), hold);
        let mut b = vec![0.0f32; 4000 * 2];
        b[0] = 1.0;
        b[1] = 1.0;
        m.process(&mut b);
        let peak = |side: usize| {
            (0..4000)
                .max_by(|&i, &j| b[i * 2 + side].abs().total_cmp(&b[j * 2 + side].abs()))
                .unwrap()
        };
        assert_eq!(peak(0), hold, "source comes out after the hold-back");
        assert!(
            (peak(1) as i64 - (hold + 240) as i64).abs() <= 1,
            "copy 5 ms later: {}",
            peak(1)
        );
        // Copy muted: only the source, in both sides when centered.
        let p = params(&[("kind", 3.0), ("adt_mute", 1.0)]);
        let mut m = Modulation::new(&p, R);
        let mut b = vec![0.0f32; 2000 * 2];
        b[0] = 1.0;
        b[1] = 1.0;
        m.process(&mut b);
        let sum: f32 = b.iter().map(|v| v.abs()).sum();
        assert!(
            (sum - 2.0 * std::f32::consts::FRAC_1_SQRT_2).abs() < 0.05,
            "{sum}"
        );
    }
}

#[cfg(test)]
mod lfo_tests {
    #[test]
    fn the_quick_sine_stays_close() {
        for i in 0..2_000 {
            let x = i as f64 * 0.01 - 10.0;
            assert!((super::lfo_sin(x) - x.sin()).abs() < 2e-3, "{x}");
        }
    }
}
