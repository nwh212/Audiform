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
//!
//! A plug-in set to its own layout (5.1, 7.1.4) has to hear every channel
//! at once, so when the clip's or its track's Audio FX hold one, the
//! copies feed a hidden 3D Bus that runs those effects on the speakers
//! instead, with the track's fader, mute and sends. A 3D Bus goes to the
//! master, so this is done only for tracks that output to the master.

use std::borrow::Cow;

use crate::dsp::channels::labels;
use crate::dsp::space::speakers;
use crate::session::model::{
    Bus, ChannelLayout, ChannelMix, ChannelSend, Clip, EffectChain, Envelope, EnvelopeTarget, Id,
    Output, Session, SpaceLayout, SpacePos, Track, TrackKind,
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

/// True when a chain holds a working plug-in set to its own channel layout.
fn needs_all_channels(chain: &EffectChain) -> bool {
    chain.enabled
        && chain
            .slots
            .iter()
            .any(|s| s.enabled && s.routing.as_ref().is_some_and(|r| r.wide))
}

/// A stable id made from another, so live changes find the same copy.
fn derived(id: Id, salt: u128) -> Id {
    uuid::Uuid::from_u128(id.as_u128() ^ salt)
}

/// A track's automation lanes that belong on the bus in its place.
fn bus_lanes(t: &Track) -> Vec<Envelope> {
    t.envelopes
        .iter()
        .filter(|e| {
            matches!(
                e.target,
                EnvelopeTarget::Volume | EnvelopeTarget::EffectParam { .. }
            )
        })
        .cloned()
        .collect()
}

/// The hidden 3D Bus that stands in for track `t`'s fader, sends and
/// effects (`effects` and `lanes`, for its copies feeding it).
fn stand_in(t: &Track, id: Id, effects: EffectChain, lanes: Vec<Envelope>) -> Bus {
    let mut b = Bus::new(format!("{} (speakers)", t.name), ChannelLayout::Stereo);
    b.id = id;
    b.volume_db = t.volume_db;
    b.muted = t.muted;
    b.sends = t.sends.clone();
    b.effects = effects;
    b.envelopes = lanes;
    b.automation_mode = t.automation_mode;
    b.room = true;
    b
}

/// A clip's effects followed by its track's, as one chain for a clip's own
/// bus. The track's slots get their own ids there (a second instance), and
/// the track's gains fold into the chain's output gain.
fn clip_then_track(c: &Clip, t: &Track, salt: u128) -> (EffectChain, Vec<(Id, Id)>) {
    let mut chain = c.effects.clone();
    let mut map = Vec::new();
    if t.effects.enabled {
        for slot in &t.effects.slots {
            let mut s = slot.clone();
            s.id = derived(slot.id, salt);
            map.push((slot.id, s.id));
            chain.slots.push(s);
        }
        chain.output_gain_db += t.effects.input_gain_db + t.effects.output_gain_db;
    }
    (chain, map)
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
    let mut buses: Vec<Bus> = Vec::new();
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
        // A plug-in in its own layout needs every channel at once: a
        // hidden 3D Bus runs it instead (only toward the master, where a
        // 3D Bus goes).
        let to_master = matches!(t.output, Output::Master);
        let clip_wide = |c: &Clip| to_master && needs_all_channels(&c.effects);
        let track_bus = (to_master
            && (needs_all_channels(&t.effects) || direct.iter().any(clip_wide)))
        .then(|| {
            stand_in(
                t,
                derived(t.id, 0xb05_0000),
                t.effects.clone(),
                bus_lanes(t),
            )
        });
        // One hidden copy per target and destination, holding each clip's
        // channel for it.
        let mut groups: Vec<(Target, String, Option<Id>, Vec<Clip>)> = Vec::new();
        for c in &direct {
            let n = channels(c);
            let dest = if clip_wide(c) {
                let salt = 0xc0b5_0000_0000_u128 ^ c.id.as_u128().rotate_left(17);
                let (chain, map) = clip_then_track(c, t, salt);
                // The clip's lanes run on the timeline there, from where the clip starts.
                let mut lanes: Vec<Envelope> = c
                    .envelopes
                    .iter()
                    .filter(|e| matches!(e.target, EnvelopeTarget::EffectParam { .. }))
                    .cloned()
                    .map(|mut e| {
                        for p in &mut e.points {
                            p.position += c.start;
                        }
                        e
                    })
                    .collect();
                lanes.extend(bus_lanes(t).into_iter().map(|mut e| {
                    if let EnvelopeTarget::EffectParam { slot_id, .. } = &mut e.target {
                        if let Some((_, to)) = map.iter().find(|(from, _)| from == slot_id) {
                            *slot_id = *to;
                        }
                    }
                    e
                }));
                let b = stand_in(t, derived(c.id, 0xc0b5_0000), chain, lanes);
                let id = b.id;
                buses.push(b);
                Some(id)
            } else {
                track_bus.as_ref().map(|b| b.id)
            };
            for (k, name) in labels(n).iter().enumerate() {
                let Some(to) = target(space.layout, name, k) else {
                    continue;
                };
                let mut one = c.clone();
                // Each channel's copy has its own stable id, so live changes
                // (level, channel mix), which find clips by id, reach every
                // copy with its own channel and not the first copy's.
                one.id = uuid::Uuid::from_u128(c.id.as_u128() ^ (0xc1a0_0000_u128 + k as u128 + 1));
                if dest.is_some_and(|d| Some(d) != track_bus.as_ref().map(|b| b.id)) {
                    // Its effects (and their lanes) run on its own bus.
                    one.effects = EffectChain::default();
                    one.envelopes
                        .retain(|e| !matches!(e.target, EnvelopeTarget::EffectParam { .. }));
                }
                one.channel_mix = Some(ChannelMix {
                    sends: (0..n)
                        .map(|j| {
                            let g = if j == k { 1.0 } else { 0.0 };
                            ChannelSend { left: g, right: g }
                        })
                        .collect(),
                    to_speakers: false,
                });
                match groups.iter_mut().find(|g| g.0 == to && g.2 == dest) {
                    Some(g) => g.3.push(one),
                    None => groups.push((to, name.clone(), dest, vec![one])),
                }
            }
        }
        for (i, (to, name, dest, clips)) in groups.into_iter().enumerate() {
            let mut h = t.clone();
            // Stable, so live changes to the track reach its copies.
            h.id = uuid::Uuid::from_u128(t.id.as_u128() ^ (0x5eed_0000_u128 + i as u128 + 1));
            h.name = format!("{} {name}", t.name);
            if let Some(bus_id) = dest {
                // The bus has the fader, mute, sends and effects.
                h.output = Output::Bus { bus_id };
                h.volume_db = 0.0;
                h.muted = false;
                h.sends.clear();
                h.effects = EffectChain::default();
                h.envelopes.retain(|e| {
                    !matches!(
                        e.target,
                        EnvelopeTarget::Volume | EnvelopeTarget::EffectParam { .. }
                    )
                });
            }
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
        buses.extend(track_bus);
    }
    out.tracks.extend(extra);
    out.buses.extend(buses);
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

    #[test]
    fn each_channel_copy_has_its_own_clip_id() {
        let s = session(SpaceLayout::Surround51, 6, None);
        let e = expanded(&s);
        let ids: Vec<_> = e
            .tracks
            .iter()
            .flat_map(|t| t.clips.iter().map(|c| c.id))
            .collect();
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        assert!(ids.len() > 1);
        assert_eq!(ids.len(), unique.len(), "copies of one clip share an id");
        // And they stay the same from one call to the next.
        let again: Vec<_> = expanded(&s)
            .tracks
            .iter()
            .flat_map(|t| t.clips.iter().map(|c| c.id))
            .collect();
        assert_eq!(ids, again);
        // Live updates give each copy its own channel.
        let live = crate::engine::plan::clip_gains(&e);
        let mixes: std::collections::HashSet<String> =
            live.iter().map(|(_, _, m)| format!("{m:?}")).collect();
        assert!(mixes.len() > 1, "each copy keeps its own channel");
    }

    fn slot(effect: &str) -> crate::session::model::EffectSlot {
        crate::session::model::EffectSlot {
            id: new_id(),
            effect: effect.into(),
            enabled: true,
            params: Default::default(),
            preset: None,
            sidechain: None,
            plugin_state: None,
            data: None,
            position: None,
            routing: None,
        }
    }

    fn wide_plugin() -> crate::session::model::EffectSlot {
        let mut s = slot("vst3:0123");
        s.routing = Some(crate::session::model::SlotRouting {
            wide: true,
            ..Default::default()
        });
        s
    }

    #[test]
    fn a_surround_plugin_on_the_track_runs_on_a_hidden_3d_bus() {
        let mut s = session(SpaceLayout::Surround51, 6, None);
        s.tracks[0].volume_db = -6.0;
        s.tracks[0].effects.slots.push(wide_plugin());
        let e = expanded(&s);
        assert_eq!(e.buses.len(), 1);
        let b = &e.buses[0];
        assert!(b.room, "a 3D Bus, so its effects hear every speaker");
        assert_eq!(
            b.effects, s.tracks[0].effects,
            "the same slots, so its window still works"
        );
        assert_eq!(b.volume_db, -6.0, "the fader moves to the bus");
        for t in &e.tracks[1..] {
            assert_eq!(t.output, Output::Bus { bus_id: b.id });
            assert!(t.effects.slots.is_empty());
            assert_eq!(t.volume_db, 0.0);
        }
        // Stable from one call to the next, so live changes reach it.
        assert_eq!(expanded(&s).buses[0].id, b.id);
    }

    #[test]
    fn a_surround_plugin_on_the_clip_runs_on_its_own_bus_before_the_tracks() {
        let mut s = session(SpaceLayout::Surround51, 6, None);
        s.tracks[0].clips[0].effects.slots.push(wide_plugin());
        let gain = slot("gain");
        s.tracks[0].effects.slots.push(gain.clone());
        let e = expanded(&s);
        let clip_bus = e
            .buses
            .iter()
            .find(|b| b.effects.slots.len() == 2)
            .expect("the clip's bus");
        assert_eq!(
            clip_bus.effects.slots[0],
            s.tracks[0].clips[0].effects.slots[0]
        );
        assert_eq!(clip_bus.effects.slots[1].effect, "gain");
        assert_ne!(
            clip_bus.effects.slots[1].id, gain.id,
            "a second instance of the track's effect"
        );
        for t in &e.tracks[1..] {
            assert_eq!(
                t.output,
                Output::Bus {
                    bus_id: clip_bus.id
                }
            );
            assert!(t.clips.iter().all(|c| c.effects.slots.is_empty()));
        }
    }

    #[test]
    fn stereo_plugins_and_tracks_to_a_bus_keep_the_copies() {
        let mut s = session(SpaceLayout::Surround51, 6, None);
        s.tracks[0].effects.slots.push(slot("gain"));
        assert!(expanded(&s).buses.is_empty(), "nothing needs every channel");
        let mut s = session(SpaceLayout::Surround51, 6, None);
        s.tracks[0].effects.slots.push(wide_plugin());
        let bus = Bus::new("Reverb", ChannelLayout::Stereo);
        s.tracks[0].output = Output::Bus { bus_id: bus.id };
        s.buses.push(bus);
        assert_eq!(
            expanded(&s).buses.len(),
            1,
            "a 3D Bus can only go to the master"
        );
    }
}
