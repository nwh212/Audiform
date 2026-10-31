// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

//! Reverb: a made space, in five kinds. Each kind runs the same engine
//! with its own shape:
//!
//! - early reflections: a handful of quick echoes off nearby walls;
//! - diffusion: a chain of all-pass filters that smears the input into a
//!   dense wash (a spring's chain is long and dispersive, so high notes
//!   arrive before low ones: the "boing");
//! - the tail: eight delay lines feeding each other through an
//!   orthogonal (Hadamard) mix, each darkened by a low-pass and turned
//!   down so the tail falls 60 dB in the Decay time. Their lengths drift
//!   slowly (Modulation) so the tail never rings on one pitch.
//!
//! Room is small with strong reflections; Hall is large, slow and soft;
//! Plate has no reflections, a dense bright wash with diffusion inside
//! the loop; Spring is short, dark at both ends, with the dispersive
//! chain; Tape is a ring of tape echoes that blur into a tail, with wow,
//! flutter and soft tape saturation in the loop.
//!
//! The Impulse Response kind is the Convolution Reverb, picked from the
//! same window.
//!
//! On a speaker bed (a 3D Space master, a surround file) the same tail
//! feeds every speaker: each hears its own mix of the eight lines (a
//! different row of the Hadamard matrix, then a short all-pass of its
//! own), so the room is different in every speaker, as a real one is.
//! The early reflections are dealt out among the speakers. The bed's
//! left and right speakers feed the reverb's left and right inputs; the
//! low-frequency channel neither feeds it nor gets any of it.

use super::{choice, num, EffectInfo, Params, Processor};
use crate::dsp::biquad::{Biquad, Coeffs, Shape};

pub const ID: &str = "altaudio.reverb";
pub const KINDS: [&str; 5] = ["Room", "Hall", "Plate", "Spring", "Tape"];
const MAX_PREDELAY_MS: f64 = 250.0;
/// Size moves the lengths between these two scales of the kind's own.
const SIZE_MIN: f64 = 0.5;
const SIZE_MAX: f64 = 1.6;

pub fn verb_info() -> EffectInfo {
    EffectInfo {
        id: ID,
        name: "Reverb",
        category: "Delay and Reverb",
        description: "A made space: Room, Hall, Plate, Spring or Tape, each shaped by size, decay, early reflections, damping, diffusion and modulation. Impulse Response (a recorded space) is in the same window.",
        params: vec![
            choice("kind", "Type", &KINDS, 0),
            num("size", "Room size", "%", 0.0, 100.0, 50.0, 1.0).group("Space"),
            num("decay", "Decay", "ms", 100.0, 20_000.0, 1800.0, 1.0).log().group("Space"),
            num("predelay", "Pre-delay", "ms", 0.0, MAX_PREDELAY_MS, 10.0, 1.0).group("Space"),
            num("early", "Early reflections", "%", 0.0, 100.0, 40.0, 1.0).group("Space"),
            num("width", "Width", "%", 0.0, 100.0, 100.0, 1.0).group("Space"),
            num("damping", "Damping", "%", 0.0, 100.0, 50.0, 1.0).group("Character"),
            num("diffusion", "Diffusion", "%", 0.0, 100.0, 70.0, 1.0).group("Character"),
            num("modulation", "Modulation", "%", 0.0, 100.0, 30.0, 1.0).group("Character"),
            num("high_cut", "High cut", "Hz", 1000.0, 20_000.0, 9000.0, 10.0).log().group("Character"),
            num("low_cut", "Low cut", "Hz", 20.0, 1000.0, 80.0, 1.0).log().group("Character"),
            num("dry", "Dry", "%", 0.0, 100.0, 100.0, 1.0).group("Output"),
            num("wet", "Wet", "%", 0.0, 100.0, 30.0, 1.0).group("Output"),
        ],
    }
}

/// What makes each kind itself.
struct Shape8 {
    /// Tail line lengths at size 1, ms.
    lines: [f64; 8],
    /// Early reflections: first and last arrival at size 1, ms; none when the count is 0.
    er: (usize, f64, f64),
    /// Input all-passes at size 1, ms.
    diffusers: &'static [f64],
    /// Dispersive first-order all-passes (springs).
    chirp: usize,
    /// All-pass inside each tail line (plates, halls).
    in_loop: bool,
    /// How far modulation moves the lines at full, in ms.
    mod_ms: f64,
    /// Damping's brightness: 1 as set, above brighter, below darker.
    bright: f64,
    /// Tape saturation in the loop.
    saturate: bool,
    /// The kind's own band, Hz: low and high edges of the tail.
    band: (f64, f64),
}

fn shape(kind: usize) -> Shape8 {
    match kind {
        1 => Shape8 {
            lines: [29.7, 37.1, 41.1, 43.7, 53.1, 59.3, 67.7, 79.1],
            er: (14, 8.0, 85.0),
            diffusers: &[2.3, 4.1, 6.7, 9.9],
            chirp: 0,
            in_loop: true,
            mod_ms: 0.35,
            bright: 0.9,
            saturate: false,
            band: (20.0, 20_000.0),
        },
        2 => Shape8 {
            lines: [8.1, 10.3, 12.9, 15.1, 17.7, 21.3, 24.1, 27.9],
            er: (0, 0.0, 0.0),
            diffusers: &[1.1, 1.9, 2.7, 3.9, 5.3, 7.1],
            chirp: 0,
            in_loop: true,
            mod_ms: 0.2,
            bright: 1.6,
            saturate: false,
            band: (60.0, 20_000.0),
        },
        3 => Shape8 {
            lines: [21.0, 23.3, 27.1, 31.7, 33.9, 37.3, 41.9, 45.1],
            er: (0, 0.0, 0.0),
            diffusers: &[1.3, 2.9],
            chirp: 40,
            in_loop: false,
            mod_ms: 0.1,
            bright: 0.7,
            saturate: false,
            band: (180.0, 5000.0),
        },
        4 => Shape8 {
            lines: [41.0, 47.3, 53.9, 61.1, 67.3, 73.9, 83.3, 97.1],
            er: (3, 60.0, 180.0),
            diffusers: &[3.1, 5.9],
            chirp: 0,
            in_loop: false,
            mod_ms: 0.9,
            bright: 0.35,
            saturate: true,
            band: (90.0, 7000.0),
        },
        _ => Shape8 {
            lines: [7.3, 9.1, 11.7, 13.9, 16.3, 19.1, 22.7, 26.1],
            er: (12, 2.0, 38.0),
            diffusers: &[1.3, 2.9, 4.3, 6.7],
            chirp: 0,
            in_loop: false,
            mod_ms: 0.15,
            bright: 1.0,
            saturate: false,
            band: (20.0, 20_000.0),
        },
    }
}

fn flush(x: f32) -> f32 {
    if x.abs() < 1e-20 {
        0.0
    } else {
        x
    }
}

/// A Schroeder all-pass of a fixed length.
struct Allpass {
    buf: Vec<f32>,
    at: usize,
    g: f32,
}

impl Allpass {
    fn new(frames: usize, g: f32) -> Self {
        Self {
            buf: vec![0.0; frames.max(1)],
            at: 0,
            g,
        }
    }
    fn run(&mut self, x: f32) -> f32 {
        let d = self.buf[self.at];
        let v = x + self.g * d;
        self.buf[self.at] = flush(v);
        self.at = (self.at + 1) % self.buf.len();
        d - self.g * v
    }
}

/// A first-order all-pass: no delay, only a phase turn that grows with
/// frequency. Many in a row spread a click into a falling chirp.
#[derive(Clone, Copy, Default)]
struct Disperse {
    x1: f32,
    y1: f32,
}

impl Disperse {
    fn run(&mut self, x: f32, a: f32) -> f32 {
        let y = a * x + self.x1 - a * self.y1;
        self.x1 = x;
        self.y1 = flush(y);
        y
    }
}

/// One tail line: a ring read a little behind its length, moving slowly.
struct Line {
    buf: Vec<f32>,
    at: usize,
    /// Length in frames at the current size.
    len: f64,
    phase: f64,
    /// LFO step per frame.
    step: f64,
    gain: f32,
    lp: f32,
    lp_state: f32,
    ap: Option<Allpass>,
}

impl Line {
    fn read(&self, depth: f64) -> f32 {
        let n = self.buf.len();
        let d = (self.len + depth * (1.0 + self.phase.sin())).clamp(1.0, (n - 2) as f64);
        let pos = self.at as f64 + n as f64 - d;
        let i0 = pos.floor();
        let frac = (pos - i0) as f32;
        let i0 = i0 as usize % n;
        let i1 = (i0 + 1) % n;
        self.buf[i0] * (1.0 - frac) + self.buf[i1] * frac
    }
}

/// Mixes eight values with the fast Walsh-Hadamard transform, scaled to
/// keep their energy (an orthogonal mix, so the loop never runs away).
fn hadamard(v: &mut [f32; 8]) {
    let mut h = 1;
    while h < 8 {
        for i in (0..8).step_by(h * 2) {
            for j in i..i + h {
                let (a, b) = (v[j], v[j + h]);
                v[j] = a + b;
                v[j + h] = a - b;
            }
        }
        h *= 2;
    }
    let s = 1.0 / 8f32.sqrt();
    for x in v.iter_mut() {
        *x *= s;
    }
}

pub struct Reverb {
    rate: f64,
    kind: usize,
    pre: [Vec<f32>; 2],
    pre_at: usize,
    pre_len: usize,
    er_ring: Vec<f32>,
    er_at: usize,
    /// (delay frames, gain, side): side 0 left, 1 right.
    taps: Vec<(usize, f32, usize)>,
    er_lp: [f32; 2],
    diff: [Vec<Allpass>; 2],
    chirp: [Vec<Disperse>; 2],
    lines: Vec<Line>,
    depth: f64,
    saturate: bool,
    low: [Biquad; 2],
    high: [Biquad; 2],
    early: f32,
    width: f32,
    dry: f32,
    wet: f32,
    /// Each early reflection's level this frame (filled by `core`).
    er_vals: Vec<f32>,
    /// On a speaker bed; None for stereo.
    bed: Option<Bed>,
}

/// The reverb's speakers on a bed.
struct Bed {
    names: Vec<String>,
    /// Per channel: how much it feeds the left and right inputs.
    feed: Vec<[f32; 2]>,
    /// Per channel: its place among the speakers that get the reverb
    /// (None for the low-frequency channel).
    out: Vec<Option<usize>>,
    /// Per speaker that gets the reverb: its row of the line mix, its own
    /// all-pass, its filters and reflection darkening, and this frame's sound.
    rows: Vec<usize>,
    decor: Vec<Allpass>,
    low: Vec<Biquad>,
    high: Vec<Biquad>,
    er_lp: Vec<f32>,
    wet: Vec<f32>,
    /// Keeps the whole bed's tail as loud as the stereo tail.
    gain: f32,
}

/// How much a speaker feeds the reverb's left and right inputs.
fn feed(name: &str, index: usize) -> [f32; 2] {
    const H: f32 = std::f32::consts::FRAC_1_SQRT_2;
    match name {
        "LFE" => [0.0, 0.0],
        "L" => [1.0, 0.0],
        "R" => [0.0, 1.0],
        "C" | "Cs" | "M" => [H, H],
        n if n.starts_with('L') => [H, 0.0],
        n if n.starts_with('R') => [0.0, H],
        _ if index.is_multiple_of(2) => [H, 0.0],
        _ => [0.0, H],
    }
}

impl Bed {
    fn new(names: &[String], rate: u32) -> Self {
        let r = f64::from(rate.max(1));
        let mut out = Vec::new();
        let mut n = 0;
        for name in names {
            if name == "LFE" {
                out.push(None);
            } else {
                out.push(Some(n));
                n += 1;
            }
        }
        // Row 0 is the plain sum, the least like a room: used last.
        const ROWS: [usize; 8] = [1, 2, 4, 7, 3, 5, 6, 0];
        let filter = |shape, f| Biquad::new(Coeffs::new(shape, rate, f, 0.707, 0.0));
        Self {
            names: names.to_vec(),
            feed: names.iter().enumerate().map(|(i, x)| feed(x, i)).collect(),
            out,
            rows: (0..n).map(|a| ROWS[a % 8]).collect(),
            decor: (0..n)
                .map(|a| Allpass::new(((0.9 + 0.61 * a as f64) / 1000.0 * r) as usize, 0.5))
                .collect(),
            low: vec![filter(Shape::HighPass, 20.0); n],
            high: vec![filter(Shape::LowPass, 20_000.0); n],
            er_lp: vec![0.0; n],
            wet: vec![0.0; n],
            gain: (2.0 / n.max(2) as f32).sqrt(),
        }
    }
}

/// +1 or -1: entry (row, k) of the 8 by 8 Hadamard matrix.
fn sign(row: usize, k: usize) -> f32 {
    if (row & k).count_ones().is_multiple_of(2) {
        1.0
    } else {
        -1.0
    }
}

impl Reverb {
    /// A reverb over a speaker bed with channels named `names`.
    pub fn wide(p: &Params, rate: u32, names: &[String]) -> Self {
        let mut v = Self::new(p, rate);
        v.bed = Some(Bed::new(names, rate));
        v.set(p, rate);
        v
    }

    pub fn new(p: &Params, rate: u32) -> Self {
        let r = f64::from(rate.max(1));
        let kind = p.index("kind").min(KINDS.len() - 1);
        let sh = shape(kind);
        let ms = |v: f64| (v / 1000.0 * r).round() as usize;
        let scale = SIZE_MIN + (SIZE_MAX - SIZE_MIN) * (p.get("size") / 100.0);
        let longest = SIZE_MAX * sh.mod_ms.max(0.1) + 4.0;
        let lines = sh
            .lines
            .iter()
            .enumerate()
            .map(|(i, l)| Line {
                buf: vec![0.0; ms(l * SIZE_MAX + longest * 2.0) + 8],
                at: 0,
                len: l * scale / 1000.0 * r,
                phase: i as f64 * 0.77,
                step: std::f64::consts::TAU * (0.31 + 0.13 * i as f64) / r,
                gain: 0.0,
                lp: 0.0,
                lp_state: 0.0,
                ap: sh
                    .in_loop
                    .then(|| Allpass::new(ms(2.1 + 0.83 * i as f64), 0.0)),
            })
            .collect();
        let diff = |offset: f64| {
            sh.diffusers
                .iter()
                .map(|d| Allpass::new(ms(d * scale * offset), 0.0))
                .collect::<Vec<_>>()
        };
        let (count, first, last) = sh.er;
        let taps = (0..count)
            .map(|k| {
                let t = if count > 1 {
                    k as f64 / (count - 1) as f64
                } else {
                    0.0
                };
                // Reflections crowd in after the first few, a little uneven.
                let wobble = 1.0 + 0.07 * ((k as f64 * 2.39).sin());
                let at = (first + (last - first) * t.powf(1.4) * wobble) * scale;
                let g = 0.85 * (-2.2 * t).exp() as f32;
                (ms(at).max(1), g, k % 2)
            })
            .collect::<Vec<_>>();
        let er_len = taps.iter().map(|t| t.0).max().unwrap_or(1) + 2;
        let pre_len = ms(MAX_PREDELAY_MS) + 2;
        let mut v = Self {
            rate: r,
            kind,
            pre: [vec![0.0; pre_len], vec![0.0; pre_len]],
            pre_at: 0,
            pre_len: 0,
            er_ring: vec![0.0; er_len],
            er_at: 0,
            taps,
            er_lp: [0.0; 2],
            diff: [diff(1.0), diff(1.13)],
            chirp: [
                vec![Disperse::default(); sh.chirp],
                vec![Disperse::default(); sh.chirp],
            ],
            lines,
            depth: 0.0,
            saturate: sh.saturate,
            low: [Biquad::new(Coeffs::new(Shape::HighPass, rate, 20.0, 0.707, 0.0)); 2].map(|b| b),
            high: [Biquad::new(Coeffs::new(Shape::LowPass, rate, 20_000.0, 0.707, 0.0)); 2]
                .map(|b| b),
            early: 0.0,
            width: 1.0,
            dry: 1.0,
            wet: 0.0,
            er_vals: Vec::new(),
            bed: None,
        };
        v.er_vals = vec![0.0; v.taps.len()];
        v.set(p, rate);
        v
    }

    fn set(&mut self, p: &Params, rate: u32) {
        let sh = shape(self.kind);
        let r = self.rate;
        let decay = (p.get("decay") / 1000.0).max(0.05);
        let damping = p.get("damping") / 100.0;
        let diffusion = (p.get("diffusion") / 100.0) as f32;
        // Damping: the tail's own low-pass, from open to dark.
        let cutoff = ((1.0 - damping).powi(2) * 18_000.0 + 1200.0) * sh.bright;
        let lp = (-std::f64::consts::TAU * cutoff.min(r * 0.45) / r).exp() as f32;
        for l in &mut self.lines {
            // Each pass through a line loses its share of 60 dB per Decay.
            l.gain = 10f64.powf(-3.0 * (l.len / r) / decay) as f32;
            l.lp = lp;
            if let Some(ap) = l.ap.as_mut() {
                ap.g = 0.45 * diffusion;
            }
        }
        for side in &mut self.diff {
            for (i, a) in side.iter_mut().enumerate() {
                a.g = (0.45 + 0.3 * diffusion) * if i % 2 == 0 { 1.0 } else { -1.0 };
            }
        }
        self.depth = p.get("modulation") / 100.0 * sh.mod_ms / 1000.0 * r;
        self.pre_len =
            ((p.get("predelay") / 1000.0 * r).round() as usize).min(self.pre[0].len() - 1);
        self.early = (p.get("early") / 100.0) as f32 * if sh.er.0 == 0 { 0.0 } else { 1.0 };
        self.width = (p.get("width") / 100.0) as f32;
        self.dry = (p.get("dry") / 100.0) as f32;
        self.wet = (p.get("wet") / 100.0) as f32;
        let lo = p.get("low_cut").max(sh.band.0);
        let hi = p.get("high_cut").min(sh.band.1);
        for b in &mut self.low {
            b.set(Coeffs::new(Shape::HighPass, rate, lo, 0.707, 0.0));
        }
        for b in &mut self.high {
            b.set(Coeffs::new(Shape::LowPass, rate, hi, 0.707, 0.0));
        }
        if let Some(bed) = self.bed.as_mut() {
            for b in &mut bed.low {
                b.set(Coeffs::new(Shape::HighPass, rate, lo, 0.707, 0.0));
            }
            for b in &mut bed.high {
                b.set(Coeffs::new(Shape::LowPass, rate, hi, 0.707, 0.0));
            }
        }
    }

    /// One frame of the engine: pre-delay, the early reflections (into
    /// `er_vals`), diffusion and the tail. Returns the eight lines' sound.
    fn core(&mut self, l: f32, r: f32) -> [f32; 8] {
        // Pre-delay.
        let n = self.pre[0].len();
        self.pre[0][self.pre_at] = l;
        self.pre[1][self.pre_at] = r;
        let back = (self.pre_at + n - self.pre_len) % n;
        let (pl, pr) = (self.pre[0][back], self.pre[1][back]);
        self.pre_at = (self.pre_at + 1) % n;

        // Early reflections off a mono copy.
        if self.early > 0.0 {
            let m = self.er_ring.len();
            self.er_ring[self.er_at] = (pl + pr) * 0.5;
            for (v, &(d, g, _)) in self.er_vals.iter_mut().zip(&self.taps) {
                *v = self.er_ring[(self.er_at + m - d) % m] * g;
            }
            self.er_at = (self.er_at + 1) % m;
        }

        // Diffusion (and a spring's dispersion).
        let mut il = pl;
        let mut ir = pr;
        for d in &mut self.chirp[0] {
            il = d.run(il, 0.62);
        }
        for d in &mut self.chirp[1] {
            ir = d.run(ir, 0.6);
        }
        for a in &mut self.diff[0] {
            il = a.run(il);
        }
        for a in &mut self.diff[1] {
            ir = a.run(ir);
        }

        // The tail.
        let mut o = [0f32; 8];
        for (k, line) in self.lines.iter().enumerate() {
            o[k] = line.read(self.depth);
        }
        let mut mixed = o;
        hadamard(&mut mixed);
        for (k, line) in self.lines.iter_mut().enumerate() {
            let mut v = mixed[k];
            line.lp_state = flush(v + line.lp * (line.lp_state - v));
            v = line.lp_state * line.gain;
            if let Some(ap) = line.ap.as_mut() {
                v = ap.run(v);
            }
            if self.saturate {
                v = v.tanh();
            }
            let input = if k < 4 { il } else { ir } * if k % 2 == 0 { 0.5 } else { -0.5 };
            let n = line.buf.len();
            line.buf[line.at] = flush(v + input);
            line.at = (line.at + 1) % n;
            line.phase += line.step;
            if line.phase > std::f64::consts::TAU {
                line.phase -= std::f64::consts::TAU;
            }
        }
        o
    }

    /// One stereo frame of wet sound.
    fn frame(&mut self, l: f32, r: f32) -> (f32, f32) {
        let o = self.core(l, r);
        // Early reflections alternate sides.
        let (mut el, mut er) = (0.0f32, 0.0f32);
        if self.early > 0.0 {
            for (v, t) in self.er_vals.iter().zip(&self.taps) {
                if t.2 == 0 {
                    el += v;
                } else {
                    er += v;
                }
            }
            if self.saturate {
                // Tape heads: each echo a little darker.
                self.er_lp[0] += 0.35 * (el - self.er_lp[0]);
                self.er_lp[1] += 0.35 * (er - self.er_lp[1]);
                el = self.er_lp[0];
                er = self.er_lp[1];
            }
        }
        let (mut tl, mut tr) = (0.0f32, 0.0f32);
        for (k, v) in o.iter().enumerate() {
            if k % 2 == 0 {
                tl += v;
            } else {
                tr += v;
            }
            if k % 4 < 2 {
                tl += 0.4 * v;
            } else {
                tr += 0.4 * v;
            }
        }
        let late = 0.5f32;
        let mut wl = tl * late + el * self.early;
        let mut wr = tr * late + er * self.early;
        wl = self.high[0].run(self.low[0].run(wl));
        wr = self.high[1].run(self.low[1].run(wr));
        // Width: 0 is mono, 100 the full spread.
        let mid = (wl + wr) * 0.5;
        let side = (wl - wr) * 0.5 * self.width;
        (mid + side, mid - side)
    }

    /// Processes interleaved frames over the bed's speakers.
    fn process_bed(&mut self, buf: &mut [f32]) {
        let Some(mut bed) = self.bed.take() else {
            return;
        };
        let ch = bed.names.len().max(1);
        let n = bed.wet.len();
        let late = 0.5f32;
        // The eight lines' sum is about √8 times one line, the stereo
        // side's about 2.15 times: the same per speaker before the bed's gain.
        let tail_scale = 0.76 * bed.gain;
        for f in buf.chunks_exact_mut(ch) {
            let (mut il, mut ir) = (0.0f32, 0.0f32);
            for (x, w) in f.iter().zip(&bed.feed) {
                il += x * w[0];
                ir += x * w[1];
            }
            let o = self.core(il, ir);
            for a in 0..n {
                let row = bed.rows[a];
                let tail: f32 = o.iter().enumerate().map(|(k, v)| sign(row, k) * v).sum();
                let mut e = 0.0f32;
                if self.early > 0.0 {
                    for v in self.er_vals.iter().skip(a).step_by(n) {
                        e += v;
                    }
                    if self.saturate {
                        bed.er_lp[a] += 0.35 * (e - bed.er_lp[a]);
                        e = bed.er_lp[a];
                    }
                }
                let w = bed.decor[a].run(tail * tail_scale * late + e * self.early);
                bed.wet[a] = bed.high[a].run(bed.low[a].run(w));
            }
            // Width: 0 is the same in every speaker.
            let mid = bed.wet.iter().sum::<f32>() / n.max(1) as f32;
            for v in &mut bed.wet {
                *v = mid + (*v - mid) * self.width;
            }
            for (x, out) in f.iter_mut().zip(&bed.out) {
                let wet = out.map_or(0.0, |a| bed.wet[a]);
                *x = *x * self.dry + wet * self.wet;
            }
        }
        self.bed = Some(bed);
    }
}

impl Processor for Reverb {
    fn process(&mut self, buf: &mut [f32]) {
        if self.bed.is_some() {
            self.process_bed(buf);
            return;
        }
        for f in buf.as_chunks_mut::<2>().0 {
            let (l, r) = (f[0], f[1]);
            let (wl, wr) = self.frame(l, r);
            f[0] = l * self.dry + wl * self.wet;
            f[1] = r * self.dry + wr * self.wet;
        }
    }

    fn reset(&mut self) {
        for b in &mut self.pre {
            b.fill(0.0);
        }
        self.er_ring.fill(0.0);
        self.er_lp = [0.0; 2];
        for side in &mut self.diff {
            for a in side.iter_mut() {
                a.buf.fill(0.0);
            }
        }
        for side in &mut self.chirp {
            side.fill(Disperse::default());
        }
        for l in &mut self.lines {
            l.buf.fill(0.0);
            l.lp_state = 0.0;
            if let Some(ap) = l.ap.as_mut() {
                ap.buf.fill(0.0);
            }
        }
        for b in self.low.iter_mut().chain(self.high.iter_mut()) {
            b.reset();
        }
        if let Some(bed) = self.bed.as_mut() {
            for a in &mut bed.decor {
                a.buf.fill(0.0);
            }
            for b in bed.low.iter_mut().chain(bed.high.iter_mut()) {
                b.reset();
            }
            bed.er_lp.fill(0.0);
        }
    }

    fn channels(&self) -> (Vec<String>, Vec<String>) {
        match &self.bed {
            Some(b) => (b.names.clone(), b.names.clone()),
            None => (super::routing::stereo(), super::routing::stereo()),
        }
    }

    fn update(&mut self, p: &Params) -> bool {
        // A new kind or size means new lines: built again.
        let sh = shape(self.kind);
        let scale = SIZE_MIN + (SIZE_MAX - SIZE_MIN) * (p.get("size") / 100.0);
        let same_size = (self.lines[0].len - sh.lines[0] * scale / 1000.0 * self.rate).abs() < 0.5;
        if p.index("kind") != self.kind || !same_size {
            return false;
        }
        self.set(p, self.rate as u32);
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
        Params::resolve(&verb_info(), &saved)
    }

    /// The wet tail of a click: its level (RMS) in each tenth of a second.
    fn tail(over: &[(&str, f64)], seconds: f64) -> Vec<f32> {
        let mut p = over.to_vec();
        p.extend([("dry", 0.0), ("wet", 100.0)]);
        let mut v = Reverb::new(&params(&p), R);
        let frames = (seconds * f64::from(R)) as usize;
        let mut b = vec![0.0f32; frames * 2];
        b[0] = 1.0;
        b[1] = 1.0;
        for c in b.chunks_mut(2 * 256) {
            v.process(c);
        }
        assert!(
            b.iter().all(|x| x.is_finite() && x.abs() < 4.0),
            "the tail stays sane"
        );
        b.chunks(2 * 4800)
            .map(|c| (c.iter().map(|x| x * x).sum::<f32>() / c.len() as f32).sqrt())
            .collect()
    }

    #[test]
    fn every_kind_rings_and_dies_away() {
        for (kind, name) in KINDS.iter().enumerate() {
            let t = tail(&[("kind", kind as f64), ("decay", 1000.0)], 3.0);
            let db = |x: f32| 20.0 * x.max(1e-12).log10();
            assert!(t[1] > 1e-4, "{}: it rings: {:?}", name, t);
            // About 60 dB down a second later (a wide margin: modulation, saturation).
            let fall = db(t[2]) - db(t[12]);
            assert!(
                fall > 35.0 && fall < 90.0,
                "{}: falls {fall} dB in a second",
                name
            );
        }
    }

    #[test]
    fn a_longer_decay_rings_longer() {
        let short = tail(&[("decay", 500.0)], 2.0);
        let long = tail(&[("decay", 4000.0)], 2.0);
        assert!(long[15] > short[15] * 10.0, "{} {}", long[15], short[15]);
    }

    #[test]
    fn dry_only_leaves_the_sound_alone_and_width_zero_is_mono() {
        let mut v = Reverb::new(&params(&[("wet", 0.0)]), R);
        let input: Vec<f32> = (0..4800)
            .flat_map(|i| [(i as f32 * 0.01).sin() * 0.3, 0.1])
            .collect();
        let mut b = input.clone();
        v.process(&mut b);
        assert!(b.iter().zip(&input).all(|(a, c)| (a - c).abs() < 1e-6));
        let mut m = Reverb::new(&params(&[("dry", 0.0), ("wet", 100.0), ("width", 0.0)]), R);
        let mut c: Vec<f32> = (0..9600)
            .flat_map(|i| [if i == 0 { 1.0 } else { 0.0 }, 0.0])
            .collect();
        m.process(&mut c);
        assert!(c
            .as_chunks::<2>()
            .0
            .iter()
            .all(|f| (f[0] - f[1]).abs() < 1e-6));
    }
}
