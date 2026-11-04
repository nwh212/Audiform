// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

//! Check for Updates: asks Audiform's GitHub releases page which version is
//! newest and compares it with this one. Only when the user picks Help >
//! Check for Updates, or once a week if they switched that on in Settings
//! (off by default). Nothing is downloaded or installed here; a newer
//! version is offered as a link to its release page.

use serde::Serialize;
use tauri::async_runtime::spawn_blocking;

use crate::separation::download;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    /// This build's version, like "0.7.1".
    pub current: String,
    /// The newest release's version, like "0.8.0".
    pub latest: String,
    pub newer: bool,
    /// The newest release's page.
    pub page: String,
}

fn parse(v: &str) -> Option<(u64, u64, u64)> {
    let v = v.trim().trim_start_matches(['v', 'V']);
    let core = v.split(['-', '+']).next()?;
    let mut it = core.split('.').map(|p| p.parse::<u64>().ok());
    Some((
        it.next()??,
        it.next().flatten().unwrap_or(0),
        it.next().flatten().unwrap_or(0),
    ))
}

/// True when `latest` is a higher version than `current`.
pub fn is_newer(latest: &str, current: &str) -> bool {
    matches!((parse(latest), parse(current)), (Some(l), Some(c)) if l > c)
}

#[tauri::command]
pub async fn update_check() -> Result<UpdateInfo, String> {
    let tag = spawn_blocking(download::latest_release_tag)
        .await
        .map_err(|e| e.to_string())??;
    let current = env!("CARGO_PKG_VERSION").to_string();
    Ok(UpdateInfo {
        newer: is_newer(&tag, &current),
        latest: tag.trim_start_matches(['v', 'V']).to_string(),
        page: format!("https://github.com/nwh212/Audiform/releases/tag/{tag}"),
        current,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions_by_number() {
        assert!(is_newer("v0.8.0", "0.7.1"));
        assert!(is_newer("v0.7.10", "0.7.9"));
        assert!(is_newer("v1.0", "0.9.9"));
        assert!(!is_newer("v0.7.1", "0.7.1"));
        assert!(!is_newer("v0.7.0", "0.7.1"));
        assert!(!is_newer("nightly", "0.7.1"));
    }
}
