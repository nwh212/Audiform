// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

//! MIDI inputs: keyboards, pads and controllers plugged into the computer.
//!
//! Each armed MIDI track listens to one input (or all of them). What is
//! played goes two ways:
//!
//! - Live: the track's instrument plays it at once, through the track's
//!   effects, fader and pan, to the speakers. This runs on its own copy of
//!   the instrument and effects, rendered a few milliseconds at a time, so
//!   it never waits for playback (which renders half a second ahead) and
//!   works whether or not the session plays.
//! - Recorded: while recording, each armed track's notes are kept with
//!   their times, and become a MIDI clip when recording stops.
//!
//! The sustain pedal holds notes the way a piano's does: a key let go while
//! it is down keeps sounding (and recording) until the pedal comes up.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use altered_plughost::protocol::NoteEvent;
use rtrb::Producer;
use serde::{Deserialize, Serialize};

use crate::dsp::effects::ChainProcessor;
use crate::plugins::instance::{self, Host};
use crate::session::model::{EffectChain, EffectSlot, Id};

/// The name Audiform's MIDI connections show under in the system.
const CLIENT: &str = "Audiform";

/// The MIDI inputs on this computer, by name.
pub fn inputs() -> Vec<String> {
    let Ok(m) = midir::MidiInput::new(CLIENT) else {
        return Vec::new();
    };
    let mut names: Vec<String> = m
        .ports()
        .iter()
        .filter_map(|p| m.port_name(p).ok())
        .collect();
    names.dedup();
    names
}

/// A key or pedal change, after the sustain pedal is taken into account.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum KeyEvent {
    On {
        pitch: u8,
        channel: u8,
        velocity: f32,
    },
    Off {
        pitch: u8,
        channel: u8,
    },
}

/// The keys down and held by the pedal on one track's input, turning raw
/// MIDI messages into the notes that start and end.
#[derive(Debug, Default)]
pub struct Keys {
    pedal: [bool; 16],
    /// Keys pressed and not yet let go: (pitch, channel).
    down: Vec<(u8, u8)>,
    /// Keys let go while the pedal was down, still sounding.
    held: Vec<(u8, u8)>,
}

impl Keys {
    /// Reads one MIDI message and adds the note starts and ends it makes.
    pub fn feed(&mut self, msg: &[u8], out: &mut Vec<KeyEvent>) {
        let Some(&status) = msg.first() else {
            return;
        };
        let channel = status & 0x0f;
        let kind = status & 0xf0;
        let a = msg.get(1).copied().unwrap_or(0) & 0x7f;
        let b = msg.get(2).copied().unwrap_or(0) & 0x7f;
        match kind {
            0x90 if b > 0 => {
                let k = (a, channel);
                // The same key again while it sounds: end it first.
                if self.down.contains(&k) || self.held.contains(&k) {
                    self.down.retain(|x| *x != k);
                    self.held.retain(|x| *x != k);
                    out.push(KeyEvent::Off { pitch: a, channel });
                }
                self.down.push(k);
                out.push(KeyEvent::On {
                    pitch: a,
                    channel,
                    velocity: f32::from(b) / 127.0,
                });
            }
            0x80 | 0x90 => {
                let k = (a, channel);
                if !self.down.contains(&k) {
                    return;
                }
                self.down.retain(|x| *x != k);
                if self.pedal[usize::from(channel)] {
                    self.held.push(k);
                } else {
                    out.push(KeyEvent::Off { pitch: a, channel });
                }
            }
            // Sustain pedal.
            0xb0 if a == 64 => {
                let on = b >= 64;
                self.pedal[usize::from(channel)] = on;
                if !on {
                    let (go, keep): (Vec<_>, Vec<_>) =
                        self.held.drain(..).partition(|x| x.1 == channel);
                    self.held = keep;
                    for (pitch, channel) in go {
                        out.push(KeyEvent::Off { pitch, channel });
                    }
                }
            }
            // All Sound Off and All Notes Off.
            0xb0 if a == 120 || a == 123 => {
                let (go, keep): (Vec<_>, Vec<_>) = self
                    .down
                    .drain(..)
                    .chain(self.held.drain(..))
                    .partition(|x| x.1 == channel);
                self.down = keep;
                for (pitch, channel) in go {
                    out.push(KeyEvent::Off { pitch, channel });
                }
            }
            _ => {}
        }
    }

    /// Ends every note still sounding.
    pub fn release_all(&mut self, out: &mut Vec<KeyEvent>) {
        for (pitch, channel) in self.down.drain(..).chain(self.held.drain(..)) {
            out.push(KeyEvent::Off { pitch, channel });
        }
        self.pedal = [false; 16];
    }
}

/// A recorded note, with its times in seconds from the start of recording.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct TakeNote {
    pub pitch: u8,
    pub channel: u8,
    /// 0 to 1.
    pub velocity: f32,
    pub start: f64,
    pub end: f64,
}

/// One armed track's recorded notes.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MidiTake {
    pub track_id: Id,
    pub notes: Vec<TakeNote>,
}

#[derive(Debug, Default)]
struct Capture {
    /// Notes started and not ended yet: (pitch, channel, velocity, start).
    open: Vec<(u8, u8, f32, f64)>,
    done: Vec<TakeNote>,
}

impl Capture {
    fn add(&mut self, e: KeyEvent, t: f64) {
        match e {
            KeyEvent::On {
                pitch,
                channel,
                velocity,
            } => self.open.push((pitch, channel, velocity, t)),
            KeyEvent::Off { pitch, channel } => {
                if let Some(i) = self
                    .open
                    .iter()
                    .position(|o| o.0 == pitch && o.1 == channel)
                {
                    let (pitch, channel, velocity, start) = self.open.remove(i);
                    self.done.push(TakeNote {
                        pitch,
                        channel,
                        velocity,
                        start,
                        end: t.max(start),
                    });
                }
            }
        }
    }

    /// The notes, every one still held ended at `t`, in order of start.
    fn finish(mut self, t: f64) -> Vec<TakeNote> {
        for (pitch, channel, velocity, start) in self.open.drain(..) {
            self.done.push(TakeNote {
                pitch,
                channel,
                velocity,
                start,
                end: t.max(start),
            });
        }
        self.done.sort_by(|a, b| a.start.total_cmp(&b.start));
        self.done
    }
}

/// An armed MIDI track, as the interface describes it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveTrack {
    pub track_id: Id,
    /// The input it listens to; None for every input.
    pub input: Option<String>,
    /// The instrument; None leaves it silent (it still records).
    pub instrument: Option<EffectSlot>,
    /// The track's effects, heard after the instrument.
    #[serde(default)]
    pub chain: Option<EffectChain>,
    #[serde(default)]
    pub gain_db: f64,
    /// -100 (left) to 100 (right).
    #[serde(default)]
    pub pan: f64,
}

impl LiveTrack {
    fn gains(&self) -> [f32; 2] {
        let g = 10f64.powf(self.gain_db.clamp(-96.0, 24.0) / 20.0);
        let pan = (self.pan / 100.0).clamp(-1.0, 1.0);
        let left = if pan > 0.0 { 1.0 - pan } else { 1.0 };
        let right = if pan < 0.0 { 1.0 + pan } else { 1.0 };
        [(g * left) as f32, (g * right) as f32]
    }

    /// What the live voice is made from: a change here starts a new one.
    fn sound(&self) -> (Option<&EffectSlot>, Option<&EffectChain>) {
        (
            self.instrument.as_ref().filter(|s| s.enabled),
            self.chain.as_ref(),
        )
    }
}

/// One track's route from the inputs.
struct Route {
    track: LiveTrack,
    keys: Keys,
    /// Notes for the live voice, taken by the player each block.
    queue: Arc<Mutex<Vec<NoteEvent>>>,
    capture: Option<Capture>,
}

#[derive(Default)]
struct Routes {
    list: Vec<Route>,
    /// When recording started.
    t0: Option<Instant>,
    /// Step input (the piano roll's): keys pressed on any input, waiting
    /// to be read, while it listens.
    step: Option<StepTap>,
}

/// Keys pressed for step input, with when they were pressed.
struct StepTap {
    keys: Keys,
    t0: Instant,
    pressed: Vec<StepKey>,
}

/// A key pressed for step input.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct StepKey {
    pub pitch: u8,
    pub channel: u8,
    /// 0 to 1.
    pub velocity: f32,
    /// Seconds since step input started listening, to tell chords from runs.
    pub at: f64,
}

impl Routes {
    /// One message from the input named `port`.
    fn message(&mut self, port: &str, msg: &[u8]) {
        let t = self.t0.map(|t0| t0.elapsed().as_secs_f64());
        let mut evs = Vec::new();
        if let Some(st) = self.step.as_mut() {
            st.keys.feed(msg, &mut evs);
            let at = st.t0.elapsed().as_secs_f64();
            for e in &evs {
                if let KeyEvent::On {
                    pitch,
                    channel,
                    velocity,
                } = *e
                {
                    st.pressed.push(StepKey {
                        pitch,
                        channel,
                        velocity,
                        at,
                    });
                }
            }
        }
        for r in &mut self.list {
            if r.track.input.as_deref().is_some_and(|i| i != port) {
                continue;
            }
            evs.clear();
            r.keys.feed(msg, &mut evs);
            if evs.is_empty() {
                continue;
            }
            if let Ok(mut q) = r.queue.lock() {
                q.extend(evs.iter().map(|e| note_event(*e)));
            }
            if let (Some(c), Some(t)) = (r.capture.as_mut(), t) {
                for e in &evs {
                    c.add(*e, t);
                }
            }
        }
    }
}

fn note_event(e: KeyEvent) -> NoteEvent {
    match e {
        KeyEvent::On {
            pitch,
            channel,
            velocity,
        } => NoteEvent {
            offset: 0,
            on: true,
            pitch,
            velocity,
            channel,
            pan: 0.0,
        },
        KeyEvent::Off { pitch, channel } => NoteEvent {
            offset: 0,
            on: false,
            pitch,
            velocity: 0.0,
            channel,
            pan: 0.0,
        },
    }
}

/// A track's instrument and effects, playing live.
struct Voice {
    track_id: Id,
    made_from: LiveTrack,
    queue: Arc<Mutex<Vec<NoteEvent>>>,
    host: Arc<Mutex<Host>>,
    chain: Option<ChainProcessor>,
    gains: [f32; 2],
    pending: Vec<(u32, f64)>,
    /// Notes sounding, so they can be let go when the voice goes.
    sounding: Vec<(u8, u8)>,
}

impl Voice {
    fn start(t: &LiveTrack, queue: Arc<Mutex<Vec<NoteEvent>>>, rate: u32) -> Option<Voice> {
        let slot = t.instrument.as_ref().filter(|s| s.enabled)?;
        // Copies of their own, so playback's keep playing undisturbed.
        let host = instance::dedicated(|| instance::host_for(slot, rate))?;
        let pending = host.lock().ok()?.differing(&instance::slot_params_of(slot));
        let chain = t.chain.as_ref().and_then(|c| {
            let mut c = c.clone();
            // An effect keyed from another track has nothing to listen to here.
            c.slots.retain(|s| s.sidechain.is_none());
            instance::dedicated(|| ChainProcessor::new(&c, rate))
        });
        Some(Voice {
            track_id: t.track_id,
            made_from: t.clone(),
            queue,
            host,
            chain,
            gains: t.gains(),
            pending,
            sounding: Vec::new(),
        })
    }

    /// Adds the next `buf.len() / 2` frames into `mix`.
    fn play(&mut self, buf: &mut [f32], mix: &mut [f32]) {
        let notes: Vec<NoteEvent> = match self.queue.lock() {
            Ok(mut q) => std::mem::take(&mut *q),
            Err(_) => Vec::new(),
        };
        for n in &notes {
            if n.on {
                self.sounding.push((n.pitch, n.channel));
            } else {
                self.sounding.retain(|x| *x != (n.pitch, n.channel));
            }
        }
        buf.fill(0.0);
        let ok = self
            .host
            .lock()
            .map(|mut h| h.process(&self.pending, &notes, buf))
            .unwrap_or(false);
        self.pending.clear();
        if !ok {
            return;
        }
        if let Some(c) = self.chain.as_mut() {
            c.process(buf);
        }
        let [gl, gr] = self.gains;
        for (m, f) in mix
            .as_chunks_mut::<2>()
            .0
            .iter_mut()
            .zip(buf.as_chunks::<2>().0)
        {
            m[0] += f[0] * gl;
            m[1] += f[1] * gr;
        }
    }

    /// Lets go of every note still sounding.
    fn silence(&mut self) {
        let offs: Vec<NoteEvent> = self
            .sounding
            .drain(..)
            .map(|(pitch, channel)| note_event(KeyEvent::Off { pitch, channel }))
            .collect();
        if offs.is_empty() {
            return;
        }
        let mut buf = vec![0.0; BLOCK * 2];
        if let Ok(mut h) = self.host.lock() {
            h.process(&[], &offs, &mut buf);
        }
    }
}

impl Drop for Voice {
    fn drop(&mut self) {
        self.silence();
    }
}

/// Frames rendered at a time.
const BLOCK: usize = 128;
/// Frames kept waiting for the speakers, at least: about 5 ms at 48 kHz.
const AHEAD: usize = 256;

/// Frames to keep waiting for a device that takes `buffer` frames per
/// callback: two of its buffers and a block, so a callback never finds
/// the queue short (which is heard as stutter at larger ASIO buffers).
fn ahead_for(buffer: Option<u32>) -> usize {
    // Without a fixed size, allow for a large shared-mode buffer.
    let b = buffer.map_or(1024, |n| n as usize);
    (2 * b + BLOCK).max(AHEAD)
}

/// Renders the live voices to the speakers.
struct Player {
    voices: Arc<Mutex<Vec<Voice>>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    rate: u32,
}

impl Player {
    fn start(rate: u32) -> Result<Player, String> {
        let (producer, stream) = super::record::open_monitor(rate, None)?;
        let ahead = ahead_for(match super::host::buffer_size() {
            cpal::BufferSize::Fixed(n) => Some(n),
            cpal::BufferSize::Default => None,
        });
        let voices: Arc<Mutex<Vec<Voice>>> = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (v, s) = (voices.clone(), stop.clone());
        let thread = thread::Builder::new()
            .name("midi-live".into())
            .spawn(move || run(producer, stream, v, s, ahead))
            .map_err(|e| e.to_string())?;
        Ok(Player {
            voices,
            stop,
            thread: Some(thread),
            rate,
        })
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn run(
    mut out: Producer<f32>,
    stream: super::host::QuietStream,
    voices: Arc<Mutex<Vec<Voice>>>,
    stop: Arc<AtomicBool>,
    ahead: usize,
) {
    let capacity = out.buffer().capacity();
    let mut buf = vec![0.0f32; BLOCK * 2];
    let mut mix = vec![0.0f32; BLOCK * 2];
    while !stop.load(Ordering::SeqCst) {
        let waiting = (capacity - out.slots()) / 2;
        if waiting >= ahead || out.slots() < BLOCK * 2 {
            thread::sleep(Duration::from_millis(1));
            continue;
        }
        mix.fill(0.0);
        if let Ok(mut vs) = voices.lock() {
            for v in vs.iter_mut() {
                v.play(&mut buf, &mut mix);
            }
        }
        for s in &mix {
            let _ = out.push(*s);
        }
    }
    drop(stream);
}

struct Inner {
    /// Open inputs, by name.
    conns: Vec<(String, midir::MidiInputConnection<()>)>,
    player: Option<Player>,
    /// The armed tracks last set, for reopening inputs.
    tracks: Vec<LiveTrack>,
    /// Step input listens to every input.
    step: bool,
}

/// MIDI input, live playing and MIDI recording. Kept in Tauri state.
pub struct MidiLive {
    routes: Arc<Mutex<Routes>>,
    inner: Mutex<Inner>,
}

impl Default for MidiLive {
    fn default() -> Self {
        Self {
            routes: Arc::new(Mutex::new(Routes::default())),
            inner: Mutex::new(Inner {
                conns: Vec::new(),
                player: None,
                tracks: Vec::new(),
                step: false,
            }),
        }
    }
}

impl MidiLive {
    /// Sets the armed MIDI tracks: opens the inputs they listen to (and
    /// closes the rest), and starts, keeps or ends each one's live voice.
    /// Slow the first time an instrument is used (it loads); call it off
    /// the interface's thread.
    pub fn set(&self, tracks: Vec<LiveTrack>) -> Result<(), String> {
        let mut inner = self.inner.lock().map_err(|e| e.to_string())?;
        // Routes: keep each track's keys and recording, update the rest.
        let queues: Vec<(Id, Arc<Mutex<Vec<NoteEvent>>>)> = {
            let mut r = self.routes.lock().map_err(|e| e.to_string())?;
            let mut old = std::mem::take(&mut r.list);
            let recording = r.t0.is_some();
            for t in &tracks {
                match old.iter().position(|x| x.track.track_id == t.track_id) {
                    Some(i) => {
                        let mut x = old.remove(i);
                        if x.track.input != t.input {
                            // Another input: the keys held on the old one let go.
                            let mut evs = Vec::new();
                            x.keys.release_all(&mut evs);
                            if let Ok(mut q) = x.queue.lock() {
                                q.extend(evs.iter().map(|e| note_event(*e)));
                            }
                        }
                        x.track = t.clone();
                        r.list.push(x);
                    }
                    None => r.list.push(Route {
                        track: t.clone(),
                        keys: Keys::default(),
                        queue: Arc::new(Mutex::new(Vec::new())),
                        capture: recording.then(Capture::default),
                    }),
                }
            }
            r.list
                .iter()
                .map(|x| (x.track.track_id, x.queue.clone()))
                .collect()
        };
        inner.tracks = tracks.clone();
        self.connect(&mut inner);

        // Live voices: only tracks with an instrument.
        let playing: Vec<&LiveTrack> = tracks.iter().filter(|t| t.sound().0.is_some()).collect();
        if playing.is_empty() {
            inner.player = None;
            return Ok(());
        }
        let rate = crate::engine::output::device_rate();
        if inner.player.as_ref().is_none_or(|p| p.rate != rate) {
            inner.player = Some(Player::start(rate)?);
        }
        let player = inner.player.as_ref().expect("just started");
        // Voices that stay as they are keep playing while new ones load.
        let missing: Vec<&LiveTrack> = {
            let mut vs = player.voices.lock().map_err(|e| e.to_string())?;
            vs.retain(|v| {
                playing
                    .iter()
                    .any(|t| t.track_id == v.track_id && t.sound() == v.made_from.sound())
            });
            for v in vs.iter_mut() {
                if let Some(t) = playing.iter().find(|t| t.track_id == v.track_id) {
                    v.gains = t.gains();
                }
            }
            playing
                .iter()
                .copied()
                .filter(|t| !vs.iter().any(|v| v.track_id == t.track_id))
                .collect()
        };
        let fresh: Vec<Voice> = missing
            .into_iter()
            .filter_map(|t| {
                let q = queues.iter().find(|q| q.0 == t.track_id)?.1.clone();
                Voice::start(t, q, rate)
            })
            .collect();
        if let Ok(mut vs) = player.voices.lock() {
            vs.extend(fresh);
        }
        Ok(())
    }

    /// Opens the inputs the tracks listen to and closes the others.
    fn connect(&self, inner: &mut Inner) {
        let all = inputs();
        let tracks = &inner.tracks;
        let wanted: Vec<String> = if inner.step || tracks.iter().any(|t| t.input.is_none()) {
            all
        } else {
            all.into_iter()
                .filter(|n| {
                    tracks
                        .iter()
                        .any(|t| t.input.as_deref() == Some(n.as_str()))
                })
                .collect()
        };
        inner.conns.retain(|(n, _)| wanted.contains(n));
        for name in wanted {
            if inner.conns.iter().any(|(n, _)| *n == name) {
                continue;
            }
            let Ok(m) = midir::MidiInput::new(CLIENT) else {
                continue;
            };
            let Some(port) = m
                .ports()
                .into_iter()
                .find(|p| m.port_name(p).ok().as_deref() == Some(name.as_str()))
            else {
                continue;
            };
            let routes = self.routes.clone();
            let label = name.clone();
            let conn = m.connect(
                &port,
                "Audiform input",
                move |_, msg, _| {
                    if let Ok(mut r) = routes.lock() {
                        r.message(&label, msg);
                    }
                },
                (),
            );
            match conn {
                Ok(c) => inner.conns.push((name, c)),
                Err(e) => crate::engine::host::log(&format!("MIDI input {name}: {e}")),
            }
        }
    }

    /// Step input: while on, keys pressed on any input are kept for
    /// [`step_take`](Self::step_take), whether or not a track is armed.
    pub fn step_listen(&self, on: bool) -> Result<(), String> {
        let mut inner = self.inner.lock().map_err(|e| e.to_string())?;
        {
            let mut r = self.routes.lock().map_err(|e| e.to_string())?;
            r.step = on.then(|| StepTap {
                keys: Keys::default(),
                t0: Instant::now(),
                pressed: Vec::new(),
            });
        }
        inner.step = on;
        self.connect(&mut inner);
        Ok(())
    }

    /// The keys pressed for step input since the last call.
    pub fn step_take(&self) -> Vec<StepKey> {
        self.routes
            .lock()
            .ok()
            .and_then(|mut r| r.step.as_mut().map(|s| std::mem::take(&mut s.pressed)))
            .unwrap_or_default()
    }

    /// Starts recording on `tracks` (armed MIDI tracks set with [`set`]).
    pub fn record_start(&self, tracks: &[Id]) -> Result<(), String> {
        let mut r = self.routes.lock().map_err(|e| e.to_string())?;
        r.t0 = Some(Instant::now());
        for x in &mut r.list {
            x.capture = tracks.contains(&x.track.track_id).then(Capture::default);
        }
        Ok(())
    }

    /// While recording, starts or ends one track's recording (armed or
    /// disarmed just now). Ending keeps what it recorded for [`record_stop`].
    pub fn record_track(&self, track: Id, on: bool) -> Result<(), String> {
        let mut r = self.routes.lock().map_err(|e| e.to_string())?;
        if r.t0.is_none() {
            return Ok(());
        }
        if let Some(x) = r.list.iter_mut().find(|x| x.track.track_id == track) {
            if on && x.capture.is_none() {
                x.capture = Some(Capture::default());
            }
        }
        Ok(())
    }

    /// Stops recording and returns each track's notes (tracks that played
    /// nothing are left out).
    pub fn record_stop(&self) -> Result<Vec<MidiTake>, String> {
        let mut r = self.routes.lock().map_err(|e| e.to_string())?;
        let Some(t0) = r.t0.take() else {
            return Ok(Vec::new());
        };
        let t = t0.elapsed().as_secs_f64();
        Ok(r.list
            .iter_mut()
            .filter_map(|x| {
                let notes = x.capture.take()?.finish(t);
                (!notes.is_empty()).then_some(MidiTake {
                    track_id: x.track.track_id,
                    notes,
                })
            })
            .collect())
    }
}

/// Plays `messages` (each at a frame) through `track`'s live voice for
/// `frames` frames, with no input device or speakers: the same keys,
/// instrument and effects as live playing. For tests. None when the
/// track's instrument does not start.
#[doc(hidden)]
pub fn render_live(
    track: &LiveTrack,
    rate: u32,
    messages: &[(usize, Vec<u8>)],
    frames: usize,
) -> Option<Vec<f32>> {
    let queue = Arc::new(Mutex::new(Vec::new()));
    let mut v = Voice::start(track, queue.clone(), rate)?;
    let mut keys = Keys::default();
    let mut out = Vec::with_capacity(frames * 2);
    let mut buf = vec![0.0f32; BLOCK * 2];
    let mut mix = vec![0.0f32; BLOCK * 2];
    let mut at = 0;
    while at < frames {
        let mut evs = Vec::new();
        for (_, m) in messages.iter().filter(|(f, _)| *f >= at && *f < at + BLOCK) {
            keys.feed(m, &mut evs);
        }
        if let Ok(mut q) = queue.lock() {
            q.extend(evs.into_iter().map(note_event));
        }
        mix.fill(0.0);
        v.play(&mut buf, &mut mix);
        out.extend_from_slice(&mix);
        at += BLOCK;
    }
    out.truncate(frames * 2);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_queue_covers_two_device_buffers() {
        assert_eq!(ahead_for(Some(64)), AHEAD);
        assert_eq!(ahead_for(Some(512)), 2 * 512 + BLOCK);
        assert_eq!(ahead_for(Some(2048)), 2 * 2048 + BLOCK);
        assert!(ahead_for(None) >= 2048);
    }

    fn feed(k: &mut Keys, msgs: &[&[u8]]) -> Vec<KeyEvent> {
        let mut out = Vec::new();
        for m in msgs {
            k.feed(m, &mut out);
        }
        out
    }

    #[test]
    fn keys_start_and_end_notes() {
        let mut k = Keys::default();
        let out = feed(
            &mut k,
            &[
                &[0x90, 60, 127],
                &[0x80, 60, 0],
                &[0x91, 64, 64],
                &[0x91, 64, 0],
            ],
        );
        assert_eq!(
            out,
            vec![
                KeyEvent::On {
                    pitch: 60,
                    channel: 0,
                    velocity: 1.0
                },
                KeyEvent::Off {
                    pitch: 60,
                    channel: 0
                },
                KeyEvent::On {
                    pitch: 64,
                    channel: 1,
                    velocity: 64.0 / 127.0
                },
                KeyEvent::Off {
                    pitch: 64,
                    channel: 1
                },
            ]
        );
    }

    #[test]
    fn the_pedal_holds_notes_until_it_comes_up() {
        let mut k = Keys::default();
        let out = feed(
            &mut k,
            &[&[0x90, 60, 100], &[0xb0, 64, 127], &[0x80, 60, 0]],
        );
        assert_eq!(out.len(), 1, "still sounding while the pedal is down");
        // The same key again: the held note ends, a new one starts.
        let out = feed(&mut k, &[&[0x90, 60, 90]]);
        assert!(matches!(out[0], KeyEvent::Off { pitch: 60, .. }));
        assert!(matches!(out[1], KeyEvent::On { pitch: 60, .. }));
        let out = feed(&mut k, &[&[0x80, 60, 0], &[0xb0, 64, 0]]);
        assert_eq!(
            out,
            vec![KeyEvent::Off {
                pitch: 60,
                channel: 0
            }]
        );
        // All Notes Off.
        let out = feed(&mut k, &[&[0x90, 50, 100], &[0xb0, 123, 0]]);
        assert_eq!(
            out[1],
            KeyEvent::Off {
                pitch: 50,
                channel: 0
            }
        );
    }

    #[test]
    fn a_recording_keeps_notes_with_their_times() {
        let mut c = Capture::default();
        c.add(
            KeyEvent::On {
                pitch: 60,
                channel: 0,
                velocity: 0.5,
            },
            0.25,
        );
        c.add(
            KeyEvent::On {
                pitch: 64,
                channel: 0,
                velocity: 0.75,
            },
            0.5,
        );
        c.add(
            KeyEvent::Off {
                pitch: 60,
                channel: 0,
            },
            1.0,
        );
        let notes = c.finish(2.0);
        assert_eq!(notes.len(), 2);
        assert_eq!(
            (notes[0].pitch, notes[0].start, notes[0].end),
            (60, 0.25, 1.0)
        );
        // Still held when recording stopped: it ends there.
        assert_eq!((notes[1].pitch, notes[1].end), (64, 2.0));
    }

    #[test]
    fn step_input_keeps_keys_from_any_input() {
        let mut r = Routes {
            step: Some(StepTap {
                keys: Keys::default(),
                t0: Instant::now(),
                pressed: Vec::new(),
            }),
            ..Routes::default()
        };
        r.message("Anything", &[0x90, 60, 127]);
        r.message("Other", &[0x92, 64, 64]);
        r.message("Other", &[0x82, 64, 0]);
        let keys: Vec<(u8, u8)> = r
            .step
            .unwrap()
            .pressed
            .iter()
            .map(|k| (k.pitch, k.channel))
            .collect();
        assert_eq!(keys, vec![(60, 0), (64, 2)], "key presses only");
    }

    #[test]
    fn routes_listen_to_their_own_input() {
        let mut r = Routes::default();
        let track = |input: Option<&str>| LiveTrack {
            track_id: crate::session::model::new_id(),
            input: input.map(str::to_string),
            instrument: None,
            chain: None,
            gain_db: 0.0,
            pan: 0.0,
        };
        for t in [track(Some("Keys")), track(None), track(Some("Pads"))] {
            r.list.push(Route {
                track: t,
                keys: Keys::default(),
                queue: Arc::new(Mutex::new(Vec::new())),
                capture: None,
            });
        }
        r.message("Keys", &[0x90, 60, 100]);
        let heard: Vec<usize> = r
            .list
            .iter()
            .map(|x| x.queue.lock().unwrap().len())
            .collect();
        assert_eq!(heard, vec![1, 1, 0]);
    }
}
