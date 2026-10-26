// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Track and bus automation: volume, pan and effect settings moving over
//! the session's timeline, from each track's and bus's envelopes.
//!
//! A volume envelope takes the fader's place while it plays (Read), as
//! a pan envelope takes the pan's. Effect settings move every 256 frames,
//! as clip effect automation does. Values hold flat before the first
//! point and after the last, and move in straight lines between points.

use std::sync::Arc;

use super::graph::Place;
use super::plan::{to_out, ParamLane};
use crate::session::model::{Envelope, EnvelopeTarget, Send, SpaceParam};

/// Lowest volume a lane reaches: at or below it is silence.
const SILENT_DB: f32 = -59.9;

/// The envelopes that play: on, and with at least one point.
fn playing(envs: &[Envelope]) -> impl Iterator<Item = &Envelope> {
    envs.iter().filter(|e| e.enabled && !e.points.is_empty())
}

/// True when any envelope of a track or bus plays.
pub fn has_automation(envs: &[Envelope]) -> bool {
    playing(envs).next().is_some()
}

/// True when a volume envelope plays (the fader is set aside).
pub fn automates_volume(envs: &[Envelope]) -> bool {
    playing(envs).any(|e| e.target == EnvelopeTarget::Volume)
}

/// True when a pan envelope plays (the pan is set aside).
pub fn automates_pan(envs: &[Envelope]) -> bool {
    playing(envs).any(|e| e.target == EnvelopeTarget::Pan)
}

fn lane(e: &Envelope, session_rate: u32, out_rate: u32) -> ParamLane {
    let (slot, param) = match &e.target {
        EnvelopeTarget::EffectParam { slot_id, param } => (*slot_id, param.clone()),
        _ => (crate::session::model::Id::nil(), String::new()),
    };
    ParamLane {
        slot,
        param,
        points: e
            .points
            .iter()
            .map(|p| (to_out(p.position, session_rate, out_rate), p.value as f32))
            .collect(),
    }
}

/// A strip's automation, ready to play, with its scratch.
#[derive(Default)]
pub struct StripAuto {
    volume: Option<ParamLane>,
    pan: Option<ParamLane>,
    pub params: Vec<ParamLane>,
    /// 3D Space sessions: settings of the spot that move.
    pub space: Vec<(SpaceParam, ParamLane)>,
    /// Per send: true when it follows the fader (and so the volume lane).
    post: Vec<bool>,
    /// Per frame of the last block: volume gain, and pan as left and right.
    pub vol: Vec<f32>,
    pub pan_lr: Vec<[f32; 2]>,
}

impl StripAuto {
    /// The automation for a track's or bus's envelopes and sends, or None
    /// when nothing moves.
    pub fn new(
        envs: &[Envelope],
        sends: &[Send],
        session_rate: u32,
        out_rate: u32,
    ) -> Option<StripAuto> {
        if !has_automation(envs) {
            return None;
        }
        let mut a = StripAuto {
            post: sends.iter().map(|s| !s.pre_fader).collect(),
            ..StripAuto::default()
        };
        for e in playing(envs) {
            match e.target {
                EnvelopeTarget::Volume => a.volume = Some(lane(e, session_rate, out_rate)),
                EnvelopeTarget::Pan => a.pan = Some(lane(e, session_rate, out_rate)),
                EnvelopeTarget::EffectParam { .. } => {
                    a.params.push(lane(e, session_rate, out_rate))
                }
                EnvelopeTarget::Space { param } => {
                    a.space.push((param, lane(e, session_rate, out_rate)))
                }
            }
        }
        Some(a)
    }

    /// The spot at timeline frame `pos`: `base` with the automated settings
    /// in place. None when no spot setting moves. (Worked out once a block
    /// on the render thread, which mixes ahead of the speakers.)
    pub fn place_at(&self, base: &Place, pos: u64) -> Option<Arc<Place>> {
        if self.space.is_empty() {
            return None;
        }
        let mut spot = base.pos;
        for (param, l) in &self.space {
            param.set(&mut spot, f64::from(l.value_at(pos)));
        }
        Some(Arc::new(Place::at(base.layout, &spot, base.stereo)))
    }

    pub fn moves_level(&self) -> bool {
        self.volume.is_some() || self.pan.is_some()
    }

    /// Works out the volume and pan for `frames` frames from timeline frame `pos`.
    pub fn fill(&mut self, pos: u64, frames: usize) {
        self.vol.clear();
        self.pan_lr.clear();
        for f in 0..frames {
            let at = pos + f as u64;
            let v = match &self.volume {
                Some(l) => {
                    let db = l.value_at(at);
                    if db <= SILENT_DB {
                        0.0
                    } else {
                        10f32.powf(db / 20.0)
                    }
                }
                None => 1.0,
            };
            self.vol.push(v);
            let p = match &self.pan {
                Some(l) => super::graph::balance(f64::from(l.value_at(at))),
                None => [1.0, 1.0],
            };
            self.pan_lr.push(p);
        }
    }

    /// Applies the automated level to one route's sound: the output gets
    /// volume and pan; a send after the fader gets the volume.
    pub fn apply(&self, route: usize, buf: &mut [f32]) {
        if !self.moves_level() {
            return;
        }
        let post = route == 0 || self.post.get(route - 1).copied().unwrap_or(true);
        if !post {
            return;
        }
        for (i, f) in buf.as_chunks_mut::<2>().0.iter_mut().enumerate() {
            let v = self.vol.get(i).copied().unwrap_or(1.0);
            let [l, r] = if route == 0 {
                self.pan_lr.get(i).copied().unwrap_or([1.0, 1.0])
            } else {
                [1.0, 1.0]
            };
            f[0] *= v * l;
            f[1] *= v * r;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::model::{new_id, EnvelopeCurve, EnvelopePoint};

    fn env(target: EnvelopeTarget, pts: &[(u64, f64)]) -> Envelope {
        Envelope {
            id: new_id(),
            target,
            enabled: true,
            points: pts
                .iter()
                .map(|&(position, value)| EnvelopePoint {
                    position,
                    value,
                    curve: EnvelopeCurve::Linear,
                })
                .collect(),
            height: None,
        }
    }

    #[test]
    fn volume_and_pan_follow_their_lanes() {
        let envs = vec![
            env(EnvelopeTarget::Volume, &[(0, 0.0), (1_000, -6.0206)]),
            env(EnvelopeTarget::Pan, &[(0, -100.0)]),
        ];
        let mut a = StripAuto::new(&envs, &[], 1_000, 1_000).unwrap();
        a.fill(1_000, 2);
        assert!((a.vol[0] - 0.5).abs() < 1e-3);
        assert_eq!(a.pan_lr[0], [1.0, 0.0]);
        let mut buf = vec![1.0, 1.0, 1.0, 1.0];
        a.apply(0, &mut buf);
        assert!((buf[0] - 0.5).abs() < 1e-3 && buf[1] == 0.0);
        // Empty or switched-off envelopes leave the strip alone.
        let mut off = env(EnvelopeTarget::Volume, &[(0, -20.0)]);
        off.enabled = false;
        assert!(StripAuto::new(&[off, env(EnvelopeTarget::Pan, &[])], &[], 1_000, 1_000).is_none());
    }

    #[test]
    fn a_space_lane_moves_the_spot() {
        use crate::session::model::{SpaceLayout, SpacePos};
        let envs = vec![env(
            EnvelopeTarget::Space {
                param: SpaceParam::X,
            },
            &[(0, -1.0), (1_000, 1.0)],
        )];
        let a = StripAuto::new(&envs, &[], 1_000, 1_000).unwrap();
        assert!(!a.moves_level());
        let base = Place::at(SpaceLayout::Surround51, &SpacePos::default(), false);
        // Mono at the front: from the left speaker to the right one.
        let start = a.place_at(&base, 0).unwrap();
        let end = a.place_at(&base, 1_000).unwrap();
        assert!(
            start.bed[0][0] > 0.4 && start.bed[0][1].abs() < 1e-6,
            "{:?}",
            start.bed
        );
        assert!(
            end.bed[0][1] > 0.4 && end.bed[0][0].abs() < 1e-6,
            "{:?}",
            end.bed
        );
        // The settings that do not move keep the spot's own.
        assert_eq!(start.pos.y, 1.0);
    }
}
