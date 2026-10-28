// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

//! MIDI clip and tempo edits.

use super::{id_taken, track_index, Op, OpError};
use crate::session::model::{MidiClip, Session, TrackKind};

/// Applies a MIDI edit and returns its inverse.
pub(super) fn apply_one(s: &mut Session, op: &Op) -> Result<Op, OpError> {
    match op {
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
        Op::SetTempo { tempo } => {
            let old = std::mem::replace(&mut s.tempo, tempo.tidy());
            Ok(Op::SetTempo { tempo: old })
        }
        _ => unreachable!("not a MIDI edit"),
    }
}

/// A MIDI clip with every value in range: notes inside MIDI's limits,
/// positive lengths, sorted by start then pitch.
fn tidy_midi_clip(c: &MidiClip) -> MidiClip {
    let ok = |v: f64, lo: f64| if v.is_finite() { v.max(lo) } else { lo };
    let mut notes: Vec<_> = c
        .notes
        .iter()
        .map(|n| crate::session::model::Note {
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

#[cfg(test)]
mod tests {
    use super::super::tests::{media, roundtrip};
    use super::super::*;
    use crate::session::model::*;

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
                channel_mix: None,
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
            routing: None,
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
}
