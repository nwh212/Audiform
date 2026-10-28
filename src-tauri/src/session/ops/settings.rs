// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

//! Session-wide settings: sample rate, name and bit depth, master volume,
//! 3D Space layout, and view state.

use super::{Op, OpError};
use crate::session::model::{Session, SessionView};

/// Applies a session settings edit and returns its inverse.
pub(super) fn apply_one(s: &mut Session, op: &Op) -> Result<Op, OpError> {
    match op {
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
        Op::SetSpace { space } => {
            let old = std::mem::replace(&mut s.space, *space);
            Ok(Op::SetSpace { space: old })
        }
        _ => unreachable!("not a session settings edit"),
    }
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

#[cfg(test)]
mod tests {
    use super::super::tests::{media, roundtrip};
    use super::super::*;
    use crate::session::model::*;

    #[test]
    fn master_volume_undoes() {
        let mut s = Session::new("Test", 48_000);
        roundtrip(&mut s, Op::SetMasterVolume { volume_db: -3.0 });
        assert_eq!(s.master.volume_db, -3.0);
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
                            channel_mix: None,
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
}
