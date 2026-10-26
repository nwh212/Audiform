// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Music Fit: makes a music bed an exact length by cutting out or
//! repeating whole sections at musical joins, instead of changing its
//! speed. Original code from textbook methods:
//!
//! 1. Analysis: an onset envelope (how much new sound starts in each
//!    short frame), a tempo from its autocorrelation, and beat times from
//!    dynamic programming that keeps beats on onsets at a steady spacing.
//! 2. Each beat gets a fingerprint: its harmony (energy per pitch class)
//!    and tone colour (energy in a few broad bands).
//! 3. A join from beat `i` to beat `j` is good when the beats around `i`
//!    sound like the beats around `j`, so the cut cannot be heard.
//! 4. To shorten, one section in the middle is skipped; to lengthen, one
//!    section is repeated. The intro and the ending are always kept. Any
//!    small difference left over (under half a beat) is taken up by a
//!    tiny time stretch.

use realfft::RealFftPlanner;
use serde::Serialize;

/// Analysis rate. Plenty for rhythm and harmony, and quick.
pub const RATE: u32 = 22_050;
const N: usize = 2048;
const HOP: usize = 512;

pub struct Analysis {
    /// Beat times in seconds from the start of the audio.
    pub beats: Vec<f64>,
    /// One fingerprint per beat interval (between beat k and k + 1).
    feats: Vec<Vec<f32>>,
    pub tempo: f64,
    pub duration: f64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    /// Seconds into the analysed audio.
    pub start: f64,
    pub end: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FitPlan {
    pub segments: Vec<Segment>,
    /// How long the joined segments play, crossfades counted, before the stretch.
    pub length: f64,
    /// Timeline length over natural length that lands it exactly; 1 is none.
    pub stretch: f64,
    pub joins: usize,
    /// How alike the two sides of the worst join are, 0 to 1.
    pub quality: f64,
    pub tempo: f64,
}

fn frames_of(mono: &[f32]) -> (Vec<f32>, Vec<Vec<f32>>) {
    let bins = N / 2 + 1;
    let mut planner = RealFftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(N);
    let mut spec = fft.make_output_vec();
    let mut buf = vec![0f32; N];
    let win: Vec<f32> = (0..N)
        .map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / N as f32).cos())
        .collect();
    // Which pitch class and which broad band each bin falls in.
    let hz = |k: usize| k as f32 * RATE as f32 / N as f32;
    let class: Vec<Option<usize>> = (0..bins)
        .map(|k| {
            let f = hz(k);
            (55.0..5000.0).contains(&f).then(|| {
                let midi = 69.0 + 12.0 * (f / 440.0).log2();
                (midi.round() as i64).rem_euclid(12) as usize
            })
        })
        .collect();
    const BANDS: [f32; 7] = [0.0, 150.0, 400.0, 1000.0, 2500.0, 6000.0, 11_025.0];
    let band: Vec<usize> = (0..bins)
        .map(|k| {
            BANDS
                .windows(2)
                .position(|w| hz(k) >= w[0] && hz(k) < w[1])
                .unwrap_or(5)
        })
        .collect();
    let mut onset = Vec::new();
    let mut feats = Vec::new();
    let mut prev = vec![0f32; bins];
    let mut at = 0;
    while at + N <= mono.len() {
        for (b, (x, w)) in buf.iter_mut().zip(mono[at..at + N].iter().zip(&win)) {
            *b = x * w;
        }
        let _ = fft.process(&mut buf, &mut spec);
        let mut chroma = [0f32; 12];
        let mut tone = [0f32; 6];
        let mut flux = 0f32;
        for k in 1..bins {
            let mag = spec[k].norm();
            let lm = (1.0 + 100.0 * mag).ln();
            flux += (lm - prev[k]).max(0.0);
            prev[k] = lm;
            if let Some(c) = class[k] {
                chroma[c] += mag * mag;
            }
            tone[band[k]] += mag * mag;
        }
        onset.push(flux);
        let mut f: Vec<f32> = Vec::with_capacity(18);
        let cn = chroma.iter().map(|v| v * v).sum::<f32>().sqrt().max(1e-9);
        f.extend(chroma.iter().map(|v| v / cn));
        let tl: Vec<f32> = tone.iter().map(|v| (1e-6 + v).ln()).collect();
        let tm = tl.iter().sum::<f32>() / 6.0;
        f.extend(tl.iter().map(|v| (v - tm) * 0.15));
        feats.push(f);
        at += HOP;
    }
    (onset, feats)
}

/// Frames between beats, from the onset envelope's autocorrelation,
/// preferring tempos near 120 beats per minute.
fn beat_period(onset: &[f32]) -> Option<usize> {
    let fps = RATE as f64 / HOP as f64;
    let mean = onset.iter().sum::<f32>() / onset.len().max(1) as f32;
    let x: Vec<f32> = onset.iter().map(|v| v - mean).collect();
    let lo = (fps * 60.0 / 180.0).floor() as usize;
    let hi = (fps * 60.0 / 70.0).ceil() as usize;
    if x.len() < hi * 4 {
        return None;
    }
    let mut best = None;
    let mut best_score = f64::MIN;
    for lag in lo..=hi {
        let ac: f64 = x.iter().zip(&x[lag..]).map(|(a, b)| f64::from(a * b)).sum();
        let bpm = 60.0 * fps / lag as f64;
        let weight = (-0.5 * ((bpm / 120.0).log2() / 0.9).powi(2)).exp();
        let score = ac * weight;
        if score > best_score {
            best_score = score;
            best = Some(lag);
        }
    }
    best.filter(|_| best_score > 0.0)
}

/// Beat frames: each beat on a strong onset, spaced about `period`.
fn track_beats(onset: &[f32], period: usize) -> Vec<usize> {
    let n = onset.len();
    let norm = onset.iter().cloned().fold(0f32, f32::max).max(1e-9);
    let o: Vec<f64> = onset.iter().map(|v| f64::from(v / norm)).collect();
    let mut score = o.clone();
    let mut from = vec![usize::MAX; n];
    let p = period as f64;
    for t in 0..n {
        let lo = t.saturating_sub(2 * period);
        let hi = t.saturating_sub(period / 2);
        let mut best = f64::MIN;
        for (q, sq) in score.iter().enumerate().take(hi).skip(lo) {
            let gap = (t - q) as f64;
            let s = sq - 4.0 * (gap / p).ln().powi(2);
            if s > best {
                best = s;
                from[t] = q;
            }
        }
        if best > f64::MIN {
            score[t] = o[t] + best;
        }
    }
    // Finish on the best-scoring beat in the last period.
    let mut t = (n.saturating_sub(period)..n)
        .max_by(|a, b| score[*a].total_cmp(&score[*b]))
        .unwrap_or(0);
    let mut beats = vec![t];
    while from[t] != usize::MAX {
        t = from[t];
        beats.push(t);
    }
    beats.reverse();
    beats
}

/// Analyses mono audio at [`RATE`]. None when there is no steady beat
/// to join on (speech, ambience, very short music).
pub fn analyze(mono: &[f32]) -> Option<Analysis> {
    let duration = mono.len() as f64 / f64::from(RATE);
    let (onset, frame_feats) = frames_of(mono);
    let period = beat_period(&onset)?;
    let beat_frames = track_beats(&onset, period);
    if beat_frames.len() < 16 {
        return None;
    }
    let to_sec = |f: usize| (f * HOP + N / 2) as f64 / f64::from(RATE);
    let beats: Vec<f64> = beat_frames.iter().map(|f| to_sec(*f)).collect();
    let feats = beat_frames
        .windows(2)
        .map(|w| {
            let span = &frame_feats[w[0]..w[1].max(w[0] + 1).min(frame_feats.len())];
            let mut m = vec![0f32; frame_feats[0].len()];
            for f in span {
                for (a, b) in m.iter_mut().zip(f) {
                    *a += b;
                }
            }
            let norm = m.iter().map(|v| v * v).sum::<f32>().sqrt().max(1e-9);
            m.iter().map(|v| v / norm).collect()
        })
        .collect();
    let tempo = 60.0 * f64::from(RATE) / HOP as f64 / period as f64;
    Some(Analysis {
        beats,
        feats,
        tempo,
        duration,
    })
}

impl Analysis {
    /// How alike the music around beat `i` is to the music around beat `j`.
    fn similarity(&self, i: usize, j: usize) -> f64 {
        let mut sum = 0.0;
        let mut n = 0.0;
        for k in -2i64..=1 {
            let (a, b) = (i as i64 + k, j as i64 + k);
            if a < 0 || b < 0 || a as usize >= self.feats.len() || b as usize >= self.feats.len() {
                continue;
            }
            let fa = &self.feats[a as usize];
            let fb = &self.feats[b as usize];
            sum += f64::from(fa.iter().zip(fb).map(|(x, y)| x * y).sum::<f32>());
            n += 1.0;
        }
        if n == 0.0 {
            0.0
        } else {
            (sum / n).clamp(0.0, 1.0)
        }
    }

    /// The best plan to play `target` seconds, with `xfade` seconds of
    /// overlap at each join.
    pub fn plan(&self, target: f64, xfade: f64) -> Option<FitPlan> {
        self.plans(target, xfade, 1).into_iter().next()
    }

    /// Up to `n` plans, best first, each joining at different places
    /// (at least two beats from every better plan's joins), so the
    /// person can listen and pick.
    pub fn plans(&self, target: f64, xfade: f64, n: usize) -> Vec<FitPlan> {
        let d = self.duration;
        if target <= 0.0 || n == 0 {
            return Vec::new();
        }
        let b = &self.beats;
        let m = b.len();
        let beat = 60.0 / self.tempo;
        // Keep the first and last few beats as they are.
        let keep = 4.min(m / 4);
        if (target - d).abs() < beat * 0.5 {
            return vec![FitPlan {
                segments: vec![Segment { start: 0.0, end: d }],
                length: d,
                stretch: target / d,
                joins: 0,
                quality: 1.0,
                tempo: self.tempo,
            }];
        }
        let mut found: Vec<(f64, FitPlan)> = Vec::new();
        let mut consider = |score: f64, plan: FitPlan| found.push((score, plan));
        if target < d {
            // Skip from beat i ahead to beat j.
            for i in keep..m - keep {
                for j in i + 1..m - keep {
                    // Each side runs half a crossfade past the beat, so the overlap cancels out.
                    let length = d - (b[j] - b[i]);
                    let err = (length - target).abs() / beat;
                    if err > 2.0 {
                        continue;
                    }
                    let sim = self.similarity(i, j);
                    let bar = if (j - i) % 4 == 0 { 0.08 } else { 0.0 };
                    consider(
                        sim + bar - 0.15 * err,
                        FitPlan {
                            segments: vec![
                                Segment {
                                    start: 0.0,
                                    end: b[i] + xfade / 2.0,
                                },
                                Segment {
                                    start: b[j] - xfade / 2.0,
                                    end: d,
                                },
                            ],
                            length,
                            stretch: target / length,
                            joins: 1,
                            quality: sim,
                            tempo: self.tempo,
                        },
                    );
                }
            }
        } else {
            // Repeat the section from beat j back to beat i, r times.
            let add = target - d;
            for i in keep + 1..m - keep {
                for j in keep..i {
                    let span = b[i] - b[j];
                    if span < beat * 3.5 {
                        continue;
                    }
                    let r = (add / span).round().max(1.0);
                    if r > 50.0 {
                        continue;
                    }
                    let length = d + r * span;
                    let err = (length - target).abs() / beat;
                    if err > 2.0 {
                        continue;
                    }
                    let sim = self.similarity(i, j);
                    let bar = if (i - j) % 4 == 0 { 0.08 } else { 0.0 };
                    let mut segments = vec![Segment {
                        start: 0.0,
                        end: b[i] + xfade / 2.0,
                    }];
                    for _ in 1..r as usize {
                        segments.push(Segment {
                            start: b[j] - xfade / 2.0,
                            end: b[i] + xfade / 2.0,
                        });
                    }
                    segments.push(Segment {
                        start: b[j] - xfade / 2.0,
                        end: d,
                    });
                    consider(
                        sim + bar - 0.15 * err - 0.02 * r,
                        FitPlan {
                            joins: segments.len() - 1,
                            segments,
                            length,
                            stretch: target / length,
                            quality: sim,
                            tempo: self.tempo,
                        },
                    );
                }
            }
        }
        found.sort_by(|a, b| b.0.total_cmp(&a.0));
        let mut out: Vec<FitPlan> = Vec::new();
        for (_, p) in found {
            let cuts = cut_points(&p);
            let near = out.iter().any(|q| {
                let other = cut_points(q);
                cuts.len() == other.len()
                    && cuts.iter().zip(&other).all(|(a, b)| {
                        (a.0 - b.0).abs() < 2.0 * beat && (a.1 - b.1).abs() < 2.0 * beat
                    })
            });
            if !near {
                out.push(p);
                if out.len() == n {
                    break;
                }
            }
        }
        out
    }
}

/// Where the first join leaves the music and where it comes back in,
/// in seconds into the clip. Later joins in a repeat plan are the same.
fn cut_points(p: &FitPlan) -> Vec<(f64, f64)> {
    p.segments
        .windows(2)
        .map(|w| (w[0].end, w[1].start))
        .take(1)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    /// 120 BPM, a four-bar chord loop with a kick on every beat.
    fn song(bars: usize) -> Vec<f32> {
        let beat = RATE as usize / 2;
        let chords: [[f32; 3]; 4] = [
            [261.6, 329.6, 392.0],
            [349.2, 440.0, 523.3],
            [392.0, 493.9, 587.3],
            [220.0, 261.6, 329.6],
        ];
        let mut out = vec![0f32; bars * 4 * beat];
        for (i, v) in out.iter_mut().enumerate() {
            let bar = i / (4 * beat);
            let in_beat = i % beat;
            let t = i as f32 / RATE as f32;
            let chord = chords[bar % 4];
            let tone: f32 = chord.iter().map(|f| (TAU * f * t).sin()).sum::<f32>() * 0.08;
            let decay = (-(in_beat as f32) / (RATE as f32 * 0.04)).exp();
            let kick = (TAU * 60.0 * in_beat as f32 / RATE as f32).sin() * decay * 0.6;
            *v = tone + kick;
        }
        out
    }

    #[test]
    fn finds_the_tempo_and_beats() {
        let a = analyze(&song(16)).expect("a steady beat");
        assert!((a.tempo - 120.0).abs() < 4.0, "tempo {}", a.tempo);
        let gaps: Vec<f64> = a.beats.windows(2).map(|w| w[1] - w[0]).collect();
        let mean = gaps.iter().sum::<f64>() / gaps.len() as f64;
        assert!((mean - 0.5).abs() < 0.02, "beat spacing {mean}");
    }

    #[test]
    fn shortens_and_lengthens_at_musical_joins() {
        let a = analyze(&song(16)).unwrap(); // 32 s
        let xf = 0.04;
        let short = a.plan(20.0, xf).unwrap();
        assert_eq!(short.joins, 1);
        assert!(
            (short.stretch - 1.0).abs() < 0.03,
            "stretch {}",
            short.stretch
        );
        // The skipped stretch is a whole number of bars, so the chords line up.
        let skipped = short.segments[1].start - short.segments[0].end + xf;
        let bars = skipped / 2.0;
        assert!((bars - bars.round()).abs() < 0.05, "skipped {skipped} s");
        // Where a whole four-bar phrase can go, the join is seamless.
        let phrase = a.plan(24.0, xf).unwrap();
        assert!(phrase.quality > 0.95, "quality {}", phrase.quality);
        let long = a.plan(50.0, xf).unwrap();
        assert!(long.joins >= 1);
        assert!(
            (long.stretch - 1.0).abs() < 0.03,
            "stretch {}",
            long.stretch
        );
        let looped = a.plan(48.0, xf).unwrap();
        assert!(looped.quality > 0.95, "quality {}", looped.quality);
        // The intro and the ending are kept.
        assert_eq!(long.segments[0].start, 0.0);
        assert!((long.segments.last().unwrap().end - a.duration).abs() < 1e-9);
    }

    #[test]
    fn offers_several_different_plans() {
        let a = analyze(&song(16)).unwrap();
        let all = a.plans(20.0, 0.04, 3);
        assert!(all.len() >= 2, "{} plans", all.len());
        assert_eq!(
            all[0].segments[0].end,
            a.plan(20.0, 0.04).unwrap().segments[0].end
        );
        let beat = 60.0 / a.tempo;
        for (k, p) in all.iter().enumerate() {
            assert!((p.stretch - 1.0).abs() < 0.03);
            for q in &all[..k] {
                let moved = (p.segments[0].end - q.segments[0].end)
                    .abs()
                    .max((p.segments[1].start - q.segments[1].start).abs());
                assert!(moved >= 2.0 * beat - 1e-9, "plans {k} too close");
            }
        }
    }

    #[test]
    fn too_little_music_is_refused() {
        assert!(analyze(&song(1)).is_none());
        assert!(analyze(&[]).is_none());
    }
}
