// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

//! Spectral view and spectral edits for the Waveform editor.
//!
//! The view is a short-time Fourier transform drawn on a log frequency
//! axis. An edit scales a time by frequency rectangle: the audio is cut
//! into overlapping windowed frames, the bins inside the rectangle are
//! turned down (with soft edges so nothing clicks), and the frames are
//! added back together. Outside the rectangle the audio comes back
//! unchanged.

use std::sync::Arc;

use realfft::num_complex::Complex32;
use realfft::{RealFftPlanner, RealToComplex};

/// Frame length for the view and for edits.
pub const FFT_SIZE: usize = 2048;
/// The bottom of the frequency axis, in Hz.
pub const MIN_HZ: f32 = 30.0;
/// The view's colors cover this many dB below full scale.
pub const RANGE_DB: f32 = 100.0;

/// Frequency at `frac` up the axis (0 bottom, 1 top), log spaced.
pub fn freq_at(frac: f32, nyquist: f32) -> f32 {
    let top = nyquist.max(MIN_HZ * 2.0);
    MIN_HZ * (top / MIN_HZ).powf(frac.clamp(0.0, 1.0))
}

/// Periodic Hann window.
fn hann(n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / n as f32).cos())
        .collect()
}

struct Fft {
    fwd: Arc<dyn RealToComplex<f32>>,
    inv: Arc<dyn realfft::ComplexToReal<f32>>,
}

impl Fft {
    fn new(n: usize) -> Self {
        let mut p = RealFftPlanner::<f32>::new();
        Self {
            fwd: p.plan_fft_forward(n),
            inv: p.plan_fft_inverse(n),
        }
    }
}

/// Spectrogram of `mono`, where sample 0 is timeline sample `origin`.
/// One column per output pixel between `start` and `end`; `rows` from
/// the top (highest frequency) down. Each cell is 0 (silent) to 255
/// (full scale).
pub fn spectrogram(
    mono: &[f32],
    origin: i64,
    rate: u32,
    start: u64,
    end: u64,
    columns: usize,
    rows: usize,
) -> Vec<u8> {
    let nyquist = rate as f32 / 2.0;
    let hz: Vec<f32> = (0..=rows)
        .map(|r| freq_at(1.0 - r as f32 / rows.max(1) as f32, nyquist))
        .collect();
    spectrogram_rows(mono, origin, rate, start, end, columns, &hz, FFT_SIZE)
}

/// The same with the frequency axis given row by row: `hz` holds each
/// row's edges from the top down (one more than the rows), so the view
/// can show any range on any scale. `n` is the frame length (a power of
/// two): longer frames resolve frequency finer and time coarser.
#[allow(clippy::too_many_arguments)]
pub fn spectrogram_rows(
    mono: &[f32],
    origin: i64,
    rate: u32,
    start: u64,
    end: u64,
    columns: usize,
    hz: &[f32],
    n: usize,
) -> Vec<u8> {
    let rows = hz.len().saturating_sub(1);
    let mut out = vec![0u8; columns * rows];
    if columns == 0 || rows == 0 || end <= start {
        return out;
    }
    let n = n.clamp(256, 32_768).next_power_of_two();
    let fft = Fft::new(n);
    let win = hann(n);
    let mut frame = vec![0f32; n];
    let mut spec = vec![Complex32::new(0.0, 0.0); n / 2 + 1];
    let mut mags = vec![0f32; n / 2 + 1];
    let bin_hz = rate as f32 / n as f32;
    // Bin ranges per row, top row first.
    let edges: Vec<(usize, usize)> = hz
        .windows(2)
        .map(|w| {
            let (hi, lo) = (w[0].max(w[1]), w[0].min(w[1]));
            let b0 = (lo / bin_hz).floor().max(0.0) as usize;
            let b1 = ((hi / bin_hz).ceil() as usize).max(b0 + 1);
            (b0.min(n / 2), b1.min(n / 2 + 1))
        })
        .collect();
    // Hann's coherent gain is 0.5, so a full-scale sine peaks at n / 4.
    let full = n as f32 / 4.0;
    let span = (end - start) as f64;
    for c in 0..columns {
        let center = start as f64 + (c as f64 + 0.5) * span / columns as f64;
        let first = center as i64 - (n / 2) as i64 - origin;
        for (i, (f, w)) in frame.iter_mut().zip(&win).enumerate() {
            let idx = first + i as i64;
            let s = if idx >= 0 {
                mono.get(idx as usize).copied().unwrap_or(0.0)
            } else {
                0.0
            };
            *f = s * w;
        }
        if fft.fwd.process(&mut frame, &mut spec).is_err() {
            continue;
        }
        for (m, s) in mags.iter_mut().zip(&spec) {
            *m = s.norm();
        }
        for (r, &(b0, b1)) in edges.iter().enumerate() {
            let m = mags[b0..b1.max(b0 + 1).min(mags.len())]
                .iter()
                .fold(0f32, |a, &v| a.max(v));
            let db = 20.0 * (m / full).max(1e-9).log10();
            let v = ((db + RANGE_DB) / RANGE_DB).clamp(0.0, 1.0);
            out[r * columns + c] = (v * 255.0).round() as u8;
        }
    }
    out
}

/// Where a spectral edit applies: a grid of weights (0 to 255) over
/// time and the log frequency axis, as painted or drawn in the view.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct Mask {
    /// Timeline sample at the left edge of column 0.
    pub start: f64,
    /// Samples per column.
    pub spc: f64,
    pub cols: usize,
    pub rows: usize,
    /// Axis position (0 bottom, 1 top) of the top edge of row 0.
    pub frac_top: f32,
    /// Axis height of one row.
    pub frac_per_row: f32,
    /// `rows` rows of `cols` weights, top row first.
    pub data: Vec<u8>,
}

impl Mask {
    /// A box from `start..end` and `low_hz..high_hz`, fully selected.
    pub fn rect(start: f64, end: f64, low_hz: f32, high_hz: f32, nyquist: f32) -> Self {
        let top = frac_of(high_hz, nyquist);
        let bottom = frac_of(low_hz, nyquist);
        let rows = 64;
        Self {
            start,
            spc: (end - start).max(1.0),
            cols: 1,
            rows,
            frac_top: top,
            frac_per_row: ((top - bottom) / rows as f32).max(1e-6),
            data: vec![255; rows],
        }
    }

    /// Weight 0 to 1 at timeline sample `t` and axis position `frac`.
    pub fn weight(&self, t: f64, frac: f32) -> f32 {
        if self.cols == 0 || self.rows == 0 || self.data.len() < self.cols * self.rows {
            return 0.0;
        }
        let c = ((t - self.start) / self.spc).floor();
        let r = ((self.frac_top - frac) / self.frac_per_row).floor();
        if c < 0.0 || r < 0.0 || c >= self.cols as f64 || r >= self.rows as f32 {
            return 0.0;
        }
        f32::from(self.data[r as usize * self.cols + c as usize]) / 255.0
    }

    /// First and last timeline sample the mask covers.
    pub fn span(&self) -> (f64, f64) {
        (self.start, self.start + self.spc * self.cols as f64)
    }
}

/// Axis position of `hz`, 0 to 1. The inverse of [`freq_at`].
pub fn frac_of(hz: f32, nyquist: f32) -> f32 {
    let top = nyquist.max(MIN_HZ * 2.0);
    if hz <= MIN_HZ {
        return 0.0;
    }
    ((hz / MIN_HZ).ln() / (top / MIN_HZ).ln()).min(1.0)
}

/// What to do inside the mask.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Action {
    /// Scale by a linear gain: 0 removes, 0.25 is -12 dB.
    Gain(f32),
    /// Fill from the audio just before and after, bin by bin.
    Repair,
}

/// Three-tap smoothing along one axis of a frames x bins grid, so mask
/// edges fade instead of cutting.
fn smooth(w: &mut [Vec<f32>], along_frames: bool) {
    let frames = w.len();
    if frames == 0 {
        return;
    }
    let bins = w[0].len();
    let src: Vec<Vec<f32>> = w.to_vec();
    for k in 0..frames {
        for b in 0..bins {
            let (prev, next) = if along_frames {
                (src[k.saturating_sub(1)][b], src[(k + 1).min(frames - 1)][b])
            } else {
                (src[k][b.saturating_sub(1)], src[k][(b + 1).min(bins - 1)])
            };
            // Keep fully selected cells fully selected; only soften edges outward.
            w[k][b] = src[k][b].max((prev + 2.0 * src[k][b] + next) / 4.0);
        }
    }
}

/// Applies `action` inside `mask` to one channel in place. Sample 0 of
/// `samples` is timeline sample `origin`.
pub fn process_channel(samples: &mut [f32], rate: u32, origin: f64, mask: &Mask, action: Action) {
    let n = FFT_SIZE;
    let hop = n / 4;
    let len = samples.len();
    if len == 0 {
        return;
    }
    let fft = Fft::new(n);
    // Square-root Hann for analysis and synthesis: together a Hann.
    let win: Vec<f32> = hann(n).into_iter().map(f32::sqrt).collect();
    let bins = n / 2 + 1;
    let nyquist = rate as f32 / 2.0;
    let bin_frac: Vec<f32> = (0..bins)
        .map(|b| frac_of(b as f32 * rate as f32 / n as f32, nyquist))
        .collect();

    // Analysis.
    let mut positions = Vec::new();
    let mut specs: Vec<Vec<Complex32>> = Vec::new();
    let mut frame = vec![0f32; n];
    let mut pos: i64 = -(n as i64);
    while pos < len as i64 {
        for (i, (f, w)) in frame.iter_mut().zip(&win).enumerate() {
            let idx = pos + i as i64;
            *f = if idx >= 0 && (idx as usize) < len {
                samples[idx as usize] * w
            } else {
                0.0
            };
        }
        let mut spec = vec![Complex32::new(0.0, 0.0); bins];
        if fft.fwd.process(&mut frame, &mut spec).is_err() {
            spec.fill(Complex32::new(0.0, 0.0));
        }
        positions.push(pos);
        specs.push(spec);
        pos += hop as i64;
    }

    // Weights per frame and bin, softened at the edges.
    let mut w: Vec<Vec<f32>> = positions
        .iter()
        .map(|&p| {
            let t = origin + p as f64 + n as f64 / 2.0;
            bin_frac.iter().map(|&fr| mask.weight(t, fr)).collect()
        })
        .collect();
    smooth(&mut w, true);
    smooth(&mut w, false);
    smooth(&mut w, false);

    match action {
        Action::Gain(g) => {
            for (spec, wk) in specs.iter_mut().zip(&w) {
                for (s, &wb) in spec.iter_mut().zip(wk) {
                    if wb > 0.0 {
                        *s *= 1.0 - wb * (1.0 - g);
                    }
                }
            }
        }
        Action::Repair => repair(&mut specs, &w),
    }

    // Synthesis.
    let mut out = vec![0f32; len];
    let mut norm = vec![0f32; len];
    let mut back = vec![0f32; n];
    for (&p, spec) in positions.iter().zip(specs.iter_mut()) {
        spec[0].im = 0.0;
        let last = spec.len() - 1;
        spec[last].im = 0.0;
        if fft.inv.process(spec, &mut back).is_err() {
            continue;
        }
        for i in 0..n {
            let idx = p + i as i64;
            if idx >= 0 && (idx as usize) < len {
                let wv = win[i];
                out[idx as usize] += back[i] * wv / n as f32;
                norm[idx as usize] += wv * wv;
            }
        }
    }
    for ((s, o), wv) in samples.iter_mut().zip(&out).zip(&norm) {
        if *wv > 1e-6 {
            *s = o / wv;
        }
    }
}

/// Fills masked cells from the frames around them: for each bin, the
/// level runs in a straight line (in dB) from the last clean frame
/// before to the first clean frame after, keeping each cell's phase.
fn repair(specs: &mut [Vec<Complex32>], w: &[Vec<f32>]) {
    let frames = specs.len();
    if frames == 0 {
        return;
    }
    let bins = specs[0].len();
    let level = |s: &Complex32| s.norm().max(1e-9).ln();
    for b in 0..bins {
        let mut k = 0;
        while k < frames {
            if w[k][b] <= 0.01 {
                k += 1;
                continue;
            }
            let k0 = k;
            while k < frames && w[k][b] > 0.01 {
                k += 1;
            }
            let k1 = k; // first clean frame after, or frames
                        // Average a few clean frames each side for a steadier level.
            let before: Vec<f32> = (k0.saturating_sub(3)..k0)
                .map(|j| level(&specs[j][b]))
                .collect();
            let after: Vec<f32> = (k1..(k1 + 3).min(frames))
                .map(|j| level(&specs[j][b]))
                .collect();
            let avg = |v: &[f32]| v.iter().sum::<f32>() / v.len() as f32;
            let (a, z) = match (before.is_empty(), after.is_empty()) {
                (false, false) => (avg(&before), avg(&after)),
                (false, true) => (avg(&before), avg(&before)),
                (true, false) => (avg(&after), avg(&after)),
                (true, true) => continue,
            };
            let span = (k1 - k0 + 1) as f32;
            for (i, j) in (k0..k1).enumerate() {
                let t = (i + 1) as f32 / span;
                let target = (a + (z - a) * t).exp();
                let s = specs[j][b];
                let m = s.norm();
                let unit = if m > 1e-12 {
                    s / m
                } else {
                    Complex32::new(1.0, 0.0)
                };
                let wb = w[j][b];
                specs[j][b] = s * (1.0 - wb) + unit * target * wb;
            }
        }
    }
}

/// Second-order low-pass (Butterworth, bilinear transform).
fn lowpass(x: &[f32], rate: u32, cutoff: f32) -> Vec<f32> {
    let w = (std::f32::consts::PI * cutoff / rate as f32).tan();
    let q = std::f32::consts::FRAC_1_SQRT_2;
    let norm = 1.0 / (1.0 + w / q + w * w);
    let b0 = w * w * norm;
    let b1 = 2.0 * b0;
    let b2 = b0;
    let a1 = 2.0 * (w * w - 1.0) * norm;
    let a2 = (1.0 - w / q + w * w) * norm;
    let (mut x1, mut x2, mut y1, mut y2) = (0f32, 0f32, 0f32, 0f32);
    x.iter()
        .map(|&v| {
            let y = b0 * v + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2;
            x2 = x1;
            x1 = v;
            y2 = y1;
            y1 = y;
            y
        })
        .collect()
}

/// Pitch (fundamental frequency) of `mono` at `columns` points between
/// `start` and `end`, in Hz, or 0 where there is no clear pitch. A
/// difference-function estimate on audio thinned to about 11 kHz,
/// searching 60 Hz to 1 kHz.
pub fn pitch_track(
    mono: &[f32],
    origin: i64,
    rate: u32,
    start: u64,
    end: u64,
    columns: usize,
) -> Vec<f32> {
    let mut out = vec![0f32; columns];
    if columns == 0 || end <= start {
        return out;
    }
    let decim = ((rate as f32 / 11_025.0).floor() as usize).max(1);
    let r = rate as f32 / decim as f32;
    let frame = ((r * 0.046) as usize).max(256); // about 46 ms
    let tau_min = (r / 1000.0).floor() as usize;
    let tau_max = ((r / 60.0).ceil() as usize).min(frame - 1);
    // Low-pass at about 1 kHz before thinning, so high sounds (a whistle,
    // sibilance) cannot fold down and pass for a pitch.
    let filtered = lowpass(&lowpass(mono, rate, 1_100.0), rate, 1_100.0);
    let mut x = vec![0f32; frame + tau_max + 1];
    let mut d = vec![0f32; tau_max + 1];
    let span = (end - start) as f64;
    for (c, o) in out.iter_mut().enumerate() {
        let center = start as f64 + (c as f64 + 0.5) * span / columns as f64;
        let first = center as i64 - ((x.len() * decim) / 2) as i64 - origin;
        let mut energy = 0f32;
        for (i, v) in x.iter_mut().enumerate() {
            let idx = first + (i * decim) as i64;
            *v = if idx >= 0 {
                filtered.get(idx as usize).copied().unwrap_or(0.0)
            } else {
                0.0
            };
            energy += *v * *v;
        }
        if energy / (x.len() as f32) < 1e-6 {
            continue; // too quiet
        }
        // Cumulative mean normalized difference.
        d[0] = 1.0;
        let mut running = 0f32;
        for tau in 1..=tau_max {
            let mut sum = 0f32;
            for i in 0..frame {
                let e = x[i] - x[i + tau];
                sum += e * e;
            }
            running += sum;
            d[tau] = if running > 0.0 {
                sum * tau as f32 / running
            } else {
                1.0
            };
        }
        let mut best = None;
        let mut tau = tau_min.max(2);
        while tau < tau_max {
            if d[tau] < 0.12 {
                while tau + 1 < tau_max && d[tau + 1] < d[tau] {
                    tau += 1;
                }
                best = Some(tau);
                break;
            }
            tau += 1;
        }
        if let Some(t) = best {
            // Parabolic refinement.
            let (a, b, cc) = (d[t - 1], d[t], d[t + 1]);
            let den = a - 2.0 * b + cc;
            let shift = if den.abs() > 1e-9 {
                0.5 * (a - cc) / den
            } else {
                0.0
            };
            *o = r / (t as f32 + shift.clamp(-1.0, 1.0));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f32, rate: u32, len: usize, amp: f32) -> Vec<f32> {
        (0..len)
            .map(|i| amp * (std::f32::consts::TAU * freq * i as f32 / rate as f32).sin())
            .collect()
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len().max(1) as f32).sqrt()
    }

    #[test]
    fn unity_region_leaves_audio_unchanged() {
        let rate = 48_000;
        let orig = sine(440.0, rate, 20_000, 0.5);
        let mut x = orig.clone();
        let m = Mask::rect(5_000.0, 15_000.0, 200.0, 2_000.0, 24_000.0);
        process_channel(&mut x, rate, 0.0, &m, Action::Gain(1.0));
        let err = orig
            .iter()
            .zip(&x)
            .map(|(a, b)| (a - b).abs())
            .fold(0f32, f32::max);
        assert!(err < 1e-3, "max error {err}");
    }

    #[test]
    fn removes_a_tone_inside_the_rectangle_only() {
        let rate = 48_000;
        let len = 48_000;
        let low = sine(300.0, rate, len, 0.3);
        let high = sine(3_000.0, rate, len, 0.3);
        let mut x: Vec<f32> = low.iter().zip(&high).map(|(a, b)| a + b).collect();
        // Remove 2-4 kHz between 0.25 s and 0.75 s.
        let m = Mask::rect(12_000.0, 36_000.0, 2_000.0, 4_000.0, 24_000.0);
        process_channel(&mut x, rate, 0.0, &m, Action::Gain(0.0));
        // In the middle only the 300 Hz tone is left.
        let mid = 20_000..28_000;
        let residual: Vec<f32> = x[mid.clone()]
            .iter()
            .zip(&low[mid.clone()])
            .map(|(a, b)| a - b)
            .collect();
        assert!(rms(&residual) < 0.01, "residual {}", rms(&residual));
        // Well outside the rectangle nothing changed.
        let before: Vec<f32> = x[..6_000]
            .iter()
            .zip(low[..6_000].iter().zip(&high[..6_000]))
            .map(|(a, (l, h))| a - l - h)
            .collect();
        assert!(rms(&before) < 1e-3);
    }

    #[test]
    fn repair_fills_a_click_from_the_audio_around_it() {
        let rate = 48_000;
        let len = 48_000;
        let tone = sine(500.0, rate, len, 0.3);
        let mut x = tone.clone();
        // A loud click in the middle.
        for (i, v) in x[24_000..24_040].iter_mut().enumerate() {
            *v += if i % 2 == 0 { 0.9 } else { -0.9 };
        }
        let m = Mask::rect(23_500.0, 24_540.0, MIN_HZ, 24_000.0, 24_000.0);
        process_channel(&mut x, rate, 0.0, &m, Action::Repair);
        let around = 23_000..25_000;
        let residual: Vec<f32> = x[around.clone()]
            .iter()
            .zip(&tone[around])
            .map(|(a, b)| a - b)
            .collect();
        assert!(rms(&residual) < 0.03, "residual {}", rms(&residual));
    }

    #[test]
    fn pitch_track_finds_a_steady_tone() {
        let rate = 44_100;
        let x = sine(220.0, rate, 44_100, 0.5);
        let p = pitch_track(&x, 0, rate, 5_000, 40_000, 8);
        for hz in &p {
            assert!((hz - 220.0).abs() < 3.0, "pitch {hz}");
        }
        let quiet = pitch_track(&vec![0.0; 44_100], 0, rate, 0, 44_100, 4);
        assert!(quiet.iter().all(|v| *v == 0.0));
        // A high whistle alone has no voice pitch.
        let whistle = sine(6_000.0, rate, 44_100, 0.5);
        let w = pitch_track(&whistle, 0, rate, 5_000, 40_000, 8);
        assert!(w.iter().all(|v| *v == 0.0), "{w:?}");
    }

    #[test]
    fn mask_weights_follow_the_painted_cells() {
        let m = Mask {
            start: 100.0,
            spc: 10.0,
            cols: 2,
            rows: 2,
            frac_top: 0.8,
            frac_per_row: 0.1,
            data: vec![255, 0, 0, 128],
        };
        assert_eq!(m.weight(105.0, 0.75), 1.0);
        assert_eq!(m.weight(115.0, 0.75), 0.0);
        assert!((m.weight(115.0, 0.65) - 0.502).abs() < 0.01);
        assert_eq!(m.weight(95.0, 0.75), 0.0);
        assert_eq!(m.weight(105.0, 0.85), 0.0);
    }

    #[test]
    fn spectrogram_shows_a_tone_in_the_right_row() {
        let rate = 48_000;
        let x = sine(1_000.0, rate, 48_000, 0.9);
        let rows = 100;
        let img = spectrogram(&x, 0, rate, 10_000, 30_000, 10, rows);
        // Row of 1 kHz on the log axis from the top.
        let frac = (1_000f32 / MIN_HZ).ln() / (24_000f32 / MIN_HZ).ln();
        let row = ((1.0 - frac) * rows as f32) as usize;
        let col: Vec<u8> = (0..rows).map(|r| img[r * 10 + 5]).collect();
        let best = col
            .iter()
            .enumerate()
            .max_by_key(|(_, v)| **v)
            .map(|(i, _)| i)
            .unwrap();
        assert!(
            best.abs_diff(row) <= 1,
            "loudest row {best}, expected {row}"
        );
        assert!(col[best] > 240);
        // Silence is dark.
        let quiet = spectrogram(&vec![0.0; 48_000], 0, rate, 0, 48_000, 4, rows);
        assert!(quiet.iter().all(|v| *v == 0));
    }
}
