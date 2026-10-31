// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

//! 3D Space sessions: multichannel clips that play each channel straight
//! from its own speaker.
//!
//! Before playback or export, such a clip leaves its track, and each of
//! its channels plays from a hidden copy of the track (same fader, mute,
//! solo, Audio FX, output and sends; no pan or room spot of its own) that
//! is pinned to one speaker. A channel whose speaker the layout lacks
//! (heights in a 5.1 session, say) plays from where that speaker would
//! stand. The copies exist only while playing or exporting; the session
//! itself never changes.
//!
//! Clips of more than two channels do this by default in 3D Space
//! sessions; a clip's Source Channels can turn it off (or on for any clip).

use std::borrow::Cow;

use crate::dsp::channels::labels;
use crate::dsp::space::speakers;
use crate::session::model::{
    ChannelLayout, ChannelMix, ChannelSend, Clip, EnvelopeTarget, Session, SpaceLayout, SpacePos,
    TrackKind,
};

/// True when `clip` (of a file with `n` channels) plays each channel from its speaker.
pub fn plays_to_speakers(clip: &Clip, n: usize) -> bool {
    match &clip.channel_mix {
        Some(m) => m.to_speakers,
        None => n > 2,
    }
}

/// Where a speaker named `name` stands in the room, from any layout that has it.
fn spot_of(name: &str) -> Option<[f32; 3]> {
    [
        SpaceLayout::Surround714,
        SpaceLayout::Surround51,
        SpaceLayout::Surround512,
    ]
    .iter()
    .flat_map(|l| speakers(*l))
    .find(|s| s.name == name)
    .and_then(|s| s.at)
}

/// Where one channel goes: a speaker of the layout, or a spot in the room.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
enum Target {
    Speaker(usize),
    Spot([f32; 3]),
}

fn target(layout: SpaceLayout, name: &str, k: usize) -> Option<Target> {
    let sp = speakers(layout);
    if let Some(i) = sp.iter().position(|s| s.name == name) {
        return Some(Target::Speaker(i));
    }
    if name == "LFE" {
        // No low-frequency speaker in this layout: left out, as in a fold-down.
        return None;
    }
    if let Some(at) = spot_of(name) {
        return Some(Target::Spot(at));
    }
    // A file with no known channel order: channel k on speaker k when there is one.
    (k < sp.len()).then_some(Target::Speaker(k))
}

/// The session as it plays: unchanged unless it is a 3D Space session with
/// clips that play to speakers.
pub fn expanded(s: &Session) -> Cow<'_, Session> {
    let Some(space) = s.space else {
        return Cow::Borrowed(s);
    };
    let channels = |c: &Clip| {
        s.media_by_id(c.media_id)
            .map_or(2, |m| usize::from(m.channels).max(1))
    };
    let any = s.tracks.iter().any(|t| {
        t.kind == TrackKind::Audio && t.clips.iter().any(|c| plays_to_speakers(c, channels(c)))
    });
    if !any {
        return Cow::Borrowed(s);
    }
    let mut out = s.clone();
    let mut extra = Vec::new();
    for t in &mut out.tracks {
        if t.kind != TrackKind::Audio {
            continue;
        }
        let (direct, keep): (Vec<Clip>, Vec<Clip>) = std::mem::take(&mut t.clips)
            .into_iter()
            .partition(|c| plays_to_speakers(c, channels(c)));
        t.clips = keep;
        if direct.is_empty() {
            continue;
        }
        // One hidden copy per target, holding each clip's channel for it.
        let mut groups: Vec<(Target, String, Vec<Clip>)> = Vec::new();
        for c in &direct {
            let n = channels(c);
            for (k, name) in labels(n).iter().enumerate() {
                let Some(to) = target(space.layout, name, k) else {
                    continue;
                };
                let mut one = c.clone();
                one.channel_mix = Some(ChannelMix {
                    sends: (0..n)
                        .map(|j| {
                            let g = if j == k { 1.0 } else { 0.0 };
                            ChannelSend { left: g, right: g }
                        })
                        .collect(),
                    to_speakers: false,
                });
                match groups.iter_mut().find(|g| g.0 == to) {
                    Some(g) => g.2.push(one),
                    None => groups.push((to, name.clone(), vec![one])),
                }
            }
        }
        for (i, (to, name, clips)) in groups.into_iter().enumerate() {
            let mut h = t.clone();
            // Stable, so live changes to the track reach its copies.
            h.id = uuid::Uuid::from_u128(t.id.as_u128() ^ (0x5eed_0000_u128 + i as u128 + 1));
            h.name = format!("{} {name}", t.name);
            h.channels = ChannelLayout::Mono;
            h.mono = false;
            h.pan = 0.0;
            h.clips = clips;
            h.envelopes.retain(|e| {
                !matches!(e.target, EnvelopeTarget::Pan | EnvelopeTarget::Space { .. })
            });
            match to {
                Target::Speaker(i) => {
                    h.speaker = Some(i);
                    h.space = None;
                }
                Target::Spot(at) => {
                    h.speaker = None;
                    h.space = Some(SpacePos {
                        x: f64::from(at[0]),
                        y: f64::from(at[1]),
                        z: f64::from(at[2]),
                        size: 0.0,
                        width: 0.0,
                        rotation: 0.0,
                        lfe: 0.0,
                        spread: 0.0,
                        spatialize: 0.0,
                        spatialize_mode: Default::default(),
                    });
                }
            }
            extra.push(h);
        }
    }
    out.tracks.extend(extra);
    Cow::Owned(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::model::{new_id, MediaRef, Space, SpaceListen, Track};

    fn session(layout: SpaceLayout, channels: u16, mix: Option<ChannelMix>) -> Session {
        let mut s = Session::new("s", 48_000);
        s.space = Some(Space {
            layout,
            listen: SpaceListen::Stereo,
        });
        let m = new_id();
        s.media.push(MediaRef {
            id: m,
            path: "f.wav".into(),
            relative_path: None,
            sample_rate: 48_000,
            channels,
            length: 48_000,
            linked: None,
        });
        let mut t = Track::new("Film", ChannelLayout::Stereo);
        let mut c = crate::session::ops::ClipSpec {
            id: new_id(),
            name: "film".into(),
            media_id: m,
            start: 0,
            source_in: 0,
            source_out: 48_000,
            channel_mix: None,
        }
        .build();
        c.channel_mix = mix;
        t.clips.push(c);
        s.tracks.push(t);
        s
    }

    #[test]
    fn a_surround_clip_feeds_each_speaker() {
        let s = session(SpaceLayout::Surround714, 6, None);
        let e = expanded(&s);
        assert_eq!(e.tracks.len(), 7, "the track plus one copy per channel");
        assert!(e.tracks[0].clips.is_empty());
        let pinned: Vec<Option<usize>> = e.tracks[1..].iter().map(|t| t.speaker).collect();
        // 7.1.4 order: L R C LFE Lrs Rrs Lss Rss...; a 5.1 file's Ls and Rs stand where no 7.1.4 speaker is named so.
        assert_eq!(&pinned[..4], &[Some(0), Some(1), Some(2), Some(3)]);
        assert!(e.tracks[5].speaker.is_none() && e.tracks[5].space.is_some());
    }

    #[test]
    fn heights_without_height_speakers_play_from_their_spot() {
        let s = session(SpaceLayout::Surround51, 12, None);
        let e = expanded(&s);
        let ltf = e.tracks.iter().find(|t| t.name.ends_with("Ltf")).unwrap();
        assert!(ltf.speaker.is_none());
        assert!(ltf.space.unwrap().z > 0.5);
    }

    #[test]
    fn stereo_and_opted_out_clips_stay_put() {
        let s = session(SpaceLayout::Surround51, 2, None);
        assert!(matches!(expanded(&s), Cow::Borrowed(_)));
        let off = crate::dsp::channels::default_mix(6);
        let s = session(SpaceLayout::Surround51, 6, Some(off));
        assert!(matches!(expanded(&s), Cow::Borrowed(_)));
        let mut plain = session(SpaceLayout::Surround51, 6, None);
        plain.space = None;
        assert!(matches!(expanded(&plain), Cow::Borrowed(_)));
    }
}
