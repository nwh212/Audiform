// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Built-in effects and the Voice Chain that runs them.
//!
//! Every effect is described by data ([`catalog`]): its id, name, group
//! and parameters with ranges and defaults. The interface draws its
//! controls from that, so adding an effect needs no interface code.
//! Saved chains store parameter values by key; a missing key takes the
//! default and an unknown key is ignored, so old files always open.
//!
//! Processing runs on the render thread, not the audio callback, so
//! effects may keep state but must not block. Effects with look-ahead
//! report a latency, which the renderer compensates for.

pub mod amp;
pub mod center;
mod dynamics;
mod echo;
mod eq;
mod filters;
mod phase;
pub mod phase_curve;
mod pitch;
pub mod print;
mod restore;
pub mod reverb;
pub mod routing;
pub mod shaped;
pub mod tube;
pub mod vocal;
pub mod width;

use std::collections::BTreeMap;

use serde::Serialize;

use crate::session::model::{EffectChain, EffectSlot, Id};

pub use dynamics::{Compressor, DeEsser, Gate, Limiter, SpeechLeveler};
pub use eq::{HumCut, ParametricEq};
pub use restore::{find_spots, repair_spots, Spots};

/// Effects with a level detector that can listen to a sidechain key.
pub const KEYED: [&str; 2] = ["altaudio.dynamics", "altaudio.breath_gate"];

/// Most effects a chain holds.
pub const MAX_SLOTS: usize = 16;

/// How a parameter is shown and stored.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParamInfo {
    pub key: &'static str,
    pub name: &'static str,
    /// Shown after the value: "dB", "Hz", "ms", ":1", "" ...
    pub unit: &'static str,
    pub min: f64,
    pub max: f64,
    pub default: f64,
    pub step: f64,
    /// Logarithmic slider (frequencies, times).
    pub log: bool,
    /// For a list: the label of each whole value from `min`.
    #[serde(skip_serializing_if = "<[&str]>::is_empty")]
    pub choices: &'static [&'static str],
    /// An on/off switch (0 or 1).
    pub toggle: bool,
    /// Controls that belong together share a group name in the editor.
    #[serde(skip_serializing_if = "str::is_empty")]
    pub group: &'static str,
    /// Changing it changes the effect's look-ahead, so playback restarts
    /// to pick it up; the editor applies it on release, not while dragging.
    pub retimes: bool,
    /// Stored with the effect but not a control (a learned noise print).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub hidden: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectInfo {
    pub id: &'static str,
    pub name: &'static str,
    /// Menu group: "Filter and EQ", "Dynamics", "Amplitude", "Restoration", "Time and Pitch".
    pub category: &'static str,
    pub description: &'static str,
    pub params: Vec<ParamInfo>,
}

pub(crate) const fn num(
    key: &'static str,
    name: &'static str,
    unit: &'static str,
    min: f64,
    max: f64,
    default: f64,
    step: f64,
) -> ParamInfo {
    ParamInfo {
        key,
        name,
        unit,
        min,
        max,
        default,
        step,
        log: false,
        choices: &[],
        toggle: false,
        group: "",
        retimes: false,
        hidden: false,
    }
}

impl ParamInfo {
    pub(crate) const fn log(mut self) -> Self {
        self.log = true;
        self
    }
    pub(crate) const fn group(mut self, g: &'static str) -> Self {
        self.group = g;
        self
    }
    pub(crate) const fn hidden(mut self) -> Self {
        self.hidden = true;
        self
    }
}

pub(crate) const fn toggle(key: &'static str, name: &'static str, default: bool) -> ParamInfo {
    ParamInfo {
        key,
        name,
        unit: "",
        min: 0.0,
        max: 1.0,
        default: if default { 1.0 } else { 0.0 },
        step: 1.0,
        log: false,
        choices: &[],
        toggle: true,
        group: "",
        retimes: false,
        hidden: false,
    }
}

pub(crate) const fn choice(
    key: &'static str,
    name: &'static str,
    choices: &'static [&'static str],
    default: usize,
) -> ParamInfo {
    ParamInfo {
        key,
        name,
        unit: "",
        min: 0.0,
        max: (choices.len() - 1) as f64,
        default: default as f64,
        step: 1.0,
        log: false,
        choices,
        toggle: false,
        group: "",
        retimes: false,
        hidden: false,
    }
}

/// Every built-in effect, in menu order.
pub fn catalog() -> Vec<EffectInfo> {
    let [b10, b20, b30] = filters::band_infos();
    vec![
        eq::parametric_info(),
        b10,
        b20,
        b30,
        eq::hum_info(),
        filters::notch_info(),
        filters::precision_info(),
        filters::curve_info(),
        phase::phase_info(),
        phase_curve::phase_curve_info(),
        dynamics::compressor_info(),
        dynamics::limiter_info(),
        dynamics::gate_info(),
        dynamics::deesser_info(),
        dynamics::leveler_info(),
        tube::opto_info(),
        tube::preamp_info(),
        vocal::vocal_info(),
        filters::center_info(),
        center::center_extractor_info(),
        width::width_info(),
        reverb::reverb_info(),
        pitch::pitch_info(),
        echo::echo_info(),
        restore::noise_info(),
        print::print_info(),
        print::eraser_info(),
        shaped::shaped_info(),
        amp::amp_info(),
        restore::hiss_info(),
        restore::room_info(),
        restore::click_info(),
        restore::clip_info(),
        restore::dc_info(),
        EffectInfo {
            id: "altaudio.gain",
            name: "Gain",
            category: "Amplitude",
            description: "Turns the level up or down by a fixed amount.",
            params: vec![num("gain", "Gain", "dB", -48.0, 24.0, 0.0, 0.1)],
        },
    ]
}

/// The catalog for the editor, with each control marked when moving it
/// changes the effect's look-ahead (found by building the effect at the
/// control's lowest, default and highest values).
pub fn ui_catalog() -> Vec<EffectInfo> {
    let mut all = catalog();
    for e in &mut all {
        let id = e.id;
        let latency = |key: &str, v: f64| {
            let slot = EffectSlot {
                id: crate::session::model::new_id(),
                effect: id.to_string(),
                enabled: true,
                params: [(key.to_string(), v)].into_iter().collect(),
                preset: None,
                sidechain: None,
                plugin_state: None,
                data: None,
                position: None,
                routing: None,
            };
            build(&slot, 48_000).map_or(0, |p| p.latency())
        };
        for p in e.params.iter_mut().filter(|p| !p.hidden) {
            let base = latency(p.key, p.default);
            p.retimes = latency(p.key, p.min) != base || latency(p.key, p.max) != base;
        }
    }
    all
}

pub fn info(id: &str) -> Option<EffectInfo> {
    if crate::plugins::is_plugin(id) {
        return crate::plugins::find(id).map(|f| crate::plugins::effect_info(&f));
    }
    catalog().into_iter().find(|e| e.id == id)
}

/// The built-in effects and the installed plug-ins, for the editor.
pub fn full_catalog() -> Vec<EffectInfo> {
    let mut all = ui_catalog();
    all.extend(crate::plugins::catalog());
    all
}

/// A chain's total look-ahead, without starting any plug-in (a running
/// one reports its own; otherwise the scan's figure).
pub fn chain_latency(chain: &EffectChain, rate: u32) -> usize {
    if !chain.enabled {
        return 0;
    }
    chain
        .slots
        .iter()
        .take(MAX_SLOTS)
        .filter(|s| s.enabled)
        .map(|s| {
            if crate::plugins::is_plugin(&s.effect) {
                if crate::plugins::find(&s.effect).is_some() {
                    crate::plugins::instance::latency(s)
                } else {
                    0
                }
            } else {
                build(s, rate).map_or(0, |p| p.latency())
            }
        })
        .sum()
}

/// A slot's parameter values, each clamped to its range, with defaults
/// for missing ones.
#[derive(Debug, Clone)]
pub struct Params(BTreeMap<&'static str, f64>);

impl Params {
    pub fn resolve(info: &EffectInfo, saved: &BTreeMap<String, f64>) -> Self {
        Params(
            info.params
                .iter()
                .map(|p| {
                    let v = saved
                        .get(p.key)
                        .copied()
                        .filter(|v| v.is_finite())
                        .unwrap_or(p.default)
                        .clamp(p.min, p.max);
                    (p.key, v)
                })
                .collect(),
        )
    }

    /// Every value by key.
    pub fn values(&self) -> impl Iterator<Item = (&'static str, f64)> + '_ {
        self.0.iter().map(|(k, v)| (*k, *v))
    }

    pub fn get(&self, key: &str) -> f64 {
        self.0.get(key).copied().unwrap_or(0.0)
    }

    pub fn on(&self, key: &str) -> bool {
        self.get(key) >= 0.5
    }

    pub fn index(&self, key: &str) -> usize {
        self.get(key).round().max(0.0) as usize
    }
}

/// One running effect. Audio is stereo interleaved, processed in place.
pub trait Processor: Send {
    fn process(&mut self, buf: &mut [f32]);
    /// Frames of delay this effect adds (look-ahead).
    fn latency(&self) -> usize {
        0
    }
    /// Clears filter memory and envelopes, as after a jump.
    fn reset(&mut self);
    /// Takes new settings while playing, keeping its state, for
    /// automation. False when the effect cannot; the chain then rebuilds it.
    fn update(&mut self, _p: &Params) -> bool {
        false
    }
    /// True for a VST3 plug-in, which keeps running between chain builds
    /// and must not hear the same audio twice.
    fn is_plugin(&self) -> bool {
        false
    }
    /// Runs with a sidechain key, as long as `buf`: an effect with a
    /// level detector listens to `key` instead of its own input. Others
    /// ignore the key.
    fn process_keyed(&mut self, buf: &mut [f32], _key: &[f32]) {
        self.process(buf);
    }
    /// The effect's input and output channels by speaker name. Built-in
    /// effects are stereo; a plug-in may have its own layout.
    fn channels(&self) -> (Vec<String>, Vec<String>) {
        (routing::stereo(), routing::stereo())
    }
}

/// A fixed delay for a stereo interleaved stream.
#[derive(Debug, Clone, Default)]
pub struct Delay {
    ring: Vec<f32>,
    at: usize,
}

impl Delay {
    pub fn new(frames: usize) -> Self {
        Self::with_channels(frames, 2)
    }

    /// A delay of `frames` for `channels` interleaved channels.
    pub fn with_channels(frames: usize, channels: usize) -> Self {
        Self {
            ring: vec![0.0; frames * channels.max(1)],
            at: 0,
        }
    }

    pub fn frames(&self) -> usize {
        self.ring.len() / 2
    }

    /// Delays `buf` in place.
    pub fn run(&mut self, buf: &mut [f32]) {
        let n = self.ring.len();
        if n == 0 {
            return;
        }
        for s in buf.iter_mut() {
            std::mem::swap(s, &mut self.ring[self.at]);
            self.at = (self.at + 1) % n;
        }
    }

    pub fn clear(&mut self) {
        self.ring.fill(0.0);
        self.at = 0;
    }
}

struct Gain(f32);

impl Processor for Gain {
    fn process(&mut self, buf: &mut [f32]) {
        for s in buf {
            *s *= self.0;
        }
    }
    fn reset(&mut self) {}
    fn update(&mut self, p: &Params) -> bool {
        self.0 = db_to_gain(p.get("gain"));
        true
    }
}

pub(crate) fn db_to_gain(db: f64) -> f32 {
    10f64.powf(db / 20.0) as f32
}

/// Effects no longer offered (Phase Curve does what Phase Rotate did, and
/// the Center Channel Extractor what the Center Remover did, and more).
/// Sessions that use them still play them.
pub const RETIRED: [&str; 2] = ["altaudio.phase_rotate", "altaudio.center_remover"];

/// Builds the processor for one slot, or None for an unknown effect (or a
/// plug-in that is not installed or would not start).
pub fn build(slot: &EffectSlot, rate: u32) -> Option<Box<dyn Processor>> {
    if crate::plugins::is_plugin(&slot.effect) {
        return crate::plugins::instance::processor(slot, rate)
            .map(|p| Box::new(p) as Box<dyn Processor>);
    }
    let info = info(&slot.effect)?;
    let p = Params::resolve(&info, &slot.params);
    Some(match info.id {
        "altaudio.parametric_eq" => Box::new(ParametricEq::new(&p, rate)),
        "altaudio.hum_cut" => Box::new(HumCut::new(&p, rate)),
        "altaudio.dynamics" => Box::new(Compressor::new(&p, rate)),
        "altaudio.limiter" => Box::new(Limiter::new(&p, rate)),
        "altaudio.breath_gate" => Box::new(Gate::new(&p, rate)),
        "altaudio.deesser" => Box::new(DeEsser::new(&p, rate)),
        "altaudio.speech_leveler" => Box::new(SpeechLeveler::new(&p, rate)),
        "altaudio.gain" => Box::new(Gain(db_to_gain(p.get("gain")))),
        "altaudio.noise_reducer" => Box::new(restore::noise_reducer(&p, rate)),
        "altaudio.noise_print" => Box::new(print::print_reducer(&p, rate)),
        "altaudio.sound_eraser" => Box::new(print::sound_eraser(&p, rate)),
        shaped::SHAPED_NOISE => Box::new(shaped::shaped_reducer(&p, rate)),
        amp::AMP => Box::new(amp::AmpModel::new(&p, rate, slot.data.as_deref())),
        tube::OPTO => Box::new(tube::Opto::new(
            &p,
            rate,
            crate::dsp::meters::for_slot(slot.id),
        )),
        tube::PREAMP => Box::new(tube::Preamp::new(
            &p,
            rate,
            crate::dsp::meters::for_slot(slot.id),
        )),
        vocal::VOCAL => Box::new(vocal::VoxStrip::new(
            &p,
            rate,
            crate::dsp::meters::for_slot(slot.id),
        )),
        "altaudio.hiss_reducer" => Box::new(restore::hiss_reducer(&p, rate)),
        "altaudio.room_reducer" => Box::new(restore::room_reducer(&p, rate)),
        "altaudio.click_repair" => Box::new(restore::GapRepair::clicks(&p, rate)),
        "altaudio.clip_repair" => Box::new(restore::GapRepair::clipping(&p, rate)),
        "altaudio.dc_offset" => Box::new(restore::DcBlock::new(rate)),
        "altaudio.band_eq_10" => Box::new(filters::BandEq::new(10, &p, rate)),
        "altaudio.band_eq_20" => Box::new(filters::BandEq::new(20, &p, rate)),
        "altaudio.band_eq_30" => Box::new(filters::BandEq::new(30, &p, rate)),
        "altaudio.notch" => Box::new(filters::Notches::new(&p, rate)),
        "altaudio.precision_filter" => Box::new(filters::Precision::new(&p, rate)),
        "altaudio.curve_eq" => Box::new(filters::CurveEq::new(&p, rate)),
        "altaudio.center_remover" => Box::new(filters::CenterRemover::new(&p, rate)),
        "altaudio.pitch_shift" => Box::new(pitch::PitchShift::new(&p, rate)),
        "altaudio.echo" => Box::new(echo::Echo::new(&p, rate)),
        "altaudio.phase_rotate" => Box::new(phase::PhaseRotate::new(&p, rate)),
        "altaudio.phase_curve" => Box::new(phase_curve::PhaseCurve::new(&p, rate)),
        center::ID => Box::new(center::CenterExtractor::new(&p, rate)),
        width::WIDTH => Box::new(width::StereoWidth::new(&p, rate)),
        reverb::ID => Box::new(reverb::ConvolutionReverb::new(
            &p,
            rate,
            slot.data.as_deref(),
        )),
        _ => return None,
    })
}

/// True when `chain` would change the sound: the same test
/// [`ChainProcessor::new`] makes, without building anything.
pub fn is_active(chain: &EffectChain) -> bool {
    chain.enabled
        && (chain.input_gain_db.clamp(-48.0, 24.0) != 0.0
            || chain.output_gain_db.clamp(-48.0, 24.0) != 0.0
            || chain
                .slots
                .iter()
                .take(MAX_SLOTS)
                .any(|s| s.enabled && info(&s.effect).is_some()))
}

/// A Voice Chain ready to run: input gain, each enabled effect in order,
/// output gain, then the dry/wet mix.
/// A running stage's slot: id, effect, settings and its own data, kept
/// so a stage can be rebuilt when automation needs it.
type SlotState = (Id, EffectInfo, BTreeMap<String, f64>, Option<String>);

/// A stage that does not simply run on the chain's channels as they
/// are: what it hears is gathered from the chain, and what it makes is
/// put back, by its routing.
struct Route {
    map: routing::Map,
    /// Channels per frame the effect runs with.
    width: usize,
    buf: Vec<f32>,
    /// Holds the channels the effect does not write back by its look-ahead.
    pass: Delay,
    /// Chain channels some output goes to.
    targets: Vec<bool>,
}

pub struct ChainProcessor {
    stages: Vec<Box<dyn Processor>>,
    /// Per stage: its routing, or None when it runs straight.
    routes: Vec<Option<Route>>,
    /// Channels per frame: 2, or one per speaker on a 3D Space master.
    ch: usize,
    /// Per stage: the track or bus keying it, and a delay that holds the
    /// key back by the look-ahead of the stages before it, so key and
    /// signal still line up when they reach it.
    keys: Vec<Option<(Id, Delay)>>,
    key_buf: Vec<f32>,
    /// Per stage: its slot id, what it is and its current settings, so
    /// automation can change one setting while it plays.
    slots: Vec<SlotState>,
    rate: u32,
    input: f32,
    output: f32,
    mix: f32,
    /// The dry signal, delayed to line up with the effects when they add
    /// latency. Grows once to the largest block seen.
    dry: Vec<f32>,
    dry_delay: Vec<f32>,
    dry_at: usize,
    latency: usize,
}

impl ChainProcessor {
    /// None when the chain would do nothing (off, empty, or every effect
    /// bypassed with unity gains), so the renderer can skip it entirely.
    pub fn new(chain: &EffectChain, rate: u32) -> Option<Self> {
        Self::with_channels(chain, rate, &routing::stereo())
    }

    /// A chain over channels named `names` (one per speaker on the
    /// master of a 3D Space session), each effect routed to them.
    pub fn with_channels(chain: &EffectChain, rate: u32, names: &[String]) -> Option<Self> {
        if !chain.enabled {
            return None;
        }
        let ch = names.len().max(1);
        let mut routes = Vec::new();
        let mut stages: Vec<Box<dyn Processor>> = Vec::new();
        let mut slots = Vec::new();
        let mut keys = Vec::new();
        let mut before = 0;
        for s in chain.slots.iter().take(MAX_SLOTS).filter(|s| s.enabled) {
            if let (Some(p), Some(i)) = (build(s, rate), info(&s.effect)) {
                keys.push(s.sidechain.map(|id| (id, Delay::new(before))));
                let (ins, outs) = p.channels();
                let map = routing::resolve(s.routing.as_ref(), &ins, &outs, names);
                routes.push(
                    (!(map.is_straight(ch) && ins.len() == ch && outs.len() == ch)).then(|| {
                        let mut targets = vec![false; ch];
                        for c in map.outputs.iter().flatten() {
                            targets[*c] = true;
                        }
                        Route {
                            width: ins.len().max(outs.len()).max(1),
                            buf: Vec::new(),
                            pass: Delay::with_channels(p.latency(), ch),
                            targets,
                            map,
                        }
                    }),
                );
                before += p.latency();
                stages.push(p);
                slots.push((s.id, i, s.params.clone(), s.data.clone()));
            }
        }
        let input = db_to_gain(chain.input_gain_db.clamp(-48.0, 24.0));
        let output = db_to_gain(chain.output_gain_db.clamp(-48.0, 24.0));
        let mix = chain.mix.clamp(0.0, 1.0) as f32;
        if stages.is_empty() && input == 1.0 && output == 1.0 {
            return None;
        }
        let latency = stages.iter().map(|s| s.latency()).sum();
        Some(Self {
            stages,
            routes,
            ch,
            keys,
            key_buf: Vec::new(),
            slots,
            rate,
            input,
            output,
            mix,
            dry: Vec::new(),
            dry_delay: vec![0.0; latency * ch],
            dry_at: 0,
            latency,
        })
    }

    pub fn latency(&self) -> usize {
        self.latency
    }

    /// True when a stage is a running plug-in, which must not be warmed up
    /// on audio it has already heard.
    pub fn has_plugins(&self) -> bool {
        self.stages.iter().any(|s| s.is_plugin())
    }

    /// Changes one setting of one effect while it plays (automation). An
    /// effect that cannot take it in place is rebuilt, unless that would
    /// change the chain's look-ahead.
    pub fn set_param(&mut self, slot: Id, key: &str, value: f64) {
        let Some(i) = self.slots.iter().position(|s| s.0 == slot) else {
            return;
        };
        let (id, info, params, data) = &mut self.slots[i];
        if params.get(key).is_some_and(|v| (v - value).abs() < 1e-6) {
            return;
        }
        params.insert(key.to_string(), value);
        let p = Params::resolve(info, params);
        if self.stages[i].update(&p) {
            return;
        }
        let slot = EffectSlot {
            id: *id,
            effect: info.id.to_string(),
            enabled: true,
            params: params.clone(),
            preset: None,
            sidechain: None,
            plugin_state: None,
            data: data.clone(),
            position: None,
            routing: None,
        };
        if let Some(fresh) = build(&slot, self.rate) {
            if fresh.latency() == self.stages[i].latency() {
                self.stages[i] = fresh;
            }
        }
    }

    pub fn reset(&mut self) {
        for s in &mut self.stages {
            s.reset();
        }
        for (_, d) in self.keys.iter_mut().flatten() {
            d.clear();
        }
        self.dry_delay.fill(0.0);
        self.dry_at = 0;
    }

    /// The tracks and buses this chain's effects listen to.
    pub fn key_sources(&self) -> impl Iterator<Item = Id> + '_ {
        self.keys.iter().flatten().map(|(id, _)| *id)
    }

    pub fn process(&mut self, buf: &mut [f32]) {
        self.process_keyed(buf, &[]);
    }

    /// Runs the chain with sidechain keys by source id, each as long as
    /// `buf`. A stage whose source has no key here listens to its input.
    pub fn process_keyed(&mut self, buf: &mut [f32], keys: &[(Id, &[f32])]) {
        let blend = self.mix < 1.0;
        if blend {
            self.dry.clear();
            self.dry.extend_from_slice(buf);
            // Delay the dry copy by the chain's latency so both line up.
            if !self.dry_delay.is_empty() {
                let n = self.dry_delay.len();
                for s in self.dry.iter_mut() {
                    std::mem::swap(s, &mut self.dry_delay[self.dry_at]);
                    self.dry_at = (self.dry_at + 1) % n;
                }
            }
        }
        if self.input != 1.0 {
            for s in buf.iter_mut() {
                *s *= self.input;
            }
        }
        let ch = self.ch;
        let frames = buf.len() / ch;
        for ((st, key), route) in self
            .stages
            .iter_mut()
            .zip(&mut self.keys)
            .zip(&mut self.routes)
        {
            // Keys are stereo: they reach stages that run on two channels.
            let found = key.as_mut().and_then(|(id, delay)| {
                keys.iter()
                    .find(|(k, b)| k == id && b.len() == frames * 2)
                    .map(|(_, b)| (delay, *b))
            });
            let key = match found {
                Some((delay, k)) => {
                    self.key_buf.clear();
                    self.key_buf.extend_from_slice(k);
                    delay.run(&mut self.key_buf);
                    Some(self.key_buf.as_slice())
                }
                None => None,
            };
            let Some(r) = route.as_mut() else {
                match key {
                    Some(k) if ch == 2 => st.process_keyed(buf, k),
                    _ => st.process(buf),
                }
                continue;
            };
            let w = r.width;
            r.buf.clear();
            r.buf.resize(frames * w, 0.0);
            for f in 0..frames {
                for (k, src) in r.map.inputs.iter().enumerate().take(w) {
                    if let Some(c) = src {
                        r.buf[f * w + k] = buf[f * ch + c];
                    }
                }
            }
            match key {
                Some(k) if w == 2 => st.process_keyed(&mut r.buf, k),
                _ => st.process(&mut r.buf),
            }
            r.pass.run(buf);
            for f in 0..frames {
                let o = &mut buf[f * ch..f * ch + ch];
                for (v, t) in o.iter_mut().zip(&r.targets) {
                    if *t {
                        *v = 0.0;
                    }
                }
                for (k, dst) in r.map.outputs.iter().enumerate().take(w) {
                    if let Some(c) = dst {
                        o[*c] += r.buf[f * w + k];
                    }
                }
            }
        }
        let out = self.output;
        if blend {
            let wet = self.mix;
            let dry = 1.0 - wet;
            for (s, d) in buf.iter_mut().zip(&self.dry) {
                *s = *s * out * wet + *d * dry;
            }
        } else if out != 1.0 {
            for s in buf.iter_mut() {
                *s *= out;
            }
        }
    }
}

/// A short, stable fingerprint of a chain, so playback knows when it
/// changed.
pub fn chain_hash(chain: &EffectChain) -> u64 {
    let text = serde_json::to_string(chain).unwrap_or_default();
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controls_that_move_the_look_ahead_are_marked() {
        let all = ui_catalog();
        let param = |e: &str, k: &str| {
            all.iter()
                .find(|x| x.id == e)
                .unwrap()
                .params
                .iter()
                .find(|p| p.key == k)
                .unwrap()
                .retimes
        };
        assert!(param("altaudio.limiter", "lookahead"));
        assert!(!param("altaudio.limiter", "ceiling"));
        assert!(!param("altaudio.gain", "gain"));
    }
    use crate::session::model::new_id;

    pub(crate) fn slot(effect: &str, params: &[(&str, f64)]) -> EffectSlot {
        EffectSlot {
            id: new_id(),
            effect: effect.into(),
            enabled: true,
            params: params.iter().map(|(k, v)| ((*k).to_string(), *v)).collect(),
            preset: None,
            sidechain: None,
            plugin_state: None,
            data: None,
            position: None,
            routing: None,
        }
    }

    #[test]
    fn keyed_effects_exist() {
        for id in KEYED {
            assert!(info(id).is_some(), "{id}");
        }
    }

    #[test]
    fn catalog_is_well_formed() {
        let all = catalog();
        let mut ids: Vec<&str> = all.iter().map(|e| e.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), all.len(), "ids are unique");
        for e in &all {
            assert!(e.id.starts_with("altaudio."));
            for p in &e.params {
                assert!(
                    p.min <= p.default && p.default <= p.max,
                    "{} {}",
                    e.id,
                    p.key
                );
                assert!(
                    !p.log || p.min > 0.0,
                    "log needs a positive range: {}",
                    p.key
                );
            }
            // Every catalog entry builds.
            assert!(build(&slot(e.id, &[]), 48_000).is_some(), "{}", e.id);
        }
    }

    #[test]
    fn params_take_defaults_and_clamp() {
        let i = info("altaudio.gain").unwrap();
        let mut saved = BTreeMap::new();
        assert_eq!(Params::resolve(&i, &saved).get("gain"), 0.0);
        saved.insert("gain".into(), 99.0);
        saved.insert("unknown".into(), 1.0);
        assert_eq!(Params::resolve(&i, &saved).get("gain"), 24.0);
        saved.insert("gain".into(), f64::NAN);
        assert_eq!(Params::resolve(&i, &saved).get("gain"), 0.0);
    }

    #[test]
    fn chain_applies_gains_mix_and_skips_when_idle() {
        let mut c = EffectChain::default();
        assert!(
            ChainProcessor::new(&c, 48_000).is_none(),
            "empty chain is skipped"
        );
        c.slots.push(slot("altaudio.gain", &[("gain", -6.0206)]));
        let mut p = ChainProcessor::new(&c, 48_000).unwrap();
        let mut b = vec![1.0f32; 8];
        p.process(&mut b);
        assert!((b[0] - 0.5).abs() < 1e-3);
        c.mix = 0.5;
        let mut p = ChainProcessor::new(&c, 48_000).unwrap();
        let mut b = vec![1.0f32; 8];
        p.process(&mut b);
        assert!((b[0] - 0.75).abs() < 1e-3, "{}", b[0]);
        c.slots[0].enabled = false;
        c.mix = 1.0;
        assert!(ChainProcessor::new(&c, 48_000).is_none(), "all bypassed");
        c.enabled = false;
        c.slots[0].enabled = true;
        assert!(ChainProcessor::new(&c, 48_000).is_none(), "chain power off");
    }

    #[test]
    fn sidechain_key_drives_the_detector_after_earlier_look_ahead() {
        let src = new_id();
        let mut c = EffectChain::default();
        // A limiter with 1 ms look-ahead, then a gate keyed by `src`.
        c.slots.push(slot(
            "altaudio.limiter",
            &[("lookahead", 1.0), ("ceiling", 0.0)],
        ));
        let mut gate = slot(
            "altaudio.breath_gate",
            &[("threshold", -20.0), ("attack", 0.1), ("range", -100.0)],
        );
        gate.sidechain = Some(src);
        c.slots.push(gate);
        let mut p = ChainProcessor::new(&c, 48_000).unwrap();
        assert_eq!(p.key_sources().collect::<Vec<_>>(), vec![src]);
        let frames = 4_000;
        // Steady signal; the key is silent, then loud from frame 2,000.
        let mut buf = vec![0.3f32; frames * 2];
        let mut key = vec![0f32; frames * 2];
        key[4_000..].fill(0.5);
        p.process_keyed(&mut buf, &[(src, &key)]);
        let lat = p.latency();
        assert!(
            buf[(1_500 + lat) * 2].abs() < 1e-6,
            "closed while the key is quiet"
        );
        assert!(buf[(2_100 + lat) * 2] > 0.29, "open once the key is loud");
        assert!(
            buf[(2_000 + lat - 5) * 2].abs() < 1e-6,
            "and not before: the key is held back by the limiter's look-ahead"
        );
        // Without its key the gate hears its own input and stays open.
        let mut p = ChainProcessor::new(&c, 48_000).unwrap();
        let mut buf = vec![0.3f32; frames * 2];
        p.process(&mut buf);
        assert!(buf[(1_500 + lat) * 2] > 0.29);
    }

    #[test]
    fn dry_path_is_delayed_with_the_effects() {
        let mut c = EffectChain::default();
        c.slots.push(slot(
            "altaudio.limiter",
            &[("lookahead", 1.0), ("ceiling", 0.0)],
        ));
        c.mix = 0.0;
        let mut p = ChainProcessor::new(&c, 48_000).unwrap();
        let lat = p.latency();
        assert_eq!(lat, 48);
        let mut b = vec![0f32; 200 * 2];
        b[0] = 0.5;
        b[1] = 0.5;
        p.process(&mut b);
        // Fully dry, but the impulse arrives after the chain's latency.
        assert_eq!(b[lat * 2], 0.5);
        assert_eq!(b[0], 0.0);
    }
}
