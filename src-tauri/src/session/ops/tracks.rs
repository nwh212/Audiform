// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

//! Track edits: add, put back, remove, move and change settings.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{check_output, id_taken, track_index, Op, OpError};
use crate::session::model::{
    ChannelLayout, EffectSlot, Output, Session, SpacePos, Track, TrackKind, TrackRole,
};

/// Track settings to change. Fields left out stay as they are.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TrackChanges {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub muted: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub solo: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record_armed: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume_db: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pan: Option<f64>,
    /// 3D Space sessions: where the track sits in the room.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub space: Option<SpacePos>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub polarity_inverted: Option<bool>,
    /// Folds the track to mono after its Voice Chain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mono: Option<bool>,
    /// Pre-renders the track's clips through its Audio FX, or stops.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prerender: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collapsed: Option<bool>,
    /// Shows or hides its automation lanes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show_automation: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hidden: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub monitor_input: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub monitor_dry: Option<bool>,
    /// Hardware input; `Some(None)` clears it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<Option<String>>,
    /// Standard or remote guest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<TrackRole>,
    /// The host profile applied; an empty id clears it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_profile_id: Option<String>,
    /// Where the track's sound goes after its fader: the master or a bus.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<Output>,
    /// MIDI tracks: the instrument plug-in that plays the notes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instrument: Option<Box<EffectSlot>>,
    /// MIDI tracks: takes the instrument out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clear_instrument: Option<bool>,
}

/// Applies a track edit and returns its inverse.
pub(super) fn apply_one(s: &mut Session, op: &Op) -> Result<Op, OpError> {
    match op {
        Op::AddTrack {
            id,
            name,
            stereo,
            index,
            midi,
        } => {
            if id_taken(s, *id) {
                return Err(OpError::DuplicateId(*id));
            }
            let layout = if *stereo {
                ChannelLayout::Stereo
            } else {
                ChannelLayout::Mono
            };
            let mut t = if *midi {
                Track::midi(name.clone())
            } else {
                Track::new(name.clone(), layout)
            };
            t.id = *id;
            s.tracks.insert((*index).min(s.tracks.len()), t);
            Ok(Op::RemoveTrack { id: *id })
        }
        Op::RestoreTrack { track, index } => {
            if id_taken(s, track.id) {
                return Err(OpError::DuplicateId(track.id));
            }
            s.tracks
                .insert((*index).min(s.tracks.len()), (**track).clone());
            Ok(Op::RemoveTrack { id: track.id })
        }
        Op::RemoveTrack { id } => {
            let i = track_index(s, *id)?;
            let track = s.tracks.remove(i);
            Ok(Op::RestoreTrack {
                track: Box::new(track),
                index: i,
            })
        }
        Op::MoveTrack { id, to } => {
            let from = track_index(s, *id)?;
            let t = s.tracks.remove(from);
            let to = (*to).min(s.tracks.len());
            s.tracks.insert(to, t);
            Ok(Op::MoveTrack { id: *id, to: from })
        }
        Op::SetTrack { id, changes } => {
            let i = track_index(s, *id)?;
            if let Some(o) = &changes.output {
                check_output(s, o)?;
            }
            let t = &mut s.tracks[i];
            let mut old = TrackChanges::default();
            if let Some(v) = &changes.name {
                old.name = Some(std::mem::replace(&mut t.name, v.clone()));
            }
            if let Some(v) = changes.muted {
                old.muted = Some(std::mem::replace(&mut t.muted, v));
            }
            if let Some(v) = changes.solo {
                old.solo = Some(std::mem::replace(&mut t.solo, v));
            }
            if let Some(v) = changes.record_armed {
                old.record_armed = Some(std::mem::replace(&mut t.record_armed, v));
            }
            if let Some(v) = changes.height {
                old.height = Some(std::mem::replace(&mut t.height, v.clamp(40, 600)));
            }
            if let Some(v) = &changes.color {
                old.color = Some(std::mem::replace(&mut t.color, v.clone()));
            }
            if let Some(v) = changes.volume_db {
                old.volume_db = Some(std::mem::replace(&mut t.volume_db, v));
            }
            if let Some(v) = changes.pan {
                old.pan = Some(std::mem::replace(&mut t.pan, v.clamp(-100.0, 100.0)));
            }
            if let Some(v) = changes.space {
                old.space = Some(t.space.replace(v.tidy()).unwrap_or_default());
            }
            if let Some(v) = changes.polarity_inverted {
                old.polarity_inverted = Some(std::mem::replace(&mut t.polarity_inverted, v));
            }
            if let Some(v) = changes.mono {
                old.mono = Some(std::mem::replace(&mut t.mono, v));
            }
            if let Some(v) = changes.prerender {
                old.prerender = Some(std::mem::replace(&mut t.prerender, v));
            }
            if let Some(v) = changes.collapsed {
                old.collapsed = Some(std::mem::replace(&mut t.collapsed, v));
            }
            if let Some(v) = changes.show_automation {
                old.show_automation = Some(std::mem::replace(&mut t.show_automation, v));
            }
            if let Some(v) = changes.hidden {
                old.hidden = Some(std::mem::replace(&mut t.hidden, v));
            }
            if let Some(v) = changes.monitor_input {
                old.monitor_input = Some(std::mem::replace(&mut t.monitor_input, v));
            }
            if let Some(v) = changes.monitor_dry {
                old.monitor_dry = Some(std::mem::replace(&mut t.monitor_dry, v));
            }
            if let Some(v) = &changes.input {
                old.input = Some(std::mem::replace(&mut t.input, v.clone()));
            }
            if let Some(v) = changes.role {
                old.role = Some(std::mem::replace(&mut t.role, v));
            }
            if let Some(v) = &changes.host_profile_id {
                let next = Some(v.clone()).filter(|v| !v.is_empty());
                old.host_profile_id =
                    Some(std::mem::replace(&mut t.host_profile_id, next).unwrap_or_default());
            }
            if let Some(v) = changes.output {
                old.output = Some(std::mem::replace(&mut t.output, v));
            }
            if changes.instrument.is_some() || changes.clear_instrument == Some(true) {
                if t.kind != TrackKind::Midi {
                    return Err(OpError::WrongTrackKind);
                }
                let new = if changes.clear_instrument == Some(true) {
                    None
                } else {
                    changes.instrument.as_deref().cloned()
                };
                match std::mem::replace(&mut t.instrument, new) {
                    Some(v) => old.instrument = Some(Box::new(v)),
                    None => old.clear_instrument = Some(true),
                }
            }
            Ok(Op::SetTrack {
                id: *id,
                changes: old,
            })
        }
        _ => unreachable!("not a track edit"),
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::roundtrip;
    use super::super::*;
    use crate::session::model::*;

    #[test]
    fn tracks_add_move_set_remove_and_undo() {
        let mut s = Session::new("Test", 48_000);
        let (a, b, c) = (new_id(), new_id(), new_id());
        for (i, id) in [a, b, c].into_iter().enumerate() {
            roundtrip(
                &mut s,
                Op::AddTrack {
                    id,
                    name: format!("T{i}"),
                    stereo: true,
                    index: i,
                    midi: false,
                },
            );
        }
        roundtrip(&mut s, Op::MoveTrack { id: c, to: 0 });
        assert_eq!(s.tracks.iter().map(|t| t.id).collect::<Vec<_>>(), [c, a, b]);
        roundtrip(
            &mut s,
            Op::SetTrack {
                id: a,
                changes: TrackChanges {
                    name: Some("Host".into()),
                    muted: Some(true),
                    ..Default::default()
                },
            },
        );
        assert_eq!(s.tracks[1].name, "Host");
        roundtrip(&mut s, Op::RemoveTrack { id: b });
        assert_eq!(s.tracks.len(), 2);
    }
}
