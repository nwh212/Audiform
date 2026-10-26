// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Edits to the open session.
//!
//! The core owns the session. The interface sends an `Op`; the core
//! applies it and returns the op that reverses it, which the interface
//! keeps on its undo stack. Ids for new things come from the interface,
//! so redo recreates exactly the same tracks and clips.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use super::model::{
    AudioType, Bus, ChannelLayout, ChannelMix, Clip, ClipLoop, ClipStretch, EffectChain,
    EffectSlot, Envelope, EnvelopePoint, EnvelopeTarget, Fade, Id, Marker, MarkerKind, MediaRef,
    MidiClip, Output, Send, Session, SessionView, Space, SpacePos, Tempo, Track, TrackKind,
    TrackRole, STRETCH_MAX, STRETCH_MIN,
};

/// Points for one effect setting's envelope on a clip.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EffectPoints {
    pub slot_id: Id,
    pub param: String,
    pub points: Vec<EnvelopePoint>,
}

/// A clip to create.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ClipSpec {
    pub id: Id,
    pub name: String,
    pub media_id: Id,
    pub start: u64,
    pub source_in: u64,
    pub source_out: u64,
}

impl ClipSpec {
    pub fn build(&self) -> Clip {
        Clip {
            id: self.id,
            name: self.name.clone(),
            media_id: self.media_id,
            start: self.start,
            source_in: self.source_in,
            source_out: self.source_out,
            gain_db: 0.0,
            muted: false,
            locked: false,
            color: None,
            fade_in: Fade::default(),
            fade_out: Fade::default(),
            envelopes: Vec::new(),
            effects: EffectChain::default(),
            group: None,
            looped: None,
            stretch: None,
            audio_type: None,
            channel_mix: None,
        }
    }
}

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collapsed: Option<bool>,
    /// Shows or hides its automation lanes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show_automation: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hidden: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub monitor_input: Option<bool>,
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

/// Changes to one clip. Fields left out are unchanged. `track_id` moves
/// the clip to another track.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ClipChange {
    pub id: Id,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub track_id: Option<Id>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_in: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_out: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gain_db: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub muted: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locked: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fade_in: Option<Fade>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fade_out: Option<Fade>,
    /// `#rrggbb`, or an empty string for the track color.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Puts the clip in this group.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<Id>,
    /// Takes the clip out of its group.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ungroup: Option<bool>,
    /// Replaces the clip's volume envelope points (values in dB,
    /// positions relative to the clip start). Empty removes the envelope.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume: Option<Vec<EnvelopePoint>>,
    /// Replaces the clip's pan envelope points (-100 is left, 100 is
    /// right). Empty removes the envelope.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pan: Option<Vec<EnvelopePoint>>,
    /// Replaces the points of effect settings' envelopes (automation of
    /// the clip's Voice Chain). Empty points remove that envelope.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect_points: Option<Vec<EffectPoints>>,
    /// Loops the clip, or changes its loop length or phase.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub looped: Option<ClipLoop>,
    /// Stops looping: the clip plays its source region once.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unloop: Option<bool>,
    /// Stretches the clip, or changes its stretch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stretch: Option<ClipStretch>,
    /// Stops stretching: the clip plays at its natural length.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unstretch: Option<bool>,
    /// Sets what the clip holds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_type: Option<AudioType>,
    /// Clears the clip's audio type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clear_audio_type: Option<bool>,
    /// Sets the clip's Source Channels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel_mix: Option<ChannelMix>,
    /// Goes back to the default Source Channels for the file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clear_channel_mix: Option<bool>,
    /// Where the clip sits in its track's stacking order after the change:
    /// 0 is the bottom; past the end puts it on top. Later clips play over
    /// earlier ones where they overlap.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<u32>,
}

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

/// Whose Voice Chain an edit changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum EffectTarget {
    Track(Id),
    Clip(Id),
    Bus(Id),
    Master,
}

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

fn sort_markers(s: &mut Session) {
    s.markers.sort_by_key(|m| m.position);
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
            let had: Vec<Id> = super::routing::key_sources(slot).collect();
            let known = |src: Id| had.contains(&src) || nodes.contains(&src);
            if let Some(src) = super::routing::key_sources(chain).find(|src| !known(*src)) {
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
        Op::AddMedia { media } => {
            if id_taken(s, media.id) {
                return Err(OpError::DuplicateId(media.id));
            }
            s.media.push(media.clone());
            Ok(Op::RemoveMedia { id: media.id })
        }
        Op::RemoveMedia { id } => {
            let used = s
                .tracks
                .iter()
                .any(|t| t.clips.iter().any(|c| c.media_id == *id));
            if used {
                return Err(OpError::MediaInUse);
            }
            let i = s
                .media
                .iter()
                .position(|m| m.id == *id)
                .ok_or(OpError::NoMedia(*id))?;
            let media = s.media.remove(i);
            Ok(Op::AddMedia { media })
        }
        Op::SetMedia { media, clip_outs } => {
            let mi = s
                .media
                .iter()
                .position(|m| m.id == media.id)
                .ok_or(OpError::NoMedia(media.id))?;
            let old = s.media[mi].clone();
            let mut new_outs: Vec<(Id, u64)> = Vec::new();
            for t in &s.tracks {
                for c in t.clips.iter().filter(|c| c.media_id == media.id) {
                    let to = match clip_outs {
                        Some(list) => match list.iter().find(|(id, _)| *id == c.id) {
                            Some((_, v)) => *v,
                            None => continue,
                        },
                        None if c.looped.is_some() => continue,
                        None if media.length > old.length && c.source_out == old.length => {
                            media.length
                        }
                        None if media.length > 0 && c.source_out > media.length => media.length,
                        None => continue,
                    };
                    if to <= c.source_in || (media.length > 0 && to > media.length) {
                        return Err(OpError::EmptyClip);
                    }
                    if to != c.source_out {
                        new_outs.push((c.id, to));
                    }
                }
            }
            let mut old_outs = Vec::with_capacity(new_outs.len());
            for t in &mut s.tracks {
                for c in &mut t.clips {
                    if let Some((_, to)) = new_outs.iter().find(|(id, _)| *id == c.id) {
                        old_outs.push((c.id, c.source_out));
                        c.source_out = *to;
                    }
                }
            }
            s.media[mi] = media.clone();
            Ok(Op::SetMedia {
                media: old,
                clip_outs: Some(old_outs),
            })
        }
        Op::AddClips { track_id, clips } => {
            let ti = track_index(s, *track_id)?;
            if s.tracks[ti].kind != TrackKind::Audio {
                return Err(OpError::WrongTrackKind);
            }
            for c in clips {
                if id_taken(s, c.id) {
                    return Err(OpError::DuplicateId(c.id));
                }
                if s.media_by_id(c.media_id).is_none() {
                    return Err(OpError::NoMedia(c.media_id));
                }
                if c.source_out <= c.source_in {
                    return Err(OpError::EmptyClip);
                }
            }
            s.tracks[ti].clips.extend(clips.iter().map(ClipSpec::build));
            Ok(Op::RemoveClips {
                ids: clips.iter().map(|c| c.id).collect(),
            })
        }
        Op::RestoreClips { clips } => {
            for (track_id, c) in clips {
                track_index(s, *track_id)?;
                if id_taken(s, c.id) {
                    return Err(OpError::DuplicateId(c.id));
                }
            }
            for (track_id, c) in clips {
                let ti = track_index(s, *track_id)?;
                s.tracks[ti].clips.push(c.clone());
            }
            Ok(Op::RemoveClips {
                ids: clips.iter().map(|(_, c)| c.id).collect(),
            })
        }
        Op::RemoveClips { ids } => {
            for id in ids {
                if !s.tracks.iter().any(|t| t.clips.iter().any(|c| c.id == *id)) {
                    return Err(OpError::NoClip(*id));
                }
            }
            let mut removed = Vec::new();
            for t in &mut s.tracks {
                let (gone, keep): (Vec<_>, Vec<_>) =
                    t.clips.drain(..).partition(|c| ids.contains(&c.id));
                t.clips = keep;
                removed.extend(gone.into_iter().map(|c| (t.id, c)));
            }
            Ok(Op::RestoreClips { clips: removed })
        }
        Op::SetSampleRate { sample_rate, exact } => {
            if !(8_000..=384_000).contains(sample_rate) {
                return Err(OpError::BadSampleRate);
            }
            let from = s.sample_rate;
            let to = *sample_rate;
            let mut before = Vec::new();
            for_each_time(s, |v| before.push(*v));
            match exact {
                Some(values) => {
                    if values.len() != before.len() {
                        return Err(OpError::BadTiming);
                    }
                    let mut it = values.iter();
                    for_each_time(s, |v| *v = *it.next().expect("length checked"));
                }
                None => for_each_time(s, |v| *v = rescale(*v, from, to)),
            }
            let v = &mut s.view;
            v.scroll = rescale(v.scroll, from, to);
            v.playhead = rescale(v.playhead, from, to);
            if let Some(sel) = v.selection.as_mut() {
                sel.start = rescale(sel.start, from, to);
                sel.end = rescale(sel.end, from, to);
            }
            if from > 0 {
                v.samples_per_pixel *= f64::from(to) / f64::from(from);
            }
            s.sample_rate = to;
            Ok(Op::SetSampleRate {
                sample_rate: from,
                exact: Some(before),
            })
        }
        Op::SetClips { changes } => {
            // Work on a copy so a failure part way leaves nothing half done.
            let mut work = s.clone();
            let mut inverses = Vec::with_capacity(changes.len());
            for ch in changes {
                inverses.push(set_clip(&mut work, ch)?);
            }
            *s = work;
            inverses.reverse();
            Ok(Op::SetClips { changes: inverses })
        }
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
        Op::SetSessionInfo { name, bit_depth } => {
            if let Some(b) = bit_depth {
                if ![16, 24, 32].contains(b) {
                    return Err(OpError::BadBitDepth);
                }
            }
            let old_name = name
                .as_ref()
                .map(|n| std::mem::replace(&mut s.name, n.trim().to_string()));
            let old_bits = bit_depth.map(|b| std::mem::replace(&mut s.bit_depth, b));
            Ok(Op::SetSessionInfo {
                name: old_name,
                bit_depth: old_bits,
            })
        }
        Op::SetMasterVolume { volume_db } => {
            let old = std::mem::replace(&mut s.master.volume_db, volume_db.clamp(-96.0, 24.0));
            Ok(Op::SetMasterVolume { volume_db: old })
        }
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
        Op::SetMidiClips { track_id, clips } => {
            let i = track_index(s, *track_id)?;
            if s.tracks[i].kind != TrackKind::Midi {
                return Err(OpError::WrongTrackKind);
            }
            for c in clips {
                let ours = s.tracks[i].midi_clips.iter().any(|x| x.id == c.id);
                if !ours && id_taken(s, c.id) {
                    return Err(OpError::DuplicateId(c.id));
                }
            }
            let clips: Vec<MidiClip> = clips.iter().map(tidy_midi_clip).collect();
            let old = std::mem::replace(&mut s.tracks[i].midi_clips, clips);
            Ok(Op::SetMidiClips {
                track_id: *track_id,
                clips: old,
            })
        }
        Op::SetEnvelopes { owner, envelopes } => {
            let tidy: Vec<Envelope> = envelopes.iter().map(tidy_envelope).collect();
            let slot = if let Some(t) = s.tracks.iter_mut().find(|t| t.id == *owner) {
                &mut t.envelopes
            } else if let Some(b) = s.buses.iter_mut().find(|b| b.id == *owner) {
                &mut b.envelopes
            } else {
                return Err(OpError::NoTrack(*owner));
            };
            let old = std::mem::replace(slot, tidy);
            Ok(Op::SetEnvelopes {
                owner: *owner,
                envelopes: old,
            })
        }
        Op::SetSpace { space } => {
            let old = std::mem::replace(&mut s.space, *space);
            Ok(Op::SetSpace { space: old })
        }
        Op::SetTempo { tempo } => {
            let old = std::mem::replace(&mut s.tempo, tempo.tidy());
            Ok(Op::SetTempo { tempo: old })
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

/// A MIDI clip with every value in range: notes inside MIDI's limits,
/// positive lengths, sorted by start then pitch.
fn tidy_midi_clip(c: &MidiClip) -> MidiClip {
    let ok = |v: f64, lo: f64| if v.is_finite() { v.max(lo) } else { lo };
    let mut notes: Vec<_> = c
        .notes
        .iter()
        .map(|n| super::model::Note {
            pitch: n.pitch.min(127),
            velocity: n.velocity.clamp(1, 127),
            start: ok(n.start, 0.0),
            length: ok(n.length, 1.0 / 64.0),
            channel: n.channel.min(15),
            pan: if n.pan.is_finite() {
                n.pan.clamp(-1.0, 1.0)
            } else {
                0.0
            },
        })
        .collect();
    notes.sort_by(|a, b| a.start.total_cmp(&b.start).then(a.pitch.cmp(&b.pitch)));
    MidiClip {
        start: ok(c.start, 0.0),
        length: ok(c.length, 1.0 / 16.0),
        gain_db: if c.gain_db.is_finite() {
            c.gain_db.clamp(-96.0, 24.0)
        } else {
            0.0
        },
        notes,
        ..c.clone()
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

fn set_clip(s: &mut Session, ch: &ClipChange) -> Result<ClipChange, OpError> {
    let (ti, ci) = s
        .tracks
        .iter()
        .enumerate()
        .find_map(|(ti, t)| {
            t.clips
                .iter()
                .position(|c| c.id == ch.id)
                .map(|ci| (ti, ci))
        })
        .ok_or(OpError::NoClip(ch.id))?;
    let to_track = match ch.track_id {
        Some(id) => Some(track_index(s, id)?),
        None => None,
    };
    if to_track.is_some_and(|t| s.tracks[t].kind != TrackKind::Audio) {
        return Err(OpError::WrongTrackKind);
    }
    let moves = ch.start.is_some()
        || ch.source_in.is_some()
        || ch.source_out.is_some()
        || ch.looped.is_some()
        || ch.unloop == Some(true)
        || ch.stretch.is_some()
        || ch.unstretch == Some(true)
        || to_track.is_some_and(|t| t != ti);
    let unlocking = ch.locked == Some(false);
    if moves && s.tracks[ti].clips[ci].locked && !unlocking {
        return Err(OpError::ClipLocked);
    }

    let mut inv = ClipChange {
        id: ch.id,
        ..ClipChange::default()
    };
    let c = &mut s.tracks[ti].clips[ci];
    fn swap<T: Clone>(field: &mut T, new: &Option<T>, old: &mut Option<T>) {
        if let Some(v) = new {
            *old = Some(std::mem::replace(field, v.clone()));
        }
    }
    swap(&mut c.name, &ch.name, &mut inv.name);
    swap(&mut c.start, &ch.start, &mut inv.start);
    swap(&mut c.source_in, &ch.source_in, &mut inv.source_in);
    swap(&mut c.source_out, &ch.source_out, &mut inv.source_out);
    swap(&mut c.muted, &ch.muted, &mut inv.muted);
    swap(&mut c.locked, &ch.locked, &mut inv.locked);
    swap(&mut c.fade_in, &ch.fade_in, &mut inv.fade_in);
    swap(&mut c.fade_out, &ch.fade_out, &mut inv.fade_out);
    let gain = ch.gain_db.map(|g| g.clamp(-96.0, 24.0));
    swap(&mut c.gain_db, &gain, &mut inv.gain_db);
    if ch.group.is_some() || ch.ungroup == Some(true) {
        let new = if ch.ungroup == Some(true) {
            None
        } else {
            ch.group
        };
        match std::mem::replace(&mut c.group, new) {
            Some(old) => inv.group = Some(old),
            None => inv.ungroup = Some(true),
        }
    }
    if ch.looped.is_some() || ch.unloop == Some(true) {
        let new = if ch.unloop == Some(true) {
            None
        } else {
            ch.looped
        };
        match std::mem::replace(&mut c.looped, new) {
            Some(old) => inv.looped = Some(old),
            None => inv.unloop = Some(true),
        }
    }
    if ch.stretch.is_some() || ch.unstretch == Some(true) {
        let new = if ch.unstretch == Some(true) {
            None
        } else {
            ch.stretch.map(|s| ClipStretch {
                ratio: s.ratio.clamp(STRETCH_MIN, STRETCH_MAX),
                pitch: s
                    .pitch
                    .clamp(super::model::PITCH_MIN, super::model::PITCH_MAX),
                ..s
            })
        };
        match std::mem::replace(&mut c.stretch, new) {
            Some(old) => inv.stretch = Some(old),
            None => inv.unstretch = Some(true),
        }
    }
    if ch.audio_type.is_some() || ch.clear_audio_type == Some(true) {
        let new = if ch.clear_audio_type == Some(true) {
            None
        } else {
            ch.audio_type
        };
        match std::mem::replace(&mut c.audio_type, new) {
            Some(old) => inv.audio_type = Some(old),
            None => inv.clear_audio_type = Some(true),
        }
    }
    if ch.channel_mix.is_some() || ch.clear_channel_mix == Some(true) {
        let new = if ch.clear_channel_mix == Some(true) {
            None
        } else {
            ch.channel_mix.clone().map(|mut m| {
                m.sends.truncate(64);
                for x in &mut m.sends {
                    x.left = if x.left.is_finite() {
                        x.left.clamp(-1.0, 1.0)
                    } else {
                        0.0
                    };
                    x.right = if x.right.is_finite() {
                        x.right.clamp(-1.0, 1.0)
                    } else {
                        0.0
                    };
                }
                m
            })
        };
        match std::mem::replace(&mut c.channel_mix, new) {
            Some(old) => inv.channel_mix = Some(old),
            None => inv.clear_channel_mix = Some(true),
        }
    }
    if let Some(points) = &ch.volume {
        inv.volume = Some(set_points(c, EnvelopeTarget::Volume, points));
    }
    if let Some(points) = &ch.pan {
        inv.pan = Some(set_points(c, EnvelopeTarget::Pan, points));
    }
    if let Some(list) = &ch.effect_points {
        let mut old = Vec::with_capacity(list.len());
        for ep in list {
            let target = EnvelopeTarget::EffectParam {
                slot_id: ep.slot_id,
                param: ep.param.clone(),
            };
            old.push(EffectPoints {
                slot_id: ep.slot_id,
                param: ep.param.clone(),
                points: set_points(c, target, &ep.points),
            });
        }
        old.reverse();
        inv.effect_points = Some(old);
    }
    if let Some(col) = &ch.color {
        let new = if col.is_empty() {
            None
        } else {
            Some(col.clone())
        };
        let old = std::mem::replace(&mut c.color, new);
        inv.color = Some(old.unwrap_or_default());
    }

    if c.source_out <= c.source_in || c.looped.is_some_and(|l| l.length == 0) {
        return Err(OpError::EmptyClip);
    }
    if let Some(l) = &mut c.looped {
        // The phase always lands inside the region.
        l.phase %= c.source_out - c.source_in;
    }
    let media_id = c.media_id;
    let source_out = c.source_out;
    if let Some(m) = s.media_by_id(media_id) {
        if m.length > 0 && source_out > m.length {
            return Err(OpError::PastSourceEnd);
        }
    }

    let mut at = (ti, ci);
    if let Some(to) = to_track.filter(|t| *t != ti) {
        let clip = s.tracks[ti].clips.remove(ci);
        s.tracks[to].clips.push(clip);
        inv.track_id = Some(s.tracks[ti].id);
        // Undo puts it back where it was in the old track's order.
        inv.layer = Some(ci as u32);
        at = (to, s.tracks[to].clips.len() - 1);
    }
    if let Some(layer) = ch.layer {
        let (t, i) = at;
        let clip = s.tracks[t].clips.remove(i);
        let to = (layer as usize).min(s.tracks[t].clips.len());
        s.tracks[t].clips.insert(to, clip);
        if inv.layer.is_none() {
            inv.layer = Some(i as u32);
        }
    }
    Ok(inv)
}

/// Lowest and highest volume envelope values, in dB. The lowest is silence.
pub const VOLUME_DB_MIN: f64 = -60.0;
pub const VOLUME_DB_MAX: f64 = 12.0;

/// Lowest and highest clip pan envelope values: full left, full right.
pub const PAN_MIN: f64 = -100.0;
pub const PAN_MAX: f64 = 100.0;

/// The id a clip's envelope for `target` uses. Fixed per clip and
/// target, so removing and re-adding an envelope gives the same session
/// back.
pub fn envelope_id(clip_id: Id, target: &EnvelopeTarget) -> Id {
    match target {
        EnvelopeTarget::Pan => Uuid::from_u128(clip_id.as_u128() ^ 1),
        EnvelopeTarget::EffectParam { slot_id, param } => {
            let mut h: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
            for b in param.bytes() {
                h = (h ^ u128::from(b)).wrapping_mul(0x0000_0000_0100_0000_0000_0000_0000_013b);
            }
            Uuid::from_u128(clip_id.as_u128() ^ slot_id.as_u128() ^ h ^ 2)
        }
        _ => clip_id,
    }
}

/// Sets a clip's points for a volume, pan or effect envelope and returns the
/// old ones. Values are clamped to the target's range.
fn set_points(
    c: &mut Clip,
    target: EnvelopeTarget,
    points: &[EnvelopePoint],
) -> Vec<EnvelopePoint> {
    let (lo, hi) = match &target {
        EnvelopeTarget::Pan => (PAN_MIN, PAN_MAX),
        EnvelopeTarget::Volume => (VOLUME_DB_MIN, VOLUME_DB_MAX),
        // An effect setting keeps to that control's range.
        EnvelopeTarget::EffectParam { slot_id, param } => c
            .effects
            .slots
            .iter()
            .find(|s| s.id == *slot_id)
            .and_then(|s| crate::dsp::effects::info(&s.effect))
            .and_then(|i| i.params.into_iter().find(|p| p.key == param.as_str()))
            .map_or((f64::MIN, f64::MAX), |p| (p.min, p.max)),
        EnvelopeTarget::Space { param } => param.range(),
    };
    let i = c.envelopes.iter().position(|e| e.target == target);
    let old = i.map(|i| c.envelopes[i].points.clone()).unwrap_or_default();
    let mut new: Vec<EnvelopePoint> = points
        .iter()
        .map(|p| EnvelopePoint {
            value: p.value.clamp(lo, hi),
            ..*p
        })
        .collect();
    new.sort_by_key(|p| p.position);
    match (i, new.is_empty()) {
        (Some(i), true) => {
            c.envelopes.remove(i);
        }
        (Some(i), false) => c.envelopes[i].points = new,
        (None, true) => {}
        (None, false) => c.envelopes.push(Envelope {
            id: envelope_id(c.id, &target),
            target,
            enabled: true,
            points: new,
        }),
    }
    old
}

fn rescale(value: u64, from: u32, to: u32) -> u64 {
    if from == to || from == 0 {
        return value;
    }
    ((u128::from(value) * u128::from(to) + u128::from(from) / 2) / u128::from(from)) as u64
}

/// Visits every timeline value stored in session samples, in a fixed
/// order. Source offsets are in file samples and are not visited.
fn for_each_time(s: &mut Session, mut f: impl FnMut(&mut u64)) {
    fn envelopes(list: &mut [crate::session::model::Envelope], f: &mut impl FnMut(&mut u64)) {
        for e in list {
            for p in &mut e.points {
                f(&mut p.position);
            }
        }
    }
    for t in &mut s.tracks {
        for c in &mut t.clips {
            f(&mut c.start);
            f(&mut c.fade_in.length);
            f(&mut c.fade_out.length);
            envelopes(&mut c.envelopes, &mut f);
        }
        envelopes(&mut t.envelopes, &mut f);
    }
    for b in &mut s.buses {
        envelopes(&mut b.envelopes, &mut f);
    }
    envelopes(&mut s.master.envelopes, &mut f);
    for m in &mut s.markers {
        f(&mut m.position);
        f(&mut m.duration);
        if let Some(d) = m.contracted_duration.as_mut() {
            f(d);
        }
    }
}

/// View state is not an edit: no undo, no dirty flag.
pub fn set_view(s: &mut Session, view: SessionView) {
    s.view = view;
}

/// An automation envelope with its points in time order and its values
/// in range: volume -96 to +24 dB, pan -100 to 100.
fn tidy_envelope(e: &Envelope) -> Envelope {
    let (lo, hi) = match e.target {
        EnvelopeTarget::Volume => (-96.0, 24.0),
        EnvelopeTarget::Pan => (-100.0, 100.0),
        EnvelopeTarget::EffectParam { .. } => (f64::MIN, f64::MAX),
        EnvelopeTarget::Space { param } => param.range(),
    };
    let mut points: Vec<EnvelopePoint> = e
        .points
        .iter()
        .filter(|p| p.value.is_finite())
        .map(|p| EnvelopePoint {
            value: p.value.clamp(lo, hi),
            ..*p
        })
        .collect();
    points.sort_by_key(|p| p.position);
    Envelope {
        points,
        ..e.clone()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::model::new_id;

    fn media(id: Id) -> MediaRef {
        MediaRef {
            id,
            path: "C:/Shows/ep.wav".into(),
            relative_path: None,
            sample_rate: 48_000,
            channels: 2,
            length: 480_000,
        }
    }

    fn roundtrip(s: &mut Session, op: Op) {
        let before = s.clone();
        let inverse = apply(s, &op).unwrap();
        let after = s.clone();
        let redo = apply(s, &inverse).unwrap();
        assert_eq!(*s, before, "undo of {op:?}");
        apply(s, &redo).unwrap();
        assert_eq!(*s, after, "redo of {op:?}");
    }

    #[test]
    fn midi_tracks_clips_instrument_and_tempo() {
        use crate::session::model::{MidiClip, Note};
        let mut s = Session::new("Test", 48_000);
        let (midi, audio) = (new_id(), new_id());
        roundtrip(
            &mut s,
            Op::AddTrack {
                id: midi,
                name: "Keys".into(),
                stereo: true,
                index: 0,
                midi: true,
            },
        );
        roundtrip(
            &mut s,
            Op::AddTrack {
                id: audio,
                name: "Voice".into(),
                stereo: false,
                index: 1,
                midi: false,
            },
        );
        assert_eq!(s.tracks[0].kind, TrackKind::Midi);
        let clip = MidiClip {
            id: new_id(),
            name: "Chords".into(),
            start: 4.0,
            length: 4.0,
            muted: false,
            color: None,
            gain_db: 0.0,
            notes: vec![
                Note {
                    pitch: 67,
                    velocity: 0,
                    start: 1.0,
                    length: 0.5,
                    channel: 0,
                    pan: 0.0,
                },
                Note {
                    pitch: 200,
                    velocity: 200,
                    start: -1.0,
                    length: f64::NAN,
                    channel: 20,
                    pan: f32::NAN,
                },
            ],
        };
        roundtrip(
            &mut s,
            Op::SetMidiClips {
                track_id: midi,
                clips: vec![clip.clone()],
            },
        );
        let n = &s.tracks[0].midi_clips[0].notes;
        assert_eq!(
            (n[0].pitch, n[0].velocity, n[0].start),
            (127, 127, 0.0),
            "tidied and sorted"
        );
        assert!(n[0].length > 0.0);
        assert_eq!((n[1].pitch, n[1].velocity), (67, 1));
        // MIDI clips only on MIDI tracks, audio clips only on audio tracks.
        let wrong = Op::SetMidiClips {
            track_id: audio,
            clips: vec![],
        };
        assert_eq!(apply(&mut s, &wrong), Err(OpError::WrongTrackKind));
        let m = media(new_id());
        let mid = m.id;
        apply(&mut s, &Op::AddMedia { media: m }).unwrap();
        let wrong = Op::AddClips {
            track_id: midi,
            clips: vec![ClipSpec {
                id: new_id(),
                name: "a".into(),
                media_id: mid,
                start: 0,
                source_in: 0,
                source_out: 10,
            }],
        };
        assert_eq!(apply(&mut s, &wrong), Err(OpError::WrongTrackKind));
        // Instrument in and out.
        let inst = crate::session::model::EffectSlot {
            id: new_id(),
            effect: "vst3:00112233445566778899AABBCCDDEEFF".into(),
            enabled: true,
            params: Default::default(),
            preset: None,
            sidechain: None,
            plugin_state: None,
            data: None,
            position: None,
        };
        roundtrip(
            &mut s,
            Op::SetTrack {
                id: midi,
                changes: TrackChanges {
                    instrument: Some(Box::new(inst.clone())),
                    ..TrackChanges::default()
                },
            },
        );
        roundtrip(
            &mut s,
            Op::SetTrack {
                id: midi,
                changes: TrackChanges {
                    clear_instrument: Some(true),
                    ..TrackChanges::default()
                },
            },
        );
        let wrong = Op::SetTrack {
            id: audio,
            changes: TrackChanges {
                instrument: Some(Box::new(inst)),
                ..TrackChanges::default()
            },
        };
        assert_eq!(apply(&mut s, &wrong), Err(OpError::WrongTrackKind));
        // Tempo, clamped.
        roundtrip(
            &mut s,
            Op::SetTempo {
                tempo: Tempo {
                    bpm: 90.0,
                    beats_per_bar: 3,
                    ..Tempo::default()
                },
            },
        );
        apply(
            &mut s,
            &Op::SetTempo {
                tempo: Tempo {
                    bpm: 9_000.0,
                    beats_per_bar: 0,
                    beat_unit: 6,
                    changes: Vec::new(),
                },
            },
        )
        .unwrap();
        assert_eq!(
            (s.tempo.bpm, s.tempo.beats_per_bar, s.tempo.beat_unit),
            (400.0, 1, 8)
        );
        assert_eq!(s.tempo.samples(1.0, 48_000), 7_200);
    }

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

    #[test]
    fn clips_need_media_and_undo_cleanly() {
        let mut s = Session::new("Test", 48_000);
        let (t, m, c) = (new_id(), new_id(), new_id());
        apply(
            &mut s,
            &Op::AddTrack {
                id: t,
                name: "Host".into(),
                stereo: false,
                index: 0,
                midi: false,
            },
        )
        .unwrap();
        let spec = ClipSpec {
            id: c,
            name: "ep".into(),
            media_id: m,
            start: 0,
            source_in: 0,
            source_out: 480_000,
        };
        assert_eq!(
            apply(
                &mut s,
                &Op::AddClips {
                    track_id: t,
                    clips: vec![spec.clone()]
                }
            ),
            Err(OpError::NoMedia(m))
        );
        roundtrip(
            &mut s,
            Op::Batch {
                ops: vec![
                    Op::AddMedia { media: media(m) },
                    Op::AddClips {
                        track_id: t,
                        clips: vec![spec],
                    },
                ],
            },
        );
        assert_eq!(s.tracks[0].clips.len(), 1);
        assert_eq!(
            apply(&mut s, &Op::RemoveMedia { id: m }),
            Err(OpError::MediaInUse)
        );
        roundtrip(&mut s, Op::RemoveClips { ids: vec![c] });
        // Removing a track with clips brings the clips back on undo.
        roundtrip(&mut s, Op::RemoveTrack { id: t });
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
    fn master_volume_undoes() {
        let mut s = Session::new("Test", 48_000);
        roundtrip(&mut s, Op::SetMasterVolume { volume_db: -3.0 });
        assert_eq!(s.master.volume_db, -3.0);
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

    #[test]
    fn sample_rate_change_keeps_times_and_undoes_exactly() {
        let mut s = Session::new("Test", 48_000);
        let (t, m) = (new_id(), new_id());
        apply(
            &mut s,
            &Op::Batch {
                ops: vec![
                    Op::AddTrack {
                        id: t,
                        name: "Host".into(),
                        stereo: false,
                        index: 0,
                        midi: false,
                    },
                    Op::AddMedia { media: media(m) },
                    Op::AddClips {
                        track_id: t,
                        clips: vec![ClipSpec {
                            id: new_id(),
                            name: "a".into(),
                            media_id: m,
                            start: 48_001,
                            source_in: 10,
                            source_out: 4_800,
                        }],
                    },
                ],
            },
        )
        .unwrap();
        s.tracks[0].clips[0].fade_in.length = 4_800;
        s.view.playhead = 96_000;
        let before = s.clone();

        let undo = apply(
            &mut s,
            &Op::SetSampleRate {
                sample_rate: 44_100,
                exact: None,
            },
        )
        .unwrap();
        let c = &s.tracks[0].clips[0];
        assert_eq!(c.start, 44_101, "one second plus a sample, rounded");
        assert_eq!(c.fade_in.length, 4_410);
        assert_eq!(
            (c.source_in, c.source_out),
            (10, 4_800),
            "file samples stay"
        );
        assert_eq!(s.view.playhead, 88_200);

        apply(&mut s, &undo).unwrap();
        assert_eq!(s.tracks, before.tracks, "undo restores exact positions");
        assert_eq!(s.sample_rate, 48_000);
        assert_eq!(s.view.playhead, 96_000);
    }

    #[test]
    fn session_info_sets_name_and_bit_depth() {
        let mut s = Session::new("Test", 48_000);
        roundtrip(
            &mut s,
            Op::SetSessionInfo {
                name: Some("EP 441".into()),
                bit_depth: Some(24),
            },
        );
        assert_eq!((s.name.as_str(), s.bit_depth), ("EP 441", 24));
        assert_eq!(
            apply(
                &mut s,
                &Op::SetSessionInfo {
                    name: None,
                    bit_depth: Some(20)
                }
            ),
            Err(OpError::BadBitDepth)
        );
    }

    #[test]
    fn set_clips_moves_trims_and_respects_locks() {
        let mut s = Session::new("Test", 48_000);
        let (a, b, m, c) = (new_id(), new_id(), new_id(), new_id());
        let mut ops = vec![Op::AddMedia { media: media(m) }];
        for id in [a, b] {
            ops.push(Op::AddTrack {
                id,
                name: "T".into(),
                stereo: false,
                index: 0,
                midi: false,
            });
        }
        ops.push(Op::AddClips {
            track_id: a,
            clips: vec![ClipSpec {
                id: c,
                name: "c".into(),
                media_id: m,
                start: 0,
                source_in: 0,
                source_out: 48_000,
            }],
        });
        apply(&mut s, &Op::Batch { ops }).unwrap();

        roundtrip(
            &mut s,
            Op::SetClips {
                changes: vec![ClipChange {
                    id: c,
                    track_id: Some(b),
                    start: Some(1_000),
                    source_in: Some(100),
                    gain_db: Some(-3.0),
                    ..ClipChange::default()
                }],
            },
        );
        let t = s.tracks.iter().find(|t| t.id == b).unwrap();
        assert_eq!(t.clips[0].start, 1_000);
        assert_eq!(t.clips[0].source_in, 100);

        let past = Op::SetClips {
            changes: vec![ClipChange {
                id: c,
                source_out: Some(480_001),
                ..ClipChange::default()
            }],
        };
        assert_eq!(apply(&mut s, &past), Err(OpError::PastSourceEnd));
        let empty = Op::SetClips {
            changes: vec![ClipChange {
                id: c,
                source_out: Some(100),
                ..ClipChange::default()
            }],
        };
        assert_eq!(apply(&mut s, &empty), Err(OpError::EmptyClip));

        apply(
            &mut s,
            &Op::SetClips {
                changes: vec![ClipChange {
                    id: c,
                    locked: Some(true),
                    ..ClipChange::default()
                }],
            },
        )
        .unwrap();
        let mv = Op::SetClips {
            changes: vec![ClipChange {
                id: c,
                start: Some(5),
                ..ClipChange::default()
            }],
        };
        assert_eq!(apply(&mut s, &mv), Err(OpError::ClipLocked));
    }

    #[test]
    fn layer_moves_a_clip_in_the_stacking_order_and_undo_puts_it_back() {
        let mut s = Session::new("t", 48_000);
        let (t, m) = (new_id(), new_id());
        let ids = [new_id(), new_id(), new_id()];
        let mut ops = vec![
            Op::AddMedia { media: media(m) },
            Op::AddTrack {
                id: t,
                name: "T".into(),
                stereo: false,
                index: 0,
                midi: false,
            },
        ];
        ops.push(Op::AddClips {
            track_id: t,
            clips: ids
                .iter()
                .enumerate()
                .map(|(i, id)| ClipSpec {
                    id: *id,
                    name: "c".into(),
                    media_id: m,
                    start: i as u64 * 1000,
                    source_in: 0,
                    source_out: 10_000,
                })
                .collect(),
        });
        apply(&mut s, &Op::Batch { ops }).unwrap();
        let order = |s: &Session| s.tracks[0].clips.iter().map(|c| c.id).collect::<Vec<_>>();
        let before = order(&s);
        let inv = apply(
            &mut s,
            &Op::SetClips {
                changes: vec![ClipChange {
                    id: ids[0],
                    start: Some(5_000),
                    layer: Some(u32::MAX),
                    ..ClipChange::default()
                }],
            },
        )
        .unwrap();
        assert_eq!(order(&s), vec![ids[1], ids[2], ids[0]], "moved to the top");
        apply(&mut s, &inv).unwrap();
        assert_eq!(order(&s), before, "undo restores the order");
    }

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

    #[test]
    fn grouping_clips_undoes() {
        let mut s = Session::new("Test", 48_000);
        let (t, m, c, g) = (new_id(), new_id(), new_id(), new_id());
        apply(
            &mut s,
            &Op::Batch {
                ops: vec![
                    Op::AddMedia { media: media(m) },
                    Op::AddTrack {
                        id: t,
                        name: "A".into(),
                        stereo: false,
                        index: 0,
                        midi: false,
                    },
                    Op::AddClips {
                        track_id: t,
                        clips: vec![ClipSpec {
                            id: c,
                            name: "c".into(),
                            media_id: m,
                            start: 0,
                            source_in: 0,
                            source_out: 100,
                        }],
                    },
                ],
            },
        )
        .unwrap();
        roundtrip(
            &mut s,
            Op::SetClips {
                changes: vec![ClipChange {
                    id: c,
                    group: Some(g),
                    ..ClipChange::default()
                }],
            },
        );
        assert_eq!(s.tracks[0].clips[0].group, Some(g));
        roundtrip(
            &mut s,
            Op::SetClips {
                changes: vec![ClipChange {
                    id: c,
                    ungroup: Some(true),
                    ..ClipChange::default()
                }],
            },
        );
        assert_eq!(s.tracks[0].clips[0].group, None);
    }

    #[test]
    fn clip_volume_points_set_and_undo() {
        use crate::session::model::EnvelopeCurve;
        let mut s = Session::new("Test", 48_000);
        let (t, m, c) = (new_id(), new_id(), new_id());
        apply(
            &mut s,
            &Op::Batch {
                ops: vec![
                    Op::AddMedia { media: media(m) },
                    Op::AddTrack {
                        id: t,
                        name: "A".into(),
                        stereo: false,
                        index: 0,
                        midi: false,
                    },
                    Op::AddClips {
                        track_id: t,
                        clips: vec![ClipSpec {
                            id: c,
                            name: "c".into(),
                            media_id: m,
                            start: 0,
                            source_in: 0,
                            source_out: 48_000,
                        }],
                    },
                ],
            },
        )
        .unwrap();
        let pt = |position, value| EnvelopePoint {
            position,
            value,
            curve: EnvelopeCurve::Linear,
        };
        roundtrip(
            &mut s,
            Op::SetClips {
                changes: vec![ClipChange {
                    id: c,
                    volume: Some(vec![pt(24_000, -99.0), pt(0, 0.0)]),
                    ..ClipChange::default()
                }],
            },
        );
        let env = &s.tracks[0].clips[0].envelopes[0];
        assert_eq!(env.points[0].position, 0, "sorted by position");
        assert_eq!(env.points[1].value, VOLUME_DB_MIN, "clamped");
        roundtrip(
            &mut s,
            Op::SetClips {
                changes: vec![ClipChange {
                    id: c,
                    volume: Some(vec![]),
                    ..ClipChange::default()
                }],
            },
        );
        assert!(s.tracks[0].clips[0].envelopes.is_empty());

        // Pan sits beside volume, with its own id and range.
        roundtrip(
            &mut s,
            Op::SetClips {
                changes: vec![ClipChange {
                    id: c,
                    volume: Some(vec![pt(0, -6.0)]),
                    pan: Some(vec![pt(0, -250.0), pt(48_000, 40.0)]),
                    ..ClipChange::default()
                }],
            },
        );
        let envs = &s.tracks[0].clips[0].envelopes;
        assert_eq!(envs.len(), 2);
        let pan = envs
            .iter()
            .find(|e| e.target == EnvelopeTarget::Pan)
            .unwrap();
        assert_eq!(pan.points[0].value, PAN_MIN, "clamped to full left");
        assert_ne!(pan.id, c, "pan has its own id");
        assert_eq!(pan.id, envelope_id(c, &EnvelopeTarget::Pan));
    }

    #[test]
    fn loop_set_and_undo() {
        let mut s = Session::new("Test", 48_000);
        let (t, m, c) = (new_id(), new_id(), new_id());
        apply(
            &mut s,
            &Op::Batch {
                ops: vec![
                    Op::AddMedia { media: media(m) },
                    Op::AddTrack {
                        id: t,
                        name: "A".into(),
                        stereo: false,
                        index: 0,
                        midi: false,
                    },
                    Op::AddClips {
                        track_id: t,
                        clips: vec![ClipSpec {
                            id: c,
                            name: "c".into(),
                            media_id: m,
                            start: 0,
                            source_in: 0,
                            source_out: 48_000,
                        }],
                    },
                ],
            },
        )
        .unwrap();
        roundtrip(
            &mut s,
            Op::SetClips {
                changes: vec![ClipChange {
                    id: c,
                    looped: Some(ClipLoop {
                        length: 200_000,
                        phase: 50_000,
                    }),
                    ..ClipChange::default()
                }],
            },
        );
        let l = s.tracks[0].clips[0].looped.unwrap();
        assert_eq!(l.length, 200_000);
        assert_eq!(l.phase, 2_000, "phase wraps into the region");
        assert_eq!(
            s.tracks[0].clips[0].timeline_length(48_000, 48_000),
            200_000
        );
        roundtrip(
            &mut s,
            Op::SetClips {
                changes: vec![ClipChange {
                    id: c,
                    unloop: Some(true),
                    ..ClipChange::default()
                }],
            },
        );
        assert!(s.tracks[0].clips[0].looped.is_none());
        let zero = apply(
            &mut s,
            &Op::SetClips {
                changes: vec![ClipChange {
                    id: c,
                    looped: Some(ClipLoop {
                        length: 0,
                        phase: 0,
                    }),
                    ..ClipChange::default()
                }],
            },
        );
        assert!(zero.is_err(), "a loop needs a length");
    }

    #[test]
    fn stretch_set_clamped_and_undone() {
        let mut s = Session::new("Test", 48_000);
        let (t, m, c) = (new_id(), new_id(), new_id());
        apply(
            &mut s,
            &Op::Batch {
                ops: vec![
                    Op::AddMedia { media: media(m) },
                    Op::AddTrack {
                        id: t,
                        name: "A".into(),
                        stereo: false,
                        index: 0,
                        midi: false,
                    },
                    Op::AddClips {
                        track_id: t,
                        clips: vec![ClipSpec {
                            id: c,
                            name: "c".into(),
                            media_id: m,
                            start: 0,
                            source_in: 0,
                            source_out: 48_000,
                        }],
                    },
                ],
            },
        )
        .unwrap();
        roundtrip(
            &mut s,
            Op::SetClips {
                changes: vec![ClipChange {
                    id: c,
                    stretch: Some(ClipStretch {
                        ratio: 1.25,
                        keep_pitch: true,
                        ..ClipStretch::default()
                    }),
                    ..ClipChange::default()
                }],
            },
        );
        assert_eq!(s.tracks[0].clips[0].timeline_length(48_000, 48_000), 60_000);
        apply(
            &mut s,
            &Op::SetClips {
                changes: vec![ClipChange {
                    id: c,
                    stretch: Some(ClipStretch {
                        ratio: 99.0,
                        keep_pitch: false,
                        ..ClipStretch::default()
                    }),
                    ..ClipChange::default()
                }],
            },
        )
        .unwrap();
        assert_eq!(s.tracks[0].clips[0].stretch.unwrap().ratio, STRETCH_MAX);
        roundtrip(
            &mut s,
            Op::SetClips {
                changes: vec![ClipChange {
                    id: c,
                    unstretch: Some(true),
                    ..ClipChange::default()
                }],
            },
        );
        assert!(s.tracks[0].clips[0].stretch.is_none());
    }

    #[test]
    fn set_media_follows_a_longer_file_and_undoes() {
        let mut s = Session::new("Test", 48_000);
        let (t, m, whole, part) = (new_id(), new_id(), new_id(), new_id());
        let clip = |id, out| ClipSpec {
            id,
            name: "take".into(),
            media_id: m,
            start: 0,
            source_in: 0,
            source_out: out,
        };
        apply(
            &mut s,
            &Op::Batch {
                ops: vec![
                    Op::AddTrack {
                        id: t,
                        name: "Host".into(),
                        stereo: true,
                        index: 0,
                        midi: false,
                    },
                    Op::AddMedia { media: media(m) },
                    Op::AddClips {
                        track_id: t,
                        clips: vec![clip(whole, 480_000), clip(part, 1_000)],
                    },
                ],
            },
        )
        .unwrap();
        let longer = MediaRef {
            length: 960_000,
            ..media(m)
        };
        roundtrip(
            &mut s,
            Op::SetMedia {
                media: longer.clone(),
                clip_outs: None,
            },
        );
        apply(
            &mut s,
            &Op::SetMedia {
                media: longer,
                clip_outs: None,
            },
        )
        .unwrap();
        let outs: Vec<u64> = s.tracks[0].clips.iter().map(|c| c.source_out).collect();
        assert_eq!(outs, vec![960_000, 1_000], "the whole-file clip follows");
        roundtrip(
            &mut s,
            Op::SetMedia {
                media: MediaRef {
                    length: 2_000,
                    ..media(m)
                },
                clip_outs: None,
            },
        );
    }

    #[test]
    fn effect_envelopes_set_clamp_and_undo() {
        let mut s = Session::new("Test", 48_000);
        let (t, m, c) = (new_id(), new_id(), new_id());
        apply(
            &mut s,
            &Op::Batch {
                ops: vec![
                    Op::AddTrack {
                        id: t,
                        name: "Music".into(),
                        stereo: true,
                        index: 0,
                        midi: false,
                    },
                    Op::AddMedia { media: media(m) },
                    Op::AddClips {
                        track_id: t,
                        clips: vec![ClipSpec {
                            id: c,
                            name: "song".into(),
                            media_id: m,
                            start: 0,
                            source_in: 0,
                            source_out: 480_000,
                        }],
                    },
                ],
            },
        )
        .unwrap();
        let slot = new_id();
        s.tracks[0].clips[0]
            .effects
            .slots
            .push(crate::session::model::EffectSlot {
                id: slot,
                effect: "altaudio.phase_rotate".into(),
                enabled: true,
                params: Default::default(),
                preset: None,
                sidechain: None,
                plugin_state: None,
                data: None,
                position: None,
            });
        let pt = |position, value| EnvelopePoint {
            position,
            value,
            curve: Default::default(),
        };
        let change = |points| Op::SetClips {
            changes: vec![ClipChange {
                id: c,
                effect_points: Some(vec![EffectPoints {
                    slot_id: slot,
                    param: "angle".into(),
                    points,
                }]),
                ..ClipChange::default()
            }],
        };
        roundtrip(&mut s, change(vec![pt(0, 0.0), pt(48_000, 400.0)]));
        apply(&mut s, &change(vec![pt(0, 0.0), pt(48_000, 400.0)])).unwrap();
        let e = &s.tracks[0].clips[0].envelopes[0];
        assert_eq!(e.points[1].value, 180.0, "kept to the control's range");
        apply(&mut s, &change(vec![])).unwrap();
        assert!(s.tracks[0].clips[0].envelopes.is_empty());
    }
}
