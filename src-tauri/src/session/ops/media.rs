// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

//! Media file edits: add, remove, and follow a file that changed on disk.

use super::{id_taken, Op, OpError};
use crate::session::model::{Id, Session};

/// Applies a media edit and returns its inverse.
pub(super) fn apply_one(s: &mut Session, op: &Op) -> Result<Op, OpError> {
    match op {
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
        _ => unreachable!("not a media edit"),
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{media, roundtrip};
    use super::super::*;
    use crate::session::model::*;

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
}
