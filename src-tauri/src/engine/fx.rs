// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! A running Voice Chain with what playback needs around it: priming for
//! look-ahead, meter peaks, and the recent input a replacement chain is
//! warmed up on when the chain is edited while it plays.

use crate::dsp::effects::ChainProcessor;
use crate::session::model::{EffectChain, Id};

/// Extra input, beyond the look-ahead, a swapped-in chain is warmed up
/// on so its filters and level detectors settle first.
const WARM_SECONDS: f64 = 0.1;

pub(crate) struct Fx {
    pub chain: ChainProcessor,
    pub latency: u64,
    /// Clip chains read their input early by the latency; this says the
    /// frames in between were fed. Track, bus and master chains are
    /// aligned by the mix graph instead and start primed.
    pub primed: bool,
    pub buf: Vec<f32>,
    /// The latest input, for warming up a chain that replaces this one.
    hist: Vec<f32>,
    keep: usize,
    /// Peaks in and out since last read: left in, right in, left out, right out.
    pub peaks: [f32; 4],
    /// Channels per frame: 2, or one per speaker on a 3D Space master.
    ch: usize,
}

impl Fx {
    pub fn new(chain: &EffectChain, rate: u32) -> Option<Self> {
        ChainProcessor::new(chain, rate).map(|c| Self::from(c, rate))
    }

    /// A chain over channels named `names` (a 3D Space master's speakers).
    pub fn with_channels(chain: &EffectChain, rate: u32, names: &[String]) -> Option<Self> {
        ChainProcessor::with_channels(chain, rate, names)
            .map(|c| Self::from_ch(c, rate, names.len()))
    }

    pub fn from(c: ChainProcessor, rate: u32) -> Self {
        Self::from_ch(c, rate, 2)
    }

    fn from_ch(c: ChainProcessor, rate: u32, ch: usize) -> Self {
        let latency = c.latency() as u64;
        let ch = ch.max(1);
        Fx {
            ch,
            keep: (latency as usize + (f64::from(rate) * WARM_SECONDS) as usize) * ch,
            latency,
            chain: c,
            primed: false,
            buf: Vec::new(),
            hist: Vec::new(),
            peaks: [0.0; 4],
        }
    }

    /// Runs the chain over `buf` in place, keeping the input history
    /// and the meter peaks.
    pub fn run(&mut self, buf: &mut [f32]) {
        self.run_keyed(buf, &[]);
    }

    /// Runs the chain with sidechain keys by source id.
    pub fn run_keyed(&mut self, buf: &mut [f32], keys: &[(Id, &[f32])]) {
        self.hist.extend_from_slice(buf);
        if self.hist.len() > self.keep {
            let cut = self.hist.len() - self.keep;
            self.hist.drain(..cut);
        }
        // The meters show the first two channels (L and R).
        let ch = self.ch;
        for f in buf.chunks_exact(ch) {
            self.peaks[0] = self.peaks[0].max(f[0].abs());
            self.peaks[1] = self.peaks[1].max(f[ch.min(2) - 1].abs());
        }
        self.chain.process_keyed(buf, keys);
        for f in buf.chunks_exact(ch) {
            self.peaks[2] = self.peaks[2].max(f[0].abs());
            self.peaks[3] = self.peaks[3].max(f[ch.min(2) - 1].abs());
        }
    }
}

/// Puts `chain` in place of the chain in `slot` while it plays. False,
/// with nothing changed, when the look-ahead would change.
pub(crate) fn swap_fx(slot: &mut Option<Fx>, chain: &EffectChain, rate: u32) -> bool {
    swap_fx_ch(slot, chain, rate, &crate::dsp::effects::routing::stereo())
}

/// [`swap_fx`] for a chain over channels named `names`.
pub(crate) fn swap_fx_ch(
    slot: &mut Option<Fx>,
    chain: &EffectChain,
    rate: u32,
    names: &[String],
) -> bool {
    // The same effects with other settings: turn the running chain's dials
    // instead of building a new chain and warming it up, which was heard as
    // a short pause on every change.
    if let Some(old) = slot.as_mut() {
        if old.chain.retune(chain) {
            return true;
        }
    }
    let new = ChainProcessor::with_channels(chain, rate, names);
    let old_lat = slot.as_ref().map_or(0, |f| f.latency);
    if new.as_ref().map_or(0, |c| c.latency() as u64) != old_lat {
        return false;
    }
    let Some(c) = new else {
        *slot = None;
        return true;
    };
    let mut fx = Fx::from_ch(c, rate, names.len());
    match slot.take() {
        // Not started yet: it primes itself on its first block.
        Some(old) if !old.primed => {}
        // Plug-ins keep running between chains; they already heard it.
        Some(_) if fx.chain.has_plugins() => fx.primed = true,
        Some(old) => {
            // Warm up on what the old chain heard; drop the output.
            let mut warm = old.hist;
            fx.run(&mut warm);
            fx.peaks = [0.0; 4];
            fx.buf = warm;
            fx.primed = true;
        }
        // No chain before, so no look-ahead: nothing to prime.
        None => fx.primed = true,
    }
    *slot = Some(fx);
    true
}
