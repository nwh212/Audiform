// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Scaffolding (P5): host voice profiles, saved as `.alhost`.
//!
//! A host profile describes how one person's voice is processed: the
//! breathing-removal gate settings and the Voice Chain preset to load
//! on their track. Built-ins: Close Mic, Standard Mic, Remote Guest. Stored
//! and round-tripped now. Processing uses them in P5.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::document::Document;
use super::model::TrackRole;

/// Breathing-removal gate settings for one voice.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GateSettings {
    /// Open threshold in dB, for example -14.0.
    pub threshold_db: f64,
    pub attack_ms: f64,
    pub hold_ms: f64,
    pub release_ms: f64,
}

/// Voice processing settings for one host or guest type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HostProfile {
    /// Always `altaudio.host_profile`.
    #[schemars(extend("const" = "altaudio.host_profile"))]
    pub format: String,
    pub schema_version: u32,

    /// Stable short id, for example `host` or `guest`.
    pub id: String,
    /// Display name, for example `Host`.
    pub name: String,
    /// Who this profile is for. None for generic profiles like Guest.
    pub person: Option<String>,
    /// Track role this profile is meant for.
    pub role: TrackRole,
    pub gate: GateSettings,
    /// Name of the Voice Chain preset loaded on the track.
    pub voice_chain_preset: Option<String>,
    pub notes: String,
}

impl Document for HostProfile {
    const FORMAT: &'static str = "altaudio.host_profile";
    const CURRENT_VERSION: u32 = 1;
    const SCHEMA_FILE: &'static str = "host-profile.schema.json";
}
