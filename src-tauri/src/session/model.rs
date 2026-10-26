// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! The multitrack session model, saved as `.altaudio`.
//!
//! Units:
//! - Timeline positions and lengths are in samples at the session
//!   sample rate.
//! - Source offsets (`source_in`, `source_out`) are in samples at the
//!   source file's own rate, so a 44.1 kHz file in a 48 kHz session
//!   never loses precision.
//! - Levels are in dB. Pan runs from -100 (left) to 100 (right).
//!
//! Fields marked "Scaffolding (Pn)" are stored and round-tripped now
//! but have no behavior until that phase.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::document::Document;

/// Stable identifier for anything that can be referenced.
pub type Id = Uuid;

/// Creates a new random identifier.
pub fn new_id() -> Id {
    Uuid::new_v4()
}

/// A multitrack session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Session {
    /// Always `altaudio.session`.
    #[schemars(extend("const" = "altaudio.session"))]
    pub format: String,
    /// Version of this file format.
    pub schema_version: u32,

    pub id: Id,
    pub name: String,
    /// Session sample rate in Hz, for example 48000.
    pub sample_rate: u32,
    /// Mixing bit depth: 16, 24, or 32 (float).
    pub bit_depth: u16,
    pub created: DateTime<Utc>,
    pub modified: DateTime<Utc>,

    /// Audio files used by clips in this session.
    pub media: Vec<MediaRef>,
    /// Audio tracks, top to bottom.
    pub tracks: Vec<Track>,
    /// Bus tracks, top to bottom.
    pub buses: Vec<Bus>,
    pub master: MasterTrack,
    /// Tempo and meter. MIDI clips and notes are placed in beats, so they
    /// follow it. Files from before MIDI tracks get the default.
    #[serde(default, skip_serializing_if = "Tempo::is_default")]
    pub tempo: Tempo,
    /// 3D Space sessions: the speaker layout the mix is placed onto, each
    /// track and bus at a spot in the room instead of a pan. None for an
    /// ordinary stereo (or mono) session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub space: Option<Space>,
    pub markers: Vec<Marker>,
    /// Playlist panel: range markers in the order they play and export.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub playlist: Vec<Id>,

    /// Where the user left off in the editor.
    pub view: SessionView,
    /// Podcast mode and Voiceover mode toggles, saved per session.
    pub modes: SessionModes,

    /// Library folders attached to this session.
    pub libraries: Vec<PathBuf>,
    /// Template this session was created from, if any.
    pub template_id: Option<Id>,

    /// Scaffolding (P6, P7): episode metadata for show sync and export.
    pub metadata: EpisodeMetadata,
}

impl Document for Session {
    const FORMAT: &'static str = "altaudio.session";
    const CURRENT_VERSION: u32 = 1;
    const SCHEMA_FILE: &'static str = "session.schema.json";
}

impl Session {
    /// Creates an empty session at the given sample rate.
    pub fn new(name: impl Into<String>, sample_rate: u32) -> Self {
        let now = Utc::now();
        Self {
            format: Self::FORMAT.into(),
            schema_version: Self::CURRENT_VERSION,
            id: new_id(),
            name: name.into(),
            sample_rate,
            bit_depth: 32,
            created: now,
            modified: now,
            media: Vec::new(),
            tracks: Vec::new(),
            buses: Vec::new(),
            master: MasterTrack::default(),
            tempo: Tempo::default(),
            space: None,
            markers: Vec::new(),
            playlist: Vec::new(),
            view: SessionView::default(),
            modes: SessionModes::default(),
            libraries: Vec::new(),
            template_id: None,
            metadata: EpisodeMetadata::default(),
        }
    }

    /// Looks up a media file by id.
    pub fn media_by_id(&self, id: Id) -> Option<&MediaRef> {
        self.media.iter().find(|m| m.id == id)
    }
}

/// The session's tempo and meter: where it starts, and any changes later
/// in the song. MIDI clips, bars and beats, and the metronome follow it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Tempo {
    /// Beats (quarter notes) per minute.
    pub bpm: f64,
    /// Beats in a bar, the top of the time signature: 4 in 4/4, 6 in 6/8.
    pub beats_per_bar: u32,
    /// The note one beat is, the bottom of the time signature: 4 in 4/4,
    /// 8 in 6/8. Files from before time signature changes get 4.
    #[serde(default = "quarter", skip_serializing_if = "is_quarter")]
    pub beat_unit: u32,
    /// Tempo and time signature changes, in bar order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changes: Vec<TempoChange>,
}

fn quarter() -> u32 {
    4
}

fn is_quarter(v: &u32) -> bool {
    *v == 4
}

/// A tempo and time signature change at the start of a bar.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TempoChange {
    pub id: Id,
    /// Bars from the start of the session where it takes over: 1 is the
    /// second bar. Always 1 or more.
    pub bar: u32,
    pub bpm: f64,
    pub beats_per_bar: u32,
    #[serde(default = "quarter")]
    pub beat_unit: u32,
}

impl Default for Tempo {
    fn default() -> Self {
        Self {
            bpm: 120.0,
            beats_per_bar: 4,
            beat_unit: 4,
            changes: Vec::new(),
        }
    }
}

/// Lowest and highest tempos a session can have.
pub const BPM_MIN: f64 = 20.0;
pub const BPM_MAX: f64 = 400.0;

/// One stretch of the tempo map with a single tempo and time signature.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TempoSegment {
    /// Bars from the session start where it begins.
    pub bar: u32,
    /// Beats (quarter notes) from the session start where it begins.
    pub beat: f64,
    /// Seconds from the session start where it begins.
    pub seconds: f64,
    pub bpm: f64,
    pub beats_per_bar: u32,
    pub beat_unit: u32,
}

impl TempoSegment {
    /// Quarter notes in one bar.
    pub fn bar_beats(&self) -> f64 {
        f64::from(self.beats_per_bar) * 4.0 / f64::from(self.beat_unit)
    }

    /// Quarter notes in one beat of the time signature (0.5 in 6/8).
    pub fn unit_beats(&self) -> f64 {
        4.0 / f64::from(self.beat_unit)
    }

    fn seconds_at(&self, beats: f64) -> f64 {
        self.seconds + (beats - self.beat) * 60.0 / self.bpm
    }
}

/// A beat unit the time signature can use: a power of two from 1 to 32.
pub fn tidy_beat_unit(unit: u32) -> u32 {
    match unit {
        0..=1 => 1,
        u if u >= 32 => 32,
        u => {
            let up = u.next_power_of_two();
            if up - u <= u - up / 2 {
                up
            } else {
                up / 2
            }
        }
    }
}

fn tidy_bpm(bpm: f64) -> f64 {
    if bpm.is_finite() {
        bpm.clamp(BPM_MIN, BPM_MAX)
    } else {
        120.0
    }
}

impl Tempo {
    pub fn is_default(&self) -> bool {
        *self == Tempo::default()
    }

    /// The same map with every value in range and the changes in bar
    /// order, one per bar (the later one wins), none at the first bar.
    pub fn tidy(&self) -> Tempo {
        let mut changes: Vec<TempoChange> = self
            .changes
            .iter()
            .filter(|c| c.bar >= 1)
            .map(|c| TempoChange {
                id: c.id,
                bar: c.bar.min(100_000),
                bpm: tidy_bpm(c.bpm),
                beats_per_bar: c.beats_per_bar.clamp(1, 32),
                beat_unit: tidy_beat_unit(c.beat_unit),
            })
            .collect();
        changes.reverse();
        changes.sort_by_key(|c| c.bar);
        changes.dedup_by_key(|c| c.bar);
        Tempo {
            bpm: tidy_bpm(self.bpm),
            beats_per_bar: self.beats_per_bar.clamp(1, 32),
            beat_unit: tidy_beat_unit(self.beat_unit),
            changes,
        }
    }

    /// The stretches of the map, in order. Never empty.
    pub fn segments(&self) -> impl Iterator<Item = TempoSegment> + '_ {
        let first = TempoSegment {
            bar: 0,
            beat: 0.0,
            seconds: 0.0,
            bpm: tidy_bpm(self.bpm),
            beats_per_bar: self.beats_per_bar.max(1),
            beat_unit: self.beat_unit.max(1),
        };
        let mut prev: Option<TempoSegment> = None;
        let mut next = 0;
        std::iter::from_fn(move || {
            let Some(p) = prev else {
                prev = Some(first);
                return prev;
            };
            // Changes out of order (a hand-edited file) are passed over.
            while let Some(c) = self.changes.get(next) {
                next += 1;
                if c.bar <= p.bar {
                    continue;
                }
                let beat = p.beat + f64::from(c.bar - p.bar) * p.bar_beats();
                let seg = TempoSegment {
                    bar: c.bar,
                    beat,
                    seconds: p.seconds_at(beat),
                    bpm: tidy_bpm(c.bpm),
                    beats_per_bar: c.beats_per_bar.max(1),
                    beat_unit: c.beat_unit.max(1),
                };
                prev = Some(seg);
                return prev;
            }
            None
        })
    }

    /// The stretch playing at `beats` from the start.
    pub fn segment_at_beat(&self, beats: f64) -> TempoSegment {
        let mut seg = self.segments().next().expect("never empty");
        for s in self.segments().skip(1) {
            if s.beat > beats {
                break;
            }
            seg = s;
        }
        seg
    }

    /// The stretch playing `seconds` from the start.
    pub fn segment_at_seconds(&self, seconds: f64) -> TempoSegment {
        let mut seg = self.segments().next().expect("never empty");
        for s in self.segments().skip(1) {
            if s.seconds > seconds {
                break;
            }
            seg = s;
        }
        seg
    }

    /// Beats to seconds.
    pub fn seconds(&self, beats: f64) -> f64 {
        let beats = beats.max(0.0);
        self.segment_at_beat(beats).seconds_at(beats)
    }

    /// Beats to samples at `rate`.
    pub fn samples(&self, beats: f64, rate: u32) -> u64 {
        (self.seconds(beats) * f64::from(rate)).round() as u64
    }

    /// Samples at `rate` to beats.
    pub fn beats(&self, samples: u64, rate: u32) -> f64 {
        let t = samples as f64 / f64::from(rate.max(1));
        let s = self.segment_at_seconds(t);
        s.beat + (t - s.seconds) * s.bpm / 60.0
    }
}

/// What a track holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TrackKind {
    /// Audio clips.
    #[default]
    Audio,
    /// MIDI clips, played by the track's instrument.
    Midi,
}

impl TrackKind {
    pub fn is_audio(&self) -> bool {
        *self == TrackKind::Audio
    }
}

/// One note in a MIDI clip.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Note {
    /// MIDI note number, 0 to 127 (60 is middle C).
    pub pitch: u8,
    /// 1 to 127.
    pub velocity: u8,
    /// Beats from the clip's start.
    pub start: f64,
    /// Beats.
    pub length: f64,
    /// MIDI channel, 0 to 15 (shown as 1 to 16, and as the note's color).
    /// Instruments with several parts (a split keyboard, an organ's
    /// manuals) play each channel with its own part.
    #[serde(default, skip_serializing_if = "is_zero_u8")]
    pub channel: u8,
    /// The note's own pan, -1 (left) to 1 (right), sent to instruments
    /// that take per-note pan. 0 sends none.
    #[serde(default, skip_serializing_if = "is_zero_f32")]
    pub pan: f32,
}

fn is_zero_u8(v: &u8) -> bool {
    *v == 0
}

fn is_zero_f32(v: &f32) -> bool {
    *v == 0.0
}

/// A MIDI clip: notes, placed in beats so they follow the tempo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MidiClip {
    pub id: Id,
    pub name: String,
    /// Timeline position of the clip's left edge, in beats.
    pub start: f64,
    /// Beats. Notes past the end are not played.
    pub length: f64,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub muted: bool,
    /// `#rrggbb` or a palette token. None uses the track color.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Clip level in dB: notes play harder or softer by this much (their
    /// velocity is scaled), up to the hardest a note can play.
    #[serde(default, skip_serializing_if = "is_zero_db")]
    pub gain_db: f64,
    pub notes: Vec<Note>,
}

fn is_zero_db(v: &f64) -> bool {
    *v == 0.0
}

/// A source audio file referenced by clips.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MediaRef {
    pub id: Id,
    /// Absolute path when the session was last saved.
    pub path: PathBuf,
    /// Path relative to the session file. Tried first on load so a
    /// session folder can move between drives or machines.
    pub relative_path: Option<PathBuf>,
    pub sample_rate: u32,
    pub channels: u16,
    /// Length in samples at the file's own rate.
    pub length: u64,
}

/// Mono or stereo. More layouts can be added without a version bump.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChannelLayout {
    Mono,
    #[default]
    Stereo,
}

/// Where a track or bus sends its output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Output {
    #[default]
    Master,
    Bus {
        bus_id: Id,
    },
}

/// An aux send: a copy of a track's or bus's sound sent on to a bus at
/// its own level, for example to a shared reverb. A pre-fader send
/// ignores the sender's fader and pan but still follows its mute and solo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Send {
    pub id: Id,
    pub bus_id: Id,
    pub level_db: f64,
    pub pan: f64,
    pub pre_fader: bool,
    pub enabled: bool,
}

impl Session {
    /// Takes every Voice Chain off (tracks, clips, buses and the master),
    /// for measuring or saving the audio as it is.
    pub fn strip_effects(&mut self) {
        self.master.effects = EffectChain::default();
        for b in &mut self.buses {
            b.effects = EffectChain::default();
        }
        for t in &mut self.tracks {
            t.effects = EffectChain::default();
            for c in &mut t.clips {
                c.effects = EffectChain::default();
            }
        }
    }
}

/// What kind of voice a track carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TrackRole {
    #[default]
    Standard,
    /// Scaffolding (P5): remote guest. Gets a softer gate, heavier noise
    /// reduction, and a telephone-bandwidth EQ.
    Guest,
}

/// An audio track.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Track {
    pub id: Id,
    pub name: String,
    /// Audio or MIDI. Left out of the file for audio tracks.
    #[serde(default, skip_serializing_if = "TrackKind::is_audio")]
    pub kind: TrackKind,
    pub role: TrackRole,
    /// Scaffolding (P5): id of the host profile applied to this track,
    /// for example `host`.
    pub host_profile_id: Option<String>,
    pub channels: ChannelLayout,
    /// Track color as `#rrggbb`. None uses the default.
    pub color: Option<String>,
    /// Track height in the Editor, in pixels.
    pub height: u32,
    /// Folded down to its name row in the Editor. Left out of the file when off.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub collapsed: bool,
    /// Its automation lanes show under it in the Editor. Left out of the
    /// file when off.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub show_automation: bool,
    /// Hidden from the Editor and Mixer lists. The track still plays and
    /// mixes. Left out of the file when off.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hidden: bool,

    pub volume_db: f64,
    pub pan: f64,
    pub muted: bool,
    pub solo: bool,
    /// Flips the track's polarity (multiplies it by -1). Left out of the
    /// file when off.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub polarity_inverted: bool,
    /// Folds the track's left and right into one centered signal, after
    /// its Voice Chain and before its fader and pan. Left out of the
    /// file when off.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub mono: bool,
    /// 3D Space sessions: where the track sits in the room (in place of
    /// its pan). None sits at the front center.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub space: Option<SpacePos>,

    /// Scaffolding (P4): armed for recording.
    pub record_armed: bool,
    /// Scaffolding (P4): input monitoring on.
    pub monitor_input: bool,
    /// Scaffolding (P4): hardware input name.
    pub input: Option<String>,

    pub output: Output,
    /// Aux sends to buses.
    pub sends: Vec<Send>,
    pub effects: EffectChain,
    /// Clips on this track. Order does not matter; position does.
    pub clips: Vec<Clip>,
    /// MIDI tracks: the VST3 instrument that plays the notes. Its sound
    /// then goes through the track's Voice Chain like any track's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instrument: Option<EffectSlot>,
    /// MIDI tracks: their clips.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub midi_clips: Vec<MidiClip>,
    /// Track automation lanes.
    pub envelopes: Vec<Envelope>,
    /// Playback only, never saved: set on the hidden copies of a track
    /// that play one channel of a multichannel clip straight into one
    /// speaker of the 3D Space layout (its index). See `engine::speaker_feed`.
    #[serde(skip)]
    #[schemars(skip)]
    pub speaker: Option<usize>,
}

impl Track {
    /// Creates an empty track with default settings.
    pub fn new(name: impl Into<String>, channels: ChannelLayout) -> Self {
        Self {
            id: new_id(),
            name: name.into(),
            kind: TrackKind::Audio,
            role: TrackRole::Standard,
            host_profile_id: None,
            channels,
            color: None,
            height: 96,
            collapsed: false,
            show_automation: false,
            hidden: false,
            volume_db: 0.0,
            pan: 0.0,
            muted: false,
            solo: false,
            polarity_inverted: false,
            mono: false,
            space: None,
            record_armed: false,
            monitor_input: false,
            input: None,
            output: Output::Master,
            sends: Vec::new(),
            effects: EffectChain::default(),
            clips: Vec::new(),
            instrument: None,
            midi_clips: Vec::new(),
            envelopes: Vec::new(),
            speaker: None,
        }
    }

    /// An empty MIDI track.
    pub fn midi(name: impl Into<String>) -> Self {
        Self {
            kind: TrackKind::Midi,
            ..Self::new(name, ChannelLayout::Stereo)
        }
    }
}

/// A bus track. Tracks and other buses output or send to it; it runs
/// its own Voice Chain and fader on the sum, then outputs to the master
/// or another bus. A bus has no clips and no inputs of its own.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Bus {
    pub id: Id,
    pub name: String,
    pub channels: ChannelLayout,
    pub color: Option<String>,
    pub height: u32,
    pub volume_db: f64,
    pub pan: f64,
    pub muted: bool,
    pub solo: bool,
    pub output: Output,
    /// Aux sends to other buses.
    pub sends: Vec<Send>,
    pub effects: EffectChain,
    pub envelopes: Vec<Envelope>,
    /// Where the bus shows among the tracks in the Editor: below this many
    /// tracks (0 is above the first). None, or more than there are
    /// tracks, shows it below the last track. Display only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<u32>,
    /// Folds the bus's left and right into one centered signal, after its
    /// Voice Chain and before its fader and pan. Left out of the file when off.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub mono: bool,
    /// 3D Space sessions: where the bus sits in the room, when it feeds
    /// the master (in place of its pan).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub space: Option<SpacePos>,
}

impl Bus {
    /// Creates an empty bus routed to the master.
    pub fn new(name: impl Into<String>, channels: ChannelLayout) -> Self {
        Self {
            id: new_id(),
            name: name.into(),
            channels,
            color: None,
            height: 72,
            volume_db: 0.0,
            pan: 0.0,
            muted: false,
            solo: false,
            output: Output::Master,
            sends: Vec::new(),
            effects: EffectChain::default(),
            envelopes: Vec::new(),
            position: None,
            mono: false,
            space: None,
        }
    }
}

/// A speaker layout a 3D Space session can be placed onto. Spots are
/// kept in room terms, so any session plays and exports in any layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum SpaceLayout {
    #[serde(rename = "2.0")]
    Stereo20,
    #[serde(rename = "5.1")]
    Surround51,
    #[serde(rename = "7.1")]
    Surround71,
    #[serde(rename = "5.1.2")]
    Surround512,
    #[serde(rename = "7.1.2")]
    Surround712,
    #[serde(rename = "5.1.4")]
    Surround514,
    #[serde(rename = "7.1.4")]
    Surround714,
}

impl SpaceLayout {
    pub const ALL: [SpaceLayout; 7] = [
        SpaceLayout::Stereo20,
        SpaceLayout::Surround51,
        SpaceLayout::Surround71,
        SpaceLayout::Surround512,
        SpaceLayout::Surround712,
        SpaceLayout::Surround514,
        SpaceLayout::Surround714,
    ];
}

/// How a 3D Space session is heard while working.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SpaceListen {
    /// Each sound folded to stereo by where it sits left to right.
    #[default]
    Stereo,
    /// The layout's speakers heard as if around a head, for headphones.
    Binaural,
    /// Each speaker to its own output of the audio device, for a room
    /// with the layout's speakers (Settings > Audio Hardware maps them).
    Speakers,
}

/// What makes a session a 3D Space session.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Space {
    /// The layout it plays in and exports to unless an export picks another.
    pub layout: SpaceLayout,
    #[serde(default)]
    pub listen: SpaceListen,
}

/// A spot in the room. x runs left (-1) to right (1), y back (-1) to
/// front (1), z ear level (0) to ceiling (1), and below ear level down
/// to -1.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SpacePos {
    pub x: f64,
    pub y: f64,
    #[serde(default)]
    pub z: f64,
    /// 0 is a point; 1 fills the room.
    #[serde(default)]
    pub size: f64,
    /// Stereo sources: how far left and right of the spot their two
    /// sides sit, 0 (together) to 1 (a full room width apart each way).
    #[serde(default = "full_width")]
    pub width: f64,
    /// Stereo sources: the pair turned around its spot, in degrees,
    /// clockwise seen from above (90 puts the left side in front).
    #[serde(default, skip_serializing_if = "is_zero_f64")]
    pub rotation: f64,
    /// How much of it also feeds the low-frequency channel, 0 (none) to 1
    /// (full level), low-passed at 120 Hz.
    #[serde(default, skip_serializing_if = "is_zero_f64")]
    pub lfe: f64,
    /// How much of it is spread through the room as a diffuse field
    /// (decorrelated copies on every speaker in use), 0 (none) to 1 (all).
    #[serde(default, skip_serializing_if = "is_zero_f64")]
    pub spread: f64,
    /// Stereo sources: how much of the difference between its sides is
    /// sent, delayed and decorrelated, to the surround and height
    /// speakers, with what the sides share fed to the center, 0 to 1.
    #[serde(default, skip_serializing_if = "is_zero_f64")]
    pub spatialize: f64,
}

fn is_zero_f64(v: &f64) -> bool {
    *v == 0.0
}

fn full_width() -> f64 {
    1.0
}

impl Default for SpacePos {
    /// Front center, as wide as the front speakers.
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 1.0,
            z: 0.0,
            size: 0.0,
            width: 1.0,
            rotation: 0.0,
            lfe: 0.0,
            spread: 0.0,
            spatialize: 0.0,
        }
    }
}

impl SpacePos {
    /// The same spot with every value in range.
    pub fn tidy(&self) -> SpacePos {
        let c = |v: f64, lo: f64, hi: f64, d: f64| if v.is_finite() { v.clamp(lo, hi) } else { d };
        SpacePos {
            x: c(self.x, -1.0, 1.0, 0.0),
            y: c(self.y, -1.0, 1.0, 1.0),
            // Below 0 is below ear level: speaker layouts play it from their
            // ear-level speakers, having none lower.
            z: c(self.z, -1.0, 1.0, 0.0),
            size: c(self.size, 0.0, 1.0, 0.0),
            width: c(self.width, 0.0, 1.0, 1.0),
            rotation: c(self.rotation, -180.0, 180.0, 0.0),
            lfe: c(self.lfe, 0.0, 1.0, 0.0),
            spread: c(self.spread, 0.0, 1.0, 0.0),
            spatialize: c(self.spatialize, 0.0, 1.0, 0.0),
        }
    }
}

/// The master track.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MasterTrack {
    pub channels: ChannelLayout,
    pub height: u32,
    pub volume_db: f64,
    pub effects: EffectChain,
    pub envelopes: Vec<Envelope>,
}

impl Default for MasterTrack {
    fn default() -> Self {
        Self {
            channels: ChannelLayout::Stereo,
            height: 72,
            volume_db: 0.0,
            effects: EffectChain::default(),
            envelopes: Vec::new(),
        }
    }
}

/// A non-destructive reference to part of a media file, placed on a
/// track.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Clip {
    pub id: Id,
    pub name: String,
    /// The media file this clip plays.
    pub media_id: Id,
    /// Timeline position of the clip's left edge, in session samples.
    pub start: u64,
    /// First source sample played, at the source file's rate.
    pub source_in: u64,
    /// Source sample after the last one played (exclusive).
    pub source_out: u64,
    pub gain_db: f64,
    pub muted: bool,
    /// Locked in time: the clip cannot be moved.
    pub locked: bool,
    /// Clip color as `#rrggbb`. None uses the track color.
    pub color: Option<String>,
    pub fade_in: Fade,
    pub fade_out: Fade,
    /// Clip automation lanes.
    pub envelopes: Vec<Envelope>,
    /// Clip Voice Chain.
    pub effects: EffectChain,
    /// Clips with the same group id are selected and moved together.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<Id>,
    /// Loop Clip: the source region from `source_in` to `source_out`
    /// repeats to fill the clip. None plays the region once.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub looped: Option<ClipLoop>,
    /// Time stretch: the source region plays over `ratio` times its
    /// natural length. Ignored while the clip loops.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stretch: Option<ClipStretch>,
    /// What the clip is: dialogue, music, sound effects or ambience.
    /// Used to select clips by kind, and later by the Audio FX Panel.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_type: Option<AudioType>,
    /// Source Channels: how much of each of the file's channels goes to the
    /// left and right. None uses the default for the file's channels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel_mix: Option<ChannelMix>,
    /// Plays backwards, from a reversed copy in the cache (the file stays
    /// as it is). Its `source_in` and `source_out` then count from the
    /// file's end, as positions in that copy.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub reversed: bool,
    /// Plays with its polarity flipped.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub inverted: bool,
}

/// A clip's Source Channels: one entry per channel of its file, in file order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ChannelMix {
    pub sends: Vec<ChannelSend>,
    /// 3D Space sessions: each channel plays straight from its own speaker
    /// (by its name: L, R, C, LFE, Ls...) instead of through `sends`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub to_speakers: bool,
}

/// How much of one source channel goes to each side, from -1 to 1
/// (1 is all of it; negative flips its polarity).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ChannelSend {
    pub left: f32,
    pub right: f32,
}

/// What a clip holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AudioType {
    Dialogue,
    Music,
    Sfx,
    Ambience,
}

/// How a looped clip repeats its source region.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ClipLoop {
    /// Length of the clip on the timeline, in session samples.
    pub length: u64,
    /// Where in the region the clip's left edge falls, in source
    /// samples from `source_in`. Splits and left trims move it.
    pub phase: u64,
}

/// How a stretched (or pitch-shifted) clip plays.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ClipStretch {
    /// Timeline length over natural length: 1.25 plays 25 percent longer.
    pub ratio: f64,
    /// True keeps the pitch (for speech and ad fitting); false changes
    /// pitch with speed, like a tape running faster or slower (varispeed).
    pub keep_pitch: bool,
    /// Pitch change in semitones, -24 to +24, independent of the stretch.
    /// Only while the pitch is kept (varispeed sets pitch by speed).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub pitch: f64,
    /// For music and other material with several notes at once: longer
    /// analysis frames. False suits a single voice or instrument.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub polyphonic: bool,
    /// The user asked for the stretch to be worked out ahead (rendered)
    /// rather than live. Playback and export sound the same either way;
    /// kept so files from other editors round-trip.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub rendered: bool,
    /// Finer settings, when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub advanced: Option<StretchAdvanced>,
}

impl Default for ClipStretch {
    fn default() -> Self {
        Self {
            ratio: 1.0,
            keep_pitch: true,
            pitch: 0.0,
            polyphonic: false,
            rendered: false,
            advanced: None,
        }
    }
}

/// Finer stretch settings. The window size sets the length of the
/// analysis frames; the rest are kept for files that carry them.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StretchAdvanced {
    /// Analysis frame length in milliseconds (about 10 to 100).
    pub window_ms: f64,
    /// How readily a sharp attack starts a new frame, 0 to 100.
    pub transient_sensitivity: f64,
    /// Keep the voice's character when the pitch moves.
    pub preserve_formants: bool,
    /// Higher precision takes longer to work out.
    pub high_precision: bool,
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

/// Pitch changes a clip can take, in semitones.
pub const PITCH_MIN: f64 = -24.0;
pub const PITCH_MAX: f64 = 24.0;

/// Stretch ratios a clip can take.
pub const STRETCH_MIN: f64 = 0.25;
pub const STRETCH_MAX: f64 = 4.0;

impl Clip {
    /// The stretch ratio in effect: 1 when not stretched or when looping.
    pub fn stretch_ratio(&self) -> f64 {
        match (self.looped, self.stretch) {
            (None, Some(s)) => s.ratio.clamp(STRETCH_MIN, STRETCH_MAX),
            _ => 1.0,
        }
    }

    /// Length on the timeline in session samples, given the source rate.
    pub fn timeline_length(&self, source_rate: u32, session_rate: u32) -> u64 {
        if let Some(l) = self.looped {
            return l.length;
        }
        let src = self.source_out.saturating_sub(self.source_in);
        let natural = if source_rate == session_rate || source_rate == 0 {
            src
        } else {
            ((u128::from(src) * u128::from(session_rate) + u128::from(source_rate) / 2)
                / u128::from(source_rate)) as u64
        };
        let r = self.stretch_ratio();
        if r == 1.0 {
            natural
        } else {
            (natural as f64 * r).round() as u64
        }
    }
}

/// Shape of a fade curve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FadeShape {
    #[default]
    Linear,
    /// Fast start: rises quickly, then eases in.
    Logarithmic,
    /// S curve: gentle at both ends.
    Cosine,
    /// Slow start: stays low, then rises late (a dip in the middle).
    Exponential,
    /// Equal power: keeps the level steady across a crossfade.
    EqualPower,
}

/// A clip fade. Length 0 means no fade.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
pub struct Fade {
    /// Length in session samples.
    pub length: u64,
    pub shape: FadeShape,
    /// Curve bend from -1.0 to 1.0. 0.0 is the neutral shape.
    pub curve: f64,
}

/// What an envelope automates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EnvelopeTarget {
    Volume,
    Pan,
    /// Scaffolding (P5): a parameter of an effect in the owner's Voice Chain.
    EffectParam {
        slot_id: Id,
        param: String,
    },
    /// 3D Space sessions: one setting of the owner's spot in the room.
    Space {
        param: SpaceParam,
    },
}

/// A setting of a spot in the room that automation can move. Values use
/// the spot's own units (see `SpacePos`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SpaceParam {
    X,
    Y,
    Z,
    Size,
    Width,
    Rotation,
    Lfe,
    Spread,
    Spatialize,
}

impl SpaceParam {
    /// The range its values take.
    pub fn range(self) -> (f64, f64) {
        match self {
            SpaceParam::X | SpaceParam::Y | SpaceParam::Z => (-1.0, 1.0),
            SpaceParam::Rotation => (-180.0, 180.0),
            _ => (0.0, 1.0),
        }
    }

    /// `pos` with this setting at `v`.
    pub fn set(self, pos: &mut SpacePos, v: f64) {
        let slot = match self {
            SpaceParam::X => &mut pos.x,
            SpaceParam::Y => &mut pos.y,
            SpaceParam::Z => &mut pos.z,
            SpaceParam::Size => &mut pos.size,
            SpaceParam::Width => &mut pos.width,
            SpaceParam::Rotation => &mut pos.rotation,
            SpaceParam::Lfe => &mut pos.lfe,
            SpaceParam::Spread => &mut pos.spread,
            SpaceParam::Spatialize => &mut pos.spatialize,
        };
        *slot = v;
    }
}

/// How the value moves from one point to the next.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EnvelopeCurve {
    #[default]
    Linear,
    Hold,
    Spline,
}

/// A single automation point.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EnvelopePoint {
    /// Position in session samples. For clip envelopes this is relative
    /// to the clip's left edge.
    pub position: u64,
    pub value: f64,
    pub curve: EnvelopeCurve,
}

/// An automation lane.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Envelope {
    pub id: Id,
    pub target: EnvelopeTarget,
    pub enabled: bool,
    /// Points sorted by position.
    pub points: Vec<EnvelopePoint>,
    /// A track's or bus's automation lane: its height on screen, in
    /// pixels. None is the standard height.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
}

/// Kind of marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MarkerKind {
    #[default]
    Cue,
    Subclip,
    /// Scaffolding (P7): a podcast chapter embedded on export.
    Chapter,
    /// Scaffolding (P5): an ad slot. Carries a contracted duration so
    /// the read can be timed.
    AdSlot,
    /// Scaffolding (P7): a region exported as a social media clip.
    SocialClip,
    /// Scaffolding (later, disc and broadcast work): where a CD track starts.
    CdTrack,
    /// Scaffolding (later, broadcast work): a timed cart, such as a jingle
    /// or a spot that must fit a set length.
    CartTimer,
}

/// Scaffolding (P7): extra data for chapter markers.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
pub struct ChapterInfo {
    pub url: Option<String>,
    pub image: Option<PathBuf>,
}

/// A point or range marker on the session timeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Marker {
    pub id: Id,
    pub name: String,
    pub kind: MarkerKind,
    /// Position in session samples.
    pub position: u64,
    /// Length in session samples. 0 is a point marker.
    pub duration: u64,
    pub description: String,
    pub chapter: Option<ChapterInfo>,
    /// Scaffolding (P5): contracted length of an ad slot, in session
    /// samples.
    pub contracted_duration: Option<u64>,
    /// `#rrggbb`. None uses the color of the marker's type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

/// A Voice Chain: an ordered list of effect slots on a clip, track,
/// bus, or the master.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EffectChain {
    /// Chain power. Off bypasses every slot.
    pub enabled: bool,
    pub input_gain_db: f64,
    pub output_gain_db: f64,
    /// Dry/wet mix from 0.0 (dry) to 1.0 (wet).
    pub mix: f64,
    pub slots: Vec<EffectSlot>,
}

impl Default for EffectChain {
    fn default() -> Self {
        Self {
            enabled: true,
            input_gain_db: 0.0,
            output_gain_db: 0.0,
            mix: 1.0,
            slots: Vec::new(),
        }
    }
}

/// Scaffolding (P5): one effect in a Voice Chain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EffectSlot {
    pub id: Id,
    /// Internal effect id, for example `altaudio.parametric_eq`.
    pub effect: String,
    pub enabled: bool,
    /// Parameter values by name.
    pub params: std::collections::BTreeMap<String, f64>,
    /// Name of the preset last loaded, if any.
    pub preset: Option<String>,
    /// Sidechain: the track or bus whose sound drives this effect's
    /// level detector (for ducking music under a voice, for example).
    /// The key is that track's or bus's sound after its Voice Chain,
    /// before its fader. Only effects with a detector use it, and only
    /// on track and bus chains. None listens to the effect's own input.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sidechain: Option<Id>,
    /// A VST3 plug-in's own settings (everything set in its window), as the
    /// plug-in saved them, base64. Only on plug-in slots (`vst3:` effects).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin_state: Option<String>,
    /// A built-in effect's own data that is not a setting, as JSON text:
    /// the Amp Model's capture and cabinet. Saved with the session and in
    /// presets, so they never depend on a file elsewhere.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
    /// Which of the numbered slots it shows in (0 is the first), so a chain
    /// can leave gaps. Effects still run in list order. None follows the
    /// one before.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<u32>,
    /// Which of the chain's channels the effect hears and which it writes
    /// back to. None maps channels by speaker name (L to L, R to R, and so
    /// on), so a stereo effect works on L and R.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub routing: Option<SlotRouting>,
}

/// An effect's channel routing within its chain.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
pub struct SlotRouting {
    /// Plug-ins: run with the plug-in's own channel layout (5.1, 7.1.4 and
    /// so on) instead of stereo.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub wide: bool,
    /// For each of the effect's inputs, the chain channel it hears (None
    /// hears silence). Empty maps by speaker name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<Option<u8>>,
    /// For each of the effect's outputs, the chain channel it replaces
    /// (None drops it). Channels no output goes to pass through untouched.
    /// Empty maps by speaker name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<Option<u8>>,
}

/// A span of the timeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TimeRange {
    pub start: u64,
    pub end: u64,
}

/// Saved editor view state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SessionView {
    /// Zoom as session samples per screen pixel.
    pub samples_per_pixel: f64,
    /// Left edge of the visible timeline, in session samples.
    pub scroll: u64,
    /// Vertical scroll of the track area, in pixels.
    pub scroll_y: u32,
    pub playhead: u64,
    pub selection: Option<TimeRange>,
}

impl Default for SessionView {
    fn default() -> Self {
        Self {
            samples_per_pixel: 1024.0,
            scroll: 0,
            scroll_y: 0,
            playhead: 0,
            selection: None,
        }
    }
}

/// Panel mode toggles, saved with the session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
pub struct SessionModes {
    /// Shows the Chapters, Transcript, and Podcast Publish panels.
    pub podcast: bool,
    /// Shows the Script, Take Lanes, and Delivery Check panels.
    pub voiceover: bool,
}

/// Scaffolding (P6, P7): episode metadata for show sync and export.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
pub struct EpisodeMetadata {
    /// Scaffolding (P6): show id in Podcast Manager.
    pub show_id: Option<String>,
    pub title: Option<String>,
    pub show: Option<String>,
    pub episode_number: Option<u32>,
    pub season_number: Option<u32>,
    pub description: Option<String>,
    pub artist: Option<String>,
    pub artwork: Option<PathBuf>,
    pub explicit: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn space_settings_round_trip_and_stay_out_of_plain_files() {
        let mut s = Session::new("t", 48_000);
        s.tracks.push(Track::new("A", ChannelLayout::Stereo));
        let plain = serde_json::to_string(&s).unwrap();
        assert!(!plain.contains("space"));
        s.space = Some(Space {
            layout: SpaceLayout::Surround712,
            listen: SpaceListen::Binaural,
        });
        s.tracks[0].space = Some(SpacePos {
            x: -0.5,
            y: 0.25,
            z: 0.75,
            size: 0.1,
            width: 0.5,
            rotation: 30.0,
            lfe: 0.25,
            spread: 0.4,
            spatialize: 0.6,
        });
        let text = serde_json::to_string(&s).unwrap();
        assert!(text.contains(r#""layout":"7.1.2""#));
        let back: Session = serde_json::from_str(&text).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn tempo_map_follows_changes_at_bars() {
        // Two bars of 4/4 at 120 (4 s), then 6/8 at 60.
        let t = Tempo {
            changes: vec![TempoChange {
                id: new_id(),
                bar: 2,
                bpm: 60.0,
                beats_per_bar: 6,
                beat_unit: 8,
            }],
            ..Tempo::default()
        };
        let segs: Vec<_> = t.segments().map(|s| (s.bar, s.beat, s.seconds)).collect();
        assert_eq!(segs, vec![(0, 0.0, 0.0), (2, 8.0, 4.0)]);
        assert_eq!(t.samples(8.0, 1_000), 4_000);
        // A quarter note at 60 is a second.
        assert_eq!(t.samples(11.0, 1_000), 7_000);
        assert!((t.beats(7_000, 1_000) - 11.0).abs() < 1e-9);
        assert_eq!(t.segment_at_seconds(3.9).bpm, 120.0);
        assert_eq!(t.segment_at_seconds(4.0).beat_unit, 8);
        // Files from before time signatures read as 4/4 with no changes.
        let old: Tempo = serde_json::from_str(r#"{"bpm":90,"beats_per_bar":3}"#).unwrap();
        assert_eq!((old.beat_unit, old.changes.len()), (4, 0));
    }

    #[test]
    fn tidy_tempo_keeps_one_change_per_bar_in_order() {
        let c = |bar, bpm| TempoChange {
            id: new_id(),
            bar,
            bpm,
            beats_per_bar: 4,
            beat_unit: 4,
        };
        let t = Tempo {
            changes: vec![c(5, 90.0), c(0, 70.0), c(2, 100.0), c(5, 95.0)],
            beat_unit: 3,
            ..Tempo::default()
        }
        .tidy();
        let got: Vec<_> = t.changes.iter().map(|c| (c.bar, c.bpm)).collect();
        assert_eq!(got, vec![(2, 100.0), (5, 95.0)]);
        assert_eq!(t.beat_unit, 4);
        assert_eq!(tidy_beat_unit(6), 8);
        assert_eq!(tidy_beat_unit(64), 32);
    }

    #[test]
    fn strip_effects_clears_every_chain() {
        let mut s = Session::new("t", 48_000);
        s.tracks.push(Track::new("A", ChannelLayout::Stereo));
        s.tracks[0].effects.input_gain_db = 6.0;
        s.master.effects.input_gain_db = 3.0;
        s.strip_effects();
        assert_eq!(s.tracks[0].effects, EffectChain::default());
        assert_eq!(s.master.effects, EffectChain::default());
    }
}
