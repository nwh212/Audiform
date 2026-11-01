// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

//! Modifier keys read straight from the system. While files are dragged
//! in from the file manager, the system owns the keyboard, so the window
//! never hears key presses; asking the system is the only way to know
//! whether Alt is down.
//!
//! Also here: the memory each process of the app's family uses (the app,
//! its browser view, its plug-in hosts), for the memory readout.
//!
//! This is its own small crate because the system calls need `unsafe`,
//! and the app crate forbids `unsafe` everywhere.

/// Whether Alt is held right now, or `None` where this is not known
/// (systems other than Windows: the window's own key events are used there).
pub fn alt_down() -> Option<bool> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_MENU};
        // SAFETY: GetAsyncKeyState only reads the key state; any key code is valid.
        let state = unsafe { GetAsyncKeyState(i32::from(VK_MENU)) };
        // The high bit is set while the key is down.
        Some(state < 0)
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// One process of the app's family: the app itself or something it started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Process {
    pub pid: u32,
    pub parent: u32,
    /// The program's file name, without its folder.
    pub name: String,
    /// Memory in use now (the working set), in MB.
    pub memory_mb: u64,
}

/// This process and everything it started (and what those started):
/// the browser view's processes, plug-in hosts, helpers. Empty where
/// the system does not say (macOS for now).
pub fn processes() -> Vec<Process> {
    let all = every_process();
    let me = std::process::id();
    let mut out: Vec<Process> = all.iter().filter(|p| p.pid == me).cloned().collect();
    let mut i = 0;
    while i < out.len() {
        let pid = out[i].pid;
        for p in all.iter().filter(|p| p.parent == pid && p.pid != pid) {
            if !out.iter().any(|o| o.pid == p.pid) {
                out.push(p.clone());
            }
        }
        i += 1;
    }
    out
}

#[cfg(windows)]
fn every_process() -> Vec<Process> {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::ProcessStatus::{
        K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
    };
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    let mut out = Vec::new();
    // SAFETY: a snapshot of the process list; every entry is read through
    // a zeroed, sized PROCESSENTRY32W, and the handle is closed after.
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return out;
        }
        let mut e: PROCESSENTRY32W = std::mem::zeroed();
        e.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut ok = Process32FirstW(snap, &mut e);
        while ok != 0 {
            let len = e
                .szExeFile
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(e.szExeFile.len());
            let name = String::from_utf16_lossy(&e.szExeFile[..len]);
            let mut memory_mb = 0;
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, e.th32ProcessID);
            if !h.is_null() {
                let mut pmc: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
                pmc.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
                if K32GetProcessMemoryInfo(h, &mut pmc, pmc.cb) != 0 {
                    memory_mb = (pmc.WorkingSetSize / 1_048_576) as u64;
                }
                CloseHandle(h);
            }
            out.push(Process {
                pid: e.th32ProcessID,
                parent: e.th32ParentProcessID,
                name,
                memory_mb,
            });
            ok = Process32NextW(snap, &mut e);
        }
        CloseHandle(snap);
    }
    out
}

#[cfg(target_os = "linux")]
fn every_process() -> Vec<Process> {
    let Ok(dir) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in dir.flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|s| s.parse::<u32>().ok())
        else {
            continue;
        };
        let Ok(stat) = std::fs::read_to_string(entry.path().join("stat")) else {
            continue;
        };
        // "pid (name) state ppid ...": the name may hold spaces and brackets.
        let (Some(open), Some(close)) = (stat.find('('), stat.rfind(')')) else {
            continue;
        };
        let name = stat[open + 1..close].to_string();
        let parent = stat[close + 1..]
            .split_whitespace()
            .nth(1)
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(0);
        let memory_mb = std::fs::read_to_string(entry.path().join("status"))
            .ok()
            .and_then(|t| {
                t.lines()
                    .find(|l| l.starts_with("VmRSS:"))
                    .and_then(|l| l.split_whitespace().nth(1))
                    .and_then(|v| v.parse::<u64>().ok())
            })
            .map_or(0, |kb| kb / 1024);
        out.push(Process {
            pid,
            parent,
            name,
            memory_mb,
        });
    }
    out
}

#[cfg(not(any(windows, target_os = "linux")))]
fn every_process() -> Vec<Process> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    #[test]
    fn answers_on_every_system() {
        let a = super::alt_down();
        assert_eq!(a.is_some(), cfg!(windows));
    }

    #[test]
    fn the_family_starts_with_this_process() {
        let list = super::processes();
        if cfg!(any(windows, target_os = "linux")) {
            assert_eq!(list.first().map(|p| p.pid), Some(std::process::id()));
            assert!(list[0].memory_mb > 0);
        } else {
            assert!(list.is_empty());
        }
    }
}
