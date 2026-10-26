// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

//! Windows asks whether Audiform may use public networks when a plug-in
//! inside it starts listening for connections (licence checks, plug-ins
//! of one maker finding each other). Audiform itself never listens. A
//! firewall rule that blocks incoming connections for this program
//! answers the question ahead of time, so Windows stops asking.
//! Connections that stay on this computer are not affected.
//!
//! The installer adds that rule, but a portable copy, or an install where
//! the administrator prompt was declined, has none. These commands check
//! for a rule covering this exact program file and add one, which takes
//! one administrator prompt. Nothing here goes on the network.

use serde::Serialize;
use tauri::async_runtime::spawn_blocking;

/// The rule's name, the same one the installer uses.
#[cfg_attr(not(windows), allow(dead_code))]
const RULE: &str = "Audiform (no incoming network)";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FirewallStatus {
    /// False off Windows, where nothing asks.
    pub supported: bool,
    /// Incoming connections to this program are already blocked.
    pub covered: bool,
    /// The program file the rule is for.
    pub program: String,
}

/// A PowerShell string literal: single quotes, with any inside doubled.
#[cfg_attr(not(windows), allow(dead_code))]
fn quoted(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// The script that adds the rule for `program` (run elevated). An older
/// rule of ours for the same file is replaced, so there is only ever one.
#[cfg_attr(not(windows), allow(dead_code))]
fn add_script(program: &str) -> String {
    let (name, prog) = (quoted(RULE), quoted(program));
    format!(
        "Get-NetFirewallApplicationFilter -Program {prog} -ErrorAction SilentlyContinue | \
         Get-NetFirewallRule | Where-Object {{ $_.DisplayName -eq {name} }} | Remove-NetFirewallRule; \
         New-NetFirewallRule -DisplayName {name} -Direction Inbound -Action Block -Program {prog} -Profile Any | Out-Null"
    )
}

/// PowerShell's -EncodedCommand form: UTF-16LE, base64.
#[cfg_attr(not(windows), allow(dead_code))]
fn encoded(script: &str) -> String {
    let bytes: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    altered_plughost::protocol::base64_encode(&bytes)
}

#[cfg(windows)]
fn powershell(script: &str) -> Result<std::process::Output, String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-EncodedCommand",
            &encoded(script),
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("Could not ask Windows about its firewall: {e}"))
}

fn program() -> String {
    std::env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn status() -> Result<FirewallStatus, String> {
    let program = program();
    #[cfg(windows)]
    {
        // Any enabled rule blocking incoming connections to this file counts,
        // including one Windows made when its question was answered with Cancel.
        let script = format!(
            "@(Get-NetFirewallApplicationFilter -Program {} -ErrorAction SilentlyContinue | Get-NetFirewallRule | \
             Where-Object {{ $_.Direction -eq 'Inbound' -and $_.Action -eq 'Block' -and $_.Enabled -eq 'True' }}).Count",
            quoted(&program)
        );
        let out = powershell(&script)?;
        let n: u32 = String::from_utf8_lossy(&out.stdout)
            .trim()
            .parse()
            .unwrap_or(0);
        Ok(FirewallStatus {
            supported: true,
            covered: n > 0,
            program,
        })
    }
    #[cfg(not(windows))]
    Ok(FirewallStatus {
        supported: false,
        covered: true,
        program,
    })
}

/// Whether Windows already blocks incoming connections for this program.
#[tauri::command]
pub async fn firewall_status() -> Result<FirewallStatus, String> {
    spawn_blocking(status).await.map_err(|e| e.to_string())?
}

/// Adds the rule for this program (one administrator prompt), then says
/// whether it is in place.
#[tauri::command]
pub async fn firewall_block() -> Result<FirewallStatus, String> {
    spawn_blocking(|| {
        #[cfg(windows)]
        {
            let inner = encoded(&add_script(&program()));
            // Start-Process with RunAs shows the administrator prompt and waits.
            let outer = format!(
                "try {{ Start-Process powershell -Verb RunAs -Wait -WindowStyle Hidden \
                 -ArgumentList '-NoProfile','-NonInteractive','-EncodedCommand','{inner}'; exit 0 }} catch {{ exit 1 }}"
            );
            let out = powershell(&outer)?;
            if !out.status.success() {
                return Err("Windows did not get permission to add the rule (the administrator prompt was declined).".into());
            }
        }
        status()
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_paths_with_apostrophes() {
        assert_eq!(
            quoted(r"C:\Users\O'Neil\Audiform.exe"),
            r"'C:\Users\O''Neil\Audiform.exe'"
        );
    }

    #[test]
    fn the_script_names_the_rule_and_the_program() {
        let s = add_script(r"D:\Apps\Audiform\Audiform.exe");
        assert!(s.contains("'Audiform (no incoming network)'"));
        assert!(s.contains(r"-Program 'D:\Apps\Audiform\Audiform.exe'"));
        assert!(s.contains("-Direction Inbound -Action Block"));
    }

    #[test]
    fn encodes_as_utf16_base64() {
        // "a" is 61 00 in UTF-16LE.
        assert_eq!(encoded("a"), "YQA=");
    }
}
