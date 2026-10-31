// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Diffuse sound for 3D Space sessions: Spread and Spatialize.
//!
//! A decorrelator turns one signal into four that sound the same but no
//! longer line up sample for sample: each goes through its own short
//! delay and a chain of all-pass filters (which change phase, not level).
//! Played from different speakers, such copies are heard as a wide,
//! enveloping sound instead of one phantom point between them.
//!
//! Spread sends part of a source through one, to every speaker of the
//! layers in use. Spatialize opens a stereo source out into the room in
//! one of four ways (`SpatializeMode`), each feeding what the sides share
//! to the center:
//! - Side Spread: what differs between the sides (left minus right),
//!   decorrelated, to the surround and height speakers, left ones in
//!   phase and right ones inverted.
//! - Matrix: the same difference on a short plain delay, as a passive
//!   surround decoder does.
//! - Ambience: decorrelated copies of the left side to the left speakers
//!   and of the right side to the right ones, all in phase.
//! - Height Lift: Side Spread to the height speakers only.
//! - Place Elements (`Steer`): frequency by frequency, each part of the
//!   sound goes where it sits in the stereo picture (see [`Steer`]).
//!
//! Our own methods, built from textbook parts.

use crate::dsp::space::speakers;
use crate::session::model::{SpaceLayout, SpatializeMode};

/// Copies a decorrelator makes.
pub const TAPS: usize = 4;

/// One Schroeder all-pass section: flat level, smeared phase.
struct AllPass {
    buf: Vec<f32>,
    at: usize,
    g: f32,
}

impl AllPass {
    fn new(len: usize, g: f32) -> Self {
        Self {
            buf: vec![0.0; len.max(1)],
            at: 0,
            g,
        }
    }

    fn run(&mut self, x: f32) -> f32 {
        let d = self.buf[self.at];
        let v = x + self.g * d;
        self.buf[self.at] = v;
        self.at = (self.at + 1) % self.buf.len();
        d - self.g * v
    }
}

/// A pre-delay then three all-pass sections.
struct Path {
    pre: Vec<f32>,
    at: usize,
    ap: [AllPass; 3],
}

impl Path {
    fn run(&mut self, x: f32) -> f32 {
        let d = self.pre[self.at];
        self.pre[self.at] = x;
        self.at = (self.at + 1) % self.pre.len();
        self.ap.iter_mut().fold(d, |v, a| a.run(v))
    }
}

/// Four decorrelated copies of one signal. All buffers are made up front,
/// so running it never allocates.
pub struct Decorrelator {
    paths: [Path; TAPS],
}

impl Decorrelator {
    /// `seed` picks a different set of delays, so two decorrelators on the
    /// same source do not match each other either.
    pub fn new(rate: u32, seed: usize) -> Self {
        let ms = |v: f32| ((v * rate as f32) / 1000.0).round().max(1.0) as usize;
        // Pre-delays 5 to 19 ms and all-pass lengths at prime-ish spacings.
        const PRE: [f32; 8] = [5.3, 9.7, 13.1, 17.9, 7.1, 11.3, 15.7, 19.3];
        const AP: [[f32; 3]; 8] = [
            [1.31, 3.71, 7.93],
            [1.79, 4.33, 8.69],
            [2.11, 5.03, 9.41],
            [2.53, 5.77, 10.13],
            [1.13, 3.97, 8.27],
            [1.97, 4.61, 9.07],
            [2.39, 5.39, 9.83],
            [2.71, 6.11, 10.61],
        ];
        let path = |k: usize| {
            let i = (k + seed * TAPS) % PRE.len();
            Path {
                pre: vec![0.0; ms(PRE[i])],
                at: 0,
                ap: AP[i].map(|l| AllPass::new(ms(l), 0.6)),
            }
        };
        Self {
            paths: [path(0), path(1), path(2), path(3)],
        }
    }

    pub fn run(&mut self, x: f32) -> [f32; TAPS] {
        let mut out = [0.0; TAPS];
        for (o, p) in out.iter_mut().zip(self.paths.iter_mut()) {
            *o = p.run(x);
        }
        out
    }
}

/// A plain delay of one signal.
pub struct Delay {
    buf: Vec<f32>,
    at: usize,
}

impl Delay {
    pub fn new(len: usize) -> Self {
        Self {
            buf: vec![0.0; len.max(1)],
            at: 0,
        }
    }

    pub fn run(&mut self, x: f32) -> f32 {
        let d = self.buf[self.at];
        self.buf[self.at] = x;
        self.at = (self.at + 1) % self.buf.len();
        d
    }
}

/// Everything one track or bus needs for Spread and Spatialize, made up
/// front so playing never allocates.
pub struct Diffuser {
    /// Spread's copies of the shared sound.
    pub spread: Decorrelator,
    /// Side Spread and Height Lift: copies of the difference.
    pub side: Decorrelator,
    /// Ambience: copies of each side.
    pub left: Decorrelator,
    pub right: Decorrelator,
    /// Matrix: the difference, 12 ms late.
    pub delay: Delay,
    /// Place Elements, made when first used (only 3D Space sessions with it on).
    pub steer: Option<Box<Steer>>,
}

impl Diffuser {
    /// `k` varies the delays from one track to the next.
    pub fn new(rate: u32, k: usize) -> Self {
        Self {
            spread: Decorrelator::new(rate, k % 2),
            side: Decorrelator::new(rate, 1 - k % 2),
            left: Decorrelator::new(rate, 2 + k % 2),
            right: Decorrelator::new(rate, 3 - k % 2),
            delay: Delay::new((rate as usize * 12) / 1000),
            steer: None,
        }
    }
}

/// Frame and hop of the Place Elements analysis. Its sound comes out
/// [`STEER_FRAME`] frames late; in 3D Space sessions every placed sound is
/// held back by the same amount, so all of it stays in time.
pub const STEER_FRAME: usize = 1024;
const STEER_HOP: usize = STEER_FRAME / 4;
const BINS: usize = STEER_FRAME / 2 + 1;
/// Directions the panning table holds, around the circle.
const DIRS: usize = 72;

/// Place Elements: a stereo sound opened out into the room part by part.
///
/// The sound is cut into short overlapping slices, each split into its
/// frequencies. For every frequency, how loud it is on each side and how
/// alike the two sides are (smoothed over a few slices) say where it sits:
/// - what is alike on both sides (a voice, a bass, a lead) is direct
///   sound: it is placed in the room by its balance between the sides,
///   centered parts in the middle front, parts panned hard reach the
///   sides of the room, and parts that are alike but in opposite phase
///   (a stereo trick, a wide pad, a phase-flipped double) go behind;
/// - what differs between the sides (reverb tails, room, crowd) is
///   ambience: each side's goes to that side's surround, rear and height
///   speakers (and a little to its front speaker), each speaker with its
///   own phase turn so they do not sound like copies.
///
/// Then each speaker's share is put back together slice by slice.
pub struct Steer {
    fft: std::sync::Arc<dyn realfft::RealToComplex<f32>>,
    ifft: std::sync::Arc<dyn realfft::ComplexToReal<f32>>,
    window: Vec<f32>,
    in_l: Vec<f32>,
    in_r: Vec<f32>,
    fill: usize,
    frame: Vec<f32>,
    spec_l: Vec<realfft::num_complex::Complex<f32>>,
    spec_r: Vec<realfft::num_complex::Complex<f32>>,
    /// Smoothed powers and cross power per frequency.
    pll: Vec<f32>,
    prr: Vec<f32>,
    plr: Vec<realfft::num_complex::Complex<f32>>,
    /// Per speaker: its spectrum this slice, its overlapping output, and
    /// its share of the ambience on each side.
    out_spec: Vec<Vec<realfft::num_complex::Complex<f32>>>,
    acc: Vec<Vec<f32>>,
    amb: Vec<[f32; 2]>,
    /// Per speaker and frequency: the phase turn of its ambience.
    turn: Vec<Vec<realfft::num_complex::Complex<f32>>>,
    /// Panning gains, per direction around the circle, per speaker.
    dirs: Vec<Vec<f32>>,
    scratch: Vec<realfft::num_complex::Complex<f32>>,
    time: Vec<f32>,
    /// This sample's output per speaker, and where reading is in the hop.
    now: Vec<f32>,
    read: usize,
}

impl Steer {
    pub fn new(layout: SpaceLayout) -> Self {
        use realfft::num_complex::Complex;
        let sp = speakers(layout);
        let n = sp.len();
        let mut planner = realfft::RealFftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(STEER_FRAME);
        let ifft = planner.plan_fft_inverse(STEER_FRAME);
        let window = (0..STEER_FRAME)
            .map(|i| (std::f32::consts::PI * i as f32 / STEER_FRAME as f32).sin())
            .collect();
        // Ambience: each side's speakers away from the front row, plus a
        // little of the front speaker on that side; power-normalized per side.
        let mut amb: Vec<[f32; 2]> = sp
            .iter()
            .map(|s| match s.at {
                Some(a) => {
                    let w = if a[1] < 0.5 || a[2] > 0.5 { 1.0 } else { 0.45 };
                    if a[0] < -0.1 {
                        [w, 0.0]
                    } else if a[0] > 0.1 {
                        [0.0, w]
                    } else {
                        [0.0, 0.0]
                    }
                }
                None => [0.0, 0.0],
            })
            .collect();
        for side in 0..2 {
            let p = amb.iter().map(|g| g[side] * g[side]).sum::<f32>().sqrt();
            if p > 0.0 {
                for g in &mut amb {
                    g[side] /= p;
                }
            }
        }
        // Fixed phase turns, different per speaker and frequency.
        let mut seed = 0x2545_f491u32;
        let turn = (0..n)
            .map(|c| {
                (0..BINS)
                    .map(|k| {
                        if amb[c] == [0.0, 0.0] || k == 0 {
                            return Complex::new(1.0, 0.0);
                        }
                        seed ^= seed << 13;
                        seed ^= seed >> 17;
                        seed ^= seed << 5;
                        let ph = seed as f32 / u32::MAX as f32 * std::f32::consts::TAU;
                        Complex::new(ph.cos(), ph.sin())
                    })
                    .collect()
            })
            .collect();
        let dirs = (0..DIRS)
            .map(|d| {
                let th = d as f32 / DIRS as f32 * std::f32::consts::TAU;
                let mut g = crate::dsp::space::point_gains(layout, th.sin(), th.cos(), 0.0, 0.0);
                let p = g.iter().map(|v| v * v).sum::<f32>().sqrt();
                if p > 0.0 {
                    for v in &mut g {
                        *v /= p;
                    }
                }
                g
            })
            .collect();
        let zero = Complex::new(0.0, 0.0);
        Self {
            spec_l: fft.make_output_vec(),
            spec_r: fft.make_output_vec(),
            scratch: fft.make_scratch_vec(),
            fft,
            ifft,
            window,
            in_l: vec![0.0; STEER_FRAME],
            in_r: vec![0.0; STEER_FRAME],
            fill: 0,
            frame: vec![0.0; STEER_FRAME],
            pll: vec![0.0; BINS],
            prr: vec![0.0; BINS],
            plr: vec![zero; BINS],
            out_spec: vec![vec![zero; BINS]; n],
            acc: vec![vec![0.0; STEER_FRAME]; n],
            amb,
            turn,
            dirs,
            time: vec![0.0; STEER_FRAME],
            now: vec![0.0; n],
            read: 0,
        }
    }

    /// Takes one stereo frame and gives one frame per speaker, [`STEER_FRAME`] late.
    pub fn push(&mut self, l: f32, r: f32) -> &[f32] {
        let at = STEER_FRAME - STEER_HOP + self.fill;
        self.in_l[at] = l;
        self.in_r[at] = r;
        for (o, a) in self.now.iter_mut().zip(&self.acc) {
            *o = a[self.read];
        }
        self.read += 1;
        self.fill += 1;
        if self.fill == STEER_HOP {
            self.fill = 0;
            self.read = 0;
            self.slice();
        }
        &self.now
    }

    fn slice(&mut self) {
        use realfft::num_complex::Complex;
        // The output read so far moves out; room for the next hop.
        for a in &mut self.acc {
            a.copy_within(STEER_HOP.., 0);
            a[STEER_FRAME - STEER_HOP..].fill(0.0);
        }
        for (side, spec) in [(0, &mut self.spec_l), (1, &mut self.spec_r)] {
            let src = if side == 0 { &self.in_l } else { &self.in_r };
            for ((f, x), w) in self.frame.iter_mut().zip(src).zip(&self.window) {
                *f = x * w;
            }
            let _ = self.fft.process_with_scratch(&mut self.frame, spec, &mut self.scratch);
        }
        for buf in [&mut self.in_l, &mut self.in_r] {
            buf.copy_within(STEER_HOP.., 0);
        }
        const A: f32 = 0.6;
        const TINY: f32 = 1e-12;
        let n = self.acc.len();
        for spec in &mut self.out_spec {
            spec.fill(Complex::new(0.0, 0.0));
        }
        for k in 0..BINS {
            let (cl, cr) = (self.spec_l[k], self.spec_r[k]);
            self.pll[k] = A * self.pll[k] + (1.0 - A) * cl.norm_sqr();
            self.prr[k] = A * self.prr[k] + (1.0 - A) * cr.norm_sqr();
            self.plr[k] = self.plr[k] * A + cl * cr.conj() * (1.0 - A);
            let (el, er) = (self.pll[k], self.prr[k]);
            let cross = self.plr[k].norm();
            let alike = (cross / (el * er).sqrt().max(TINY)).clamp(0.0, 1.0);
            let pan = ((er - el) / (el + er).max(TINY)).clamp(-1.0, 1.0);
            // In opposite phase: pushed behind.
            let behind = (-self.plr[k].re / cross.max(TINY)).clamp(0.0, 1.0) * alike;
            // The direct part: its strength on both sides together, with
            // the phase of the louder side.
            let base = if cl.norm_sqr() >= cr.norm_sqr() { cl } else { cr };
            let mag = (cl.norm_sqr() + cr.norm_sqr()).sqrt();
            let dir = if base.norm() > TINY { base * (mag / base.norm()) } else { Complex::new(0.0, 0.0) };
            let direct = dir * alike;
            let ambient = (1.0 - alike * alike).max(0.0).sqrt();
            // Where it sits: across the front by its balance, out to the
            // sides when panned hard, toward the back when in opposite phase.
            let front = pan.signum() * pan.abs().powf(0.8) * 80.0;
            let side = if pan.abs() < 0.05 { 1.0 } else { pan.signum() };
            let deg = front + side * behind * (180.0 - front.abs());
            let d = ((deg.rem_euclid(360.0)) / 360.0 * DIRS as f32).round() as usize % DIRS;
            let g = &self.dirs[d];
            for (c, gc) in g.iter().enumerate().take(n) {
                let mut v = direct * gc;
                let [al, ar] = self.amb[c];
                if al > 0.0 || ar > 0.0 {
                    v += (cl * al + cr * ar) * ambient * self.turn[c][k];
                }
                self.out_spec[c][k] = v;
            }
        }
        // Back to sound, each speaker, overlapped and added.
        let scale = 0.5 / STEER_FRAME as f32;
        for c in 0..n {
            let spec = &mut self.out_spec[c];
            spec[0].im = 0.0;
            spec[BINS - 1].im = 0.0;
            let _ = self.ifft.process_with_scratch(spec, &mut self.time, &mut self.scratch);
            for ((a, t), w) in self.acc[c].iter_mut().zip(&self.time).zip(&self.window) {
                *a += t * w * scale;
            }
        }
    }
}

/// What Spread and Spatialize send to each speaker, worked out from a spot.
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    /// Spread: per speaker, the gain for its decorrelated copy (0 off).
    pub spread: Vec<f32>,
    /// Spatialize: per speaker, the gain for the decorrelated side sound
    /// (positive on the left, negative on the right, 0 off).
    pub side: Vec<f32>,
    /// Spatialize: the center speaker and the gain for the shared sound.
    pub center: Option<(usize, f32)>,
    /// Each speaker folded to stereo for listening (left, right).
    pub fold: Vec<[f32; 2]>,
    /// What the direct (placed) sound keeps, so the total stays as loud.
    pub direct: f32,
    /// How Spatialize opens the sound out.
    pub mode: SpatializeMode,
    /// Place Elements: how much of the sound it takes (0 off).
    pub steer: f32,
    /// Something to add: Spread, Spatialize, or both.
    pub on: bool,
    pub spreads: bool,
    pub sides: bool,
}

/// The field for a source at height `z` with `spread` and `spatialize`
/// (0 to 1 each), Spatialize done the `mode` way. Spatialize applies only
/// to stereo sources.
pub fn field(
    layout: SpaceLayout,
    z: f32,
    spread: f32,
    spatialize: f32,
    mode: SpatializeMode,
    stereo: bool,
) -> Field {
    let sp = speakers(layout);
    let n = sp.len();
    let spread = spread.clamp(0.0, 1.0);
    let spat = if stereo {
        spatialize.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let has_top = sp.iter().any(|s| s.at.is_some_and(|a| a[2] > 0.5));
    let fold: Vec<[f32; 2]> = sp
        .iter()
        .map(|s| match s.at {
            Some(a) => {
                let t = ((a[0] + 1.0) / 2.0).clamp(0.0, 1.0) * std::f32::consts::FRAC_PI_2;
                [t.cos(), t.sin()]
            }
            None => [0.0, 0.0],
        })
        .collect();
    let mut f = Field {
        spread: vec![0.0; n],
        side: vec![0.0; n],
        center: None,
        fold,
        direct: 1.0,
        mode,
        steer: 0.0,
        on: spread > 0.0 || spat > 0.0,
        spreads: spread > 0.0,
        sides: spat > 0.0,
    };
    if spread > 0.0 {
        // Every placed speaker of the layers the height uses: the floor
        // always, the top as the sound rises (and both fully at mid height).
        let lift = if has_top { z.clamp(0.0, 1.0) } else { 0.0 };
        let weight = |a: [f32; 3]| {
            if a[2] > 0.5 {
                lift.min(0.5) * 2.0
            } else {
                1.0 - (lift - 0.5).max(0.0)
            }
        };
        let w: Vec<f32> = sp.iter().map(|s| s.at.map_or(0.0, weight)).collect();
        let p = w.iter().map(|v| v * v).sum::<f32>().sqrt().max(1e-6);
        for (o, v) in f.spread.iter_mut().zip(&w) {
            *o = spread.sqrt() * v / p;
        }
        f.direct *= (1.0 - spread).sqrt();
    }
    if spat > 0.0 && mode == SpatializeMode::Steer {
        // Place Elements takes over `spat` of the sound; the placed part keeps the rest.
        f.steer = spat;
        f.direct *= 1.0 - spat;
        return f;
    }
    if spat > 0.0 {
        // The side sound to every speaker not in the front row (Height Lift:
        // the height speakers only, when there are any), left ones in phase
        // and right ones inverted (Ambience: in phase, as each side feeds
        // its own); the center ones stay out of it.
        let heights_only = mode == SpatializeMode::HeightLift && has_top;
        let w: Vec<f32> = sp
            .iter()
            .map(|s| match s.at {
                Some(a)
                    if (heights_only && a[2] > 0.5)
                        || (!heights_only && (a[1] < 0.5 || a[2] > 0.5)) =>
                {
                    if a[0] < -0.1 {
                        1.0
                    } else if a[0] > 0.1 {
                        -1.0
                    } else {
                        0.0
                    }
                }
                _ => 0.0,
            })
            .collect();
        let p = w.iter().map(|v| v * v).sum::<f32>().sqrt();
        if p > 0.0 {
            for (o, v) in f.side.iter_mut().zip(&w) {
                *o = spat * 0.8 * v / p;
            }
        }
        f.center = sp
            .iter()
            .position(|s| {
                s.at.is_some_and(|a| a[0].abs() < 0.1 && a[1] > 0.5 && a[2] < 0.5)
            })
            .map(|c| (c, spat * 0.5));
        // The front pair gives up a little of what moves to the center and around.
        f.direct *= 1.0 - 0.3 * spat;
    }
    f
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_keep_level_but_differ() {
        let rate = 48_000;
        let mut d = Decorrelator::new(rate, 0);
        let n = rate as usize;
        let mut e_in = 0.0f64;
        let mut e_out = [0.0f64; TAPS];
        let mut cross = 0.0f64;
        let mut s = 1u32;
        for _ in 0..n {
            // White noise, the same every run.
            s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let x = (s >> 8) as f32 / (1u32 << 24) as f32 - 0.5;
            let y = d.run(x);
            e_in += f64::from(x * x);
            for (e, v) in e_out.iter_mut().zip(y) {
                *e += f64::from(v * v);
            }
            cross += f64::from(y[0] * y[1]);
        }
        for e in e_out {
            assert!((e / e_in - 1.0).abs() < 0.1, "level kept: {}", e / e_in);
        }
        let corr = cross / (e_out[0] * e_out[1]).sqrt();
        assert!(corr.abs() < 0.1, "copies differ: {corr}");
    }

    #[test]
    fn each_spatialize_method_feeds_its_own_speakers() {
        use SpatializeMode as M;
        let l = SpaceLayout::Surround714;
        let sp = speakers(l);
        let height = |i: usize| sp[i].at.is_some_and(|a| a[2] > 0.5);
        // Height Lift: only the height speakers get the side sound.
        let f = field(l, 0.0, 0.0, 1.0, M::HeightLift, true);
        assert!(f
            .side
            .iter()
            .enumerate()
            .all(|(i, v)| *v == 0.0 || height(i)));
        assert!(f
            .side
            .iter()
            .enumerate()
            .any(|(i, v)| *v != 0.0 && height(i)));
        // Side Spread reaches the surrounds as well.
        let f = field(l, 0.0, 0.0, 1.0, M::SideSpread, true);
        assert!(f
            .side
            .iter()
            .enumerate()
            .any(|(i, v)| *v != 0.0 && !height(i)));
        // Without heights, Height Lift falls back to the surrounds.
        let f = field(SpaceLayout::Surround51, 0.0, 0.0, 1.0, M::HeightLift, true);
        assert!(f.side[4] > 0.0 && f.side[5] < 0.0);
        // Every method keeps the center feed and tells the engine which it is.
        for m in [M::SideSpread, M::Matrix, M::Ambience, M::HeightLift] {
            let f = field(l, 0.0, 0.0, 1.0, m, true);
            assert_eq!(f.mode, m);
            assert!(f.center.is_some());
        }
    }

    #[test]
    fn the_matrix_delay_is_a_plain_delay() {
        let mut d = Delay::new(3);
        let out: Vec<f32> = [1.0, 2.0, 3.0, 4.0, 5.0]
            .iter()
            .map(|v| d.run(*v))
            .collect();
        assert_eq!(out, vec![0.0, 0.0, 0.0, 1.0, 2.0]);
    }

    #[test]
    fn fields_keep_power_and_spread_where_they_should() {
        let f = field(
            SpaceLayout::Surround714,
            0.0,
            1.0,
            0.0,
            SpatializeMode::SideSpread,
            false,
        );
        let p: f32 = f.spread.iter().map(|v| v * v).sum();
        assert!((p - 1.0).abs() < 1e-4, "{p}");
        assert_eq!(f.direct, 0.0);
        // At ear level the height speakers stay out of it; the LFE always does.
        assert_eq!(f.spread[8], 0.0);
        assert_eq!(f.spread[3], 0.0);
        let f = field(
            SpaceLayout::Surround51,
            0.0,
            0.0,
            1.0,
            SpatializeMode::SideSpread,
            true,
        );
        // Rear left in phase, rear right inverted, front pair out of it, center fed.
        assert!(f.side[4] > 0.0 && f.side[5] < 0.0);
        assert_eq!(f.side[0], 0.0);
        assert_eq!(f.center.map(|c| c.0), Some(2));
        // Mono sources have no sides to spatialize.
        assert!(
            !field(
                SpaceLayout::Surround51,
                0.0,
                0.0,
                1.0,
                SpatializeMode::SideSpread,
                false
            )
            .on
        );
    }

    /// The energy each speaker of 7.1 gets from Place Elements for a tone
    /// at `l` and `r` gains (negative flips the phase).
    fn steer_energy(l: f32, r: f32) -> Vec<f32> {
        let mut st = Steer::new(SpaceLayout::Surround71);
        let n = speakers(SpaceLayout::Surround71).len();
        let mut e = vec![0f32; n];
        for i in 0..48_000 {
            let x = (i as f32 * 0.0573).sin();
            let out = st.push(x * l, x * r);
            if i > 8_000 {
                for (a, v) in e.iter_mut().zip(out) {
                    *a += v * v;
                }
            }
        }
        e
    }

    #[test]
    fn place_elements_puts_each_part_where_it_sits() {
        let names: Vec<&str> = speakers(SpaceLayout::Surround71).iter().map(|s| s.name).collect();
        let at = |n: &str| names.iter().position(|x| *x == n).unwrap();
        let loudest = |e: &[f32]| (0..e.len()).max_by(|a, b| e[*a].total_cmp(&e[*b])).unwrap();
        // Centered: the center speaker.
        assert_eq!(names[loudest(&steer_energy(1.0, 1.0))], "C");
        // Hard left: the left side of the room, not the front right.
        let e = steer_energy(1.0, 0.0);
        let left: f32 = names.iter().zip(&e).filter(|(n, _)| n.starts_with('L')).map(|(_, v)| v).sum();
        assert!(left > 50.0 * e[at("R")], "{e:?}");
        // Alike but in opposite phase: behind.
        let e = steer_energy(1.0, -1.0);
        let back = e[at("Lrs")] + e[at("Rrs")];
        assert!(back > 4.0 * (e[at("L")] + e[at("R")] + e[at("C")]), "{e:?}");
        assert!(e.iter().all(|v| v.is_finite()));
    }

}
