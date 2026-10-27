// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

//! Bus edits and aux sends.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{bus_index, check_output, id_taken, EffectTarget, Op, OpError, TrackChanges};
use crate::session::model::{Bus, ChannelLayout, EffectChain, Id, Output, Send, Session, SpacePos};

/// Bus settings to change. Fields left out stay as they are.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BusChanges {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub muted: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub solo: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume_db: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pan: Option<f64>,
    /// 3D Space sessions: where the bus sits in the room.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub space: Option<SpacePos>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    /// Where the bus's sound goes: the master or another bus.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<Output>,
    /// Where the bus shows among the tracks: below this many tracks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<u32>,
    /// Shows the bus below the last track again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clear_position: Option<bool>,
    /// Folds the bus to mono after its Voice Chain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mono: Option<bool>,
}

/// Applies a bus edit and returns its inverse.
pub(super) fn apply_one(s: &mut Session, op: &Op) -> Result<Op, OpError> {
    match op {
        Op::AddBus {
            id,
            name,
            stereo,
            index,
        } => {
            if id_taken(s, *id) {
                return Err(OpError::DuplicateId(*id));
            }
            let layout = if *stereo {
                ChannelLayout::Stereo
            } else {
                ChannelLayout::Mono
            };
            let mut b = Bus::new(name.clone(), layout);
            b.id = *id;
            s.buses.insert((*index).min(s.buses.len()), b);
            Ok(Op::RemoveBus { id: *id })
        }
        Op::RestoreBus { bus, index } => {
            if id_taken(s, bus.id) {
                return Err(OpError::DuplicateId(bus.id));
            }
            s.buses.insert((*index).min(s.buses.len()), (**bus).clone());
            Ok(Op::RemoveBus { id: bus.id })
        }
        Op::RemoveBus { id } => remove_bus(s, *id),
        Op::MoveBus { id, to } => {
            let from = bus_index(s, *id)?;
            let b = s.buses.remove(from);
            let to = (*to).min(s.buses.len());
            s.buses.insert(to, b);
            Ok(Op::MoveBus { id: *id, to: from })
        }
        Op::SetBus { id, changes } => {
            let i = bus_index(s, *id)?;
            if let Some(o) = &changes.output {
                check_output(s, o)?;
            }
            let b = &mut s.buses[i];
            let mut old = BusChanges::default();
            if let Some(v) = &changes.name {
                old.name = Some(std::mem::replace(&mut b.name, v.clone()));
            }
            if let Some(v) = changes.muted {
                old.muted = Some(std::mem::replace(&mut b.muted, v));
            }
            if let Some(v) = changes.solo {
                old.solo = Some(std::mem::replace(&mut b.solo, v));
            }
            if let Some(v) = &changes.color {
                old.color = Some(std::mem::replace(&mut b.color, v.clone()));
            }
            if let Some(v) = changes.volume_db {
                old.volume_db = Some(std::mem::replace(&mut b.volume_db, v));
            }
            if let Some(v) = changes.pan {
                old.pan = Some(std::mem::replace(&mut b.pan, v.clamp(-100.0, 100.0)));
            }
            if let Some(v) = changes.space {
                old.space = Some(b.space.replace(v.tidy()).unwrap_or_default());
            }
            if let Some(v) = changes.height {
                old.height = Some(std::mem::replace(&mut b.height, v.clamp(40, 600)));
            }
            if let Some(v) = changes.mono {
                old.mono = Some(std::mem::replace(&mut b.mono, v));
            }
            if let Some(v) = changes.output {
                old.output = Some(std::mem::replace(&mut b.output, v));
            }
            if changes.position.is_some() || changes.clear_position == Some(true) {
                let new = if changes.clear_position == Some(true) {
                    None
                } else {
                    changes.position
                };
                match std::mem::replace(&mut b.position, new) {
                    Some(p) => old.position = Some(p),
                    None => old.clear_position = Some(true),
                }
            }
            Ok(Op::SetBus {
                id: *id,
                changes: old,
            })
        }
        Op::SetSends { owner, sends } => {
            for x in sends {
                bus_index(s, x.bus_id)?;
            }
            let sends: Vec<Send> = sends
                .iter()
                .map(|x| Send {
                    level_db: x.level_db.clamp(-96.0, 12.0),
                    pan: x.pan.clamp(-100.0, 100.0),
                    ..x.clone()
                })
                .collect();
            let list = if let Some(t) = s.tracks.iter_mut().find(|t| t.id == *owner) {
                &mut t.sends
            } else if let Some(b) = s.buses.iter_mut().find(|b| b.id == *owner) {
                &mut b.sends
            } else {
                return Err(OpError::NoTrack(*owner));
            };
            let old = std::mem::replace(list, sends);
            Ok(Op::SetSends {
                owner: *owner,
                sends: old,
            })
        }
        _ => unreachable!("not a bus edit"),
    }
}

/// Removes a bus and everything that pointed at it. The inverse puts the
/// bus back first, then each route, sends list and chain as it was.
fn remove_bus(s: &mut Session, id: Id) -> Result<Op, OpError> {
    let index = bus_index(s, id)?;
    let mut undo = Vec::new();
    let to_master = |o: &mut Output| -> Option<Output> {
        (*o == Output::Bus { bus_id: id }).then(|| std::mem::replace(o, Output::Master))
    };
    let unkey = |c: &mut EffectChain| -> Option<EffectChain> {
        let old = c.clone();
        let mut hit = false;
        for slot in &mut c.slots {
            if slot.sidechain == Some(id) {
                slot.sidechain = None;
                hit = true;
            }
        }
        hit.then_some(old)
    };
    for t in &mut s.tracks {
        if let Some(old) = to_master(&mut t.output) {
            undo.push(Op::SetTrack {
                id: t.id,
                changes: TrackChanges {
                    output: Some(old),
                    ..TrackChanges::default()
                },
            });
        }
        if t.sends.iter().any(|x| x.bus_id == id) {
            let old = t.sends.clone();
            t.sends.retain(|x| x.bus_id != id);
            undo.push(Op::SetSends {
                owner: t.id,
                sends: old,
            });
        }
        if let Some(old) = unkey(&mut t.effects) {
            undo.push(Op::SetEffects {
                target: EffectTarget::Track(t.id),
                chain: old,
            });
        }
    }
    let bus = s.buses.remove(index);
    for b in &mut s.buses {
        if let Some(old) = to_master(&mut b.output) {
            undo.push(Op::SetBus {
                id: b.id,
                changes: BusChanges {
                    output: Some(old),
                    ..BusChanges::default()
                },
            });
        }
        if b.sends.iter().any(|x| x.bus_id == id) {
            let old = b.sends.clone();
            b.sends.retain(|x| x.bus_id != id);
            undo.push(Op::SetSends {
                owner: b.id,
                sends: old,
            });
        }
        if let Some(old) = unkey(&mut b.effects) {
            undo.push(Op::SetEffects {
                target: EffectTarget::Bus(b.id),
                chain: old,
            });
        }
    }
    if let Some(old) = unkey(&mut s.master.effects) {
        undo.push(Op::SetEffects {
            target: EffectTarget::Master,
            chain: old,
        });
    }
    let restore = Op::RestoreBus {
        bus: Box::new(bus),
        index,
    };
    if undo.is_empty() {
        return Ok(restore);
    }
    undo.insert(0, restore);
    Ok(Op::Batch { ops: undo })
}

#[cfg(test)]
mod tests {
    use super::super::tests::roundtrip;
    use super::super::*;
    use crate::session::model::*;

    #[test]
    fn buses_route_send_key_and_undo_in_one_step() {
        let mut s = Session::new("Test", 48_000);
        let (voice, music, mix, verb) = (new_id(), new_id(), new_id(), new_id());
        for (i, id) in [voice, music].into_iter().enumerate() {
            roundtrip(
                &mut s,
                Op::AddTrack {
                    id,
                    name: "T".into(),
                    stereo: true,
                    index: i,
                    midi: false,
                },
            );
        }
        for (i, id) in [mix, verb].into_iter().enumerate() {
            roundtrip(
                &mut s,
                Op::AddBus {
                    id,
                    name: "B".into(),
                    stereo: true,
                    index: i,
                },
            );
        }
        let to = |bus_id| Some(Output::Bus { bus_id });
        roundtrip(
            &mut s,
            Op::SetTrack {
                id: voice,
                changes: TrackChanges {
                    output: to(mix),
                    ..TrackChanges::default()
                },
            },
        );
        roundtrip(
            &mut s,
            Op::SetBus {
                id: verb,
                changes: BusChanges {
                    output: to(mix),
                    volume_db: Some(-3.0),
                    ..BusChanges::default()
                },
            },
        );
        let send = Send {
            id: new_id(),
            bus_id: verb,
            level_db: 50.0,
            pan: 0.0,
            pre_fader: false,
            enabled: true,
        };
        roundtrip(
            &mut s,
            Op::SetSends {
                owner: voice,
                sends: vec![send.clone()],
            },
        );
        assert_eq!(s.tracks[0].sends[0].level_db, 12.0, "send level is clamped");
        // Music ducked by the voice.
        let mut duck = EffectChain::default();
        duck.slots.push(crate::session::model::EffectSlot {
            id: new_id(),
            effect: "altaudio.dynamics".into(),
            enabled: true,
            params: Default::default(),
            preset: None,
            sidechain: Some(voice),
            plugin_state: None,
            data: None,
            position: None,
            routing: None,
        });
        roundtrip(
            &mut s,
            Op::SetEffects {
                target: EffectTarget::Track(music),
                chain: duck.clone(),
            },
        );
        let mut keyed_by_mix = duck.clone();
        keyed_by_mix.slots[0].sidechain = Some(mix);
        roundtrip(
            &mut s,
            Op::SetEffects {
                target: EffectTarget::Master,
                chain: keyed_by_mix.clone(),
            },
        );

        roundtrip(
            &mut s,
            Op::SetEffects {
                target: EffectTarget::Bus(verb),
                chain: EffectChain::default(),
            },
        );

        // Loops are refused and leave the session as it was.
        let before = s.clone();
        let op = Op::SetEffects {
            target: EffectTarget::Bus(verb),
            chain: keyed_by_mix,
        };
        assert_eq!(
            apply(&mut s, &op),
            Err(OpError::RoutingLoop),
            "verb feeds mix"
        );
        let back = Op::SetBus {
            id: mix,
            changes: BusChanges {
                output: to(verb),
                ..BusChanges::default()
            },
        };
        assert_eq!(apply(&mut s, &back), Err(OpError::RoutingLoop));
        assert_eq!(s, before);
        let mut self_key = duck.clone();
        self_key.slots[0].sidechain = Some(music);
        let op = Op::SetEffects {
            target: EffectTarget::Track(music),
            chain: self_key,
        };
        assert_eq!(apply(&mut s, &op), Err(OpError::RoutingLoop));
        let mut unknown = duck;
        unknown.slots[0].sidechain = Some(new_id());
        let op = Op::SetEffects {
            target: EffectTarget::Track(music),
            chain: unknown,
        };
        assert!(matches!(apply(&mut s, &op), Err(OpError::NoKeySource(_))));
        let op = Op::SetSends {
            owner: voice,
            sends: vec![Send {
                bus_id: new_id(),
                ..send
            }],
        };
        assert!(matches!(apply(&mut s, &op), Err(OpError::NoBus(_))));
        assert_eq!(s, before);

        // Removing Mix sends its feeders to the master and unkeys the
        // master's effect; undo puts it all back.
        let inv = apply(&mut s, &Op::RemoveBus { id: mix }).unwrap();
        assert_eq!(s.tracks[0].output, Output::Master);
        assert_eq!(s.buses[0].output, Output::Master);
        assert_eq!(s.master.effects.slots[0].sidechain, None);
        apply(&mut s, &inv).unwrap();
        assert_eq!(s, before);
        roundtrip(&mut s, Op::RemoveBus { id: mix });
        roundtrip(&mut s, Op::MoveBus { id: verb, to: 0 });
        roundtrip(
            &mut s,
            Op::SetBus {
                id: verb,
                changes: BusChanges {
                    position: Some(1),
                    ..BusChanges::default()
                },
            },
        );
        assert_eq!(s.buses[0].position, Some(1));
        roundtrip(
            &mut s,
            Op::SetBus {
                id: verb,
                changes: BusChanges {
                    mono: Some(true),
                    ..BusChanges::default()
                },
            },
        );
        roundtrip(
            &mut s,
            Op::SetTrack {
                id: music,
                changes: TrackChanges {
                    mono: Some(true),
                    ..TrackChanges::default()
                },
            },
        );
        assert!(s.buses[0].mono && s.tracks[1].mono);
        // Removing Reverb takes the voice's send with it.
        roundtrip(&mut s, Op::RemoveBus { id: verb });
        assert!(s.tracks[0].sends.is_empty());
        // A removed track leaves its key in place, and the chain stays editable.
        apply(&mut s, &Op::RemoveTrack { id: voice }).unwrap();
        let chain = s.tracks[0].effects.clone();
        roundtrip(
            &mut s,
            Op::SetEffects {
                target: EffectTarget::Track(music),
                chain,
            },
        );
    }
}
