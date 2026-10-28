// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

//! Audio Separation: the list of models, where downloads live, and
//! running the separation engine.
//!
//! The engine is a separate, closed program, kept outside this repository
//! and downloaded the first time someone uses Audio Separation, with the
//! model they pick, so the editor itself carries none of it. Audiform only
//! talks to it through a job file and its output lines (`job`,
//! `parse_line`). Separation runs entirely on
//! the user's own computer. Nothing is downloaded until the user asks for
//! it; see PRIVACY.md and `download.rs`.

pub mod download;

use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{json, Value};

/// One model the user can download.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelEntry {
    pub id: &'static str,
    pub name: &'static str,
    /// The group it is listed under, such as "Vocals and instrumental".
    pub category: &'static str,
    /// Quality score of the separated target (signal-to-distortion ratio in
    /// dB, higher is better), as published for the model.
    pub sdr: f32,
    /// What the score was measured on.
    pub sdr_note: &'static str,
    /// Names of the two results.
    pub target_name: &'static str,
    pub rest_name: &'static str,
    pub author: &'static str,
    /// Where the model is described by its author.
    pub page: &'static str,
    /// The author's own download of the original file.
    #[serde(skip)]
    pub url: &'static str,
    #[serde(skip)]
    pub sha256: &'static str,
    pub bytes: u64,
    /// What the author says about use of the weights.
    pub licence: &'static str,
    /// What the engine needs to know beyond the file itself.
    #[serde(skip)]
    pub spec: fn() -> Value,
}

/// The best public models, checked again at every major release (see
/// docs/releasing.md). Only vocal and instrumental models for now.
pub fn catalog() -> Vec<ModelEntry> {
    vec![
        ModelEntry {
            id: "bs-polarformer",
            name: "BS PolarFormer",
            category: "Vocals and instrumental",
            sdr: 11.00,
            sdr_note: "vocals, Multisong test set",
            target_name: "Vocals",
            rest_name: "Instrumental",
            author: "ZFTurbo",
            page: "https://github.com/ZFTurbo/Music-Source-Separation-Training/releases/tag/v1.0.20",
            url: "https://github.com/ZFTurbo/Music-Source-Separation-Training/releases/download/v1.0.20/model_bs_polarformer_float16.ckpt",
            sha256: "fc8b72c3beb92caad4e14f180979c02ddf18a330182176cf6c7bb0eb6c685e87",
            bytes: 102_410_301,
            licence: "Published freely by the author; no licence is stated for the weights.",
            spec: || {
                json!({
                    "arch": "bs",
                    "freqs_per_bands": [2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,
                        4,4,4,4,4,4,4,4,4,4,4,4,12,12,12,12,12,12,12,12,24,24,24,24,24,24,24,24,
                        48,48,48,48,48,48,48,48,128,129],
                    "sample_rate": 44100, "n_fft": 2048, "hop": 512, "win": 2048,
                    "heads": 8, "dim_head": 64, "chunk_size": 882000, "num_overlap": 2
                })
            },
        },
        ModelEntry {
            id: "mel-roformer-kim",
            name: "Mel-Band Roformer (Kim)",
            category: "Vocals and instrumental",
            sdr: 10.98,
            sdr_note: "vocals, Multisong test set",
            target_name: "Vocals",
            rest_name: "Instrumental",
            author: "Kimberley Jensen",
            page: "https://huggingface.co/KimberleyJSN/melbandroformer",
            url: "https://huggingface.co/KimberleyJSN/melbandroformer/resolve/main/MelBandRoformer.ckpt",
            sha256: "87201f4d31afb5bc79993230fc49446918425574db48c01c405e44f365c7559e",
            bytes: 913_106_900,
            licence: "MIT licence, as stated on the author's page.",
            spec: || {
                json!({
                    "arch": "mel", "num_bands": 60,
                    "sample_rate": 44100, "n_fft": 2048, "hop": 441, "win": 2048,
                    "heads": 8, "dim_head": 64, "chunk_size": 352800, "num_overlap": 2
                })
            },
        },
    ]
}

pub fn model(id: &str) -> Option<ModelEntry> {
    catalog().into_iter().find(|m| m.id == id)
}

/// Where downloads go, under the app's own data folder.
pub fn root(data_dir: &Path) -> PathBuf {
    data_dir.join("separation")
}

pub fn model_path(data_dir: &Path, m: &ModelEntry) -> PathBuf {
    root(data_dir).join("models").join(format!("{}.ckpt", m.id))
}

const ENGINE_FILE: &str = if cfg!(windows) {
    "audiform-separate.exe"
} else {
    "audiform-separate"
};

/// The engine, if it is installed. `AUDIFORM_SEPARATE` points at a local
/// build instead, for development.
pub fn engine_path(data_dir: &Path) -> PathBuf {
    match std::env::var_os("AUDIFORM_SEPARATE") {
        Some(p) => PathBuf::from(p),
        None => root(data_dir).join("engine").join(ENGINE_FILE),
    }
}

/// The engine build for this computer, attached to each Audiform release
/// next to the installers. None where no build is published yet.
pub fn engine_asset() -> Option<&'static str> {
    if cfg!(all(windows, target_arch = "x86_64")) {
        Some("audiform-separate-windows-x64.zip")
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some("audiform-separate-linux-x64.zip")
    } else {
        None
    }
}

pub fn engine_url(asset: &str) -> String {
    format!(
        "https://github.com/nwh212/Audiform/releases/download/v{}/{asset}",
        env!("CARGO_PKG_VERSION")
    )
}

/// The job file the engine reads.
/// `overlap` replaces the model's own (how many times each moment is
/// separated and blended: higher is smoother and slower).
pub fn job(
    m: &ModelEntry,
    checkpoint: &Path,
    input: &Path,
    target: &Path,
    rest: &Path,
    gpu: bool,
    overlap: Option<u32>,
) -> Value {
    let mut spec = (m.spec)();
    if let Some(o) = overlap {
        spec["num_overlap"] = json!(o.clamp(1, 8));
    }
    json!({
        "spec": spec,
        "checkpoint": checkpoint,
        "input": input,
        "target": target,
        "rest": rest,
        "device": if gpu { "gpu" } else { "cpu" },
    })
}

/// One line of engine output.
#[derive(Debug, PartialEq)]
pub enum EngineLine {
    Progress(f32),
    Done,
    /// The GPU could not start (true) or something else went wrong.
    Error {
        gpu: bool,
        message: String,
    },
    Other,
}

pub fn parse_line(line: &str) -> EngineLine {
    let line = line.trim();
    if let Some(p) = line.strip_prefix("progress ") {
        return p
            .parse()
            .map(EngineLine::Progress)
            .unwrap_or(EngineLine::Other);
    }
    if line == "done" {
        return EngineLine::Done;
    }
    if let Some(rest) = line.strip_prefix("error ") {
        return match rest.strip_prefix("gpu ") {
            Some(m) => EngineLine::Error {
                gpu: true,
                message: m.to_string(),
            },
            None => EngineLine::Error {
                gpu: false,
                message: rest.to_string(),
            },
        };
    }
    EngineLine::Other
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_model_has_a_pinned_download() {
        for m in catalog() {
            assert!(m.url.starts_with("https://"), "{}", m.id);
            assert_eq!(m.sha256.len(), 64, "{}", m.id);
            assert!(
                download::allowed(m.url),
                "{} is not on an allowed host",
                m.id
            );
            assert!(m.sdr > 0.0 && m.bytes > 0);
            assert!((m.spec)()["arch"].is_string());
        }
    }

    #[test]
    fn engine_lines_parse() {
        assert_eq!(parse_line("progress 0.25\n"), EngineLine::Progress(0.25));
        assert_eq!(parse_line("done"), EngineLine::Done);
        assert_eq!(
            parse_line("error gpu no adapter"),
            EngineLine::Error {
                gpu: true,
                message: "no adapter".into()
            }
        );
        assert_eq!(
            parse_line("error bad file"),
            EngineLine::Error {
                gpu: false,
                message: "bad file".into()
            }
        );
        assert_eq!(parse_line("hello"), EngineLine::Other);
    }
}
