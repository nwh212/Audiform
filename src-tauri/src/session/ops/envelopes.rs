// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

//! Envelopes: clip volume, pan and effect points, and track and bus automation.

use uuid::Uuid;

use super::{Op, OpError};
use crate::session::model::{Clip, Envelope, EnvelopePoint, EnvelopeTarget, Id, Session};

/// Applies an envelope edit and returns its inverse.
pub(super) fn apply_one(s: &mut Session, op: &Op) -> Result<Op, OpError> {
    match op {
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
        _ => unreachable!("not an envelope edit"),
    }
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
pub(super) fn set_points(
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
            height: None,
        }),
    }
    old
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
        height: e.height.map(|h| h.clamp(24, 400)),
        ..e.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{media, roundtrip};
    use super::super::*;
    use crate::session::model::*;

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
                routing: None,
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
