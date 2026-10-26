// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Structural checks on a loaded session.
//!
//! JSON Schema covers shape. These checks cover meaning: references
//! point at things that exist, ids are unique, and bus routing has no
//! loops. Loading reports issues instead of refusing the file, so the
//! user can still recover their work.

use std::collections::{HashMap, HashSet};

use super::model::{Id, Output, Session};

/// One problem found in a session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    /// Where the problem is, for example `tracks[2].clips[0]`.
    pub location: String,
    pub message: String,
}

impl Issue {
    fn new(location: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            location: location.into(),
            message: message.into(),
        }
    }
}

/// Checks a session and returns every issue found. Empty means valid.
pub fn validate(session: &Session) -> Vec<Issue> {
    let mut issues = Vec::new();

    if session.sample_rate == 0 {
        issues.push(Issue::new("sample_rate", "sample rate must be above 0"));
    }
    if !matches!(session.bit_depth, 16 | 24 | 32) {
        issues.push(Issue::new("bit_depth", "bit depth must be 16, 24, or 32"));
    }

    check_unique_ids(session, &mut issues);

    let media: HashSet<Id> = session.media.iter().map(|m| m.id).collect();
    let buses: HashSet<Id> = session.buses.iter().map(|b| b.id).collect();

    let check_output = |loc: String, output: &Output, issues: &mut Vec<Issue>| {
        if let Output::Bus { bus_id } = output {
            if !buses.contains(bus_id) {
                issues.push(Issue::new(loc, format!("outputs to missing bus {bus_id}")));
            }
        }
    };

    for (ti, track) in session.tracks.iter().enumerate() {
        check_output(format!("tracks[{ti}].output"), &track.output, &mut issues);
        for (si, send) in track.sends.iter().enumerate() {
            if !buses.contains(&send.bus_id) {
                issues.push(Issue::new(
                    format!("tracks[{ti}].sends[{si}]"),
                    format!("sends to missing bus {}", send.bus_id),
                ));
            }
        }
        for (ci, clip) in track.clips.iter().enumerate() {
            let loc = format!("tracks[{ti}].clips[{ci}]");
            if !media.contains(&clip.media_id) {
                issues.push(Issue::new(
                    &loc,
                    format!("uses missing media {}", clip.media_id),
                ));
            }
            if clip.source_out <= clip.source_in {
                issues.push(Issue::new(&loc, "source_out must be after source_in"));
            }
        }
    }

    for (bi, bus) in session.buses.iter().enumerate() {
        check_output(format!("buses[{bi}].output"), &bus.output, &mut issues);
    }

    check_bus_loops(session, &mut issues);

    for (mi, marker) in session.markers.iter().enumerate() {
        if marker.chapter.is_some() && marker.kind != super::model::MarkerKind::Chapter {
            issues.push(Issue::new(
                format!("markers[{mi}]"),
                "chapter data on a marker that is not a chapter",
            ));
        }
        if marker.contracted_duration.is_some() && marker.kind != super::model::MarkerKind::AdSlot {
            issues.push(Issue::new(
                format!("markers[{mi}]"),
                "contracted duration on a marker that is not an ad slot",
            ));
        }
    }

    issues
}

fn check_unique_ids(session: &Session, issues: &mut Vec<Issue>) {
    let mut seen: HashMap<Id, String> = HashMap::new();
    let mut record = |id: Id, loc: String, issues: &mut Vec<Issue>| {
        if let Some(first) = seen.get(&id) {
            issues.push(Issue::new(
                loc,
                format!("id {id} is already used by {first}"),
            ));
        } else {
            seen.insert(id, loc);
        }
    };

    for (i, m) in session.media.iter().enumerate() {
        record(m.id, format!("media[{i}]"), issues);
    }
    for (ti, t) in session.tracks.iter().enumerate() {
        record(t.id, format!("tracks[{ti}]"), issues);
        for (ci, c) in t.clips.iter().enumerate() {
            record(c.id, format!("tracks[{ti}].clips[{ci}]"), issues);
        }
    }
    for (bi, b) in session.buses.iter().enumerate() {
        record(b.id, format!("buses[{bi}]"), issues);
    }
    for (mi, m) in session.markers.iter().enumerate() {
        record(m.id, format!("markers[{mi}]"), issues);
    }
}

/// Finds buses whose output chain leads back to themselves.
fn check_bus_loops(session: &Session, issues: &mut Vec<Issue>) {
    let next: HashMap<Id, Id> = session
        .buses
        .iter()
        .filter_map(|b| match b.output {
            Output::Bus { bus_id } => Some((b.id, bus_id)),
            Output::Master => None,
        })
        .collect();

    for (bi, bus) in session.buses.iter().enumerate() {
        let mut visited = HashSet::new();
        let mut current = bus.id;
        while let Some(&to) = next.get(&current) {
            if to == bus.id {
                issues.push(Issue::new(
                    format!("buses[{bi}].output"),
                    "bus routing loops back to itself",
                ));
                break;
            }
            if !visited.insert(to) {
                // A loop that does not include this bus. It is reported
                // when its own buses are checked.
                break;
            }
            current = to;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::model::{new_id, Bus, ChannelLayout, Clip, Fade, MediaRef, Track};

    fn session_with_clip() -> Session {
        let mut s = Session::new("Test", 48_000);
        let media = MediaRef {
            id: new_id(),
            path: "C:/audio/intro.wav".into(),
            relative_path: None,
            sample_rate: 48_000,
            channels: 2,
            length: 480_000,
        };
        let mut track = Track::new("Host", ChannelLayout::Mono);
        track.clips.push(Clip {
            id: new_id(),
            name: "intro".into(),
            media_id: media.id,
            start: 0,
            source_in: 0,
            source_out: 480_000,
            gain_db: 0.0,
            muted: false,
            locked: false,
            color: None,
            fade_in: Fade::default(),
            fade_out: Fade::default(),
            envelopes: Vec::new(),
            effects: Default::default(),
            group: None,
            looped: None,
            stretch: None,
            audio_type: None,
        });
        s.media.push(media);
        s.tracks.push(track);
        s
    }

    #[test]
    fn clean_session_has_no_issues() {
        assert!(validate(&session_with_clip()).is_empty());
    }

    #[test]
    fn missing_media_is_reported() {
        let mut s = session_with_clip();
        s.media.clear();
        let issues = validate(&s);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].location, "tracks[0].clips[0]");
    }

    #[test]
    fn empty_source_range_is_reported() {
        let mut s = session_with_clip();
        s.tracks[0].clips[0].source_out = 0;
        assert_eq!(validate(&s).len(), 1);
    }

    #[test]
    fn duplicate_ids_are_reported() {
        let mut s = session_with_clip();
        let dup = s.tracks[0].clone();
        s.tracks.push(dup);
        // The track id and the clip id are both duplicated.
        assert_eq!(validate(&s).len(), 2);
    }

    #[test]
    fn missing_output_bus_is_reported() {
        let mut s = session_with_clip();
        s.tracks[0].output = Output::Bus { bus_id: new_id() };
        assert_eq!(validate(&s)[0].location, "tracks[0].output");
    }

    #[test]
    fn bus_loop_is_reported() {
        let mut s = Session::new("Loop", 48_000);
        let mut a = Bus::new("A", ChannelLayout::Stereo);
        let mut b = Bus::new("B", ChannelLayout::Stereo);
        a.output = Output::Bus { bus_id: b.id };
        b.output = Output::Bus { bus_id: a.id };
        s.buses = vec![a, b];
        let issues = validate(&s);
        assert_eq!(issues.len(), 2);
        assert!(issues.iter().all(|i| i.message.contains("loops")));
    }

    #[test]
    fn bus_chain_without_loop_is_fine() {
        let mut s = Session::new("Chain", 48_000);
        let a = Bus::new("A", ChannelLayout::Stereo);
        let mut b = Bus::new("B", ChannelLayout::Stereo);
        b.output = Output::Bus { bus_id: a.id };
        s.buses = vec![a, b];
        assert!(validate(&s).is_empty());
    }

    #[test]
    fn contracted_duration_only_on_ad_slots() {
        use crate::session::model::{Marker, MarkerKind};
        let mut s = Session::new("Ads", 48_000);
        s.markers.push(Marker {
            id: new_id(),
            name: "Cue".into(),
            kind: MarkerKind::Cue,
            position: 0,
            duration: 0,
            description: String::new(),
            chapter: None,
            contracted_duration: Some(48_000),
            color: None,
        });
        assert_eq!(validate(&s).len(), 1);
        s.markers[0].kind = MarkerKind::AdSlot;
        assert!(validate(&s).is_empty());
    }

    #[test]
    fn bad_bit_depth_is_reported() {
        let mut s = Session::new("Bits", 48_000);
        s.bit_depth = 12;
        assert_eq!(validate(&s)[0].location, "bit_depth");
    }
}
