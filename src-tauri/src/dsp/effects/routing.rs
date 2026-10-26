// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

//! Effect channel routing: which of a chain's channels an effect hears,
//! and which it writes back to. A chain has two channels (L, R) on tracks,
//! clips and buses, and one per speaker on the master of a 3D Space
//! session. An effect has its own inputs and outputs: two for the
//! built-in effects, or a plug-in's own layout (5.1 and so on).
//!
//! Without a routing of its own, an effect's channels go to the chain
//! channels of the same speaker name, so a stereo effect works on L and
//! R and a 5.1 plug-in on a 7.1.4 master works on its 5.1 speakers. A
//! chain channel no output goes to passes through untouched (held back
//! by the effect's look-ahead, so everything stays in time).

use crate::session::model::SlotRouting;

/// Where each of an effect's channels goes: per input, the chain channel
/// it hears; per output, the chain channel it replaces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Map {
    pub inputs: Vec<Option<usize>>,
    pub outputs: Vec<Option<usize>>,
}

/// The chain channel an effect channel named `name` goes to by default.
/// `own` is every name on the effect's side, which tells a 5.1 surround
/// from a 7.1 rear surround.
pub fn by_name(name: &str, own: &[String], chain: &[String]) -> Option<usize> {
    let find = |n: &str| chain.iter().position(|c| c == n);
    if let Some(i) = find(name) {
        return Some(i);
    }
    let has = |n: &str| own.iter().any(|o| o == n);
    let tries: &[&str] = match name {
        // With side surrounds as well, these are the rear ones.
        "Ls" if has("Lss") => &["Lrs"],
        "Rs" if has("Rss") => &["Rrs"],
        // A 5.1 surround sits at the side in a 7.1 room.
        "Ls" => &["Lss", "Lrs"],
        "Rs" => &["Rss", "Rrs"],
        "Lss" => &["Ls"],
        "Rss" => &["Rs"],
        "Lrs" => &["Ls"],
        "Rrs" => &["Rs"],
        "Ltf" | "Ltr" => &["Ltm"],
        "Rtf" | "Rtr" => &["Rtm"],
        "Ltm" => &["Ltf"],
        "Rtm" => &["Rtf"],
        "M" => &["C", "L"],
        _ => &[],
    };
    tries.iter().find_map(|n| find(n))
}

/// The effect's map in a chain with channels `chain`: the slot's own
/// routing where it has one (channels out of range are dropped), or by
/// speaker name.
pub fn resolve(
    routing: Option<&SlotRouting>,
    ins: &[String],
    outs: &[String],
    chain: &[String],
) -> Map {
    let n = chain.len();
    let own = |set: &[Option<u8>], names: &[String]| -> Vec<Option<usize>> {
        (0..names.len())
            .map(|k| {
                set.get(k)
                    .copied()
                    .flatten()
                    .map(usize::from)
                    .filter(|c| *c < n)
            })
            .collect()
    };
    let named = |names: &[String]| -> Vec<Option<usize>> {
        names.iter().map(|x| by_name(x, names, chain)).collect()
    };
    let r = routing.filter(|r| !r.inputs.is_empty() || !r.outputs.is_empty());
    Map {
        inputs: match r {
            Some(r) if !r.inputs.is_empty() => own(&r.inputs, ins),
            _ => named(ins),
        },
        outputs: match r {
            Some(r) if !r.outputs.is_empty() => own(&r.outputs, outs),
            _ => named(outs),
        },
    }
}

impl Map {
    /// True when the effect simply runs on the chain's channels as they are.
    pub fn is_straight(&self, channels: usize) -> bool {
        let same = |v: &[Option<usize>]| v.len() == channels && v.iter().enumerate().all(|(k, c)| *c == Some(k));
        same(&self.inputs) && same(&self.outputs)
    }
}

/// The names of the stereo channels.
pub fn stereo() -> Vec<String> {
    vec!["L".into(), "R".into()]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn a_stereo_effect_on_stereo_runs_straight() {
        let m = resolve(None, &stereo(), &stereo(), &stereo());
        assert!(m.is_straight(2));
    }

    #[test]
    fn a_five_one_effect_on_a_seven_one_four_master_works_on_its_speakers() {
        let bed = names(&["L", "R", "C", "LFE", "Lrs", "Rrs", "Lss", "Rss", "Ltf", "Rtf", "Ltr", "Rtr"]);
        let fx = names(&["L", "R", "C", "LFE", "Ls", "Rs"]);
        let m = resolve(None, &fx, &fx, &bed);
        assert_eq!(m.inputs, vec![Some(0), Some(1), Some(2), Some(3), Some(6), Some(7)]);
        assert_eq!(m.outputs, m.inputs);
        let fx71 = names(&["L", "R", "C", "LFE", "Ls", "Rs", "Lss", "Rss"]);
        let m = resolve(None, &fx71, &fx71, &bed);
        assert_eq!(m.inputs[4..], [Some(4), Some(5), Some(6), Some(7)]);
    }

    #[test]
    fn a_stereo_effect_on_the_bed_works_on_l_and_r() {
        let bed = names(&["L", "R", "C", "LFE", "Ls", "Rs"]);
        let m = resolve(None, &stereo(), &stereo(), &bed);
        assert_eq!(m.inputs, vec![Some(0), Some(1)]);
        assert!(!m.is_straight(6));
    }

    #[test]
    fn its_own_routing_wins_and_stays_in_range() {
        let r = SlotRouting {
            wide: false,
            inputs: vec![Some(1), Some(0)],
            outputs: vec![Some(9), None],
        };
        let m = resolve(Some(&r), &stereo(), &stereo(), &stereo());
        assert_eq!(m.inputs, vec![Some(1), Some(0)]);
        assert_eq!(m.outputs, vec![None, None]);
    }
}
