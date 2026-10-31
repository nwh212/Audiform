// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

//! Built-in effects on more than two channels: a 3D Space master (its
//! speaker bed) or a surround file in the Waveform editor and Batch
//! Convert, from 3.0 up to 7.1.4.
//!
//! - The Reverb runs natively on the bed: one room, heard differently in
//!   every speaker (see `verb.rs`).
//! - Gain works on every channel as it is.
//! - Every other effect runs as a set of stereo copies with the same
//!   settings, one per speaker pair (L and R, Ls and Rs, the height
//!   pairs...), and a copy per lone speaker (C) fed the same sound on
//!   both sides.
//! - Dynamics, Limiter and Breath Gate listen to the loudest channel of
//!   the whole bed (or to their sidechain), so every copy moves its gain
//!   together and the image holds still.
//! - Delays, pitch and tone shapers leave the low-frequency channel alone
//!   (it only carries the deep end); filters and repair work on it too.
//! - Effects that only make sense on a stereo pair (width, center
//!   extraction, the channel mixer) keep working on L and R.

use super::{build, routing, verb, EffectInfo, Params, Processor};
use crate::session::model::EffectSlot;

/// Effects that only work on a left and right pair.
const STEREO_ONLY: [&str; 5] = [
    super::center::ID,
    super::width::WIDTH,
    super::mixer::ID,
    "altaudio.phase_rotate",
    "altaudio.center_remover",
];

/// Effects whose copies move their gain together, from the loudest channel.
const LINKED: [&str; 3] = [
    "altaudio.dynamics",
    "altaudio.limiter",
    "altaudio.breath_gate",
];

/// Effects that leave the low-frequency channel alone.
const SKIP_LFE: [&str; 9] = [
    "altaudio.echo",
    "altaudio.pitch_shift",
    "altaudio.phase_curve",
    "altaudio.convolution_reverb",
    super::amp::AMP,
    super::tube::PREAMP,
    super::tube::OPTO,
    super::vocal::VOCAL,
    "altaudio.speech_leveler",
];

/// The processor for a built-in effect over channels named `names`
/// (more than two), or None when the effect runs on L and R only (or is
/// a plug-in, which has its own layout).
pub fn build_wide(slot: &EffectSlot, rate: u32, names: &[String]) -> Option<Box<dyn Processor>> {
    if names.len() <= 2
        || crate::plugins::is_plugin(&slot.effect)
        || STEREO_ONLY.contains(&slot.effect.as_str())
    {
        return None;
    }
    let info: EffectInfo = super::info(&slot.effect)?;
    if info.id == verb::ID {
        let p = Params::resolve(&info, &slot.params);
        return Some(Box::new(verb::Reverb::wide(&p, rate, names)));
    }
    if info.id == "altaudio.gain" {
        let p = Params::resolve(&info, &slot.params);
        return Some(Box::new(AllChannels {
            gain: super::db_to_gain(p.get("gain")),
            names: names.to_vec(),
        }));
    }
    let skip_lfe = SKIP_LFE.contains(&info.id);
    let mut units = Vec::new();
    for group in groups(names, skip_lfe) {
        units.push(Unit {
            chans: group,
            proc: build(slot, rate)?,
            buf: Vec::new(),
        });
    }
    let latency = units.first().map_or(0, |u| u.proc.latency());
    let passed: Vec<usize> = (0..names.len())
        .filter(|c| !units.iter().any(|u| u.chans.contains(c)))
        .collect();
    Some(Box::new(Pairs {
        names: names.to_vec(),
        linked: LINKED.contains(&info.id),
        units,
        pass: super::Delay::with_channels(latency, passed.len()),
        passed,
        pass_buf: Vec::new(),
        key: Vec::new(),
    }))
}

/// The speaker groups an effect runs on: pairs by name (L with R, Lss
/// with Rss, Ltf with Rtf...), then the rest in pairs by order (files
/// with unnamed channels) or alone (C). The low-frequency channel is
/// left out when `skip_lfe`.
pub fn groups(names: &[String], skip_lfe: bool) -> Vec<Vec<usize>> {
    let mut used = vec![false; names.len()];
    let mut out = Vec::new();
    for (i, n) in names.iter().enumerate() {
        if used[i] || n == "LFE" {
            continue;
        }
        let Some(rest) = n.strip_prefix('L') else {
            continue;
        };
        let twin = format!("R{rest}");
        if let Some(j) = names.iter().position(|m| *m == twin).filter(|j| !used[*j]) {
            used[i] = true;
            used[j] = true;
            out.push(vec![i, j]);
        }
    }
    let mut numbered: Vec<usize> = Vec::new();
    for (i, n) in names.iter().enumerate() {
        if used[i] || (skip_lfe && n == "LFE") {
            continue;
        }
        if n.parse::<u32>().is_ok() {
            numbered.push(i);
        } else {
            out.push(vec![i]);
        }
    }
    out.extend(numbered.chunks(2).map(<[usize]>::to_vec));
    out.sort_by_key(|g| g[0]);
    out
}

/// A per-sample gain, the same on every channel.
struct AllChannels {
    gain: f32,
    names: Vec<String>,
}

impl Processor for AllChannels {
    fn process(&mut self, buf: &mut [f32]) {
        for s in buf {
            *s *= self.gain;
        }
    }
    fn reset(&mut self) {}
    fn update(&mut self, p: &Params) -> bool {
        self.gain = super::db_to_gain(p.get("gain"));
        true
    }
    fn channels(&self) -> (Vec<String>, Vec<String>) {
        (self.names.clone(), self.names.clone())
    }
}

/// One stereo copy of the effect and the speakers it works on (two, or
/// one fed to both sides).
struct Unit {
    chans: Vec<usize>,
    proc: Box<dyn Processor>,
    buf: Vec<f32>,
}

struct Pairs {
    names: Vec<String>,
    linked: bool,
    units: Vec<Unit>,
    /// Channels no copy works on, held back by the effect's look-ahead so
    /// they stay in time.
    passed: Vec<usize>,
    pass: super::Delay,
    pass_buf: Vec<f32>,
    /// The loudest channel of each frame, as a stereo key.
    key: Vec<f32>,
}

impl Processor for Pairs {
    fn process(&mut self, buf: &mut [f32]) {
        self.process_keyed(buf, &[]);
    }

    fn process_keyed(&mut self, buf: &mut [f32], key: &[f32]) {
        let ch = self.names.len();
        let frames = buf.len() / ch;
        let key: Option<&[f32]> = if key.len() == frames * 2 {
            Some(key)
        } else if self.linked {
            self.key.clear();
            for f in buf.chunks_exact(ch) {
                let loud = f
                    .iter()
                    .copied()
                    .fold(0.0f32, |m, v| if v.abs() > m.abs() { v } else { m });
                self.key.extend([loud, loud]);
            }
            Some(&self.key)
        } else {
            None
        };
        for u in &mut self.units {
            u.buf.clear();
            match u.chans[..] {
                [a, b] => u
                    .buf
                    .extend(buf.chunks_exact(ch).flat_map(|f| [f[a], f[b]])),
                [a] => u
                    .buf
                    .extend(buf.chunks_exact(ch).flat_map(|f| [f[a], f[a]])),
                _ => continue,
            }
            match key {
                Some(k) => u.proc.process_keyed(&mut u.buf, k),
                None => u.proc.process(&mut u.buf),
            }
            for (f, o) in buf.chunks_exact_mut(ch).zip(u.buf.as_chunks::<2>().0) {
                match u.chans[..] {
                    [a, b] => {
                        f[a] = o[0];
                        f[b] = o[1];
                    }
                    [a] => f[a] = (o[0] + o[1]) * 0.5,
                    _ => {}
                }
            }
        }
        if !self.passed.is_empty() && self.latency() > 0 {
            let n = self.passed.len();
            self.pass_buf.clear();
            for f in buf.chunks_exact(ch) {
                self.pass_buf.extend(self.passed.iter().map(|c| f[*c]));
            }
            self.pass.run(&mut self.pass_buf);
            for (f, p) in buf.chunks_exact_mut(ch).zip(self.pass_buf.chunks_exact(n)) {
                for (c, v) in self.passed.iter().zip(p) {
                    f[*c] = *v;
                }
            }
        }
    }

    fn latency(&self) -> usize {
        self.units.first().map_or(0, |u| u.proc.latency())
    }

    fn reset(&mut self) {
        for u in &mut self.units {
            u.proc.reset();
        }
        self.pass.clear();
    }

    fn update(&mut self, p: &Params) -> bool {
        let mut all = true;
        for u in &mut self.units {
            all &= u.proc.update(p);
        }
        all
    }

    fn channels(&self) -> (Vec<String>, Vec<String>) {
        (self.names.clone(), self.names.clone())
    }
}

/// The routing names for a chain of `n` channels: L and R for stereo,
/// a file's speakers by its channel count otherwise.
pub fn names_for(n: usize) -> Vec<String> {
    if n == 2 {
        routing::stereo()
    } else {
        crate::dsp::channels::labels(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::effects::tests::slot;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| (*x).to_string()).collect()
    }

    #[test]
    fn speakers_pair_by_name() {
        let bed = s(&[
            "L", "R", "C", "LFE", "Lrs", "Rrs", "Lss", "Rss", "Ltf", "Rtf", "Ltr", "Rtr",
        ]);
        assert_eq!(
            groups(&bed, true),
            vec![
                vec![0, 1],
                vec![2],
                vec![4, 5],
                vec![6, 7],
                vec![8, 9],
                vec![10, 11]
            ]
        );
        assert!(groups(&bed, false).contains(&vec![3]));
        assert_eq!(
            groups(&s(&["1", "2", "3"]), true),
            vec![vec![0, 1], vec![2]]
        );
    }

    fn bed51() -> Vec<String> {
        s(&["L", "R", "C", "LFE", "Ls", "Rs"])
    }

    fn run(p: &mut dyn Processor, frames: &[[f32; 6]]) -> Vec<[f32; 6]> {
        let mut b: Vec<f32> = frames.iter().flatten().copied().collect();
        p.process(&mut b);
        b.as_chunks::<6>().0.to_vec()
    }

    #[test]
    fn gain_reaches_every_speaker() {
        let mut g = build_wide(
            &slot("altaudio.gain", &[("gain", -6.0206)]),
            48_000,
            &bed51(),
        )
        .unwrap();
        let out = run(g.as_mut(), &[[1.0; 6]]);
        assert!(
            out[0].iter().all(|v| (v - 0.5).abs() < 1e-3),
            "{:?}",
            out[0]
        );
    }

    #[test]
    fn a_filter_works_on_every_channel() {
        let hp = slot(
            "altaudio.parametric_eq",
            &[("hp_on", 1.0), ("hp_freq", 1000.0)],
        );
        let Some(mut eq) = build_wide(&hp, 48_000, &bed51()) else {
            panic!("the EQ runs on a bed")
        };
        // A steady level (DC) is cut on every channel the filter reaches.
        let out = run(eq.as_mut(), &vec![[0.5; 6]; 48_000]);
        let last = out.last().unwrap();
        assert!(last.iter().all(|v| v.abs() < 0.05), "{last:?}");
    }

    #[test]
    fn the_limiter_turns_every_speaker_down_together() {
        let lim = slot(
            "altaudio.limiter",
            &[("ceiling", -6.0), ("input_boost", 0.0), ("lookahead", 0.0)],
        );
        let mut l = build_wide(&lim, 48_000, &bed51()).unwrap();
        // Only the left surround is loud; the fronts are quiet.
        let frame = [0.1, 0.1, 0.1, 0.1, 1.0, 0.1];
        let out = run(l.as_mut(), &vec![frame; 4800]);
        let f = out.last().unwrap();
        assert!(f[4] <= 0.502, "the ceiling holds: {}", f[4]);
        let ratio = f[0] / f[4];
        assert!(
            (ratio - 0.1).abs() < 0.01,
            "the fronts come down too: {f:?}"
        );
    }

    #[test]
    fn echo_leaves_the_low_channel_alone_and_in_time() {
        let mut e = build_wide(&slot("altaudio.echo", &[]), 48_000, &bed51()).unwrap();
        assert_eq!(e.latency(), 0);
        let mut frames = vec![[0.0f32; 6]; 4800];
        frames[0][3] = 1.0;
        let out = run(e.as_mut(), &frames);
        assert_eq!(out[0][3], 1.0);
        assert!(out[1..].iter().all(|f| f[3] == 0.0));
    }

    #[test]
    fn width_stays_on_the_front_pair() {
        assert!(build_wide(&slot(super::super::width::WIDTH, &[]), 48_000, &bed51()).is_none());
    }

    #[test]
    fn the_reverb_fills_the_room_but_not_the_low_channel() {
        let mut v = build_wide(
            &slot(verb::ID, &[("dry", 0.0), ("wet", 100.0)]),
            48_000,
            &bed51(),
        )
        .unwrap();
        let mut frames = vec![[0.0f32; 6]; 48_000];
        frames[0][0] = 1.0;
        let out = run(v.as_mut(), &frames);
        let energy = |c: usize| out.iter().map(|f| f[c] * f[c]).sum::<f32>();
        for c in [0, 1, 2, 4, 5] {
            assert!(energy(c) > 1e-4, "speaker {c} gets the room");
        }
        assert_eq!(energy(3), 0.0);
        // Each speaker hears its own room, not a copy of another's.
        let corr = |a: usize, b: usize| {
            let ab: f32 = out.iter().map(|f| f[a] * f[b]).sum();
            ab / (energy(a) * energy(b)).sqrt()
        };
        assert!(corr(4, 5).abs() < 0.5, "{}", corr(4, 5));
        assert!(out.iter().flatten().all(|v| v.is_finite() && v.abs() < 4.0));
    }
}
