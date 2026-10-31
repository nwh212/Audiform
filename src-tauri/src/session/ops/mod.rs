// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

//! Edits to the open session.
//!
//! The core owns the session. The interface sends an `Op`; the core
//! applies it and returns the op that reverses it, which the interface
//! keeps on its undo stack. Ids for new things come from the interface,
//! so redo recreates exactly the same tracks and clips.
//!
//! Each area has its own file. This file holds the `Op` enum, the error
//! type, the shared lookups and the dispatch to each area.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::session::model::{
    Bus, Clip, EffectChain, Envelope, Id, Marker, MediaRef, MidiClip, Output, Send, Session, Space,
    Tempo, Track,
};

mod buses;
mod clips;
mod effects;
mod envelopes;
mod markers;
mod media;
mod midi;
mod settings;
mod tracks;

pub use buses::BusChanges;
pub use clips::{ClipChange, ClipSpec, EffectPoints, InvertSide};
pub use effects::EffectTarget;
pub use envelopes::{envelope_id, PAN_MAX, PAN_MIN, VOLUME_DB_MAX, VOLUME_DB_MIN};
pub use markers::MarkerChange;
pub use settings::set_view;
pub use tracks::TrackChanges;

/// One edit. See the module docs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Op {
    /// Replaces a track's, clip's or the master's whole Voice Chain.
    SetEffects {
        target: EffectTarget,
        chain: EffectChain,
    },
    AddTrack {
        id: Id,
        name: String,
        stereo: bool,
        index: usize,
        /// A MIDI track instead of an audio track.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        midi: bool,
    },
    /// Puts back a removed track exactly as it was.
    RestoreTrack {
        track: Box<Track>,
        index: usize,
    },
    RemoveTrack {
        id: Id,
    },
    MoveTrack {
        id: Id,
        to: usize,
    },
    SetTrack {
        id: Id,
        changes: TrackChanges,
    },
    AddMedia {
        media: MediaRef,
    },
    RemoveMedia {
        id: Id,
    },
    /// Replaces a media reference after its file changed on disk (for
    /// example a Waveform editor save that made it longer). Without
    /// `clip_outs`, clips that ended at the old end follow the new end and
    /// clips past a shorter end are trimmed to it. With `clip_outs`, those
    /// clips get exactly these source ends (the inverse uses this).
    SetMedia {
        media: MediaRef,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        clip_outs: Option<Vec<(Id, u64)>>,
    },
    AddClips {
        track_id: Id,
        clips: Vec<ClipSpec>,
    },
    /// Puts back removed clips exactly as they were.
    RestoreClips {
        clips: Vec<(Id, Clip)>,
    },
    RemoveClips {
        ids: Vec<Id>,
    },
    /// Moves, trims, fades and other changes to existing clips.
    SetClips {
        changes: Vec<ClipChange>,
    },
    /// Adds markers, or puts removed ones back.
    AddMarkers {
        markers: Vec<Marker>,
    },
    RemoveMarkers {
        ids: Vec<Id>,
    },
    SetMarkers {
        changes: Vec<MarkerChange>,
    },
    /// Replaces the Playlist panel's order.
    SetPlaylist {
        ids: Vec<Id>,
    },
    /// Changes the session rate. Every timeline position (clips, fades,
    /// envelopes, markers, view) is converted so it stays at the same
    /// time. `exact` is filled in by the inverse so undo restores the
    /// original sample values with no rounding drift.
    SetSampleRate {
        sample_rate: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        exact: Option<Vec<u64>>,
    },
    /// Session name and bit depth. Fields left out are unchanged.
    SetSessionInfo {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        bit_depth: Option<u16>,
    },
    /// Master output level in dB.
    SetMasterVolume {
        volume_db: f64,
    },
    /// Adds an empty bus at `index` in the bus list, routed to the master.
    AddBus {
        id: Id,
        name: String,
        stereo: bool,
        index: usize,
        /// Adds it as a 3D Bus.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        room: bool,
    },
    /// Puts back a removed bus exactly as it was. Routes that pointed at
    /// it are put back by the ops batched with this one.
    RestoreBus {
        bus: Box<Bus>,
        index: usize,
    },
    /// Removes a bus. Tracks and buses that output to it go to the
    /// master instead, sends to it are removed, and effects keyed by it
    /// listen to their own input again; undo puts all of that back.
    RemoveBus {
        id: Id,
    },
    MoveBus {
        id: Id,
        to: usize,
    },
    SetBus {
        id: Id,
        changes: BusChanges,
    },
    /// Replaces a MIDI track's clips (and their notes).
    SetMidiClips {
        track_id: Id,
        clips: Vec<MidiClip>,
    },
    /// The session's tempo and meter. MIDI clips and notes follow it.
    SetTempo {
        tempo: Tempo,
    },
    /// Replaces the automation envelopes of a track or bus (volume, pan
    /// and effect settings over the session's timeline).
    SetEnvelopes {
        owner: Id,
        envelopes: Vec<Envelope>,
    },
    /// Makes the session a 3D Space session with this speaker layout, or
    /// an ordinary one again (None). Track and bus spots are kept.
    SetSpace {
        space: Option<Space>,
    },
    /// Replaces the aux sends of a track or bus.
    SetSends {
        owner: Id,
        sends: Vec<Send>,
    },
    /// Several ops as one step. Applied in order, reversed in reverse.
    Batch {
        ops: Vec<Op>,
    },
}

#[derive(Debug, Error, PartialEq)]
pub enum OpError {
    #[error("No track with id {0}.")]
    NoTrack(Id),
    #[error("No clip with id {0}.")]
    NoClip(Id),
    #[error("No media file with id {0}.")]
    NoMedia(Id),
    #[error("Something with id {0} already exists.")]
    DuplicateId(Id),
    #[error("The media file is still used by a clip.")]
    MediaInUse,
    #[error("A clip must play at least one sample.")]
    EmptyClip,
    #[error("Sample rate must be between 8000 and 384000 Hz.")]
    BadSampleRate,
    #[error("Bit depth must be 16, 24 or 32.")]
    BadBitDepth,
    #[error("The session changed since this step was recorded.")]
    BadTiming,
    #[error("The clip is locked in place.")]
    ClipLocked,
    #[error("A clip cannot reach past the end of its file.")]
    PastSourceEnd,
    #[error("No marker with id {0}.")]
    NoMarker(Id),
    #[error("No bus with id {0}.")]
    NoBus(Id),
    #[error("No track or bus with id {0} to listen to.")]
    NoKeySource(Id),
    #[error("That routing would feed the sound back into itself.")]
    RoutingLoop,
    #[error("Audio clips go on audio tracks, MIDI clips on MIDI tracks.")]
    WrongTrackKind,
}

fn track_index(s: &Session, id: Id) -> Result<usize, OpError> {
    s.tracks
        .iter()
        .position(|t| t.id == id)
        .ok_or(OpError::NoTrack(id))
}

fn bus_index(s: &Session, id: Id) -> Result<usize, OpError> {
    s.buses
        .iter()
        .position(|b| b.id == id)
        .ok_or(OpError::NoBus(id))
}

fn check_output(s: &Session, output: &Output) -> Result<(), OpError> {
    match output {
        Output::Master => Ok(()),
        Output::Bus { bus_id } => bus_index(s, *bus_id).map(|_| ()),
    }
}

/// True when `op` can change who feeds whom.
fn changes_routing(op: &Op) -> bool {
    match op {
        Op::SetTrack { changes, .. } => changes.output.is_some(),
        Op::SetBus { changes, .. } => changes.output.is_some(),
        Op::SetSends { .. } | Op::SetEffects { .. } | Op::RestoreTrack { .. } => true,
        _ => false,
    }
}

fn id_taken(s: &Session, id: Id) -> bool {
    s.tracks.iter().any(|t| {
        t.id == id || t.clips.iter().any(|c| c.id == id) || t.midi_clips.iter().any(|c| c.id == id)
    }) || s.media.iter().any(|m| m.id == id)
        || s.buses.iter().any(|b| b.id == id)
        || s.markers.iter().any(|m| m.id == id)
}

/// Applies an op and returns its inverse. On error the session is unchanged.
pub fn apply(s: &mut Session, op: &Op) -> Result<Op, OpError> {
    let inverse = apply_one(s, op)?;
    if changes_routing(op) && super::routing::has_loop(s) {
        apply_one(s, &inverse).expect("an inverse always applies");
        return Err(OpError::RoutingLoop);
    }
    Ok(inverse)
}

fn apply_one(s: &mut Session, op: &Op) -> Result<Op, OpError> {
    match op {
        Op::SetEffects { .. } => effects::apply_one(s, op),
        Op::AddTrack { .. }
        | Op::RestoreTrack { .. }
        | Op::RemoveTrack { .. }
        | Op::MoveTrack { .. }
        | Op::SetTrack { .. } => tracks::apply_one(s, op),
        Op::AddMedia { .. } | Op::RemoveMedia { .. } | Op::SetMedia { .. } => {
            media::apply_one(s, op)
        }
        Op::AddClips { .. }
        | Op::RestoreClips { .. }
        | Op::RemoveClips { .. }
        | Op::SetClips { .. } => clips::apply_one(s, op),
        Op::AddMarkers { .. }
        | Op::RemoveMarkers { .. }
        | Op::SetMarkers { .. }
        | Op::SetPlaylist { .. } => markers::apply_one(s, op),
        Op::SetSampleRate { .. }
        | Op::SetSessionInfo { .. }
        | Op::SetMasterVolume { .. }
        | Op::SetSpace { .. } => settings::apply_one(s, op),
        Op::AddBus { .. }
        | Op::RestoreBus { .. }
        | Op::RemoveBus { .. }
        | Op::MoveBus { .. }
        | Op::SetBus { .. }
        | Op::SetSends { .. } => buses::apply_one(s, op),
        Op::SetMidiClips { .. } | Op::SetTempo { .. } => midi::apply_one(s, op),
        Op::SetEnvelopes { .. } => envelopes::apply_one(s, op),
        Op::Batch { ops } => {
            // Work on a copy so a failure part way leaves nothing half done.
            let mut work = s.clone();
            let mut inverses = Vec::with_capacity(ops.len());
            for op in ops {
                inverses.push(apply(&mut work, op)?);
            }
            *s = work;
            inverses.reverse();
            Ok(Op::Batch { ops: inverses })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::model::*;

    pub(super) fn media(id: Id) -> MediaRef {
        MediaRef {
            id,
            path: "C:/Shows/ep.wav".into(),
            relative_path: None,
            sample_rate: 48_000,
            channels: 2,
            length: 480_000,
            linked: None,
        }
    }

    pub(super) fn roundtrip(s: &mut Session, op: Op) {
        let before = s.clone();
        let inverse = apply(s, &op).unwrap();
        let after = s.clone();
        let redo = apply(s, &inverse).unwrap();
        assert_eq!(*s, before, "undo of {op:?}");
        apply(s, &redo).unwrap();
        assert_eq!(*s, after, "redo of {op:?}");
    }

    #[test]
    fn failed_batch_changes_nothing() {
        let mut s = Session::new("Test", 48_000);
        let before = s.clone();
        let err = apply(
            &mut s,
            &Op::Batch {
                ops: vec![
                    Op::AddTrack {
                        id: new_id(),
                        name: "ok".into(),
                        stereo: true,
                        index: 0,
                        midi: false,
                    },
                    Op::RemoveTrack { id: new_id() },
                ],
            },
        );
        assert!(err.is_err());
        assert_eq!(s, before);
    }

    #[test]
    fn duplicate_ids_are_refused() {
        let mut s = Session::new("Test", 48_000);
        let id = new_id();
        let add = Op::AddTrack {
            id,
            name: "a".into(),
            stereo: true,
            index: 0,
            midi: false,
        };
        apply(&mut s, &add).unwrap();
        assert_eq!(apply(&mut s, &add), Err(OpError::DuplicateId(id)));
    }

    #[test]
    fn ops_serialize_with_a_tag() {
        let op = Op::MoveTrack {
            id: new_id(),
            to: 2,
        };
        let json = serde_json::to_value(&op).unwrap();
        assert_eq!(json["op"], "move_track");
        let back: Op = serde_json::from_value(json).unwrap();
        assert_eq!(back, op);
    }
}
