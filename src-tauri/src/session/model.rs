// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

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

/// The session's tempo and meter.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Tempo {
    /// Beats (quarter notes) per minute.
    pub bpm: f64,
    /// Beats in a bar, for example 4 in 4/4 or 3 in 3/4.
    pub beats_per_bar: u32,
}

impl Default for Tempo {
    fn default() -> Self {
        Self {
            bpm: 120.0,
            beats_per_bar: 4,
        }
    }
}

/// Lowest and highest tempos a session can have.
pub const BPM_MIN: f64 = 20.0;
pub const BPM_MAX: f64 = 400.0;

impl Tempo {
    pub fn is_default(&self) -> bool {
        *self == Tempo::default()
    }

    /// Beats to samples at `rate`.
    pub fn samples(&self, beats: f64, rate: u32) -> u64 {
        (beats.max(0.0) * 60.0 / self.bpm.clamp(BPM_MIN, BPM_MAX) * f64::from(rate)).round() as u64
    }

    /// Samples at `rate` to beats.
    pub fn beats(&self, samples: u64, rate: u32) -> f64 {
        samples as f64 / f64::from(rate.max(1)) * self.bpm.clamp(BPM_MIN, BPM_MAX) / 60.0
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
    pub notes: Vec<Note>,
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
            hidden: false,
            volume_db: 0.0,
            pan: 0.0,
            muted: false,
            solo: false,
            polarity_inverted: false,
            mono: false,
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

/// How a stretched clip plays.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ClipStretch {
    /// Timeline length over natural length: 1.25 plays 25 percent longer.
    pub ratio: f64,
    /// True keeps the pitch (for speech and ad fitting); false changes
    /// pitch with speed, like a tape running faster or slower.
    pub keep_pitch: bool,
}

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
    Logarithmic,
    Cosine,
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
