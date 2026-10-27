// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Reading audio files: identify and measure them (`probe`), and stream
//! decoded samples at any rate and channel count (`AudioReader`).

pub mod cue;
mod dts;
mod opus;
mod probe;
mod reader;
mod scan;
mod source;
mod unzip;

pub use probe::{probe, AudioInfo, ProbeError};
pub use reader::{map_channels, AudioReader};
pub use scan::{expand_paths, is_supported};
pub use unzip::extract_audio;

/// File extensions the Files panel accepts, lowercase, without the dot.
pub const SUPPORTED_EXTENSIONS: &[&str] = &[
    "wav", "wave", "aif", "aiff", "aifc", "mp3", "flac", "m4a", "mp4", "aac", "ogg", "oga", "opus",
    "mogg", "dts",
];
