// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! VST3 plug-ins: the Plug-in Manager (folders, scan, what was found) and
//! running plug-ins' windows, events and settings.

use std::collections::HashSet;
use std::path::PathBuf;

use serde::Serialize;
use tauri::async_runtime::spawn_blocking;

use crate::plugins::{self, instance, Registry};
use crate::session::model::{EffectChain, EffectSlot, Id, Session};

/// Ids of every slot in the session that holds a plug-in.
/// Each plug-in slot's saved settings, for noticing which ones an edit changed.
pub fn plugin_states(s: &Session) -> std::collections::HashMap<Id, Option<String>> {
    let mut out = std::collections::HashMap::new();
    let mut add = |slot: &crate::session::model::EffectSlot| {
        if plugins::is_plugin(&slot.effect) {
            out.insert(slot.id, slot.plugin_state.clone());
        }
    };
    for t in &s.tracks {
        t.effects.slots.iter().for_each(&mut add);
        for c in &t.clips {
            c.effects.slots.iter().for_each(&mut add);
        }
        if let Some(i) = &t.instrument {
            add(i);
        }
    }
    for b in &s.buses {
        b.effects.slots.iter().for_each(&mut add);
    }
    s.master.effects.slots.iter().for_each(&mut add);
    out
}

pub fn plugin_slots(s: &Session) -> HashSet<Id> {
    let mut out = HashSet::new();
    let mut add = |c: &EffectChain| {
        for slot in &c.slots {
            if plugins::is_plugin(&slot.effect) {
                out.insert(slot.id);
            }
        }
    };
    for t in &s.tracks {
        if t.clips.is_empty() && t.instrument.is_none() {
            continue;
        }
        add(&t.effects);
        for c in &t.clips {
            add(&c.effects);
        }
    }
    for b in &s.buses {
        add(&b.effects);
    }
    add(&s.master.effects);
    out.extend(
        s.tracks
            .iter()
            .filter_map(|t| t.instrument.as_ref().map(|i| i.id)),
    );
    out
}

/// Every plug-in slot in the session that plays: enabled chain slots on
/// tracks, clips, buses and the master, and MIDI tracks' instruments.
/// Tracks with no clips and no instrument hear nothing, so their
/// plug-ins are left to load when something is put on them. Pass the
/// session as it is heard, so pre-rendered tracks (which play a render
/// instead of their effects) don't load plug-ins they won't use.
pub fn playing_plugin_slots(s: &Session) -> Vec<EffectSlot> {
    let mut out = Vec::new();
    let mut add = |c: &EffectChain| {
        if c.enabled {
            out.extend(
                c.slots
                    .iter()
                    .filter(|x| x.enabled && plugins::is_plugin(&x.effect))
                    .cloned(),
            );
        }
    };
    for t in &s.tracks {
        add(&t.effects);
        for c in &t.clips {
            add(&c.effects);
        }
    }
    for b in &s.buses {
        add(&b.effects);
    }
    add(&s.master.effects);
    out.extend(s.tracks.iter().filter_map(|t| t.instrument.clone()));
    out
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderView {
    pub path: PathBuf,
    /// One of the folders plug-in installers use.
    pub standard: bool,
    /// Searched (a standard folder can be turned off).
    pub on: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginView {
    /// The effect id to put in a Voice Chain slot.
    pub effect: String,
    pub name: String,
    pub vendor: String,
    pub version: String,
    pub kind: String,
    pub bundle: PathBuf,
    pub latency: u32,
    pub has_window: bool,
    /// Plays notes on a MIDI track rather than sitting in a Voice Chain.
    pub instrument: bool,
    /// Why it was only partly read, if it was.
    pub note: Option<String>,
    pub controls: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FailedView {
    pub bundle: PathBuf,
    pub error: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryView {
    pub folders: Vec<FolderView>,
    pub plugins: Vec<PluginView>,
    pub failed: Vec<FailedView>,
    /// Makers whose plug-ins run together in one process (lower case).
    pub shared: Vec<String>,
}

fn view(reg: &Registry) -> RegistryView {
    let mut folders: Vec<FolderView> = plugins::standard_folders()
        .into_iter()
        .map(|path| FolderView {
            on: !reg.skipped.contains(&path),
            path,
            standard: true,
        })
        .collect();
    for f in &reg.folders {
        if !folders.iter().any(|x| &x.path == f) {
            folders.push(FolderView {
                path: f.clone(),
                standard: false,
                on: true,
            });
        }
    }
    RegistryView {
        shared: instance::shared_maker_list(),
        folders,
        plugins: reg
            .plugins
            .iter()
            .map(|p| PluginView {
                effect: format!("{}{}", plugins::PREFIX, p.info.cid),
                name: p.info.name.clone(),
                vendor: p.info.vendor.clone(),
                version: p.info.version.clone(),
                kind: p.info.sub_categories.clone(),
                bundle: p.bundle.clone(),
                latency: p.info.latency,
                has_window: p.info.has_editor,
                instrument: p.info.instrument,
                note: p.info.note.clone(),
                controls: p.info.params.iter().filter(|x| !x.hidden).count(),
            })
            .collect(),
        failed: reg
            .failed
            .iter()
            .map(|f| FailedView {
                bundle: f.bundle.clone(),
                error: f.error.clone(),
            })
            .collect(),
    }
}

/// The Plug-in Manager's list: folders, plug-ins found, and failures.
#[tauri::command]
pub fn plugins_registry() -> RegistryView {
    view(&plugins::snapshot())
}

/// Sets the makers whose plug-ins run together in one process.
#[tauri::command]
pub fn plugins_set_shared(makers: Vec<String>) -> RegistryView {
    view(&plugins::set_shared_makers(makers))
}

/// Scans the folders. Each bundle is read by a host process of its own,
/// so a bundle that crashes is listed as failed. `all` rescans unchanged
/// bundles too.
#[tauri::command]
pub async fn plugins_scan(
    app: tauri::AppHandle,
    all: bool,
) -> Result<(plugins::ScanReport, RegistryView), String> {
    use tauri::Emitter;
    spawn_blocking(move || {
        let report = plugins::scan(all, |done, total, bundle| {
            let name = bundle
                .file_stem()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let _ = app.emit(
                "plugins-scan-progress",
                serde_json::json!({ "done": done, "total": total, "name": name }),
            );
        });
        (report, view(&plugins::snapshot()))
    })
    .await
    .map_err(|e| e.to_string())
}

/// Stops the scan in progress, keeping what it read so far.
#[tauri::command]
pub fn plugins_scan_stop() {
    plugins::stop_scan();
}

/// Adds or removes one of the user's folders, or turns a standard one off or on.
#[tauri::command]
pub fn plugins_set_folder(path: PathBuf, on: bool) -> RegistryView {
    view(&plugins::set_folder(&path, on))
}

/// Opens a plug-in's own window (starting the plug-in if it is not
/// running yet), or brings it to the front.
#[tauri::command]
pub async fn plugin_open_window(
    app: tauri::AppHandle,
    slot: EffectSlot,
    title: String,
) -> Result<(), String> {
    let owner = main_window_handle(&app);
    spawn_blocking(move || {
        let rate = crate::engine::output::device_rate();
        let host = instance::host_for(&slot, rate)
            .ok_or("The plug-in is not installed or would not start.")?;
        let mut h = host.lock().map_err(|e| e.to_string())?;
        h.open_window(&title, owner, !slot.enabled)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The main window's handle on Windows, for plug-in windows to belong to.
#[cfg(windows)]
fn main_window_handle(app: &tauri::AppHandle) -> u64 {
    use tauri::Manager;
    app.get_webview_window("main")
        .and_then(|w| w.hwnd().ok())
        .map(|h| h.0 as usize as u64)
        .unwrap_or(0)
}

#[cfg(not(windows))]
fn main_window_handle(_app: &tauri::AppHandle) -> u64 {
    0
}

#[tauri::command]
pub fn plugin_close_window(slot_id: Id) {
    if let Some(h) = instance::running(slot_id) {
        if let Ok(mut h) = h.lock() {
            h.close_window();
        }
    }
}

/// The effect was bypassed or turned on: its window's power button follows.
#[tauri::command]
pub async fn plugin_show_bypassed(slot_id: Id, bypassed: bool) {
    let _ = spawn_blocking(move || {
        if let Some(h) = instance::running(slot_id) {
            if let Ok(mut h) = h.lock() {
                h.show_bypassed(bypassed);
            }
        }
    })
    .await;
}

/// Hides every plug-in window while the close prompt shows, or shows them again.
#[tauri::command]
pub async fn plugin_hide_windows(hidden: bool) {
    let _ = spawn_blocking(move || instance::hide_windows(hidden)).await;
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SlotEvents {
    pub slot_id: Id,
    #[serde(flatten)]
    pub events: instance::Events,
}

/// What running plug-ins reported since the last call: controls moved in
/// their windows, settings to save, closed windows, crashes.
#[tauri::command]
pub fn plugin_events() -> Vec<SlotEvents> {
    instance::take_events()
        .into_iter()
        .map(|(slot_id, events)| SlotEvents { slot_id, events })
        .collect()
}

/// A running plug-in's settings, to save in its slot. None when it is not running.
#[tauri::command]
pub async fn plugin_state(slot_id: Id) -> Result<Option<String>, String> {
    spawn_blocking(move || {
        let h = instance::running(slot_id)?;
        let mut h = h.lock().ok()?;
        h.state()
    })
    .await
    .map_err(|e| e.to_string())
}

/// Starts a stopped (crashed) plug-in again on the next playback.
#[tauri::command]
pub fn plugin_restart(slot_id: Id) {
    instance::restart(slot_id);
}

/// Plays one note through a MIDI track's instrument, so notes can be
/// heard while they are drawn and moved in the piano roll. Quietly does
/// nothing while the session plays (the instrument is busy then) or when
/// the track has no instrument. True when a note played.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn midi_audition(
    open: tauri::State<'_, super::session::OpenSession>,
    transport: tauri::State<'_, crate::engine::Transport>,
    preview: tauri::State<'_, crate::engine::PreviewPlayer>,
    track_id: Id,
    pitch: u8,
    velocity: f32,
    channel: Option<u8>,
    seconds: f64,
) -> Result<bool, String> {
    if transport.is_playing() {
        return Ok(false);
    }
    let slot = {
        let s = open.0.lock().map_err(|e| e.to_string())?;
        s.session
            .tracks
            .iter()
            .find(|t| t.id == track_id)
            .and_then(|t| t.instrument.clone())
    };
    let Some(slot) = slot.filter(|s| s.enabled) else {
        return Ok(false);
    };
    let make = spawn_blocking(move || {
        instance::audition(&slot, pitch, velocity, channel.unwrap_or(0), seconds)
    })
    .await
    .map_err(|e| e.to_string())??;
    preview.play_made(make)?;
    Ok(true)
}

/// Plays a MIDI clip's notes alone through its track's instrument (the
/// piano roll's Play). Times are seconds from the start. Quietly does
/// nothing while the session plays or when the track has no instrument.
/// True when it started.
#[tauri::command]
pub async fn midi_play_notes(
    open: tauri::State<'_, super::session::OpenSession>,
    transport: tauri::State<'_, crate::engine::Transport>,
    preview: tauri::State<'_, crate::engine::PreviewPlayer>,
    track_id: Id,
    notes: Vec<instance::SeqNote>,
) -> Result<bool, String> {
    if transport.is_playing() || notes.is_empty() {
        return Ok(false);
    }
    let slot = {
        let s = open.0.lock().map_err(|e| e.to_string())?;
        s.session
            .tracks
            .iter()
            .find(|t| t.id == track_id)
            .and_then(|t| t.instrument.clone())
    };
    let Some(slot) = slot.filter(|s| s.enabled) else {
        return Ok(false);
    };
    let make = spawn_blocking(move || instance::sequence(&slot, notes))
        .await
        .map_err(|e| e.to_string())??;
    preview.play_made(make)?;
    Ok(true)
}

/// Presets for a plug-in (`effect` is its `vst3:` id) in the standard
/// VST3 preset folders, shared with every other VST3 host.
#[tauri::command]
pub async fn plugin_presets(effect: String) -> Vec<crate::plugins::presets::PresetFile> {
    let Some(cid) = effect.strip_prefix(plugins::PREFIX).map(str::to_string) else {
        return Vec::new();
    };
    spawn_blocking(move || crate::plugins::presets::list(&cid))
        .await
        .unwrap_or_default()
}

/// A preset file's settings, to put in a slot as its `plugin_state`.
#[tauri::command]
pub async fn plugin_preset_read(path: PathBuf) -> Result<String, String> {
    spawn_blocking(move || crate::plugins::presets::read(&path))
        .await
        .map_err(|e| e.to_string())?
}

/// Saves a plug-in slot's settings now (from the running plug-in, or
/// as saved in the session) as a preset named `name`. Returns its path.
#[tauri::command]
pub async fn plugin_preset_save(slot: EffectSlot, name: String) -> Result<PathBuf, String> {
    spawn_blocking(move || {
        let found = plugins::find(&slot.effect).ok_or("The plug-in is not installed.")?;
        let state = instance::running(slot.id)
            .and_then(|h| h.lock().ok().and_then(|mut h| h.state()))
            .or(slot.plugin_state.clone())
            .ok_or("The plug-in has no settings to save yet. Open it once, then save.")?;
        crate::plugins::presets::save(
            &found.info.cid,
            &found.info.vendor,
            &found.info.name,
            &name,
            &state,
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Deletes a preset file from the user's own preset folder.
#[tauri::command]
pub fn plugin_preset_delete(path: PathBuf) -> Result<(), String> {
    crate::plugins::presets::delete(&path)
}
