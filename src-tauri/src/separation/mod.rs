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
    /// None when the author or a public benchmark gives no score.
    pub sdr: Option<f32>,
    /// What the score was measured on.
    pub sdr_note: &'static str,
    /// Names of the two results.
    pub target_name: &'static str,
    pub rest_name: &'static str,
    /// Models that give several sources at once (6 stems): each one's name
    /// and published score. Empty for a model with one target and the rest.
    pub stems: &'static [(&'static str, f32)],
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
            category: "Vocals",
            sdr: Some(11.00),
            sdr_note: "vocals, Multisong test set",
            target_name: "Vocals",
            rest_name: "Instrumental",
            stems: &[],
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
            category: "Vocals",
            sdr: Some(10.98),
            sdr_note: "vocals, Multisong test set",
            target_name: "Vocals",
            rest_name: "Instrumental",
            stems: &[],
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
        ModelEntry {
            id: "bs-roformer-sw",
            name: "BS-RoFormer SW (6 stems)",
            category: "Instruments",
            sdr: Some(11.30),
            sdr_note: "vocals, MVSep Multisong set; bass 14.62, drums 14.11, guitar 9.05, piano 7.83, other 8.71",
            target_name: "Vocals",
            rest_name: "Instrumental",
            // The model's outputs come in the order it was trained with.
            stems: &[
                ("Bass", 14.62),
                ("Drums", 14.11),
                ("Other", 8.71),
                ("Vocals", 11.30),
                ("Guitar", 9.05),
                ("Piano", 7.83),
            ],
            author: "Community (original trainer unknown), fixed for UVR and MSST by jarredou",
            page: "https://huggingface.co/enerjazzer/BS-ROFO-SW-Fixed",
            url: "https://huggingface.co/enerjazzer/BS-ROFO-SW-Fixed/resolve/main/BS-Rofo-SW-Fixed.ckpt",
            sha256: "24e7d35ee9c64415673d3fd33e06a67cac2c103c5df6267ba1576459c775916e",
            bytes: 699_412_152,
            licence: "No licence is stated for the weights (listed as unknown).",
            spec: || {
                json!({
                    "arch": "bs",
                    "freqs_per_bands": [2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,
                        4,4,4,4,4,4,4,4,4,4,4,4,12,12,12,12,12,12,12,12,24,24,24,24,24,24,24,24,
                        48,48,48,48,48,48,48,48,128,129],
                    "sample_rate": 44100, "n_fft": 2048, "hop": 512, "win": 2048,
                    "heads": 8, "dim_head": 64, "chunk_size": 588800, "num_overlap": 2
                })
            },
        },
        ModelEntry {
            id: "xlance-bass",
            name: "Bass (single instrument)",
            category: "Instruments",
            sdr: None,
            sdr_note: "no public score; fine-tuned by X-LANCE from the 6-stem model above, whose own bass score is the published one",
            target_name: "Bass",
            rest_name: "Without bass",
            stems: &[],
            author: "X-LANCE (MIT licence), as converted and re-uploaded by noblebarkrr",
            page: "https://huggingface.co/chenxie95/xlance-msr-ckpt",
            url: "https://huggingface.co/noblebarkrr/mvsepless_resources/resolve/main/bs_roformer/bs_bass_xlancer.ckpt",
            sha256: "c409465e3dccfd83fa34e6a807dbdae2f4f1db24f2f85a0fdc146b6fa828afff",
            bytes: 204_411_168,
            licence: "MIT licence, as stated on the X-LANCE page for the original files.",
            spec: || {
                json!({
                    "arch": "bs",
                    "freqs_per_bands": [2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,
                        4,4,4,4,4,4,4,4,4,4,4,4,12,12,12,12,12,12,12,12,24,24,24,24,24,24,24,24,
                        48,48,48,48,48,48,48,48,128,129],
                    "sample_rate": 44100, "n_fft": 2048, "hop": 512, "win": 2048,
                    "heads": 8, "dim_head": 64, "chunk_size": 588800, "num_overlap": 2
                })
            },
        },
        ModelEntry {
            id: "xlance-drums",
            name: "Drums (single instrument)",
            category: "Instruments",
            sdr: None,
            sdr_note: "no public score; fine-tuned by X-LANCE from the 6-stem model above, whose own drums score is the published one",
            target_name: "Drums",
            rest_name: "Without drums",
            stems: &[],
            author: "X-LANCE (MIT licence), as converted and re-uploaded by noblebarkrr",
            page: "https://huggingface.co/chenxie95/xlance-msr-ckpt",
            url: "https://huggingface.co/noblebarkrr/mvsepless_resources/resolve/main/bs_roformer/bs_drums2_xlancer.ckpt",
            sha256: "7d53b4d663212d748b1450ca9c9d97a13f8b95eeee1ac39cf986bc916f227314",
            bytes: 204_412_534,
            licence: "MIT licence, as stated on the X-LANCE page for the original files.",
            spec: || {
                json!({
                    "arch": "bs",
                    "freqs_per_bands": [2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,
                        4,4,4,4,4,4,4,4,4,4,4,4,12,12,12,12,12,12,12,12,24,24,24,24,24,24,24,24,
                        48,48,48,48,48,48,48,48,128,129],
                    "sample_rate": 44100, "n_fft": 2048, "hop": 512, "win": 2048,
                    "heads": 8, "dim_head": 64, "chunk_size": 588800, "num_overlap": 2
                })
            },
        },
        ModelEntry {
            id: "xlance-guitar",
            name: "Guitar (single instrument)",
            category: "Instruments",
            sdr: None,
            sdr_note: "no public score; fine-tuned by X-LANCE from the 6-stem model above, whose own guitar score is the published one",
            target_name: "Guitar",
            rest_name: "Without guitar",
            stems: &[],
            author: "X-LANCE (MIT licence), as converted and re-uploaded by noblebarkrr",
            page: "https://huggingface.co/chenxie95/xlance-msr-ckpt",
            url: "https://huggingface.co/noblebarkrr/mvsepless_resources/resolve/main/bs_roformer/bs_gtr_xlancer.ckpt",
            sha256: "813a92bd210e1fc7412d658c777e3676b4991e33499437ea2f830d7b58af2684",
            bytes: 204_409_973,
            licence: "MIT licence, as stated on the X-LANCE page for the original files.",
            spec: || {
                json!({
                    "arch": "bs",
                    "freqs_per_bands": [2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,
                        4,4,4,4,4,4,4,4,4,4,4,4,12,12,12,12,12,12,12,12,24,24,24,24,24,24,24,24,
                        48,48,48,48,48,48,48,48,128,129],
                    "sample_rate": 44100, "n_fft": 2048, "hop": 512, "win": 2048,
                    "heads": 8, "dim_head": 64, "chunk_size": 588800, "num_overlap": 2
                })
            },
        },
        ModelEntry {
            id: "xlance-keys",
            name: "Piano and keys (single instrument)",
            category: "Instruments",
            sdr: None,
            sdr_note: "no public score; fine-tuned by X-LANCE from the 6-stem model above, whose own piano and keys score is the published one",
            target_name: "Piano and keys",
            rest_name: "Without keys",
            stems: &[],
            author: "X-LANCE (MIT licence), as converted and re-uploaded by noblebarkrr",
            page: "https://huggingface.co/chenxie95/xlance-msr-ckpt",
            url: "https://huggingface.co/noblebarkrr/mvsepless_resources/resolve/main/bs_roformer/bs_keys_xlancer.ckpt",
            sha256: "09ec2bcde2f2cbc0001ef444c4854562bdebd8cf62e34d09311da8fc594090a3",
            bytes: 204_409_973,
            licence: "MIT licence, as stated on the X-LANCE page for the original files.",
            spec: || {
                json!({
                    "arch": "bs",
                    "freqs_per_bands": [2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,
                        4,4,4,4,4,4,4,4,4,4,4,4,12,12,12,12,12,12,12,12,24,24,24,24,24,24,24,24,
                        48,48,48,48,48,48,48,48,128,129],
                    "sample_rate": 44100, "n_fft": 2048, "hop": 512, "win": 2048,
                    "heads": 8, "dim_head": 64, "chunk_size": 588800, "num_overlap": 2
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

/// The engine build for this computer, in the engine release. None where
/// no build is published yet.
pub fn engine_asset() -> Option<&'static str> {
    if cfg!(all(windows, target_arch = "x86_64")) {
        Some("audiform-separate-windows-x64.zip")
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some("audiform-separate-linux-x64.zip")
    } else {
        None
    }
}

/// The engine release Audiform uses. Engines are released on their own
/// (as pre-releases, so they never show as Audiform's latest version);
/// change this when a new engine is needed.
pub const ENGINE_TAG: &str = "engine-v0.2.0";

pub fn engine_url(asset: &str) -> String {
    format!("https://github.com/nwh212/Audiform/releases/download/{ENGINE_TAG}/{asset}")
}

/// The job file the engine reads. `outputs` are the files to write, in
/// the order the model's results are named (a model with `stems` gives
/// one file per stem; any other gives the target and the rest).
/// `overlap` replaces the model's own (how many times each moment is
/// separated and blended: higher is smoother and slower).
pub fn job(
    m: &ModelEntry,
    checkpoint: &Path,
    input: &Path,
    outputs: &[PathBuf],
    gpu: bool,
    overlap: Option<u32>,
) -> Value {
    let mut spec = (m.spec)();
    if let Some(o) = overlap {
        spec["num_overlap"] = json!(o.clamp(1, 8));
    }
    let files: Vec<Value> = if m.stems.is_empty() {
        vec![
            json!({ "path": outputs[0], "stems": [0] }),
            json!({ "path": outputs[1], "stems": [0], "rest": true }),
        ]
    } else {
        outputs
            .iter()
            .enumerate()
            .map(|(k, p)| json!({ "path": p, "stems": [k] }))
            .collect()
    };
    json!({
        "spec": spec,
        "checkpoint": checkpoint,
        "input": input,
        "outputs": files,
        "device": if gpu { "gpu" } else { "cpu" },
    })
}

impl ModelEntry {
    /// The names of the results, in order.
    pub fn output_names(&self) -> Vec<&'static str> {
        if self.stems.is_empty() {
            vec![self.target_name, self.rest_name]
        } else {
            self.stems.iter().map(|(n, _)| *n).collect()
        }
    }
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
            assert!(m.sdr.is_none_or(|v| v > 0.0) && m.bytes > 0);
            assert!((m.spec)()["arch"].is_string());
        }
    }

    #[test]
    fn a_six_stem_model_writes_one_file_per_stem() {
        let m = model("bs-roformer-sw").unwrap();
        assert_eq!(m.output_names().len(), 6);
        let outs: Vec<PathBuf> = (0..6).map(|k| PathBuf::from(format!("o{k}.wav"))).collect();
        let j = job(
            &m,
            Path::new("m.ckpt"),
            Path::new("in.wav"),
            &outs,
            false,
            None,
        );
        let files = j["outputs"].as_array().unwrap();
        assert_eq!(files.len(), 6);
        assert_eq!(files[3]["stems"], json!([3]));
        // The model writes bass, drums, other, vocals, guitar, piano.
        assert_eq!(
            m.output_names(),
            ["Bass", "Drums", "Other", "Vocals", "Guitar", "Piano"]
        );
        assert!(files[3].get("rest").is_none());
        // A two-result model gives its target and the rest.
        let v = model("bs-polarformer").unwrap();
        let j = job(
            &v,
            Path::new("m.ckpt"),
            Path::new("in.wav"),
            &outs[..2],
            false,
            None,
        );
        assert_eq!(j["outputs"][1]["rest"], json!(true));
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
