// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) The Audiform contributors.

//! The messages of protocol 2, one JSON object per line, each with a
//! `type`. Audiform writes [`Request`]s to the engine's standard input and
//! reads [`Reply`]s from its standard output.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PROTOCOL: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    /// First, always: the engine answers `Ready` or refuses.
    Hello {
        protocol: u32,
        host: String,
        host_version: String,
    },
    /// Loads a model (instant when the same one is loaded already).
    Load {
        model: String,
        /// The model's settings, as Audiform's model list keeps them.
        spec: Value,
        checkpoint: PathBuf,
        /// "gpu" or "cpu".
        device: String,
    },
    Run {
        job: u64,
        input: PathBuf,
        outputs: Vec<Output>,
        #[serde(skip_serializing_if = "Option::is_none")]
        overlap: Option<usize>,
    },
    Cancel {
        job: u64,
    },
    Unload,
    Quit,
}

/// One file the engine writes: these stems added up, or the mix without them.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Output {
    pub path: PathBuf,
    pub stems: Vec<usize>,
    #[serde(default)]
    pub rest: bool,
}

impl Output {
    pub fn stems(path: impl Into<PathBuf>, stems: &[usize]) -> Self {
        Output {
            path: path.into(),
            stems: stems.to_vec(),
            rest: false,
        }
    }

    /// The mix with these stems taken out.
    pub fn without(path: impl Into<PathBuf>, stems: &[usize]) -> Self {
        Output {
            path: path.into(),
            stems: stems.to_vec(),
            rest: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Reply {
    Ready {
        engine: String,
        protocol: u32,
    },
    Loaded {
        model: String,
        device: String,
        stems: usize,
        channels: usize,
        sample_rate: u32,
        seconds: f64,
    },
    Progress {
        job: u64,
        done: f32,
    },
    Done {
        job: u64,
        seconds: f64,
    },
    Cancelled {
        job: u64,
    },
    Unloaded,
    Error {
        #[serde(default)]
        job: Option<u64>,
        #[serde(default)]
        gpu: bool,
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_match_the_engine() {
        let s = serde_json::to_string(&Request::Hello {
            protocol: 2,
            host: "Audiform".into(),
            host_version: "2.0.0".into(),
        })
        .unwrap();
        assert_eq!(
            s,
            r#"{"type":"hello","protocol":2,"host":"Audiform","host_version":"2.0.0"}"#
        );
        let s = serde_json::to_string(&Request::Run {
            job: 1,
            input: "in.wav".into(),
            outputs: vec![Output::without("b.wav", &[0])],
            overlap: None,
        })
        .unwrap();
        assert_eq!(
            s,
            r#"{"type":"run","job":1,"input":"in.wav","outputs":[{"path":"b.wav","stems":[0],"rest":true}]}"#
        );
        let r: Reply =
            serde_json::from_str(r#"{"type":"error","job":null,"gpu":true,"message":"x"}"#)
                .unwrap();
        assert_eq!(
            r,
            Reply::Error {
                job: None,
                gpu: true,
                message: "x".into()
            }
        );
    }
}
