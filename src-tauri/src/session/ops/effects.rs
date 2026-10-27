// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

//! Voice Chain edits on tracks, clips, buses and the master.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{bus_index, track_index, Op, OpError};
use crate::session::model::{Id, Session};

/// Whose Voice Chain an edit changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum EffectTarget {
    Track(Id),
    Clip(Id),
    Bus(Id),
    Master,
}

/// Applies an effects edit and returns its inverse.
pub(super) fn apply_one(s: &mut Session, op: &Op) -> Result<Op, OpError> {
    match op {
        Op::SetEffects { target, chain } => {
            let nodes: Vec<Id> = s
                .tracks
                .iter()
                .map(|t| t.id)
                .chain(s.buses.iter().map(|b| b.id))
                .collect();
            let slot = match *target {
                EffectTarget::Track(id) => {
                    let i = track_index(s, id)?;
                    &mut s.tracks[i].effects
                }
                EffectTarget::Clip(id) => {
                    &mut s
                        .tracks
                        .iter_mut()
                        .flat_map(|t| t.clips.iter_mut())
                        .find(|c| c.id == id)
                        .ok_or(OpError::NoClip(id))?
                        .effects
                }
                EffectTarget::Bus(id) => {
                    let i = bus_index(s, id)?;
                    &mut s.buses[i].effects
                }
                EffectTarget::Master => &mut s.master.effects,
            };
            // A key may point at a removed track (undo brings it back), but
            // a new key must point at a track or bus that exists.
            let had: Vec<Id> = crate::session::routing::key_sources(slot).collect();
            let known = |src: Id| had.contains(&src) || nodes.contains(&src);
            if let Some(src) = crate::session::routing::key_sources(chain).find(|src| !known(*src))
            {
                return Err(OpError::NoKeySource(src));
            }
            let mut chain = chain.clone();
            chain.slots.truncate(crate::dsp::effects::MAX_SLOTS);
            let old = std::mem::replace(slot, chain);
            Ok(Op::SetEffects {
                target: *target,
                chain: old,
            })
        }
        _ => unreachable!("not an effects edit"),
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::roundtrip;
    use super::super::*;
    use crate::session::model::*;

    #[test]
    fn voice_chains_set_and_undo() {
        let mut s = Session::new("Test", 48_000);
        let t = new_id();
        roundtrip(
            &mut s,
            Op::AddTrack {
                id: t,
                name: "Host".into(),
                stereo: false,
                index: 0,
                midi: false,
            },
        );
        let mut chain = EffectChain::default();
        for _ in 0..20 {
            chain.slots.push(crate::session::model::EffectSlot {
                id: new_id(),
                effect: "altaudio.gain".into(),
                enabled: true,
                params: Default::default(),
                preset: None,
                sidechain: None,
                plugin_state: None,
                data: None,
                position: None,
                routing: None,
            });
        }
        roundtrip(
            &mut s,
            Op::SetEffects {
                target: EffectTarget::Track(t),
                chain: chain.clone(),
            },
        );
        assert_eq!(s.tracks[0].effects.slots.len(), 16, "a chain holds 16");
        roundtrip(
            &mut s,
            Op::SetEffects {
                target: EffectTarget::Master,
                chain,
            },
        );
        let missing = Op::SetEffects {
            target: EffectTarget::Clip(new_id()),
            chain: EffectChain::default(),
        };
        assert!(matches!(apply(&mut s, &missing), Err(OpError::NoClip(_))));
    }
}
