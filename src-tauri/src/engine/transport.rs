// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

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
    /// Render-thread meters, oldest first, released as the device plays them.
    chain_meters: Mutex<VecDeque<BlockPeaks>>,
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
            chain_meters: Mutex::new(VecDeque::new()),
        }
    }

    /// Routed mixes: hands new levels to the render thread.
    fn send_levels(&self, levels: Levels) {
        if let Ok(mut g) = self.levels.lock() {
            *g = Some(levels);
        }
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
        while q.front().is_some_and(|(at, _)| *at <= played) {
            let (_, peaks) = q.pop_front().expect("checked");
            out.merge(&peaks);
        }
        out
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
        *cur = Some(start(session.clone(), from, jump)?);
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
        }
    }

    /// Brings playback in line with an edited session: new gains apply
    /// at once, edited chains swap in; changes that move audio, or a
    /// chain's look-ahead, restart from the same spot.
    pub fn sync(&self, session: &Session) -> Result<(), String> {
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
    let out_rate = match ready_rx.recv_timeout(Duration::from_secs(5)) {
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
    let supported = match device.default_output_config() {
        Ok(c) => c,
        Err(e) => {
            let _ = ready.send(Err(format!("The audio device is not available: {e}")));
            return;
        }
    };
    let format = supported.sample_format();
    let config: StreamConfig = supported.config();
    let out_rate = config.sample_rate;
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

    let cap = ((f64::from(out_rate) * AHEAD_SECONDS) as usize).max(BLOCK * 2) * 2;
    let mut producers = Vec::with_capacity(rings);
    let mut consumers = Vec::with_capacity(rings);
    for _ in 0..rings {
        let (p, c) = RingBuffer::<f32>::new(cap);
        producers.push(p);
        consumers.push(c);
    }

    // Fill ahead before the device starts, so playback starts clean.
    let mut blocks: Vec<Vec<f32>> = vec![Vec::with_capacity(BLOCK * 2); rings];
    while producers.iter().all(|p| p.slots() >= BLOCK * 2) {
        mix.render(BLOCK, &mut blocks);
        push_all(&mut producers, &blocks);
        shared.push_meters(&mut mix);
    }

    let render_shared = shared.clone();
    let render = thread::Builder::new()
        .name("transport-render".into())
        .spawn(move || render_loop(mix, producers, blocks, render_shared));
    let render = match render {
        Ok(r) => r,
        Err(e) => {
            let _ = ready.send(Err(e.to_string()));
            return;
        }
    };

    let stream = match format {
        SampleFormat::F32 => build::<f32>(&device, config, consumers, shared.clone(), channels),
        SampleFormat::I16 => build::<i16>(&device, config, consumers, shared.clone(), channels),
        SampleFormat::I32 => build::<i32>(&device, config, consumers, shared.clone(), channels),
        SampleFormat::U16 => build::<u16>(&device, config, consumers, shared.clone(), channels),
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

fn render_loop(
    mut mix: Mix,
    mut producers: Vec<Producer<f32>>,
    mut blocks: Vec<Vec<f32>>,
    shared: Arc<Shared>,
) {
    while !shared.stop.load(Ordering::SeqCst) {
        if let Some(update) = shared.take_chains() {
            // Checked by the caller: every chain here keeps its look-ahead.
            mix.set_chains(&update);
        }
        if let Some(levels) = shared.take_levels() {
            mix.graph().set_levels(levels);
        }
        if producers.iter().all(|p| p.slots() >= BLOCK * 2) {
            mix.render(BLOCK, &mut blocks);
            push_all(&mut producers, &blocks);
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
    shared: Arc<Shared>,
    channels: usize,
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
                    &shared,
                    &mut gains,
                    &mut peaks,
                );
            },
            |e| eprintln!("transport stream error: {e}"),
            None,
        )
        .map_err(|e| e.to_string())
}

/// The audio thread's whole job: no allocation, no locks, no I/O.
fn mix<T>(
    out: &mut [T],
    channels: usize,
    consumers: &mut [Consumer<f32>],
    shared: &Shared,
    gains: &mut [[f32; 2]],
    peaks: &mut [[f32; 2]],
) where
    T: SizedSample + FromSample<f32>,
{
    let channels = channels.max(1);
    let frames = out.len() / channels;
    let avail = consumers.iter().map(|c| c.slots() / 2).min().unwrap_or(0);
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
    let mut mpeak = [0f32; 2];
    for f in 0..n {
        let mut l = 0f32;
        let mut r = 0f32;
        for (t, c) in consumers.iter_mut().enumerate() {
            let sl = c.pop().unwrap_or(0.0) * gains[t][0];
            let sr = c.pop().unwrap_or(0.0) * gains[t][1];
            peaks[t][0] = peaks[t][0].max(sl.abs());
            peaks[t][1] = peaks[t][1].max(sr.abs());
            l += sl;
            r += sr;
        }
        l *= master;
        r *= master;
        if mono {
            let m = (l + r) * 0.5;
            l = m;
            r = m;
        }
        mpeak[0] = mpeak[0].max(l.abs());
        mpeak[1] = mpeak[1].max(r.abs());
        super::scope::push(l, r);
        let frame = &mut out[f * channels..(f + 1) * channels];
        frame[0] = T::from_sample(l);
        if channels > 1 {
            frame[1] = T::from_sample(r);
        }
        for s in frame.iter_mut().skip(2) {
            *s = T::from_sample(0.0f32);
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
mod tests {
    use super::Jump;

    #[test]
    fn position_follows_loops_and_skips() {
        let lp = Some(Jump::Loop {
            start: 100,
            end: 200,
        });
        assert_eq!(Jump::position(lp, 50, 40), 90);
        assert_eq!(Jump::position(lp, 50, 150), 100);
        assert_eq!(Jump::position(lp, 50, 275), 125);
        // Starting after the loop plays straight on.
        assert_eq!(Jump::position(lp, 300, 10), 310);
        let sk = Some(Jump::Skip {
            start: 100,
            end: 200,
        });
        assert_eq!(Jump::position(sk, 50, 40), 90);
        assert_eq!(Jump::position(sk, 50, 60), 210);
        assert_eq!(Jump::position(None, 50, 60), 110);
        // Starting inside a skipped range starts after it.
        assert_eq!(
            Jump::Skip {
                start: 100,
                end: 200
            }
            .first(150),
            200
        );
    }

    use super::*;

    #[test]
    fn mix_applies_gains_meters_and_counts_frames() {
        let shared = Shared::new(2, &[[1.0, 0.5], [0.0, 0.0]], 1.0);
        let (mut p0, c0) = RingBuffer::<f32>::new(16);
        let (mut p1, c1) = RingBuffer::<f32>::new(16);
        for v in [0.5, 0.5, -0.25, -0.25] {
            p0.push(v).unwrap();
        }
        for v in [0.9, 0.9, 0.9, 0.9] {
            p1.push(v).unwrap();
        }
        let mut consumers = vec![c0, c1];
        let mut gains = vec![[0.0; 2]; 2];
        let mut peaks = vec![[0.0; 2]; 2];
        let mut out = [9.0f32; 6];
        mix(&mut out, 2, &mut consumers, &shared, &mut gains, &mut peaks);
        // Two frames mixed, the third is silence because the rings ran dry.
        assert_eq!(out, [0.5, 0.25, -0.25, -0.125, 0.0, 0.0]);
        assert_eq!(shared.played.load(Ordering::Relaxed), 2);
        assert_eq!(shared.underruns.load(Ordering::Relaxed), 1);
        assert_eq!(bits_f32(shared.meters[0][0].load(Ordering::Relaxed)), 0.5);
        assert_eq!(
            bits_f32(shared.meters[1][0].load(Ordering::Relaxed)),
            0.0,
            "muted track shows no level"
        );
        assert_eq!(
            bits_f32(shared.master_meter[1].load(Ordering::Relaxed)),
            0.25
        );
    }

    #[test]
    fn gain_updates_apply_without_restart() {
        let shared = Shared::new(1, &[[1.0, 1.0]], 1.0);
        shared.set_gains(&[[0.25, 0.0]], 2.0);
        let (mut p, c) = RingBuffer::<f32>::new(4);
        p.push(1.0).unwrap();
        p.push(1.0).unwrap();
        let mut consumers = vec![c];
        let mut out = [0f32; 2];
        mix(
            &mut out,
            2,
            &mut consumers,
            &shared,
            &mut [[0.0; 2]],
            &mut [[0.0; 2]],
        );
        assert_eq!(out, [0.5, 0.0]);
    }

    #[test]
    fn mono_check_folds_what_plays() {
        let shared = Shared::new(1, &[[1.0, 1.0]], 1.0);
        let (mut p, c) = RingBuffer::<f32>::new(4);
        p.push(1.0).unwrap();
        p.push(-0.5).unwrap();
        let mut consumers = vec![c];
        let mut out = [0f32; 2];
        set_mono_check(true);
        mix(
            &mut out,
            2,
            &mut consumers,
            &shared,
            &mut [[0.0; 2]],
            &mut [[0.0; 2]],
        );
        set_mono_check(false);
        assert_eq!(out, [0.25, 0.25]);
    }

    #[test]
    fn extra_device_channels_stay_silent() {
        let shared = Shared::new(1, &[[1.0, 1.0]], 1.0);
        let (mut p, c) = RingBuffer::<f32>::new(4);
        p.push(0.3).unwrap();
        p.push(0.6).unwrap();
        let mut out = [7f32; 4];
        mix(
            &mut out,
            4,
            &mut [c],
            &shared,
            &mut [[0.0; 2]],
            &mut [[0.0; 2]],
        );
        assert_eq!(out, [0.3, 0.6, 0.0, 0.0]);
    }

    #[test]
    fn render_meters_wait_for_the_device_to_play_them() {
        let shared = Shared::new(2, &[[1.0, 1.0], [1.0, 1.0]], 1.0);
        let reading = |c: [f32; 4], bus: f32| GraphMeters {
            track_chains: vec![Some(c), None],
            buses: vec![[bus, bus]],
            ..GraphMeters::default()
        };
        shared.chain_meters.lock().unwrap().extend([
            (100, reading([0.5, 0.4, 0.2, 0.1], 0.3)),
            (200, reading([0.9, 0.9, 0.3, 0.3], 0.1)),
        ]);
        assert_eq!(
            shared.played_meters(),
            GraphMeters::default(),
            "nothing played yet"
        );
        shared.played.store(150, Ordering::Relaxed);
        assert_eq!(shared.played_meters(), reading([0.5, 0.4, 0.2, 0.1], 0.3));
        shared.played.store(400, Ordering::Relaxed);
        let m = shared.played_meters();
        assert_eq!(m.track_chains[0], Some([0.9, 0.9, 0.3, 0.3]));
        assert_eq!(m.buses[0], [0.1, 0.1]);
        assert_eq!(
            shared.played_meters(),
            GraphMeters::default(),
            "each reading is used once"
        );
    }

    #[test]
    fn chain_updates_merge_until_picked_up() {
        let shared = Shared::new(1, &[[1.0, 1.0]], 1.0);
        let id = crate::session::model::new_id();
        let a = EffectChain {
            input_gain_db: 1.0,
            ..Default::default()
        };
        let b = EffectChain {
            input_gain_db: 2.0,
            ..Default::default()
        };
        shared.send_chains(ChainUpdate {
            tracks: vec![(id, a.clone())],
            master: Some(a),
            ..ChainUpdate::default()
        });
        shared.send_chains(ChainUpdate {
            tracks: vec![(id, b.clone())],
            buses: vec![(id, b.clone())],
            ..ChainUpdate::default()
        });
        let got = shared.take_chains().unwrap();
        assert_eq!(got.tracks.len(), 1);
        assert_eq!(got.tracks[0].1.input_gain_db, 2.0);
        assert_eq!(got.buses.len(), 1);
        assert_eq!(
            got.master.unwrap().input_gain_db,
            1.0,
            "kept until replaced"
        );
        assert!(shared.take_chains().is_none());
    }

    #[test]
    fn idle_status() {
        let t = Transport::default();
        assert!(!t.status().playing);
        assert_eq!(t.stop(), None);
    }
}
