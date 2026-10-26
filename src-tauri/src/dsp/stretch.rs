// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Pitch-preserving time stretch for stereo audio, streamed.
//!
//! The method is waveform-similarity overlap-add: the output is built
//! from short windowed frames of the input placed at an even spacing.
//! To make a clip longer, frames are taken from the input closer
//! together than they are placed; to make it shorter, further apart.
//! Before each frame is taken, a small search finds the position whose
//! shape best continues the audio already written, so the pieces join
//! without clicks or warble. It is well suited to speech within about
//! 25 percent either way. Pitch is unchanged because no frame is
//! resampled.

/// Frame length in seconds. About one speech pitch period at its lowest
/// times a few, short enough to follow syllables.
const FRAME_SECONDS: f64 = 0.03;

pub struct Stretcher {
    /// Output length over input length.
    ratio: f64,
    n: usize,
    hop: usize,
    search: usize,
    win: Vec<f32>,
    /// Input not yet consumed, stereo interleaved, starting at frame `base`.
    input: Vec<f32>,
    base: usize,
    /// Next frame to take, before the search (absolute input frame).
    target: f64,
    /// Where the previous frame's natural continuation starts.
    cont: Option<usize>,
    /// Overlap-add buffer, `n` stereo frames.
    ola: Vec<f32>,
    ended: bool,
    flushed: bool,
}

impl Stretcher {
    /// `ratio` above 1 makes the audio longer (slower), below 1 shorter.
    pub fn new(ratio: f64, rate: u32) -> Self {
        Self::with_frame(ratio, rate, FRAME_SECONDS)
    }

    /// The same with a chosen frame length in seconds: longer suits music
    /// with several notes at once, shorter a single voice.
    pub fn with_frame(ratio: f64, rate: u32, frame_seconds: f64) -> Self {
        let mut n = ((f64::from(rate) * frame_seconds) as usize).max(256);
        n -= n % 4;
        let hop = n / 2;
        // Periodic Hann: two windows half a frame apart add up to exactly 1.
        let win = (0..n)
            .map(|i| (0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / n as f64).cos()) as f32)
            .collect();
        Self {
            // Pitch changes stretch further than the clip itself (see the renderer).
            ratio: ratio.clamp(0.125, 8.0),
            n,
            hop,
            search: n / 4,
            win,
            input: Vec::new(),
            base: 0,
            target: 0.0,
            cont: None,
            ola: vec![0.0; n * 2],
            ended: false,
            flushed: false,
        }
    }

    /// Adds stereo interleaved input.
    pub fn push(&mut self, samples: &[f32]) {
        self.input.extend_from_slice(samples);
    }

    /// No more input will come; `pull` then drains what is left.
    pub fn finish(&mut self) {
        self.ended = true;
    }

    fn end(&self) -> usize {
        self.base + self.input.len() / 2
    }

    fn at(&self, frame: usize, ch: usize) -> f32 {
        if frame < self.base {
            return 0.0;
        }
        self.input
            .get((frame - self.base) * 2 + ch)
            .copied()
            .unwrap_or(0.0)
    }

    fn mono(&self, frame: usize) -> f32 {
        self.at(frame, 0) + self.at(frame, 1)
    }

    /// The start near `target` whose first half best matches the audio
    /// starting at `template`.
    fn best(&self, target: usize, template: usize) -> usize {
        let lo = target.saturating_sub(self.search).max(self.base);
        let hi = target + self.search;
        let len = self.hop;
        let mut best = target.max(lo);
        let mut score = f32::MIN;
        // Every second sample is enough to find the alignment for speech.
        for c in lo..=hi {
            let mut dot = 0.0f32;
            let mut energy = 1e-9f32;
            for i in (0..len).step_by(2) {
                let a = self.mono(template + i);
                let b = self.mono(c + i);
                dot += a * b;
                energy += b * b;
            }
            let s = dot / energy.sqrt();
            if s > score {
                score = s;
                best = c;
            }
        }
        best
    }

    /// Appends all output that the input so far allows (stereo interleaved).
    pub fn pull(&mut self, out: &mut Vec<f32>) {
        loop {
            if self.flushed {
                return;
            }
            let target = self.target.round() as usize;
            let need = (target + self.search).max(self.cont.unwrap_or(0)) + self.n;
            if !self.ended && need > self.end() {
                return;
            }
            if self.ended && target >= self.end() {
                // Last frame written: the second half of the buffer is the tail.
                out.extend_from_slice(&self.ola[..self.hop * 2]);
                self.flushed = true;
                return;
            }
            let first = self.cont.is_none();
            let c = match self.cont {
                None => target,
                Some(t) => self.best(target, t),
            };
            for i in 0..self.n {
                // The very first frame starts at full level so the clip does not fade in.
                let w = if first && i < self.hop {
                    1.0
                } else {
                    self.win[i]
                };
                self.ola[i * 2] += self.at(c + i, 0) * w;
                self.ola[i * 2 + 1] += self.at(c + i, 1) * w;
            }
            out.extend_from_slice(&self.ola[..self.hop * 2]);
            self.ola.copy_within(self.hop * 2.., 0);
            let tail = self.ola.len() - self.hop * 2;
            self.ola[tail..].fill(0.0);
            self.cont = Some(c + self.hop);
            self.target += self.hop as f64 / self.ratio;
            // Drop input nothing will look at again.
            let keep = (self.target.round() as usize)
                .saturating_sub(self.search)
                .min(c + self.hop);
            if keep > self.base {
                let drop = (keep - self.base).min(self.input.len() / 2);
                self.input.drain(..drop * 2);
                self.base += drop;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f32, rate: u32, secs: f32) -> Vec<f32> {
        let n = (rate as f32 * secs) as usize;
        (0..n)
            .flat_map(|i| {
                let v = (i as f32 * freq * std::f32::consts::TAU / rate as f32).sin() * 0.5;
                [v, v]
            })
            .collect()
    }

    fn run(input: &[f32], ratio: f64, rate: u32) -> Vec<f32> {
        let mut s = Stretcher::new(ratio, rate);
        let mut out = Vec::new();
        for block in input.chunks(4_096) {
            s.push(block);
            s.pull(&mut out);
        }
        s.finish();
        s.pull(&mut out);
        out
    }

    /// Rising zero crossings per second in the left channel.
    fn freq(stereo: &[f32], rate: u32) -> f32 {
        let left: Vec<f32> = stereo.iter().step_by(2).copied().collect();
        let skip = left.len() / 10;
        let body = &left[skip..left.len() - skip];
        let ups = body
            .windows(2)
            .filter(|w| w[0] < 0.0 && w[1] >= 0.0)
            .count();
        ups as f32 * rate as f32 / body.len() as f32
    }

    #[test]
    fn longer_and_shorter_keep_the_pitch() {
        let rate = 48_000;
        let input = sine(220.0, rate, 2.0);
        for ratio in [1.25, 0.8, 1.1] {
            let out = run(&input, ratio, rate);
            let want = input.len() as f64 / 2.0 * ratio;
            let got = out.len() as f64 / 2.0;
            assert!(
                (got - want).abs() / want < 0.02,
                "ratio {ratio}: {got} frames, want {want}"
            );
            let f = freq(&out, rate);
            assert!((f - 220.0).abs() < 4.0, "ratio {ratio}: pitch {f} Hz");
        }
    }

    #[test]
    fn steady_tone_stays_steady() {
        let rate = 48_000;
        let out = run(&sine(220.0, rate, 1.0), 1.2, rate);
        // No dropouts or bursts: the level of each 10 ms stays near the input's.
        let left: Vec<f32> = out.iter().step_by(2).copied().collect();
        for chunk in left[4_800..left.len() - 4_800].chunks(480) {
            let rms = (chunk.iter().map(|v| v * v).sum::<f32>() / chunk.len() as f32).sqrt();
            assert!((rms - 0.3536).abs() < 0.06, "rms {rms}");
        }
    }

    #[test]
    fn starts_at_full_level() {
        let out = run(&vec![0.5; 48_000 * 2], 1.2, 48_000);
        assert!((out[0] - 0.5).abs() < 1e-6 && (out[200] - 0.5).abs() < 1e-3);
    }
}
