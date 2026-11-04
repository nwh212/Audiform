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
    /// The folded heading the model is listed under. Empty: by what it splits.
    pub group: &'static str,
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
            group: "",
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
            group: "",
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
            id: "mel-roformer-deux",
            name: "Mel-Band Roformer Deux (vocals and instrumental)",
            category: "Vocals",
            group: "",
            sdr: Some(11.37),
            sdr_note: "vocals 11.37, instrumental 17.55, as given on the author's page",
            target_name: "Vocals",
            rest_name: "Instrumental",
            // Both come from the model itself, in the order it was trained with.
            stems: &[("Vocals", 11.37), ("Instrumental", 17.55)],
            author: "becruily",
            page: "https://huggingface.co/becruily/mel-band-roformer-deux",
            url: "https://huggingface.co/becruily/mel-band-roformer-deux/resolve/main/becruily_deux.ckpt",
            sha256: "10255c02295bf3e3865d4ee50ff752d7b19b124ed5fd93b147babc4333eda3aa",
            bytes: 435_006_815,
            licence: "CC BY-NC 4.0 (non-commercial use), as stated on the author's page.",
            spec: || {
                json!({
                    "arch": "mel", "num_bands": 60,
                    "sample_rate": 44100, "n_fft": 2048, "hop": 441, "win": 2048,
                    "heads": 8, "dim_head": 64, "chunk_size": 573300, "num_overlap": 2
                })
            },
        },
        ModelEntry {
            id: "mel-denoise",
            name: "Mel-Band Roformer Denoise",
            category: "Denoise",
            group: "",
            sdr: Some(27.9959),
            sdr_note: "as given in the file's name, on the author's own test",
            target_name: "Clean",
            rest_name: "Removed noise",
            stems: &[],
            author: "aufr33, as mirrored by poiqazwsx",
            page: "https://huggingface.co/poiqazwsx/melband-roformer-denoise",
            url: "https://huggingface.co/poiqazwsx/melband-roformer-denoise/resolve/main/denoise_mel_band_roformer_aufr33_sdr_27.9959.ckpt",
            sha256: "7c1c39191edc34e942ca7f2346ce6b6c0e1208a5f76349ffce6f696bd12910de",
            bytes: 913_097_300,
            licence: "No licence is stated for the weights; the mirror credits aufr33.",
            spec: || {
                json!({
                    "arch": "mel", "num_bands": 60,
                    "sample_rate": 44100, "n_fft": 2048, "hop": 441, "win": 2048,
                    "heads": 8, "dim_head": 64, "chunk_size": 352800, "num_overlap": 2
                })
            },
        },
        ModelEntry {
            id: "mel-denoise-aggressive",
            name: "Mel-Band Roformer Denoise (aggressive)",
            category: "Denoise",
            group: "",
            sdr: Some(27.9768),
            sdr_note: "as given in the file's name, on the author's own test; takes out more noise, and can take some of the voice with it",
            target_name: "Clean",
            rest_name: "Removed noise",
            stems: &[],
            author: "aufr33, as mirrored by poiqazwsx",
            page: "https://huggingface.co/poiqazwsx/melband-roformer-denoise",
            url: "https://huggingface.co/poiqazwsx/melband-roformer-denoise/resolve/main/denoise_mel_band_roformer_aufr33_aggr_sdr_27.9768.ckpt",
            sha256: "a25e3b233722cd81e2de7b8e798a3fef29d4b9799ccacda60b0dc958a1e2a5bb",
            bytes: 913_097_300,
            licence: "No licence is stated for the weights; the mirror credits aufr33.",
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
            group: "",
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
            group: "",
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
            group: "",
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
            group: "",
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
            id: "chencfd-guitar",
            name: "Guitar (trained for guitar)",
            category: "Instruments",
            group: "",
            sdr: Some(7.98),
            sdr_note: "guitar, on the trainer's own test set (a public multitrack set), as given in the file's name",
            target_name: "Guitar",
            rest_name: "Without guitar",
            stems: &[],
            author: "chenCFD",
            page: "https://github.com/chenCFD/Guitar_Source_Separation",
            url: "https://github.com/chenCFD/Guitar_Source_Separation/releases/download/v1.0/model_mel_band_roformer_ep_314_sdr_7.9825.ckpt",
            sha256: "1a81a87b46eed215c051579ac5cff3b139bfdfb9fa3bf4666df059ef87404d14",
            bytes: 276_726_667,
            licence: "MIT licence, as stated in the trainer's repository.",
            spec: || {
                json!({
                    "arch": "mel", "num_bands": 60,
                    "sample_rate": 44100, "n_fft": 2048, "hop": 512, "win": 2048,
                    "heads": 8, "dim_head": 64, "chunk_size": 131584, "num_overlap": 4
                })
            },
        },
        ModelEntry {
            id: "karaoke-frazer-becruily",
            name: "Lead vocal (apart from backing vocals)",
            category: "Vocals",
            group: "",
            sdr: Some(10.10),
            sdr_note: "lead vocal, on a public lead-and-backing test set; run it on a vocal stem to get the backing vocals as the rest",
            target_name: "Lead vocal",
            rest_name: "Backing vocals and instruments",
            stems: &[],
            author: "frazer and becruily",
            page: "https://huggingface.co/becruily/bs-roformer-karaoke",
            url: "https://github.com/nomadkaraoke/python-audio-separator/releases/download/model-configs/bs_roformer_karaoke_frazer_becruily.ckpt",
            sha256: "eb90ee24c1154d83fbcfd27e96182f19e061557cc6e4746953125e08c29389f9",
            bytes: 204_436_907,
            licence: "Published freely by the trainers; no licence is stated for the weights.",
            spec: || {
                json!({
                    "arch": "bs",
                    "freqs_per_bands": [2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,
                        4,4,4,4,4,4,4,4,4,4,4,4,12,12,12,12,12,12,12,12,24,24,24,24,24,24,24,24,
                        48,48,48,48,48,48,48,48,128,129],
                    "sample_rate": 44100, "n_fft": 2048, "hop": 512, "win": 2048,
                    "heads": 8, "dim_head": 64, "chunk_size": 882000, "num_overlap": 4
                })
            },
        },
        ModelEntry {
            id: "bleed-suppressor",
            name: "Bleed Suppressor (cleans an instrumental)",
            category: "Denoise",
            group: "",
            sdr: None,
            sdr_note: "no public score; takes out what other stems left behind in an instrumental",
            target_name: "Instrumental",
            rest_name: "Bleed",
            stems: &[],
            author: "unwa and 97chris",
            page: "https://github.com/nomadkaraoke/python-audio-separator",
            url: "https://github.com/nomadkaraoke/python-audio-separator/releases/download/model-configs/mel_band_roformer_bleed_suppressor_v1.ckpt",
            sha256: "a9a9d10faa7f8997676a78e66d741d7acb9cc449334763f3c8f626d68ec6e575",
            bytes: 913_102_724,
            licence: "Published freely by the trainers; no licence is stated for the weights.",
            spec: || {
                json!({
                    "arch": "mel", "num_bands": 60,
                    "sample_rate": 44100, "n_fft": 2048, "hop": 441, "win": 2048,
                    "heads": 8, "dim_head": 64, "chunk_size": 485100, "num_overlap": 2
                })
            },
        },
        ModelEntry {
            id: "gilliaan-strings",
            name: "Bowed strings (violins, violas, cellos, basses)",
            category: "Instruments",
            group: "",
            sdr: Some(4.69),
            sdr_note: "strings, on a public strings test set; the best public strings model",
            target_name: "Strings",
            rest_name: "Without strings",
            stems: &[("Strings", 4.69), ("Without strings", 0.0)],
            author: "gilliaan",
            page: "https://huggingface.co/gilliaan/Stem-Separation-Models",
            url: "https://huggingface.co/gilliaan/Stem-Separation-Models/resolve/main/BowedStrings/BandSplitRoformer/gilliaan_bsroformer_bowedstrings_v2.ckpt",
            sha256: "e49feede4125404930ea4dd2cd634b8a5850cc9e24dc0565a01ccac466a5bbff",
            bytes: 303_413_749,
            licence: "CC BY-NC-SA 4.0, as stated by the trainer: free for non-commercial use, with credit, shared alike.",
            spec: || {
                json!({
                    "arch": "bs",
                    "freqs_per_bands": [2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,
                        4,4,4,4,4,4,4,4,4,4,4,4,12,12,12,12,12,12,12,12,24,24,24,24,24,24,24,24,
                        48,48,48,48,48,48,48,48,128,129],
                    "sample_rate": 44100, "n_fft": 2048, "hop": 441, "win": 2048,
                    "heads": 8, "dim_head": 64, "chunk_size": 882000, "num_overlap": 4
                })
            },
        },
        ModelEntry {
            id: "karaoke-anvuew",
            name: "Lead vocal (apart from backing vocals, GPL)",
            category: "Vocals",
            group: "",
            sdr: Some(10.22),
            sdr_note: "lead vocal, on a public lead-and-backing test set; run it on a vocal stem to get the backing vocals as the rest",
            target_name: "Lead vocal",
            rest_name: "Backing vocals and instruments",
            stems: &[],
            author: "anvuew (trained on becruily's data)",
            page: "https://huggingface.co/anvuew/karaoke_bs_roformer",
            url: "https://huggingface.co/anvuew/karaoke_bs_roformer/resolve/main/karaoke_bs_roformer_anvuew.ckpt",
            sha256: "206d04757cb5f75ca3b55f8a0a48f5c26aa2351d4ff3c7adbfc9affa30ea3ae4",
            bytes: 204_486_925,
            licence: "GPL-3.0, as stated by the trainer.",
            spec: || {
                json!({
                    "arch": "bs",
                    "freqs_per_bands": [2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,
                        4,4,4,4,4,4,4,4,4,4,4,4,12,12,12,12,12,12,12,12,24,24,24,24,24,24,24,24,
                        48,48,48,48,48,48,48,48,128,129],
                    "sample_rate": 44100, "n_fft": 2048, "hop": 512, "win": 2048,
                    "heads": 8, "dim_head": 64, "chunk_size": 640000, "num_overlap": 4
                })
            },
        },
        ModelEntry {
            id: "bs-roformer-anvuew",
            name: "BS-RoFormer (anvuew, GPL)",
            category: "Vocals",
            group: "",
            sdr: Some(12.55),
            sdr_note: "vocals, on the trainer's own test set (not the Multisong set the others are scored on), as given in the file's name",
            target_name: "Vocals",
            rest_name: "Instrumental",
            stems: &[],
            author: "anvuew (trained on bascurtiz's data)",
            page: "https://huggingface.co/anvuew/BS-RoFormer",
            url: "https://huggingface.co/anvuew/BS-RoFormer/resolve/main/bs_roformer_ft1_anvuew_sdr_12.55.ckpt",
            sha256: "60347271e8493fdff28ef558c3b2297afb869a1f4462594ed548038264bec395",
            bytes: 204_493_312,
            licence: "GPL-3.0, as stated by the trainer.",
            spec: || {
                json!({
                    "arch": "bs",
                    "freqs_per_bands": [2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,
                        4,4,4,4,4,4,4,4,4,4,4,4,12,12,12,12,12,12,12,12,24,24,24,24,24,24,24,24,
                        48,48,48,48,48,48,48,48,128,129],
                    "sample_rate": 44100, "n_fft": 2048, "hop": 512, "win": 2048,
                    "heads": 8, "dim_head": 64, "chunk_size": 960000, "num_overlap": 4
                })
            },
        },
        ModelEntry {
            id: "dereverb-anvuew",
            name: "Reverb Remover (dry sound from a reverberant one)",
            category: "Denoise",
            group: "",
            sdr: Some(22.51),
            sdr_note: "dry sound, on the trainer's own test set, as given in the file's name",
            target_name: "Dry",
            rest_name: "Reverb",
            stems: &[],
            author: "anvuew",
            page: "https://huggingface.co/anvuew/dereverb_bs_roformer",
            url: "https://huggingface.co/anvuew/dereverb_bs_roformer/resolve/main/dereverb_bs_roformer_anvuew_sdr_22.5050.ckpt",
            sha256: "b4dfa4c4aa251c0afc303e5d512d904a1c2255e51d8d0dbfbddb18d740563d42",
            bytes: 204_496_717,
            licence: "GPL-3.0, as stated by the trainer.",
            spec: || {
                json!({
                    "arch": "bs",
                    "freqs_per_bands": [2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,
                        4,4,4,4,4,4,4,4,4,4,4,4,12,12,12,12,12,12,12,12,24,24,24,24,24,24,24,24,
                        48,48,48,48,48,48,48,48,128,129],
                    "sample_rate": 44100, "n_fft": 2048, "hop": 512, "win": 2048,
                    "heads": 8, "dim_head": 64, "chunk_size": 448000, "num_overlap": 2
                })
            },
        },
        ModelEntry {
            id: "xlance-keys",
            name: "Piano and keys (single instrument)",
            category: "Instruments",
            group: "",
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
        ModelEntry {
            id: "mega-53",
            name: "Mega 53 (every instrument at once)",
            category: "Instruments",
            group: "",
            sdr: None,
            sdr_note: "no single score; ZFTurbo's own notes call it an instrument finder more than a clean separator. Heavy: 53 results, wants a large graphics card",
            target_name: "Vocals",
            rest_name: "Rest",
            stems: &[
                ("Accordion", 0.0),
                ("Acoustic guitar", 0.0),
                ("Backing vocals", 0.0),
                ("Banjo", 0.0),
                ("Bass", 0.0),
                ("Bassoon", 0.0),
                ("Bells", 0.0),
                ("Bowed strings", 0.0),
                ("Brass", 0.0),
                ("Cello", 0.0),
                ("Clarinet", 0.0),
                ("Congas", 0.0),
                ("Digital piano", 0.0),
                ("Dobro", 0.0),
                ("Double bass", 0.0),
                ("Drum kit", 0.0),
                ("Electric guitar", 0.0),
                ("Flute", 0.0),
                ("French horn", 0.0),
                ("Glockenspiel", 0.0),
                ("Guitar", 0.0),
                ("Harmonica", 0.0),
                ("Harp", 0.0),
                ("Harpsichord", 0.0),
                ("Hi-hat", 0.0),
                ("Keys", 0.0),
                ("Kick", 0.0),
                ("Lead vocal", 0.0),
                ("Mandolin", 0.0),
                ("Marimba", 0.0),
                ("Oboe", 0.0),
                ("Organ", 0.0),
                ("Percussion", 0.0),
                ("Piano", 0.0),
                ("Saxophone", 0.0),
                ("Sitar", 0.0),
                ("Snare", 0.0),
                ("Strings (ensemble)", 0.0),
                ("Synth", 0.0),
                ("Tambourine", 0.0),
                ("Timpani", 0.0),
                ("Toms", 0.0),
                ("Triangle", 0.0),
                ("Trombone", 0.0),
                ("Trumpet", 0.0),
                ("Tuba", 0.0),
                ("Ukulele", 0.0),
                ("Viola", 0.0),
                ("Violin", 0.0),
                ("Vocals", 0.0),
                ("Winds", 0.0),
                ("Wind chimes", 0.0),
                ("Woodwinds", 0.0),
            ],
            author: "ZFTurbo (MVSep)",
            page: "https://github.com/ZFTurbo/Music-Source-Separation-Training/releases/tag/v1.0.21",
            url: "https://github.com/ZFTurbo/Music-Source-Separation-Training/releases/download/v1.0.21/mvsep_mega_model_bs_roformer_53_stems_v1.ckpt",
            sha256: "c62820893bbf86d4e734f966bd142d9157cfc8bb8e79e9d8f9ea553f3ff3519f",
            bytes: 1_368_919_887,
            licence: "Published by ZFTurbo with the MIT-licensed training code; no separate licence is stated for the weights.",
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
            id: "mega-53-accordion",
            name: "Accordion (Mega 53, on its own)",
            category: "Instruments",
            group: "Mega 53, one instrument at a time",
            sdr: None,
            sdr_note: "no public score; one instrument taken out of the Mega 53 model, a 78 MB download instead of 1.37 GB",
            target_name: "Accordion",
            rest_name: "Without accordion",
            stems: &[],
            author: "ZFTurbo (MVSep), split per instrument by noblebarkrr",
            page: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems",
            url: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems/resolve/main/v1/bs_mega_53stem_accordion_mvsep.ckpt",
            sha256: "aafcbe186aaec0489d28e388d9d1d3c12660c5640a8494bdaf3cbfee60c08c78",
            bytes: 77_619_808,
            licence: "Cut from ZFTurbo's Mega 53 model (MIT-licensed training code); the split itself states no licence.",
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
            id: "mega-53-acoustic-guitar",
            name: "Acoustic guitar (Mega 53, on its own)",
            category: "Instruments",
            group: "Mega 53, one instrument at a time",
            sdr: None,
            sdr_note: "no public score; one instrument taken out of the Mega 53 model, a 78 MB download instead of 1.37 GB",
            target_name: "Acoustic guitar",
            rest_name: "Without acoustic guitar",
            stems: &[],
            author: "ZFTurbo (MVSep), split per instrument by noblebarkrr",
            page: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems",
            url: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems/resolve/main/v1/bs_mega_53stem_acoustic-guitar_mvsep.ckpt",
            sha256: "fa386b2e7b1ea4f12b9b5c557444c0dc78648ef4ee299de2759d86457e182b3e",
            bytes: 77_624_038,
            licence: "Cut from ZFTurbo's Mega 53 model (MIT-licensed training code); the split itself states no licence.",
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
            id: "mega-53-back-vocal",
            name: "Backing vocals (Mega 53, on its own)",
            category: "Instruments",
            group: "Mega 53, one instrument at a time",
            sdr: None,
            sdr_note: "no public score; one instrument taken out of the Mega 53 model, a 78 MB download instead of 1.37 GB",
            target_name: "Backing vocals",
            rest_name: "Without backing vocals",
            stems: &[],
            author: "ZFTurbo (MVSep), split per instrument by noblebarkrr",
            page: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems",
            url: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems/resolve/main/v1/bs_mega_53stem_back-vocal_mvsep.ckpt",
            sha256: "9e60a9057340574977b0f858084113d952ba4731a7bbe054bf49d1d5338ca9d9",
            bytes: 77_620_513,
            licence: "Cut from ZFTurbo's Mega 53 model (MIT-licensed training code); the split itself states no licence.",
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
            id: "mega-53-banjo",
            name: "Banjo (Mega 53, on its own)",
            category: "Instruments",
            group: "Mega 53, one instrument at a time",
            sdr: None,
            sdr_note: "no public score; one instrument taken out of the Mega 53 model, a 78 MB download instead of 1.37 GB",
            target_name: "Banjo",
            rest_name: "Without banjo",
            stems: &[],
            author: "ZFTurbo (MVSep), split per instrument by noblebarkrr",
            page: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems",
            url: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems/resolve/main/v1/bs_mega_53stem_banjo_mvsep.ckpt",
            sha256: "7f9d0ad56db9289c767d41104156c2dce5b88631e05958bc78479b77e62e4f0d",
            bytes: 77_616_988,
            licence: "Cut from ZFTurbo's Mega 53 model (MIT-licensed training code); the split itself states no licence.",
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
            id: "mega-53-bass",
            name: "Bass (Mega 53, on its own)",
            category: "Instruments",
            group: "Mega 53, one instrument at a time",
            sdr: None,
            sdr_note: "no public score; one instrument taken out of the Mega 53 model, a 78 MB download instead of 1.37 GB",
            target_name: "Bass",
            rest_name: "Without bass",
            stems: &[],
            author: "ZFTurbo (MVSep), split per instrument by noblebarkrr",
            page: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems",
            url: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems/resolve/main/v1/bs_mega_53stem_bass_mvsep.ckpt",
            sha256: "b17600dd1c2027a72ab34180ff6191d3bdeb786db9fbb13c58337c33fc10163c",
            bytes: 77_616_219,
            licence: "Cut from ZFTurbo's Mega 53 model (MIT-licensed training code); the split itself states no licence.",
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
            id: "mega-53-bassoon",
            name: "Bassoon (Mega 53, on its own)",
            category: "Instruments",
            group: "Mega 53, one instrument at a time",
            sdr: None,
            sdr_note: "no public score; one instrument taken out of the Mega 53 model, a 78 MB download instead of 1.37 GB",
            target_name: "Bassoon",
            rest_name: "Without bassoon",
            stems: &[],
            author: "ZFTurbo (MVSep), split per instrument by noblebarkrr",
            page: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems",
            url: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems/resolve/main/v1/bs_mega_53stem_bassoon_mvsep.ckpt",
            sha256: "2d7329356a80027377e11d6d20c1c37a6b1c8a97e6fc502379142ba88773caae",
            bytes: 77_618_398,
            licence: "Cut from ZFTurbo's Mega 53 model (MIT-licensed training code); the split itself states no licence.",
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
            id: "mega-53-bells",
            name: "Bells (Mega 53, on its own)",
            category: "Instruments",
            group: "Mega 53, one instrument at a time",
            sdr: None,
            sdr_note: "no public score; one instrument taken out of the Mega 53 model, a 78 MB download instead of 1.37 GB",
            target_name: "Bells",
            rest_name: "Without bells",
            stems: &[],
            author: "ZFTurbo (MVSep), split per instrument by noblebarkrr",
            page: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems",
            url: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems/resolve/main/v1/bs_mega_53stem_bells_mvsep.ckpt",
            sha256: "6328f387259bb50ef12e36d3ef6cc5703520a0a89cd3bd743cf4f420f87701b8",
            bytes: 77_616_988,
            licence: "Cut from ZFTurbo's Mega 53 model (MIT-licensed training code); the split itself states no licence.",
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
            id: "mega-53-bowed-strings",
            name: "Bowed strings (Mega 53, on its own)",
            category: "Instruments",
            group: "Mega 53, one instrument at a time",
            sdr: None,
            sdr_note: "no public score; one instrument taken out of the Mega 53 model, a 78 MB download instead of 1.37 GB",
            target_name: "Bowed strings",
            rest_name: "Without bowed strings",
            stems: &[],
            author: "ZFTurbo (MVSep), split per instrument by noblebarkrr",
            page: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems",
            url: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems/resolve/main/v1/bs_mega_53stem_bowed_strings_mvsep.ckpt",
            sha256: "598a644d78dd6a385140b2134733eef42d9525805a84d55f224c80a1ca39ff61",
            bytes: 77_622_628,
            licence: "Cut from ZFTurbo's Mega 53 model (MIT-licensed training code); the split itself states no licence.",
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
            id: "mega-53-brass",
            name: "Brass (Mega 53, on its own)",
            category: "Instruments",
            group: "Mega 53, one instrument at a time",
            sdr: None,
            sdr_note: "no public score; one instrument taken out of the Mega 53 model, a 78 MB download instead of 1.37 GB",
            target_name: "Brass",
            rest_name: "Without brass",
            stems: &[],
            author: "ZFTurbo (MVSep), split per instrument by noblebarkrr",
            page: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems",
            url: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems/resolve/main/v1/bs_mega_53stem_brass_mvsep.ckpt",
            sha256: "e7d7bf86a031f3253019ff9dc424485d8085946fc428ab66522c568ee17b8027",
            bytes: 77_616_988,
            licence: "Cut from ZFTurbo's Mega 53 model (MIT-licensed training code); the split itself states no licence.",
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
            id: "mega-53-cello",
            name: "Cello (Mega 53, on its own)",
            category: "Instruments",
            group: "Mega 53, one instrument at a time",
            sdr: None,
            sdr_note: "no public score; one instrument taken out of the Mega 53 model, a 78 MB download instead of 1.37 GB",
            target_name: "Cello",
            rest_name: "Without cello",
            stems: &[],
            author: "ZFTurbo (MVSep), split per instrument by noblebarkrr",
            page: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems",
            url: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems/resolve/main/v1/bs_mega_53stem_cello_mvsep.ckpt",
            sha256: "bb6600ebc4dfe1d896d221073d114f51902a0b385afa23503034b9e8befba62c",
            bytes: 77_616_988,
            licence: "Cut from ZFTurbo's Mega 53 model (MIT-licensed training code); the split itself states no licence.",
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
            id: "mega-53-clarinet",
            name: "Clarinet (Mega 53, on its own)",
            category: "Instruments",
            group: "Mega 53, one instrument at a time",
            sdr: None,
            sdr_note: "no public score; one instrument taken out of the Mega 53 model, a 78 MB download instead of 1.37 GB",
            target_name: "Clarinet",
            rest_name: "Without clarinet",
            stems: &[],
            author: "ZFTurbo (MVSep), split per instrument by noblebarkrr",
            page: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems",
            url: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems/resolve/main/v1/bs_mega_53stem_clarinet_mvsep.ckpt",
            sha256: "3c4ac033c03727943a491e05dc9c9aebbd92afc3d8e83b3484881e9ed3f3dde7",
            bytes: 77_619_103,
            licence: "Cut from ZFTurbo's Mega 53 model (MIT-licensed training code); the split itself states no licence.",
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
            id: "mega-53-congas",
            name: "Congas (Mega 53, on its own)",
            category: "Instruments",
            group: "Mega 53, one instrument at a time",
            sdr: None,
            sdr_note: "no public score; one instrument taken out of the Mega 53 model, a 78 MB download instead of 1.37 GB",
            target_name: "Congas",
            rest_name: "Without congas",
            stems: &[],
            author: "ZFTurbo (MVSep), split per instrument by noblebarkrr",
            page: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems",
            url: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems/resolve/main/v1/bs_mega_53stem_congas_mvsep.ckpt",
            sha256: "78885092a3fdf0f63f349ba34e8fa4ff1ef84959a7e94f544b437e2404af5287",
            bytes: 77_617_693,
            licence: "Cut from ZFTurbo's Mega 53 model (MIT-licensed training code); the split itself states no licence.",
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
            id: "mega-53-digital-piano",
            name: "Digital piano (Mega 53, on its own)",
            category: "Instruments",
            group: "Mega 53, one instrument at a time",
            sdr: None,
            sdr_note: "no public score; one instrument taken out of the Mega 53 model, a 78 MB download instead of 1.37 GB",
            target_name: "Digital piano",
            rest_name: "Without digital piano",
            stems: &[],
            author: "ZFTurbo (MVSep), split per instrument by noblebarkrr",
            page: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems",
            url: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems/resolve/main/v1/bs_mega_53stem_digital-piano_mvsep.ckpt",
            sha256: "481c741ff36208c9b11860297ff5c9a502873662724d35a437bf64aaade890bd",
            bytes: 77_622_628,
            licence: "Cut from ZFTurbo's Mega 53 model (MIT-licensed training code); the split itself states no licence.",
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
            id: "mega-53-dobro",
            name: "Dobro (Mega 53, on its own)",
            category: "Instruments",
            group: "Mega 53, one instrument at a time",
            sdr: None,
            sdr_note: "no public score; one instrument taken out of the Mega 53 model, a 78 MB download instead of 1.37 GB",
            target_name: "Dobro",
            rest_name: "Without dobro",
            stems: &[],
            author: "ZFTurbo (MVSep), split per instrument by noblebarkrr",
            page: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems",
            url: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems/resolve/main/v1/bs_mega_53stem_dobro_mvsep.ckpt",
            sha256: "a8b99b1b837f04200cbb5b99d33c0ea2ac46bb76424879385d8fc6dde8abfe0b",
            bytes: 77_616_988,
            licence: "Cut from ZFTurbo's Mega 53 model (MIT-licensed training code); the split itself states no licence.",
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
            id: "mega-53-double-bass",
            name: "Double bass (Mega 53, on its own)",
            category: "Instruments",
            group: "Mega 53, one instrument at a time",
            sdr: None,
            sdr_note: "no public score; one instrument taken out of the Mega 53 model, a 78 MB download instead of 1.37 GB",
            target_name: "Double bass",
            rest_name: "Without double bass",
            stems: &[],
            author: "ZFTurbo (MVSep), split per instrument by noblebarkrr",
            page: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems",
            url: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems/resolve/main/v1/bs_mega_53stem_double-bass_mvsep.ckpt",
            sha256: "731d7bcaae4ce02f5a19116c1b5c1004bfe7208d14cc6b93aaab2753c0302cfb",
            bytes: 77_621_218,
            licence: "Cut from ZFTurbo's Mega 53 model (MIT-licensed training code); the split itself states no licence.",
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
            id: "mega-53-drums",
            name: "Drum kit (Mega 53, on its own)",
            category: "Instruments",
            group: "Mega 53, one instrument at a time",
            sdr: None,
            sdr_note: "no public score; one instrument taken out of the Mega 53 model, a 78 MB download instead of 1.37 GB",
            target_name: "Drum kit",
            rest_name: "Without drum kit",
            stems: &[],
            author: "ZFTurbo (MVSep), split per instrument by noblebarkrr",
            page: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems",
            url: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems/resolve/main/v1/bs_mega_53stem_drums_mvsep.ckpt",
            sha256: "abfa64203417f0874aa3936320b1666364ef1badd917f5613a3017d0ea73c7a2",
            bytes: 77_616_988,
            licence: "Cut from ZFTurbo's Mega 53 model (MIT-licensed training code); the split itself states no licence.",
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
            id: "mega-53-electric-guitar",
            name: "Electric guitar (Mega 53, on its own)",
            category: "Instruments",
            group: "Mega 53, one instrument at a time",
            sdr: None,
            sdr_note: "no public score; one instrument taken out of the Mega 53 model, a 78 MB download instead of 1.37 GB",
            target_name: "Electric guitar",
            rest_name: "Without electric guitar",
            stems: &[],
            author: "ZFTurbo (MVSep), split per instrument by noblebarkrr",
            page: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems",
            url: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems/resolve/main/v1/bs_mega_53stem_electric-guitar_mvsep.ckpt",
            sha256: "cd506bfce9474f91a31001967f2c4935ce4e67f643da3df20d04058da927c553",
            bytes: 77_624_038,
            licence: "Cut from ZFTurbo's Mega 53 model (MIT-licensed training code); the split itself states no licence.",
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
            id: "mega-53-flute",
            name: "Flute (Mega 53, on its own)",
            category: "Instruments",
            group: "Mega 53, one instrument at a time",
            sdr: None,
            sdr_note: "no public score; one instrument taken out of the Mega 53 model, a 78 MB download instead of 1.37 GB",
            target_name: "Flute",
            rest_name: "Without flute",
            stems: &[],
            author: "ZFTurbo (MVSep), split per instrument by noblebarkrr",
            page: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems",
            url: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems/resolve/main/v1/bs_mega_53stem_flute_mvsep.ckpt",
            sha256: "65dfa1e2c093165812cbdf8945715e7f71566bc3a8196ecad3edce1d29bb2e45",
            bytes: 77_616_988,
            licence: "Cut from ZFTurbo's Mega 53 model (MIT-licensed training code); the split itself states no licence.",
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
            id: "mega-53-french-horn",
            name: "French horn (Mega 53, on its own)",
            category: "Instruments",
            group: "Mega 53, one instrument at a time",
            sdr: None,
            sdr_note: "no public score; one instrument taken out of the Mega 53 model, a 78 MB download instead of 1.37 GB",
            target_name: "French horn",
            rest_name: "Without french horn",
            stems: &[],
            author: "ZFTurbo (MVSep), split per instrument by noblebarkrr",
            page: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems",
            url: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems/resolve/main/v1/bs_mega_53stem_french-horn_mvsep.ckpt",
            sha256: "484b9e68a962891daffb1d671f847509a7e6c7f72de42c445c774881dfe2c420",
            bytes: 77_621_218,
            licence: "Cut from ZFTurbo's Mega 53 model (MIT-licensed training code); the split itself states no licence.",
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
            id: "mega-53-glockenspiel",
            name: "Glockenspiel (Mega 53, on its own)",
            category: "Instruments",
            group: "Mega 53, one instrument at a time",
            sdr: None,
            sdr_note: "no public score; one instrument taken out of the Mega 53 model, a 78 MB download instead of 1.37 GB",
            target_name: "Glockenspiel",
            rest_name: "Without glockenspiel",
            stems: &[],
            author: "ZFTurbo (MVSep), split per instrument by noblebarkrr",
            page: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems",
            url: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems/resolve/main/v1/bs_mega_53stem_glockenspiel_mvsep.ckpt",
            sha256: "dc18ac41fb8e0e1cf2dcb290d67c9e2f7a295f380e91864eda9f3cc805015231",
            bytes: 77_621_923,
            licence: "Cut from ZFTurbo's Mega 53 model (MIT-licensed training code); the split itself states no licence.",
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
            id: "mega-53-guitar",
            name: "Guitar (Mega 53, on its own)",
            category: "Instruments",
            group: "Mega 53, one instrument at a time",
            sdr: None,
            sdr_note: "no public score; one instrument taken out of the Mega 53 model, a 78 MB download instead of 1.37 GB",
            target_name: "Guitar",
            rest_name: "Without guitar",
            stems: &[],
            author: "ZFTurbo (MVSep), split per instrument by noblebarkrr",
            page: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems",
            url: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems/resolve/main/v1/bs_mega_53stem_guitar_mvsep.ckpt",
            sha256: "9e4524d60cc7eb01eae4048ecda6e3fbf9a9a404e12838f4ed58b8dfb733762d",
            bytes: 77_617_693,
            licence: "Cut from ZFTurbo's Mega 53 model (MIT-licensed training code); the split itself states no licence.",
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
            id: "mega-53-harmonica",
            name: "Harmonica (Mega 53, on its own)",
            category: "Instruments",
            group: "Mega 53, one instrument at a time",
            sdr: None,
            sdr_note: "no public score; one instrument taken out of the Mega 53 model, a 78 MB download instead of 1.37 GB",
            target_name: "Harmonica",
            rest_name: "Without harmonica",
            stems: &[],
            author: "ZFTurbo (MVSep), split per instrument by noblebarkrr",
            page: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems",
            url: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems/resolve/main/v1/bs_mega_53stem_harmonica_mvsep.ckpt",
            sha256: "90f9e794573c6492695efb18c5bd8967f28f7a1832aa9d237a2ebe53ffd7347c",
            bytes: 77_619_808,
            licence: "Cut from ZFTurbo's Mega 53 model (MIT-licensed training code); the split itself states no licence.",
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
            id: "mega-53-harp",
            name: "Harp (Mega 53, on its own)",
            category: "Instruments",
            group: "Mega 53, one instrument at a time",
            sdr: None,
            sdr_note: "no public score; one instrument taken out of the Mega 53 model, a 78 MB download instead of 1.37 GB",
            target_name: "Harp",
            rest_name: "Without harp",
            stems: &[],
            author: "ZFTurbo (MVSep), split per instrument by noblebarkrr",
            page: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems",
            url: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems/resolve/main/v1/bs_mega_53stem_harp_mvsep.ckpt",
            sha256: "5fde42f720b062391e5a62df3b0ab86244bb0a6471b4896db3677e88cbf90ef6",
            bytes: 77_616_219,
            licence: "Cut from ZFTurbo's Mega 53 model (MIT-licensed training code); the split itself states no licence.",
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
            id: "mega-53-harpsichord",
            name: "Harpsichord (Mega 53, on its own)",
            category: "Instruments",
            group: "Mega 53, one instrument at a time",
            sdr: None,
            sdr_note: "no public score; one instrument taken out of the Mega 53 model, a 78 MB download instead of 1.37 GB",
            target_name: "Harpsichord",
            rest_name: "Without harpsichord",
            stems: &[],
            author: "ZFTurbo (MVSep), split per instrument by noblebarkrr",
            page: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems",
            url: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems/resolve/main/v1/bs_mega_53stem_harpsichord_mvsep.ckpt",
            sha256: "362f6c681a465aea21d3d4b8d8eba29e0e2ae41152bf0b8798201cfb00f32ee1",
            bytes: 77_621_218,
            licence: "Cut from ZFTurbo's Mega 53 model (MIT-licensed training code); the split itself states no licence.",
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
            id: "mega-53-hh",
            name: "Hi-hat (Mega 53, on its own)",
            category: "Instruments",
            group: "Mega 53, one instrument at a time",
            sdr: None,
            sdr_note: "no public score; one instrument taken out of the Mega 53 model, a 78 MB download instead of 1.37 GB",
            target_name: "Hi-hat",
            rest_name: "Without hi-hat",
            stems: &[],
            author: "ZFTurbo (MVSep), split per instrument by noblebarkrr",
            page: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems",
            url: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems/resolve/main/v1/bs_mega_53stem_hh_mvsep.ckpt",
            sha256: "94901a05d358f16b2de4064eec45f0947bb28bf239040a3fe79e2ae7fadd2984",
            bytes: 77_614_809,
            licence: "Cut from ZFTurbo's Mega 53 model (MIT-licensed training code); the split itself states no licence.",
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
            id: "mega-53-keys",
            name: "Keys (Mega 53, on its own)",
            category: "Instruments",
            group: "Mega 53, one instrument at a time",
            sdr: None,
            sdr_note: "no public score; one instrument taken out of the Mega 53 model, a 78 MB download instead of 1.37 GB",
            target_name: "Keys",
            rest_name: "Without keys",
            stems: &[],
            author: "ZFTurbo (MVSep), split per instrument by noblebarkrr",
            page: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems",
            url: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems/resolve/main/v1/bs_mega_53stem_keys_mvsep.ckpt",
            sha256: "4bc2bde8c5b26d5e22720304deace45dfeaffb2d02dc07922a8f2be17d82c844",
            bytes: 77_616_219,
            licence: "Cut from ZFTurbo's Mega 53 model (MIT-licensed training code); the split itself states no licence.",
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
            id: "mega-53-kick",
            name: "Kick (Mega 53, on its own)",
            category: "Instruments",
            group: "Mega 53, one instrument at a time",
            sdr: None,
            sdr_note: "no public score; one instrument taken out of the Mega 53 model, a 78 MB download instead of 1.37 GB",
            target_name: "Kick",
            rest_name: "Without kick",
            stems: &[],
            author: "ZFTurbo (MVSep), split per instrument by noblebarkrr",
            page: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems",
            url: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems/resolve/main/v1/bs_mega_53stem_kick_mvsep.ckpt",
            sha256: "ad914b83f9ef13bedf9608771f4f1a86174e27a84f7f235dc2d1dd91bab85fbc",
            bytes: 77_616_219,
            licence: "Cut from ZFTurbo's Mega 53 model (MIT-licensed training code); the split itself states no licence.",
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
            id: "mega-53-lead-vocal",
            name: "Lead vocal (Mega 53, on its own)",
            category: "Instruments",
            group: "Mega 53, one instrument at a time",
            sdr: None,
            sdr_note: "no public score; one instrument taken out of the Mega 53 model, a 78 MB download instead of 1.37 GB",
            target_name: "Lead vocal",
            rest_name: "Without lead vocal",
            stems: &[],
            author: "ZFTurbo (MVSep), split per instrument by noblebarkrr",
            page: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems",
            url: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems/resolve/main/v1/bs_mega_53stem_lead-vocal_mvsep.ckpt",
            sha256: "7622ef83040f3a1e683d4250af10bcc52ebc36f0b4188ff2101d23c6dae310ce",
            bytes: 77_620_513,
            licence: "Cut from ZFTurbo's Mega 53 model (MIT-licensed training code); the split itself states no licence.",
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
            id: "mega-53-mandolin",
            name: "Mandolin (Mega 53, on its own)",
            category: "Instruments",
            group: "Mega 53, one instrument at a time",
            sdr: None,
            sdr_note: "no public score; one instrument taken out of the Mega 53 model, a 78 MB download instead of 1.37 GB",
            target_name: "Mandolin",
            rest_name: "Without mandolin",
            stems: &[],
            author: "ZFTurbo (MVSep), split per instrument by noblebarkrr",
            page: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems",
            url: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems/resolve/main/v1/bs_mega_53stem_mandolin_mvsep.ckpt",
            sha256: "1a5589c6ee51a8f3f87f68e48aaf41792c31f63d5c0eaba497ae0ecd4bb6a675",
            bytes: 77_619_103,
            licence: "Cut from ZFTurbo's Mega 53 model (MIT-licensed training code); the split itself states no licence.",
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
            id: "mega-53-marimba",
            name: "Marimba (Mega 53, on its own)",
            category: "Instruments",
            group: "Mega 53, one instrument at a time",
            sdr: None,
            sdr_note: "no public score; one instrument taken out of the Mega 53 model, a 78 MB download instead of 1.37 GB",
            target_name: "Marimba",
            rest_name: "Without marimba",
            stems: &[],
            author: "ZFTurbo (MVSep), split per instrument by noblebarkrr",
            page: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems",
            url: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems/resolve/main/v1/bs_mega_53stem_marimba_mvsep.ckpt",
            sha256: "559835d6525898228f1ebcfac770ed44185217e547159a3c0f836391637fc3e8",
            bytes: 77_618_398,
            licence: "Cut from ZFTurbo's Mega 53 model (MIT-licensed training code); the split itself states no licence.",
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
            id: "mega-53-oboe",
            name: "Oboe (Mega 53, on its own)",
            category: "Instruments",
            group: "Mega 53, one instrument at a time",
            sdr: None,
            sdr_note: "no public score; one instrument taken out of the Mega 53 model, a 78 MB download instead of 1.37 GB",
            target_name: "Oboe",
            rest_name: "Without oboe",
            stems: &[],
            author: "ZFTurbo (MVSep), split per instrument by noblebarkrr",
            page: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems",
            url: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems/resolve/main/v1/bs_mega_53stem_oboe_mvsep.ckpt",
            sha256: "7de88790f82b86af910f943f5a31f30237c48997011e6123c5922449f2a22285",
            bytes: 77_616_219,
            licence: "Cut from ZFTurbo's Mega 53 model (MIT-licensed training code); the split itself states no licence.",
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
            id: "mega-53-organ",
            name: "Organ (Mega 53, on its own)",
            category: "Instruments",
            group: "Mega 53, one instrument at a time",
            sdr: None,
            sdr_note: "no public score; one instrument taken out of the Mega 53 model, a 78 MB download instead of 1.37 GB",
            target_name: "Organ",
            rest_name: "Without organ",
            stems: &[],
            author: "ZFTurbo (MVSep), split per instrument by noblebarkrr",
            page: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems",
            url: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems/resolve/main/v1/bs_mega_53stem_organ_mvsep.ckpt",
            sha256: "14c1c2a26fbd5061be85a31a170b6989ba296871895905ea6a1a048d200895ba",
            bytes: 77_616_988,
            licence: "Cut from ZFTurbo's Mega 53 model (MIT-licensed training code); the split itself states no licence.",
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
            id: "mega-53-percussion",
            name: "Percussion (Mega 53, on its own)",
            category: "Instruments",
            group: "Mega 53, one instrument at a time",
            sdr: None,
            sdr_note: "no public score; one instrument taken out of the Mega 53 model, a 78 MB download instead of 1.37 GB",
            target_name: "Percussion",
            rest_name: "Without percussion",
            stems: &[],
            author: "ZFTurbo (MVSep), split per instrument by noblebarkrr",
            page: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems",
            url: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems/resolve/main/v1/bs_mega_53stem_percussion_mvsep.ckpt",
            sha256: "82187665185d51e6050b4d6afaf45e7acafd85d1eefffd86c47ac0aa601e089f",
            bytes: 77_620_513,
            licence: "Cut from ZFTurbo's Mega 53 model (MIT-licensed training code); the split itself states no licence.",
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
            id: "mega-53-piano",
            name: "Piano (Mega 53, on its own)",
            category: "Instruments",
            group: "Mega 53, one instrument at a time",
            sdr: None,
            sdr_note: "no public score; one instrument taken out of the Mega 53 model, a 78 MB download instead of 1.37 GB",
            target_name: "Piano",
            rest_name: "Without piano",
            stems: &[],
            author: "ZFTurbo (MVSep), split per instrument by noblebarkrr",
            page: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems",
            url: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems/resolve/main/v1/bs_mega_53stem_piano_mvsep.ckpt",
            sha256: "8e58ad544386136028ec1f5764bb95a235235492144112d94782ff1333905fe9",
            bytes: 77_616_988,
            licence: "Cut from ZFTurbo's Mega 53 model (MIT-licensed training code); the split itself states no licence.",
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
            id: "mega-53-saxophone",
            name: "Saxophone (Mega 53, on its own)",
            category: "Instruments",
            group: "Mega 53, one instrument at a time",
            sdr: None,
            sdr_note: "no public score; one instrument taken out of the Mega 53 model, a 78 MB download instead of 1.37 GB",
            target_name: "Saxophone",
            rest_name: "Without saxophone",
            stems: &[],
            author: "ZFTurbo (MVSep), split per instrument by noblebarkrr",
            page: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems",
            url: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems/resolve/main/v1/bs_mega_53stem_saxophone_mvsep.ckpt",
            sha256: "7027da3852828194f224f5f822582a38015b0afc452fe3e407229084c10de6a9",
            bytes: 77_619_808,
            licence: "Cut from ZFTurbo's Mega 53 model (MIT-licensed training code); the split itself states no licence.",
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
            id: "mega-53-sitar",
            name: "Sitar (Mega 53, on its own)",
            category: "Instruments",
            group: "Mega 53, one instrument at a time",
            sdr: None,
            sdr_note: "no public score; one instrument taken out of the Mega 53 model, a 78 MB download instead of 1.37 GB",
            target_name: "Sitar",
            rest_name: "Without sitar",
            stems: &[],
            author: "ZFTurbo (MVSep), split per instrument by noblebarkrr",
            page: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems",
            url: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems/resolve/main/v1/bs_mega_53stem_sitar_mvsep.ckpt",
            sha256: "32683e3fcddebe1a5605fb5cffc76f8e89ec19d35f0ed49fe6f0f31476fd37d1",
            bytes: 77_616_988,
            licence: "Cut from ZFTurbo's Mega 53 model (MIT-licensed training code); the split itself states no licence.",
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
            id: "mega-53-snare",
            name: "Snare (Mega 53, on its own)",
            category: "Instruments",
            group: "Mega 53, one instrument at a time",
            sdr: None,
            sdr_note: "no public score; one instrument taken out of the Mega 53 model, a 78 MB download instead of 1.37 GB",
            target_name: "Snare",
            rest_name: "Without snare",
            stems: &[],
            author: "ZFTurbo (MVSep), split per instrument by noblebarkrr",
            page: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems",
            url: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems/resolve/main/v1/bs_mega_53stem_snare_mvsep.ckpt",
            sha256: "c4cde14867f1311836d4aa07649e39df4df43c4ba98780783fe96cd4fe8293bf",
            bytes: 77_616_988,
            licence: "Cut from ZFTurbo's Mega 53 model (MIT-licensed training code); the split itself states no licence.",
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
            id: "mega-53-strings",
            name: "Strings (ensemble) (Mega 53, on its own)",
            category: "Instruments",
            group: "Mega 53, one instrument at a time",
            sdr: None,
            sdr_note: "no public score; one instrument taken out of the Mega 53 model, a 78 MB download instead of 1.37 GB",
            target_name: "Strings (ensemble)",
            rest_name: "Without strings (ensemble)",
            stems: &[],
            author: "ZFTurbo (MVSep), split per instrument by noblebarkrr",
            page: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems",
            url: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems/resolve/main/v1/bs_mega_53stem_strings_mvsep.ckpt",
            sha256: "2e8b76ff287f6127e1f3dd927b56946784d368d5a40e7f4f74b1373a647675c5",
            bytes: 77_618_398,
            licence: "Cut from ZFTurbo's Mega 53 model (MIT-licensed training code); the split itself states no licence.",
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
            id: "mega-53-synth",
            name: "Synth (Mega 53, on its own)",
            category: "Instruments",
            group: "Mega 53, one instrument at a time",
            sdr: None,
            sdr_note: "no public score; one instrument taken out of the Mega 53 model, a 78 MB download instead of 1.37 GB",
            target_name: "Synth",
            rest_name: "Without synth",
            stems: &[],
            author: "ZFTurbo (MVSep), split per instrument by noblebarkrr",
            page: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems",
            url: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems/resolve/main/v1/bs_mega_53stem_synth_mvsep.ckpt",
            sha256: "04749df7e2fab770a0d26bf9aa4850c1a6cd89547fc8bfe9c9d223602c0afb82",
            bytes: 77_616_988,
            licence: "Cut from ZFTurbo's Mega 53 model (MIT-licensed training code); the split itself states no licence.",
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
            id: "mega-53-tambourine",
            name: "Tambourine (Mega 53, on its own)",
            category: "Instruments",
            group: "Mega 53, one instrument at a time",
            sdr: None,
            sdr_note: "no public score; one instrument taken out of the Mega 53 model, a 78 MB download instead of 1.37 GB",
            target_name: "Tambourine",
            rest_name: "Without tambourine",
            stems: &[],
            author: "ZFTurbo (MVSep), split per instrument by noblebarkrr",
            page: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems",
            url: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems/resolve/main/v1/bs_mega_53stem_tambourine_mvsep.ckpt",
            sha256: "8e22d5e185ab3db1262f1fc6b98eff22d9f97fc6f6e7f381cacf4624e6b69ad2",
            bytes: 77_620_513,
            licence: "Cut from ZFTurbo's Mega 53 model (MIT-licensed training code); the split itself states no licence.",
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
            id: "mega-53-timpani",
            name: "Timpani (Mega 53, on its own)",
            category: "Instruments",
            group: "Mega 53, one instrument at a time",
            sdr: None,
            sdr_note: "no public score; one instrument taken out of the Mega 53 model, a 78 MB download instead of 1.37 GB",
            target_name: "Timpani",
            rest_name: "Without timpani",
            stems: &[],
            author: "ZFTurbo (MVSep), split per instrument by noblebarkrr",
            page: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems",
            url: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems/resolve/main/v1/bs_mega_53stem_timpani_mvsep.ckpt",
            sha256: "a2f4813c366246f3af010340d9f1c7854bcb48e3187e51065dfaff34ca515b54",
            bytes: 77_618_398,
            licence: "Cut from ZFTurbo's Mega 53 model (MIT-licensed training code); the split itself states no licence.",
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
            id: "mega-53-toms",
            name: "Toms (Mega 53, on its own)",
            category: "Instruments",
            group: "Mega 53, one instrument at a time",
            sdr: None,
            sdr_note: "no public score; one instrument taken out of the Mega 53 model, a 78 MB download instead of 1.37 GB",
            target_name: "Toms",
            rest_name: "Without toms",
            stems: &[],
            author: "ZFTurbo (MVSep), split per instrument by noblebarkrr",
            page: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems",
            url: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems/resolve/main/v1/bs_mega_53stem_toms_mvsep.ckpt",
            sha256: "ceb45bbc16419363bb842484dadcc786a9ca290c4553c9eced0e98dc8f0f2458",
            bytes: 77_616_219,
            licence: "Cut from ZFTurbo's Mega 53 model (MIT-licensed training code); the split itself states no licence.",
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
            id: "mega-53-triangle",
            name: "Triangle (Mega 53, on its own)",
            category: "Instruments",
            group: "Mega 53, one instrument at a time",
            sdr: None,
            sdr_note: "no public score; one instrument taken out of the Mega 53 model, a 78 MB download instead of 1.37 GB",
            target_name: "Triangle",
            rest_name: "Without triangle",
            stems: &[],
            author: "ZFTurbo (MVSep), split per instrument by noblebarkrr",
            page: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems",
            url: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems/resolve/main/v1/bs_mega_53stem_triangle_mvsep.ckpt",
            sha256: "8e3692157868c6c7d0d4d6f36be263ed363e01cf66e4c97a400acb8c05ca250f",
            bytes: 77_619_103,
            licence: "Cut from ZFTurbo's Mega 53 model (MIT-licensed training code); the split itself states no licence.",
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
            id: "mega-53-trombone",
            name: "Trombone (Mega 53, on its own)",
            category: "Instruments",
            group: "Mega 53, one instrument at a time",
            sdr: None,
            sdr_note: "no public score; one instrument taken out of the Mega 53 model, a 78 MB download instead of 1.37 GB",
            target_name: "Trombone",
            rest_name: "Without trombone",
            stems: &[],
            author: "ZFTurbo (MVSep), split per instrument by noblebarkrr",
            page: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems",
            url: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems/resolve/main/v1/bs_mega_53stem_trombone_mvsep.ckpt",
            sha256: "f50d2e13e3e02cd74c3a5e084975d64a2a1f2090757c64495944a00823947f0d",
            bytes: 77_619_103,
            licence: "Cut from ZFTurbo's Mega 53 model (MIT-licensed training code); the split itself states no licence.",
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
            id: "mega-53-trumpet",
            name: "Trumpet (Mega 53, on its own)",
            category: "Instruments",
            group: "Mega 53, one instrument at a time",
            sdr: None,
            sdr_note: "no public score; one instrument taken out of the Mega 53 model, a 78 MB download instead of 1.37 GB",
            target_name: "Trumpet",
            rest_name: "Without trumpet",
            stems: &[],
            author: "ZFTurbo (MVSep), split per instrument by noblebarkrr",
            page: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems",
            url: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems/resolve/main/v1/bs_mega_53stem_trumpet_mvsep.ckpt",
            sha256: "47530e8c6ee14eb3f4469efc629b0a3942cfff062f3a7eb788cb15355fd7b724",
            bytes: 77_618_398,
            licence: "Cut from ZFTurbo's Mega 53 model (MIT-licensed training code); the split itself states no licence.",
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
            id: "mega-53-tuba",
            name: "Tuba (Mega 53, on its own)",
            category: "Instruments",
            group: "Mega 53, one instrument at a time",
            sdr: None,
            sdr_note: "no public score; one instrument taken out of the Mega 53 model, a 78 MB download instead of 1.37 GB",
            target_name: "Tuba",
            rest_name: "Without tuba",
            stems: &[],
            author: "ZFTurbo (MVSep), split per instrument by noblebarkrr",
            page: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems",
            url: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems/resolve/main/v1/bs_mega_53stem_tuba_mvsep.ckpt",
            sha256: "0bb341f247101ba9e723a57089e0bdade930eab9fa09c349d1d36c3272bce34f",
            bytes: 77_616_219,
            licence: "Cut from ZFTurbo's Mega 53 model (MIT-licensed training code); the split itself states no licence.",
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
            id: "mega-53-ukulele",
            name: "Ukulele (Mega 53, on its own)",
            category: "Instruments",
            group: "Mega 53, one instrument at a time",
            sdr: None,
            sdr_note: "no public score; one instrument taken out of the Mega 53 model, a 78 MB download instead of 1.37 GB",
            target_name: "Ukulele",
            rest_name: "Without ukulele",
            stems: &[],
            author: "ZFTurbo (MVSep), split per instrument by noblebarkrr",
            page: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems",
            url: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems/resolve/main/v1/bs_mega_53stem_ukulele_mvsep.ckpt",
            sha256: "56eb5a2cb4c3a45be630c27b03815ead66e9a892922b84f9ac27eeb1fe0be2f0",
            bytes: 77_618_398,
            licence: "Cut from ZFTurbo's Mega 53 model (MIT-licensed training code); the split itself states no licence.",
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
            id: "mega-53-viola",
            name: "Viola (Mega 53, on its own)",
            category: "Instruments",
            group: "Mega 53, one instrument at a time",
            sdr: None,
            sdr_note: "no public score; one instrument taken out of the Mega 53 model, a 78 MB download instead of 1.37 GB",
            target_name: "Viola",
            rest_name: "Without viola",
            stems: &[],
            author: "ZFTurbo (MVSep), split per instrument by noblebarkrr",
            page: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems",
            url: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems/resolve/main/v1/bs_mega_53stem_viola_mvsep.ckpt",
            sha256: "41d47d879a45fe94bba0db01476cce5e93395b06c756b98463b1eda95a468354",
            bytes: 77_616_988,
            licence: "Cut from ZFTurbo's Mega 53 model (MIT-licensed training code); the split itself states no licence.",
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
            id: "mega-53-violin",
            name: "Violin (Mega 53, on its own)",
            category: "Instruments",
            group: "Mega 53, one instrument at a time",
            sdr: None,
            sdr_note: "no public score; one instrument taken out of the Mega 53 model, a 78 MB download instead of 1.37 GB",
            target_name: "Violin",
            rest_name: "Without violin",
            stems: &[],
            author: "ZFTurbo (MVSep), split per instrument by noblebarkrr",
            page: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems",
            url: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems/resolve/main/v1/bs_mega_53stem_violin_mvsep.ckpt",
            sha256: "43dd11a1816ba7c58d74f4de1a48ffc05b98fc499dfb24cf8e71c5ba729f2ada",
            bytes: 77_617_693,
            licence: "Cut from ZFTurbo's Mega 53 model (MIT-licensed training code); the split itself states no licence.",
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
            id: "mega-53-vocal",
            name: "Vocals (Mega 53, on its own)",
            category: "Instruments",
            group: "Mega 53, one instrument at a time",
            sdr: None,
            sdr_note: "no public score; one instrument taken out of the Mega 53 model, a 78 MB download instead of 1.37 GB",
            target_name: "Vocals",
            rest_name: "Without vocals",
            stems: &[],
            author: "ZFTurbo (MVSep), split per instrument by noblebarkrr",
            page: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems",
            url: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems/resolve/main/v1/bs_mega_53stem_vocal_mvsep.ckpt",
            sha256: "917580ad6a1bab75d1b438db537ab98387839efb1809e5a50c78b7afdb96c8af",
            bytes: 77_616_988,
            licence: "Cut from ZFTurbo's Mega 53 model (MIT-licensed training code); the split itself states no licence.",
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
            id: "mega-53-wind",
            name: "Winds (Mega 53, on its own)",
            category: "Instruments",
            group: "Mega 53, one instrument at a time",
            sdr: None,
            sdr_note: "no public score; one instrument taken out of the Mega 53 model, a 78 MB download instead of 1.37 GB",
            target_name: "Winds",
            rest_name: "Without winds",
            stems: &[],
            author: "ZFTurbo (MVSep), split per instrument by noblebarkrr",
            page: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems",
            url: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems/resolve/main/v1/bs_mega_53stem_wind_mvsep.ckpt",
            sha256: "c2ae2f8b7aa777dba3596fa8a8c0aec42b1bf3c493ebdc9aaf2daeffcab9d025",
            bytes: 77_616_219,
            licence: "Cut from ZFTurbo's Mega 53 model (MIT-licensed training code); the split itself states no licence.",
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
            id: "mega-53-wind-chimes",
            name: "Wind chimes (Mega 53, on its own)",
            category: "Instruments",
            group: "Mega 53, one instrument at a time",
            sdr: None,
            sdr_note: "no public score; one instrument taken out of the Mega 53 model, a 78 MB download instead of 1.37 GB",
            target_name: "Wind chimes",
            rest_name: "Without wind chimes",
            stems: &[],
            author: "ZFTurbo (MVSep), split per instrument by noblebarkrr",
            page: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems",
            url: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems/resolve/main/v1/bs_mega_53stem_wind-chimes_mvsep.ckpt",
            sha256: "9bcb89de76916a44243e89ffd02051b527848afd8d2d618bd82e04de3bd861f3",
            bytes: 77_621_218,
            licence: "Cut from ZFTurbo's Mega 53 model (MIT-licensed training code); the split itself states no licence.",
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
            id: "mega-53-woodwind",
            name: "Woodwinds (Mega 53, on its own)",
            category: "Instruments",
            group: "Mega 53, one instrument at a time",
            sdr: None,
            sdr_note: "no public score; one instrument taken out of the Mega 53 model, a 78 MB download instead of 1.37 GB",
            target_name: "Woodwinds",
            rest_name: "Without woodwinds",
            stems: &[],
            author: "ZFTurbo (MVSep), split per instrument by noblebarkrr",
            page: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems",
            url: "https://huggingface.co/noblebarkrr/BS-Roformer-MVSep-Mega-53-stems/resolve/main/v1/bs_mega_53stem_woodwind_mvsep.ckpt",
            sha256: "9a1335d2cd21cd4fdd6964e7ab96c9e9776b2643644f763e2f59bb6c53d39060",
            bytes: 77_619_103,
            licence: "Cut from ZFTurbo's Mega 53 model (MIT-licensed training code); the split itself states no licence.",
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
            id: "duet-drypaintman",
            name: "Duet (two singers apart)",
            category: "Vocals",
            group: "",
            sdr: Some(7.93),
            sdr_note: "the trainer's own test, given in the file name",
            target_name: "Singer 1",
            rest_name: "Singer 2",
            stems: &[
                ("Singer 1", 0.0),
                ("Singer 2", 0.0),
            ],
            author: "DryPaintMan",
            page: "https://huggingface.co/DryPaintMan/MelBandRoformer-Duet",
            url: "https://huggingface.co/DryPaintMan/MelBandRoformer-Duet/resolve/main/model_mel_band_roformer_ep_0_sdr_7.9319_fixed.ckpt",
            sha256: "9ae8ae1a7d2fab0a7c405fee27ac8332702a31d689de58c9fb6f63b30410832b",
            bytes: 630_204_333,
            licence: "No licence is stated for the weights.",
            spec: || {
                json!({
                    "arch": "mel", "num_bands": 60,
                    "sample_rate": 44100, "n_fft": 2048, "hop": 441, "win": 2048,
                    "heads": 8, "dim_head": 64, "chunk_size": 352800, "num_overlap": 2
                })
            },
        },
        ModelEntry {
            id: "karaoke-gilliaan",
            name: "Lead vocal (gilliaan)",
            category: "Vocals",
            group: "",
            sdr: None,
            sdr_note: "no public score",
            target_name: "Lead vocal",
            rest_name: "Backing vocals and instruments",
            stems: &[
                ("Lead vocal", 0.0),
                ("Backing vocals and instruments", 0.0),
            ],
            author: "gilliaan",
            page: "https://huggingface.co/gilliaan/Stem-Separation-Models",
            url: "https://huggingface.co/gilliaan/Stem-Separation-Models/resolve/main/Karaoke/MelBandRoformer/gilliaan_melroformer_karaoke_v1.ckpt",
            sha256: "6aee6813b27f5f27d81c9be46f17c0db34fa7194e5e57276211a76477506061f",
            bytes: 869_725_664,
            licence: "CC BY-NC-SA 4.0, as stated by the trainer: free for non-commercial use, with credit, shared alike.",
            spec: || {
                json!({
                    "arch": "mel", "num_bands": 60,
                    "sample_rate": 44100, "n_fft": 2048, "hop": 441, "win": 2048,
                    "heads": 8, "dim_head": 64, "chunk_size": 882000, "num_overlap": 2
                })
            },
        },
        ModelEntry {
            id: "drums-gilliaan",
            name: "Drums (gilliaan)",
            category: "Instruments",
            group: "",
            sdr: None,
            sdr_note: "no public score",
            target_name: "Drums",
            rest_name: "Other",
            stems: &[
                ("Drums", 0.0),
                ("Other", 0.0),
            ],
            author: "gilliaan",
            page: "https://huggingface.co/gilliaan/Stem-Separation-Models",
            url: "https://huggingface.co/gilliaan/Stem-Separation-Models/resolve/main/Drums/BandSplitRoformer/gilliaan_bsroformer_drums_v1.ckpt",
            sha256: "f60c4370257257301922dec9c6528cfab1b82b02d772ae0a0090cb566d4c4ca7",
            bytes: 303_490_678,
            licence: "CC BY-NC-SA 4.0, as stated by the trainer: free for non-commercial use, with credit, shared alike.",
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
            id: "deeffect-gilliaan",
            name: "De-effect (dry vocals, beta)",
            category: "Denoise",
            group: "",
            sdr: None,
            sdr_note: "no public score; takes reverb and delay off a vocal. Beta, as the trainer says",
            target_name: "Dry vocals",
            rest_name: "Effects",
            stems: &[],
            author: "gilliaan",
            page: "https://huggingface.co/gilliaan/Stem-Separation-Models",
            url: "https://huggingface.co/gilliaan/Stem-Separation-Models/resolve/main/DeEffect/BandSplitRoformer/gilliaan_bsroformer_deeffect_v1beta.ckpt",
            sha256: "c18a1bcb951257166cef8ab2455cd15b53961836f29b5ddae632846ec6800b5e",
            bytes: 204_495_355,
            licence: "CC BY-NC-SA 4.0, as stated by the trainer: free for non-commercial use, with credit, shared alike.",
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
            id: "cleaninvert-gilliaan",
            name: "Clean an inverted vocal",
            category: "Denoise",
            group: "",
            sdr: None,
            sdr_note: "no public score; cleans vocals got by phase inversion (a mix minus its instrumental)",
            target_name: "Clean vocals",
            rest_name: "Leftover",
            stems: &[],
            author: "gilliaan",
            page: "https://huggingface.co/gilliaan/Stem-Separation-Models",
            url: "https://huggingface.co/gilliaan/Stem-Separation-Models/resolve/main/CleanInvert/BandSplitRoformer/gilliaan_bsroformer_cleaninvert_v1.ckpt",
            sha256: "c8e93c31ac540f4c5cf2afcf85d23d78150e30215e8b5c89fdf308a1df99a45f",
            bytes: 204_495_355,
            licence: "CC BY-NC-SA 4.0, as stated by the trainer: free for non-commercial use, with credit, shared alike.",
            spec: || {
                json!({
                    "arch": "bs",
                    "freqs_per_bands": [2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,
                        4,4,4,4,4,4,4,4,4,4,4,4,12,12,12,12,12,12,12,12,24,24,24,24,24,24,24,24,
                        48,48,48,48,48,48,48,48,128,129],
                    "sample_rate": 44100, "n_fft": 2048, "hop": 512, "win": 2048,
                    "heads": 8, "dim_head": 64, "chunk_size": 785920, "num_overlap": 2
                })
            },
        },
        ModelEntry {
            id: "centerwide-mel-gilliaan",
            name: "Center and wide (Mel-Band v3)",
            category: "Stereo",
            group: "",
            sdr: None,
            sdr_note: "no public score; the trainer's newest and best-sounding RoFormer CenterWide. Learned, not a mid/side matrix: hard-panned parts go to wide",
            target_name: "Center",
            rest_name: "Wide",
            stems: &[
                ("Center", 0.0),
                ("Wide", 0.0),
            ],
            author: "gilliaan",
            page: "https://huggingface.co/gilliaan/CenterWide-Models",
            url: "https://huggingface.co/gilliaan/CenterWide-Models/resolve/main/MelbandRoformer/gilliaan_melroformer_centerwide_v3.ckpt",
            sha256: "2b909a3e49abcea44f36e81760a2f1769e14fde0b59a4bbd737c2eef2180ca75",
            bytes: 869_623_230,
            licence: "CC BY-NC-SA 4.0, as stated by the trainer: free for non-commercial use, with credit, shared alike.",
            spec: || {
                json!({
                    "arch": "mel", "num_bands": 60,
                    "sample_rate": 44100, "n_fft": 2048, "hop": 441, "win": 2048,
                    "heads": 8, "dim_head": 64, "chunk_size": 132300, "num_overlap": 4
                })
            },
        },
        ModelEntry {
            id: "centerwide-bs-gilliaan",
            name: "Center and wide (BS v3 beta)",
            category: "Stereo",
            group: "",
            sdr: None,
            sdr_note: "no public score; smaller BS-RoFormer CenterWide, beta",
            target_name: "Center",
            rest_name: "Wide",
            stems: &[
                ("Center", 0.0),
                ("Wide", 0.0),
            ],
            author: "gilliaan",
            page: "https://huggingface.co/gilliaan/CenterWide-Models",
            url: "https://huggingface.co/gilliaan/CenterWide-Models/resolve/main/BandSplitRoformer/gilliaan_bsroformer_centerwide_v3beta.ckpt",
            sha256: "9dc81b9d8080224325d96cf54b195d149f7cf7ab508b9b982efb17a35e9c8d66",
            bytes: 303_490_227,
            licence: "CC BY-NC-SA 4.0, as stated by the trainer: free for non-commercial use, with credit, shared alike.",
            spec: || {
                json!({
                    "arch": "bs",
                    "freqs_per_bands": [2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,
                        4,4,4,4,4,4,4,4,4,4,4,4,12,12,12,12,12,12,12,12,24,24,24,24,24,24,24,24,
                        48,48,48,48,48,48,48,48,128,129],
                    "sample_rate": 44100, "n_fft": 2048, "hop": 512, "win": 2048,
                    "heads": 8, "dim_head": 64, "chunk_size": 132300, "num_overlap": 4
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

/// The oldest engine that understands the jobs this Audiform writes.
pub const MIN_ENGINE: &str = "0.2.0";

/// Whether an engine's `--version` output (like "0.2.0") is at least `MIN_ENGINE`.
pub fn engine_is_new_enough(version: &str) -> bool {
    let parse = |v: &str| -> Option<(u32, u32, u32)> {
        let mut it = v.trim().split('.').map(|p| p.trim().parse::<u32>().ok());
        Some((it.next()??, it.next()??, it.next().flatten().unwrap_or(0)))
    };
    match (parse(version), parse(MIN_ENGINE)) {
        (Some(have), Some(need)) => have >= need,
        _ => false,
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
    fn ids_are_unique_and_every_model_has_a_tab() {
        let all = catalog();
        let mut ids: Vec<_> = all.iter().map(|m| m.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), all.len(), "duplicate model ids");
        for m in &all {
            assert!(
                ["Vocals", "Instruments", "Denoise", "Stereo"].contains(&m.category),
                "{}",
                m.id
            );
        }
        let mega = all.iter().find(|m| m.id == "mega-53").unwrap();
        assert_eq!(mega.output_names().len(), 53);
        let split: Vec<_> = all
            .iter()
            .filter(|m| m.group.starts_with("Mega 53"))
            .collect();
        assert_eq!(split.len(), 53);
        assert!(split.iter().all(|m| m.output_names().len() == 2));
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
    fn an_old_engine_is_noticed() {
        assert!(engine_is_new_enough("0.2.0\n"));
        assert!(engine_is_new_enough("0.3.1"));
        assert!(engine_is_new_enough("1.0.0"));
        assert!(!engine_is_new_enough("0.1.0"));
        assert!(!engine_is_new_enough("garbage"));
        assert!(!engine_is_new_enough(""));
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
