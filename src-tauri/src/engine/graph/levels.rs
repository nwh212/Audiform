// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

//! Levels for the mix graph: what each track and bus is heard at (fader,
//! pan, mute, solo, sends and 3D Space placement), worked out from the
//! session. Pure functions of the session, kept apart from the graph that
//! uses them.

use std::sync::Arc;

use crate::session::model::Session;
use crate::session::routing::{self, Node};

/// Levels for one track or bus, as left and right multipliers on the
/// sound after its chain: `main` for its output, then one per send.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct NodeLevel {
    pub main: [f32; 2],
    pub sends: Vec<[f32; 2]>,
    /// Fold left and right into one after the chain (the Mono button).
    pub mono: bool,
    /// 3D Space sessions: where it plays in the room, when it feeds the
    /// master.
    pub place: Option<Arc<Place>>,
}

/// A source's spot in a 3D Space session, worked out as gains: per side
/// (left, right), one per speaker for the bed, and stereo for listening.
#[derive(Debug, Clone, PartialEq)]
pub struct Place {
    pub bed: [Vec<f32>; 2],
    pub fold: [[f32; 2]; 2],
    /// How much feeds the low-frequency channel (its index), from both sides.
    pub lfe: Option<(usize, f32)>,
    /// Spread and Spatialize, when either is on.
    pub field: Option<crate::dsp::diffuse::Field>,
    /// What it was worked out from, so automation can move one setting.
    pub layout: crate::session::model::SpaceLayout,
    pub pos: crate::session::model::SpacePos,
    pub stereo: bool,
}

impl Place {
    fn new(
        s: &Session,
        pos: Option<&crate::session::model::SpacePos>,
        stereo: bool,
    ) -> Option<Arc<Place>> {
        let layout = s.space?.layout;
        let pos = pos.copied().unwrap_or_default();
        Some(Arc::new(Place::at(layout, &pos, stereo)))
    }

    /// Straight into speaker `c` of `layout` alone (for one channel of a
    /// multichannel clip, on a mono strip), folded to stereo by where the
    /// speaker stands; the low-frequency channel is left out of the fold.
    pub fn speaker(layout: crate::session::model::SpaceLayout, c: usize) -> Place {
        let sp = crate::dsp::space::speakers(layout);
        let mut g = vec![0f32; sp.len()];
        if let Some(v) = g.get_mut(c) {
            // A mono strip carries its sound on both sides: half from each.
            *v = 0.5;
        }
        let pos = crate::session::model::SpacePos {
            x: sp
                .get(c)
                .and_then(|s| s.at)
                .map_or(0.0, |a| f64::from(a[0])),
            ..Default::default()
        };
        let fold = if sp.get(c).is_some_and(|s| s.at.is_some()) {
            crate::dsp::space::fold_gains(&pos, false)
        } else {
            [[0.0; 2]; 2]
        };
        Place {
            bed: [g.clone(), g],
            fold,
            lfe: None,
            field: None,
            layout,
            pos,
            stereo: false,
        }
    }

    /// The gains for a spot in `layout`.
    pub fn at(
        layout: crate::session::model::SpaceLayout,
        pos: &crate::session::model::SpacePos,
        stereo: bool,
    ) -> Place {
        let pos = pos.tidy();
        let [l, r] = crate::dsp::space::source_gains(layout, &pos, stereo);
        let lfe = crate::dsp::space::lfe_channel(layout)
            .filter(|_| pos.lfe > 0.0)
            .map(|c| (c, pos.lfe as f32 * 0.5));
        let field = crate::dsp::diffuse::field(
            layout,
            pos.z as f32,
            pos.spread as f32,
            pos.spatialize as f32,
            pos.spatialize_mode,
            stereo,
        );
        // The placed sound gives up what Spread and Spatialize take.
        let k = field.direct;
        let scale = |g: Vec<f32>| g.into_iter().map(|v| v * k).collect::<Vec<f32>>();
        let fold = crate::dsp::space::fold_gains(&pos, stereo).map(|s| s.map(|v| v * k));
        Place {
            bed: [scale(l), scale(r)],
            fold,
            lfe,
            field: field.on.then_some(field),
            layout,
            pos,
            stereo,
        }
    }
}

/// Every level in the graph. Changes apply live.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Levels {
    pub tracks: Vec<NodeLevel>,
    pub buses: Vec<NodeLevel>,
}

fn db(v: f64) -> f32 {
    10f64.powf(v.clamp(-96.0, 24.0) / 20.0) as f32
}

/// Pan as a balance: centered plays both sides at full level, and
/// panning turns the far side down, so a centered voice plays at
/// exactly the level the fader shows.
pub fn balance(pan: f64) -> [f32; 2] {
    let pan = (pan.clamp(-100.0, 100.0) / 100.0) as f32;
    let left = if pan > 0.0 { 1.0 - pan } else { 1.0 };
    let right = if pan < 0.0 { 1.0 + pan } else { 1.0 };
    [left, right]
}

/// Who can be heard, with mute and solo. With nothing soloed, everything
/// not muted. With a solo, a soloed track or bus is heard along with what
/// feeds it and the buses it feeds (so soloing a bus plays its tracks,
/// and soloing a track plays it through its bus and its reverb send).
pub fn audible(s: &Session) -> (Vec<bool>, Vec<bool>) {
    let soloed: Vec<Node> = s
        .tracks
        .iter()
        .enumerate()
        .filter(|(_, t)| t.solo)
        .map(|(i, _)| Node::Track(i))
        .chain(
            s.buses
                .iter()
                .enumerate()
                .filter(|(_, b)| b.solo)
                .map(|(i, _)| Node::Bus(i)),
        )
        .collect();
    let mut ok_t = vec![soloed.is_empty(); s.tracks.len()];
    let mut ok_b = vec![soloed.is_empty(); s.buses.len()];
    if !soloed.is_empty() {
        let routes = routing::routes(s);
        let walk = |forward: bool, ok_t: &mut Vec<bool>, ok_b: &mut Vec<bool>| {
            let mut stack = soloed.clone();
            let mut seen = Vec::new();
            while let Some(n) = stack.pop() {
                if seen.contains(&n) {
                    continue;
                }
                seen.push(n);
                match n {
                    Node::Track(i) => ok_t[i] = true,
                    Node::Bus(i) => ok_b[i] = true,
                }
                for (a, b) in &routes {
                    let (from, to) = if forward { (*a, *b) } else { (*b, *a) };
                    if from == n {
                        stack.push(to);
                    }
                }
            }
        };
        walk(true, &mut ok_t, &mut ok_b);
        walk(false, &mut ok_t, &mut ok_b);
    }
    let t = s
        .tracks
        .iter()
        .zip(ok_t)
        .map(|(t, ok)| ok && !t.muted)
        .collect();
    let b = s
        .buses
        .iter()
        .zip(ok_b)
        .map(|(b, ok)| ok && !b.muted)
        .collect();
    (t, b)
}

/// Every level from the session: faders, pans, mutes, solos, polarity
/// and sends. A post-fader send follows the fader but not the pan (it
/// has its own); a pre-fader send ignores both. Every send follows mute
/// and solo.
pub fn levels(s: &Session) -> Levels {
    let (heard_t, heard_b) = audible(s);
    let space = s.space.is_some();
    let node = |heard: bool,
                sign: f32,
                volume_db: f64,
                pan: f64,
                sends: &[crate::session::model::Send],
                mono: bool,
                place: Option<Arc<Place>>| {
        let on = if heard { sign } else { 0.0 };
        let fader = db(volume_db);
        // In a 3D Space session the spot does what the pan would.
        let [l, r] = if space { [1.0, 1.0] } else { balance(pan) };
        NodeLevel {
            main: [on * fader * l, on * fader * r],
            sends: sends
                .iter()
                .map(|x| {
                    let g = if x.enabled { on * db(x.level_db) } else { 0.0 };
                    let g = if x.pre_fader { g } else { g * fader };
                    let [l, r] = balance(x.pan);
                    [g * l, g * r]
                })
                .collect(),
            mono,
            place,
        }
    };
    Levels {
        tracks: s
            .tracks
            .iter()
            .zip(heard_t)
            .map(|(t, h)| {
                let sign = if t.polarity_inverted { -1.0 } else { 1.0 };
                // An automated volume or pan takes the fader's or pan's place.
                let vol = if crate::engine::automation::automates_volume(&t.envelopes) {
                    0.0
                } else {
                    t.volume_db
                };
                let pan = if crate::engine::automation::automates_pan(&t.envelopes) {
                    0.0
                } else {
                    t.pan
                };
                let stereo = t.channels != crate::session::model::ChannelLayout::Mono && !t.mono;
                let place = (t.output == crate::session::model::Output::Master)
                    .then(|| match (t.speaker, s.space) {
                        (Some(c), Some(sp)) => Some(Arc::new(Place::speaker(sp.layout, c))),
                        _ => Place::new(s, t.space.as_ref(), stereo),
                    })
                    .flatten();
                node(h, sign, vol, pan, &t.sends, t.mono, place)
            })
            .collect(),
        buses: s
            .buses
            .iter()
            .zip(heard_b)
            .map(|(b, h)| {
                let vol = if crate::engine::automation::automates_volume(&b.envelopes) {
                    0.0
                } else {
                    b.volume_db
                };
                let pan = if crate::engine::automation::automates_pan(&b.envelopes) {
                    0.0
                } else {
                    b.pan
                };
                let stereo = b.channels != crate::session::model::ChannelLayout::Mono && !b.mono;
                let place = (b.output == crate::session::model::Output::Master)
                    .then(|| Place::new(s, b.space.as_ref(), stereo))
                    .flatten();
                node(h, 1.0, vol, pan, &b.sends, b.mono, place)
            })
            .collect(),
    }
}
