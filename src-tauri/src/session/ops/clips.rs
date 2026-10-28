// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

//! Audio clip edits: add, put back, remove and change.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::envelopes::set_points;
use super::{id_taken, track_index, Op, OpError};
use crate::session::model::{
    AudioType, ChannelMix, Clip, ClipLoop, ClipStretch, EffectChain, EnvelopePoint, EnvelopeTarget,
    Fade, Id, InvertChannel, Session, TrackKind, STRETCH_MAX, STRETCH_MIN,
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
    /// Which of the file's channels the clip plays (one side of a stereo
    /// file, say). None plays them as usual.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel_mix: Option<ChannelMix>,
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
            channel_mix: self.channel_mix.clone(),
            reversed: false,
            inverted: false,
            invert_channel: None,
        }
    }
}

/// Which channels a clip's polarity flip covers, in an edit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InvertSide {
    Both,
    Left,
    Right,
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
    /// Plays the clip backwards, or forwards again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reversed: Option<bool>,
    /// Flips the clip's polarity, or back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inverted: Option<bool>,
    /// Which channel the flip applies to: "left", "right", or "both".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invert_channel: Option<InvertSide>,
    /// Where the clip sits in its track's stacking order after the change:
    /// 0 is the bottom; past the end puts it on top. Later clips play over
    /// earlier ones where they overlap.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<u32>,
}

/// Applies a clip edit and returns its inverse.
pub(super) fn apply_one(s: &mut Session, op: &Op) -> Result<Op, OpError> {
    match op {
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
        _ => unreachable!("not a clip edit"),
    }
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
                pitch: s.pitch.clamp(
                    crate::session::model::PITCH_MIN,
                    crate::session::model::PITCH_MAX,
                ),
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
    if let Some(v) = ch.reversed {
        inv.reversed = Some(std::mem::replace(&mut c.reversed, v));
    }
    if let Some(v) = ch.inverted {
        inv.inverted = Some(std::mem::replace(&mut c.inverted, v));
    }
    if let Some(side) = ch.invert_channel {
        let new = match side {
            InvertSide::Both => None,
            InvertSide::Left => Some(InvertChannel::Left),
            InvertSide::Right => Some(InvertChannel::Right),
        };
        inv.invert_channel = Some(match std::mem::replace(&mut c.invert_channel, new) {
            None => InvertSide::Both,
            Some(InvertChannel::Left) => InvertSide::Left,
            Some(InvertChannel::Right) => InvertSide::Right,
        });
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

#[cfg(test)]
mod tests {
    use super::super::tests::{media, roundtrip};
    use super::super::*;
    use crate::session::model::*;

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
            channel_mix: None,
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
                channel_mix: None,
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
                    channel_mix: None,
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
                            channel_mix: None,
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
                            channel_mix: None,
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
                            channel_mix: None,
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
}
