// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Amp Model: plays a neural amp capture (a `.nam` file, see
//! `dsp::nam`) with a noise gate before it, a three-band tone stack and
//! level matching after it, and an optional cabinet impulse response at
//! the end. The capture and cabinet live in the slot's `data` (JSON), so
//! a session or preset carries them with it.
//!
//! Captures are mono: the two channels are summed going in and the
//! result is sent to both.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use realfft::num_complex::Complex32;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};
use serde::{Deserialize, Serialize};

use super::{db_to_gain, num, toggle, EffectInfo, Params, Processor};
use crate::dsp::biquad::{Biquad, Coeffs, Shape};
use crate::dsp::nam::{self, Model, ModelInfo};

pub const AMP: &str = "altaudio.amp_model";

/// Captures are leveled to this when Level Match is on, dB.
const TARGET_DB: f64 = -18.0;
/// Cabinet responses are cut to this long, seconds.
pub const IR_MAX_SECONDS: f64 = 1.0;
/// Cabinet convolution block: also the delay it adds, in frames.
const PART: usize = 128;

pub fn amp_info() -> EffectInfo {
    EffectInfo {
        id: AMP,
        name: "Amp Model",
        category: "Amplitude",
        description: "Plays a neural amp capture (a .nam file) of a real amp, pedal or full rig, with a noise gate before it, Bass, Middle and Treble after it, and an optional cabinet impulse response. Load them in its window.",
        params: vec![
            num("input", "Input", "dB", -24.0, 24.0, 0.0, 0.1),
            toggle("gate", "Gate", true),
            num("gate_db", "Gate Threshold", "dB", -100.0, -20.0, -80.0, 0.5),
            num("bass", "Bass", "dB", -12.0, 12.0, 0.0, 0.1),
            num("middle", "Middle", "dB", -12.0, 12.0, 0.0, 0.1),
            num("treble", "Treble", "dB", -12.0, 12.0, 0.0, 0.1),
            num("output", "Output", "dB", -40.0, 12.0, 0.0, 0.1),
            toggle("level", "Level Match", true),
            toggle("cab", "Cabinet", true),
        ],
    }
}

/// What the slot's `data` holds.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AmpData {
    /// The capture file's JSON, as read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<serde_json::Value>,
    /// The capture's file name, for showing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ir: Option<Ir>,
}

/// A cabinet impulse response, mono.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ir {
    pub name: String,
    pub rate: u32,
    pub samples: Vec<f32>,
}

/// A slot's data, read once and kept while the app runs (models are big).
struct Loaded {
    model: Option<(Model, ModelInfo)>,
    ir: Option<Ir>,
}

fn cache() -> &'static Mutex<HashMap<u64, Arc<Loaded>>> {
    static C: OnceLock<Mutex<HashMap<u64, Arc<Loaded>>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

fn hash(text: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

fn load(data: Option<&str>) -> Arc<Loaded> {
    let Some(text) = data.filter(|t| !t.is_empty()) else {
        return Arc::new(Loaded {
            model: None,
            ir: None,
        });
    };
    let key = hash(text);
    if let Some(l) = cache().lock().ok().and_then(|c| c.get(&key).cloned()) {
        return l;
    }
    let d: AmpData = serde_json::from_str(text).unwrap_or_default();
    let loaded = Arc::new(Loaded {
        model: d.model.as_ref().and_then(|m| nam::parse_value(m).ok()),
        ir: d.ir,
    });
    if let Ok(mut c) = cache().lock() {
        // Keep the cache small: a handful of captures in use at once.
        if c.len() > 16 {
            c.clear();
        }
        c.insert(key, loaded.clone());
    }
    loaded
}

/// Checks a capture file's text and says what it is.
pub fn read_model(text: &str) -> Result<(serde_json::Value, ModelInfo), String> {
    let v: serde_json::Value =
        serde_json::from_str(text).map_err(|_| "This is not a model file.".to_string())?;
    let (_, info) = nam::parse_value(&v)?;
    Ok((v, info))
}

/// A response at another rate, by windowed-sinc interpolation (done once,
/// when the effect is built).
fn resample(x: &[f32], from: u32, to: u32) -> Vec<f32> {
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
/// PART, each multiplied in the frequency domain with the input's recent
/// blocks. Adds PART frames of delay.
struct Convolver {
    fwd: Arc<dyn RealToComplex<f32>>,
    inv: Arc<dyn ComplexToReal<f32>>,
    /// The response's blocks, transformed.
    parts: Vec<Vec<Complex32>>,
    /// The input's recent blocks, transformed, newest at `head`.
    history: Vec<Vec<Complex32>>,
    head: usize,
    /// Last block and this block of input (2 * PART).
    frame: Vec<f32>,
    fill: usize,
    out: Vec<f32>,
    acc: Vec<Complex32>,
    time: Vec<f32>,
    scratch_f: Vec<Complex32>,
    scratch_i: Vec<Complex32>,
}

impl Convolver {
    fn new(ir: &[f32]) -> Convolver {
        let n = 2 * PART;
        let mut planner = RealFftPlanner::<f32>::new();
        let fwd = planner.plan_fft_forward(n);
        let inv = planner.plan_fft_inverse(n);
        let bins = n / 2 + 1;
        let mut scratch_f = fwd.make_scratch_vec();
        let parts: Vec<Vec<Complex32>> = ir
            .chunks(PART)
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
            history: vec![vec![Complex32::new(0.0, 0.0); bins]; count],
            head: 0,
            frame: vec![0.0; n],
            fill: 0,
            out: vec![0.0; PART],
            acc: vec![Complex32::new(0.0, 0.0); bins],
            time: vec![0.0; n],
            scratch_i: inv.make_scratch_vec(),
            scratch_f,
            fwd,
            inv,
            parts,
        }
    }

    fn reset(&mut self) {
        for h in &mut self.history {
            h.fill(Complex32::new(0.0, 0.0));
        }
        self.frame.fill(0.0);
        self.out.fill(0.0);
        self.fill = 0;
    }

    /// One sample in, one out, PART frames late.
    #[inline]
    fn run(&mut self, x: f32) -> f32 {
        let y = self.out[self.fill];
        self.frame[PART + self.fill] = x;
        self.fill += 1;
        if self.fill == PART {
            self.fill = 0;
            self.block();
        }
        y
    }

    fn block(&mut self) {
        let n = 2 * PART;
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
        for (o, t) in self.out.iter_mut().zip(&self.time[PART..]) {
            *o = t * scale;
        }
        self.frame.copy_within(PART.., 0);
    }
}

pub struct AmpModel {
    model: Option<Model>,
    input: f32,
    gate: Option<f32>,
    gate_env: f32,
    gate_gain: f32,
    gate_attack: f32,
    gate_release: f32,
    tone: [Biquad; 3],
    output: f32,
    cab: Option<Convolver>,
    /// The cabinet's delay, kept even when it is turned off, so turning
    /// it on or off never moves the sound in time.
    delay: Option<Vec<f32>>,
    delay_at: usize,
    mono: Vec<f32>,
    rate: u32,
}

fn tone_coeffs(p: &Params, rate: u32) -> [Coeffs; 3] {
    [
        Coeffs::new(Shape::LowShelf, rate, 120.0, 0.7, p.get("bass")),
        Coeffs::new(Shape::Peak, rate, 700.0, 0.8, p.get("middle")),
        Coeffs::new(Shape::HighShelf, rate, 3_200.0, 0.7, p.get("treble")),
    ]
}

impl AmpModel {
    pub fn new(p: &Params, rate: u32, data: Option<&str>) -> AmpModel {
        let loaded = load(data);
        let model = loaded.model.as_ref().map(|(m, _)| {
            let mut m = m.clone();
            m.reset();
            m
        });
        let ir = loaded.ir.as_ref().filter(|i| !i.samples.is_empty());
        let cab = ir.filter(|_| p.get("cab") >= 0.5).map(|i| {
            let max = (IR_MAX_SECONDS * f64::from(i.rate)) as usize;
            let cut = &i.samples[..i.samples.len().min(max)];
            Convolver::new(&resample(cut, i.rate, rate))
        });
        let delay = (ir.is_some() && cab.is_none()).then(|| vec![0.0; PART]);
        let ms = |v: f64| (-1.0 / (v * 0.001 * f64::from(rate))).exp() as f32;
        let mut a = AmpModel {
            model,
            input: 1.0,
            gate: None,
            gate_env: 0.0,
            gate_gain: 1.0,
            gate_attack: ms(1.0),
            gate_release: ms(60.0),
            tone: tone_coeffs(p, rate).map(Biquad::new),
            output: 1.0,
            cab,
            delay,
            delay_at: 0,
            mono: Vec::new(),
            rate,
        };
        let loudness = loaded.model.as_ref().and_then(|(_, i)| i.loudness);
        a.set(p, loudness);
        a
    }

    fn set(&mut self, p: &Params, loudness: Option<f64>) {
        self.input = db_to_gain(p.get("input"));
        self.gate = (p.get("gate") >= 0.5).then(|| db_to_gain(p.get("gate_db")));
        let level = match loudness {
            Some(l) if p.get("level") >= 0.5 && self.model.is_some() => TARGET_DB - l,
            _ => 0.0,
        };
        self.output = db_to_gain(p.get("output") + level);
        for (b, c) in self.tone.iter_mut().zip(tone_coeffs(p, self.rate)) {
            b.set(c);
        }
    }
}

impl Processor for AmpModel {
    fn process(&mut self, buf: &mut [f32]) {
        let frames = buf.len() / 2;
        self.mono.clear();
        self.mono.extend(
            buf.chunks_exact(2)
                .map(|f| (f[0] + f[1]) * 0.5 * self.input),
        );
        // Gate: opens fast above the threshold, closes slowly below it.
        if let Some(th) = self.gate {
            for s in &mut self.mono {
                let a = s.abs();
                let k = if a > self.gate_env {
                    self.gate_attack
                } else {
                    self.gate_release
                };
                self.gate_env = a + k * (self.gate_env - a);
                let want = if self.gate_env > th { 1.0 } else { 0.0 };
                let k = if want > self.gate_gain {
                    self.gate_attack
                } else {
                    self.gate_release
                };
                self.gate_gain = want + k * (self.gate_gain - want);
                *s *= self.gate_gain;
            }
        }
        if let Some(m) = &mut self.model {
            m.process(&mut self.mono);
        }
        for s in &mut self.mono {
            let mut y = *s;
            for b in &mut self.tone {
                y = b.run(y);
            }
            y *= self.output;
            if let Some(c) = &mut self.cab {
                y = c.run(y);
            } else if let Some(d) = &mut self.delay {
                let out = d[self.delay_at];
                d[self.delay_at] = y;
                self.delay_at = (self.delay_at + 1) % d.len();
                y = out;
            }
            *s = y;
        }
        for i in 0..frames {
            buf[i * 2] = self.mono[i];
            buf[i * 2 + 1] = self.mono[i];
        }
    }

    fn latency(&self) -> usize {
        if self.cab.is_some() || self.delay.is_some() {
            PART
        } else {
            0
        }
    }

    fn reset(&mut self) {
        if let Some(m) = &mut self.model {
            m.reset();
        }
        for b in &mut self.tone {
            b.reset();
        }
        if let Some(c) = &mut self.cab {
            c.reset();
        }
        if let Some(d) = &mut self.delay {
            d.fill(0.0);
        }
        self.gate_env = 0.0;
        self.gate_gain = 1.0;
    }

    fn update(&mut self, _p: &Params) -> bool {
        // Level match needs the capture's loudness, which lives with the
        // data; rebuilding (cheap: the capture is cached) keeps it right.
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::effects::info;

    fn params(set: &[(&str, f64)]) -> Params {
        let i = info(AMP).unwrap();
        let saved = set.iter().map(|(k, v)| (k.to_string(), *v)).collect();
        Params::resolve(&i, &saved)
    }

    #[test]
    fn with_no_capture_it_passes_sound_through_its_tone_and_gain() {
        let mut a = AmpModel::new(&params(&[("gate", 0.0), ("output", -6.0206)]), 48_000, None);
        let mut buf = vec![0.5f32; 2_000];
        a.process(&mut buf);
        // Flat tone, mono sum of equal channels, half the level.
        assert!((buf[1_998] - 0.25).abs() < 1e-3, "{}", buf[1_998]);
        assert_eq!(a.latency(), 0);
    }

    #[test]
    fn a_cabinet_convolves_after_its_delay() {
        // A response of a single tap at 3 frames, half height.
        let mut ir = vec![0f32; 300];
        ir[3] = 0.5;
        let data = serde_json::to_string(&AmpData {
            ir: Some(Ir {
                name: "test".into(),
                rate: 48_000,
                samples: ir,
            }),
            ..AmpData::default()
        })
        .unwrap();
        let mut a = AmpModel::new(&params(&[("gate", 0.0)]), 48_000, Some(&data));
        assert_eq!(a.latency(), PART);
        let mut buf = vec![0f32; 1_024];
        buf[0] = 1.0;
        buf[1] = 1.0;
        a.process(&mut buf);
        let at = |f: usize| buf[f * 2];
        assert!((at(PART + 3) - 0.5).abs() < 1e-4, "{}", at(PART + 3));
        assert!(at(PART + 2).abs() < 1e-4);
        // Cabinet off keeps the same delay, without the response.
        let mut b = AmpModel::new(&params(&[("gate", 0.0), ("cab", 0.0)]), 48_000, Some(&data));
        assert_eq!(b.latency(), PART);
        let mut buf = vec![0f32; 1_024];
        buf[0] = 1.0;
        buf[1] = 1.0;
        b.process(&mut buf);
        assert!((buf[PART * 2] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn resampling_keeps_a_smooth_response() {
        let x: Vec<f32> = (0..441).map(|i| (i as f32 * 0.01).sin()).collect();
        let y = resample(&x, 44_100, 48_000);
        assert_eq!(y.len(), 480);
        // The middle follows the same curve, at the new rate.
        let want = (240.0f32 * 44_100.0 / 48_000.0 * 0.01).sin();
        assert!((y[240] - want).abs() < 0.01, "{} {want}", y[240]);
    }
}
