// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! AAC in an M4A file, through the encoder built into the operating
//! system (the `altered-os-aac` crate beside this one). Nothing is
//! bundled, so there is no codec licence to carry.
//!
//! Windows: Media Foundation's AAC encoder and MP4 writer (every Windows
//! 10 and 11 install has them). macOS and Linux: not yet; on macOS the
//! system encoder (Audio Toolbox) is the planned route, and Linux has
//! no system AAC encoder, so it will need a bundled one.
//!
//! The Windows encoder takes 44.1 or 48 kHz, mono or stereo, at 96, 128,
//! 160 or 192 kbps. `resolve` picks values inside those limits.

/// Bit rates the system encoder accepts, kbps.
pub const AAC_BITRATES: [u16; 4] = [96, 128, 160, 192];
/// Rates the system encoder accepts.
pub const AAC_RATES: [u32; 2] = [44_100, 48_000];

pub use altered_os_aac::{AacWriter, AVAILABLE, UNAVAILABLE};

/// The nearest bit rate the encoder accepts.
pub fn nearest_bitrate(kbps: u16) -> u16 {
    *AAC_BITRATES
        .iter()
        .min_by_key(|b| (i32::from(**b) - i32::from(kbps)).unsigned_abs())
        .unwrap_or(&128)
}

/// The rate to write: 48 kHz stays, everything else goes to 44.1 kHz
/// unless it is above 44.1.
pub fn pick_rate(rate: u32) -> u32 {
    if rate > 44_100 {
        48_000
    } else {
        44_100
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_values_the_encoder_takes() {
        assert_eq!(nearest_bitrate(192), 192);
        assert_eq!(nearest_bitrate(256), 192);
        assert_eq!(nearest_bitrate(64), 96);
        assert_eq!(nearest_bitrate(140), 128);
        assert_eq!(pick_rate(48_000), 48_000);
        assert_eq!(pick_rate(96_000), 48_000);
        assert_eq!(pick_rate(22_050), 44_100);
    }
}
