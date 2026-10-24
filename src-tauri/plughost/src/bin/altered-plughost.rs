// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

//! A stand-alone host, for the tests here. The editor runs the same code
//! from its own executable (`--plugin-host`, `--plugin-scan`).

fn main() {
    let args: Vec<String> = std::env::args().collect();
    std::process::exit(altered_plughost::main_with_args(&args).unwrap_or(2));
}
