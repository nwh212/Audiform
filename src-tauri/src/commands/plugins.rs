// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

//! VST3 plug-ins: the Plug-in Manager (folders, scan, what was found) and
//! running plug-ins' windows, events and settings.

use std::collections::HashSet;
use std::path::PathBuf;

use serde::Serialize;
use tauri::async_runtime::spawn_blocking;

use crate::plugins::{self, instance, Registry};
use crate::session::model::{EffectChain, EffectSlot, Id, Session};

/// Ids of every slot in the session that holds a plug-in.
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

/// Adds or removes one of the user's folders, or turns a standard one off or on.
#[tauri::command]
pub fn plugins_set_folder(path: PathBuf, on: bool) -> RegistryView {
    view(&plugins::set_folder(&path, on))
}

/// Opens a plug-in's own window (starting the plug-in if it is not
/// running yet), or brings it to the front.
#[tauri::command]
pub async fn plugin_open_window(slot: EffectSlot, title: String) -> Result<(), String> {
    spawn_blocking(move || {
        let rate = crate::engine::output::device_rate();
        let host = instance::host_for(&slot, rate)
            .ok_or("The plug-in is not installed or would not start.")?;
        let mut h = host.lock().map_err(|e| e.to_string())?;
        h.open_window(&title)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn plugin_close_window(slot_id: Id) {
    if let Some(h) = instance::running(slot_id) {
        if let Ok(mut h) = h.lock() {
            h.close_window();
        }
    }
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
