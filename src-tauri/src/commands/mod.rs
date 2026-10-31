// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Tauri command handlers. Keep these thin: parse input, call into the
//! core modules, return plain data.

pub mod align;
pub mod amp;
pub mod app;
pub mod batch;
pub mod browse;
pub mod cache;
pub mod details;
pub mod devices;
pub mod effects;
pub mod export;
pub mod firewall;
pub mod generate;
pub mod keymap;
pub mod library;
pub mod linked;
pub mod links;
pub mod live;
pub mod media;
pub mod musicfit;
pub mod peaks;
pub mod plugins;
pub mod preferences;
pub mod prerender;
pub mod record;
pub mod reverb;
pub mod separation;
pub mod session;
pub mod spectral;
pub mod template;
pub mod themes;
pub mod transport;
pub mod wave;
pub mod wave_fx;
pub mod workspace;
