// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Import of `.sesx` multitrack session files.
//!
//! Reads what an edit needs to carry over: tracks (name, mono or
//! stereo, height, mute, solo, arm, volume, pan, output), buses, clips
//! (position, source range, fades, lock, time stretch and pitch) and the
//! media files they use. VST3 plug-ins in effect racks (on clips, tracks,
//! buses and the master) come across with their saved settings, so they
//! open as they were. The other editor's own built-in effects and all
//! automation are left out; the result names the effects that were
//! skipped so the user is told.
//!
//! This reads the user's own session files for interoperability. It
//! only parses the documented-by-example XML structure; nothing from the
//! other application is reused.

pub mod export;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Serialize;
use thiserror::Error;

use crate::codecs::{probe, AudioInfo};
use crate::session::model::{
    new_id, Bus, ChannelLayout, ChannelMix, ChannelSend, Clip, ClipStretch, EffectChain,
    EffectSlot, Envelope, EnvelopeCurve, EnvelopePoint, EnvelopeTarget, Fade, FadeShape, Id,
    MediaRef, Output, Session, StretchAdvanced, Track, STRETCH_MAX, STRETCH_MIN,
};

#[derive(Debug, Error)]
pub enum SesxError {
    #[error("Could not read the file: {0}")]
    Io(#[from] std::io::Error),
    #[error("This is not a readable .sesx session: {0}")]
    Xml(String),
    #[error("This file has no multitrack session in it.")]
    NoSession,
}

/// What came across, for the notice shown after import.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub tracks: usize,
    pub clips: usize,
    /// Track, clip, bus and master effects that were left out.
    pub skipped_effects: usize,
    /// Their names, each once.
    pub skipped_names: Vec<String>,
    pub buses: usize,
    /// VST3 plug-ins brought in with their settings.
    pub plugins: usize,
    /// Plug-ins brought in that are not in the Plug-in Manager's list on
    /// this computer (not installed, or not scanned yet), each once.
    pub missing_plugins: Vec<String>,
    /// Clips with a time stretch or pitch change.
    pub stretched: usize,
    /// The other editor's built-in effects brought in as ours (its
    /// per-frequency phase shifter as Phase Curve).
    pub converted: usize,
    /// Media files that could not be found. Their clips are still placed.
    pub missing_files: Vec<PathBuf>,
}

#[derive(Debug)]
pub struct Imported {
    pub session: Session,
    pub report: ImportReport,
    /// Media ids not found on disk.
    pub missing: Vec<Id>,
}

struct FileEntry {
    absolute: Option<String>,
    relative: Option<String>,
    /// The channel count the other editor noted for the file.
    channels: Option<u16>,
}

fn attr<'a>(n: roxmltree::Node<'a, 'a>, name: &str) -> Option<&'a str> {
    n.attribute(name)
}

fn attr_u64(n: roxmltree::Node<'_, '_>, name: &str) -> Option<u64> {
    n.attribute(name)
        .and_then(|v| v.trim().parse::<f64>().ok())
        .map(|v| v.max(0.0).round() as u64)
}

fn child<'a>(n: roxmltree::Node<'a, 'a>, tag: &str) -> Option<roxmltree::Node<'a, 'a>> {
    n.children().find(|c| c.has_tag_name(tag))
}

/// Value of `<parameter name="...">` inside the component with `componentID`.
fn component_param(
    track_params: roxmltree::Node<'_, '_>,
    component: &str,
    param: &str,
) -> Option<f64> {
    track_params
        .children()
        .filter(|c| c.has_tag_name("component") && c.attribute("componentID") == Some(component))
        .flat_map(|c| c.children().filter(|p| p.has_tag_name("parameter")))
        .find(|p| p.attribute("name") == Some(param))
        .and_then(|p| p.attribute("parameterValue"))
        .and_then(|v| v.parse::<f64>().ok())
}

/// Most slots a Voice Chain holds.
const MAX_SLOTS: usize = 16;

/// An effects rack preset read from a preset file.
#[derive(Debug)]
pub struct RackPreset {
    pub name: String,
    pub chain: EffectChain,
}

/// Reads the effects rack presets in an XML preset file: every element
/// holding `<slot>`s is a rack, named by the nearest `name` (or
/// `presetName`) attribute above it. The other editor's built-in
/// effects come across as ours and VST3 plug-ins keep their settings,
/// as in a session import. `report` counts what was brought in or left out.
pub fn read_rack_presets(text: &str, report: &mut ImportReport) -> Result<Vec<RackPreset>, String> {
    let doc = roxmltree::Document::parse(text)
        .map_err(|e| format!("This file is not readable XML: {e}"))?;
    let mut out = Vec::new();
    for node in doc.descendants().filter(|n| n.is_element()) {
        if !node.children().any(|c| c.has_tag_name("slot")) {
            continue;
        }
        let name = node
            .ancestors()
            .find_map(|a| a.attribute("name").or_else(|| a.attribute("presetName")))
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .map_or_else(
                || format!("Imported Rack {}", out.len() + 1),
                str::to_string,
            );
        let chain = read_rack(node, report);
        if !chain.slots.is_empty() {
            out.push(RackPreset { name, chain });
        }
    }
    Ok(out)
}

/// Reads an effect rack into a Voice Chain: VST3 plug-ins with their
/// settings, in slot order. Anything else is counted as skipped.
fn read_rack(rack: roxmltree::Node<'_, '_>, report: &mut ImportReport) -> EffectChain {
    let mut chain = EffectChain::default();
    let mut slots: Vec<(u64, roxmltree::Node<'_, '_>)> = rack
        .children()
        .filter(|s| s.has_tag_name("slot"))
        .filter_map(|s| {
            let c = s.children().find(|c| c.has_tag_name("component"))?;
            Some((attr_u64(s, "slotIndex").unwrap_or(u64::MAX), c))
        })
        .collect();
    slots.sort_by_key(|(i, _)| *i);
    for (index, c) in slots {
        let name = c.attribute("name").unwrap_or("Effect").to_string();
        let slot = c.parent().unwrap_or(c);
        // The other editor's per-frequency phase shifter comes across as our Phase Curve.
        if c.attribute("componentID") == Some(PHASE_SHIFTER_ID) && chain.slots.len() < MAX_SLOTS {
            if let Some(params) = child(c, "presetChunkData")
                .and_then(|d| d.text())
                .and_then(phase_curve_params)
            {
                chain.slots.push(EffectSlot {
                    id: new_id(),
                    effect: "altaudio.phase_curve".into(),
                    enabled: slot.attribute("powered") != Some("false"),
                    params,
                    preset: None,
                    sidechain: None,
                    plugin_state: None,
                    data: None,
                    position: u32::try_from(index)
                        .ok()
                        .filter(|i| (*i as usize) < MAX_SLOTS),
                    routing: None,
                });
                report.converted += 1;
                continue;
            }
        }
        // The other editor's Parametric Equalizer comes across as our Parametric EQ.
        if c.attribute("componentID") == Some(PARAMETRIC_EQ_ID) && chain.slots.len() < MAX_SLOTS {
            let params = parametric_eq_params(c);
            chain.slots.push(EffectSlot {
                id: new_id(),
                effect: "altaudio.parametric_eq".into(),
                enabled: slot.attribute("powered") != Some("false")
                    && params.get("bypass").is_none_or(|v| *v < 0.5),
                params: params.into_iter().filter(|(k, _)| k != "bypass").collect(),
                preset: None,
                sidechain: None,
                plugin_state: None,
                data: None,
                position: u32::try_from(index)
                    .ok()
                    .filter(|i| (*i as usize) < MAX_SLOTS),
                routing: None,
            });
            report.converted += 1;
            continue;
        }
        // The other editor's vocal strip plug-in comes across as our Vox Strip.
        if c.attribute("componentID") == Some(XVOX_ID) && chain.slots.len() < MAX_SLOTS {
            if let Some((params, on)) = child(c, "presetChunkData")
                .and_then(|d| d.text())
                .and_then(vox_strip_params)
            {
                chain.slots.push(EffectSlot {
                    id: new_id(),
                    effect: crate::dsp::effects::vocal::VOCAL.into(),
                    enabled: on && slot.attribute("powered") != Some("false"),
                    params,
                    preset: None,
                    sidechain: None,
                    plugin_state: None,
                    data: None,
                    position: u32::try_from(index)
                        .ok()
                        .filter(|i| (*i as usize) < MAX_SLOTS),
                    routing: None,
                });
                report.converted += 1;
                continue;
            }
        }
        let plugin = c
            .attribute("componentID")
            .and_then(vst3_effect)
            .filter(|_| chain.slots.len() < MAX_SLOTS);
        let Some(effect) = plugin else {
            report.skipped_effects += 1;
            if !report.skipped_names.contains(&name) {
                report.skipped_names.push(name);
            }
            continue;
        };
        let state = child(c, "presetChunkData")
            .and_then(|d| d.text())
            .and_then(chunk_to_state);
        if crate::plugins::find(&effect).is_none() && !report.missing_plugins.contains(&name) {
            report.missing_plugins.push(name.clone());
        }
        report.plugins += 1;
        chain.slots.push(EffectSlot {
            id: new_id(),
            effect,
            enabled: slot.attribute("powered") != Some("false"),
            params: Default::default(),
            preset: None,
            sidechain: None,
            plugin_state: state,
            data: None,
            position: u32::try_from(index)
                .ok()
                .filter(|i| (*i as usize) < MAX_SLOTS),
            routing: None,
        });
    }
    chain
}

/// The channel count in a `<file>`'s importer settings ("Channels:0:2").
fn noted_channels(f: roxmltree::Node<'_, '_>) -> Option<u16> {
    f.attribute("importerPrivateSettings")?
        .split(';')
        .find_map(|kv| kv.strip_prefix("Channels:"))?
        .rsplit(':')
        .next()?
        .parse()
        .ok()
        .filter(|n| *n > 0)
}

/// A clip's `<channelMap>` as Source Channels: which of the file's
/// channels feed the track's. None when it is the file's usual mix
/// (each channel to its own side, or a mono file on both).
///
/// On a stereo track one listed channel plays on both sides (a mono
/// source), as the other editor does when a clip uses only the left or
/// right of a stereo file; two listed channels go left and right.
fn channel_map(
    c: roxmltree::Node<'_, '_>,
    file_channels: u16,
    stereo_track: bool,
) -> Option<ChannelMix> {
    let map: Vec<(usize, usize)> = child(c, "channelMap")?
        .children()
        .filter(|n| n.has_tag_name("channel"))
        .filter_map(|n| {
            Some((
                n.attribute("index")?.parse().ok()?,
                n.attribute("sourceIndex")?.parse().ok()?,
            ))
        })
        .collect();
    let n = usize::from(file_channels);
    if map.is_empty() || n == 0 || map.iter().any(|&(_, src)| src >= n) {
        return None;
    }
    let usual = if n == 1 {
        map.iter().all(|&(_, src)| src == 0)
    } else {
        map.len() == n.min(2) && map.iter().all(|&(i, src)| i == src)
    };
    if usual {
        return None;
    }
    let mut sends = vec![
        ChannelSend {
            left: 0.0,
            right: 0.0
        };
        n
    ];
    let one = map.len() == 1 || !stereo_track;
    for &(i, src) in &map {
        let s = &mut sends[src];
        if one {
            *s = ChannelSend {
                left: 1.0,
                right: 1.0,
            };
        } else if i == 0 {
            s.left = 1.0;
        } else {
            s.right = 1.0;
        }
    }
    Some(ChannelMix {
        sends,
        to_speakers: false,
    })
}

/// The component id of the other editor's Parametric Equalizer.
const PARAMETRIC_EQ_ID: &str = "8edcd326-33de-4a4d-985b-f1bf48ed1427";

/// dB from the flat middle (0.5) to either end of a gain knob: ±48 dB,
/// checked against the other editor's own readout (Band 2 at +6.3 dB).
const EQ_DB_HALF_SPAN: f64 = 48.0;

/// Parametric EQ settings from the other editor's equalizer (its knobs
/// are written out as 0 to 1 values with their names).
fn parametric_eq_params(c: roxmltree::Node<'_, '_>) -> std::collections::BTreeMap<String, f64> {
    let knob = |name: &str| {
        c.children()
            .find(|p| p.has_tag_name("parameter") && p.attribute("name") == Some(name))
            .and_then(|p| p.attribute("parameterValue"))
            .and_then(|v| v.trim().parse::<f64>().ok())
    };
    // Frequencies run from 20 Hz to half the rate the effect was set up
    // at, which it keeps as a share of 192 kHz (0.2297 is 44.1 kHz).
    let nyquist = knob("Original Sample Rate")
        .filter(|v| *v > 0.0)
        .map_or(22_050.0, |v| v * 192_000.0 / 2.0);
    let hz = |v: f64| (20.0 + v * (nyquist - 20.0)).round();
    let db = |v: f64| ((v - 0.5) * 2.0 * EQ_DB_HALF_SPAN * 100.0).round() / 100.0;
    let on = |v: f64| if v >= 0.5 { 1.0 } else { 0.0 };
    // Pass filter slopes step by 6 dB/oct from 12 (0.4 is 24 dB/oct); ours
    // are 12, 24 and 48, so the nearest of those.
    let slope = |v: f64| {
        let db = 12.0 + (v * 5.0).round() * 6.0;
        if db < 18.0 {
            0.0
        } else if db < 36.0 {
            1.0
        } else {
            2.0
        }
    };
    let mut out = std::collections::BTreeMap::new();
    let mut put = |key: &str, v: Option<f64>| {
        if let Some(v) = v {
            out.insert(key.to_string(), v);
        }
    };
    // The overall gain's middle-ish 2/3 is 0 dB.
    put(
        "gain",
        knob("Gain").map(|v| ((v * 60.0 - 40.0) * 10.0).round() / 10.0),
    );
    put("hp_on", knob("High-Pass Filter Enable").map(on));
    put("hp_freq", knob("High-Pass Cutoff Frequency").map(hz));
    put("hp_slope", knob("High-Pass Slope").map(slope));
    put("ls_on", knob("Low Shelf Enable").map(on));
    put("ls_freq", knob("Low Shelf Frequency").map(hz));
    put("ls_gain", knob("Low Shelf Gain").map(db));
    for b in 1..=5 {
        let name = |what: &str| format!("EQ Band {b} {what}");
        put(&format!("b{b}_on"), knob(&name("Enable")).map(on));
        put(
            &format!("b{b}_freq"),
            knob(&name("Center Frequency")).map(hz),
        );
        put(&format!("b{b}_gain"), knob(&name("Gain")).map(db));
        put(
            &format!("b{b}_q"),
            knob(&name("Q")).map(|v| ((v * 10_000.0) * 100.0).round() / 100.0),
        );
    }
    put("hs_on", knob("High Shelf Enable").map(on));
    put("hs_freq", knob("High Shelf Frequency").map(hz));
    put("hs_gain", knob("High Shelf Gain").map(db));
    put("lp_on", knob("Low-Pass Filter Enable").map(on));
    put("lp_freq", knob("Low-Pass Cutoff Frequency").map(hz));
    put("lp_slope", knob("Low-Pass Slope").map(slope));
    put("bypass", knob("Bypass"));
    out
}

/// The component id of the vocal strip plug-in some sessions use.
const XVOX_ID: &str = "3:abcdef01 9182faeb 58766f78 58766f78";

/// Vox Strip settings from that plug-in's saved settings (its state holds
/// a small XML list of 0 to 1 knobs; gains are linear, trims in dB, tone
/// knobs -1 to 1). Each knob goes to the nearest Vox Strip control.
/// Returns the settings and whether the strip was on.
fn vox_strip_params(text: &str) -> Option<(std::collections::BTreeMap<String, f64>, bool)> {
    let raw = altered_plughost::protocol::base64_decode(text)?;
    let xml = String::from_utf8_lossy(&raw);
    let start = xml.find("<XVox")?;
    let end = xml[start..]
        .find("</XVox>")
        .map_or(xml.len(), |e| start + e);
    let mut knobs = std::collections::BTreeMap::<String, f64>::new();
    for part in xml[start..end].split("<PARAM ").skip(1) {
        let id = part.split("id=\"").nth(1).and_then(|r| r.split('"').next());
        let value = part
            .split("value=\"")
            .nth(1)
            .and_then(|r| r.split('"').next())
            .and_then(|v| v.parse::<f64>().ok());
        if let (Some(id), Some(value)) = (id, value) {
            knobs.insert(id.to_string(), value);
        }
    }
    if knobs.is_empty() {
        return None;
    }
    let k = |name: &str| knobs.get(name).copied();
    let on = |name: &str| k(name).map(|v| if v >= 0.5 { 1.0 } else { 0.0 });
    let db = |gain: f64| 20.0 * gain.max(1e-4).log10();
    let mut out = std::collections::BTreeMap::new();
    let mut put = |key: &str, v: Option<f64>| {
        if let Some(v) = v {
            out.insert(key.to_string(), v);
        }
    };
    put("input", k("input_gain").map(db));
    put("output", k("output_gain").map(db));
    put("low_cut", on("hpf"));
    put("dyn_on", on("dyn_enable"));
    put("dyn_style", k("dyn_mode").map(f64::round));
    put("comp", k("dyn_comp").map(|v| (v * 100.0).round()));
    put("deess", k("deesser_amount").map(|v| (v * 100.0).round()));
    put("dyn_trim", k("dyn_trim"));
    put("tone_on", on("tone_enable"));
    put("tone_style", k("tone_mode").map(f64::round));
    put("low", k("tone_low").map(|v| v * 12.0));
    put("mid", k("tone_mid").map(|v| v * 12.0));
    put("high", k("tone_high").map(|v| v * 12.0));
    put("air", k("tone_exciter_gain").map(|v| (v * 100.0).round()));
    put("tone_trim", k("tone_trim"));
    put("fx_on", on("sfx_enable"));
    put("fx_style", k("sfx_mode").map(f64::round));
    put("fx", k("sfx_control").map(|v| (v * 100.0).round()));
    put("fx_trim", k("sfx_trim"));
    // The plug-in's space (reverb and delay) has no place in Vox Strip.
    let enabled = k("globalEnable").is_none_or(|v| v >= 0.5);
    Some((out, enabled))
}

/// The component id the other editor gives its per-frequency phase shifter.
const PHASE_SHIFTER_ID: &str = "7354704a-d87c-11e0-b05b-763c4824019b";

/// Phase Curve settings from that effect's saved settings (base64 of
/// little-endian 32-bit floats). Read from the tester's sessions: the
/// sample rate first; the low and high ends of the frequency axis at 6
/// and 7, as shares of half the sample rate; the number of points at 10;
/// then from 11 each point as two numbers from 0 to 1: where it sits
/// along the axis (taken as a log scale) and its phase (0.5 is 0°, the
/// ends ±180°). More points than Phase Curve holds are thinned evenly,
/// keeping the first and last.
fn phase_curve_params(text: &str) -> Option<std::collections::BTreeMap<String, f64>> {
    use crate::dsp::effects::phase_curve::{FREQ_KEYS, MAX_POINTS, PHASE_KEYS};
    let raw = altered_plughost::protocol::base64_decode(text)?;
    let f: Vec<f64> = raw
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| f64::from(f32::from_le_bytes(*b)))
        .collect();
    let rate = *f.first()?;
    let count = *f.get(10)? as usize;
    if !(8_000.0..=768_000.0).contains(&rate) || count < 2 || f.len() < 11 + count * 2 {
        return None;
    }
    let nyquist = rate / 2.0;
    let lo = (f[6] * nyquist).clamp(10.0, nyquist);
    let hi = (f[7] * nyquist).clamp(lo + 1.0, nyquist);
    let all: Vec<(f64, f64)> = (0..count)
        .map(|i| {
            let x = f[11 + i * 2].clamp(0.0, 1.0);
            let y = f[12 + i * 2].clamp(0.0, 1.0);
            (lo * (hi / lo).powf(x), (y - 0.5) * 360.0)
        })
        .collect();
    let keep: Vec<(f64, f64)> = if all.len() <= MAX_POINTS {
        all
    } else {
        (0..MAX_POINTS)
            .map(|k| all[(k * (all.len() - 1) + (MAX_POINTS - 1) / 2) / (MAX_POINTS - 1)])
            .collect()
    };
    let mut out = std::collections::BTreeMap::new();
    out.insert("points".to_string(), keep.len() as f64);
    for (i, (hz, deg)) in keep.iter().enumerate() {
        out.insert(FREQ_KEYS[i].to_string(), (hz * 10.0).round() / 10.0);
        out.insert(PHASE_KEYS[i].to_string(), (deg * 10.0).round() / 10.0);
    }
    Some(out)
}

/// Reads every effect rack directly inside `n` into one chain.
fn racks(n: roxmltree::Node<'_, '_>, report: &mut ImportReport) -> EffectChain {
    let mut chain = EffectChain::default();
    for r in n.children().filter(|c| c.has_tag_name("effectsRack")) {
        chain.slots.extend(read_rack(r, report).slots);
    }
    if chain.slots.len() > MAX_SLOTS {
        report.skipped_effects += chain.slots.len() - MAX_SLOTS;
        chain.slots.truncate(MAX_SLOTS);
    }
    chain
}

/// A VST3 component id in a `.sesx` ("3:" then four groups of eight hex
/// digits) as our effect id. The class id's bytes are laid out one way
/// on Windows and another elsewhere; whichever layout a scanned plug-in
/// uses wins, else this platform's.
fn vst3_effect(id: &str) -> Option<String> {
    let words: Vec<u32> = id
        .strip_prefix("3:")?
        .split_whitespace()
        .map(|w| u32::from_str_radix(w, 16).ok())
        .collect::<Option<_>>()?;
    let [a, b, c, d] = words[..] else {
        return None;
    };
    let hex = |bytes: Vec<u8>| -> String { bytes.iter().map(|x| format!("{x:02X}")).collect() };
    let plain = hex([a, b, c, d].iter().flat_map(|w| w.to_be_bytes()).collect());
    let mut com = a.to_le_bytes().to_vec();
    com.extend_from_slice(&((b >> 16) as u16).to_le_bytes());
    com.extend_from_slice(&(b as u16).to_le_bytes());
    com.extend_from_slice(&c.to_be_bytes());
    com.extend_from_slice(&d.to_be_bytes());
    let com = hex(com);
    let id = |cid: &str| format!("{}{cid}", crate::plugins::PREFIX);
    for cid in [&plain, &com] {
        if crate::plugins::find(&id(cid)).is_some() {
            return Some(id(cid));
        }
    }
    Some(id(if cfg!(windows) { &com } else { &plain }))
}

/// A plug-in's saved settings in a `.sesx` (base64 of: component length
/// and controller length as 64-bit numbers, then each part) in the form
/// our plug-in host reads (base64 of: component length as a 32-bit
/// number, then the two parts).
fn chunk_to_state(text: &str) -> Option<String> {
    use altered_plughost::protocol::{base64_decode, base64_encode};
    let raw = base64_decode(text)?;
    let comp = u64::from_le_bytes(raw.get(..8)?.try_into().ok()?) as usize;
    let ctl = u64::from_le_bytes(raw.get(8..16)?.try_into().ok()?) as usize;
    if comp.checked_add(ctl)?.checked_add(16)? != raw.len() {
        return None;
    }
    let mut out = Vec::with_capacity(raw.len() - 12);
    out.extend_from_slice(&u32::try_from(comp).ok()?.to_le_bytes());
    out.extend_from_slice(&raw[16..]);
    Some(base64_encode(&out))
}

/// A clip's `<clipStretch>`: speed, pitch and how. None when absent.
fn stretch(n: Option<roxmltree::Node<'_, '_>>) -> Option<ClipStretch> {
    let n = n?;
    let num = |k: &str| n.attribute(k).and_then(|v| v.trim().parse::<f64>().ok());
    let ratio = num("stretchRatio").unwrap_or(1.0);
    let kind = n.attribute("stretchType").unwrap_or("");
    let varispeed = kind.eq_ignore_ascii_case("varispeed") || kind.eq_ignore_ascii_case("resample");
    Some(ClipStretch {
        ratio: if ratio.is_finite() && ratio > 0.0 {
            ratio.clamp(STRETCH_MIN, STRETCH_MAX)
        } else {
            1.0
        },
        keep_pitch: !varispeed,
        pitch: if varispeed {
            0.0
        } else {
            num("pitchAdjustment").unwrap_or(0.0).clamp(-24.0, 24.0)
        },
        // "solo" is a single voice; anything else keeps several notes.
        polyphonic: !varispeed && !kind.eq_ignore_ascii_case("solo"),
        rendered: n.attribute("stretchMode") == Some("rendered"),
        advanced: Some(StretchAdvanced {
            window_ms: num("adaptiveWindowSize").unwrap_or(0.0).clamp(0.0, 200.0),
            transient_sensitivity: num("transientSensitivity")
                .unwrap_or(40.0)
                .clamp(0.0, 100.0),
            preserve_formants: n.attribute("preserveFormants") == Some("true"),
            high_precision: n.attribute("stretchQuality") == Some("high"),
        }),
    })
}

/// Level, mute, pan and any drawn volume line of a track or bus. Its
/// components sit in its audio parameters or straight inside the track,
/// depending on the version that saved the file, so both are looked at.
fn mix_params(nodes: &[Option<roxmltree::Node<'_, '_>>]) -> (f64, bool, f64, Option<Envelope>) {
    let first = |component: &str, param: &str| {
        nodes
            .iter()
            .flatten()
            .find_map(|n| component_param(*n, component, param))
    };
    let muted = first("Audition.Mute", "mute").is_some_and(|v| v >= 0.5);
    let pan = first("Audition.StereoPanner", "Pan").map_or(0.0, |p| {
        let p = if p.abs() <= 1.0 { p * 100.0 } else { p };
        p.clamp(-100.0, 100.0).round()
    });
    let (volume, envelope) = level(nodes, None);
    (volume, muted, pan, envelope)
}

/// The volume fader of a track, bus or clip: the first `Audition.Fader`
/// component among the nodes' children.
fn fader<'a, 'i>(nodes: &[Option<roxmltree::Node<'a, 'i>>]) -> Option<roxmltree::Node<'a, 'i>> {
    nodes.iter().flatten().find_map(|n| {
        n.children().find(|c| {
            c.has_tag_name("component") && c.attribute("componentID") == Some("Audition.Fader")
        })
    })
}

/// A fader's level in dB, and its volume line when one was drawn.
///
/// The fader keeps two gains that multiply: `volume` (the level you see
/// and draw on) and `static gain` (a fixed gain under it). A drawn line
/// keeps its points as `parameterKeyframe`s under `volume` (inside a
/// `parameterKeyframes` list in newer files), values as gains. Then the
/// line is the level, imported as a volume envelope, and only the static
/// gain stays as the fixed level.
///
/// A track's points count from the start of the session. A clip's count
/// from the start of its file, so `clip` gives the clip's source in point
/// and length: the points move to clip time, and a line shared by the
/// pieces of a split clip is cut to this piece, with its level at each
/// end worked out from the points either side.
fn level(
    nodes: &[Option<roxmltree::Node<'_, '_>>],
    clip: Option<(u64, u64)>,
) -> (f64, Option<Envelope>) {
    let Some(f) = fader(nodes) else {
        return (0.0, None);
    };
    let param = |name: &str| {
        f.children()
            .find(|p| p.has_tag_name("parameter") && p.attribute("name") == Some(name))
    };
    let value = |p: roxmltree::Node<'_, '_>, key: &str| {
        p.attribute(key).and_then(|v| v.trim().parse::<f64>().ok())
    };
    let fixed = param("static gain")
        .and_then(|p| value(p, "parameterValue"))
        .unwrap_or(1.0);
    let volume = param("volume");
    let mut raw: Vec<(f64, f64)> = volume
        .into_iter()
        .flat_map(|v| {
            v.descendants()
                .filter(|k| k.has_tag_name("parameterKeyframe"))
        })
        .filter_map(|k| Some((value(k, "sampleOffset")?, gain_to_db(value(k, "value")?))))
        .collect();
    raw.sort_by(|a, b| a.0.total_cmp(&b.0));
    if let Some((source_in, length)) = clip {
        raw = clip_line(&raw, source_in as f64, length as f64);
    }
    let mut points: Vec<EnvelopePoint> = raw
        .into_iter()
        .map(|(at, db)| EnvelopePoint {
            position: at.max(0.0).round() as u64,
            value: (db * 100.0).round() / 100.0,
            curve: EnvelopeCurve::Linear,
        })
        .collect();
    if points.is_empty() {
        let v = volume
            .and_then(|p| value(p, "parameterValue"))
            .unwrap_or(1.0);
        return (gain_to_db(v * fixed), None);
    }
    points.sort_by_key(|p| p.position);
    let envelope = Envelope {
        id: new_id(),
        target: EnvelopeTarget::Volume,
        enabled: true,
        points,
        height: None,
    };
    (gain_to_db(fixed), Some(envelope))
}

/// A line's points (position, dB) moved by `-origin` and cut to `0..=length`,
/// with points added at the ends where the line crosses them.
fn clip_line(points: &[(f64, f64)], origin: f64, length: f64) -> Vec<(f64, f64)> {
    let moved: Vec<(f64, f64)> = points.iter().map(|&(p, v)| (p - origin, v)).collect();
    if moved.is_empty() {
        return moved;
    }
    let at = |x: f64| -> f64 {
        let first = moved[0];
        let last = moved[moved.len() - 1];
        if x <= first.0 {
            return first.1;
        }
        if x >= last.0 {
            return last.1;
        }
        let k = (1..moved.len()).find(|&i| x <= moved[i].0).unwrap_or(1);
        let (a, b) = (moved[k - 1], moved[k]);
        if b.0 - a.0 <= 0.0 {
            b.1
        } else {
            a.1 + (b.1 - a.1) * (x - a.0) / (b.0 - a.0)
        }
    };
    let mut out = vec![(0.0, at(0.0))];
    out.extend(
        moved
            .iter()
            .copied()
            .filter(|&(p, _)| p > 0.0 && p < length),
    );
    if length > 0.0 {
        out.push((length, at(length)));
    }
    out
}

/// Where a track or bus sends its sound: a bus by its `.sesx` id, else the master.
fn output_of(a: Option<roxmltree::Node<'_, '_>>, buses: &HashMap<String, Id>) -> Output {
    a.and_then(|a| child(a, "trackOutput"))
        .filter(|o| o.attribute("type") == Some("trackID"))
        .and_then(|o| o.attribute("outputID"))
        .and_then(|id| buses.get(id))
        .map_or(Output::Master, |bus_id| Output::Bus { bus_id: *bus_id })
}

fn gain_to_db(gain: f64) -> f64 {
    if gain <= 0.000_001 {
        -96.0
    } else {
        ((20.0 * gain.log10()) * 10.0).round() / 10.0
    }
}

fn fade(node: Option<roxmltree::Node<'_, '_>>, fade_in: bool) -> Fade {
    let Some(n) = node else {
        return Fade::default();
    };
    let start = attr_u64(n, "startPoint").unwrap_or(0);
    let end = attr_u64(n, "endPoint").unwrap_or(0);
    let length = end.saturating_sub(start);
    let shape = match attr(n, "type") {
        Some("log") => FadeShape::Logarithmic,
        Some("cosine") => FadeShape::Cosine,
        _ => FadeShape::Linear,
    };
    let _ = fade_in;
    Fade {
        length,
        shape,
        curve: 0.0,
    }
}

/// Parses a `.sesx` file and builds a session. Media paths are tried
/// relative to the `.sesx` file first, then as stored.
pub fn import(path: &Path) -> Result<Imported, SesxError> {
    let text = std::fs::read_to_string(path)?;
    // Some files start with a byte order mark.
    let text = text.trim_start_matches('\u{feff}');
    let doc = roxmltree::Document::parse_with_options(
        text,
        roxmltree::ParsingOptions {
            allow_dtd: true,
            ..Default::default()
        },
    )
    .map_err(|e| SesxError::Xml(e.to_string()))?;
    let root = doc.root_element();
    let session_node = root
        .descendants()
        .find(|n| n.has_tag_name("session"))
        .ok_or(SesxError::NoSession)?;

    let sample_rate = attr_u64(session_node, "sampleRate")
        .unwrap_or(48_000)
        .clamp(8_000, 384_000) as u32;
    let name = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Imported Session".into());
    let mut session = Session::new(name, sample_rate);
    let mut report = ImportReport::default();

    // Files.
    let mut files: HashMap<String, FileEntry> = HashMap::new();
    if let Some(list) = root.descendants().find(|n| n.has_tag_name("files")) {
        for f in list.children().filter(|c| c.has_tag_name("file")) {
            if let Some(id) = f.attribute("id") {
                files.insert(
                    id.to_string(),
                    FileEntry {
                        absolute: f.attribute("absolutePath").map(str::to_string),
                        relative: f.attribute("relativePath").map(str::to_string),
                        channels: noted_channels(f),
                    },
                );
            }
        }
    }

    let base = path.parent().unwrap_or(Path::new(""));
    let probed = probe_files(&files, base);
    // File id in the .sesx to our media id, created on first use.
    let mut media_ids: HashMap<String, Id> = HashMap::new();
    let mut missing = Vec::new();

    let tracks_node = child(session_node, "tracks");
    // Buses first, so tracks can be routed to them.
    let mut bus_ids: HashMap<String, Id> = HashMap::new();
    let bus_nodes: Vec<_> = tracks_node
        .iter()
        .flat_map(|n| n.children())
        .filter(|n| n.has_tag_name("busTrack"))
        .collect();
    for b in &bus_nodes {
        let name = child(*b, "trackParameters")
            .and_then(|p| child(p, "name"))
            .and_then(|n| n.text())
            .map(str::to_string)
            .unwrap_or_else(|| format!("Bus {}", session.buses.len() + 1));
        let audio = child(*b, "trackAudioParameters");
        let mono = audio.and_then(|a| a.attribute("audioChannelType")) == Some("mono");
        let mut bus = Bus::new(
            name,
            if mono {
                ChannelLayout::Mono
            } else {
                ChannelLayout::Stereo
            },
        );
        let env;
        (bus.volume_db, bus.muted, bus.pan, env) = mix_params(&[audio, Some(*b)]);
        bus.envelopes.extend(env);
        if let Some(id) = b.attribute("id") {
            bus_ids.insert(id.to_string(), bus.id);
        }
        // Shown where it sat among the tracks.
        bus.position = Some(
            b.prev_siblings()
                .filter(|n| n.has_tag_name("audioTrack"))
                .count() as u32,
        );
        bus.effects = racks(*b, &mut report);
        session.buses.push(bus);
        report.buses += 1;
    }
    for (b, bus) in bus_nodes.iter().zip(session.buses.iter_mut()) {
        bus.output = output_of(child(*b, "trackAudioParameters"), &bus_ids);
        if let Output::Bus { bus_id } = bus.output {
            if bus_id == bus.id {
                bus.output = Output::Master;
            }
        }
    }
    for t in tracks_node
        .iter()
        .flat_map(|n| n.children())
        .filter(|n| n.has_tag_name("audioTrack"))
    {
        let params = child(t, "trackParameters");
        let audio = child(t, "trackAudioParameters");
        let track_name = params
            .and_then(|p| child(p, "name"))
            .and_then(|n| n.text())
            .map(str::to_string)
            .unwrap_or_else(|| format!("Track {}", session.tracks.len() + 1));
        let stereo = audio.and_then(|a| a.attribute("audioChannelType")) != Some("mono");
        let mut track = Track::new(
            track_name,
            if stereo {
                ChannelLayout::Stereo
            } else {
                ChannelLayout::Mono
            },
        );
        if let Some(h) = params.and_then(|p| attr_u64(p, "trackHeight")) {
            track.height = (h as u32).clamp(40, 600);
        }
        // The other editor's per-track render switch.
        track.prerender = t.attribute("preRendered") == Some("true");
        if let Some(a) = audio {
            track.solo = a.attribute("solo") == Some("true");
            track.record_armed = a.attribute("recordArmed") == Some("true");
        }
        let env;
        (track.volume_db, track.muted, track.pan, env) = mix_params(&[audio, Some(t)]);
        track.envelopes.extend(env);
        track.output = output_of(audio, &bus_ids);
        track.effects = racks(t, &mut report);

        // Lowest first: a later clip plays on top of an earlier one.
        let mut clip_nodes: Vec<_> = t
            .children()
            .filter(|n| n.has_tag_name("audioClip"))
            .collect();
        clip_nodes.sort_by_key(|c| attr_u64(*c, "zOrder").unwrap_or(0));
        for c in clip_nodes {
            let Some(file_key) = c.attribute("fileID") else {
                continue;
            };
            let clip_stretch = stretch(child(c, "clipStretch"));
            // A stretched clip's source points are in stretched time:
            // divide by the ratio for points in the file.
            let ratio = clip_stretch.map_or(1.0, |s| s.ratio);
            let in_file = |v: u64| (v as f64 / ratio).round() as u64;
            let source_in = in_file(attr_u64(c, "sourceInPoint").unwrap_or(0));
            let source_out = in_file(attr_u64(c, "sourceOutPoint").unwrap_or(0));
            if source_out <= source_in {
                continue;
            }
            let media_id = match media_ids.get(file_key) {
                Some(id) => *id,
                None => {
                    let entry = files.get(file_key);
                    let (media, found) =
                        resolve_media(probed.get(file_key), entry, sample_rate, stereo, source_out);
                    if !found {
                        missing.push(media.id);
                        report.missing_files.push(media.path.clone());
                    }
                    let id = media.id;
                    session.media.push(media);
                    media_ids.insert(file_key.to_string(), id);
                    id
                }
            };
            // Make sure the stored length covers every clip that uses it.
            if let Some(m) = session.media.iter_mut().find(|m| m.id == media_id) {
                if missing.contains(&media_id) {
                    m.length = m.length.max(source_out);
                }
            }
            let file_channels = session
                .media
                .iter()
                .find(|m| m.id == media_id)
                .map_or(2, |m| m.channels);
            let channel_mix = channel_map(c, file_channels, stereo);
            let effects = racks(c, &mut report);
            let (gain_db, clip_line) = level(
                &[Some(c)],
                Some((
                    attr_u64(c, "sourceInPoint").unwrap_or(0),
                    attr_u64(c, "endPoint")
                        .unwrap_or(0)
                        .saturating_sub(attr_u64(c, "startPoint").unwrap_or(0)),
                )),
            );
            if clip_stretch.is_some() {
                report.stretched += 1;
            }
            track.clips.push(Clip {
                id: new_id(),
                name: c.attribute("name").unwrap_or("Clip").to_string(),
                media_id,
                start: attr_u64(c, "startPoint").unwrap_or(0),
                source_in,
                source_out,
                gain_db,
                muted: component_param(c, "Audition.Mute", "mute").is_some_and(|v| v >= 0.5),
                locked: c.attribute("lockedInTime") == Some("true"),
                color: None,
                fade_in: fade(child(c, "fadeIn"), true),
                fade_out: fade(child(c, "fadeOut"), false),
                envelopes: clip_line.into_iter().collect(),
                effects,
                group: None,
                looped: None,
                stretch: clip_stretch,
                audio_type: None,
                channel_mix,
                reversed: false,
                inverted: false,
                invert_channel: None,
            });
            report.clips += 1;
        }
        session.tracks.push(track);
        report.tracks += 1;
    }

    if let Some(master) = tracks_node
        .iter()
        .flat_map(|n| n.children())
        .find(|n| n.has_tag_name("masterTrack"))
    {
        let (volume, _, _, env) =
            mix_params(&[child(master, "trackAudioParameters"), Some(master)]);
        session.master.volume_db = volume;
        session.master.envelopes.extend(env);
        session.master.effects = racks(master, &mut report);
    }

    Ok(Imported {
        session,
        report,
        missing,
    })
}

/// Finds and measures every file the session lists, all at the same
/// time, so a session with dozens of files opens in about the time of
/// its slowest file instead of the sum of them.
fn probe_files(
    files: &HashMap<String, FileEntry>,
    base: &Path,
) -> HashMap<String, (PathBuf, AudioInfo)> {
    use rayon::prelude::*;
    files
        .par_iter()
        .filter_map(|(k, e)| find_file(e, base).map(|f| (k.clone(), f)))
        .collect()
}

/// The file an entry names, where it is found: by its path relative to
/// the session, its absolute path, or its name next to the session.
fn find_file(e: &FileEntry, base: &Path) -> Option<(PathBuf, AudioInfo)> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(r) = &e.relative {
        candidates.push(base.join(r.replace('\\', "/")));
    }
    if let Some(a) = &e.absolute {
        candidates.push(PathBuf::from(a));
        // A Windows path read on another system: try its file name
        // next to the session as a last resort.
        if let Some(name) = a.rsplit(['\\', '/']).next() {
            candidates.push(base.join(name));
        }
    }
    candidates
        .into_iter()
        .filter(|p| p.is_file())
        .find_map(|p| probe(&p).ok().map(|info| (p, info)))
}

/// A media file as found and measured, or a placeholder built from the
/// session for a missing one, so its clips still show where they were.
fn resolve_media(
    found: Option<&(PathBuf, AudioInfo)>,
    entry: Option<&FileEntry>,
    session_rate: u32,
    stereo: bool,
    min_length: u64,
) -> (MediaRef, bool) {
    if let Some((p, info)) = found {
        return (
            MediaRef {
                id: new_id(),
                path: p.clone(),
                relative_path: None,
                sample_rate: info.sample_rate,
                channels: info.channels,
                length: info.length,
            },
            true,
        );
    }
    let stored = entry
        .and_then(|e| e.absolute.clone().or_else(|| e.relative.clone()))
        .unwrap_or_default();
    (
        MediaRef {
            id: new_id(),
            path: PathBuf::from(stored),
            relative_path: None,
            sample_rate: session_rate,
            channels: entry
                .and_then(|e| e.channels)
                .unwrap_or(if stereo { 2 } else { 1 }),
            length: min_length,
        },
        false,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="no" ?>
<!DOCTYPE sesx>
<sesx version="1.9">
  <session audioChannelType="stereo" bitDepth="24" duration="400000" sampleRate="44100">
    <tracks>
      <audioTrack id="10001" index="1">
        <trackParameters trackHeight="117">
          <name>HOST</name>
        </trackParameters>
        <trackAudioParameters audioChannelType="mono" recordArmed="false" solo="true">
          <component componentID="Audition.Fader" id="trackFader" name="volume" powered="true">
            <parameter index="0" name="volume" parameterValue="1.58489322662353515625"/>
          </component>
          <component componentID="Audition.Mute" id="trackMute" name="Mute" powered="true">
            <parameter index="1" name="mute" parameterValue="1"/>
          </component>
          <component componentID="Audition.StereoPanner" id="trackPan" name="StereoPanner" powered="true">
            <parameter index="0" name="Pan" parameterValue="-0.5"/>
          </component>
        </trackAudioParameters>
        <effectsRack preFader="true">
          <slot><component componentID="Noise.Thing" name="Noise"/></slot>
          <slot/>
          <slot><component componentID="Level.Thing" name="Level"/></slot>
        </effectsRack>
        <audioClip fileID="0" id="0" name="host take" sourceInPoint="1000" sourceOutPoint="23050" startPoint="44100" endPoint="66150" lockedInTime="true">
          <fadeIn startPoint="0" endPoint="4410" type="log"/>
          <fadeOut startPoint="17640" endPoint="22050" type="linear"/>
        </audioClip>
        <audioClip fileID="0" id="1" name="host take 2" sourceInPoint="0" sourceOutPoint="0" startPoint="0" endPoint="0"/>
      </audioTrack>
      <audioTrack id="10002" index="2">
        <trackParameters><name>MISSING</name></trackParameters>
        <trackAudioParameters audioChannelType="stereo"/>
        <audioClip fileID="1" id="0" name="gone" sourceInPoint="0" sourceOutPoint="88200" startPoint="0" endPoint="88200"/>
      </audioTrack>
      <masterTrack id="10000"><effectsRack><slot><component name="Limit"/></slot></effectsRack></masterTrack>
    </tracks>
  </session>
  <files>
    <file id="0" absolutePath="Z:\nowhere\host.wav" relativePath="Raw/host.wav"/>
    <file id="1" absolutePath="Z:\nowhere\gone.wav"/>
  </files>
</sesx>"#;

    #[test]
    fn imports_tracks_clips_and_finds_media_by_relative_path() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("Raw")).unwrap();
        let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/media/mono_44k_24.wav");
        std::fs::copy(&src, dir.path().join("Raw/host.wav")).unwrap();
        let sesx = dir.path().join("EP1.sesx");
        // Byte order mark, as found in real .sesx files.
        std::fs::write(&sesx, format!("\u{feff}{SAMPLE}")).unwrap();

        let out = import(&sesx).unwrap();
        let s = &out.session;
        assert_eq!(s.name, "EP1");
        assert_eq!(s.sample_rate, 44_100);
        assert_eq!(out.report.tracks, 2);
        assert_eq!(out.report.clips, 2, "the empty clip is skipped");
        assert_eq!(
            out.report.skipped_effects, 3,
            "two track effects and one master effect"
        );
        assert_eq!(out.report.skipped_names, vec!["Noise", "Level", "Limit"]);

        let host = &s.tracks[0];
        assert_eq!(host.name, "HOST");
        assert_eq!(host.channels, ChannelLayout::Mono);
        assert_eq!(host.height, 117);
        assert!(host.solo && host.muted);
        assert_eq!(host.volume_db, 4.0);
        assert_eq!(host.pan, -50.0);
        let clip = &host.clips[0];
        assert_eq!(
            (clip.start, clip.source_in, clip.source_out),
            (44_100, 1000, 23_050)
        );
        assert!(clip.locked);
        assert_eq!(clip.fade_in.length, 4410);
        assert_eq!(clip.fade_in.shape, FadeShape::Logarithmic);
        assert_eq!(clip.fade_out.length, 4410);

        let media = s.media_by_id(clip.media_id).unwrap();
        assert_eq!(media.path, dir.path().join("Raw/host.wav"));
        assert_eq!(
            (media.sample_rate, media.channels, media.length),
            (44_100, 1, 22_050)
        );

        // The missing file still gets a placeholder so its clip shows.
        assert_eq!(out.missing.len(), 1);
        assert_eq!(
            out.report.missing_files,
            vec![PathBuf::from("Z:\\nowhere\\gone.wav")]
        );
        let gone = s.media_by_id(s.tracks[1].clips[0].media_id).unwrap();
        assert_eq!(gone.length, 88_200);
        assert!(
            crate::session::validate::validate(s).is_empty(),
            "{:?}",
            crate::session::validate::validate(s)
        );
    }

    #[test]
    fn brings_in_plug_ins_buses_and_stretched_clips() {
        use altered_plughost::protocol::{base64_decode, base64_encode};
        // Settings as saved: 64-bit lengths (3 bytes of component, 2 of controller), then the data.
        let mut chunk = 3u64.to_le_bytes().to_vec();
        chunk.extend_from_slice(&2u64.to_le_bytes());
        chunk.extend_from_slice(b"abcde");
        let chunk = base64_encode(&chunk);
        let xml = format!(
            r#"<sesx><session sampleRate="48000"><tracks>
  <audioTrack id="10001">
    <trackParameters><name>VOX</name></trackParameters>
    <trackAudioParameters audioChannelType="stereo"><trackOutput outputID="10013" type="trackID"/></trackAudioParameters>
    <effectsRack preFader="true">
      <slot powered="true" slotIndex="1"><component componentID="3:56535441 54383141 75746f2d 54756e65" name="Tuner"><presetChunkData><![CDATA[{chunk}]]></presetChunkData></component></slot>
      <slot powered="false" slotIndex="0"><component componentID="7ea99d31-ca8f-454f-acd7-81a258efb336" name="Built-in Reverb"/></slot>
    </effectsRack>
    <audioClip fileID="0" name="line" sourceInPoint="10290" sourceOutPoint="20580" startPoint="0" endPoint="10290">
      <clipStretch adaptiveWindowSize="37" pitchAdjustment="2" preserveFormants="true" stretchMode="realtime" stretchQuality="high" stretchRatio="1.029" stretchType="solo" transientSensitivity="40"/>
    </audioClip>
  </audioTrack>
  <busTrack id="10013"><trackParameters><name>Bus A</name></trackParameters>
    <trackAudioParameters><trackOutput outputID="10000" type="trackID"/></trackAudioParameters>
  </busTrack>
  <masterTrack id="10000"><effectsRack><slot powered="false" slotIndex="0"><component componentID="3:abcdef01 9182faeb 55414478 55334349" name="Room"/></slot></effectsRack></masterTrack>
</tracks></session><files><file id="0" absolutePath="Z:\x.wav"/></files></sesx>"#
        );
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("x.sesx");
        std::fs::write(&p, xml).unwrap();
        let out = import(&p).unwrap();
        let s = &out.session;
        assert_eq!(out.report.buses, 1);
        assert_eq!(out.report.plugins, 2);
        assert_eq!(
            out.report.skipped_names,
            vec!["Built-in Reverb".to_string()]
        );
        assert_eq!(out.report.stretched, 1);
        let t = &s.tracks[0];
        assert_eq!(
            t.output,
            Output::Bus {
                bus_id: s.buses[0].id
            }
        );
        let slot = &t.effects.slots[0];
        let cid = if cfg!(windows) {
            "41545356385441317574"
        } else {
            "56535441543831417574"
        };
        assert!(
            slot.effect.starts_with(&format!("vst3:{cid}")),
            "{}",
            slot.effect
        );
        // Our form: a 32-bit component length, then both parts.
        let state = base64_decode(slot.plugin_state.as_deref().unwrap()).unwrap();
        assert_eq!(state, [&3u32.to_le_bytes()[..], b"abcde"].concat());
        assert!(!s.master.effects.slots[0].enabled);
        // Source points come back in the file's own time.
        let c = &t.clips[0];
        let st = c.stretch.unwrap();
        assert!((st.ratio - 1.029).abs() < 1e-9 && st.keep_pitch && !st.polyphonic);
        assert_eq!(st.pitch, 2.0);
        assert_eq!((c.source_in, c.source_out), (10_000, 20_000));
    }

    #[test]
    fn brings_in_the_phase_shifter_as_phase_curve() {
        // As saved in the tester's session: six points from 20 Hz up.
        let p = phase_curve_params("AEQsRwAAgD8AAIA/ED74PAAAAAAAAIA/uMVtOgAAgD8AAAAAAAAAAAAAwEAAAAAAAAAAP1OBhD4AADo/t6IPPwAASD+VLU4/AABmP0/ZYj8AAIA/AACAPwAAgD8=").unwrap();
        assert_eq!(p["points"], 6.0);
        assert_eq!((p["p1_freq"], p["p1_phase"]), (20.0, 0.0));
        assert_eq!(p["p6_freq"], 22_050.0);
        assert_eq!(p["p6_phase"], 180.0);
        // The second point: 0.2588 up a log axis from 20 Hz, phase +81.6°.
        assert!(
            (p["p2_freq"] - 20.0 * (22_050f64 / 20.0).powf(0.2588)).abs() < 1.0,
            "{}",
            p["p2_freq"]
        );
        assert!((p["p2_phase"] - 81.6).abs() < 0.2, "{}", p["p2_phase"]);
        assert!(phase_curve_params("bm90IGEgY2h1bms=").is_none());
    }

    #[test]
    fn rejects_files_that_are_not_sessions() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("x.sesx");
        std::fs::write(&p, "<notsesx/>").unwrap();
        assert!(matches!(import(&p), Err(SesxError::NoSession)));
        std::fs::write(&p, "not xml at all").unwrap();
        assert!(matches!(import(&p), Err(SesxError::Xml(_))));
    }

    #[test]
    fn the_vocal_strip_plug_in_comes_across_as_vox_strip() {
        use altered_plughost::protocol::base64_encode;
        let xml = r#"head <XVox><PARAM id="deesser_amount" value="0.5"/><PARAM id="dyn_comp" value="0.25"/><PARAM id="tone_low" value="-0.5"/><PARAM id="hpf" value="1.0"/><PARAM id="space_enable" value="0.0"/><PARAM id="globalEnable" value="1.0"/></XVox>"#;
        let (p, on) = vox_strip_params(&base64_encode(xml.as_bytes())).unwrap();
        assert!(on);
        assert_eq!(p["deess"], 50.0);
        assert_eq!(p["comp"], 25.0);
        assert_eq!(p["low"], -6.0);
        assert_eq!(p["low_cut"], 1.0);
        assert!(!p.contains_key("space_on"));
    }

    #[test]
    fn the_other_editors_parametric_equalizer_comes_across() {
        // Track 3 of a tester's session, with the other editor's readout:
        // L 59 Hz +4.4 dB, Band 2 134 Hz +6.3 dB Q 1, Band 3 937 Hz -2.5 dB,
        // H 8772 Hz -6.8 dB, high-pass 40 Hz at 24 dB/oct.
        let xml = r#"<component componentID="8edcd326-33de-4a4d-985b-f1bf48ed1427" name="Parametric Equalizer">
            <parameter index="0" name="Low Shelf Frequency" parameterValue="0.0017696"/>
            <parameter index="1" name="Low Shelf Gain" parameterValue="0.5460526"/>
            <parameter index="2" name="High Shelf Frequency" parameterValue="0.3972737"/>
            <parameter index="3" name="High Shelf Gain" parameterValue="0.4292763"/>
            <parameter index="8" name="EQ Band 2 Center Frequency" parameterValue="0.0051921"/>
            <parameter index="9" name="EQ Band 2 Q" parameterValue="0.0001"/>
            <parameter index="10" name="EQ Band 2 Gain" parameterValue="0.5657895"/>
            <parameter index="11" name="EQ Band 2 Enable" parameterValue="1"/>
            <parameter index="12" name="EQ Band 3 Center Frequency" parameterValue="0.0416182"/>
            <parameter index="14" name="EQ Band 3 Gain" parameterValue="0.4736842"/>
            <parameter index="24" name="Gain" parameterValue="0.6666669845581055"/>
            <parameter index="31" name="Original Sample Rate" parameterValue="0.22968749701976776"/>
            <parameter index="37" name="High-Pass Filter Enable" parameterValue="0"/>
            <parameter index="38" name="High-Pass Cutoff Frequency" parameterValue="0.0009078524"/>
            <parameter index="39" name="High-Pass Slope" parameterValue="0.4"/>
        </component>"#;
        let doc = roxmltree::Document::parse(xml).unwrap();
        let p = parametric_eq_params(doc.root_element());
        assert_eq!(p["ls_freq"], 59.0);
        assert_eq!(p["ls_gain"], 4.42);
        assert_eq!(p["b2_freq"], 134.0);
        assert_eq!(p["b2_gain"], 6.32);
        assert_eq!(p["b2_q"], 1.0);
        assert_eq!(p["b2_on"], 1.0);
        assert_eq!(p["b3_freq"], 937.0);
        assert_eq!(p["b3_gain"], -2.53);
        assert_eq!(p["hs_freq"], 8772.0);
        assert_eq!(p["hs_gain"], -6.79);
        assert_eq!(p["hp_freq"], 40.0);
        assert_eq!(p["hp_slope"], 1.0);
        assert_eq!(p["hp_on"], 0.0);
        assert_eq!(p["gain"], 0.0);
    }

    #[test]
    fn a_clip_using_one_side_of_a_stereo_file_keeps_to_that_side() {
        let right = roxmltree::Document::parse(
            r#"<audioClip><channelMap><channel index="0" sourceIndex="1"/></channelMap></audioClip>"#,
        )
        .unwrap();
        let mix = channel_map(right.root_element(), 2, true).unwrap();
        assert_eq!(
            mix.sends[0],
            ChannelSend {
                left: 0.0,
                right: 0.0
            }
        );
        assert_eq!(
            mix.sends[1],
            ChannelSend {
                left: 1.0,
                right: 1.0
            }
        );
        // Both sides as usual, or a mono file: nothing to set.
        let both = roxmltree::Document::parse(
            r#"<audioClip><channelMap><channel index="0" sourceIndex="0"/><channel index="1" sourceIndex="1"/></channelMap></audioClip>"#,
        )
        .unwrap();
        assert!(channel_map(both.root_element(), 2, true).is_none());
        let mono = roxmltree::Document::parse(
            r#"<audioClip><channelMap><channel index="0" sourceIndex="0"/></channelMap></audioClip>"#,
        )
        .unwrap();
        assert!(channel_map(mono.root_element(), 1, true).is_none());
        // Left only of a stereo file.
        let left = channel_map(mono.root_element(), 2, true).unwrap();
        assert_eq!(
            left.sends[0],
            ChannelSend {
                left: 1.0,
                right: 1.0
            }
        );
        assert_eq!(
            left.sends[1],
            ChannelSend {
                left: 0.0,
                right: 0.0
            }
        );
    }

    #[test]
    fn a_tracks_render_switch_comes_across_as_pre_render() {
        const SES: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<sesx version="1.9">
  <session sampleRate="44100" duration="100">
    <tracks>
      <audioTrack id="10001" index="0" preRendered="true"><trackParameters><name>On</name></trackParameters><trackAudioParameters audioChannelType="stereo"/></audioTrack>
      <audioTrack id="10002" index="1" preRendered="false"><trackParameters><name>Off</name></trackParameters><trackAudioParameters audioChannelType="stereo"/></audioTrack>
      <audioTrack id="10003" index="2"><trackParameters><name>None</name></trackParameters><trackAudioParameters audioChannelType="stereo"/></audioTrack>
    </tracks>
  </session>
  <files/>
</sesx>"#;
        let dir = tempfile::tempdir().unwrap();
        let sesx = dir.path().join("Render.sesx");
        std::fs::write(&sesx, SES).unwrap();
        let s = import(&sesx).unwrap().session;
        assert_eq!(
            s.tracks.iter().map(|t| t.prerender).collect::<Vec<_>>(),
            [true, false, false]
        );
    }

    #[test]
    fn imports_clip_and_track_levels_and_drawn_volume_lines() {
        const LEVELS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<sesx version="1.9">
  <session sampleRate="44100" duration="400000">
    <tracks>
      <audioTrack id="10001" index="1">
        <trackParameters><name>A</name></trackParameters>
        <trackAudioParameters audioChannelType="stereo"/>
        <component componentID="Audition.Fader" id="trackFader" name="volume" powered="true">
          <parameter index="0" name="volume" parameterValue="0.5"/>
          <parameter index="1" name="static gain" parameterValue="1"/>
        </component>
        <audioClip fileID="0" id="0" name="quiet" sourceInPoint="0" sourceOutPoint="44100" startPoint="0" endPoint="44100">
          <component componentID="Audition.Fader" id="clipGain" name="volume" powered="true">
            <parameter index="0" name="volume" parameterValue="0.5"/>
            <parameter index="1" name="static gain" parameterValue="0.5"/>
          </component>
        </audioClip>
        <audioClip fileID="0" id="1" name="drawn" sourceInPoint="0" sourceOutPoint="44100" startPoint="44100" endPoint="88200">
          <component componentID="Audition.Fader" id="clipGain" name="volume" powered="true">
            <parameter index="0" name="volume" parameterValue="1">
              <parameterKeyframe sampleOffset="22050" value="0.5"/>
              <parameterKeyframe sampleOffset="0" value="1"/>
            </parameter>
          </component>
        </audioClip>
      </audioTrack>
    </tracks>
  </session>
  <files><file id="0" absolutePath="Z:\nowhere\a.wav"/></files>
</sesx>"#;
        let dir = tempfile::tempdir().unwrap();
        let sesx = dir.path().join("Levels.sesx");
        std::fs::write(&sesx, LEVELS).unwrap();
        let s = import(&sesx).unwrap().session;
        let t = &s.tracks[0];
        // The track's fader sits straight inside the track in this file.
        assert_eq!(t.volume_db, -6.0);
        // Volume and static gain multiply: 0.5 x 0.5 is -12 dB.
        assert_eq!(t.clips[0].gain_db, -12.0);
        assert!(t.clips[0].envelopes.is_empty());
        // A drawn line comes in as a volume envelope, points in order.
        let drawn = &t.clips[1];
        assert_eq!(drawn.gain_db, 0.0);
        let env = &drawn.envelopes[0];
        assert_eq!(env.target, EnvelopeTarget::Volume);
        let pts: Vec<_> = env.points.iter().map(|p| (p.position, p.value)).collect();
        assert_eq!(pts, vec![(0, 0.0), (22050, -6.0), (44100, -6.0)]);
    }

    #[test]
    fn a_clip_line_counts_from_the_file_and_is_cut_to_the_clip() {
        // Points at 100 and 200 in the file; the clip plays the file from 150 for 100.
        let pts = clip_line(&[(100.0, 0.0), (200.0, -10.0)], 150.0, 100.0);
        assert_eq!(pts, vec![(0.0, -5.0), (50.0, -10.0), (100.0, -10.0)]);
        // A piece wholly after the points keeps the last level.
        assert_eq!(
            clip_line(&[(100.0, -3.0), (200.0, -6.0)], 500.0, 50.0),
            vec![(0.0, -6.0), (50.0, -6.0)]
        );
    }

    #[test]
    fn converts_gain_to_db() {
        assert_eq!(gain_to_db(1.0), 0.0);
        assert_eq!(gain_to_db(0.5), -6.0);
        assert_eq!(gain_to_db(0.0), -96.0);
    }

    #[test]
    fn reads_rack_presets_and_converts_built_in_effects() {
        let xml = r#"<presets>
          <preset name="Warm Vocal">
            <rack>
              <slot slotIndex="0"><component componentID="8edcd326-33de-4a4d-985b-f1bf48ed1427" name="Parametric Equalizer"/></slot>
              <slot slotIndex="1"><component componentID="3:11111111 22222222 33333333 44444444" name="Some Plug-in"/></slot>
            </rack>
          </preset>
          <preset name="Empty"><rack><slot slotIndex="0"><component componentID="nothing" name="Gone"/></slot></rack></preset>
        </presets>"#;
        let mut report = ImportReport::default();
        let got = read_rack_presets(xml, &mut report).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].name, "Warm Vocal");
        assert_eq!(got[0].chain.slots[0].effect, "altaudio.parametric_eq");
        assert!(got[0].chain.slots[1].effect.starts_with("vst3:"));
        assert_eq!(report.converted, 1);
        assert_eq!(report.skipped_names, vec!["Gone".to_string()]);
        assert!(read_rack_presets("not xml <", &mut report).is_err());
    }
}
