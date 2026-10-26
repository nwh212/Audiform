// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

// Hide the extra console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Started again as a plug-in host or scanner: see the plughost crate.
    let args: Vec<String> = std::env::args().collect();
    if let Some(code) = altered_plughost::main_with_args(&args) {
        std::process::exit(code);
    }
    audiform_lib::run()
}
