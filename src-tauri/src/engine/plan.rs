// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Turns a session into what the playback engine needs: clip positions
//! in output-device frames, and per-track gains.
//!
//! Gains change often (faders, mute, solo, sends) and never need a
//! rebuild: the audio thread applies them from atomics in a plain
//! session, the mix graph ramps them in when routed. Anything that moves
//! audio in time (clips, tracks, order) or changes who feeds whom
//! (outputs, sends, buses, sidechains) changes the `layout`, and
//! playback restarts from where it was. Voice Chain changes are listed
//! apart (`chains`): they swap into the running renderer and graph,
//! with no restart, unless the chain's look-ahead changes.
//!
//! MIDI tracks plan their notes in output frames at the session tempo,
//! with their instrument; the renderer plays them through it.

use std::path::PathBuf;

use crate::dsp::effects::chain_hash;
use crate::session::model::{
    Clip, EffectChain, EffectSlot, EnvelopeTarget, Fade, FadeShape, Id, Session, Tempo, Track,
    TrackKind,
};

#[derive(Debug, Clone, PartialEq)]
pub struct ClipPlan {
    pub id: Id,
    pub path: PathBuf,
    /// Timeline start, in output frames.
    pub start: u64,
    /// Length, in output frames.
    pub len: u64,
    /// Where in the source file the clip begins, in seconds.
    pub source_start: f64,
    /// Linear clip gain. 0 when the clip is muted.
    pub gain: f32,
    /// Fade lengths in output frames.
    pub fade_in: u64,
    pub fade_out: u64,
    pub fade_in_shape: FadeLook,
    pub fade_out_shape: FadeLook,
    /// Volume envelope: (frame from the clip start, level in dB), sorted.
    /// Empty means no envelope.
    pub volume: Vec<(u64, f32)>,
    /// Pan envelope: (frame from the clip start, -100 left to 100
    /// right), sorted. Empty means centered.
    pub pan: Vec<(u64, f32)>,
    /// Loop Clip: the source region's length in output frames, or 0
    /// when the clip plays its region once.
    pub loop_len: u64,
    /// Where in the region the clip starts, in output frames.
    pub loop_phase: u64,
    /// Time stretch: timeline length over natural length. 1 is none.
    pub stretch: f64,
    /// Stretch keeps the pitch (true) or changes it with speed (false).
    pub keep_pitch: bool,
    /// Pitch change in semitones (pitch-keeping stretch only).
    pub pitch: f64,
    /// Stretch analysis frame length in seconds; 0 picks one by material.
    pub stretch_frame: f64,
    /// Clip Voice Chain, run after the clip's gain and fades.
    pub effects: EffectChain,
    /// Automation of the clip chain's effect settings, over time.
    pub automation: Vec<ParamLane>,
    /// Clips above this one on the track that overlap it. A clip on top
    /// covers the ones under it; they come through only where it fades.
    pub covers: Vec<Cover>,
    /// Source Channels: `[left, right]` per channel of the file. None plays
    /// mono to both sides and stereo as it is.
    pub mix: Option<Vec<[f32; 2]>>,
}

/// Where a clip on top covers this one, in frames from this clip's start.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cover {
    pub from: u64,
    pub to: u64,
    pub fade_in: u64,
    pub fade_in_shape: FadeLook,
    pub fade_out: u64,
    pub fade_out_shape: FadeLook,
    /// This clip fades out across the top clip's fade in (or in across
    /// its fade out): a crossfade, where both play through their own fades.
    pub crossfade_in: bool,
    pub crossfade_out: bool,
}

impl Cover {
    /// How much of this clip comes through at `rel`: 1 outside the top
    /// clip, 0 under its body, and the rest of its fade inside a fade.
    fn pass(&self, rel: u64) -> f32 {
        if rel < self.from || rel >= self.to {
            return 1.0;
        }
        let into = rel - self.from;
        if into < self.fade_in {
            return if self.crossfade_in {
                1.0
            } else {
                1.0 - fade_curve(self.fade_in_shape, into as f32 / self.fade_in as f32)
            };
        }
        let left = self.to - rel;
        if left < self.fade_out {
            return if self.crossfade_out {
                1.0
            } else {
                1.0 - fade_curve(self.fade_out_shape, left as f32 / self.fade_out as f32)
            };
        }
        0.0
    }
}

/// Fills in each clip's covers: every later clip (drawn on top) that
/// overlaps it and is not muted.
pub(crate) fn add_covers(clips: &mut [ClipPlan]) {
    for i in 0..clips.len() {
        let (lo, hi) = clips.split_at_mut(i + 1);
        let a = &mut lo[i];
        let a_end = a.start + a.len;
        for b in hi.iter() {
            let b_end = b.start + b.len;
            if b.gain == 0.0 || b.start >= a_end || b_end <= a.start {
                continue;
            }
            let from = b.start.saturating_sub(a.start);
            // Where b starts before a, its fade in is partly gone already.
            let cut = a.start.saturating_sub(b.start);
            let to = b_end - a.start;
            a.covers.push(Cover {
                from,
                to,
                fade_in: b.fade_in.saturating_sub(cut),
                fade_in_shape: b.fade_in_shape,
                fade_out: b.fade_out.min(to - from),
                fade_out_shape: b.fade_out_shape,
                crossfade_in: a.fade_out > 0 && a_end <= b.start + b.fade_in + 1 && a_end > b.start,
                crossfade_out: a.fade_in > 0
                    && a.start + 1 >= b_end.saturating_sub(b.fade_out)
                    && a.start < b_end,
            });
        }
    }
}

/// One effect setting moving over time: (frame from the clip start,
/// value), sorted. The value holds flat before the first point and
/// after the last, and moves in straight lines between them.
#[derive(Debug, Clone, PartialEq)]
pub struct ParamLane {
    pub slot: Id,
    pub param: String,
    pub points: Vec<(u64, f32)>,
}

impl ParamLane {
    pub fn value_at(&self, rel: u64) -> f32 {
        let pts = &self.points;
        let Some(first) = pts.first() else { return 0.0 };
        if rel <= first.0 {
            return first.1;
        }
        let last = pts[pts.len() - 1];
        if rel >= last.0 {
            return last.1;
        }
        let i = pts.partition_point(|p| p.0 <= rel);
        let (a, b) = (pts[i - 1], pts[i]);
        let t = (rel - a.0) as f32 / (b.0 - a.0).max(1) as f32;
        a.1 + (b.1 - a.1) * t
    }
}

/// A clip's effect-setting envelopes, in output frames.
fn param_lanes(c: &Clip, session_rate: u32, out_rate: u32) -> Vec<ParamLane> {
    c.envelopes
        .iter()
        .filter(|e| e.enabled && !e.points.is_empty())
        .filter_map(|e| match &e.target {
            EnvelopeTarget::EffectParam { slot_id, param } => Some(ParamLane {
                slot: *slot_id,
                param: param.clone(),
                points: e
                    .points
                    .iter()
                    .map(|p| (convert(p.position, session_rate, out_rate), p.value as f32))
                    .collect(),
            }),
            _ => None,
        })
        .collect()
}

impl ClipPlan {
    /// A plain clip: full level, no fades.
    pub fn plain(id: Id, path: PathBuf, start: u64, len: u64, source_start: f64) -> Self {
        Self {
            id,
            path,
            start,
            len,
            source_start,
            gain: 1.0,
            fade_in: 0,
            fade_out: 0,
            fade_in_shape: (FadeShape::Linear, 0.0),
            fade_out_shape: (FadeShape::Linear, 0.0),
            volume: Vec::new(),
            pan: Vec::new(),
            loop_len: 0,
            loop_phase: 0,
            stretch: 1.0,
            keep_pitch: true,
            pitch: 0.0,
            stretch_frame: 0.0,
            effects: EffectChain::default(),
            automation: Vec::new(),
            covers: Vec::new(),
            mix: None,
        }
    }

    /// True when the gain is the same for every frame.
    pub fn is_flat(&self) -> bool {
        self.fade_in == 0
            && self.fade_out == 0
            && self.volume.is_empty()
            && self.pan.is_empty()
            && self.loop_len == 0
            && self.covers.is_empty()
    }

    /// Looped clips: a short dip on both sides of each seam inside the
    /// clip, so a region whose ends don't line up never clicks. The
    /// clip's own edges are left to its fades.
    fn seam_gain(&self, rel: u64) -> f32 {
        if self.loop_len == 0 {
            return 1.0;
        }
        let seam = (self.loop_len / 4).clamp(1, SEAM_FRAMES);
        let p = (self.loop_phase + rel) % self.loop_len;
        let to_next = self.loop_len - p;
        // Frames since the last seam (only if that seam is inside the clip).
        let after = if p < seam && rel >= p && rel > 0 {
            Some(p)
        } else {
            None
        };
        // Frames until the next seam (only if it comes before the clip ends).
        let before = if to_next <= seam && rel + to_next < self.len {
            Some(to_next - 1)
        } else {
            None
        };
        match after.into_iter().chain(before).min() {
            Some(d) => (d as f32 + 0.5) / seam as f32,
            None => 1.0,
        }
    }

    /// Envelope level at `rel` in dB: held before the first point and
    /// after the last, straight lines in dB between points.
    fn volume_db(&self, rel: u64) -> f32 {
        interpolate(&self.volume, rel)
    }

    /// Left and right gain at `rel`: clip gain, volume envelope and
    /// fades, then the pan envelope as a balance (centered is full level
    /// on both sides, the same rule as track pan).
    pub fn gains_at(&self, rel: u64) -> [f32; 2] {
        let g = self.gain_at(rel);
        if self.pan.is_empty() {
            return [g, g];
        }
        let pan = (interpolate(&self.pan, rel) / 100.0).clamp(-1.0, 1.0);
        let left = if pan > 0.0 { 1.0 - pan } else { 1.0 };
        let right = if pan < 0.0 { 1.0 + pan } else { 1.0 };
        [g * left, g * right]
    }

    /// Gain at `rel`, a frame offset from the clip's start.
    pub fn gain_at(&self, rel: u64) -> f32 {
        let mut g = self.gain;
        if !self.volume.is_empty() {
            let db = self.volume_db(rel);
            // The bottom of the range is silence, not just quiet.
            g *= if db <= VOLUME_FLOOR_DB {
                0.0
            } else {
                10f32.powf(db / 20.0)
            };
        }
        if rel < self.fade_in {
            g *= fade_curve(self.fade_in_shape, rel as f32 / self.fade_in as f32);
        }
        for c in &self.covers {
            g *= c.pass(rel);
        }
        g *= self.seam_gain(rel);
        let left = self.len.saturating_sub(rel);
        if left < self.fade_out {
            g *= fade_curve(self.fade_out_shape, left as f32 / self.fade_out as f32);
        }
        g
    }
}

/// Value of a sorted point list at `rel`: held before the first point
/// and after the last, straight lines between points.
fn interpolate(v: &[(u64, f32)], rel: u64) -> f32 {
    let i = v.partition_point(|p| p.0 <= rel);
    if i == 0 {
        return v[0].1;
    }
    if i == v.len() {
        return v[v.len() - 1].1;
    }
    let (a, b) = (v[i - 1], v[i]);
    let t = (rel - a.0) as f32 / (b.0 - a.0).max(1) as f32;
    a.1 + (b.1 - a.1) * t
}

/// Length of the dip on each side of a loop seam: 2.5 ms at 48 kHz.
const SEAM_FRAMES: u64 = 120;

/// Envelope level treated as silence.
const VOLUME_FLOOR_DB: f32 = -59.9;

/// A clip's envelope points for `target` in output frames, if it has one.
fn envelope_points(
    c: &Clip,
    target: &EnvelopeTarget,
    session_rate: u32,
    out_rate: u32,
) -> Vec<(u64, f32)> {
    c.envelopes
        .iter()
        .find(|e| &e.target == target && e.enabled)
        .map(|e| {
            e.points
                .iter()
                .map(|p| (convert(p.position, session_rate, out_rate), p.value as f32))
                .collect()
        })
        .unwrap_or_default()
}

/// A fade's shape and its bend, -1 to 1 (0 is the plain shape).
pub type FadeLook = (FadeShape, f32);

/// Fade gain for `t` from 0 (silent) to 1 (full). The bend skews the
/// shape: above 0 it rises sooner (bulging up), below 0 later (dipping).
pub fn fade_curve((shape, bend): FadeLook, t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let bend = if bend.is_finite() { bend.clamp(-1.0, 1.0) } else { 0.0 };
    let t = if bend == 0.0 { t } else { t.powf(4f32.powf(-bend)) };
    match shape {
        FadeShape::Linear => t,
        // Rises fast, then eases in: sounds even to the ear on speech.
        FadeShape::Logarithmic => 1.0 - (1.0 - t) * (1.0 - t),
        // S-curve: gentle at both ends.
        FadeShape::Cosine => 0.5 - 0.5 * (std::f32::consts::PI * t).cos(),
        // Dips: quiet for longer, then rises late.
        FadeShape::Exponential => t * t,
        // Quarter sine: a crossfade of two of these keeps the power even.
        FadeShape::EqualPower => (std::f32::consts::FRAC_PI_2 * t).sin(),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TrackPlan {
    pub id: Id,
    pub clips: Vec<ClipPlan>,
    /// A MIDI track with an instrument: what it plays.
    pub midi: Option<MidiPlan>,
}

/// One note in output frames.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NotePlan {
    pub on: u64,
    /// After `on`.
    pub off: u64,
    pub pitch: u8,
    /// 0 to 1.
    pub velocity: f32,
    /// MIDI channel, 0 to 15.
    pub channel: u8,
    /// -1 to 1; 0 sends none.
    pub pan: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MidiPlan {
    pub instrument: EffectSlot,
    pub tempo: Tempo,
    /// Every note of every unmuted clip, cut at its clip's end, in order of `on`.
    pub notes: Vec<NotePlan>,
    /// Every note's end: (frame, pitch, channel), in order.
    pub offs: Vec<(u64, u8, u8)>,
}

/// What a MIDI track plays, or None when it has no instrument (silent).
fn midi_plan(t: &Track, tempo: &Tempo, out_rate: u32) -> Option<MidiPlan> {
    if t.kind != TrackKind::Midi {
        return None;
    }
    let instrument = t.instrument.clone()?;
    let mut notes: Vec<NotePlan> = Vec::new();
    for c in t.midi_clips.iter().filter(|c| !c.muted) {
        let level = 10f32.powf(c.gain_db.clamp(-96.0, 24.0) as f32 / 20.0);
        for n in &c.notes {
            if n.start >= c.length || n.length <= 0.0 {
                continue;
            }
            let on = tempo.samples(c.start + n.start, out_rate);
            let off = tempo.samples(c.start + (n.start + n.length).min(c.length), out_rate);
            notes.push(NotePlan {
                on,
                off: off.max(on + 1),
                pitch: n.pitch.min(127),
                velocity: (f32::from(n.velocity.clamp(1, 127)) / 127.0 * level)
                    .clamp(1.0 / 127.0, 1.0),
                channel: n.channel.min(15),
                pan: n.pan.clamp(-1.0, 1.0),
            });
        }
    }
    notes.sort_by_key(|n| n.on);
    let mut offs: Vec<(u64, u8, u8)> = notes.iter().map(|n| (n.off, n.pitch, n.channel)).collect();
    offs.sort_unstable();
    Some(MidiPlan {
        instrument,
        tempo: tempo.clone(),
        notes,
        offs,
    })
}

/// Session samples where the track's last MIDI clip ends.
pub fn midi_end(session: &Session) -> u64 {
    session
        .tracks
        .iter()
        .flat_map(|t| &t.midi_clips)
        .map(|c| {
            session
                .tempo
                .samples(c.start + c.length, session.sample_rate)
        })
        .max()
        .unwrap_or(0)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub out_rate: u32,
    pub session_rate: u32,
    pub tracks: Vec<TrackPlan>,
}

fn convert(value: u64, from: u32, to: u32) -> u64 {
    if from == to || from == 0 {
        return value;
    }
    ((u128::from(value) * u128::from(to) + u128::from(from) / 2) / u128::from(from)) as u64
}

pub fn build(session: &Session, out_rate: u32) -> Plan {
    let tracks = session
        .tracks
        .iter()
        .map(|t| TrackPlan {
            id: t.id,
            midi: midi_plan(t, &session.tempo, out_rate),
            clips: t
                .clips
                .iter()
                .filter_map(|c| {
                    let m = session.media_by_id(c.media_id)?;
                    let src_rate = if m.sample_rate > 0 {
                        m.sample_rate
                    } else {
                        session.sample_rate
                    };
                    let src_len = c.source_out.saturating_sub(c.source_in);
                    if src_len == 0 {
                        return None;
                    }
                    let region = convert(src_len, src_rate, out_rate);
                    let (len, loop_len, loop_phase) = match c.looped {
                        Some(l) if region > 0 => (
                            convert(l.length, session.sample_rate, out_rate),
                            region,
                            convert(l.phase % src_len, src_rate, out_rate) % region,
                        ),
                        _ => {
                            let r = c.stretch_ratio();
                            ((region as f64 * r).round() as u64, 0, 0)
                        }
                    };
                    if len == 0 {
                        return None;
                    }
                    let fade = |l: u64| convert(l, session.sample_rate, out_rate).min(len);
                    // Made here if it is missing (a session from another computer),
                    // since its region only makes sense in the reversed copy.
                    let rev = if c.reversed {
                        crate::engine::reverse::cached(&m.path)
                            .or_else(|| crate::engine::reverse::prepare(&m.path).ok())
                    } else {
                        None
                    };
                    Some(ClipPlan {
                        id: c.id,
                        mix: crate::dsp::channels::effective(
                            c.channel_mix.as_ref(),
                            usize::from(m.channels),
                        ),
                        // Reversed: its file's reversed copy, from the mirrored spot.
                        path: rev.clone().unwrap_or_else(|| m.path.clone()),
                        start: convert(c.start, session.sample_rate, out_rate),
                        len,
                        // A reversed clip's region is kept in the reversed copy's terms.
                        source_start: c.source_in as f64 / f64::from(src_rate),
                        gain: if c.muted {
                            0.0
                        } else {
                            let g = 10f64.powf(c.gain_db.clamp(-96.0, 24.0) / 20.0) as f32;
                            if c.inverted { -g } else { g }
                        },
                        fade_in: fade(c.fade_in.length),
                        fade_out: fade(c.fade_out.length),
                        fade_in_shape: (c.fade_in.shape, c.fade_in.curve as f32),
                        fade_out_shape: (c.fade_out.shape, c.fade_out.curve as f32),
                        volume: envelope_points(
                            c,
                            &EnvelopeTarget::Volume,
                            session.sample_rate,
                            out_rate,
                        ),
                        pan: envelope_points(
                            c,
                            &EnvelopeTarget::Pan,
                            session.sample_rate,
                            out_rate,
                        ),
                        loop_len,
                        loop_phase,
                        stretch: c.stretch_ratio(),
                        keep_pitch: c.stretch.is_none_or(|s| s.keep_pitch),
                        pitch: match (c.looped, c.stretch) {
                            (None, Some(s)) if s.keep_pitch => s.pitch.clamp(
                                crate::session::model::PITCH_MIN,
                                crate::session::model::PITCH_MAX,
                            ),
                            _ => 0.0,
                        },
                        stretch_frame: c.stretch.map_or(0.0, |s| match s.advanced {
                            Some(a) if a.window_ms > 0.0 => {
                                (a.window_ms / 1000.0).clamp(0.005, 0.2)
                            }
                            // Several notes at once: longer frames keep chords steady.
                            _ if s.polyphonic => 0.08,
                            _ => 0.0,
                        }),
                        effects: c.effects.clone(),
                        automation: param_lanes(c, session.sample_rate, out_rate),
                        covers: Vec::new(),
                    })
                })
                .collect(),
        })
        .map(|mut t: TrackPlan| {
            add_covers(&mut t.clips);
            t
        })
        .collect();
    Plan {
        out_rate,
        session_rate: session.sample_rate,
        tracks,
    }
}

/// Session sample to output frame and back.
pub fn to_out(session_sample: u64, session_rate: u32, out_rate: u32) -> u64 {
    convert(session_sample, session_rate, out_rate)
}

pub fn to_session(out_frame: u64, session_rate: u32, out_rate: u32) -> u64 {
    convert(out_frame, out_rate, session_rate)
}

/// Left and right gain for each track, with mute, solo, volume, pan
/// and polarity: what the audio thread applies in a plain session.
/// See [`super::graph::levels`] for the rules.
pub fn gains(session: &Session) -> Vec<[f32; 2]> {
    super::graph::levels(session)
        .tracks
        .into_iter()
        .map(|l| l.main)
        .collect()
}

/// Master gain from the master track's volume.
pub fn master_gain(session: &Session) -> f32 {
    10f64.powf(session.master.volume_db.clamp(-96.0, 24.0) / 20.0) as f32
}

/// Everything that places audio in time. Playback restarts when it changes.
/// Clip id, media id, start, source in, source out, then the clip's
/// level shape: gain bits (0 when muted), fade in, fade out, and a hash
/// of its envelopes and loop.
pub type ClipKey = (Id, Id, u64, u64, u64, u64, Fade, Fade, u64);

fn envelope_hash(c: &Clip) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut mix = |v: u64| {
        h ^= v;
        h = h.wrapping_mul(0x100_0000_01b3);
    };
    // Reversed or inverted changes what plays too.
    mix(u64::from(c.reversed) | u64::from(c.inverted) << 1);
    // A stretch changes what plays where, so it is part of the shape too.
    if let Some(st) = c.stretch {
        mix(0x5354_5243);
        mix(st.ratio.to_bits());
        mix(u64::from(st.keep_pitch));
        mix(st.pitch.to_bits());
        mix(u64::from(st.polyphonic));
        mix(st.advanced.map_or(0, |a| a.window_ms.to_bits()));
    }
    // The loop changes what plays where, so it is part of the shape too.
    if let Some(l) = c.looped {
        mix(0x4c4f_4f50);
        mix(l.length);
        mix(l.phase);
    }
    for e in &c.envelopes {
        mix(match e.target {
            EnvelopeTarget::Volume => 1,
            EnvelopeTarget::Pan => 2,
            EnvelopeTarget::EffectParam { .. } => 3,
            EnvelopeTarget::Space { .. } => 4,
        });
        mix(u64::from(e.enabled));
        for p in &e.points {
            mix(p.position);
            mix(p.value.to_bits());
        }
    }
    h
}

/// Per track: track id and its clips, in order; then a fingerprint of
/// the routing. A change restarts playback from the same spot.
#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    pub tracks: Vec<(Id, Vec<ClipKey>)>,
    pub routing: u64,
    /// MIDI notes, instruments and the tempo.
    pub midi: u64,
}

/// Whose Voice Chain it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Owner {
    Track,
    Clip,
    Bus,
    Master,
}

/// Every Voice Chain that plays: owner id, whose it is, and a
/// fingerprint of the chain.
pub type Chains = Vec<(Id, Owner, u64)>;

pub fn chains(session: &Session) -> Chains {
    let mut out = Vec::new();
    for t in &session.tracks {
        out.push((t.id, Owner::Track, chain_hash(&t.effects)));
        for c in &t.clips {
            out.push((c.id, Owner::Clip, chain_hash(&c.effects)));
        }
    }
    for b in &session.buses {
        out.push((b.id, Owner::Bus, chain_hash(&b.effects)));
    }
    out.push((
        Id::nil(),
        Owner::Master,
        chain_hash(&session.master.effects),
    ));
    out
}

/// The chain of the track, clip or bus `id`, or the master's.
pub fn chain_of(session: &Session, id: Id, owner: Owner) -> Option<&EffectChain> {
    match owner {
        Owner::Track => session
            .tracks
            .iter()
            .find(|t| t.id == id)
            .map(|t| &t.effects),
        Owner::Clip => session
            .tracks
            .iter()
            .flat_map(|t| &t.clips)
            .find(|c| c.id == id)
            .map(|c| &c.effects),
        Owner::Bus => session
            .buses
            .iter()
            .find(|b| b.id == id)
            .map(|b| &b.effects),
        Owner::Master => Some(&session.master.effects),
    }
}

/// A fingerprint of who feeds whom: whether the mix is routed, the
/// buses, every output and send (not their levels), and every sidechain.
fn routing_hash(session: &Session) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut mix = |v: u128| {
        for b in v.to_le_bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x100_0000_01b3);
        }
    };
    let routed = crate::session::routing::is_routed(session);
    mix(u128::from(routed));
    mix(session.space.map_or(0, |s| s.layout as u128 + 1));
    mix(session.space.map_or(0, |s| s.listen as u128));
    // Automation edits restart playback in place, so they are heard.
    for envs in session
        .tracks
        .iter()
        .map(|t| &t.envelopes)
        .chain(session.buses.iter().map(|b| &b.envelopes))
    {
        for b in serde_json::to_vec(envs).unwrap_or_default() {
            mix(u128::from(b));
        }
    }
    let output = |o: &crate::session::model::Output| match o {
        crate::session::model::Output::Master => 0,
        crate::session::model::Output::Bus { bus_id } => bus_id.as_u128(),
    };
    let chain = |c: &EffectChain, mix: &mut dyn FnMut(u128)| {
        for s in crate::session::routing::key_sources(c) {
            mix(s.as_u128());
        }
    };
    for t in &session.tracks {
        mix(output(&t.output));
        for x in &t.sends {
            mix(x.id.as_u128() ^ x.bus_id.as_u128());
            mix(u128::from(x.pre_fader));
        }
        chain(&t.effects, &mut mix);
        mix(1);
    }
    if routed {
        for b in &session.buses {
            mix(b.id.as_u128());
            mix(u128::from(
                b.channels == crate::session::model::ChannelLayout::Mono,
            ));
            mix(output(&b.output));
            for x in &b.sends {
                mix(x.id.as_u128() ^ x.bus_id.as_u128());
                mix(u128::from(x.pre_fader));
            }
            chain(&b.effects, &mut mix);
            mix(2);
        }
        chain(&session.master.effects, &mut mix);
    }
    h
}

pub fn layout(session: &Session) -> Layout {
    Layout {
        tracks: clip_layout(session),
        routing: routing_hash(session),
        midi: midi_hash(session),
    }
}

/// A fingerprint of what MIDI tracks play: the tempo, each instrument
/// (not its settings, which reach a running instrument directly) and
/// every MIDI clip.
fn midi_hash(session: &Session) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut mix = |v: u64| {
        h ^= v;
        h = h.wrapping_mul(0x100_0000_01b3);
    };
    for seg in session.tempo.segments() {
        mix(u64::from(seg.bar));
        mix(seg.bpm.to_bits());
        mix(u64::from(seg.beats_per_bar));
        mix(u64::from(seg.beat_unit));
    }
    for t in session.tracks.iter().filter(|t| t.kind == TrackKind::Midi) {
        mix(t.id.as_u128() as u64);
        if let Some(i) = &t.instrument {
            mix(i.id.as_u128() as u64);
            for b in i.effect.bytes() {
                mix(u64::from(b));
            }
        }
        for c in &t.midi_clips {
            mix(c.id.as_u128() as u64);
            mix(c.start.to_bits());
            mix(c.length.to_bits());
            mix(u64::from(c.muted));
            for n in &c.notes {
                mix(u64::from(n.pitch) | u64::from(n.velocity) << 8);
                mix(n.start.to_bits());
                mix(n.length.to_bits());
            }
        }
    }
    h
}

fn clip_layout(session: &Session) -> Vec<(Id, Vec<ClipKey>)> {
    session
        .tracks
        .iter()
        .map(|t| {
            (
                t.id,
                t.clips
                    .iter()
                    .map(|c| {
                        let gain = if c.muted {
                            f64::NEG_INFINITY
                        } else {
                            c.gain_db
                        };
                        (
                            c.id,
                            c.media_id,
                            c.start,
                            c.source_in,
                            c.source_out,
                            gain.to_bits(),
                            c.fade_in,
                            c.fade_out,
                            envelope_hash(c),
                        )
                    })
                    .collect(),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::model::{new_id, ChannelLayout, MediaRef, Track};
    use crate::session::ops::{apply, ClipSpec, Op};

    fn session_with_clip(file_rate: u32) -> Session {
        let mut s = Session::new("t", 48_000);
        s.tracks.push(Track::new("A", ChannelLayout::Stereo));
        let m = MediaRef {
            id: new_id(),
            path: "a.wav".into(),
            relative_path: None,
            sample_rate: file_rate,
            channels: 2,
            length: 441_000,
        };
        let mid = m.id;
        s.media.push(m);
        let tid = s.tracks[0].id;
        apply(
            &mut s,
            &Op::AddClips {
                track_id: tid,
                clips: vec![ClipSpec {
                    id: new_id(),
                    name: "a".into(),
                    media_id: mid,
                    start: 48_000,
                    source_in: 44_100,
                    source_out: 88_200,
                }],
            },
        )
        .unwrap();
        s
    }

    #[test]
    fn places_clips_in_output_frames() {
        let s = session_with_clip(44_100);
        let p = build(&s, 44_100);
        let c = &p.tracks[0].clips[0];
        assert_eq!(c.start, 44_100, "one second in at 44.1 kHz output");
        assert_eq!(c.len, 44_100);
        assert!((c.source_start - 1.0).abs() < 1e-9);
        assert_eq!(build(&s, 48_000).tracks[0].clips[0].len, 48_000);
    }

    #[test]
    fn gains_follow_mute_solo_volume_and_pan() {
        let mut s = Session::new("t", 48_000);
        for name in ["A", "B", "C"] {
            s.tracks.push(Track::new(name, ChannelLayout::Stereo));
        }
        s.tracks[0].volume_db = -6.0;
        s.tracks[0].pan = 50.0;
        s.tracks[2].muted = true;
        let g = gains(&s);
        assert!((g[0][0] - 0.5012 * 0.5).abs() < 1e-3);
        assert!((g[0][1] - 0.5012).abs() < 1e-3);
        assert_eq!(g[1], [1.0, 1.0]);
        assert_eq!(g[2], [0.0, 0.0]);

        s.tracks[1].solo = true;
        let g = gains(&s);
        assert_eq!(g[0], [0.0, 0.0]);
        assert_eq!(g[1], [1.0, 1.0]);
    }

    #[test]
    fn polarity_invert_flips_the_track_gain() {
        let mut s = Session::new("t", 48_000);
        s.tracks.push(Track::new("a", ChannelLayout::Mono));
        s.tracks[0].polarity_inverted = true;
        let g = gains(&s);
        assert_eq!(g[0], [-1.0, -1.0]);
    }

    #[test]
    fn layout_ignores_gain_changes() {
        let mut s = session_with_clip(48_000);
        let before = layout(&s);
        s.tracks[0].volume_db = -10.0;
        s.tracks[0].muted = true;
        assert_eq!(layout(&s), before);
        // Voice Chain edits are not timing; they are listed apart.
        let chained = chains(&s);
        s.tracks[0].effects.input_gain_db = 3.0;
        assert_eq!(layout(&s), before);
        assert_ne!(chains(&s), chained);
        s.tracks[0].clips[0].start += 1;
        assert_ne!(layout(&s), before);
    }

    #[test]
    fn converts_positions_both_ways() {
        assert_eq!(to_out(48_000, 48_000, 44_100), 44_100);
        assert_eq!(to_session(44_100, 48_000, 44_100), 48_000);
    }

    #[test]
    fn volume_envelope_interpolates_in_db() {
        let mut c = ClipPlan::plain(new_id(), "a.wav".into(), 0, 1_000, 0.0);
        c.volume = vec![(100, 0.0), (300, -20.0)];
        assert!(
            (c.gain_at(0) - 1.0).abs() < 1e-6,
            "held before the first point"
        );
        assert!((c.gain_at(200) - 0.316).abs() < 0.01, "-10 dB halfway");
        assert!(
            (c.gain_at(900) - 0.1).abs() < 1e-3,
            "held after the last point"
        );
        c.volume = vec![(0, -60.0)];
        assert_eq!(c.gain_at(10), 0.0, "the floor is silence");
        assert!(!c.is_flat());
    }

    #[test]
    fn loop_seams_dip_but_clip_edges_do_not() {
        let mut c = ClipPlan::plain(new_id(), "a.wav".into(), 0, 10_000, 0.0);
        c.loop_len = 4_000;
        assert!(!c.is_flat());
        assert_eq!(c.gain_at(0), 1.0, "the clip's own start is untouched");
        assert_eq!(c.gain_at(2_000), 1.0, "mid-region is full level");
        assert!(c.gain_at(4_000) < 0.01, "right at the seam");
        assert!(c.gain_at(3_999) < 0.01, "just before the seam");
        assert!(
            (c.gain_at(4_060) - 0.504).abs() < 0.01,
            "halfway out of the dip"
        );
        assert_eq!(c.gain_at(4_200), 1.0);
        assert_eq!(c.gain_at(9_999), 1.0, "the clip's own end is untouched");
        c.loop_phase = 3_950;
        assert!(
            c.gain_at(50) < 0.01,
            "a seam just inside the left edge still dips"
        );
    }

    #[test]
    fn pan_envelope_balances_left_and_right() {
        let mut c = ClipPlan::plain(new_id(), "a.wav".into(), 0, 1_000, 0.0);
        assert_eq!(c.gains_at(10), [1.0, 1.0]);
        c.pan = vec![(0, -100.0), (1_000, 100.0)];
        assert!(!c.is_flat());
        assert_eq!(c.gains_at(0), [1.0, 0.0], "full left");
        assert_eq!(c.gains_at(500), [1.0, 1.0], "centered halfway");
        let [l, r] = c.gains_at(750);
        assert!(
            (l - 0.5).abs() < 1e-3 && (r - 1.0).abs() < 1e-6,
            "half right turns left down"
        );
        c.gain = 0.5;
        assert_eq!(c.gains_at(1_000), [0.0, 0.5], "clip gain still applies");
    }
}
