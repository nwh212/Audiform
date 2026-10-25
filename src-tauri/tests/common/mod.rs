// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

//! Sample documents with every field filled, built from fixed ids and
//! dates so output is stable across runs.

#![allow(dead_code)]

use std::collections::BTreeMap;

use altered_audio_editor_lib::library::{
    Collection, Library, LibraryItem, LibraryItemKind, LibraryScope, Template,
};
use altered_audio_editor_lib::session::host_profile::{GateSettings, HostProfile};
use altered_audio_editor_lib::session::*;
use chrono::{DateTime, TimeZone, Utc};
use uuid::Uuid;

pub fn id(n: u128) -> Id {
    Uuid::from_u128(n)
}

pub fn date() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 11, 12, 0, 0).unwrap()
}

pub fn effect_chain() -> EffectChain {
    let mut params = BTreeMap::new();
    params.insert("threshold_db".to_string(), -20.0);
    params.insert("ratio".to_string(), 4.0);
    EffectChain {
        enabled: true,
        input_gain_db: -1.5,
        output_gain_db: 2.0,
        mix: 0.85,
        slots: vec![EffectSlot {
            id: id(900),
            effect: "altaudio.dynamics".into(),
            enabled: false,
            params,
            preset: Some("Voice leveler".into()),
            sidechain: None,
            plugin_state: None,
            data: None,
            position: None,
        }],
    }
}

pub fn envelope() -> Envelope {
    Envelope {
        id: id(800),
        target: EnvelopeTarget::EffectParam {
            slot_id: id(900),
            param: "threshold_db".into(),
        },
        enabled: true,
        points: vec![
            EnvelopePoint {
                position: 0,
                value: 0.0,
                curve: EnvelopeCurve::Linear,
            },
            EnvelopePoint {
                position: 48_000,
                value: -6.5,
                curve: EnvelopeCurve::Spline,
            },
            EnvelopePoint {
                position: 96_000,
                value: -6.5,
                curve: EnvelopeCurve::Hold,
            },
        ],
    }
}

pub fn clip() -> Clip {
    Clip {
        id: id(300),
        name: "Cold open".into(),
        media_id: id(100),
        start: 48_000,
        source_in: 44_100,
        source_out: 441_000,
        gain_db: -3.25,
        muted: false,
        locked: true,
        color: Some("#3d8f6a".into()),
        fade_in: Fade {
            length: 4_800,
            shape: FadeShape::Cosine,
            curve: 0.25,
        },
        fade_out: Fade {
            length: 9_600,
            shape: FadeShape::Logarithmic,
            curve: -0.5,
        },
        envelopes: vec![Envelope {
            id: id(801),
            target: EnvelopeTarget::Volume,
            enabled: true,
            points: vec![EnvelopePoint {
                position: 0,
                value: -3.0,
                curve: EnvelopeCurve::Linear,
            }],
        }],
        effects: EffectChain::default(),
        group: None,
        looped: None,
        stretch: None,
        audio_type: None,
    }
}

pub fn track() -> Track {
    Track {
        id: id(200),
        name: "Host".into(),
        kind: Default::default(),
        role: TrackRole::Standard,
        host_profile_id: Some("host".into()),
        channels: ChannelLayout::Mono,
        color: Some("#3f73b8".into()),
        height: 120,
        collapsed: false,
        hidden: false,
        volume_db: -2.0,
        pan: -15.0,
        muted: false,
        solo: true,
        polarity_inverted: false,
        mono: false,
        record_armed: true,
        monitor_input: false,
        input: Some("Mic 1".into()),
        output: Output::Bus { bus_id: id(400) },
        sends: vec![Send {
            id: id(700),
            bus_id: id(400),
            level_db: -12.0,
            pan: 0.0,
            pre_fader: true,
            enabled: false,
        }],
        effects: effect_chain(),
        clips: vec![clip()],
        instrument: None,
        midi_clips: Vec::new(),
        envelopes: vec![envelope()],
    }
}

pub fn bus() -> Bus {
    Bus {
        id: id(400),
        name: "Voices".into(),
        channels: ChannelLayout::Stereo,
        color: None,
        height: 80,
        volume_db: 1.5,
        pan: 0.0,
        muted: true,
        solo: false,
        output: Output::Master,
        sends: Vec::new(),
        effects: EffectChain::default(),
        envelopes: Vec::new(),
        position: None,
        mono: false,
    }
}

pub fn marker() -> Marker {
    Marker {
        id: id(500),
        name: "Segment 2".into(),
        kind: MarkerKind::Chapter,
        position: 1_440_000,
        duration: 0,
        description: "News roundup".into(),
        chapter: Some(ChapterInfo {
            url: Some("https://thegcrn.com".into()),
            image: Some("art/segment2.png".into()),
        }),
        contracted_duration: None,
        color: None,
    }
}

pub fn session() -> Session {
    Session {
        format: Session::FORMAT.into(),
        schema_version: Session::CURRENT_VERSION,
        id: id(1),
        name: "Show 412".into(),
        sample_rate: 48_000,
        bit_depth: 32,
        created: date(),
        modified: date(),
        tempo: Default::default(),
        media: vec![MediaRef {
            id: id(100),
            path: "C:/GCRN/Show/412/host.wav".into(),
            relative_path: Some("host.wav".into()),
            sample_rate: 44_100,
            channels: 1,
            length: 44_100 * 3_600,
        }],
        tracks: vec![track(), {
            let mut guest = track();
            guest.id = id(201);
            guest.name = "Guest".into();
            guest.role = TrackRole::Guest;
            guest.host_profile_id = Some("guest".into());
            guest.output = Output::Master;
            guest.sends = Vec::new();
            guest.clips = Vec::new();
            guest.envelopes = Vec::new();
            guest.effects = EffectChain::default();
            guest
        }],
        buses: vec![bus()],
        master: MasterTrack {
            channels: ChannelLayout::Stereo,
            height: 64,
            volume_db: -0.5,
            effects: effect_chain(),
            envelopes: Vec::new(),
        },
        markers: vec![
            marker(),
            Marker {
                id: id(501),
                name: "Ad read".into(),
                kind: MarkerKind::Subclip,
                position: 2_000_000,
                duration: 1_440_000,
                description: String::new(),
                chapter: None,
                contracted_duration: None,
                color: None,
            },
            Marker {
                id: id(502),
                name: "Sponsor read".into(),
                kind: MarkerKind::AdSlot,
                position: 3_000_000,
                duration: 2_880_000,
                description: String::new(),
                chapter: None,
                contracted_duration: Some(2_880_000),
                color: None,
            },
            Marker {
                id: id(503),
                name: "Best moment".into(),
                kind: MarkerKind::SocialClip,
                position: 4_000_000,
                duration: 1_440_000,
                description: String::new(),
                chapter: None,
                contracted_duration: None,
                color: None,
            },
        ],
        playlist: Vec::new(),
        view: SessionView {
            samples_per_pixel: 512.5,
            scroll: 96_000,
            scroll_y: 40,
            playhead: 123_456,
            selection: Some(TimeRange {
                start: 48_000,
                end: 96_000,
            }),
        },
        modes: SessionModes {
            podcast: true,
            voiceover: false,
        },
        libraries: vec!["C:/GCRN/Libraries/Promos".into()],
        template_id: Some(id(2)),
        metadata: EpisodeMetadata {
            show_id: Some("show".into()),
            title: Some("Episode 412".into()),
            show: Some("Show".into()),
            episode_number: Some(412),
            season_number: None,
            description: Some("This week on the show.".into()),
            artist: Some("The GCRN".into()),
            artwork: Some("art/412.png".into()),
            explicit: false,
        },
    }
}

pub fn library_item() -> LibraryItem {
    LibraryItem {
        id: id(1_100),
        name: "Guest promo".into(),
        kind: LibraryItemKind::Audio,
        file: "items/guest-promo.wav".into(),
        copied: true,
        original_path: Some("C:/Users/host/Downloads/guest-promo.wav".into()),
        sample_rate: 48_000,
        channels: 2,
        length: 1_440_000,
        tags: vec!["promo".into(), "guest".into()],
        notes: "Use before the second segment.".into(),
        color: Some("#b8693f".into()),
        added: date(),
    }
}

pub fn library() -> Library {
    Library {
        format: Library::FORMAT.into(),
        schema_version: Library::CURRENT_VERSION,
        id: id(1_000),
        name: "Network promos".into(),
        scope: LibraryScope::Network,
        created: date(),
        items: vec![library_item()],
        collections: vec![Collection {
            id: id(1_200),
            name: "Guest promos".into(),
            items: vec![id(1_100)],
            color: None,
        }],
        color: None,
    }
}

pub fn template() -> Template {
    let mut session = session();
    session.id = id(3);
    Template {
        format: Template::FORMAT.into(),
        schema_version: Template::CURRENT_VERSION,
        id: id(2),
        name: "Show episode".into(),
        description: "Two hosts, voice bus, intro on track 1.".into(),
        created: date(),
        session,
    }
}

pub fn workspace() -> Workspace {
    Workspace {
        format: Workspace::FORMAT.into(),
        schema_version: Workspace::CURRENT_VERSION,
        id: "default".into(),
        name: "Default".into(),
        built_in: true,
        layout: serde_json::json!({
            "grid": { "root": { "type": "branch", "data": [] } },
            "panels": {},
            "activeGroup": null
        }),
    }
}

pub fn keymap() -> Keymap {
    let mut bindings = std::collections::BTreeMap::new();
    bindings.insert("help.about".to_string(), vec!["F1".to_string()]);
    bindings.insert("edit.find".to_string(), vec![]);
    Keymap::new(bindings)
}

pub fn preferences() -> Preferences {
    let mut values = std::collections::BTreeMap::new();
    values.insert(
        "drive-manager.folder".to_string(),
        serde_json::json!("C:\\Shows\\Raw"),
    );
    values.insert(
        "drive-manager.audio-only".to_string(),
        serde_json::json!(true),
    );
    Preferences::new(values)
}

pub fn host_profile() -> HostProfile {
    HostProfile {
        format: HostProfile::FORMAT.into(),
        schema_version: HostProfile::CURRENT_VERSION,
        id: "host".into(),
        name: "Host".into(),
        person: Some("Pat".into()),
        role: TrackRole::Standard,
        gate: GateSettings {
            threshold_db: -14.0,
            attack_ms: 2.0,
            hold_ms: 50.0,
            release_ms: 200.0,
        },
        voice_chain_preset: Some("Podcast Voice".into()),
        notes: "Primary host.".into(),
    }
}
