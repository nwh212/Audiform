// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Who feeds whom: track and bus outputs, aux sends, and sidechain keys.
//!
//! Sound flows one way, from tracks through buses to the master. Edits
//! that would make it flow back into itself are refused, and the engine
//! runs tracks and buses in the order worked out here, so every bus
//! hears all of its inputs (and every sidechained effect hears its key)
//! for the same stretch of time.

use std::collections::HashMap;

use super::model::{EffectChain, Id, Output, Session};

/// A track or a bus, by index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Node {
    Track(usize),
    Bus(usize),
}

/// Ids of the tracks and buses that key `chain`'s effects.
pub fn key_sources(chain: &EffectChain) -> impl Iterator<Item = Id> + '_ {
    chain.slots.iter().filter_map(|s| s.sidechain)
}

fn index(s: &Session) -> HashMap<Id, Node> {
    let mut index: HashMap<Id, Node> = HashMap::new();
    for (i, t) in s.tracks.iter().enumerate() {
        index.insert(t.id, Node::Track(i));
    }
    for (i, b) in s.buses.iter().enumerate() {
        index.insert(b.id, Node::Bus(i));
    }
    index
}

/// The bus a track or bus outputs to, if it outputs to a bus that exists.
pub fn output_bus(s: &Session, output: &Output) -> Option<usize> {
    match output {
        Output::Bus { bus_id } => s.buses.iter().position(|b| b.id == *bus_id),
        Output::Master => None,
    }
}

/// Where sound flows, as (from, to): each output to a bus, and each send
/// (on or off, so turning a send on never makes a loop). Routes to buses
/// that do not exist are left out.
pub fn routes(s: &Session) -> Vec<(Node, Node)> {
    let mut out = Vec::new();
    let mut add = |from: Node, output: &Output, sends: &[super::model::Send]| {
        if let Some(b) = output_bus(s, output) {
            out.push((from, Node::Bus(b)));
        }
        for send in sends {
            if let Some(b) = s.buses.iter().position(|b| b.id == send.bus_id) {
                out.push((from, Node::Bus(b)));
            }
        }
    };
    for (i, t) in s.tracks.iter().enumerate() {
        add(Node::Track(i), &t.output, &t.sends);
    }
    for (i, b) in s.buses.iter().enumerate() {
        add(Node::Bus(i), &b.output, &b.sends);
    }
    out
}

/// Sidechain keys as (source, the track or bus whose chain listens).
pub fn keys(s: &Session) -> Vec<(Node, Node)> {
    let index = index(s);
    let mut out = Vec::new();
    let nodes = s
        .tracks
        .iter()
        .enumerate()
        .map(|(i, t)| (Node::Track(i), &t.effects))
        .chain(
            s.buses
                .iter()
                .enumerate()
                .map(|(i, b)| (Node::Bus(i), &b.effects)),
        );
    for (to, fx) in nodes {
        for src in key_sources(fx) {
            if let Some(n) = index.get(&src) {
                out.push((*n, to));
            }
        }
    }
    out
}

/// Every edge: routes, then keys.
fn edges(s: &Session) -> Vec<(Node, Node)> {
    let mut e = routes(s);
    e.extend(keys(s));
    e
}

/// Every track and bus in an order where each comes after everything
/// that feeds it, and the edges that order keeps. Edges that would close
/// a loop (only possible in a damaged file) are left out of the list.
pub fn order(s: &Session) -> (Vec<Node>, Vec<(Node, Node)>) {
    let nodes: Vec<Node> = (0..s.tracks.len())
        .map(Node::Track)
        .chain((0..s.buses.len()).map(Node::Bus))
        .collect();
    let mut kept: Vec<(Node, Node)> = Vec::new();
    for e in edges(s) {
        if e.0 != e.1 && !reaches(&kept, e.1, e.0) {
            kept.push(e);
        }
    }
    // Kahn's algorithm, keeping the session's own order among equals.
    let mut incoming: HashMap<Node, usize> = nodes.iter().map(|n| (*n, 0)).collect();
    for (_, to) in &kept {
        *incoming.get_mut(to).expect("node listed") += 1;
    }
    let mut done = Vec::with_capacity(nodes.len());
    let mut left = nodes;
    while !left.is_empty() {
        let i = left
            .iter()
            .position(|n| incoming[n] == 0)
            .expect("loops were dropped above");
        let n = left.remove(i);
        for (_, to) in kept.iter().filter(|(f, _)| *f == n) {
            *incoming.get_mut(to).expect("node listed") -= 1;
        }
        done.push(n);
    }
    (done, kept)
}

/// True when `from` leads to `to` along `edges`.
fn reaches(edges: &[(Node, Node)], from: Node, to: Node) -> bool {
    let mut stack = vec![from];
    let mut seen = Vec::new();
    while let Some(n) = stack.pop() {
        if n == to {
            return true;
        }
        if seen.contains(&n) {
            continue;
        }
        seen.push(n);
        stack.extend(edges.iter().filter(|(f, _)| *f == n).map(|(_, t)| *t));
    }
    false
}

/// True when some sound would feed back into itself.
pub fn has_loop(s: &Session) -> bool {
    let mut kept: Vec<(Node, Node)> = Vec::new();
    for e in edges(s) {
        if e.0 == e.1 || reaches(&kept, e.1, e.0) {
            return true;
        }
        kept.push(e);
    }
    false
}

/// True when the mix needs more than tracks summed into the master:
/// a track or bus routed to a bus, a send, a master chain, or a
/// sidechain. Then the whole mix runs on the render thread.
pub fn is_routed(s: &Session) -> bool {
    // 3D Space sessions place every track on the render thread.
    s.space.is_some()
        // Automation moves levels and settings on the render thread.
        || s.tracks.iter().any(|t| crate::engine::automation::has_automation(&t.envelopes))
        || s.buses.iter().any(|b| crate::engine::automation::has_automation(&b.envelopes))
        || crate::dsp::effects::is_active(&s.master.effects)
        || s.tracks
            .iter()
            .any(|t| t.output != Output::Master || t.sends.iter().any(|x| x.enabled))
        || s.tracks
            .iter()
            .any(|t| key_sources(&t.effects).next().is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::model::{new_id, Bus, ChannelLayout, EffectSlot, Send, Track};

    fn send(bus_id: Id) -> Send {
        Send {
            id: new_id(),
            bus_id,
            level_db: 0.0,
            pan: 0.0,
            pre_fader: false,
            enabled: true,
        }
    }

    #[test]
    fn order_puts_feeders_first_and_finds_loops() {
        let mut s = Session::new("t", 48_000);
        s.tracks.push(Track::new("Voice", ChannelLayout::Mono));
        s.tracks.push(Track::new("Music", ChannelLayout::Stereo));
        s.buses.push(Bus::new("Mix", ChannelLayout::Stereo));
        s.buses.push(Bus::new("Reverb", ChannelLayout::Stereo));
        let (mix, verb) = (s.buses[0].id, s.buses[1].id);
        // Reverb outputs into Mix, so Reverb runs first even though it is listed second.
        s.buses[1].output = Output::Bus { bus_id: mix };
        s.tracks[0].sends.push(send(verb));
        assert!(!has_loop(&s));
        let (ord, _) = order(&s);
        let at = |n| ord.iter().position(|x| *x == n).unwrap();
        assert!(at(Node::Bus(1)) < at(Node::Bus(0)));
        assert!(at(Node::Track(0)) < at(Node::Bus(1)));

        // Mix sending back to Reverb would loop.
        s.buses[0].sends.push(send(verb));
        assert!(has_loop(&s));
        let (ord, kept) = order(&s);
        assert_eq!(ord.len(), 4, "a damaged file still plays every node");
        assert_eq!(kept.len(), 2);
        s.buses[0].sends.clear();

        // Music ducked by Voice: Voice runs first.
        let mut slot = EffectSlot {
            id: new_id(),
            effect: "altaudio.dynamics".into(),
            enabled: true,
            params: Default::default(),
            preset: None,
            sidechain: Some(s.tracks[0].id),
            plugin_state: None,
            data: None,
            position: None,
        };
        s.tracks[1].effects.slots.push(slot.clone());
        assert!(!has_loop(&s));
        assert!(is_routed(&s));
        // A track keyed by itself is a loop.
        slot.sidechain = Some(s.tracks[1].id);
        s.tracks[1].effects.slots.push(slot);
        assert!(has_loop(&s));
    }

    #[test]
    fn plain_sessions_are_not_routed() {
        let mut s = Session::new("t", 48_000);
        s.tracks.push(Track::new("Voice", ChannelLayout::Mono));
        s.buses.push(Bus::new("Unused", ChannelLayout::Stereo));
        assert!(!is_routed(&s), "a bus nobody feeds changes nothing");
        let mut off = send(s.buses[0].id);
        off.enabled = false;
        s.tracks[0].sends.push(off);
        assert!(!is_routed(&s));
        s.tracks[0].sends[0].enabled = true;
        assert!(is_routed(&s));
    }
}
