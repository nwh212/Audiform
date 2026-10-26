// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! The metronome: a click on every beat of the session's tempo map,
//! heard during playback and recording, never in a mixdown.
//!
//! The render thread makes the click for each block it renders, at the
//! timeline position of that block, so it follows loops, skips and tempo
//! and time signature changes. The settings are atomics, read once per
//! block, so turning it on or changing its sound is heard within the
//! audio already rendered ahead (about half a second).

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use serde::{Deserialize, Serialize};

use crate::session::model::Tempo;

/// Clicks between beats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Subdivide {
    /// One click per beat.
    #[default]
    None,
    /// Two per beat.
    Halves,
    /// Three per beat.
    Triplets,
    /// Four per beat.
    Quarters,
}

impl Subdivide {
    fn count(self) -> u32 {
        match self {
            Subdivide::None => 1,
            Subdivide::Halves => 2,
            Subdivide::Triplets => 3,
            Subdivide::Quarters => 4,
        }
    }
}

/// The click's sound.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sound {
    /// A short bright tick.
    #[default]
    Tick,
    /// A hollow knock.
    Wood,
    /// A pitched beep.
    Beep,
    /// A short burst of noise.
    Shaker,
}

impl Sound {
    fn index(self) -> u32 {
        match self {
            Sound::Tick => 0,
            Sound::Wood => 1,
            Sound::Beep => 2,
            Sound::Shaker => 3,
        }
    }

    fn from_index(i: u32) -> Sound {
        match i {
            1 => Sound::Wood,
            2 => Sound::Beep,
            3 => Sound::Shaker,
            _ => Sound::Tick,
        }
    }
}

/// What the metronome plays. The interface decides whether it is on
/// (playback and recording, or recording only) and sends `on`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub on: bool,
    /// Level in dB, -60 to +6.
    pub level_db: f32,
    pub sound: Sound,
    /// The first beat of each bar clicks higher.
    pub accent: bool,
    pub subdivide: Subdivide,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            on: false,
            level_db: -6.0,
            sound: Sound::Tick,
            accent: true,
            subdivide: Subdivide::None,
        }
    }
}

static ON: AtomicBool = AtomicBool::new(false);
static LEVEL: AtomicU32 = AtomicU32::new(0);
static SOUND: AtomicU32 = AtomicU32::new(0);
static ACCENT: AtomicBool = AtomicBool::new(true);
static SUBDIVIDE: AtomicU32 = AtomicU32::new(1);

/// Sets what the metronome plays, while playing or not.
pub fn set(s: Settings) {
    let gain = 10f32.powf(s.level_db.clamp(-60.0, 6.0) / 20.0);
    LEVEL.store(gain.to_bits(), Ordering::Relaxed);
    SOUND.store(s.sound.index(), Ordering::Relaxed);
    ACCENT.store(s.accent, Ordering::Relaxed);
    SUBDIVIDE.store(s.subdivide.count(), Ordering::Relaxed);
    ON.store(s.on, Ordering::Relaxed);
}

/// Whether the metronome is on.
pub fn is_on() -> bool {
    ON.load(Ordering::Relaxed)
}

/// How strong a click is.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Kind {
    /// The first beat of a bar.
    Bar,
    Beat,
    /// Between beats.
    Sub,
}

/// One click sounding.
#[derive(Debug, Clone, Copy)]
struct Voice {
    kind: Kind,
    age: u32,
}

/// Click length, seconds.
const LENGTH: f64 = 0.08;

/// Makes the click for blocks of the timeline.
pub struct Click {
    tempo: Tempo,
    rate: u32,
    voice: Option<Voice>,
    /// The next click: timeline frame and kind.
    next: Option<(u64, Kind)>,
    /// Where the last block ended, to notice jumps.
    expected: u64,
    sub: u32,
    noise: u32,
}

impl Click {
    pub fn new(tempo: &Tempo, rate: u32) -> Click {
        Click {
            tempo: tempo.clone(),
            rate: rate.max(1),
            voice: None,
            next: None,
            expected: u64::MAX,
            sub: 1,
            noise: 0x1234_5678,
        }
    }

    /// The first click at or after timeline frame `frame`.
    fn next_from(&self, frame: u64, sub: u32) -> Option<(u64, Kind)> {
        let rate = f64::from(self.rate);
        let at = self.tempo.beats(frame, self.rate);
        let mut best: Option<(u64, Kind)> = None;
        let mut iter = self.tempo.segments().peekable();
        while let Some(seg) = iter.next() {
            let end = iter.peek().map(|n| n.beat);
            if end.is_some_and(|e| e <= at) {
                continue;
            }
            let step = seg.unit_beats() / f64::from(sub);
            let from = (at - seg.beat).max(0.0);
            // A hair of slack so a click right on `frame` is not missed.
            let mut k = ((from - 1e-9) / step).ceil().max(0.0) as u64;
            loop {
                let beat = seg.beat + k as f64 * step;
                if end.is_some_and(|e| beat >= e - 1e-9) {
                    break;
                }
                let t = (self.tempo.seconds(beat) * rate).round() as u64;
                if t < frame {
                    k += 1;
                    continue;
                }
                let per_bar = u64::from(seg.beats_per_bar) * u64::from(sub);
                let kind = if k.is_multiple_of(per_bar) {
                    Kind::Bar
                } else if k.is_multiple_of(u64::from(sub)) {
                    Kind::Beat
                } else {
                    Kind::Sub
                };
                best = Some((t, kind));
                break;
            }
            if best.is_some() {
                break;
            }
        }
        best
    }

    /// Writes the click for timeline frames `start..` into `out`
    /// (interleaved stereo, cleared first). Silent while it is off.
    pub fn render(&mut self, start: u64, out: &mut [f32]) {
        out.fill(0.0);
        let frames = out.len() / 2;
        if !ON.load(Ordering::Relaxed) {
            self.voice = None;
            // The next click is found again when it is turned on.
            self.expected = u64::MAX;
            return;
        }
        let sub = SUBDIVIDE.load(Ordering::Relaxed).clamp(1, 4);
        if start != self.expected || sub != self.sub {
            self.sub = sub;
            self.next = self.next_from(start, sub);
        }
        self.expected = start + frames as u64;
        let gain = f32::from_bits(LEVEL.load(Ordering::Relaxed));
        let sound = Sound::from_index(SOUND.load(Ordering::Relaxed));
        let accent = ACCENT.load(Ordering::Relaxed);
        let rate = self.rate as f32;
        let length = (LENGTH * f64::from(self.rate)) as u32;
        for i in 0..frames {
            let frame = start + i as u64;
            if let Some((at, kind)) = self.next {
                if frame >= at {
                    self.voice = Some(Voice { kind, age: 0 });
                    self.next = self.next_from(frame + 1, sub);
                }
            }
            let Some(v) = self.voice.as_mut() else {
                continue;
            };
            let kind = if accent || v.kind != Kind::Bar {
                v.kind
            } else {
                Kind::Beat
            };
            let t = v.age as f32 / rate;
            self.noise ^= self.noise << 13;
            self.noise ^= self.noise >> 17;
            self.noise ^= self.noise << 5;
            let white = (self.noise as f32 / u32::MAX as f32) * 2.0 - 1.0;
            let s = voice(sound, kind, t, white) * gain;
            out[i * 2] = s;
            out[i * 2 + 1] = s;
            v.age += 1;
            if v.age >= length {
                self.voice = None;
            }
        }
    }
}

/// One sample of a click `t` seconds in.
fn voice(sound: Sound, kind: Kind, t: f32, white: f32) -> f32 {
    use std::f32::consts::TAU;
    let (pitch, level) = match kind {
        Kind::Bar => (1.5, 1.0),
        Kind::Beat => (1.0, 0.75),
        Kind::Sub => (1.0, 0.4),
    };
    match sound {
        Sound::Tick => {
            let env = (-t / 0.006).exp();
            (TAU * 1_600.0 * pitch * t).sin() * env * level
        }
        Sound::Wood => {
            let env = (-t / 0.018).exp();
            let f = 820.0 * pitch;
            ((TAU * f * t).sin() * 0.7 + (TAU * f * 2.63 * t).sin() * 0.3) * env * level
        }
        Sound::Beep => {
            // Soft edges so it does not crackle.
            let env = (t / 0.003).min(1.0) * ((0.05 - t) / 0.01).clamp(0.0, 1.0);
            (TAU * 880.0 * pitch * t).sin() * env * level * 0.8
        }
        Sound::Shaker => {
            let env = (-t / (0.012 * pitch)).exp();
            white * env * level * 0.7
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::model::{new_id, TempoChange};

    fn clicks(tempo: &Tempo, rate: u32, from: u64, to: u64, sub: u32) -> Vec<(u64, Kind)> {
        let c = Click::new(tempo, rate);
        let mut out = Vec::new();
        let mut at = from;
        while let Some((t, k)) = c.next_from(at, sub) {
            if t >= to {
                break;
            }
            out.push((t, k));
            at = t + 1;
        }
        out
    }

    #[test]
    fn clicks_land_on_every_beat_with_the_bar_marked() {
        let t = Tempo::default();
        // 120 BPM at 1,000 frames a second: a beat every 500 frames.
        let c = clicks(&t, 1_000, 0, 2_600, 1);
        let at: Vec<u64> = c.iter().map(|c| c.0).collect();
        assert_eq!(at, vec![0, 500, 1_000, 1_500, 2_000, 2_500]);
        assert_eq!(c[0].1, Kind::Bar);
        assert_eq!(c[1].1, Kind::Beat);
        assert_eq!(c[4].1, Kind::Bar);
        // Halves add a quieter click between beats.
        let c = clicks(&t, 1_000, 0, 1_000, 2);
        assert_eq!(c.len(), 4);
        assert_eq!(c[1], (250, Kind::Sub));
        // Starting partway through finds the next beat.
        assert_eq!(clicks(&t, 1_000, 501, 1_100, 1), vec![(1_000, Kind::Beat)]);
    }

    #[test]
    fn clicks_follow_tempo_and_time_signature_changes() {
        // One bar of 4/4 at 120, then 6/8 at 60 (eighth-note clicks).
        let t = Tempo {
            changes: vec![TempoChange {
                id: new_id(),
                bar: 1,
                bpm: 60.0,
                beats_per_bar: 6,
                beat_unit: 8,
            }],
            ..Tempo::default()
        };
        let c = clicks(&t, 1_000, 0, 5_100, 1);
        let at: Vec<u64> = c.iter().map(|c| c.0).collect();
        // The change starts at 2 s; an eighth at 60 BPM is half a second.
        assert_eq!(
            at,
            vec![0, 500, 1_000, 1_500, 2_000, 2_500, 3_000, 3_500, 4_000, 4_500, 5_000]
        );
        assert_eq!(c[4].1, Kind::Bar);
        assert_eq!(c[5].1, Kind::Beat);
        // Six eighths later, the next bar.
        assert_eq!(c[10].1, Kind::Bar);
    }

    #[test]
    fn renders_silence_when_off_and_clicks_when_on() {
        let t = Tempo::default();
        let mut c = Click::new(&t, 1_000);
        let mut buf = vec![1.0; 200];
        set(Settings::default());
        c.render(0, &mut buf);
        assert!(buf.iter().all(|s| *s == 0.0));
        set(Settings {
            on: true,
            sound: Sound::Wood,
            ..Settings::default()
        });
        c.render(400, &mut buf);
        c.render(500, &mut buf);
        assert!(buf.iter().any(|s| s.abs() > 0.05));
        set(Settings::default());
    }
}
