// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Multitrack playback.
//!
//! Threads:
//! - the caller (a Tauri command) starts, stops and updates playback;
//! - a render thread streams every track's clips a little ahead through
//!   the mix graph (`graph.rs`) into lock-free ring buffers: one per
//!   track in a plain session, one for the whole mix when routed;
//! - the device's audio thread mixes those rings with the current gains
//!   (atomics), applies the master fader, writes the output, and records
//!   peak meters.
//!
//! In a plain session fader, pan, mute and solo changes take effect on
//! the next audio buffer. When routed (buses, sends, a master chain or a
//! sidechain), track and bus levels apply on the render thread, so they
//! are heard once the audio already rendered ahead has played (about
//! half a second); the master fader stays instant. Voice Chain edits
//! swap into the render thread without a restart (heard after the same
//! half second), unless a chain's look-ahead changes. Edits that move
//! audio in time or change the routing restart playback in place.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, StreamConfig};
use rtrb::{Consumer, Producer, RingBuffer};
use serde::Serialize;

use super::graph::{self, Graph, GraphMeters, Levels, Mix};
use super::metronome::Click;
use super::plan::{self, Owner};
use super::render::{ChainUpdate, Renderer};
use crate::session::model::{EffectChain, Id, Session};

/// Meter readings from the render thread kept for the status call, at most.
const METER_QUEUE: usize = 512;

/// The graph's peaks for a stretch of rendered audio, tagged with the
/// rendered frame count at its end.
type BlockPeaks = (u64, GraphMeters);

/// Mono check: the audio thread folds what plays to mono while this is
/// on, for checking how a mix holds up in mono. Listening only: exports
/// never see it. Read with a relaxed load, so it applies within one buffer.
static MONO_CHECK: AtomicBool = AtomicBool::new(false);

/// Turns the mono check on or off, while playing or not.
pub fn set_mono_check(on: bool) {
    MONO_CHECK.store(on, Ordering::Relaxed);
}

pub fn mono_check() -> bool {
    MONO_CHECK.load(Ordering::Relaxed)
}

/// Audio rendered ahead of the device, per track.
const AHEAD_SECONDS: f64 = 0.5;
/// Frames rendered per step on the render thread.
const BLOCK: usize = 2048;

fn f32_bits(v: f32) -> u32 {
    v.to_bits()
}

fn bits_f32(b: u32) -> f32 {
    f32::from_bits(b)
}

/// State shared with the render and audio threads.
struct Shared {
    stop: AtomicBool,
    /// Output frames sent to the device.
    played: AtomicU64,
    gains: Vec<[AtomicU32; 2]>,
    master: AtomicU32,
    /// Peak since last read, as f32 bits. Positive floats order like
    /// their bit patterns, so `fetch_max` works on the bits.
    meters: Vec<[AtomicU32; 2]>,
    master_meter: [AtomicU32; 2],
    underruns: AtomicU64,
    /// Chains edited during playback, waiting for the render thread.
    chains: Mutex<Option<ChainUpdate>>,
    /// Routed mixes: levels changed during playback, waiting for the
    /// render thread.
    levels: Mutex<Option<Levels>>,
    /// Clip levels changed during playback, waiting for the render thread.
    clip_gains: Mutex<Option<Vec<(Id, f32)>>>,
    /// Render-thread meters, oldest first, released as the device plays them.
    chain_meters: Mutex<VecDeque<BlockPeaks>>,
    /// The last reading `played_meters` gave, given again when no new block
    /// was played since (otherwise the meters blinked to silence).
    last_played: Mutex<GraphMeters>,
    /// A level heard over part of the timeline while a level control is
    /// being dragged (1.0 when none), applied as each sample plays.
    audition: Audition,
    /// What the audio thread needs to know where on the timeline it is.
    clock: Clock,
}

/// A gain over `start..end` of the timeline (session samples), for
/// hearing a level change before it is applied.
struct Audition {
    gain: AtomicU32,
    start: AtomicU64,
    end: AtomicU64,
    /// One track's stream only (a clip's level handle), or every track
    /// (`u32::MAX`).
    track: AtomicU32,
    /// Output frames from here on were rendered with the edited level
    /// already, so they are not auditioned again (`u64::MAX`: none yet).
    until: AtomicU64,
}

impl Audition {
    fn set(&self, gain: f32, start: u64, end: u64, track: u32) {
        self.start.store(start, Ordering::Relaxed);
        self.end.store(end, Ordering::Relaxed);
        self.track.store(track, Ordering::Relaxed);
        self.until.store(u64::MAX, Ordering::Relaxed);
        self.gain.store(f32_bits(gain), Ordering::Relaxed);
    }
}

/// Where playback started and how it jumps, set once playback runs.
struct Clock {
    from: AtomicU64,
    /// 0 plays straight on, 1 loops, 2 skips.
    kind: AtomicU32,
    a: AtomicU64,
    b: AtomicU64,
    session_rate: AtomicU32,
    out_rate: AtomicU32,
}

impl Clock {
    /// The timeline position (session samples) of output frame `played`.
    #[inline]
    fn at(&self, played: u64) -> u64 {
        let sr = self.session_rate.load(Ordering::Relaxed);
        let or = self.out_rate.load(Ordering::Relaxed);
        let elapsed = if sr == 0 || or == 0 {
            played
        } else {
            plan::to_session(played, sr, or)
        };
        let (a, b) = (
            self.a.load(Ordering::Relaxed),
            self.b.load(Ordering::Relaxed),
        );
        let jump = match self.kind.load(Ordering::Relaxed) {
            1 => Some(Jump::Loop { start: a, end: b }),
            2 => Some(Jump::Skip { start: a, end: b }),
            _ => None,
        };
        Jump::position(jump, self.from.load(Ordering::Relaxed), elapsed)
    }
}

impl Shared {
    fn new(tracks: usize, gains: &[[f32; 2]], master: f32) -> Self {
        Self {
            stop: AtomicBool::new(false),
            played: AtomicU64::new(0),
            gains: (0..tracks)
                .map(|i| {
                    let g = gains.get(i).copied().unwrap_or([0.0, 0.0]);
                    [
                        AtomicU32::new(f32_bits(g[0])),
                        AtomicU32::new(f32_bits(g[1])),
                    ]
                })
                .collect(),
            master: AtomicU32::new(f32_bits(master)),
            meters: (0..tracks)
                .map(|_| [AtomicU32::new(0), AtomicU32::new(0)])
                .collect(),
            master_meter: [AtomicU32::new(0), AtomicU32::new(0)],
            underruns: AtomicU64::new(0),
            chains: Mutex::new(None),
            levels: Mutex::new(None),
            clip_gains: Mutex::new(None),
            chain_meters: Mutex::new(VecDeque::new()),
            last_played: Mutex::new(GraphMeters::default()),
            audition: Audition {
                gain: AtomicU32::new(f32_bits(1.0)),
                start: AtomicU64::new(0),
                end: AtomicU64::new(0),
                track: AtomicU32::new(u32::MAX),
                until: AtomicU64::new(u64::MAX),
            },
            clock: Clock {
                from: AtomicU64::new(0),
                kind: AtomicU32::new(0),
                a: AtomicU64::new(0),
                b: AtomicU64::new(0),
                session_rate: AtomicU32::new(0),
                out_rate: AtomicU32::new(0),
            },
        }
    }

    fn set_clock(&self, from: u64, jump: Option<Jump>, session_rate: u32, out_rate: u32) {
        let (kind, a, b) = match jump {
            None => (0, 0, 0),
            Some(Jump::Loop { start, end }) => (1, start, end),
            Some(Jump::Skip { start, end }) => (2, start, end),
        };
        self.clock.a.store(a, Ordering::Relaxed);
        self.clock.b.store(b, Ordering::Relaxed);
        self.clock.kind.store(kind, Ordering::Relaxed);
        self.clock.from.store(from, Ordering::Relaxed);
        self.clock
            .session_rate
            .store(session_rate, Ordering::Relaxed);
        self.clock.out_rate.store(out_rate, Ordering::Relaxed);
    }

    /// Routed mixes: hands new levels to the render thread.
    fn send_levels(&self, levels: Levels) {
        if let Ok(mut g) = self.levels.lock() {
            *g = Some(levels);
        }
    }

    fn send_clip_gains(&self, gains: Vec<(Id, f32)>) {
        if let Ok(mut g) = self.clip_gains.lock() {
            *g = Some(gains);
        }
    }

    fn take_clip_gains(&self) -> Option<Vec<(Id, f32)>> {
        self.clip_gains.try_lock().ok().and_then(|mut g| g.take())
    }

    fn take_levels(&self) -> Option<Levels> {
        self.levels.try_lock().ok().and_then(|mut g| g.take())
    }

    /// Queues edited chains for the render thread, merged with any it
    /// has not picked up yet.
    fn send_chains(&self, update: ChainUpdate) {
        let Ok(mut g) = self.chains.lock() else {
            return;
        };
        let pending = g.get_or_insert_with(ChainUpdate::default);
        for (id, c) in update.tracks {
            pending.tracks.retain(|(i, _)| *i != id);
            pending.tracks.push((id, c));
        }
        for (id, c) in update.clips {
            pending.clips.retain(|(i, _)| *i != id);
            pending.clips.push((id, c));
        }
        for (id, c) in update.buses {
            pending.buses.retain(|(i, _)| *i != id);
            pending.buses.push((id, c));
        }
        if update.master.is_some() {
            pending.master = update.master;
        }
    }

    /// The render thread's side: swaps in waiting chains, if any.
    fn take_chains(&self) -> Option<ChainUpdate> {
        self.chains.try_lock().ok().and_then(|mut g| g.take())
    }

    fn push_meters(&self, mix: &mut Mix) {
        let peaks = mix.graph().take_meters();
        if let Ok(mut q) = self.chain_meters.lock() {
            q.push_back((mix.rendered(), peaks));
            while q.len() > METER_QUEUE {
                q.pop_front();
            }
        }
    }

    /// Render-thread peaks for the audio the device has played since the
    /// last call.
    fn played_meters(&self) -> GraphMeters {
        let played = self.played.load(Ordering::Relaxed);
        let mut out = GraphMeters::default();
        let Ok(mut q) = self.chain_meters.lock() else {
            return out;
        };
        let mut any = false;
        while q.front().is_some_and(|(at, _)| *at <= played) {
            let (_, peaks) = q.pop_front().expect("checked");
            out.merge(&peaks);
            any = true;
        }
        drop(q);
        let Ok(mut last) = self.last_played.lock() else {
            return out;
        };
        if any {
            *last = out.clone();
            out
        } else {
            last.clone()
        }
    }

    fn set_gains(&self, gains: &[[f32; 2]], master: f32) {
        for (slot, g) in self.gains.iter().zip(gains) {
            slot[0].store(f32_bits(g[0]), Ordering::Relaxed);
            slot[1].store(f32_bits(g[1]), Ordering::Relaxed);
        }
        self.master.store(f32_bits(master), Ordering::Relaxed);
    }
}

/// Where playback leaves the timeline and where it picks up again, in
/// session samples.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Jump {
    /// Loop Playback: play `start..end` over and over.
    Loop { start: u64, end: u64 },
    /// Skip Selection: play up to `start`, then carry on from `end`.
    Skip { start: u64, end: u64 },
}

impl Jump {
    /// Where playback really begins when asked to start at `from`:
    /// starting inside a skipped range starts after it.
    fn first(self, from: u64) -> u64 {
        match self {
            Jump::Skip { start, end } if from >= start && from < end => end,
            _ => from,
        }
    }

    /// The jump as (at, to), if it still lies ahead of `from`.
    fn points(self, from: u64) -> Option<(u64, u64)> {
        match self {
            Jump::Loop { start, end } if end > start && from < end => Some((end, start)),
            Jump::Skip { start, end } if end > start && from < start => Some((start, end)),
            _ => None,
        }
    }

    /// Timeline position after `elapsed` samples of playback from `from`.
    pub fn position(jump: Option<Jump>, from: u64, elapsed: u64) -> u64 {
        let linear = from + elapsed;
        let Some(j) = jump else { return linear };
        match (j, j.points(from)) {
            (Jump::Loop { start, end }, Some(_)) if linear >= end => {
                start + (linear - end) % (end - start)
            }
            (Jump::Skip { start, end }, Some(_)) if linear >= start => linear + (end - start),
            _ => linear,
        }
    }
}

struct Running {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
    /// Session sample where playback started.
    start: u64,
    jump: Option<Jump>,
    session_rate: u32,
    out_rate: u32,
    layout: plan::Layout,
    track_ids: Vec<Id>,
    bus_ids: Vec<Id>,
    /// The whole mix runs on the render thread.
    routed: bool,
    /// Fingerprints of the chains playing, and the chains themselves.
    chains: plan::Chains,
    chain_docs: HashMap<(Id, Owner), EffectChain>,
}

impl Running {
    fn position(&self) -> u64 {
        let elapsed = plan::to_session(
            self.shared.played.load(Ordering::Relaxed),
            self.session_rate,
            self.out_rate,
        );
        Jump::position(self.jump, self.start, elapsed)
    }

    fn stop(&mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Peak levels since the last status call, 0 to 1 (and above on overs).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransportStatus {
    pub playing: bool,
    /// Session samples.
    pub position: u64,
    pub track_ids: Vec<Id>,
    pub meters: Vec<[f32; 2]>,
    pub master: [f32; 2],
    pub underruns: u64,
    /// Per track, its Voice Chain's peaks in and out (left in, right
    /// in, left out, right out) for the audio just heard; null for a
    /// track whose chain does nothing.
    pub chain_meters: Vec<Option<[f32; 4]>>,
    /// Buses, in session order, with their levels after the fader and
    /// their chains' peaks. Empty unless the mix is routed.
    pub bus_ids: Vec<Id>,
    pub bus_meters: Vec<[f32; 2]>,
    pub bus_chain_meters: Vec<Option<[f32; 4]>>,
    pub master_chain_meter: Option<[f32; 4]>,
    /// 3D Space sessions: the master's speaker channels, after its fader,
    /// in the layout's channel order. Empty otherwise.
    pub bed_meters: Vec<f32>,
}

#[derive(Default)]
pub struct Transport {
    current: Mutex<Option<Running>>,
}

impl Transport {
    /// Starts playing `session` from session sample `from`, looping or
    /// skipping as `jump` says.
    pub fn play(&self, session: &Session, from: u64, jump: Option<Jump>) -> Result<(), String> {
        let mut cur = self.current.lock().map_err(|e| e.to_string())?;
        if let Some(mut r) = cur.take() {
            r.stop();
        }
        let session = super::speaker_feed::expanded(session).into_owned();
        *cur = Some(start(session, from, jump)?);
        Ok(())
    }

    /// Stops and returns the position reached, if it was playing.
    pub fn stop(&self) -> Option<u64> {
        let mut cur = self.current.lock().ok()?;
        let mut r = cur.take()?;
        let pos = r.position();
        r.stop();
        Some(pos)
    }

    pub fn is_playing(&self) -> bool {
        self.current.lock().map(|c| c.is_some()).unwrap_or(false)
    }

    pub fn status(&self) -> TransportStatus {
        let idle = TransportStatus {
            playing: false,
            position: 0,
            track_ids: Vec::new(),
            meters: Vec::new(),
            master: [0.0, 0.0],
            underruns: 0,
            chain_meters: Vec::new(),
            bus_ids: Vec::new(),
            bus_meters: Vec::new(),
            bus_chain_meters: Vec::new(),
            master_chain_meter: None,
            bed_meters: Vec::new(),
        };
        let Ok(cur) = self.current.lock() else {
            return idle;
        };
        let Some(r) = cur.as_ref() else {
            return idle;
        };
        let take = |a: &AtomicU32| bits_f32(a.swap(0, Ordering::Relaxed));
        let mut g = r.shared.played_meters();
        let n = r.track_ids.len();
        let meters = if r.routed {
            g.tracks.resize(n, [0.0; 2]);
            std::mem::take(&mut g.tracks)
        } else {
            r.shared
                .meters
                .iter()
                .map(|m| [take(&m[0]), take(&m[1])])
                .collect()
        };
        g.track_chains.resize(n, None);
        g.buses.resize(r.bus_ids.len(), [0.0; 2]);
        g.bus_chains.resize(r.bus_ids.len(), None);
        TransportStatus {
            playing: true,
            position: r.position(),
            track_ids: r.track_ids.clone(),
            meters,
            master: [
                take(&r.shared.master_meter[0]),
                take(&r.shared.master_meter[1]),
            ],
            underruns: r.shared.underruns.load(Ordering::Relaxed),
            chain_meters: g.track_chains,
            bus_ids: r.bus_ids.clone(),
            bus_meters: g.buses,
            bus_chain_meters: g.bus_chains,
            master_chain_meter: g.master_chain,
            bed_meters: {
                let m = bits_f32(r.shared.master.load(Ordering::Relaxed));
                g.bed.iter().map(|v| v * m).collect()
            },
        }
    }

    /// Brings playback in line with an edited session: new gains apply
    /// at once, edited chains swap in; changes that move audio, or a
    /// chain's look-ahead, restart from the same spot.
    /// Hears `gain` over `start..end` of the timeline (session samples)
    /// from now on, before any edit is made; 1.0 ends it.
    pub fn audition(&self, gain: f32, start: u64, end: u64) {
        if let Ok(cur) = self.current.lock() {
            if let Some(r) = cur.as_ref() {
                r.shared.audition.set(gain, start, end, u32::MAX);
            }
        }
    }

    /// Hears `gain` on one track only, over `start..end` of the timeline:
    /// a clip's level while its handle is dragged. The edit made on
    /// release takes over seamlessly (see [`Audition::until`]). Not
    /// possible when the mix is routed (one stream for everything); the
    /// edit is heard a moment later then.
    /// False when it cannot (not playing, or routed).
    pub fn audition_track(&self, track: Id, gain: f32, start: u64, end: u64) -> bool {
        let Ok(cur) = self.current.lock() else {
            return false;
        };
        let Some(r) = cur.as_ref().filter(|r| !r.routed) else {
            return false;
        };
        let Some(t) = r.track_ids.iter().position(|&i| i == track) else {
            return false;
        };
        r.shared.audition.set(gain, start, end, t as u32);
        true
    }

    pub fn sync(&self, session: &Session) -> Result<(), String> {
        // As it plays: multichannel clips' channels on their speakers.
        let session = &*super::speaker_feed::expanded(session);
        let mut cur = self.current.lock().map_err(|e| e.to_string())?;
        let Some(r) = cur.as_mut() else {
            return Ok(());
        };
        if r.layout == plan::layout(session) && r.session_rate == session.sample_rate {
            let master = plan::master_gain(session);
            if r.routed {
                r.shared.set_gains(&[[1.0, 1.0]], master);
            } else {
                r.shared.set_gains(&plan::gains(session), master);
            }
            // Routed: every level. Plain: the graph still folds tracks to mono.
            r.shared.send_levels(graph::levels(session));
            r.shared.send_clip_gains(plan::clip_gains(session));
            let now = plan::chains(session);
            if now == r.chains {
                return Ok(());
            }
            if let Some(update) = chain_update(r, session, &now) {
                for (id, c) in update.tracks.iter() {
                    r.chain_docs.insert((*id, Owner::Track), c.clone());
                }
                for (id, c) in update.clips.iter() {
                    r.chain_docs.insert((*id, Owner::Clip), c.clone());
                }
                for (id, c) in update.buses.iter() {
                    r.chain_docs.insert((*id, Owner::Bus), c.clone());
                }
                if let Some(c) = &update.master {
                    r.chain_docs.insert((Id::nil(), Owner::Master), c.clone());
                }
                r.shared.send_chains(update);
                r.chains = now;
                return Ok(());
            }
        }
        let mut pos = r.position();
        if r.session_rate != session.sample_rate {
            pos = plan::to_out(pos, r.session_rate, session.sample_rate);
        }
        let jump = r.jump;
        let mut old = cur.take().expect("checked above");
        old.stop();
        *cur = Some(start(session.clone(), pos, jump)?);
        Ok(())
    }
}

/// The chains that changed, when every one keeps its look-ahead (so it
/// can swap in while playing). None means playback must restart.
fn chain_update(r: &Running, session: &Session, now: &plan::Chains) -> Option<ChainUpdate> {
    let latency = |c: &EffectChain| crate::dsp::effects::chain_latency(c, r.out_rate);
    let empty = EffectChain::default();
    let mut update = ChainUpdate::default();
    for &(id, owner, hash) in now {
        if r.chains
            .iter()
            .any(|&(i, o, h)| i == id && o == owner && h == hash)
        {
            continue;
        }
        let new = plan::chain_of(session, id, owner)?;
        let old = r.chain_docs.get(&(id, owner)).unwrap_or(&empty);
        if latency(old) != latency(new) {
            return None;
        }
        match owner {
            Owner::Track => update.tracks.push((id, new.clone())),
            Owner::Clip => update.clips.push((id, new.clone())),
            Owner::Bus => update.buses.push((id, new.clone())),
            Owner::Master => update.master = Some(new.clone()),
        }
    }
    Some(update)
}

fn chain_docs(session: &Session) -> HashMap<(Id, Owner), EffectChain> {
    let mut m = HashMap::new();
    for t in &session.tracks {
        m.insert((t.id, Owner::Track), t.effects.clone());
        for c in &t.clips {
            m.insert((c.id, Owner::Clip), c.effects.clone());
        }
    }
    for b in &session.buses {
        m.insert((b.id, Owner::Bus), b.effects.clone());
    }
    m.insert((Id::nil(), Owner::Master), session.master.effects.clone());
    m
}

impl Drop for Transport {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

fn start(session: Session, from: u64, jump: Option<Jump>) -> Result<Running, String> {
    let from = jump.map_or(from, |j| j.first(from));
    let jump = jump.filter(|j| j.points(from).is_some());
    let routed = crate::session::routing::is_routed(&session);
    let master = plan::master_gain(&session);
    let layout = plan::layout(&session);
    let chains = plan::chains(&session);
    let docs = chain_docs(&session);
    let track_ids: Vec<Id> = session.tracks.iter().map(|t| t.id).collect();
    let bus_ids: Vec<Id> = if routed {
        session.buses.iter().map(|b| b.id).collect()
    } else {
        Vec::new()
    };
    let session_rate = session.sample_rate;
    let has_plugins = !crate::commands::plugins::playing_plugin_slots(&session).is_empty();
    // Routed: one ring for the whole mix, at unity before the master fader.
    let (rings, gains) = if routed {
        (1, vec![[1.0, 1.0]])
    } else {
        (track_ids.len().max(1), plan::gains(&session))
    };
    let shared = Arc::new(Shared::new(rings, &gains, master));

    let (ready_tx, ready_rx) = mpsc::channel::<Result<u32, String>>();
    let thread_shared = shared.clone();
    let thread = thread::Builder::new()
        .name("transport".into())
        .spawn(move || run(session, from, jump, thread_shared, ready_tx))
        .map_err(|e| e.to_string())?;
    // Plug-ins and instruments may still be loading (licence checks,
    // samples): give them longer than a plain session needs.
    let wait = if has_plugins { 30 } else { 5 };
    let out_rate = match ready_rx.recv_timeout(Duration::from_secs(wait)) {
        Ok(Ok(rate)) => rate,
        Ok(Err(e)) => {
            let _ = thread.join();
            return Err(e);
        }
        Err(_) => {
            shared.stop.store(true, Ordering::SeqCst);
            return Err("The audio device did not respond.".into());
        }
    };
    shared.set_clock(from, jump, session_rate, out_rate);
    Ok(Running {
        shared,
        thread: Some(thread),
        start: from,
        jump,
        session_rate,
        out_rate,
        layout,
        track_ids,
        bus_ids,
        routed,
        chains,
        chain_docs: docs,
    })
}

fn run(
    mut session: Session,
    from: u64,
    jump: Option<Jump>,
    shared: Arc<Shared>,
    ready: mpsc::Sender<Result<u32, String>>,
) {
    let Some(device) = super::output::output_device_any() else {
        let _ = ready.send(Err("No audio output device found.".into()));
        return;
    };
    // Which outputs the mix plays on. 3D Space sessions heard on
    // speakers send each speaker to its own output.
    let omap = super::output::map();
    let stereo_out = omap.stereo.map(usize::from);
    let bed_out: Option<Vec<usize>> = session
        .space
        .filter(|sp| sp.listen == crate::session::model::SpaceListen::Speakers)
        .map(|sp| {
            crate::dsp::space::speakers(sp.layout)
                .iter()
                .enumerate()
                .map(|(k, s)| omap.speaker(s.name, k))
                .collect()
        });
    let wanted = match &bed_out {
        Some(m) => m.iter().max().map_or(2, |&o| o + 1),
        None => stereo_out[0].max(stereo_out[1]) + 1,
    };
    let supported = match super::output::config_for(&device, wanted) {
        Some(c) => c,
        None => {
            let _ = ready.send(Err("The audio device is not available.".into()));
            return;
        }
    };
    let format = supported.sample_format();
    let config: StreamConfig = supported.config();
    let out_rate = config.sample_rate;
    super::scope::set_rate(out_rate);
    let channels = usize::from(config.channels);

    if session.tracks.is_empty() {
        // Keep the clock running with one silent track.
        session.tracks.push(crate::session::model::Track::new(
            "",
            crate::session::model::ChannelLayout::Stereo,
        ));
    }
    let routed = crate::session::routing::is_routed(&session);
    let start_frame = plan::to_out(from, session.sample_rate, out_rate);
    let mut renderer = Renderer::new(plan::build(&session, out_rate), start_frame);
    if let Some((at, to)) = jump.and_then(|j| j.points(from)) {
        let r = session.sample_rate;
        renderer = renderer.with_jump(plan::to_out(at, r, out_rate), plan::to_out(to, r, out_rate));
    }
    let mut mix = Mix::new(renderer, Graph::new(&session, out_rate, routed));
    let rings = mix.outputs();
    // The metronome, on its own ring so faders and meters leave it alone.
    let mut click = Ticker {
        click: Click::new(&session.tempo, out_rate),
        start: start_frame,
        jump: jump.map(|j| {
            let r = session.sample_rate;
            match j {
                Jump::Loop { start, end } => Jump::Loop {
                    start: plan::to_out(start, r, out_rate),
                    end: plan::to_out(end, r, out_rate),
                },
                Jump::Skip { start, end } => Jump::Skip {
                    start: plan::to_out(start, r, out_rate),
                    end: plan::to_out(end, r, out_rate),
                },
            }
        }),
        buf: Vec::with_capacity(BLOCK * 2),
    };

    let cap = ((f64::from(out_rate) * AHEAD_SECONDS) as usize).max(BLOCK * 2) * 2;
    let mut producers = Vec::with_capacity(rings);
    let mut consumers = Vec::with_capacity(rings);
    for _ in 0..rings {
        let (p, c) = RingBuffer::<f32>::new(cap);
        producers.push(p);
        consumers.push(c);
    }
    let (mut click_out, click_in) = RingBuffer::<f32>::new(cap);
    let nb = bed_out.as_ref().map_or(0, Vec::len);
    let (mut bed_prod, bed_in) = if nb > 0 {
        let (p, c) = RingBuffer::<f32>::new(cap / 2 * nb);
        (Some(p), Some(c))
    } else {
        (None, None)
    };
    let mut routing = Routing {
        stereo: stereo_out,
        bed: bed_in.zip(bed_out),
        frame: vec![0.0; nb],
    };

    // Fill ahead before the device starts, so playback starts clean.
    let mut blocks: Vec<Vec<f32>> = vec![Vec::with_capacity(BLOCK * 2); rings];
    while has_room(&producers, &click_out, bed_prod.as_ref(), nb) {
        render_block(
            &mut mix,
            &mut blocks,
            &mut producers,
            &mut click,
            &mut click_out,
            bed_prod.as_mut(),
        );
        shared.push_meters(&mut mix);
    }

    let render_shared = shared.clone();
    let render = thread::Builder::new()
        .name("transport-render".into())
        .spawn(move || {
            render_loop(
                mix,
                producers,
                blocks,
                click,
                click_out,
                bed_prod,
                render_shared,
            )
        });
    let render = match render {
        Ok(r) => r,
        Err(e) => {
            let _ = ready.send(Err(e.to_string()));
            return;
        }
    };

    let stream = match format {
        SampleFormat::F32 => build::<f32>(
            &device,
            config,
            consumers,
            click_in,
            shared.clone(),
            channels,
            std::mem::take(&mut routing),
        ),
        SampleFormat::I16 => build::<i16>(
            &device,
            config,
            consumers,
            click_in,
            shared.clone(),
            channels,
            std::mem::take(&mut routing),
        ),
        SampleFormat::I32 => build::<i32>(
            &device,
            config,
            consumers,
            click_in,
            shared.clone(),
            channels,
            std::mem::take(&mut routing),
        ),
        SampleFormat::U16 => build::<u16>(
            &device,
            config,
            consumers,
            click_in,
            shared.clone(),
            channels,
            std::mem::take(&mut routing),
        ),
        other => Err(format!("Unsupported output sample format: {other}")),
    };
    let stream = match stream.and_then(|s| s.play().map(|_| s).map_err(|e| e.to_string())) {
        Ok(s) => s,
        Err(e) => {
            shared.stop.store(true, Ordering::SeqCst);
            let _ = render.join();
            let _ = ready.send(Err(format!("Could not start playback: {e}")));
            return;
        }
    };
    let _ = ready.send(Ok(out_rate));

    while !shared.stop.load(Ordering::SeqCst) {
        thread::sleep(Duration::from_millis(15));
    }
    drop(stream);
    let _ = render.join();
}

fn push_all(producers: &mut [Producer<f32>], blocks: &[Vec<f32>]) {
    for (p, b) in producers.iter_mut().zip(blocks) {
        let (_, left) = p.push_partial_slice(b);
        debug_assert!(left.is_empty(), "room was checked first");
    }
}

/// The metronome for the frames the mix hands out, at their timeline
/// position (output frames), through loops and skips.
struct Ticker {
    click: Click,
    start: u64,
    jump: Option<Jump>,
    buf: Vec<f32>,
}

impl Ticker {
    /// Frames from `linear` (start plus frames played) to the next jump.
    fn until_jump(&self, linear: u64) -> u64 {
        match self.jump.filter(|j| j.points(self.start).is_some()) {
            Some(Jump::Loop { start, end }) if linear >= end => {
                let len = end - start;
                len - (linear - end) % len
            }
            Some(Jump::Loop { end, .. }) => end - linear,
            Some(Jump::Skip { start, .. }) if linear < start => start - linear,
            _ => u64::MAX,
        }
    }

    /// The click for the `frames` frames after `rendered` frames played.
    fn render(&mut self, rendered: u64, frames: usize) {
        self.buf.clear();
        self.buf.resize(frames * 2, 0.0);
        let mut done = 0;
        while done < frames {
            let pos = Jump::position(self.jump, self.start, rendered + done as u64);
            // Stop short where the timeline jumps.
            let left = self.until_jump(self.start + rendered + done as u64);
            let n = (frames - done).min(usize::try_from(left).unwrap_or(usize::MAX).max(1));
            self.click
                .render(pos, &mut self.buf[done * 2..(done + n) * 2]);
            done += n;
        }
    }
}

/// Room in every queue for one more block.
fn has_room(
    producers: &[Producer<f32>],
    click_out: &Producer<f32>,
    bed: Option<&Producer<f32>>,
    nb: usize,
) -> bool {
    producers.iter().all(|p| p.slots() >= BLOCK * 2)
        && click_out.slots() >= BLOCK * 2
        && bed.is_none_or(|b| b.slots() >= BLOCK * nb)
}

/// Where the device's outputs get the mix from: the stereo mix's left and
/// right outputs, or each speaker of the bed on its own output.
#[derive(Default)]
struct Routing {
    stereo: [usize; 2],
    /// The bed's queue and each speaker's output.
    bed: Option<(Consumer<f32>, Vec<usize>)>,
    /// One frame of the bed, scratch for the audio thread.
    frame: Vec<f32>,
}

impl Routing {
    #[cfg(test)]
    fn stereo() -> Self {
        Self {
            stereo: [0, 1],
            ..Self::default()
        }
    }
}

/// Renders one block of the mix and its click, and queues both (and the
/// speaker bed when it plays on its own outputs).
fn render_block(
    mix: &mut Mix,
    blocks: &mut [Vec<f32>],
    producers: &mut [Producer<f32>],
    click: &mut Ticker,
    click_out: &mut Producer<f32>,
    bed: Option<&mut Producer<f32>>,
) {
    let rendered = mix.rendered();
    let n = mix.render(BLOCK, blocks);
    push_all(producers, blocks);
    if let Some(b) = bed {
        let (_, left) = b.push_partial_slice(mix.graph().bed());
        debug_assert!(left.is_empty(), "room was checked first");
    }
    click.render(rendered, n);
    let (_, left) = click_out.push_partial_slice(&click.buf);
    debug_assert!(left.is_empty(), "room was checked first");
}

fn render_loop(
    mut mix: Mix,
    mut producers: Vec<Producer<f32>>,
    mut blocks: Vec<Vec<f32>>,
    mut click: Ticker,
    mut click_out: Producer<f32>,
    mut bed: Option<Producer<f32>>,
    shared: Arc<Shared>,
) {
    let nb = mix.graph().bed_channels();
    while !shared.stop.load(Ordering::SeqCst) {
        if let Some(update) = shared.take_chains() {
            // Checked by the caller: every chain here keeps its look-ahead.
            mix.set_chains(&update);
        }
        if let Some(levels) = shared.take_levels() {
            mix.graph().set_levels(levels);
        }
        if let Some(gains) = shared.take_clip_gains() {
            mix.set_clip_gains(&gains);
            // A track's audition ends where audio with the edited level begins.
            if shared.audition.track.load(Ordering::Relaxed) != u32::MAX {
                shared
                    .audition
                    .until
                    .fetch_min(mix.rendered(), Ordering::Relaxed);
            }
        }
        if has_room(&producers, &click_out, bed.as_ref(), nb) {
            render_block(
                &mut mix,
                &mut blocks,
                &mut producers,
                &mut click,
                &mut click_out,
                bed.as_mut(),
            );
            shared.push_meters(&mut mix);
        } else {
            thread::sleep(Duration::from_millis(5));
        }
    }
}

fn build<T>(
    device: &cpal::Device,
    config: StreamConfig,
    mut consumers: Vec<Consumer<f32>>,
    mut click: Consumer<f32>,
    shared: Arc<Shared>,
    channels: usize,
    mut routing: Routing,
) -> Result<cpal::Stream, String>
where
    T: SizedSample + FromSample<f32>,
{
    // Scratch for the audio thread, allocated here, never in the callback.
    let n = consumers.len();
    let mut gains = vec![[0f32; 2]; n];
    let mut peaks = vec![[0f32; 2]; n];
    device
        .build_output_stream(
            config,
            move |out: &mut [T], _| {
                mix(
                    out,
                    channels,
                    &mut consumers,
                    &mut click,
                    &shared,
                    &mut gains,
                    &mut peaks,
                    &mut routing,
                );
            },
            |e| eprintln!("transport stream error: {e}"),
            None,
        )
        .map_err(|e| e.to_string())
}

/// The audio thread's whole job: no allocation, no locks, no I/O.
#[allow(clippy::too_many_arguments)]
fn mix<T>(
    out: &mut [T],
    channels: usize,
    consumers: &mut [Consumer<f32>],
    click: &mut Consumer<f32>,
    shared: &Shared,
    gains: &mut [[f32; 2]],
    peaks: &mut [[f32; 2]],
    routing: &mut Routing,
) where
    T: SizedSample + FromSample<f32>,
{
    let channels = channels.max(1);
    let frames = out.len() / channels;
    let avail = consumers
        .iter()
        .map(|c| c.slots() / 2)
        .min()
        .unwrap_or(0)
        .min(click.slots() / 2)
        .min(
            routing
                .bed
                .as_ref()
                .map_or(usize::MAX, |(c, m)| c.slots() / m.len().max(1)),
        );
    let n = avail.min(frames);
    for (g, a) in gains.iter_mut().zip(&shared.gains) {
        g[0] = bits_f32(a[0].load(Ordering::Relaxed));
        g[1] = bits_f32(a[1].load(Ordering::Relaxed));
    }
    for p in peaks.iter_mut() {
        *p = [0.0, 0.0];
    }
    let master = bits_f32(shared.master.load(Ordering::Relaxed));
    let mono = MONO_CHECK.load(Ordering::Relaxed);
    // A level being auditioned over part of the timeline.
    let aud_gain = bits_f32(shared.audition.gain.load(Ordering::Relaxed));
    let auditioning = aud_gain != 1.0;
    let (aud_start, aud_end) = (
        shared.audition.start.load(Ordering::Relaxed),
        shared.audition.end.load(Ordering::Relaxed),
    );
    let aud_track = shared.audition.track.load(Ordering::Relaxed) as usize;
    let aud_until = shared.audition.until.load(Ordering::Relaxed);
    let played0 = shared.played.load(Ordering::Relaxed);
    let mut mpeak = [0f32; 2];
    for f in 0..n {
        let mut l = 0f32;
        let mut r = 0f32;
        // This frame's audition: on every track, on one, or none.
        let aud = auditioning && played0 + (f as u64) < aud_until && {
            let at = shared.clock.at(played0 + f as u64);
            at >= aud_start && at < aud_end
        };
        for (t, c) in consumers.iter_mut().enumerate() {
            let k = if aud && t == aud_track { aud_gain } else { 1.0 };
            let sl = c.pop().unwrap_or(0.0) * gains[t][0] * k;
            let sr = c.pop().unwrap_or(0.0) * gains[t][1] * k;
            peaks[t][0] = peaks[t][0].max(sl.abs());
            peaks[t][1] = peaks[t][1].max(sr.abs());
            l += sl;
            r += sr;
        }
        l *= master;
        r *= master;
        if aud && aud_track == u32::MAX as usize {
            l *= aud_gain;
            r *= aud_gain;
        }
        if mono {
            let m = (l + r) * 0.5;
            l = m;
            r = m;
        }
        mpeak[0] = mpeak[0].max(l.abs());
        mpeak[1] = mpeak[1].max(r.abs());
        super::scope::push(l, r);
        // The metronome after the master fader, meters and scope.
        let ck = [click.pop().unwrap_or(0.0), click.pop().unwrap_or(0.0)];
        l += ck[0];
        r += ck[1];
        let frame = &mut out[f * channels..(f + 1) * channels];
        for s in frame.iter_mut() {
            *s = T::from_sample(0.0f32);
        }
        match routing.bed.as_mut() {
            // Each speaker on its own output; the click on the stereo pair.
            Some((bed, map)) => {
                for (k, &o) in map.iter().enumerate() {
                    let v = bed.pop().unwrap_or(0.0) * master;
                    routing.frame[k] = v;
                    if let Some(s) = frame.get_mut(o) {
                        *s = T::from_sample(v);
                    }
                }
                let [ol, or] = routing.stereo;
                for (o, v) in [(ol, ck[0]), (or, ck[1])] {
                    if v != 0.0 {
                        if let Some(s) = frame.get_mut(o) {
                            let speaker = map
                                .iter()
                                .position(|&m| m == o)
                                .map_or(0.0, |k| routing.frame[k]);
                            *s = T::from_sample(speaker + v);
                        }
                    }
                }
            }
            None => {
                let [ol, or] = routing.stereo;
                if channels == 1 {
                    frame[0] = T::from_sample(l);
                } else {
                    if let Some(s) = frame.get_mut(ol) {
                        *s = T::from_sample(l);
                    }
                    if let Some(s) = frame.get_mut(or) {
                        *s = T::from_sample(r);
                    }
                }
            }
        }
    }
    for s in out[n * channels..].iter_mut() {
        *s = T::from_sample(0.0f32);
    }
    if n < frames {
        shared.underruns.fetch_add(1, Ordering::Relaxed);
    }
    for (m, p) in shared.meters.iter().zip(peaks.iter()) {
        m[0].fetch_max(f32_bits(p[0]), Ordering::Relaxed);
        m[1].fetch_max(f32_bits(p[1]), Ordering::Relaxed);
    }
    shared.master_meter[0].fetch_max(f32_bits(mpeak[0]), Ordering::Relaxed);
    shared.master_meter[1].fetch_max(f32_bits(mpeak[1]), Ordering::Relaxed);
    shared.played.fetch_add(n as u64, Ordering::Relaxed);
}

#[cfg(test)]
mod tests;
