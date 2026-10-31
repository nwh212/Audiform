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

/// An ASIO driver opens for one program at a time, so right after the
/// app starts (an update restarts it while the old copy is still letting
/// go), or after a plug-in that crashed, a lookup can come back empty even
/// though the drivers are installed. In that one case ask again a few
/// times before giving up. The system driver is never retried.
fn retry_if_empty<T>(asio: Option<bool>, mut look: impl FnMut() -> Vec<T>) -> Vec<T> {
    let mut found = look();
    let want = asio.unwrap_or_else(crate::engine::host::asio) && crate::engine::host::ASIO_BUILT;
    if !found.is_empty() || !want || crate::engine::host::asio_drivers().is_empty() {
        return found;
    }
    for _ in 0..4 {
        std::thread::sleep(std::time::Duration::from_millis(400));
        found = look();
        if !found.is_empty() {
            break;
        }
    }
    found
}

/// ASIO: the one driver in use is the only device, named after itself.
fn asio_wanted(asio: Option<bool>) -> bool {
    asio.unwrap_or_else(crate::engine::host::asio) && crate::engine::host::ASIO_BUILT
}

fn asio_inputs() -> Vec<AudioInput> {
    let Some(d) = crate::engine::host::asio_device() else {
        return Vec::new();
    };
    let Some(name) = crate::engine::host::asio_driver() else {
        return Vec::new();
    };
    let channels = d.default_input_config().map(|c| c.channels()).unwrap_or(0);
    if channels == 0 {
        return Vec::new();
    }
    vec![AudioInput {
        name,
        channels,
        is_default: true,
    }]
}

fn asio_outputs() -> Vec<AudioOutput> {
    let Some(d) = crate::engine::host::asio_device() else {
        return Vec::new();
    };
    let Some(name) = crate::engine::host::asio_driver() else {
        return Vec::new();
    };
    if d.default_output_config().is_err() {
        return Vec::new();
    }
    vec![AudioOutput {
        name,
        is_default: true,
    }]
}

fn list(asio: Option<bool>) -> Vec<AudioInput> {
    if asio_wanted(asio) {
        // Only the driver in use is asked; the others installed stay unloaded.
        let was = crate::engine::host::asio();
        crate::engine::host::set_asio(true);
        let out = asio_inputs();
        crate::engine::host::set_asio(was);
        return out;
    }
    let host = crate::engine::host::host_for(false);
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
    if asio_wanted(asio) {
        let was = crate::engine::host::asio();
        crate::engine::host::set_asio(true);
        let out = asio_outputs();
        crate::engine::host::set_asio(was);
        return out;
    }
    let host = crate::engine::host::host_for(false);
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
    spawn_blocking(move || retry_if_empty(asio, || outputs(asio)))
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
    /// The ASIO driver in use (picked, or the app's guess).
    pub driver: Option<String>,
    /// Frames per ASIO buffer; 0 is the driver's own.
    pub buffer: u32,
    /// Buffer sizes offered.
    pub buffers: Vec<u32>,
    /// Where the audio log is, for reporting problems.
    pub log: Option<String>,
}

/// Which driver is in use, and the ASIO drivers installed. Loads no
/// driver: the names come from the registry.
#[tauri::command]
pub async fn audio_driver() -> Result<AudioDriver, String> {
    spawn_blocking(|| AudioDriver {
        asio_built: crate::engine::host::ASIO_BUILT,
        asio: crate::engine::host::asio(),
        drivers: crate::engine::host::asio_drivers(),
        driver: crate::engine::host::asio_driver(),
        buffer: crate::engine::host::asio_buffer(),
        buffers: crate::engine::host::ASIO_BUFFERS.to_vec(),
        log: crate::engine::host::log_path().map(|p| p.display().to_string()),
    })
    .await
    .map_err(|e| e.to_string())
}

/// Uses ASIO (true) or the system driver (false) from the next time
/// playback, recording or a preview starts; with ASIO, which driver
/// (None lets the app pick) and the buffer size in frames (0 keeps the
/// driver's own).
#[tauri::command]
pub fn set_audio_driver(asio: bool, driver: Option<String>, buffer: Option<u32>) {
    crate::engine::host::set_asio_driver(driver.clone(), buffer.unwrap_or(0));
    crate::engine::host::set_asio(asio);
    crate::engine::host::log(&format!(
        "driver: {}{}",
        if asio && crate::engine::host::ASIO_BUILT {
            "ASIO"
        } else {
            "system"
        },
        if asio {
            format!(
                ", \"{}\", buffer {}",
                crate::engine::host::asio_driver().unwrap_or_else(|| "none".into()),
                buffer.unwrap_or(0)
            )
        } else {
            String::new()
        }
    ));
}

/// Writes one line from the interface to the audio log: an error that
/// came with no detail of its own, with the steps that led to it.
#[tauri::command]
pub fn log_line(line: String) {
    let line: String = line.chars().take(2000).collect();
    crate::engine::host::log(&line);
}

/// The audio inputs available now. Empty when there are none.
#[tauri::command]
pub async fn audio_inputs(asio: Option<bool>) -> Result<Vec<AudioInput>, String> {
    spawn_blocking(move || retry_if_empty(asio, || list(asio)))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_system_driver_is_looked_up_once() {
        let mut calls = 0;
        let found: Vec<u8> = retry_if_empty(Some(false), || {
            calls += 1;
            Vec::new()
        });
        assert!(found.is_empty());
        assert_eq!(calls, 1);
    }

    #[test]
    fn a_list_that_has_items_is_not_asked_for_again() {
        let mut calls = 0;
        let found = retry_if_empty(Some(true), || {
            calls += 1;
            vec![1u8]
        });
        assert_eq!(found, vec![1]);
        assert_eq!(calls, 1);
    }
}
