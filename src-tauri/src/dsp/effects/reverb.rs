// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

//! Convolution Reverb: plays the sound through a recorded space, an
//! impulse response (an audio file of a room's echo, a hall, a plate, a
//! spring). A mono response is used on both channels, a stereo one per
//! channel, and a four-channel one as true stereo (left to left, left to
//! right, right to left, right to right).
//!
//! The slot's `data` names the response file (its path and name), so a
//! session stays small; the file is read once and kept while the app
//! runs. Without a file, or when it cannot be found, only the dry sound
//! plays.
//!
//! Responses are leveled so Wet at 0 dB comes out about as loud as the
//! sound going in. Length shortens the response (with a fade), Pre-delay
//! holds the reverb back, Low cut and High cut shape it, and Width
//! narrows or widens it. The convolution works in blocks, which adds a
//! small fixed delay, reported as latency, with the dry sound held back
//! to match.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use serde::{Deserialize, Serialize};

use super::{num, EffectInfo, Params, Processor};
use crate::codecs::AudioReader;
use crate::dsp::biquad::{Biquad, Coeffs, Shape};
use crate::dsp::convolve::Convolver;

pub const ID: &str = "altaudio.convolution_reverb";
/// Responses are cut to this long, seconds.
pub const MAX_SECONDS: f64 = 12.0;
/// Convolution block: also the delay it adds, in frames.
const PART: usize = 512;
const MAX_PREDELAY_MS: f64 = 250.0;
/// Wet and Dry at their lowest are silent.
pub const LEVEL_FLOOR: f64 = -48.0;
const LOW_CUT_OFF: f64 = 20.0;
const HIGH_CUT_OFF: f64 = 20_000.0;

pub fn reverb_info() -> EffectInfo {
    EffectInfo {
        id: ID,
        name: "Convolution Reverb",
        category: "Delay and Reverb",
        description: "Plays the sound through a recorded space: load an impulse response (any audio file of a room, hall, plate or spring echo) in its window.",
        params: vec![
            num("dry", "Dry", "dB", LEVEL_FLOOR, 6.0, 0.0, 0.1),
            num("wet", "Wet", "dB", LEVEL_FLOOR, 6.0, -10.0, 0.1),
            num("predelay", "Pre-delay", "ms", 0.0, MAX_PREDELAY_MS, 0.0, 1.0),
            num("length", "Length", "%", 5.0, 100.0, 100.0, 1.0),
            num("width", "Width", "%", 0.0, 200.0, 100.0, 1.0),
            num("low_cut", "Low cut", "Hz", LOW_CUT_OFF, 1_000.0, LOW_CUT_OFF, 1.0).log(),
            num("high_cut", "High cut", "Hz", 1_000.0, HIGH_CUT_OFF, HIGH_CUT_OFF, 1.0).log(),
        ],
    }
}

/// What the slot's `data` holds.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReverbData {
    pub path: PathBuf,
    /// The file's name, for showing.
    pub name: String,
}

/// A response read from its file, at the rate it plays at.
#[derive(Debug)]
pub struct Response {
    /// One, two or four channels (see the module notes), leveled.
    pub channels: Vec<Vec<f32>>,
    /// The file's own rate and channel count, for showing.
    pub file_rate: u32,
    pub file_channels: usize,
}

type Cache = Mutex<HashMap<(PathBuf, u32), Arc<Response>>>;

fn cache() -> &'static Cache {
    static C: OnceLock<Cache> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Reads a response file at `rate`: mono, stereo or four channels.
pub fn read(path: &Path, rate: u32) -> Result<Response, String> {
    let probe = AudioReader::open(path, 2, None).map_err(|e| e.to_string())?;
    let file_rate = probe.source_rate();
    let file_channels = probe.source_channels();
    drop(probe);
    let n = match file_channels {
        1 => 1,
        4 => 4,
        _ => 2,
    };
    let mut r = AudioReader::open(path, n, Some(rate)).map_err(|e| e.to_string())?;
    let max = (MAX_SECONDS * f64::from(rate)) as usize * n;
    let mut all = Vec::new();
    let mut buf = Vec::new();
    while all.len() < max {
        buf.clear();
        let more = r.read_into(&mut buf).map_err(|e| e.to_string())?;
        all.extend_from_slice(&buf);
        if !more {
            break;
        }
    }
    all.truncate(max - max % n);
    let frames = all.len() / n;
    if frames == 0 {
        return Err("This file has no sound in it.".into());
    }
    let mut channels: Vec<Vec<f32>> = (0..n)
        .map(|c| (0..frames).map(|i| all[i * n + c]).collect())
        .collect();
    // Trailing silence only costs work.
    let end = channels
        .iter()
        .map(|ch| ch.iter().rposition(|v| v.abs() > 1e-5).map_or(0, |i| i + 1))
        .max()
        .unwrap_or(0)
        .max(1);
    for ch in &mut channels {
        ch.truncate(end);
    }
    level(&mut channels);
    Ok(Response {
        channels,
        file_rate,
        file_channels,
    })
}

/// Scales a response so noise through it comes out at about the level it
/// went in: each output's energy is brought to 1.
fn level(channels: &mut [Vec<f32>]) {
    let energy = |ch: &[f32]| {
        ch.iter()
            .map(|v| f64::from(*v) * f64::from(*v))
            .sum::<f64>()
    };
    let per_out = match channels.len() {
        4 => {
            (energy(&channels[0])
                + energy(&channels[2])
                + energy(&channels[1])
                + energy(&channels[3]))
                / 2.0
        }
        n => channels.iter().map(|c| energy(c)).sum::<f64>() / n as f64,
    };
    if per_out <= 1e-12 {
        return;
    }
    let g = (1.0 / per_out.sqrt()) as f32;
    for ch in channels {
        for v in ch.iter_mut() {
            *v *= g;
        }
    }
}

fn load(data: Option<&str>, rate: u32) -> Option<Arc<Response>> {
    let d: ReverbData = serde_json::from_str(data.filter(|t| !t.is_empty())?).ok()?;
    let key = (d.path.clone(), rate);
    if let Some(r) = cache().lock().ok().and_then(|c| c.get(&key).cloned()) {
        return Some(r);
    }
    let r = Arc::new(read(&d.path, rate).ok()?);
    if let Ok(mut c) = cache().lock() {
        if c.len() > 16 {
            c.clear();
        }
        c.insert(key, r.clone());
    }
    Some(r)
}

/// Forgets a file read earlier, so a changed file is read again.
pub fn forget(path: &Path) {
    if let Ok(mut c) = cache().lock() {
        c.retain(|(p, _), _| p != path);
    }
}

fn gain(db: f64) -> f32 {
    if db <= LEVEL_FLOOR + 1e-9 {
        0.0
    } else {
        super::db_to_gain(db)
    }
}

/// A response shortened to `fraction` of its length, the last tenth faded out.
fn shorten(ch: &[f32], fraction: f64) -> Vec<f32> {
    if fraction >= 0.999 {
        return ch.to_vec();
    }
    let n = ((ch.len() as f64 * fraction).round() as usize).max(1);
    let fade = (n / 10).max(1);
    ch[..n]
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let left = n - i;
            if left < fade {
                v * (0.5 - 0.5 * (std::f32::consts::PI * left as f32 / fade as f32).cos())
            } else {
                *v
            }
        })
        .collect()
}

pub struct ConvolutionReverb {
    /// Mono or stereo: one per channel. True stereo: LL, LR, RL, RR.
    convs: Vec<Convolver>,
    /// Length (%), which the convolvers were built with.
    length: f64,
    rate: u32,
    dry: f32,
    wet: f32,
    width: f32,
    /// The dry sound held back by the convolution's delay.
    dry_line: [Vec<f32>; 2],
    dry_at: usize,
    /// Pre-delay: the sound going into the reverb, held back.
    pre: [Vec<f32>; 2],
    pre_at: usize,
    pre_len: usize,
    low: Option<[Biquad; 2]>,
    high: Option<[Biquad; 2]>,
}

impl ConvolutionReverb {
    pub fn new(p: &Params, rate: u32, data: Option<&str>) -> Self {
        let fraction = p.get("length") / 100.0;
        let convs = match load(data, rate) {
            Some(r) => {
                let parts: Vec<Vec<f32>> =
                    r.channels.iter().map(|c| shorten(c, fraction)).collect();
                match parts.len() {
                    1 => vec![
                        Convolver::new(&parts[0], PART),
                        Convolver::new(&parts[0], PART),
                    ],
                    _ => parts.iter().map(|c| Convolver::new(c, PART)).collect(),
                }
            }
            None => Vec::new(),
        };
        let pre_max = (MAX_PREDELAY_MS / 1000.0 * f64::from(rate)) as usize + 1;
        let mut s = Self {
            convs,
            length: p.get("length"),
            rate,
            dry: 1.0,
            wet: 0.0,
            width: 1.0,
            dry_line: [vec![0.0; PART], vec![0.0; PART]],
            dry_at: 0,
            pre: [vec![0.0; pre_max], vec![0.0; pre_max]],
            pre_at: 0,
            pre_len: 0,
            low: None,
            high: None,
        };
        s.set_params(p);
        s
    }

    fn set_params(&mut self, p: &Params) {
        self.dry = gain(p.get("dry"));
        self.wet = gain(p.get("wet"));
        self.width = (p.get("width") / 100.0) as f32;
        self.pre_len = ((p.get("predelay").clamp(0.0, MAX_PREDELAY_MS) / 1000.0)
            * f64::from(self.rate))
        .round() as usize;
        self.pre_len = self.pre_len.min(self.pre[0].len() - 1);
        let filt = |shape: Shape, f: f64, on: bool, old: &mut Option<[Biquad; 2]>| {
            if !on {
                *old = None;
                return;
            }
            let c = Coeffs::new(shape, self.rate, f, std::f64::consts::FRAC_1_SQRT_2, 0.0);
            match old {
                Some(b) => {
                    b[0].set(c);
                    b[1].set(c);
                }
                None => *old = Some([Biquad::new(c), Biquad::new(c)]),
            }
        };
        let lc = p.get("low_cut");
        let hc = p.get("high_cut").min(f64::from(self.rate) * 0.45);
        filt(Shape::HighPass, lc, lc > LOW_CUT_OFF + 0.5, &mut self.low);
        filt(
            Shape::LowPass,
            hc,
            p.get("high_cut") < HIGH_CUT_OFF - 0.5,
            &mut self.high,
        );
    }
}

impl Processor for ConvolutionReverb {
    fn process(&mut self, buf: &mut [f32]) {
        let pre_n = self.pre[0].len();
        for f in buf.as_chunks_mut::<2>().0 {
            let (l, r) = (f[0], f[1]);
            // The dry sound, held back to line up with the reverb.
            let dl = std::mem::replace(&mut self.dry_line[0][self.dry_at], l);
            let dr = std::mem::replace(&mut self.dry_line[1][self.dry_at], r);
            self.dry_at = (self.dry_at + 1) % PART;
            if self.convs.is_empty() {
                f[0] = dl * self.dry;
                f[1] = dr * self.dry;
                continue;
            }
            // Pre-delay.
            self.pre[0][self.pre_at] = l;
            self.pre[1][self.pre_at] = r;
            let read = (self.pre_at + pre_n - self.pre_len) % pre_n;
            let (il, ir) = (self.pre[0][read], self.pre[1][read]);
            self.pre_at = (self.pre_at + 1) % pre_n;
            let (mut wl, mut wr) = match self.convs.len() {
                4 => {
                    let (ll, lr) = (self.convs[0].run(il), self.convs[1].run(il));
                    let (rl, rr) = (self.convs[2].run(ir), self.convs[3].run(ir));
                    (ll + rl, lr + rr)
                }
                _ => (self.convs[0].run(il), self.convs[1].run(ir)),
            };
            if let Some(b) = &mut self.low {
                wl = b[0].run(wl);
                wr = b[1].run(wr);
            }
            if let Some(b) = &mut self.high {
                wl = b[0].run(wl);
                wr = b[1].run(wr);
            }
            if (self.width - 1.0).abs() > 1e-4 {
                let m = (wl + wr) * 0.5;
                let s = (wl - wr) * 0.5 * self.width;
                wl = m + s;
                wr = m - s;
            }
            f[0] = dl * self.dry + wl * self.wet;
            f[1] = dr * self.dry + wr * self.wet;
        }
    }

    fn latency(&self) -> usize {
        PART
    }

    fn reset(&mut self) {
        for c in &mut self.convs {
            c.reset();
        }
        for c in 0..2 {
            self.dry_line[c].fill(0.0);
            self.pre[c].fill(0.0);
        }
        for b in self.low.iter_mut().chain(self.high.iter_mut()) {
            b[0].reset();
            b[1].reset();
        }
    }

    /// Everything but Length (which reshapes the response) changes while it plays.
    fn update(&mut self, p: &Params) -> bool {
        if (p.get("length") - self.length).abs() > 1e-9 {
            return false;
        }
        self.set_params(p);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::effects::info;

    fn params(set: &[(&str, f64)]) -> Params {
        let map = set.iter().map(|(k, v)| (k.to_string(), *v)).collect();
        Params::resolve(&info(ID).unwrap(), &map)
    }

    fn write_ir(name: &str, channels: u16, frames: &[[f32; 2]]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("audiform-reverb-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        let spec = hound::WavSpec {
            channels,
            sample_rate: 48_000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut w = hound::WavWriter::create(&path, spec).unwrap();
        for f in frames {
            for v in f.iter().take(channels as usize) {
                w.write_sample(*v).unwrap();
            }
        }
        w.finalize().unwrap();
        path
    }

    fn data(path: &Path) -> String {
        serde_json::to_string(&ReverbData {
            path: path.to_path_buf(),
            name: "test".into(),
        })
        .unwrap()
    }

    #[test]
    fn without_a_response_only_the_dry_sound_plays_on_time() {
        let mut fx = ConvolutionReverb::new(&params(&[]), 48_000, None);
        let mut buf = vec![0.0; PART * 4];
        buf[0] = 1.0;
        fx.process(&mut buf);
        assert!((buf[PART * 2] - 1.0).abs() < 1e-6);
        assert_eq!(fx.latency(), PART);
    }

    #[test]
    fn an_echo_in_the_response_comes_out_at_its_time() {
        // A response with one tap 1000 frames in.
        let mut frames = vec![[0.0f32; 2]; 2000];
        frames[1000] = [1.0, 1.0];
        let path = write_ir("tap.wav", 1, &frames);
        let mut fx = ConvolutionReverb::new(
            &params(&[("dry", LEVEL_FLOOR), ("wet", 0.0)]),
            48_000,
            Some(&data(&path)),
        );
        let mut buf = vec![0.0; 4096 * 2];
        buf[0] = 0.5;
        buf[1] = 0.5;
        fx.process(&mut buf);
        let at = PART + 1000;
        assert!((buf[at * 2] - 0.5).abs() < 1e-3, "{}", buf[at * 2]);
        assert!(buf[(at - 1) * 2].abs() < 1e-3);
    }

    #[test]
    fn pre_delay_holds_the_reverb_back() {
        let mut frames = vec![[0.0f32; 2]; 10];
        frames[0] = [1.0, 1.0];
        let path = write_ir("click.wav", 1, &frames);
        let mut fx = ConvolutionReverb::new(
            &params(&[("dry", LEVEL_FLOOR), ("wet", 0.0), ("predelay", 10.0)]),
            48_000,
            Some(&data(&path)),
        );
        let mut buf = vec![0.0; 4096 * 2];
        buf[0] = 1.0;
        fx.process(&mut buf);
        let at = PART + 480;
        assert!((buf[at * 2] - 1.0).abs() < 1e-3, "{}", buf[at * 2]);
    }

    #[test]
    fn responses_are_leveled() {
        let mut ch = vec![vec![0.5f32; 100]];
        level(&mut ch);
        let e: f32 = ch[0].iter().map(|v| v * v).sum();
        assert!((e - 1.0).abs() < 1e-4);
    }
}
