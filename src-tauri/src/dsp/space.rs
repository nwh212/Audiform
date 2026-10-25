// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

//! 3D Space sessions: speaker layouts, and placing a sound at a spot in
//! the room onto them.
//!
//! The room is a box: x runs left (-1) to right (1), y back (-1) to
//! front (1), z floor (0) to ceiling (1). Each speaker sits on that box.
//! A sound is placed by working one axis at a time: between the floor
//! and ceiling speakers by its height, then within each layer between
//! the two rows of speakers either side of it (front, side, back), then
//! within each row between the two speakers either side of it. Every
//! step shares power (cos and sin), so a sound keeps its loudness as it
//! moves. Size then blends toward all the speakers of its layers, for a
//! sound that fills the room. Our own method.
//!
//! Listening: most people mix on stereo speakers or headphones, so
//! playback folds each sound to stereo by where it sits left to right.
//! Exports write every speaker channel.

use serde::Serialize;

use crate::session::model::{SpaceLayout, SpacePos};

/// One speaker: its WAV channel bit and its place in the room (None for
/// the low-frequency channel, which sounds are not placed in).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Speaker {
    pub name: &'static str,
    pub mask: u32,
    pub at: Option<[f32; 3]>,
}

const FL: Speaker = Speaker {
    name: "L",
    mask: 0x1,
    at: Some([-1.0, 1.0, 0.0]),
};
const FR: Speaker = Speaker {
    name: "R",
    mask: 0x2,
    at: Some([1.0, 1.0, 0.0]),
};
const FC: Speaker = Speaker {
    name: "C",
    mask: 0x4,
    at: Some([0.0, 1.0, 0.0]),
};
const LFE: Speaker = Speaker {
    name: "LFE",
    mask: 0x8,
    at: None,
};
const BL: Speaker = Speaker {
    name: "Ls",
    mask: 0x10,
    at: Some([-1.0, -1.0, 0.0]),
};
const BR: Speaker = Speaker {
    name: "Rs",
    mask: 0x20,
    at: Some([1.0, -1.0, 0.0]),
};
const BL7: Speaker = Speaker {
    name: "Lrs",
    mask: 0x10,
    at: Some([-1.0, -1.0, 0.0]),
};
const BR7: Speaker = Speaker {
    name: "Rrs",
    mask: 0x20,
    at: Some([1.0, -1.0, 0.0]),
};
const SL: Speaker = Speaker {
    name: "Lss",
    mask: 0x200,
    at: Some([-1.0, 0.0, 0.0]),
};
const SR: Speaker = Speaker {
    name: "Rss",
    mask: 0x400,
    at: Some([1.0, 0.0, 0.0]),
};
const TML: Speaker = Speaker {
    name: "Ltm",
    mask: 0x1000,
    at: Some([-1.0, 0.0, 1.0]),
};
const TMR: Speaker = Speaker {
    name: "Rtm",
    mask: 0x4000,
    at: Some([1.0, 0.0, 1.0]),
};
const TFL: Speaker = Speaker {
    name: "Ltf",
    mask: 0x1000,
    at: Some([-1.0, 1.0, 1.0]),
};
const TFR: Speaker = Speaker {
    name: "Rtf",
    mask: 0x4000,
    at: Some([1.0, 1.0, 1.0]),
};
const TBL: Speaker = Speaker {
    name: "Ltr",
    mask: 0x8000,
    at: Some([-1.0, -1.0, 1.0]),
};
const TBR: Speaker = Speaker {
    name: "Rtr",
    mask: 0x20000,
    at: Some([1.0, -1.0, 1.0]),
};

/// A layout's speakers, in WAV channel order (by channel bit).
pub fn speakers(layout: SpaceLayout) -> &'static [Speaker] {
    match layout {
        SpaceLayout::Stereo20 => &[FL, FR],
        SpaceLayout::Surround51 => &[FL, FR, FC, LFE, BL, BR],
        SpaceLayout::Surround512 => &[FL, FR, FC, LFE, BL, BR, TML, TMR],
        SpaceLayout::Surround712 => &[FL, FR, FC, LFE, BL7, BR7, SL, SR, TML, TMR],
        SpaceLayout::Surround71 => &[FL, FR, FC, LFE, BL7, BR7, SL, SR],
        SpaceLayout::Surround514 => &[FL, FR, FC, LFE, BL, BR, TFL, TFR, TBL, TBR],
        SpaceLayout::Surround714 => &[FL, FR, FC, LFE, BL7, BR7, SL, SR, TFL, TFR, TBL, TBR],
    }
}

/// The WAV channel mask for a layout.
pub fn channel_mask(layout: SpaceLayout) -> u32 {
    speakers(layout).iter().map(|s| s.mask).sum()
}

/// What the interface shows about a layout.
#[derive(Debug, Clone, Serialize)]
pub struct LayoutInfo {
    pub channels: usize,
    pub names: Vec<&'static str>,
    /// Each placed speaker: name and where it is.
    pub spots: Vec<(&'static str, [f32; 3])>,
}

pub fn layout_info(layout: SpaceLayout) -> LayoutInfo {
    let sp = speakers(layout);
    LayoutInfo {
        channels: sp.len(),
        names: sp.iter().map(|s| s.name).collect(),
        spots: sp
            .iter()
            .filter_map(|s| s.at.map(|a| (s.name, a)))
            .collect(),
    }
}

/// Shares a unit of power between two places, `t` of the way from the first to the second.
fn share(t: f32) -> (f32, f32) {
    let a = t.clamp(0.0, 1.0) * std::f32::consts::FRAC_PI_2;
    (a.cos(), a.sin())
}

/// Spreads a unit of power between the two entries of `keys` (sorted low
/// to high) either side of `v`. Returns (index, gain) pairs.
fn between(keys: &[f32], v: f32) -> [(usize, f32); 2] {
    if v <= keys[0] {
        return [(0, 1.0), (0, 0.0)];
    }
    let last = keys.len() - 1;
    if v >= keys[last] {
        return [(last, 1.0), (last, 0.0)];
    }
    let i = keys
        .windows(2)
        .position(|w| v >= w[0] && v <= w[1])
        .unwrap_or(0);
    let (a, b) = share((v - keys[i]) / (keys[i + 1] - keys[i]).max(1e-6));
    [(i, a), (i + 1, b)]
}

/// Gains for one point, one per speaker of `layout` (in channel order).
pub fn point_gains(layout: SpaceLayout, x: f32, y: f32, z: f32, size: f32) -> Vec<f32> {
    let sp = speakers(layout);
    let mut g = vec![0f32; sp.len()];
    let has_top = sp.iter().any(|s| s.at.is_some_and(|a| a[2] > 0.5));
    let (floor, top) = if has_top { share(z) } else { (1.0, 0.0) };
    for (layer, lg) in [(0.0f32, floor), (1.0f32, top)] {
        if lg <= 0.0 {
            continue;
        }
        // This layer's speakers, grouped into rows by y.
        let mine: Vec<(usize, [f32; 3])> = sp
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.at.filter(|a| (a[2] - layer).abs() < 0.5).map(|a| (i, a)))
            .collect();
        let mut rows: Vec<f32> = mine.iter().map(|(_, a)| a[1]).collect();
        rows.sort_by(f32::total_cmp);
        rows.dedup();
        for (ri, rg) in between(&rows, y) {
            if rg <= 0.0 {
                continue;
            }
            let mut row: Vec<(usize, f32)> = mine
                .iter()
                .filter(|(_, a)| (a[1] - rows[ri]).abs() < 1e-6)
                .map(|(i, a)| (*i, a[0]))
                .collect();
            row.sort_by(|a, b| a.1.total_cmp(&b.1));
            let xs: Vec<f32> = row.iter().map(|r| r.1).collect();
            for (k, kg) in between(&xs, x) {
                g[row[k].0] += lg * rg * kg;
            }
        }
    }
    let size = size.clamp(0.0, 1.0);
    if size > 0.0 {
        // Blend toward every placed speaker of the layers in use.
        let used: Vec<usize> = sp
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                s.at.is_some_and(|a| if a[2] > 0.5 { top > 0.0 } else { floor > 0.0 })
            })
            .map(|(i, _)| i)
            .collect();
        let u = 1.0 / (used.len().max(1) as f32).sqrt();
        for v in g.iter_mut() {
            *v *= 1.0 - size;
        }
        for &i in &used {
            g[i] += size * u;
        }
        let p: f32 = g.iter().map(|v| v * v).sum::<f32>().sqrt();
        if p > 0.0 {
            for v in g.iter_mut() {
                *v /= p;
            }
        }
    }
    g
}

/// The two points a source plays from: one for mono, a pair for stereo
/// (left and right spread by `width` about its spot).
pub fn source_gains(layout: SpaceLayout, pos: &SpacePos, stereo: bool) -> [Vec<f32>; 2] {
    let (x, y, z, size) = (pos.x as f32, pos.y as f32, pos.z as f32, pos.size as f32);
    if !stereo {
        // A mono source has the same sound on both sides: half from each.
        let g: Vec<f32> = point_gains(layout, x, y, z, size)
            .iter()
            .map(|v| v * 0.5)
            .collect();
        return [g.clone(), g];
    }
    let w = pos.width as f32;
    [
        point_gains(layout, (x - w).clamp(-1.0, 1.0), y, z, size),
        point_gains(layout, (x + w).clamp(-1.0, 1.0), y, z, size),
    ]
}

/// Stereo gains for listening: each point folded by where it sits left to
/// right, keeping its loudness.
pub fn fold_gains(pos: &SpacePos, stereo: bool) -> [[f32; 2]; 2] {
    let lr = |x: f64| {
        let (l, r) = share(((x as f32) + 1.0) / 2.0);
        [l, r]
    };
    if !stereo {
        let [l, r] = lr(pos.x);
        return [[l * 0.5, r * 0.5]; 2];
    }
    [
        lr((pos.x - pos.width).clamp(-1.0, 1.0)),
        lr((pos.x + pos.width).clamp(-1.0, 1.0)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn power(g: &[f32]) -> f32 {
        g.iter().map(|v| v * v).sum()
    }

    #[test]
    fn a_point_on_a_speaker_plays_only_from_it() {
        let g = point_gains(SpaceLayout::Surround714, 0.0, 1.0, 0.0, 0.0);
        assert!((g[2] - 1.0).abs() < 1e-6, "center: {g:?}");
        assert!((power(&g) - 1.0).abs() < 1e-5);
        let g = point_gains(SpaceLayout::Surround714, -1.0, -1.0, 1.0, 0.0);
        assert!((g[10] - 1.0).abs() < 1e-6, "top rear left: {g:?}");
        // The low-frequency channel never gets a placed sound.
        assert_eq!(g[3], 0.0);
    }

    #[test]
    fn moving_keeps_loudness_and_shares_between_neighbors() {
        for layout in SpaceLayout::ALL {
            for &(x, y, z) in &[
                (0.3, 0.7, 0.0),
                (-0.8, -0.2, 0.4),
                (1.0, 0.0, 1.0),
                (0.0, 0.0, 0.5),
            ] {
                let g = point_gains(layout, x, y, z, 0.0);
                assert!(
                    (power(&g) - 1.0).abs() < 1e-4,
                    "{layout:?} {x} {y} {z}: {g:?}"
                );
                let g = point_gains(layout, x, y, z, 0.6);
                assert!((power(&g) - 1.0).abs() < 1e-4);
            }
        }
        // Halfway between left and center: both at -3 dB.
        let g = point_gains(SpaceLayout::Surround51, -0.5, 1.0, 0.0, 0.0);
        assert!((g[0] - g[2]).abs() < 1e-6 && (g[0] - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-3);
        // A layout without height keeps a raised sound on the floor.
        let g = point_gains(SpaceLayout::Surround51, 0.0, 1.0, 1.0, 0.0);
        assert!((g[2] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn masks_and_folding() {
        assert_eq!(channel_mask(SpaceLayout::Surround51), 0x3F);
        assert_eq!(channel_mask(SpaceLayout::Surround714), 0x2D63F);
        assert_eq!(speakers(SpaceLayout::Surround714).len(), 12);
        let pos = SpacePos {
            x: -1.0,
            ..SpacePos::default()
        };
        let f = fold_gains(&pos, false);
        assert!((f[0][0] - 0.5).abs() < 1e-6 && f[0][1].abs() < 1e-6);
    }
}
