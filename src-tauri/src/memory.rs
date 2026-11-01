// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

//! Where the memory goes: the app's own process, the browser view that
//! draws the editor, each plug-in host, and anything else the app
//! started. Shown in Help > About and written to the audio log while
//! playing, so a heavy session can say what is heavy.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginMemory {
    /// The plug-ins running in one host process.
    pub name: String,
    pub memory_mb: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    /// The app's own process: the engine, files, sessions.
    pub app_mb: u64,
    /// The browser view's processes (the editor's window).
    pub view_mb: u64,
    pub view_processes: u32,
    /// Plug-in hosts, heaviest first.
    pub plugins: Vec<PluginMemory>,
    /// Anything else the app started (an audio separation run, for example).
    pub other_mb: u64,
    pub total_mb: u64,
    /// False where the system does not say (macOS for now).
    pub known: bool,
}

fn is_view(name: &str) -> bool {
    let n = name.to_lowercase();
    n.contains("webview") || n.contains("webkit")
}

/// The breakdown now.
pub fn report() -> Report {
    let procs = audiform_os_keys::processes();
    let hosts = crate::plugins::instance::host_pids();
    let me = std::process::id();
    let own_exe = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()));
    let mut r = Report {
        app_mb: 0,
        view_mb: 0,
        view_processes: 0,
        plugins: Vec::new(),
        other_mb: 0,
        total_mb: 0,
        known: !procs.is_empty(),
    };
    for p in &procs {
        r.total_mb += p.memory_mb;
        if p.pid == me {
            r.app_mb = p.memory_mb;
        } else if let Some((_, names)) = hosts.iter().find(|(pid, _)| *pid == p.pid) {
            r.plugins.push(PluginMemory {
                name: names.join(", "),
                memory_mb: p.memory_mb,
            });
        } else if is_view(&p.name) {
            r.view_mb += p.memory_mb;
            r.view_processes += 1;
        } else if own_exe.as_deref() == Some(&p.name.to_lowercase()) {
            // A plug-in host still starting, or one whose plug-in has stopped.
            r.plugins.push(PluginMemory {
                name: "plug-in host".into(),
                memory_mb: p.memory_mb,
            });
        } else {
            r.other_mb += p.memory_mb;
        }
    }
    r.plugins.sort_by_key(|p| std::cmp::Reverse(p.memory_mb));
    r
}

/// One line for the audio log.
pub fn line() -> Option<String> {
    let r = report();
    if !r.known {
        return None;
    }
    let plugins: u64 = r.plugins.iter().map(|p| p.memory_mb).sum();
    let mut s = format!(
        "memory: {} MB in all: Audiform {} MB, editor view {} MB ({} processes), plug-ins {} MB",
        r.total_mb, r.app_mb, r.view_mb, r.view_processes, plugins
    );
    if !r.plugins.is_empty() {
        let list: Vec<String> = r
            .plugins
            .iter()
            .take(8)
            .map(|p| format!("{} {}", p.name, p.memory_mb))
            .collect();
        s.push_str(&format!(" ({})", list.join(", ")));
    }
    if r.other_mb > 0 {
        s.push_str(&format!(", other {} MB", r.other_mb));
    }
    Some(s)
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_report_counts_this_process() {
        let r = super::report();
        if r.known {
            assert!(r.app_mb > 0);
            assert!(r.total_mb >= r.app_mb);
            assert!(super::line().unwrap().starts_with("memory: "));
        }
    }
}
