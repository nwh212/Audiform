// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

//! The mix graph: everything between each track's clips and the master
//! fader. Runs on the render thread (or a mixdown's worker), after the
//! renderer, never on the audio device's thread.
//!
//! Per track: its Voice Chain, then its fader and pan, then its output
//! (the master or a bus) and its aux sends. Per bus: the sum of what
//! feeds it, its chain, fader, pan, output and sends. Then the master
//! chain. Tracks and buses run in routing order (see
//! `session::routing`), so a bus hears all of its inputs and a
//! sidechained effect hears its key for the same stretch of time.
//!
//! Look-ahead: every chain may delay its sound. Each route and key is
//! held back just enough that all of a bus's inputs (and the keys its
//! effects listen to) arrive in step, so the whole graph delays the mix
//! by one fixed amount, its `lead`. The renderer runs that far ahead of
//! the timeline and [`Mix`] primes the graph with the first `lead`
//! frames, so what comes out lines up with the timeline exactly.
//!
//! Two shapes:
//! - Plain sessions (every track straight to the master, no sends, no
//!   master chain, no sidechain): the graph runs only the track chains
//!   and hands each track out separately, and the audio thread applies
//!   faders, so they react within one buffer.
//! - Routed sessions: the graph mixes everything and hands out the
//!   master before its fader. Track and bus faders then lag by the
//!   render-ahead time; the master fader stays on the audio thread and
//!   is instant (decided 2026-10-20, see `docs/decisions.md`).
//!
//! Faders, pans, mutes, solos and send levels are live: new levels ramp
//! in over one block, with no restart. Chain edits swap in without a
//! restart while their look-ahead stays the same.

use std::collections::HashMap;
use std::sync::Arc;

use super::fx::{swap_fx, Fx};
use super::render::{ChainUpdate, Renderer};
use crate::dsp::effects::Delay;
use crate::session::model::{ChannelLayout, EffectChain, Id, Session, Tempo};
use crate::session::routing::{self, Node};

/// Levels for one track or bus, as left and right multipliers on the
/// sound after its chain: `main` for its output, then one per send.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct NodeLevel {
    pub main: [f32; 2],
    pub sends: Vec<[f32; 2]>,
    /// Fold left and right into one after the chain (the Mono button).
    pub mono: bool,
    /// 3D Space sessions: where it plays in the room, when it feeds the
    /// master.
    pub place: Option<Arc<Place>>,
}

/// A source's spot in a 3D Space session, worked out as gains: per side
/// (left, right), one per speaker for the bed, and stereo for listening.
#[derive(Debug, Clone, PartialEq)]
pub struct Place {
    pub bed: [Vec<f32>; 2],
    pub fold: [[f32; 2]; 2],
}

impl Place {
    fn new(
        s: &Session,
        pos: Option<&crate::session::model::SpacePos>,
        stereo: bool,
    ) -> Option<Arc<Place>> {
        let layout = s.space?.layout;
        let pos = pos.copied().unwrap_or_default().tidy();
        let [l, r] = crate::dsp::space::source_gains(layout, &pos, stereo);
        Some(Arc::new(Place {
            bed: [l, r],
            fold: crate::dsp::space::fold_gains(&pos, stereo),
        }))
    }
}

/// Every level in the graph. Changes apply live.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Levels {
    pub tracks: Vec<NodeLevel>,
    pub buses: Vec<NodeLevel>,
}

fn db(v: f64) -> f32 {
    10f64.powf(v.clamp(-96.0, 24.0) / 20.0) as f32
}

/// Pan as a balance: centered plays both sides at full level, and
/// panning turns the far side down, so a centered voice plays at
/// exactly the level the fader shows.
pub fn balance(pan: f64) -> [f32; 2] {
    let pan = (pan.clamp(-100.0, 100.0) / 100.0) as f32;
    let left = if pan > 0.0 { 1.0 - pan } else { 1.0 };
    let right = if pan < 0.0 { 1.0 + pan } else { 1.0 };
    [left, right]
}

/// Who can be heard, with mute and solo. With nothing soloed, everything
/// not muted. With a solo, a soloed track or bus is heard along with what
/// feeds it and the buses it feeds (so soloing a bus plays its tracks,
/// and soloing a track plays it through its bus and its reverb send).
pub fn audible(s: &Session) -> (Vec<bool>, Vec<bool>) {
    let soloed: Vec<Node> = s
        .tracks
        .iter()
        .enumerate()
        .filter(|(_, t)| t.solo)
        .map(|(i, _)| Node::Track(i))
        .chain(
            s.buses
                .iter()
                .enumerate()
                .filter(|(_, b)| b.solo)
                .map(|(i, _)| Node::Bus(i)),
        )
        .collect();
    let mut ok_t = vec![soloed.is_empty(); s.tracks.len()];
    let mut ok_b = vec![soloed.is_empty(); s.buses.len()];
    if !soloed.is_empty() {
        let routes = routing::routes(s);
        let walk = |forward: bool, ok_t: &mut Vec<bool>, ok_b: &mut Vec<bool>| {
            let mut stack = soloed.clone();
            let mut seen = Vec::new();
            while let Some(n) = stack.pop() {
                if seen.contains(&n) {
                    continue;
                }
                seen.push(n);
                match n {
                    Node::Track(i) => ok_t[i] = true,
                    Node::Bus(i) => ok_b[i] = true,
                }
                for (a, b) in &routes {
                    let (from, to) = if forward { (*a, *b) } else { (*b, *a) };
                    if from == n {
                        stack.push(to);
                    }
                }
            }
        };
        walk(true, &mut ok_t, &mut ok_b);
        walk(false, &mut ok_t, &mut ok_b);
    }
    let t = s
        .tracks
        .iter()
        .zip(ok_t)
        .map(|(t, ok)| ok && !t.muted)
        .collect();
    let b = s
        .buses
        .iter()
        .zip(ok_b)
        .map(|(b, ok)| ok && !b.muted)
        .collect();
    (t, b)
}

/// Every level from the session: faders, pans, mutes, solos, polarity
/// and sends. A post-fader send follows the fader but not the pan (it
/// has its own); a pre-fader send ignores both. Every send follows mute
/// and solo.
pub fn levels(s: &Session) -> Levels {
    let (heard_t, heard_b) = audible(s);
    let space = s.space.is_some();
    let node = |heard: bool,
                sign: f32,
                volume_db: f64,
                pan: f64,
                sends: &[crate::session::model::Send],
                mono: bool,
                place: Option<Arc<Place>>| {
        let on = if heard { sign } else { 0.0 };
        let fader = db(volume_db);
        // In a 3D Space session the spot does what the pan would.
        let [l, r] = if space { [1.0, 1.0] } else { balance(pan) };
        NodeLevel {
            main: [on * fader * l, on * fader * r],
            sends: sends
                .iter()
                .map(|x| {
                    let g = if x.enabled { on * db(x.level_db) } else { 0.0 };
                    let g = if x.pre_fader { g } else { g * fader };
                    let [l, r] = balance(x.pan);
                    [g * l, g * r]
                })
                .collect(),
            mono,
            place,
        }
    };
    Levels {
        tracks: s
            .tracks
            .iter()
            .zip(heard_t)
            .map(|(t, h)| {
                let sign = if t.polarity_inverted { -1.0 } else { 1.0 };
                // An automated volume or pan takes the fader's or pan's place.
                let vol = if super::automation::automates_volume(&t.envelopes) { 0.0 } else { t.volume_db };
                let pan = if super::automation::automates_pan(&t.envelopes) { 0.0 } else { t.pan };
                let stereo = t.channels != crate::session::model::ChannelLayout::Mono && !t.mono;
                let place = (t.output == crate::session::model::Output::Master)
                    .then(|| Place::new(s, t.space.as_ref(), stereo))
                    .flatten();
                node(h, sign, vol, pan, &t.sends, t.mono, place)
            })
            .collect(),
        buses: s
            .buses
            .iter()
            .zip(heard_b)
            .map(|(b, h)| {
                let vol = if super::automation::automates_volume(&b.envelopes) { 0.0 } else { b.volume_db };
                let pan = if super::automation::automates_pan(&b.envelopes) { 0.0 } else { b.pan };
                let stereo = b.channels != crate::session::model::ChannelLayout::Mono && !b.mono;
                let place = (b.output == crate::session::model::Output::Master)
                    .then(|| Place::new(s, b.space.as_ref(), stereo))
                    .flatten();
                node(h, 1.0, vol, pan, &b.sends, b.mono, place)
            })
            .collect(),
    }
}

enum Dest {
    Bus(usize),
    Master,
    /// Plain sessions: the track's own output, faded on the audio thread.
    Direct(usize),
}

struct Route {
    dest: Dest,
    delay: Delay,
}

/// A sidechain key: a copy of another track's or bus's sound, held back
/// to line up with this one's input.
struct Tap {
    src: Node,
    id: Id,
    delay: Delay,
    buf: Vec<f32>,
}

/// One track, bus, or the master, as it runs.
#[derive(Default)]
struct Strip {
    id: Id,
    mono: bool,
    fx: Option<Fx>,
    /// Tracks: holds the clips' sound back to this strip's input time.
    in_delay: Delay,
    input: Vec<f32>,
    /// The sound after the chain, before the fader (pre-fader sends
    /// and keys use it).
    pre: Vec<f32>,
    keys: Vec<Tap>,
    /// The output first, then each send in the session's order.
    routes: Vec<Route>,
    peak: [f32; 2],
    /// Its volume, pan and effect settings moving over the timeline.
    auto: Option<super::automation::StripAuto>,
}

/// Peaks since the last read, 0 to 1 (and above on overs).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GraphMeters {
    /// Each track after its fader. Routed sessions only (plain ones are
    /// metered on the audio thread).
    pub tracks: Vec<[f32; 2]>,
    /// Each track's chain in and out (left in, right in, left out, right out).
    pub track_chains: Vec<Option<[f32; 4]>>,
    pub buses: Vec<[f32; 2]>,
    pub bus_chains: Vec<Option<[f32; 4]>>,
    pub master_chain: Option<[f32; 4]>,
}

impl GraphMeters {
    /// Combines two readings, keeping the higher peak of each.
    pub fn merge(&mut self, other: &GraphMeters) {
        fn pairs(a: &mut Vec<[f32; 2]>, b: &[[f32; 2]]) {
            if a.len() < b.len() {
                a.resize(b.len(), [0.0; 2]);
            }
            for (x, y) in a.iter_mut().zip(b) {
                x[0] = x[0].max(y[0]);
                x[1] = x[1].max(y[1]);
            }
        }
        fn chains(a: &mut Vec<Option<[f32; 4]>>, b: &[Option<[f32; 4]>]) {
            if a.len() < b.len() {
                a.resize(b.len(), None);
            }
            for (x, y) in a.iter_mut().zip(b) {
                one(x, y);
            }
        }
        fn one(x: &mut Option<[f32; 4]>, y: &Option<[f32; 4]>) {
            if let Some(y) = y {
                let x = x.get_or_insert([0.0; 4]);
                for (a, b) in x.iter_mut().zip(y) {
                    *a = a.max(*b);
                }
            }
        }
        pairs(&mut self.tracks, &other.tracks);
        chains(&mut self.track_chains, &other.track_chains);
        pairs(&mut self.buses, &other.buses);
        chains(&mut self.bus_chains, &other.bus_chains);
        one(&mut self.master_chain, &other.master_chain);
    }
}

pub struct Graph {
    rate: u32,
    routed: bool,
    tracks: Vec<Strip>,
    buses: Vec<Strip>,
    master: Strip,
    order: Vec<Node>,
    lead: u64,
    levels: Levels,
    /// The levels the last block ended on, so changes ramp in.
    applied: Levels,
    scratch: Vec<f32>,
    /// The session's tempo map, told to plug-ins as playback crosses changes.
    tempo: Arc<Tempo>,
    /// 3D Space sessions: speaker channels in the bed.
    bed_channels: usize,
    /// The bed for the last block (interleaved, one channel per speaker),
    /// made only when asked for (exports).
    bed: Vec<f32>,
    want_bed: bool,
    /// 3D Space sessions heard on headphones: the bed rendered to two ears.
    binaural: Option<crate::dsp::binaural::Binaural>,
    /// The timeline frame the next block starts at, for automation.
    pos: u64,
}

impl Graph {
    /// Builds the graph for `s` at output rate `rate`. `routed` mixes
    /// everything here; otherwise (only for plain sessions) each track
    /// comes out on its own, before its fader.
    pub fn new(s: &Session, rate: u32, routed: bool) -> Self {
        let (order, kept) = routing::order(s);
        let order: Vec<Node> = if routed {
            order
        } else {
            order
                .into_iter()
                .filter(|n| matches!(n, Node::Track(_)))
                .collect()
        };
        let fx = |c: &EffectChain| {
            Fx::new(c, rate).map(|mut f| {
                // The graph aligns look-ahead itself; there is nothing to prime.
                f.primed = true;
                f
            })
        };
        let mut tracks: Vec<Strip> = s
            .tracks
            .iter()
            .map(|t| Strip {
                id: t.id,
                fx: fx(&t.effects),
                auto: super::automation::StripAuto::new(&t.envelopes, &t.sends, s.sample_rate, rate),
                ..Strip::default()
            })
            .collect();
        let mut buses: Vec<Strip> = if routed {
            s.buses
                .iter()
                .map(|b| Strip {
                    id: b.id,
                    mono: b.channels == ChannelLayout::Mono,
                    fx: fx(&b.effects),
                    auto: super::automation::StripAuto::new(&b.envelopes, &b.sends, s.sample_rate, rate),
                    ..Strip::default()
                })
                .collect()
        } else {
            Vec::new()
        };
        let mut master = Strip {
            id: Id::nil(),
            fx: if routed { fx(&s.master.effects) } else { None },
            ..Strip::default()
        };
        let ids: HashMap<Id, Node> = s
            .tracks
            .iter()
            .enumerate()
            .map(|(i, t)| (t.id, Node::Track(i)))
            .chain(
                s.buses
                    .iter()
                    .enumerate()
                    .map(|(i, b)| (b.id, Node::Bus(i))),
            )
            .collect();
        let lat = |st: &Strip| st.fx.as_ref().map_or(0, |f| f.latency);
        let strip = |n: Node, t: &[Strip], b: &[Strip]| -> u64 {
            match n {
                Node::Track(i) => lat(&t[i]),
                Node::Bus(i) => lat(&b[i]),
            }
        };

        // Keys each strip listens to, kept only when the routing order kept them.
        let keys_of = |st: &Strip, me: Option<Node>| -> Vec<(Node, Id)> {
            let Some(f) = st.fx.as_ref() else {
                return Vec::new();
            };
            let mut v: Vec<(Node, Id)> = Vec::new();
            for id in f.chain.key_sources() {
                let Some(src) = ids.get(&id).copied() else {
                    continue;
                };
                let ok = match me {
                    Some(me) => kept.contains(&(src, me)),
                    None => true,
                };
                if ok && !v.iter().any(|(_, i)| *i == id) {
                    v.push((src, id));
                }
            }
            v
        };
        // Where each strip's output and sends go, kept only when the
        // routing order kept them (a damaged file's loop goes to the master).
        let main_dest = |n: Node| -> Dest {
            if !routed {
                if let Node::Track(i) = n {
                    return Dest::Direct(i);
                }
            }
            let output = match n {
                Node::Track(i) => &s.tracks[i].output,
                Node::Bus(i) => &s.buses[i].output,
            };
            match routing::output_bus(s, output) {
                Some(b) if kept.contains(&(n, Node::Bus(b))) => Dest::Bus(b),
                _ => Dest::Master,
            }
        };
        let send_dests = |n: Node| -> Vec<Option<usize>> {
            if !routed {
                return Vec::new();
            }
            let sends = match n {
                Node::Track(i) => &s.tracks[i].sends,
                Node::Bus(i) => &s.buses[i].sends,
            };
            sends
                .iter()
                .map(|x| {
                    let b = s.buses.iter().position(|b| b.id == x.bus_id)?;
                    kept.contains(&(n, Node::Bus(b))).then_some(b)
                })
                .collect()
        };

        // When each strip's input and output happen, counted from the
        // clips' sound coming out of the renderer.
        let mut in_t: HashMap<Node, u64> = HashMap::new();
        let mut out_t: HashMap<Node, u64> = HashMap::new();
        let mut master_in = 0u64;
        for &n in &order {
            let mut t = 0u64;
            for (src, dst) in routing::routes(s) {
                if dst == n && kept.contains(&(src, dst)) {
                    if let Some(o) = out_t.get(&src) {
                        t = t.max(*o);
                    }
                }
            }
            let st = match n {
                Node::Track(i) => &tracks[i],
                Node::Bus(i) => &buses[i],
            };
            if routed {
                for (src, _) in keys_of(st, Some(n)) {
                    if let Some(o) = out_t.get(&src) {
                        t = t.max(*o);
                    }
                }
            }
            in_t.insert(n, t);
            let o = t + strip(n, &tracks, &buses);
            out_t.insert(n, o);
            if !routed || matches!(main_dest(n), Dest::Master) {
                master_in = master_in.max(o);
            }
        }
        if routed {
            for (src, _) in keys_of(&master, None) {
                if let Some(o) = out_t.get(&src) {
                    master_in = master_in.max(*o);
                }
            }
        }
        let lead = master_in + lat(&master);
        let input_time = |d: &Dest| match d {
            Dest::Bus(b) => in_t.get(&Node::Bus(*b)).copied().unwrap_or(0),
            Dest::Master | Dest::Direct(_) => master_in,
        };

        for &n in &order {
            let o = out_t[&n];
            let mut routes = Vec::new();
            let main = main_dest(n);
            routes.push(Route {
                delay: Delay::new((input_time(&main) - o) as usize),
                dest: main,
            });
            for d in send_dests(n) {
                // A send to a bus that is gone (or would loop) keeps its
                // place, so levels line up, but plays nowhere.
                let dest = d.map_or(Dest::Master, Dest::Bus);
                let delay = match d {
                    Some(_) => Delay::new((input_time(&dest) - o) as usize),
                    None => Delay::new(0),
                };
                routes.push(Route { dest, delay });
            }
            let me = in_t[&n];
            let st = match n {
                Node::Track(i) => &mut tracks[i],
                Node::Bus(i) => &mut buses[i],
            };
            st.keys = if routed {
                keys_of(st, Some(n))
                    .into_iter()
                    .map(|(src, id)| Tap {
                        src,
                        id,
                        delay: Delay::new((me - out_t[&src]) as usize),
                        buf: Vec::new(),
                    })
                    .collect()
            } else {
                Vec::new()
            };
            if let Node::Track(_) = n {
                st.in_delay = Delay::new(me as usize);
            }
            st.routes = routes;
        }
        if routed {
            master.keys = keys_of(&master, None)
                .into_iter()
                .map(|(src, id)| Tap {
                    src,
                    id,
                    delay: Delay::new((master_in - out_t[&src]) as usize),
                    buf: Vec::new(),
                })
                .collect();
        }

        let lv = levels(s);
        Self {
            rate,
            routed,
            tracks,
            buses,
            master,
            order,
            lead,
            applied: lv.clone(),
            levels: lv,
            scratch: Vec::new(),
            tempo: Arc::new(s.tempo.clone()),
            bed_channels: s
                .space
                .map_or(0, |sp| crate::dsp::space::speakers(sp.layout).len()),
            bed: Vec::new(),
            want_bed: false,
            binaural: s
                .space
                .filter(|sp| sp.listen == crate::session::model::SpaceListen::Binaural)
                .map(|sp| crate::dsp::binaural::Binaural::new(sp.layout, rate)),
            pos: 0,
        }
    }

    /// Where in the timeline the next block starts (output frames).
    pub fn set_position(&mut self, frame: u64) {
        self.pos = frame;
    }

    /// 3D Space sessions: speaker channels in the bed (0 otherwise).
    pub fn bed_channels(&self) -> usize {
        self.bed_channels
    }

    /// Makes the bed as well as the stereo mix (for exports).
    pub fn set_want_bed(&mut self, on: bool) {
        self.want_bed = on && self.bed_channels > 0;
    }

    /// The bed for the last block, before the master fader.
    pub fn bed(&self) -> &[f32] {
        &self.bed
    }

    /// The session's tempo map.
    pub fn tempo(&self) -> &Arc<Tempo> {
        &self.tempo
    }

    /// How far the graph delays the mix, in frames.
    pub fn lead(&self) -> u64 {
        self.lead
    }

    pub fn is_routed(&self) -> bool {
        self.routed
    }

    /// How many buffers [`Graph::process`] fills: one for a routed mix,
    /// one per track otherwise.
    pub fn outputs(&self) -> usize {
        if self.routed {
            1
        } else {
            self.tracks.len()
        }
    }

    /// New levels, ramped in over the next block. Ignored if the shape
    /// of the session changed (that restarts playback instead).
    pub fn set_levels(&mut self, levels: Levels) {
        let same = |a: &[NodeLevel], b: &[NodeLevel]| {
            a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.sends.len() == y.sends.len())
        };
        if !same(&levels.tracks, &self.levels.tracks) {
            return;
        }
        if self.routed && !same(&levels.buses, &self.levels.buses) {
            return;
        }
        self.levels = levels;
    }

    /// Swaps in edited chains. False if one cannot swap in place (its
    /// look-ahead changed); the caller restarts playback for those.
    pub fn set_chains(&mut self, update: &ChainUpdate) -> bool {
        let rate = self.rate;
        let mut ok = true;
        for (id, chain) in &update.tracks {
            if let Some(st) = self.tracks.iter_mut().find(|st| st.id == *id) {
                ok &= swap_fx(&mut st.fx, chain, rate);
            }
        }
        for (id, chain) in &update.buses {
            if let Some(st) = self.buses.iter_mut().find(|st| st.id == *id) {
                ok &= swap_fx(&mut st.fx, chain, rate);
            }
        }
        if let Some(chain) = &update.master {
            if self.routed {
                ok &= swap_fx(&mut self.master.fx, chain, rate);
            }
        }
        ok
    }

    /// Peaks since the last call.
    pub fn take_meters(&mut self) -> GraphMeters {
        let chain = |st: &mut Strip| st.fx.as_mut().map(|f| std::mem::take(&mut f.peaks));
        GraphMeters {
            tracks: if self.routed {
                self.tracks
                    .iter_mut()
                    .map(|st| std::mem::take(&mut st.peak))
                    .collect()
            } else {
                Vec::new()
            },
            track_chains: self.tracks.iter_mut().map(chain).collect(),
            buses: self
                .buses
                .iter_mut()
                .map(|st| std::mem::take(&mut st.peak))
                .collect(),
            bus_chains: self.buses.iter_mut().map(chain).collect(),
            master_chain: chain(&mut self.master),
        }
    }

    fn strip(&self, n: Node) -> &Strip {
        match n {
            Node::Track(i) => &self.tracks[i],
            Node::Bus(i) => &self.buses[i],
        }
    }

    /// Runs one block. `raw[t]` holds track `t`'s clips (stereo
    /// interleaved, all the same length). `out` gets the mix before the
    /// master fader (routed), or each track before its fader (plain);
    /// see [`Graph::outputs`].
    pub fn process(&mut self, raw: &[Vec<f32>], out: &mut [Vec<f32>]) {
        let n = raw.first().map_or(0, Vec::len);
        let frames = n / 2;
        for st in &mut self.buses {
            st.input.clear();
            st.input.resize(n, 0.0);
        }
        self.master.input.clear();
        self.master.input.resize(n, 0.0);
        let nb = self.bed_channels;
        let make_bed = self.want_bed || self.binaural.is_some();
        if make_bed {
            self.bed.clear();
            self.bed.resize(frames * nb, 0.0);
        }
        for o in out.iter_mut() {
            o.clear();
            o.resize(n, 0.0);
        }
        let done = self.run_plugin_tracks(raw, n);
        let mut scratch = std::mem::take(&mut self.scratch);
        for k in 0..self.order.len() {
            let node = self.order[k];
            let mut st = match node {
                Node::Track(i) => std::mem::take(&mut self.tracks[i]),
                Node::Bus(i) => std::mem::take(&mut self.buses[i]),
            };
            let ran = matches!(node, Node::Track(i) if done.get(i).copied().unwrap_or(false));
            if !ran {
                if let Node::Track(i) = node {
                    prepare_track(&mut st, &raw[i][..n]);
                } else if st.mono {
                    fold_mono(&mut st.input);
                }
                self.fill_keys(&mut st.keys, n);
                run_strip_at(&mut st, self.pos);
            }
            if let Some(a) = st.auto.as_mut().filter(|a| a.moves_level()) {
                a.fill(self.pos, frames);
            }
            let (from, to) = match node {
                Node::Track(i) => (&self.applied.tracks[i], &self.levels.tracks[i]),
                Node::Bus(i) => (&self.applied.buses[i], &self.levels.buses[i]),
            };
            if to.mono {
                // After the chain (and its keys), before the fader, pan and sends.
                fold_mono(&mut st.pre);
            }
            for (r, route) in st.routes.iter_mut().enumerate() {
                scratch.clear();
                scratch.extend_from_slice(&st.pre);
                if self.routed {
                    let (a, b) = if r == 0 {
                        (from.main, to.main)
                    } else {
                        (
                            from.sends.get(r - 1).copied().unwrap_or([0.0; 2]),
                            to.sends.get(r - 1).copied().unwrap_or([0.0; 2]),
                        )
                    };
                    ramp(&mut scratch, a, b, frames);
                    if let Some(au) = st.auto.as_ref() {
                        au.apply(r, &mut scratch);
                    }
                    if r == 0 {
                        for f in scratch.as_chunks::<2>().0 {
                            st.peak[0] = st.peak[0].max(f[0].abs());
                            st.peak[1] = st.peak[1].max(f[1].abs());
                        }
                    }
                }
                route.delay.run(&mut scratch);
                // 3D Space: placed into the room, and folded to stereo to listen.
                if r == 0 && matches!(route.dest, Dest::Master) {
                    if let Some(p) = to.place.as_ref() {
                        let f = p.fold;
                        for (o, v) in self
                            .master
                            .input
                            .chunks_exact_mut(2)
                            .zip(scratch.chunks_exact(2))
                        {
                            o[0] += v[0] * f[0][0] + v[1] * f[1][0];
                            o[1] += v[0] * f[0][1] + v[1] * f[1][1];
                        }
                        if make_bed {
                            let [gl, gr] = &p.bed;
                            for (o, v) in self.bed.chunks_exact_mut(nb).zip(scratch.chunks_exact(2))
                            {
                                for c in 0..nb {
                                    o[c] += v[0] * gl[c] + v[1] * gr[c];
                                }
                            }
                        }
                        continue;
                    }
                }
                let into: &mut Vec<f32> = match route.dest {
                    Dest::Bus(b) => &mut self.buses[b].input,
                    Dest::Master if r == 0 => &mut self.master.input,
                    // A send whose bus is gone plays nowhere.
                    Dest::Master => continue,
                    Dest::Direct(t) => &mut out[t],
                };
                for (o, v) in into.iter_mut().zip(&scratch) {
                    *o += *v;
                }
            }
            match node {
                Node::Track(i) => self.tracks[i] = st,
                Node::Bus(i) => self.buses[i] = st,
            }
        }
        if self.routed && nb > 0 {
            // 3D Space sessions have no master chain: it would change only
            // the stereo listening mix, not the speakers.
            if let Some(o) = out.first_mut() {
                match self.binaural.as_mut() {
                    Some(b) => b.process(&self.bed, o),
                    None => o.copy_from_slice(&self.master.input),
                }
            }
        } else if self.routed {
            let mut m = std::mem::take(&mut self.master);
            self.fill_keys(&mut m.keys, n);
            run_strip(&mut m);
            if let Some(o) = out.first_mut() {
                o.copy_from_slice(&m.pre);
            }
            self.master = m;
        }
        self.scratch = scratch;
        self.applied = self.levels.clone();
    }

    /// Runs, side by side, the chains of tracks that hold a plug-in and
    /// listen to no other track. Each plug-in waits on its own host
    /// process, so running them together keeps a session with many heavy
    /// plug-ins from adding up their times. Returns which tracks ran.
    fn run_plugin_tracks(&mut self, raw: &[Vec<f32>], n: usize) -> Vec<bool> {
        let mut done = vec![false; self.tracks.len()];
        let heavy: Vec<usize> = (0..self.tracks.len().min(raw.len()))
            .filter(|&i| {
                let t = &self.tracks[i];
                t.keys.is_empty() && t.fx.as_ref().is_some_and(|f| f.chain.has_plugins())
            })
            .collect();
        if heavy.len() < 2 {
            return done;
        }
        let threads = std::thread::available_parallelism()
            .map_or(4, std::num::NonZero::get)
            .clamp(2, 16)
            .min(heavy.len());
        let mut taken: Vec<(usize, Strip)> = heavy
            .iter()
            .map(|&i| (i, std::mem::take(&mut self.tracks[i])))
            .collect();
        let per = taken.len().div_ceil(threads);
        let position = crate::plugins::instance::block_position();
        let pos = self.pos;
        let tempo = self.tempo.clone();
        std::thread::scope(|scope| {
            for chunk in taken.chunks_mut(per) {
                let tempo = tempo.clone();
                scope.spawn(move || {
                    crate::plugins::instance::set_block_position(position);
                    crate::plugins::instance::set_block_tempo(Some(tempo));
                    for (i, st) in chunk.iter_mut() {
                        prepare_track(st, &raw[*i][..n]);
                        run_strip_at(st, pos);
                    }
                });
            }
        });
        for (i, st) in taken {
            self.tracks[i] = st;
            done[i] = true;
        }
        done
    }

    fn fill_keys(&self, keys: &mut [Tap], n: usize) {
        for tap in keys {
            tap.buf.clear();
            let src = &self.strip(tap.src).pre;
            if src.len() == n {
                tap.buf.extend_from_slice(src);
            } else {
                tap.buf.resize(n, 0.0);
            }
            tap.delay.run(&mut tap.buf);
        }
    }
}

/// Left and right averaged onto both sides.
pub fn fold_mono(buf: &mut [f32]) {
    for f in buf.as_chunks_mut::<2>().0 {
        let m = (f[0] + f[1]) * 0.5;
        *f = [m, m];
    }
}

/// Runs a strip's chain over its input, leaving the result in `pre`.
/// A track strip's input for this block: its clips, held back to line up,
/// folded to mono when asked.
fn prepare_track(st: &mut Strip, raw: &[f32]) {
    st.input.clear();
    st.input.extend_from_slice(raw);
    st.in_delay.run(&mut st.input);
    if st.mono {
        fold_mono(&mut st.input);
    }
}

fn run_strip(st: &mut Strip) {
    std::mem::swap(&mut st.input, &mut st.pre);
    if let Some(fx) = st.fx.as_mut() {
        let keys: Vec<(Id, &[f32])> = st.keys.iter().map(|t| (t.id, t.buf.as_slice())).collect();
        fx.run_keyed(&mut st.pre, &keys);
    }
}

/// Runs a strip whose block starts at timeline frame `pos`, moving its
/// automated effect settings every 256 frames.
fn run_strip_at(st: &mut Strip, pos: u64) {
    let lanes = st.auto.as_ref().map_or(0, |a| a.params.len());
    if lanes == 0 || st.fx.is_none() {
        run_strip(st);
        return;
    }
    const STEP: usize = 256;
    std::mem::swap(&mut st.input, &mut st.pre);
    let (Some(fx), Some(auto)) = (st.fx.as_mut(), st.auto.as_ref()) else {
        return;
    };
    let total = st.pre.len();
    let mut at = 0;
    while at < total {
        let end = (at + STEP * 2).min(total);
        let frame = pos + (at / 2) as u64;
        for l in &auto.params {
            fx.chain.set_param(l.slot, &l.param, f64::from(l.value_at(frame)));
        }
        let keys: Vec<(Id, &[f32])> = st
            .keys
            .iter()
            .map(|t| (t.id, &t.buf[at.min(t.buf.len())..end.min(t.buf.len())]))
            .collect();
        fx.run_keyed(&mut st.pre[at..end], &keys);
        at = end;
    }
}

/// Multiplies `buf` by a gain moving from `a` to `b` across the block.
fn ramp(buf: &mut [f32], a: [f32; 2], b: [f32; 2], frames: usize) {
    if a == b {
        if a != [1.0; 2] {
            for f in buf.as_chunks_mut::<2>().0 {
                f[0] *= a[0];
                f[1] *= a[1];
            }
        }
        return;
    }
    let step = 1.0 / frames.max(1) as f32;
    for (i, f) in buf.as_chunks_mut::<2>().0.iter_mut().enumerate() {
        let t = (i + 1) as f32 * step;
        f[0] *= a[0] + (b[0] - a[0]) * t;
        f[1] *= a[1] + (b[1] - a[1]) * t;
    }
}

/// The renderer and the mix graph together: clips in, the mix (or each
/// track) out, lined up with the timeline.
pub struct Mix {
    renderer: Renderer,
    graph: Graph,
    raw: Vec<Vec<f32>>,
    rendered: u64,
}

impl Mix {
    /// `renderer` must start at the timeline frame the mix should start
    /// at. The graph is primed with its lead first, which reads that
    /// many frames further on.
    pub fn new(mut renderer: Renderer, mut graph: Graph) -> Self {
        let mut raw = vec![Vec::new(); renderer.track_count()];
        let mut sink = vec![Vec::new(); graph.outputs()];
        let mut left = graph.lead();
        crate::plugins::instance::set_block_tempo(Some(graph.tempo().clone()));
        while left > 0 && !raw.is_empty() {
            let want = left.min(4_096) as usize;
            crate::plugins::instance::set_block_position(renderer.position() as i64);
            graph.set_position(renderer.position());
            renderer.render(want, &mut raw);
            graph.process(&raw, &mut sink);
            left -= raw[0].len() as u64 / 2;
        }
        let _ = graph.take_meters();
        Self {
            renderer,
            graph,
            raw,
            rendered: 0,
        }
    }

    /// Renders up to `frames` frames into `out` (see [`Graph::outputs`]).
    /// May stop short at a loop or skip point. Returns the frames made.
    pub fn render(&mut self, frames: usize, out: &mut [Vec<f32>]) -> usize {
        if self.raw.is_empty() {
            for o in out.iter_mut() {
                o.clear();
                o.resize(frames * 2, 0.0);
            }
            self.rendered += frames as u64;
            return frames;
        }
        // Plug-ins are told where in the timeline this block is (their song
        // position) and the tempo there.
        crate::plugins::instance::set_block_position(self.renderer.position() as i64);
        crate::plugins::instance::set_block_tempo(Some(self.graph.tempo().clone()));
        self.graph.set_position(self.renderer.position());
        self.renderer.render(frames, &mut self.raw);
        self.graph.process(&self.raw, out);
        let n = self.raw[0].len() / 2;
        self.rendered += n as u64;
        n
    }

    /// Frames handed out since the start, across jumps, so it lines up
    /// with the frames the device has played.
    pub fn rendered(&self) -> u64 {
        self.rendered
    }

    /// The timeline frame the next block starts at.
    pub fn position(&self) -> u64 {
        self.renderer.position()
    }

    pub fn outputs(&self) -> usize {
        self.graph.outputs()
    }

    pub fn graph(&mut self) -> &mut Graph {
        &mut self.graph
    }

    /// Swaps in edited chains (clips, tracks, buses, master). False if
    /// any changed look-ahead; the caller restarts for those.
    pub fn set_chains(&mut self, update: &ChainUpdate) -> bool {
        let clips = self.renderer.set_chains(update);
        let rest = self.graph.set_chains(update);
        clips && rest
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::plan;
    use crate::session::model::{new_id, Bus, EffectSlot, MediaRef, Output, Send, Track};
    use crate::session::ops::{apply, ClipSpec, Op};
    use std::path::{Path, PathBuf};

    const RATE: u32 = 48_000;

    /// One second, mono: `f(i)` for each frame.
    fn wav(dir: &Path, name: &str, f: impl Fn(usize) -> f32) -> PathBuf {
        let path = dir.join(name);
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: RATE,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(&path, spec).unwrap();
        for i in 0..RATE as usize {
            w.write_sample((f(i).clamp(-1.0, 1.0) * 32_767.0) as i16)
                .unwrap();
        }
        w.finalize().unwrap();
        path
    }

    fn click(dir: &Path) -> PathBuf {
        wav(dir, "click.wav", |i| if i == 1_000 { 1.0 } else { 0.0 })
    }

    /// Adds a track playing `path` from source frame `source_in`, placed at `start`.
    fn track(s: &mut Session, path: &Path, start: u64, source_in: u64, len: u64) -> usize {
        let m = MediaRef {
            id: new_id(),
            path: path.into(),
            relative_path: None,
            sample_rate: RATE,
            channels: 1,
            length: u64::from(RATE),
        };
        let mid = m.id;
        s.media.push(m);
        s.tracks.push(Track::new("t", ChannelLayout::Stereo));
        let tid = s.tracks.last().unwrap().id;
        apply(
            s,
            &Op::AddClips {
                track_id: tid,
                clips: vec![ClipSpec {
                    id: new_id(),
                    name: "c".into(),
                    media_id: mid,
                    start,
                    source_in,
                    source_out: source_in + len,
                }],
            },
        )
        .unwrap();
        s.tracks.len() - 1
    }

    fn chain(effect: &str, params: &[(&str, f64)]) -> EffectChain {
        let mut c = EffectChain::default();
        c.slots.push(EffectSlot {
            id: new_id(),
            effect: effect.into(),
            enabled: true,
            params: params.iter().map(|(k, v)| ((*k).to_string(), *v)).collect(),
            preset: None,
            sidechain: None,
            plugin_state: None,
            data: None,
            position: None,
        });
        c
    }

    fn limiter(ms: f64) -> EffectChain {
        chain("altaudio.limiter", &[("ceiling", 0.0), ("lookahead", ms)])
    }

    fn mix(s: &Session, routed: bool, from: u64) -> Mix {
        Mix::new(
            Renderer::new(plan::build(s, RATE), from),
            Graph::new(s, RATE, routed),
        )
    }

    /// Renders `frames` frames from `from`; one buffer per output.
    fn render(s: &Session, routed: bool, from: u64, frames: usize) -> Vec<Vec<f32>> {
        let mut m = mix(s, routed, from);
        let mut out = vec![Vec::new(); m.outputs()];
        let mut all = vec![Vec::new(); m.outputs()];
        let mut done = 0;
        while done < frames {
            let got = m.render(1_024.min(frames - done), &mut out);
            for (a, o) in all.iter_mut().zip(&out) {
                a.extend_from_slice(o);
            }
            done += got;
        }
        all
    }

    fn clicks(buf: &[f32], over: f32) -> Vec<usize> {
        buf.as_chunks::<2>()
            .0
            .iter()
            .enumerate()
            .filter(|(_, f)| f[0].abs() > over)
            .map(|(i, _)| i)
            .collect()
    }

    fn peak(buf: &[f32], from: usize, to: usize) -> f32 {
        buf[from * 2..to * 2]
            .iter()
            .fold(0f32, |m, v| m.max(v.abs()))
    }

    #[test]
    fn track_chains_keep_time_with_look_ahead() {
        let dir = tempfile::tempdir().unwrap();
        let path = click(dir.path());
        for (a, b) in [
            (limiter(5.0), EffectChain::default()),
            (limiter(5.0), limiter(2.0)),
        ] {
            let mut s = Session::new("t", RATE);
            // Click at timeline 5,500 on both tracks.
            track(&mut s, &path, 5_000, 500, 10_000);
            track(&mut s, &path, 5_000, 500, 10_000);
            s.tracks[0].effects = a;
            s.tracks[1].effects = b;
            s.tracks[1].clips[0].effects = limiter(1.0);
            for routed in [false, true] {
                let out = render(&s, routed, 0, 12_000);
                for o in &out {
                    assert_eq!(clicks(o, 0.5), vec![5_500], "routed {routed}");
                }
                let out = render(&s, routed, 5_200, 1_024);
                assert_eq!(clicks(&out[0], 0.5), vec![300], "starting mid-clip");
            }
        }
        // A gain effect on the track halves the click.
        let mut s = Session::new("t", RATE);
        track(&mut s, &path, 5_000, 500, 10_000);
        s.tracks[0].effects = chain("altaudio.gain", &[("gain", -6.0206)]);
        let out = render(&s, false, 5_000, 1_024);
        assert!((peak(&out[0], 0, 1_024) - 0.5).abs() < 0.01);
    }

    #[test]
    fn buses_sum_their_inputs_in_step_with_direct_tracks() {
        let dir = tempfile::tempdir().unwrap();
        let path = click(dir.path());
        let mut s = Session::new("t", RATE);
        let direct = track(&mut s, &path, 5_000, 500, 10_000);
        let routed = track(&mut s, &path, 5_000, 500, 10_000);
        let sent = track(&mut s, &path, 5_000, 500, 10_000);
        s.tracks[direct].effects = limiter(1.0);
        // A bus with 5 ms of look-ahead and -6 dB, and a send-only reverb bus.
        let mut bus = Bus::new("Voices", ChannelLayout::Stereo);
        bus.effects = limiter(5.0);
        bus.volume_db = -6.0206;
        let verb = Bus::new("Reverb", ChannelLayout::Stereo);
        let (bus_id, verb_id) = (bus.id, verb.id);
        s.buses.push(bus);
        s.buses.push(verb);
        s.tracks[routed].output = Output::Bus { bus_id };
        s.tracks[sent].volume_db = -96.0;
        s.tracks[sent].sends.push(Send {
            id: new_id(),
            bus_id: verb_id,
            level_db: -6.0206,
            pan: 100.0,
            pre_fader: true,
            enabled: true,
        });
        assert!(crate::session::routing::is_routed(&s));
        let out = render(&s, true, 0, 12_000);
        // Every path lands on frame 5,500: 1 (direct) + 0.5 (bus) +
        // 0.5 on the right only (pre-fader send, panned right).
        let f = |i: usize| (out[0][i * 2], out[0][i * 2 + 1]);
        let (l, r) = f(5_500);
        assert!((l - 1.5).abs() < 0.01, "left {l}");
        assert!((r - 2.0).abs() < 0.01, "right {r}");
        assert_eq!(clicks(&out[0], 0.01), vec![5_500]);

        // Meters: the bus after its fader.
        let mut m = mix(&s, true, 5_000);
        let mut o = vec![Vec::new()];
        m.render(1_024, &mut o);
        let meters = m.graph().take_meters();
        assert!((meters.buses[0][0] - 0.5).abs() < 0.01, "{meters:?}");
        assert!(meters.bus_chains[0].is_some());
    }

    #[test]
    fn soloing_a_bus_plays_what_feeds_it() {
        let mut s = Session::new("t", RATE);
        for _ in 0..3 {
            s.tracks.push(Track::new("t", ChannelLayout::Stereo));
        }
        s.buses.push(Bus::new("b", ChannelLayout::Stereo));
        s.buses.push(Bus::new("verb", ChannelLayout::Stereo));
        let (b, verb) = (s.buses[0].id, s.buses[1].id);
        s.tracks[0].output = Output::Bus { bus_id: b };
        s.buses[0].solo = true;
        let (t, bs) = audible(&s);
        assert_eq!(t, vec![true, false, false]);
        assert_eq!(bs, vec![true, false]);
        // A soloed track is heard through its send's bus.
        s.buses[0].solo = false;
        s.tracks[1].solo = true;
        s.tracks[1].sends.push(Send {
            id: new_id(),
            bus_id: verb,
            level_db: 0.0,
            pan: 0.0,
            pre_fader: false,
            enabled: true,
        });
        let (t, bs) = audible(&s);
        assert_eq!(t, vec![false, true, false]);
        assert_eq!(bs, vec![false, true]);
        s.tracks[1].muted = true;
        assert_eq!(audible(&s).0, vec![false, false, false]);
        // Levels: muted means silent sends too, polarity flips both.
        s.tracks[1].muted = false;
        s.tracks[1].polarity_inverted = true;
        s.tracks[1].volume_db = -6.0206;
        let lv = levels(&s);
        assert!((lv.tracks[1].main[0] + 0.5).abs() < 1e-3);
        assert!(
            (lv.tracks[1].sends[0][0] + 0.5).abs() < 1e-3,
            "post-fader send"
        );
        assert_eq!(lv.tracks[0].main, [0.0, 0.0]);
    }

    #[test]
    fn sidechain_ducks_music_under_the_voice() {
        let dir = tempfile::tempdir().unwrap();
        let tone = wav(dir.path(), "tone.wav", |i| {
            (i as f32 * 220.0 * std::f32::consts::TAU / RATE as f32).sin() * 0.5
        });
        let burst = wav(dir.path(), "burst.wav", |i| {
            if (24_000..36_000).contains(&i) {
                0.5
            } else {
                0.0
            }
        });
        let mut s = Session::new("t", RATE);
        let voice = track(&mut s, &burst, 0, 0, 48_000);
        let music = track(&mut s, &tone, 0, 0, 48_000);
        let mut duck = chain(
            "altaudio.dynamics",
            &[
                ("threshold", -30.0),
                ("ratio", 20.0),
                ("attack", 0.1),
                ("release", 20.0),
                ("knee", 0.0),
            ],
        );
        duck.slots[0].sidechain = Some(s.tracks[voice].id);
        s.tracks[music].effects = duck;
        // A look-ahead on the voice: the key must still line up.
        s.tracks[voice].effects = limiter(5.0);
        // The key is taken before the fader, so a muted voice still ducks.
        s.tracks[voice].muted = true;
        assert!(crate::session::routing::is_routed(&s));
        let out = &render(&s, true, 0, 48_000)[0];
        let before = peak(out, 10_000, 20_000);
        let during = peak(out, 25_000, 35_000);
        let after = peak(out, 42_000, 47_000);
        assert!((before - 0.5).abs() < 0.02, "untouched before: {before}");
        assert!(during < 0.06, "ducked under the voice: {during}");
        assert!((after - 0.5).abs() < 0.02, "back after: {after}");
        // The duck starts with the voice, not before it.
        assert!(peak(out, 23_000, 23_990) > 0.45);
    }

    #[test]
    fn levels_change_live_and_ramp() {
        let dir = tempfile::tempdir().unwrap();
        let dc = wav(dir.path(), "dc.wav", |_| 0.5);
        let mut s = Session::new("t", RATE);
        track(&mut s, &dc, 0, 0, 48_000);
        s.buses.push(Bus::new("b", ChannelLayout::Stereo));
        s.tracks[0].output = Output::Bus {
            bus_id: s.buses[0].id,
        };
        let mut m = mix(&s, true, 0);
        let mut o = vec![Vec::new()];
        m.render(1_000, &mut o);
        assert!((o[0][1_998] - 0.5).abs() < 1e-3);
        s.buses[0].volume_db = -6.0206;
        m.graph().set_levels(levels(&s));
        m.render(1_000, &mut o);
        assert!((o[0][998] - 0.375).abs() < 0.01, "halfway down the ramp");
        assert!((o[0][1_998] - 0.25).abs() < 1e-3);
        m.render(1_000, &mut o);
        assert!((o[0][0] - 0.25).abs() < 1e-3, "and it stays there");
    }

    #[test]
    fn mono_folds_a_track_after_its_chain_and_before_its_pan() {
        let dir = tempfile::tempdir().unwrap();
        let dc = wav(dir.path(), "dc.wav", |_| 0.5);
        let mut s = Session::new("t", RATE);
        track(&mut s, &dc, 0, 0, 48_000);
        // Hard left before the fold: the right side is silent.
        s.tracks[0].effects = chain("altaudio.gain", &[("gain", 0.0)]);
        let mut g = Graph::new(&s, RATE, true);
        let raw = vec![[0.5f32, 0.0].repeat(64)];
        let mut out = vec![Vec::new()];
        g.process(&raw, &mut out);
        assert_eq!((out[0][10], out[0][11]), (0.5, 0.0));
        s.tracks[0].mono = true;
        g.set_levels(levels(&s));
        g.process(&raw, &mut out);
        assert_eq!(
            (out[0][10], out[0][11]),
            (0.25, 0.25),
            "folded live, no rebuild"
        );
        // Pan still places the folded track.
        s.tracks[0].pan = -100.0;
        g.set_levels(levels(&s));
        g.process(&raw, &mut out);
        g.process(&raw, &mut out);
        assert_eq!((out[0][10], out[0][11]), (0.25, 0.0));
    }

    #[test]
    fn chains_swap_in_while_playing() {
        let dir = tempfile::tempdir().unwrap();
        let path = click(dir.path());
        let mut s = Session::new("t", RATE);
        track(&mut s, &path, 5_000, 500, 10_000);
        s.buses.push(Bus::new("b", ChannelLayout::Stereo));
        let (tid, bid) = (s.tracks[0].id, s.buses[0].id);
        s.tracks[0].output = Output::Bus { bus_id: bid };
        s.tracks[0].effects = limiter(5.0);
        s.buses[0].effects = limiter(5.0);
        let lim = |ceiling: f64| {
            chain(
                "altaudio.limiter",
                &[("ceiling", ceiling), ("lookahead", 5.0)],
            )
        };
        let play = |swap: Option<ChainUpdate>| {
            let mut m = mix(&s, true, 0);
            let mut out = vec![Vec::new()];
            let mut all = Vec::new();
            for block in 0..12 {
                if block == 5 {
                    if let Some(u) = &swap {
                        assert!(m.set_chains(u));
                    }
                }
                m.render(1_024, &mut out);
                all.extend_from_slice(&out[0]);
            }
            all
        };
        let plain = play(None);
        let same = play(Some(ChainUpdate {
            tracks: vec![(tid, lim(0.0))],
            buses: vec![(bid, lim(0.0))],
            ..ChainUpdate::default()
        }));
        assert_eq!(
            plain, same,
            "the same chains pick up exactly where the old ones were"
        );
        let lower = play(Some(ChainUpdate {
            buses: vec![(bid, lim(-6.0))],
            ..ChainUpdate::default()
        }));
        assert_eq!(clicks(&lower, 0.4), vec![5_500]);
        assert!((peak(&lower, 0, 12_000) - 0.5).abs() < 0.02);
        // A different look-ahead cannot swap in place.
        let mut m = mix(&s, true, 0);
        assert!(!m.set_chains(&ChainUpdate {
            tracks: vec![(tid, limiter(10.0))],
            ..ChainUpdate::default()
        }));
    }

    #[test]
    fn track_chain_reaches_every_clip() {
        let dir = tempfile::tempdir().unwrap();
        let tone = wav(dir.path(), "tone.wav", |i| {
            (i as f32 * 220.0 * std::f32::consts::TAU / RATE as f32).sin() * 0.5
        });
        for fx in [
            chain("altaudio.gain", &[("gain", -12.0)]),
            chain(
                "altaudio.limiter",
                &[("ceiling", -12.0), ("lookahead", 5.0)],
            ),
        ] {
            let mut s = Session::new("t", RATE);
            let t = track(&mut s, &tone, 0, 0, 12_000);
            for start in [30_000, 60_000] {
                let mid = s.media[0].id;
                let tid = s.tracks[t].id;
                apply(
                    &mut s,
                    &Op::AddClips {
                        track_id: tid,
                        clips: vec![ClipSpec {
                            id: new_id(),
                            name: "c".into(),
                            media_id: mid,
                            start,
                            source_in: 0,
                            source_out: 12_000,
                        }],
                    },
                )
                .unwrap();
            }
            s.tracks[t].effects = fx;
            let out = &render(&s, false, 0, 80_000)[0];
            let (a, b, c) = (
                peak(out, 4_000, 11_000),
                peak(out, 34_000, 41_000),
                peak(out, 64_000, 71_000),
            );
            assert!((a - b).abs() < 0.03 && (a - c).abs() < 0.03, "{a} {b} {c}");
            assert!(a < 0.35, "first clip {a} not processed");
        }
    }

    #[test]
    fn loops_play_through_the_lead() {
        let dir = tempfile::tempdir().unwrap();
        let path = click(dir.path());
        let mut s = Session::new("t", RATE);
        track(&mut s, &path, 5_000, 500, 10_000);
        s.buses.push(Bus::new("b", ChannelLayout::Stereo));
        s.tracks[0].output = Output::Bus {
            bus_id: s.buses[0].id,
        };
        s.buses[0].effects = limiter(5.0);
        // Loop 5,000..6,000 from 4,000: click at 1,500 rendered frames, then every 1,000.
        let r = Renderer::new(plan::build(&s, RATE), 4_000).with_jump(6_000, 5_000);
        let mut m = Mix::new(r, Graph::new(&s, RATE, true));
        let mut out = vec![Vec::new()];
        let mut all = Vec::new();
        while all.len() < 4_600 * 2 {
            m.render(1_000, &mut out);
            all.extend_from_slice(&out[0]);
        }
        assert_eq!(clicks(&all, 0.5), vec![1_500, 2_500, 3_500, 4_500]);
    }

    #[test]
    fn a_damaged_loop_still_plays() {
        let mut s = Session::new("t", RATE);
        s.tracks.push(Track::new("t", ChannelLayout::Stereo));
        s.buses.push(Bus::new("a", ChannelLayout::Stereo));
        s.buses.push(Bus::new("b", ChannelLayout::Stereo));
        let (a, b) = (s.buses[0].id, s.buses[1].id);
        s.tracks[0].output = Output::Bus { bus_id: a };
        s.buses[0].output = Output::Bus { bus_id: b };
        s.buses[1].output = Output::Bus { bus_id: a };
        let mut g = Graph::new(&s, RATE, true);
        let raw = vec![vec![0.25f32; 64]];
        let mut out = vec![Vec::new()];
        g.process(&raw, &mut out);
        assert_eq!(out[0].len(), 64);
        assert!(
            (out[0][10] - 0.25).abs() < 1e-6,
            "the loop's back edge goes to the master"
        );
    }
}
