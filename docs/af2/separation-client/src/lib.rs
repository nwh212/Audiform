// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) The Audiform contributors.

//! Everything Audiform 2 needs to use the separation engine, which is a
//! separate, privately licensed program downloaded the first time someone
//! uses Audio Separation.
//!
//! 1. **Download.** Fetch the small manifest (`MANIFEST_FILE`) from the
//!    engine release, read it with [`Manifest::parse`], and pick this
//!    computer's download with [`Manifest::asset`]. Fetching is left to
//!    Audiform, so all its network access stays in one place.
//! 2. **Install.** [`install::verify`] checks the download against the
//!    manifest's SHA-256; [`install::unpack`] puts the engine in place.
//! 3. **Run.** [`Engine::start`] starts it once (it shakes hands on
//!    protocol 2 and refuses anything else); [`Engine::load`] loads a
//!    model, which stays loaded for every [`Engine::run`] after it until
//!    another model is loaded. Dropping the [`Engine`] stops it, and it
//!    stops by itself if Audiform goes away.
//!
//! The engine reads one WAV at the model's sample rate (Audiform decodes
//! and resamples) and writes 32-bit float WAVs, one per [`Output`]: the
//! sum of some of the model's stems, or the mix with them taken out.

pub mod engine;
pub mod install;
pub mod manifest;
pub mod protocol;

pub use engine::{Engine, Loaded, RunEnd};
pub use manifest::{Asset, Manifest, MANIFEST_FILE};
pub use protocol::{Output, PROTOCOL};

/// Something went wrong; `Gpu` means the graphics card could not be used,
/// so Audiform can offer the processor instead.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Engine(String),
    #[error("The graphics card could not be used: {0}")]
    Gpu(String),
    #[error("The separation engine stopped: {0}")]
    Stopped(String),
    #[error("{0}")]
    Install(String),
}
