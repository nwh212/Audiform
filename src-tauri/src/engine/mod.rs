// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Audio output: file preview, and multitrack playback (P2).
//!
//! Real-time rule: the device callback never allocates, locks, or
//! touches the disk. Worker threads prepare audio and hand it over
//! through lock-free ring buffers; settings arrive as atomics.

pub mod automation;
#[cfg(test)]
pub mod bench;
pub(crate) mod fx;
pub mod graph;
pub mod host;
pub mod metronome;
pub mod output;
pub mod plan;
pub mod preview;
pub mod record;
pub mod render;
pub mod reverse;
pub mod scope;
pub mod speaker_feed;
pub mod transport;

pub use preview::{PreviewPlayer, PreviewStatus};
pub use record::{RecordStatus, RecordTarget, Recorder, Take};
pub use transport::{Jump, Transport, TransportStatus};
