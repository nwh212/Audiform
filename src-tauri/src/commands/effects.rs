// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! The built-in effects, for the Voice Chain panel and the Effects menu,
//! and learning a noise print for the Noise Print Reducer.

use std::collections::BTreeMap;
use std::sync::atomic::AtomicBool;

use serde::{Deserialize, Serialize};
use tauri::async_runtime::spawn_blocking;
use tauri::State;

use super::session::OpenSession;
use super::wave::{without_effects, OpenWave};
use crate::dsp::effects::print::{learn, print_params};
use crate::dsp::effects::{build, full_catalog, EffectInfo, KEYED};
use crate::export::mixdown::{self, MixdownRequest};
use crate::session::model::{EffectSlot, Id, Session};

/// An effect for the editor, marked when it can listen to a sidechain,
/// and for a plug-in, whether it has a window of its own.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogEntry {
    #[serde(flatten)]
    info: EffectInfo,
    sidechain: bool,
    plugin: bool,
    has_window: bool,
    /// Kept so older sessions still play it, but no longer offered.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    retired: bool,
}

#[tauri::command]
pub fn effects_catalog() -> Vec<CatalogEntry> {
    full_catalog()
        .into_iter()
        .map(|info| {
            let found = crate::plugins::find(info.id);
            CatalogEntry {
                sidechain: KEYED.contains(&info.id)
                    || found.as_ref().is_some_and(|f| f.info.sidechain),
                plugin: found.is_some(),
                has_window: found.is_some_and(|f| f.info.has_editor),
                retired: crate::dsp::effects::RETIRED.contains(&info.id),
                info,
            }
        })
        .collect()
}

/// Effects whose response is a fixed curve, so a graph means something.
const GRAPHED: [&str; 8] = [
    "altaudio.parametric_eq",
    "altaudio.band_eq_10",
    "altaudio.band_eq_20",
    "altaudio.band_eq_30",
    "altaudio.hum_cut",
    "altaudio.notch",
    "altaudio.precision_filter",
    "altaudio.curve_eq",
];

/// The rate the graph is measured at.
const GRAPH_RATE: u32 = 48_000;

/// How `slot` changes each frequency: `points` (frequency, dB) pairs
/// from 20 Hz to 20 kHz, evenly spaced in pitch. Measured by sending a
/// click through the effect, so the graph is what the effect really
/// does. Empty for effects that are not a fixed curve.
pub fn response(slot: &EffectSlot, points: usize) -> Vec<[f64; 2]> {
    if !GRAPHED.contains(&slot.effect.as_str()) || points < 2 {
        return Vec::new();
    }
    let mut slot = slot.clone();
    slot.enabled = true;
    let Some(mut fx) = build(&slot, GRAPH_RATE) else {
        return Vec::new();
    };
    let n = 16_384;
    let lat = fx.latency();
    let mut buf = vec![0f32; (lat + n) * 2];
    buf[0] = 1.0;
    buf[1] = 1.0;
    fx.process(&mut buf);
    let ir: Vec<f32> = buf.as_chunks::<2>().0[lat..].iter().map(|f| f[0]).collect();
    // A plain DFT at just the frequencies wanted: few points, so cheap.
    (0..points)
        .map(|i| {
            let f = 20.0 * 1000f64.powf(i as f64 / (points - 1) as f64);
            let w = std::f64::consts::TAU * f / f64::from(GRAPH_RATE);
            let (mut re, mut im) = (0.0f64, 0.0f64);
            for (t, v) in ir.iter().enumerate() {
                let a = w * t as f64;
                re += f64::from(*v) * a.cos();
                im -= f64::from(*v) * a.sin();
            }
            [
                f,
                (20.0 * (re * re + im * im).sqrt().max(1e-6).log10()).clamp(-120.0, 40.0),
            ]
        })
        .collect()
}

#[tauri::command]
pub async fn effect_response(slot: EffectSlot, points: usize) -> Result<Vec<[f64; 2]>, String> {
    spawn_blocking(move || response(&slot, points.clamp(2, 400)))
        .await
        .map_err(|e| e.to_string())
}

/// The Shaped Noise Reducer's graph: its learned noise and its threshold.
#[derive(Debug, Clone, Serialize)]
pub struct NoiseCurves {
    pub print: Vec<[f64; 2]>,
    pub threshold: Vec<[f64; 2]>,
}

/// The learned noise and the threshold of a Shaped Noise Reducer, in dB
/// at `points` frequencies, on the scale of a `scope`-point spectrum view.
#[tauri::command]
pub fn shaped_noise_curves(
    slot: EffectSlot,
    points: usize,
    scope: usize,
) -> Result<NoiseCurves, String> {
    use crate::dsp::effects::{info, shaped, Params};
    let i = info(shaped::SHAPED_NOISE).ok_or("Unknown effect.")?;
    let p = Params::resolve(&i, &slot.params);
    let (print, threshold) = shaped::curves(&p, 48_000, points.clamp(2, 400), scope);
    Ok(NoiseCurves { print, threshold })
}

/// The Vox Strip's Tone curve, in dB at `points` frequencies.
#[tauri::command]
pub fn vox_tone_curve(slot: EffectSlot, points: usize) -> Result<Vec<[f64; 2]>, String> {
    use crate::dsp::effects::{info, vocal, Params};
    let i = info(vocal::VOCAL).ok_or("Unknown effect.")?;
    let p = Params::resolve(&i, &slot.params);
    Ok(vocal::tone_curve(&p, points.clamp(2, 400)))
}

/// An effect's own meter (gain reduction in dB, output in dBFS), for
/// windows that draw one. None until the effect has been built.
#[tauri::command]
pub fn effect_meter(slot: crate::session::model::Id) -> Option<[f32; 3]> {
    crate::dsp::meters::read(slot)
}

/// Where to learn a noise print from: a stretch of the Waveform
/// editor's file, or of one track or clip in the session.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum LearnSource {
    Wave { start: u64, end: u64 },
    Track { id: Id, start: u64, end: u64 },
    Clip { id: Id, start: u64, end: u64 },
}

#[derive(Debug, Clone, Serialize)]
pub struct LearnedPrint {
    /// The settings to put on the Noise Print Reducer.
    pub params: BTreeMap<String, f64>,
    pub seconds: f64,
}

/// Only what `keep` says (a track's clips or one clip), with no Voice
/// Chains and at unity level, so the print is of the raw audio.
pub(crate) fn isolate(session: &Session, track: Option<Id>, clip: Option<Id>) -> Session {
    let mut s = without_effects(session.clone());
    s.tracks.retain(|t| {
        track.is_none_or(|id| t.id == id)
            && clip.is_none_or(|id| t.clips.iter().any(|c| c.id == id))
    });
    for t in &mut s.tracks {
        t.muted = false;
        t.solo = false;
        t.volume_db = 0.0;
        if let Some(id) = clip {
            t.clips.retain(|c| c.id == id);
        }
    }
    s.master.volume_db = 0.0;
    s
}

pub(crate) fn render(session: &Session, start: u64, end: u64) -> Result<Vec<f32>, String> {
    let req = MixdownRequest {
        range: Some((start, end)),
        clip_only: true,
        ..Default::default()
    };
    let mut audio = Vec::with_capacity(((end - start) * 2) as usize);
    mixdown::render_mix(
        session,
        &req,
        &AtomicBool::new(false),
        |_, _| {},
        |block| {
            audio.extend_from_slice(block);
            Ok(())
        },
    )
    .map_err(|e| e.to_string())?;
    Ok(audio)
}

/// Learns a noise print from `source`. The stretch should hold only the
/// noise to remove: a pause, the room before anyone speaks.
#[tauri::command]
pub async fn learn_noise_print(
    open: State<'_, OpenSession>,
    wave: State<'_, OpenWave>,
    source: LearnSource,
) -> Result<LearnedPrint, String> {
    let (session, start, end) = match source {
        LearnSource::Wave { start, end } => {
            let w = wave.0.lock().map_err(|e| e.to_string())?;
            let s = w
                .as_ref()
                .ok_or("No file is open in the Waveform editor.")?
                .session
                .clone();
            (isolate(&s, None, None), start, end)
        }
        LearnSource::Track { id, start, end } => (
            isolate(
                &open.0.lock().map_err(|e| e.to_string())?.session,
                Some(id),
                None,
            ),
            start,
            end,
        ),
        LearnSource::Clip { id, start, end } => (
            isolate(
                &open.0.lock().map_err(|e| e.to_string())?.session,
                None,
                Some(id),
            ),
            start,
            end,
        ),
    };
    if end <= start {
        return Err("Select a stretch with only the noise in it, then learn.".into());
    }
    spawn_blocking(move || {
        let rate = session.sample_rate;
        let audio = render(&session, start, end)?;
        let bands = learn(&audio, rate).ok_or("That selection is too short to learn from. Select at least half a second of the noise.")?;
        if bands.iter().all(|v| *v <= crate::dsp::effects::print::SILENT_DB + 1.0) {
            return Err("That selection is silent, so there is no noise to learn.".into());
        }
        let seconds = (end - start) as f64 / f64::from(rate);
        Ok(LearnedPrint {
            params: print_params(&bands, (seconds * 100.0).round() / 100.0).into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
            seconds,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::model::{ChannelLayout, Track};

    #[test]
    fn graphs_an_eq_and_skips_others() {
        let slot = |effect: &str, params: &[(&str, f64)]| EffectSlot {
            id: crate::session::model::new_id(),
            effect: effect.into(),
            enabled: false,
            params: params.iter().map(|(k, v)| ((*k).to_string(), *v)).collect(),
            preset: None,
            sidechain: None,
            plugin_state: None,
            data: None,
            position: None,
            routing: None,
        };
        let flat = response(&slot("altaudio.curve_eq", &[]), 32);
        assert_eq!(flat.len(), 32);
        assert!(flat.iter().all(|p| p[1].abs() < 0.5), "{flat:?}");
        let notch = response(&slot("altaudio.notch", &[]), 200);
        let deepest = notch.iter().fold(0f64, |m, p| m.min(p[1]));
        assert!(deepest < -10.0, "{deepest}");
        assert!(response(&slot("altaudio.limiter", &[]), 32).is_empty());
    }

    #[test]
    fn isolate_keeps_one_track_or_clip_and_drops_chains() {
        let mut s = Session::new("t", 48_000);
        s.tracks.push(Track::new("A", ChannelLayout::Stereo));
        s.tracks.push(Track::new("B", ChannelLayout::Stereo));
        s.tracks[0].effects.input_gain_db = 6.0;
        s.tracks[0].volume_db = -12.0;
        let a = s.tracks[0].id;
        let only = isolate(&s, Some(a), None);
        assert_eq!(only.tracks.len(), 1);
        assert_eq!(only.tracks[0].effects, Default::default());
        assert_eq!(only.tracks[0].volume_db, 0.0);
    }
}

/// An effect's input and output channels, by speaker name.
#[derive(Serialize)]
pub struct EffectChannels {
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
}

/// The channels a slot's effect hears and makes: L and R for built-in
/// effects; for a plug-in, its main input and output (its own layout when
/// the slot asks for it, which starts the plug-in if it is not running).
#[tauri::command]
pub async fn effect_channels(slot: EffectSlot) -> EffectChannels {
    let stereo = crate::dsp::effects::routing::stereo;
    if !crate::plugins::is_plugin(&slot.effect) {
        return EffectChannels {
            inputs: stereo(),
            outputs: stereo(),
        };
    }
    tauri::async_runtime::spawn_blocking(move || {
        let rate = crate::engine::output::device_rate();
        crate::plugins::instance::host_for(&slot, rate)
            .and_then(|h| {
                h.lock().ok().map(|h| EffectChannels {
                    inputs: h.inputs.clone(),
                    outputs: h.outputs.clone(),
                })
            })
            .unwrap_or(EffectChannels {
                inputs: stereo(),
                outputs: stereo(),
            })
    })
    .await
    .unwrap_or(EffectChannels {
        inputs: stereo(),
        outputs: stereo(),
    })
}

/// One preset brought in by [`rack_presets_import`].
#[derive(Debug, Serialize)]
pub struct ImportedRack {
    pub name: String,
    pub chain: crate::session::model::EffectChain,
}

/// What [`rack_presets_import`] brought in.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RackImport {
    pub presets: Vec<ImportedRack>,
    /// Effects of the other editor brought in as ours.
    pub converted: usize,
    /// Plug-ins not installed (or not scanned) here.
    pub missing_plugins: Vec<String>,
    /// Effects that could not come along.
    pub skipped: Vec<String>,
}

/// Audio FX > Presets > Import: asks for effects rack preset files (XML)
/// and reads their racks, converting the other editor's built-in effects
/// to ours as a session import does. None on cancel.
#[tauri::command]
pub async fn rack_presets_import(app: tauri::AppHandle) -> Result<Option<RackImport>, String> {
    use tauri::Manager;
    use tauri_plugin_dialog::DialogExt;
    let parent = app.get_webview_window("main");
    spawn_blocking(move || {
        let mut d = app
            .dialog()
            .file()
            .set_title("Import Effects Rack Presets")
            .add_filter("Effects rack presets", &["xml"]);
        if let Some(w) = parent.as_ref() {
            d = d.set_parent(w);
        }
        let Some(files) = d.blocking_pick_files() else {
            return Ok(None);
        };
        let mut report = crate::sesx::ImportReport::default();
        let mut presets = Vec::new();
        for f in files {
            let Some(path) = f.as_path().map(std::path::Path::to_path_buf) else {
                continue;
            };
            let text = std::fs::read_to_string(&path)
                .map_err(|e| format!("Could not read {}: {e}", path.display()))?;
            for p in crate::sesx::read_rack_presets(&text, &mut report)? {
                presets.push(ImportedRack {
                    name: p.name,
                    chain: p.chain,
                });
            }
        }
        Ok(Some(RackImport {
            presets,
            converted: report.converted,
            missing_plugins: report.missing_plugins,
            skipped: report.skipped_names,
        }))
    })
    .await
    .map_err(|e| e.to_string())?
}
