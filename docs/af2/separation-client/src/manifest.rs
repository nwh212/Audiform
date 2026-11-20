// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) The Audiform contributors.

//! The engine release's manifest: one small JSON file naming the engine's
//! version, its protocol and, per computer, the download with its size and
//! SHA-256. Audiform fetches it first, so it learns everything in one
//! request and downloads only its own build.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{Error, PROTOCOL};

/// The manifest's file name in the engine release.
pub const MANIFEST_FILE: &str = "audiform2-separate.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Manifest {
    /// Always "audiform-separate".
    pub format: String,
    /// The engine's version, like "1.0.0".
    pub engine: String,
    pub protocol: u32,
    /// By computer: "windows-x64", "linux-x64".
    pub assets: BTreeMap<String, Asset>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Asset {
    /// The zip's file name in the same release.
    pub file: String,
    pub bytes: u64,
    pub sha256: String,
}

/// This computer's name in the manifest, if a build exists for it.
pub fn platform() -> Option<&'static str> {
    if cfg!(all(windows, target_arch = "x86_64")) {
        Some("windows-x64")
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some("linux-x64")
    } else {
        None
    }
}

impl Manifest {
    /// Reads a manifest, refusing one for another protocol.
    pub fn parse(bytes: &[u8]) -> Result<Manifest, Error> {
        let m: Manifest = serde_json::from_slice(bytes)
            .map_err(|e| Error::Install(format!("The engine's manifest could not be read: {e}")))?;
        if m.format != "audiform-separate" {
            return Err(Error::Install(
                "That is not a separation engine manifest.".into(),
            ));
        }
        if m.protocol != PROTOCOL {
            return Err(Error::Install(format!(
                "That engine speaks protocol {}, and this Audiform speaks {PROTOCOL}.",
                m.protocol
            )));
        }
        Ok(m)
    }

    /// This computer's download.
    pub fn asset(&self) -> Result<&Asset, Error> {
        let p = platform().ok_or_else(|| {
            Error::Install("Audio Separation is not available for this computer yet.".into())
        })?;
        self.assets
            .get(p)
            .ok_or_else(|| Error::Install(format!("The engine release has no build for {p}.")))
    }

    /// Whether `installed` (an engine's `--version`) is older than this release.
    pub fn is_newer_than(&self, installed: &str) -> bool {
        let v = |s: &str| -> Vec<u32> {
            s.trim()
                .split('.')
                .map(|p| p.parse().unwrap_or(0))
                .collect()
        };
        v(&self.engine) > v(installed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{"format":"audiform-separate","engine":"1.0.0","protocol":2,
        "assets":{"windows-x64":{"file":"audiform2-separate-windows-x64.zip","bytes":10,"sha256":"ab"},
                  "linux-x64":{"file":"audiform2-separate-linux-x64.zip","bytes":12,"sha256":"cd"}}}"#;

    #[test]
    fn picks_this_computers_build() {
        let m = Manifest::parse(SAMPLE.as_bytes()).unwrap();
        if platform().is_some() {
            assert!(m.asset().unwrap().file.starts_with("audiform2-separate-"));
        }
        assert!(m.is_newer_than("0.2.0"));
        assert!(!m.is_newer_than("1.0.0"));
        assert!(m.is_newer_than("0.9.9"));
    }

    #[test]
    fn refuses_other_protocols() {
        let old = SAMPLE.replace("\"protocol\":2", "\"protocol\":1");
        assert!(Manifest::parse(old.as_bytes()).is_err());
        assert!(Manifest::parse(b"{}").is_err());
    }
}
