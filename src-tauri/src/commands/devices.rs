// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! The computer's audio inputs, for each track's input picker, plus on
//! Windows each output as a 'System audio' input. Only names and
//! channel counts are read; nothing is opened or recorded.

use cpal::traits::{DeviceTrait, HostTrait};
use serde::Serialize;

use crate::engine::record::SYSTEM_AUDIO_PREFIX;
use tauri::async_runtime::spawn_blocking;

#[derive(Debug, Clone, Serialize)]
pub struct AudioInput {
    /// Device name as the system reports it.
    pub name: String,
    /// Input channels on the device.
    pub channels: u16,
    pub is_default: bool,
}

fn list(asio: Option<bool>) -> Vec<AudioInput> {
    let host = crate::engine::host::host_for(asio.unwrap_or_else(crate::engine::host::asio));
    let default = host
        .default_input_device()
        .and_then(|d| d.description().ok())
        .map(|d| d.name().to_string());
    let Ok(devices) = host.input_devices() else {
        return Vec::new();
    };
    let mut out: Vec<AudioInput> = devices
        .filter_map(|d| {
            let name = d.description().ok()?.name().to_string();
            let channels = d.default_input_config().map(|c| c.channels()).unwrap_or(0);
            Some(AudioInput {
                is_default: default.as_deref() == Some(name.as_str()),
                name,
                channels,
            })
        })
        .filter(|d| d.channels > 0)
        .collect();
    out.sort_by_key(|d| (!d.is_default, d.name.to_lowercase()));
    // Windows can record what an output plays, such as a call app.
    if cfg!(windows) {
        if let Ok(outputs) = host.output_devices() {
            let mut sys: Vec<AudioInput> = outputs
                .filter_map(|d| {
                    let name = d.description().ok()?.name().to_string();
                    let channels = d.default_output_config().ok()?.channels();
                    Some(AudioInput {
                        name: format!("{SYSTEM_AUDIO_PREFIX}{name}"),
                        channels,
                        is_default: false,
                    })
                })
                .collect();
            sys.sort_by_key(|d| d.name.to_lowercase());
            out.extend(sys);
        }
    }
    out
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioOutput {
    pub name: String,
    pub is_default: bool,
}

fn outputs(asio: Option<bool>) -> Vec<AudioOutput> {
    let host = crate::engine::host::host_for(asio.unwrap_or_else(crate::engine::host::asio));
    let default = host
        .default_output_device()
        .and_then(|d| d.description().ok())
        .map(|d| d.name().to_string());
    let Ok(devices) = host.output_devices() else {
        return Vec::new();
    };
    let mut out: Vec<AudioOutput> = devices
        .filter_map(|d| {
            let name = d.description().ok()?.name().to_string();
            d.default_output_config().ok()?;
            Some(AudioOutput {
                is_default: default.as_deref() == Some(name.as_str()),
                name,
            })
        })
        .collect();
    out.sort_by_key(|d| (!d.is_default, d.name.to_lowercase()));
    out.dedup_by(|a, b| a.name == b.name);
    out
}

/// The speakers and headphones available now, the system default first.
#[tauri::command]
pub async fn audio_outputs(asio: Option<bool>) -> Result<Vec<AudioOutput>, String> {
    spawn_blocking(move || outputs(asio))
        .await
        .map_err(|e| e.to_string())
}

/// Plays through `name` from the next time playback starts; None (or a
/// name that is not connected) uses the system default.
#[tauri::command]
pub fn set_audio_output(name: Option<String>) {
    crate::engine::output::set_choice(name);
}

/// Which outputs playback uses (from 0): the stereo mix's left and right,
/// and each speaker by name for 3D Space sessions heard on speakers.
#[tauri::command]
pub fn set_output_map(stereo: [u16; 2], speakers: std::collections::HashMap<String, u16>) {
    crate::engine::output::set_map(crate::engine::output::OutputMap { stereo, speakers });
}

/// How many outputs the playback device has.
#[tauri::command]
pub async fn output_channels() -> Result<u16, String> {
    spawn_blocking(crate::engine::output::output_channels)
        .await
        .map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioDriver {
    /// This build can use ASIO.
    pub asio_built: bool,
    /// ASIO is in use now.
    pub asio: bool,
    /// Installed ASIO drivers.
    pub drivers: Vec<String>,
}

/// Which driver is in use, and the ASIO drivers installed.
#[tauri::command]
pub async fn audio_driver() -> Result<AudioDriver, String> {
    spawn_blocking(|| AudioDriver {
        asio_built: crate::engine::host::ASIO_BUILT,
        asio: crate::engine::host::asio(),
        drivers: crate::engine::host::asio_drivers(),
    })
    .await
    .map_err(|e| e.to_string())
}

/// Uses ASIO (true) or the system driver (false) from the next time
/// playback, recording or a preview starts.
#[tauri::command]
pub fn set_audio_driver(asio: bool) {
    crate::engine::host::set_asio(asio);
}

/// The audio inputs available now. Empty when there are none.
#[tauri::command]
pub async fn audio_inputs(asio: Option<bool>) -> Result<Vec<AudioInput>, String> {
    spawn_blocking(move || list(asio))
        .await
        .map_err(|e| e.to_string())
}

/// Opens the system's own sound settings, where inputs are set up.
/// Local only: it starts the system settings app, nothing on the web.
#[tauri::command]
pub fn open_sound_settings() -> Result<(), String> {
    use std::process::Command;
    #[cfg(target_os = "windows")]
    let tries: &[(&str, &[&str])] = &[
        ("explorer", &["ms-settings:sound"]),
        ("control", &["mmsys.cpl"]),
    ];
    #[cfg(target_os = "macos")]
    let tries: &[(&str, &[&str])] = &[(
        "open",
        &["x-apple.systempreferences:com.apple.preference.sound"],
    )];
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let tries: &[(&str, &[&str])] = &[
        ("pavucontrol", &[]),
        ("gnome-control-center", &["sound"]),
        ("systemsettings", &["kcm_pulseaudio"]),
    ];
    for (cmd, args) in tries {
        if Command::new(cmd).args(*args).spawn().is_ok() {
            return Ok(());
        }
    }
    Err("Could not open the system's sound settings.".into())
}
