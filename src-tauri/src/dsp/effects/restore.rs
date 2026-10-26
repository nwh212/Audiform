// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Restoration effects: Noise Reducer, Hiss Reducer, Room Reducer,
//! Click Repair, Clip Repair and DC Offset Fix. Original code built from
//! textbook methods: noise floors tracked per frequency from the
//! quietest recent frames, spectral gain with smoothing to keep it from
//! sounding watery, a decaying-tail model for room sound, and short gaps
//! filled with a smooth curve from the samples on each side.

use super::{db_to_gain, num, EffectInfo, Params, Processor};
use crate::dsp::stft::{Mask, Stft};

pub fn noise_info() -> EffectInfo {
    EffectInfo {
        id: "altaudio.noise_reducer",
        name: "Noise Reducer",
        category: "Restoration",
        description: "Learns the steady background (fans, hum, room tone) as it plays and turns it down under the voice.",
        params: vec![
            num("reduction", "Reduction", "dB", 0.0, 40.0, 12.0, 0.5),
            num("strength", "Strength", "", 1.0, 4.0, 2.0, 0.1),
            num("learn", "Learn time", "s", 0.5, 10.0, 2.0, 0.1).log(),
            num("smoothing", "Smoothing", "%", 0.0, 100.0, 50.0, 1.0),
        ],
    }
}

pub fn hiss_info() -> EffectInfo {
    EffectInfo {
        id: "altaudio.hiss_reducer",
        name: "Hiss Reducer",
        category: "Restoration",
        description: "Turns down tape and preamp hiss in the highs, leaving the lows alone.",
        params: vec![
            num("reduction", "Reduction", "dB", 0.0, 40.0, 10.0, 0.5),
            num("strength", "Strength", "", 1.0, 4.0, 1.8, 0.1),
            num("freq", "Above", "Hz", 1000.0, 12_000.0, 3000.0, 10.0).log(),
            num("learn", "Learn time", "s", 0.5, 10.0, 2.0, 0.1).log(),
        ],
    }
}

pub fn room_info() -> EffectInfo {
    EffectInfo {
        id: "altaudio.room_reducer",
        name: "Room Reducer",
        category: "Restoration",
        description:
            "Shortens the echo of a live room after each word, so the voice sounds closer.",
        params: vec![
            num("amount", "Amount", "%", 0.0, 100.0, 50.0, 1.0),
            num("decay", "Room decay", "s", 0.1, 3.0, 0.6, 0.01).log(),
            num("reduction", "Most reduction", "dB", 0.0, 30.0, 12.0, 0.5),
        ],
    }
}

pub fn click_info() -> EffectInfo {
    EffectInfo {
        id: "altaudio.click_repair",
        name: "Click Repair",
        category: "Restoration",
        description:
            "Finds short clicks, pops and mouth noises and fills them from the sound on each side.",
        params: vec![
            num("sensitivity", "Sensitivity", "%", 0.0, 100.0, 50.0, 1.0),
            num("width", "Longest click", "ms", 0.1, 3.0, 1.0, 0.1),
        ],
    }
}

pub fn clip_info() -> EffectInfo {
    EffectInfo {
        id: "altaudio.clip_repair",
        name: "Clip Repair",
        category: "Restoration",
        description: "Rebuilds peaks that were flattened by recording too hot, then lowers the level to make room.",
        params: vec![
            num("level", "Clipped above", "dB", -12.0, 0.0, -0.5, 0.1),
            num("longest", "Longest flat run", "ms", 0.1, 10.0, 3.0, 0.1),
            num("output", "Output level", "dB", -12.0, 0.0, -3.0, 0.1),
        ],
    }
}

pub fn dc_info() -> EffectInfo {
    EffectInfo {
        id: "altaudio.dc_offset",
        name: "DC Offset Fix",
        category: "Restoration",
        description: "Centers a waveform that sits above or below zero, which some recorders and interfaces add.",
        params: vec![],
    }
}

// --- Spectral reducers ---

/// Mean bin power below this is digital silence, not background noise
/// (far below any recorder's noise floor).
const DIGITAL_SILENCE: f32 = 1e-14;

/// How far the average noise sits above its tracked minimum.
const MIN_BIAS: f32 = 2.5;

/// Background noise per bin, from the quietest recent frames.
pub struct NoiseMask {
    /// Bins below this are left alone (Hiss Reducer).
    from_bin: usize,
    floor: f32,
    strength: f32,
    /// Per frame: how much the floor may rise (it falls at once).
    rise: f32,
    /// Gain smoothing over time, 0 to nearly 1.
    smooth: f32,
    power_smooth: f32,
    smoothed: Vec<f32>,
    noise: Vec<f32>,
    last: Vec<f32>,
    frames: u32,
}

impl NoiseMask {
    fn new(
        bins: usize,
        from_bin: usize,
        reduction_db: f64,
        strength: f64,
        learn_s: f64,
        smoothing: f64,
        fps: f64,
    ) -> Self {
        // The floor can climb 10 dB within the learn time, so a new,
        // louder background is picked up but speech never is.
        let rise = 10f64.powf(1.0 / (learn_s * fps)) as f32;
        Self {
            from_bin,
            floor: db_to_gain(-reduction_db),
            strength: strength as f32,
            rise,
            smooth: (smoothing / 100.0 * 0.9) as f32,
            power_smooth: 0.7,
            smoothed: vec![0.0; bins],
            noise: vec![f32::MAX; bins],
            last: vec![1.0; bins],
            frames: 0,
        }
    }
}

impl Mask for NoiseMask {
    fn mask(&mut self, power: &[f32], gains: &mut [f32]) {
        // Digital silence (the gaps between clips, a muted stretch) says
        // nothing about the background noise. Learning from it would
        // drop the noise estimate to zero and switch the reducer off for
        // every clip after the first gap, so the estimate holds still.
        let mean = power.iter().sum::<f32>() / power.len().max(1) as f32;
        if mean < DIGITAL_SILENCE {
            gains.copy_from_slice(&self.last);
            return;
        }
        self.frames = self.frames.saturating_add(1);
        let a = self.power_smooth;
        let first = self.frames == 1;
        for ((s, n), p) in self
            .smoothed
            .iter_mut()
            .zip(self.noise.iter_mut())
            .zip(power)
        {
            *s = if first { *p } else { a * *s + (1.0 - a) * p };
            *n = if *s < *n {
                *s
            } else {
                (*n * self.rise).min(*s)
            };
        }
        for k in 0..power.len() {
            if k < self.from_bin {
                gains[k] = 1.0;
                continue;
            }
            // Average three neighbouring bins so single bins do not flicker.
            let lo = k.saturating_sub(1);
            let hi = (k + 1).min(power.len() - 1);
            let p = (power[lo] + power[k] + power[hi]) / 3.0 + 1e-20;
            // The quietest frames sit well below the average noise, so the
            // tracked minimum is scaled up to the noise's typical level.
            let nz = (self.noise[lo] + self.noise[k] + self.noise[hi]) / 3.0 * MIN_BIAS;
            let want = (1.0 - self.strength * nz / p)
                .max(0.0)
                .sqrt()
                .max(self.floor);
            // Open fast, close gently.
            let g = if want > self.last[k] {
                want
            } else {
                want + (self.last[k] - want) * self.smooth
            };
            self.last[k] = g;
            gains[k] = g;
        }
    }

    fn reset(&mut self) {
        self.smoothed.fill(0.0);
        self.noise.fill(f32::MAX);
        self.last.fill(1.0);
        self.frames = 0;
    }
}

/// Late room sound per bin: an earlier frame's power, decayed by the
/// room's decay time. Whatever is louder than that is the direct voice.
pub struct RoomMask {
    amount: f32,
    floor: f32,
    /// Power falls by this over `delay` frames.
    decay: f32,
    history: Vec<Vec<f32>>,
    at: usize,
    last: Vec<f32>,
    smoothed: Vec<f32>,
}

impl RoomMask {
    fn new(bins: usize, amount: f64, decay_s: f64, reduction_db: f64, fps: f64) -> Self {
        // Look back about 50 ms: past the direct sound and early reflections.
        let delay = (0.05 * fps).round().max(1.0) as usize;
        let dt = delay as f64 / fps;
        // -60 dB over the decay time, in power.
        let decay = 10f64.powf(-6.0 * dt / decay_s) as f32;
        Self {
            amount: (amount / 100.0 * 1.5) as f32,
            floor: db_to_gain(-reduction_db),
            decay,
            history: vec![vec![0.0; bins]; delay],
            at: 0,
            last: vec![1.0; bins],
            smoothed: vec![0.0; bins],
        }
    }
}

impl Mask for RoomMask {
    fn mask(&mut self, power: &[f32], gains: &mut [f32]) {
        // Light smoothing over time: single frames of noise are too jumpy
        // to compare one by one.
        for (s, p) in self.smoothed.iter_mut().zip(power) {
            *s = 0.6 * *s + 0.4 * p;
        }
        let old = &self.history[self.at];
        for k in 0..power.len() {
            let tail = old[k] * self.decay;
            let p = self.smoothed[k] + 1e-20;
            let want = (1.0 - self.amount * tail / p)
                .max(0.0)
                .sqrt()
                .max(self.floor);
            let g = if want > self.last[k] {
                want
            } else {
                want + (self.last[k] - want) * 0.5
            };
            self.last[k] = g;
            gains[k] = g;
        }
        self.history[self.at].copy_from_slice(&self.smoothed);
        self.at = (self.at + 1) % self.history.len();
    }

    fn reset(&mut self) {
        for h in &mut self.history {
            h.fill(0.0);
        }
        self.at = 0;
        self.last.fill(1.0);
        self.smoothed.fill(0.0);
    }
}

pub struct Spectral<M: Mask>(pub Stft<M>);

impl<M: Mask> Processor for Spectral<M> {
    fn process(&mut self, buf: &mut [f32]) {
        self.0.process(buf);
    }
    fn latency(&self) -> usize {
        self.0.latency()
    }
    fn reset(&mut self) {
        self.0.reset();
    }
}

pub fn noise_reducer(p: &Params, rate: u32) -> Spectral<NoiseMask> {
    let n = Stft::<NoiseMask>::size_for(40.0, rate);
    let fps = f64::from(rate) / (n / 4) as f64;
    let mask = NoiseMask::new(
        n / 2 + 1,
        0,
        p.get("reduction"),
        p.get("strength"),
        p.get("learn"),
        p.get("smoothing"),
        fps,
    );
    Spectral(Stft::new(n, mask))
}

pub fn hiss_reducer(p: &Params, rate: u32) -> Spectral<NoiseMask> {
    let n = Stft::<NoiseMask>::size_for(40.0, rate);
    let fps = f64::from(rate) / (n / 4) as f64;
    let from = (p.get("freq") / (f64::from(rate) / n as f64)).floor() as usize;
    let mask = NoiseMask::new(
        n / 2 + 1,
        from,
        p.get("reduction"),
        p.get("strength"),
        p.get("learn"),
        60.0,
        fps,
    );
    Spectral(Stft::new(n, mask))
}

pub fn room_reducer(p: &Params, rate: u32) -> Spectral<RoomMask> {
    let n = Stft::<RoomMask>::size_for(40.0, rate);
    let fps = f64::from(rate) / (n / 4) as f64;
    let mask = RoomMask::new(
        n / 2 + 1,
        p.get("amount"),
        p.get("decay"),
        p.get("reduction"),
        fps,
    );
    Spectral(Stft::new(n, mask))
}

// --- Short gap repair (clicks and clipped peaks) ---

#[derive(Clone, Copy, PartialEq)]
enum Find {
    /// A spike far bigger than the signal's usual sample-to-sample change.
    Click { factor: f32 },
    /// A run of samples stuck at or above the clip level.
    Clip { level: f32 },
}

/// One channel. Samples wait `delay` frames so a span found now can be
/// filled before it leaves.
struct GapFixer {
    find: Find,
    longest: usize,
    ring: Vec<f32>,
    /// Absolute index of the next sample in.
    t: u64,
    /// Typical size of the second difference (Click).
    typical: f32,
    /// Open span: first bad sample, last bad sample.
    open: Option<(u64, u64)>,
    clip_sign: f32,
    /// Every span filled so far, by input sample index (for QuickPass).
    found: Vec<(u64, u64)>,
}

impl GapFixer {
    fn new(find: Find, longest: usize) -> Self {
        Self {
            find,
            longest: longest.max(1),
            ring: vec![0.0; longest.max(1) + 8],
            t: 0,
            typical: 1e-4,
            open: None,
            clip_sign: 0.0,
            found: Vec::new(),
        }
    }

    fn delay(&self) -> usize {
        self.ring.len()
    }

    fn at(&self, i: u64) -> f32 {
        self.ring[(i % self.ring.len() as u64) as usize]
    }

    fn set(&mut self, i: u64, v: f32) {
        let n = self.ring.len() as u64;
        self.ring[(i % n) as usize] = v;
    }

    /// Fills `s..=e` with a smooth curve from two good samples on each side.
    fn fill(&mut self, s: u64, e: u64) {
        let len = (e - s + 1) as f32;
        let (p0, p1) = (self.at(s - 2), self.at(s - 1));
        let (p2, p3) = (self.at(e + 1), self.at(e + 2));
        let m1 = (p1 - p0) * (len + 1.0);
        let m2 = (p3 - p2) * (len + 1.0);
        for (j, i) in (s..=e).enumerate() {
            let u = (j as f32 + 1.0) / (len + 1.0);
            let (u2, u3) = (u * u, u * u * u);
            let v = (2.0 * u3 - 3.0 * u2 + 1.0) * p1
                + (u3 - 2.0 * u2 + u) * m1
                + (-2.0 * u3 + 3.0 * u2) * p2
                + (u3 - u2) * m2;
            self.set(i, v);
        }
    }

    fn step(&mut self, x: f32) -> f32 {
        let n = self.ring.len() as u64;
        // The sample leaving now went in `n` steps ago.
        let out = if self.t >= n {
            self.at(self.t - n)
        } else {
            0.0
        };
        self.set(self.t, x);
        let t = self.t;
        self.t += 1;
        if t < 3 {
            return out;
        }
        let bad = match self.find {
            Find::Click { factor } => {
                let d = (x - 2.0 * self.at(t - 1) + self.at(t - 2)).abs();
                let bad = d > factor * self.typical && d > 0.01;
                if !bad {
                    // Learn the usual roughness from clean samples only.
                    self.typical += (d - self.typical) * 0.002;
                    self.typical = self.typical.max(1e-5);
                }
                bad
            }
            Find::Clip { level } => {
                let stuck = x.abs() >= level;
                if stuck && self.open.is_some() && x.signum() != self.clip_sign {
                    false
                } else {
                    if stuck {
                        self.clip_sign = x.signum();
                    }
                    stuck
                }
            }
        };
        match (self.open, bad) {
            (None, true) => self.open = Some((t, t)),
            (Some((s, _)), true) => self.open = Some((s, t)),
            (Some((s, e)), false) if t >= e + 2 => {
                self.open = None;
                let len = (e - s + 1) as usize;
                let room = s >= 2 && (t + 1).saturating_sub(s - 2) <= n;
                let worth = match self.find {
                    // Clipping needs a few stuck samples to be clipping at all.
                    Find::Clip { .. } => len >= 3,
                    Find::Click { .. } => true,
                };
                if len <= self.longest && room && worth {
                    self.fill(s, e);
                    self.found.push((s, e));
                }
            }
            _ => {}
        }
        out
    }

    fn reset(&mut self) {
        self.ring.fill(0.0);
        self.t = 0;
        self.typical = 1e-4;
        self.open = None;
        self.found.clear();
    }
}

pub struct GapRepair {
    ch: [GapFixer; 2],
    gain: f32,
}

impl GapRepair {
    pub fn clicks(p: &Params, rate: u32) -> Self {
        // 0% sensitivity needs a spike 40 times the usual change; 100% needs 4.
        let factor = (40.0 * 0.1f64.powf(p.get("sensitivity") / 100.0)) as f32;
        let longest = (p.get("width") * f64::from(rate) / 1000.0).round() as usize + 2;
        let f = || GapFixer::new(Find::Click { factor }, longest);
        Self {
            ch: [f(), f()],
            gain: 1.0,
        }
    }

    pub fn clipping(p: &Params, rate: u32) -> Self {
        let level = db_to_gain(p.get("level")) * 0.999;
        let longest = (p.get("longest") * f64::from(rate) / 1000.0).round() as usize;
        let f = || GapFixer::new(Find::Clip { level }, longest);
        Self {
            ch: [f(), f()],
            gain: db_to_gain(p.get("output")),
        }
    }
}

/// What QuickPass looks for in a clip.
#[derive(Debug, Clone, Copy, PartialEq, serde::Deserialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "kind"
)]
pub enum Spots {
    /// Clicks and pops: `sensitivity` 0 to 100, `width_ms` the longest click.
    Clicks { sensitivity: f64, width_ms: f64 },
    /// Flattened peaks at or above `level_db`, no longer than `longest_ms`.
    Clipping { level_db: f64, longest_ms: f64 },
}

fn fixer(spots: Spots, rate: u32) -> GapFixer {
    match spots {
        Spots::Clicks {
            sensitivity,
            width_ms,
        } => {
            let factor = (40.0 * 0.1f64.powf(sensitivity.clamp(0.0, 100.0) / 100.0)) as f32;
            let longest =
                (width_ms.clamp(0.1, 3.0) * f64::from(rate) / 1000.0).round() as usize + 2;
            GapFixer::new(Find::Click { factor }, longest)
        }
        Spots::Clipping {
            level_db,
            longest_ms,
        } => {
            let level = db_to_gain(level_db.clamp(-12.0, 0.0)) * 0.999;
            let longest = (longest_ms.clamp(0.1, 10.0) * f64::from(rate) / 1000.0).round() as usize;
            GapFixer::new(Find::Clip { level }, longest)
        }
    }
}

/// Merges spans that touch or sit within `gap` frames of each other.
fn merge(mut spans: Vec<(u64, u64)>, gap: u64) -> Vec<(u64, u64)> {
    spans.sort_unstable();
    let mut out: Vec<(u64, u64)> = Vec::new();
    for (s, e) in spans {
        match out.last_mut() {
            Some(last) if s <= last.1 + gap => last.1 = last.1.max(e),
            _ => out.push((s, e)),
        }
    }
    out
}

/// The spots in interleaved audio with `ch` channels, as frame spans
/// (first, last), each channel checked on its own.
pub fn find_spots(audio: &[f32], ch: usize, rate: u32, spots: Spots) -> Vec<(u64, u64)> {
    let ch = ch.max(1);
    let mut all = Vec::new();
    for c in 0..ch {
        let mut f = fixer(spots, rate);
        for frame in audio.chunks_exact(ch) {
            f.step(frame[c]);
        }
        for _ in 0..f.delay() {
            f.step(0.0);
        }
        // The silence that flushes the delay is not part of the audio, so
        // a jump into it at the very end is no click.
        let frames = (audio.len() / ch) as u64;
        all.extend(f.found.into_iter().filter(|(_, e)| *e + 2 < frames));
    }
    merge(all, (rate / 1000) as u64)
}

/// Repairs only `spans` (frames) in interleaved audio, in place. The
/// rest of the audio is left exactly as it was.
pub fn repair_spots(audio: &mut [f32], ch: usize, rate: u32, spots: Spots, spans: &[(u64, u64)]) {
    let ch = ch.max(1);
    let frames = audio.len() / ch;
    // A little either side, so a span found on the played audio covers
    // the same spot in the file.
    let margin = (rate / 500) as u64;
    let mut keep = vec![false; frames];
    for &(s, e) in spans {
        let a = s.saturating_sub(margin) as usize;
        let b = ((e + margin) as usize).min(frames.saturating_sub(1));
        for k in keep.iter_mut().take(b + 1).skip(a) {
            *k = true;
        }
    }
    for c in 0..ch {
        let mut f = fixer(spots, rate);
        let d = f.delay();
        let input: Vec<f32> = audio.iter().skip(c).step_by(ch).copied().collect();
        let mut out = Vec::with_capacity(frames + d);
        for &x in &input {
            out.push(f.step(x));
        }
        for _ in 0..d {
            out.push(f.step(0.0));
        }
        for (i, k) in keep.iter().enumerate() {
            if *k {
                audio[i * ch + c] = out[i + d];
            }
        }
    }
}

impl Processor for GapRepair {
    fn process(&mut self, buf: &mut [f32]) {
        for f in buf.as_chunks_mut::<2>().0 {
            f[0] = self.ch[0].step(f[0]) * self.gain;
            f[1] = self.ch[1].step(f[1]) * self.gain;
        }
    }
    fn latency(&self) -> usize {
        self.ch[0].delay()
    }
    fn reset(&mut self) {
        for c in &mut self.ch {
            c.reset();
        }
    }
}

/// Removes a constant offset with a very low cut (about 5 Hz).
pub struct DcBlock {
    r: f32,
    x1: [f32; 2],
    y1: [f32; 2],
}

impl DcBlock {
    pub fn new(rate: u32) -> Self {
        Self {
            r: 1.0 - (std::f64::consts::TAU * 5.0 / f64::from(rate)) as f32,
            x1: [0.0; 2],
            y1: [0.0; 2],
        }
    }
}

impl Processor for DcBlock {
    fn process(&mut self, buf: &mut [f32]) {
        for f in buf.as_chunks_mut::<2>().0 {
            for (c, v) in f.iter_mut().enumerate() {
                let y = *v - self.x1[c] + self.r * self.y1[c];
                self.x1[c] = *v;
                self.y1[c] = y;
                *v = y;
            }
        }
    }
    fn reset(&mut self) {
        self.x1 = [0.0; 2];
        self.y1 = [0.0; 2];
    }
}

#[cfg(test)]
mod tests {
    use super::super::build;
    use super::super::tests::slot;
    use std::f32::consts::PI;

    const R: usize = 48_000;

    /// A deterministic noise source.
    fn noise(n: usize, amp: f32, seed: u32) -> Vec<f32> {
        let mut s = seed;
        (0..n)
            .map(|_| {
                s ^= s << 13;
                s ^= s >> 17;
                s ^= s << 5;
                (s as f32 / u32::MAX as f32 * 2.0 - 1.0) * amp
            })
            .collect()
    }

    fn stereo(m: &[f32]) -> Vec<f32> {
        m.iter().flat_map(|v| [*v, *v]).collect()
    }

    fn run(effect: &str, params: &[(&str, f64)], mono: &[f32]) -> (Vec<f32>, usize) {
        let mut p = build(&slot(effect, params), R as u32).unwrap();
        let lat = p.latency();
        let mut b = stereo(mono);
        for c in b.chunks_mut(2 * 1000) {
            p.process(c);
        }
        (b.iter().step_by(2).copied().collect(), lat)
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len().max(1) as f32).sqrt()
    }

    fn db(x: f32) -> f32 {
        20.0 * x.max(1e-9).log10()
    }

    #[test]
    fn noise_reducer_lowers_the_background_and_keeps_the_voice() {
        // Steady noise throughout; a tone (the "voice") on for 300 ms, off for 300 ms.
        let total = R * 6;
        let mut x = noise(total, 0.02, 7);
        for (i, v) in x.iter_mut().enumerate() {
            if (i / (R * 3 / 10)) % 2 == 1 {
                *v += (2.0 * PI * 440.0 * i as f32 / R as f32).sin() * 0.3;
            }
        }
        let (y, lat) = run("altaudio.noise_reducer", &[("reduction", 15.0)], &x);
        // Compare gaps and tone bursts in the second half, once it has learned.
        let seg = R * 3 / 10;
        let mut gap_in = 0.0;
        let mut gap_out = 0.0;
        let mut tone_in = 0.0;
        let mut tone_out = 0.0;
        for k in 10..18 {
            let a = k * seg + seg / 4;
            let b = a + seg / 2;
            let i = rms(&x[a..b]);
            let o = rms(&y[a + lat..b + lat]);
            if k % 2 == 0 {
                gap_in += i;
                gap_out += o;
            } else {
                tone_in += i;
                tone_out += o;
            }
        }
        let gap_cut = db(gap_in) - db(gap_out);
        let tone_cut = db(tone_in) - db(tone_out);
        assert!(gap_cut > 9.0, "background came down {gap_cut} dB");
        assert!(tone_cut < 1.5, "voice kept within {tone_cut} dB");
    }

    #[test]
    fn hiss_reducer_leaves_the_lows_alone() {
        let x: Vec<f32> = (0..R * 3)
            .map(|i| (2.0 * PI * 150.0 * i as f32 / R as f32).sin() * 0.3)
            .zip(noise(R * 3, 0.01, 3))
            .map(|(a, b)| a + b)
            .collect();
        let (y, lat) = run("altaudio.hiss_reducer", &[], &x);
        let a = R * 2;
        let low = |s: &[f32]| {
            // Crude low-pass: a 64-sample moving average keeps the 150 Hz tone.
            let w: Vec<f32> = s
                .windows(64)
                .map(|w| w.iter().sum::<f32>() / 64.0)
                .collect();
            rms(&w)
        };
        let lo_in = low(&x[a..a + R / 2]);
        let lo_out = low(&y[a + lat..a + lat + R / 2]);
        assert!((db(lo_in) - db(lo_out)).abs() < 0.7, "lows kept");
        assert!(
            rms(&y[a + lat..a + lat + R / 2]) < rms(&x[a..a + R / 2]),
            "some hiss removed"
        );
    }

    #[test]
    fn room_reducer_shortens_a_decaying_tail() {
        // A burst, then an exponentially decaying noise tail (0.8 s room).
        let mut x = vec![0f32; R * 3];
        let n = noise(R * 3, 1.0, 11);
        for i in 0..R * 2 {
            let t = i as f32 / R as f32;
            let env = if t < 0.3 {
                0.5
            } else {
                0.5 * 10f32.powf(-3.0 * (t - 0.3) / 0.8)
            };
            x[i] = n[i] * env;
        }
        let (y, lat) = run(
            "altaudio.room_reducer",
            &[("amount", 80.0), ("decay", 0.8)],
            &x,
        );
        let tail = |s: &[f32], o: usize| rms(&s[o + R * 45 / 100..o + R * 7 / 10]);
        let burst = |s: &[f32], o: usize| rms(&s[o + R / 10..o + R / 4]);
        assert!(db(tail(&x, 0)) - db(tail(&y, lat)) > 4.0, "tail is shorter");
        assert!(
            db(burst(&x, 0)) - db(burst(&y, lat)) < 2.0,
            "direct sound kept"
        );
    }

    #[test]
    fn click_repair_fills_a_click_and_leaves_clean_audio() {
        let clean: Vec<f32> = (0..R)
            .map(|i| (2.0 * PI * 300.0 * i as f32 / R as f32).sin() * 0.3)
            .collect();
        let mut x = clean.clone();
        for (k, at) in [12_000usize, 25_000, 37_000].into_iter().enumerate() {
            x[at] += if k % 2 == 0 { 0.6 } else { -0.5 };
            x[at + 1] += 0.3;
        }
        let (y, lat) = run("altaudio.click_repair", &[], &x);
        for at in [12_000usize, 25_000, 37_000] {
            let err = (at - 3..at + 4)
                .map(|i| (y[i + lat] - clean[i]).abs())
                .fold(0f32, f32::max);
            assert!(err < 0.02, "click at {at} left {err}");
        }
        let (z, lat) = run("altaudio.click_repair", &[], &clean);
        let worst = (1000..R - lat)
            .map(|i| (z[i + lat] - clean[i]).abs())
            .fold(0f32, f32::max);
        assert!(worst < 1e-6, "clean audio untouched: {worst}");
    }

    #[test]
    fn clip_repair_rebuilds_flattened_peaks() {
        let clean: Vec<f32> = (0..R / 2)
            .map(|i| (2.0 * PI * 200.0 * i as f32 / R as f32).sin() * 1.3)
            .collect();
        let x: Vec<f32> = clean.iter().map(|v| v.clamp(-0.95, 0.95)).collect();
        let (y, lat) = run(
            "altaudio.clip_repair",
            &[("level", -0.45), ("output", 0.0), ("longest", 5.0)],
            &x,
        );
        let err = |s: &[f32], o: usize| {
            (1000..R / 2 - 1000)
                .map(|i| (s[i + o] - clean[i]).abs())
                .fold(0f32, f32::max)
        };
        assert!(
            err(&y, lat) < err(&x, 0) * 0.35,
            "peaks come back: {} vs {}",
            err(&y, lat),
            err(&x, 0)
        );
    }

    #[test]
    fn spot_requests_read_from_the_interface() {
        use super::Spots;
        let c: Spots =
            serde_json::from_str(r#"{"kind":"clicks","sensitivity":40,"widthMs":0.8}"#).unwrap();
        assert_eq!(
            c,
            Spots::Clicks {
                sensitivity: 40.0,
                width_ms: 0.8
            }
        );
        let p: Spots =
            serde_json::from_str(r#"{"kind":"clipping","levelDb":-0.5,"longestMs":3}"#).unwrap();
        assert_eq!(
            p,
            Spots::Clipping {
                level_db: -0.5,
                longest_ms: 3.0
            }
        );
    }

    #[test]
    fn quickpass_finds_and_repairs_only_the_chosen_spots() {
        use super::{find_spots, repair_spots, Spots};
        let clean: Vec<f32> = (0..R)
            .map(|i| (2.0 * PI * 300.0 * i as f32 / R as f32).sin() * 0.3)
            .collect();
        let mut x = clean.clone();
        for at in [10_000usize, 30_000] {
            x[at] += 0.6;
        }
        let spots = Spots::Clicks {
            sensitivity: 50.0,
            width_ms: 1.0,
        };
        let found = find_spots(&x, 1, R as u32, spots);
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(found[0].0 <= 10_000 && found[0].1 >= 10_000);
        // Fix only the first one.
        let mut y = x.clone();
        repair_spots(&mut y, 1, R as u32, spots, &found[..1]);
        assert!((y[10_000] - clean[10_000]).abs() < 0.02, "first fixed");
        assert_eq!(y[30_000], x[30_000], "second left alone");
        let untouched = (0..R)
            .filter(|i| !(9_900..10_100).contains(i))
            .all(|i| y[i] == x[i]);
        assert!(untouched, "nothing else changes");
    }

    #[test]
    fn dc_offset_is_removed() {
        let x: Vec<f32> = (0..R)
            .map(|i| (2.0 * PI * 200.0 * i as f32 / R as f32).sin() * 0.3 + 0.2)
            .collect();
        let (y, _) = run("altaudio.dc_offset", &[], &x);
        let mean = y[R / 2..].iter().sum::<f32>() / (R / 2) as f32;
        assert!(mean.abs() < 0.005, "{mean}");
    }
}
