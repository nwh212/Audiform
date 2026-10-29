// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

//! Channel Mixer: builds each output side from any amount of each input
//! side, with the polarity of each part flipped if asked. Swaps sides,
//! folds to mono, takes one side to both, or fixes a cable wired in
//! reverse polarity.

use super::{num, toggle, EffectInfo, Params, Processor};

pub const ID: &str = "altaudio.channel_mixer";

pub fn mixer_info() -> EffectInfo {
    EffectInfo {
        id: ID,
        name: "Channel Mixer",
        category: "Stereo",
        description: "Makes each output side from a share of the left and right inputs, each with its own polarity switch: swap sides, fold to mono, copy one side to both.",
        params: vec![
            num("ll", "Left from left", "%", 0.0, 100.0, 100.0, 1.0).group("Left output"),
            toggle("ll_inv", "Invert left into left", false).group("Left output"),
            num("lr", "Left from right", "%", 0.0, 100.0, 0.0, 1.0).group("Left output"),
            toggle("lr_inv", "Invert right into left", false).group("Left output"),
            num("rl", "Right from left", "%", 0.0, 100.0, 0.0, 1.0).group("Right output"),
            toggle("rl_inv", "Invert left into right", false).group("Right output"),
            num("rr", "Right from right", "%", 0.0, 100.0, 100.0, 1.0).group("Right output"),
            toggle("rr_inv", "Invert right into right", false).group("Right output"),
        ],
    }
}

pub struct ChannelMixer {
    /// Output left and right, each as (from left, from right).
    m: [[f32; 2]; 2],
}

impl ChannelMixer {
    pub fn new(p: &Params) -> Self {
        let part = |k: &str, inv: &str| {
            let g = (p.get(k) / 100.0) as f32;
            if p.on(inv) {
                -g
            } else {
                g
            }
        };
        Self {
            m: [
                [part("ll", "ll_inv"), part("lr", "lr_inv")],
                [part("rl", "rl_inv"), part("rr", "rr_inv")],
            ],
        }
    }
}

impl Processor for ChannelMixer {
    fn process(&mut self, buf: &mut [f32]) {
        let [[a, b], [c, d]] = self.m;
        for f in buf.as_chunks_mut::<2>().0 {
            let (l, r) = (f[0], f[1]);
            f[0] = a * l + b * r;
            f[1] = c * l + d * r;
        }
    }

    fn reset(&mut self) {}

    fn update(&mut self, p: &Params) -> bool {
        *self = Self::new(p);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn mix(over: &[(&str, f64)], l: f32, r: f32) -> (f32, f32) {
        let saved: BTreeMap<String, f64> =
            over.iter().map(|(k, v)| ((*k).to_string(), *v)).collect();
        let mut m = ChannelMixer::new(&Params::resolve(&mixer_info(), &saved));
        let mut b = [l, r];
        m.process(&mut b);
        (b[0], b[1])
    }

    #[test]
    fn passes_swaps_and_flips() {
        assert_eq!(mix(&[], 0.5, -0.25), (0.5, -0.25));
        let swap = [("ll", 0.0), ("lr", 100.0), ("rl", 100.0), ("rr", 0.0)];
        assert_eq!(mix(&swap, 0.5, -0.25), (-0.25, 0.5));
        assert_eq!(mix(&[("rr_inv", 1.0)], 0.5, 0.5), (0.5, -0.5));
        let (l, r) = mix(
            &[("ll", 50.0), ("lr", 50.0), ("rl", 50.0), ("rr", 50.0)],
            1.0,
            0.0,
        );
        assert!((l - 0.5).abs() < 1e-6 && (r - 0.5).abs() < 1e-6);
    }
}
