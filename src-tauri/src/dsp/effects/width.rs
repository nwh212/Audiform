// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

//! Stereo Width: moves what the two sides share (the center) left or
//! right, and makes the stereo picture narrower or wider.
//!
//! The sound is split into mid (what left and right share) and side
//! (where they differ). Width scales the side: 0% is mono, 100% leaves
//! the sound as it is, 300% triples the difference. Center Pan places the
//! mid with an equal-power pan law that is unity at the center.

use super::{num, EffectInfo, Params, Processor};

pub const WIDTH: &str = "altaudio.stereo_width";

pub fn width_info() -> EffectInfo {
    EffectInfo {
        id: WIDTH,
        name: "Stereo Width",
        category: "Stereo",
        description: "Moves the center of the stereo picture left or right, and makes it narrower (down to mono) or wider.",
        params: vec![
            num("pan", "Center Pan", "", -100.0, 100.0, 0.0, 1.0),
            num("width", "Width", "%", 0.0, 300.0, 100.0, 1.0),
        ],
    }
}

pub struct StereoWidth {
    gl: f32,
    gr: f32,
    side: f32,
    /// Smoothed values, so automation and drags never click.
    cur: [f32; 3],
    smooth: f32,
}

impl StereoWidth {
    pub fn new(p: &Params, rate: u32) -> Self {
        let mut s = Self {
            gl: 1.0,
            gr: 1.0,
            side: 1.0,
            cur: [1.0; 3],
            smooth: (-1.0 / (0.01 * rate as f32)).exp(),
        };
        s.set(p);
        s.cur = [s.gl, s.gr, s.side];
        s
    }

    fn set(&mut self, p: &Params) {
        // Equal power, scaled so the center is unity on both sides.
        let theta = ((p.get("pan") as f32 / 100.0 + 1.0) * std::f32::consts::FRAC_PI_4)
            .clamp(0.0, std::f32::consts::FRAC_PI_2);
        self.gl = theta.cos() * std::f32::consts::SQRT_2;
        self.gr = theta.sin() * std::f32::consts::SQRT_2;
        self.side = (p.get("width") / 100.0) as f32;
    }
}

impl Processor for StereoWidth {
    fn process(&mut self, buf: &mut [f32]) {
        let want = [self.gl, self.gr, self.side];
        for f in buf.as_chunks_mut::<2>().0 {
            for (c, w) in self.cur.iter_mut().zip(want) {
                *c = w + (*c - w) * self.smooth;
            }
            let mid = (f[0] + f[1]) * 0.5;
            let side = (f[0] - f[1]) * 0.5 * self.cur[2];
            f[0] = mid * self.cur[0] + side;
            f[1] = mid * self.cur[1] - side;
        }
    }

    fn reset(&mut self) {
        self.cur = [self.gl, self.gr, self.side];
    }

    fn update(&mut self, p: &Params) -> bool {
        self.set(p);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::effects::{build, info};
    use crate::session::model::EffectSlot;
    use std::collections::BTreeMap;

    fn run(pan: f64, width: f64, l: f32, r: f32) -> [f32; 2] {
        let slot = EffectSlot {
            id: crate::session::model::new_id(),
            effect: WIDTH.into(),
            enabled: true,
            params: BTreeMap::from([("pan".to_string(), pan), ("width".to_string(), width)]),
            preset: None,
            sidechain: None,
            plugin_state: None,
            data: None,
            position: None,
            routing: None,
        };
        let mut fx = build(&slot, 48_000).unwrap();
        let mut buf: Vec<f32> = (0..4800).flat_map(|_| [l, r]).collect();
        fx.process(&mut buf);
        let n = buf.len();
        [buf[n - 2], buf[n - 1]]
    }

    #[test]
    fn as_is_at_the_defaults() {
        let [l, r] = run(0.0, 100.0, 0.7, -0.2);
        assert!((l - 0.7).abs() < 1e-4 && (r + 0.2).abs() < 1e-4);
        assert!(info(WIDTH).is_some());
    }

    #[test]
    fn zero_width_is_mono_and_wide_is_wider() {
        let [l, r] = run(0.0, 0.0, 1.0, 0.0);
        assert!((l - r).abs() < 1e-4 && (l - 0.5).abs() < 1e-4);
        let [l, r] = run(0.0, 300.0, 1.0, 0.0);
        assert!((l - r).abs() > 2.9, "side tripled: {l} {r}");
    }

    #[test]
    fn center_pan_moves_the_shared_part() {
        let [l, r] = run(-100.0, 100.0, 0.5, 0.5);
        assert!(l > 0.6 && r.abs() < 1e-3, "{l} {r}");
        let [l, r] = run(100.0, 100.0, 0.5, 0.5);
        assert!(r > 0.6 && l.abs() < 1e-3, "{l} {r}");
    }
}
