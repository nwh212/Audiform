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

/// Picks the output by its system name; None goes back to the default.
pub fn set_choice(name: Option<String>) {
    if let Ok(mut c) = CHOICE.lock() {
        *c = name.filter(|n| !n.trim().is_empty());
    }
}

pub fn choice() -> Option<String> {
    CHOICE.lock().ok().and_then(|c| c.clone())
}

/// The output to open now.
pub fn output_device(host: &cpal::Host) -> Option<cpal::Device> {
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
        .map_or(48_000, |c| c.config().sample_rate)
}

#[cfg(test)]
mod tests {
    use super::*;

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
