// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

//! Marker and Playlist edits.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{id_taken, Op, OpError};
use crate::session::model::{Id, Marker, MarkerKind, Session};

/// Changes to one marker. Fields left out are unchanged.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MarkerChange {
    pub id: Id,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<MarkerKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// `#rrggbb`, or an empty string for the type's color.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Ad slots: the contracted length of the read, in session samples.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contracted_duration: Option<u64>,
    /// Removes the contracted length (when a marker stops being an ad slot).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clear_contracted: Option<bool>,
}

/// Applies a marker edit and returns its inverse.
pub(super) fn apply_one(s: &mut Session, op: &Op) -> Result<Op, OpError> {
    match op {
        Op::AddMarkers { markers } => {
            for m in markers {
                if id_taken(s, m.id) {
                    return Err(OpError::DuplicateId(m.id));
                }
            }
            s.markers.extend(markers.iter().cloned());
            sort_markers(s);
            Ok(Op::RemoveMarkers {
                ids: markers.iter().map(|m| m.id).collect(),
            })
        }
        Op::RemoveMarkers { ids } => {
            for id in ids {
                if !s.markers.iter().any(|m| m.id == *id) {
                    return Err(OpError::NoMarker(*id));
                }
            }
            let (gone, kept): (Vec<Marker>, Vec<Marker>) =
                s.markers.drain(..).partition(|m| ids.contains(&m.id));
            s.markers = kept;
            Ok(Op::AddMarkers { markers: gone })
        }
        Op::SetPlaylist { ids } => {
            let old = std::mem::replace(&mut s.playlist, ids.clone());
            Ok(Op::SetPlaylist { ids: old })
        }
        Op::SetMarkers { changes } => {
            for ch in changes {
                if !s.markers.iter().any(|m| m.id == ch.id) {
                    return Err(OpError::NoMarker(ch.id));
                }
            }
            let mut inverses = Vec::with_capacity(changes.len());
            for ch in changes {
                let m = s
                    .markers
                    .iter_mut()
                    .find(|m| m.id == ch.id)
                    .expect("checked above");
                let mut inv = MarkerChange {
                    id: ch.id,
                    ..MarkerChange::default()
                };
                if let Some(v) = &ch.name {
                    inv.name = Some(std::mem::replace(&mut m.name, v.clone()));
                }
                if let Some(v) = ch.kind {
                    inv.kind = Some(std::mem::replace(&mut m.kind, v));
                }
                if let Some(v) = ch.position {
                    inv.position = Some(std::mem::replace(&mut m.position, v));
                }
                if let Some(v) = ch.duration {
                    inv.duration = Some(std::mem::replace(&mut m.duration, v));
                }
                if let Some(v) = &ch.description {
                    inv.description = Some(std::mem::replace(&mut m.description, v.clone()));
                }
                if let Some(v) = &ch.color {
                    let new = if v.is_empty() { None } else { Some(v.clone()) };
                    inv.color = Some(std::mem::replace(&mut m.color, new).unwrap_or_default());
                }
                if ch.contracted_duration.is_some() || ch.clear_contracted == Some(true) {
                    let new = if ch.clear_contracted == Some(true) {
                        None
                    } else {
                        ch.contracted_duration
                    };
                    match std::mem::replace(&mut m.contracted_duration, new) {
                        Some(old) => inv.contracted_duration = Some(old),
                        None => inv.clear_contracted = Some(true),
                    }
                }
                inverses.push(inv);
            }
            sort_markers(s);
            inverses.reverse();
            Ok(Op::SetMarkers { changes: inverses })
        }
        _ => unreachable!("not a marker edit"),
    }
}

fn sort_markers(s: &mut Session) {
    s.markers.sort_by_key(|m| m.position);
}

#[cfg(test)]
mod tests {
    use super::super::tests::roundtrip;
    use super::super::*;
    use crate::session::model::*;

    #[test]
    fn markers_add_set_remove_and_undo() {
        let mut s = Session::new("Test", 48_000);
        let (a, b) = (new_id(), new_id());
        let mk = |id, position| Marker {
            id,
            name: "M".into(),
            kind: MarkerKind::Cue,
            position,
            duration: 0,
            description: String::new(),
            chapter: None,
            contracted_duration: None,
            color: None,
        };
        roundtrip(
            &mut s,
            Op::AddMarkers {
                markers: vec![mk(a, 500), mk(b, 100)],
            },
        );
        assert_eq!(s.markers[0].id, b, "kept in time order");
        roundtrip(
            &mut s,
            Op::SetMarkers {
                changes: vec![MarkerChange {
                    id: b,
                    position: Some(900),
                    name: Some("Ad".into()),
                    kind: Some(MarkerKind::AdSlot),
                    ..MarkerChange::default()
                }],
            },
        );
        assert_eq!(s.markers[1].name, "Ad");
        let before = s.clone();
        let inv = apply(&mut s, &Op::RemoveMarkers { ids: vec![a] }).unwrap();
        assert_eq!(s.markers.len(), 1);
        apply(&mut s, &inv).unwrap();
        assert_eq!(s, before);
    }
}
