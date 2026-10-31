// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Which output plays the audio: the one picked in Settings > Audio
//! Hardware, or the system default when none is picked or the picked
//! one is not connected. Playback, file preview and input monitoring
//! all ask here.

use std::sync::Mutex;

use cpal::traits::{DeviceTrait, HostTrait};

static CHOICE: Mutex<Option<String>> = Mutex::new(None);
static MAP: Mutex<Option<OutputMap>> = Mutex::new(None);

/// Which device outputs (from 0) the mix plays on: the stereo mix's left
/// and right, and in 3D Space sessions heard on speakers, each speaker
/// by name. A speaker not named goes to the output at its place in the
/// layout (L on 1, R on 2, C on 3...).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputMap {
    pub stereo: [u16; 2],
    pub speakers: std::collections::HashMap<String, u16>,
}

impl Default for OutputMap {
    fn default() -> Self {
        Self {
            stereo: [0, 1],
            speakers: std::collections::HashMap::new(),
        }
    }
}

impl OutputMap {
    /// The output for speaker `name`, the `index`th of its layout.
    pub fn speaker(&self, name: &str, index: usize) -> usize {
        self.speakers.get(name).map_or(index, |&o| usize::from(o))
    }
}

pub fn set_map(map: OutputMap) {
    if let Ok(mut m) = MAP.lock() {
        *m = Some(map);
    }
}

pub fn map() -> OutputMap {
    MAP.lock().ok().and_then(|m| m.clone()).unwrap_or_default()
}

/// The device's settings for playback with at least `channels` outputs:
/// its default when that has enough, otherwise its widest setting at the
/// default rate (and format, when it offers one).
pub fn config_for(device: &cpal::Device, channels: usize) -> Option<cpal::SupportedStreamConfig> {
    let def = super::host::sane_config(device.default_output_config().ok()?);
    if usize::from(def.channels()) >= channels {
        return Some(def);
    }
    let rate = def.sample_rate();
    let wide = device
        .supported_output_configs()
        .ok()?
        .filter(|r| r.min_sample_rate() <= rate && rate <= r.max_sample_rate())
        .max_by_key(|r| (r.channels(), r.sample_format() == def.sample_format()))?;
    if wide.channels() <= def.channels() {
        return Some(def);
    }
    Some(super::host::sane_config(wide.with_sample_rate(rate)))
}

/// Outputs on the chosen device in its widest setting (0 with no device).
pub fn output_channels() -> u16 {
    output_device_any()
        .map(|d| {
            let def = d.default_output_config().map_or(0, |c| c.channels());
            d.supported_output_configs()
                .map(|all| all.map(|r| r.channels()).max().unwrap_or(0))
                .unwrap_or(0)
                .max(def)
        })
        .unwrap_or(0)
}

/// Picks the output by its system name; None goes back to the default.
pub fn set_choice(name: Option<String>) {
    if let Ok(mut c) = CHOICE.lock() {
        *c = name.filter(|n| !n.trim().is_empty());
    }
}

pub fn choice() -> Option<String> {
    CHOICE.lock().ok().and_then(|c| c.clone())
}

/// The output to open now. On ASIO it is the one driver in use, whatever
/// output name is picked (names are the system driver's).
pub fn output_device(host: &cpal::Host) -> Option<cpal::Device> {
    if super::host::asio() {
        return match host.id() {
            #[cfg(all(target_os = "windows", feature = "asio"))]
            cpal::HostId::Asio => super::host::asio_device(),
            _ => default_or_named(host),
        };
    }
    default_or_named(host)
}

fn default_or_named(host: &cpal::Host) -> Option<cpal::Device> {
    if let Some(name) = choice() {
        if let Ok(mut all) = host.output_devices() {
            if let Some(d) = all.find(|d| d.description().ok().is_some_and(|x| x.name() == name)) {
                return Some(d);
            }
        }
    }
    host.default_output_device()
}

/// The output to open now: on the chosen driver, or on the system driver
/// when that has none. ASIO drivers open for one program at a time, so
/// after a recording holds the driver, playback goes to the system output
/// instead of failing with "No audio output device found".
pub fn output_device_any() -> Option<cpal::Device> {
    let host = crate::engine::host::host();
    output_device(&host).or_else(|| output_device(&cpal::default_host()))
}

/// The rate playback runs at: the output device's own. 48 kHz when there
/// is no device.
pub fn device_rate() -> u32 {
    output_device_any()
        .and_then(|d| d.default_output_config().ok())
        .map_or(48_000, |c| super::host::sane_rate(c.config().sample_rate))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unnamed_speakers_keep_their_place() {
        let mut m = OutputMap::default();
        m.speakers.insert("C".into(), 7);
        assert_eq!(m.speaker("C", 2), 7);
        assert_eq!(m.speaker("Ls", 4), 4);
    }

    #[test]
    fn blank_names_mean_the_default() {
        set_choice(Some("  ".into()));
        assert_eq!(choice(), None);
        set_choice(Some("Speakers".into()));
        assert_eq!(choice().as_deref(), Some("Speakers"));
        set_choice(None);
        assert_eq!(choice(), None);
    }
}
