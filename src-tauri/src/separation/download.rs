// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

//! The one place Audiform reaches the internet: downloading the separation
//! engine and separation models, only when the user clicks Download.
//!
//! It uses the `curl` program that ships with Windows, macOS and Linux
//! rather than an HTTP library, so the editor gains no network code of its
//! own. Only HTTPS, only the hosts listed below, and every file is checked
//! against a known SHA-256 before it is used. Nothing about the user is
//! sent; it is a plain file download. See PRIVACY.md.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use sha2::{Digest, Sha256};

/// Where downloads may come from: Audiform's own releases, and the
/// authors' own pages for the models.
const HOSTS: &[&str] = &["https://github.com/", "https://huggingface.co/"];

pub fn allowed(url: &str) -> bool {
    HOSTS.iter().any(|h| url.starts_with(h))
}

/// Downloads `url` to `dest`, calling `progress(bytes so far)` as it goes.
/// Stops when `cancel` is set. The file is written beside `dest` first and
/// only renamed into place once complete.
pub fn fetch(
    url: &str,
    dest: &Path,
    cancel: &AtomicBool,
    mut progress: impl FnMut(u64),
) -> Result<(), String> {
    if !allowed(url) {
        return Err("That download is not from an allowed site.".into());
    }
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("Could not create {}: {e}", dir.display()))?;
    }
    let part = dest.with_extension("part");
    let _ = std::fs::remove_file(&part);
    let mut cmd = Command::new(curl());
    cmd.args([
        "--location",
        "--fail",
        "--silent",
        "--show-error",
        "--proto",
        "=https",
        "--proto-redir",
        "=https",
        "--output",
    ])
    .arg(&part)
    .arg(url)
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::piped());
    no_window(&mut cmd);
    let mut child = cmd.spawn().map_err(|e| {
        format!("Could not start the download ({e}). Audiform uses the system's curl program.")
    })?;
    loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = std::fs::remove_file(&part);
            return Err("Download cancelled.".into());
        }
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            if !status.success() {
                let mut err = String::new();
                if let Some(mut s) = child.stderr.take() {
                    let _ = s.read_to_string(&mut err);
                }
                let _ = std::fs::remove_file(&part);
                let err = err.trim();
                return Err(if err.is_empty() {
                    "The download failed.".into()
                } else {
                    format!("The download failed: {err}")
                });
            }
            break;
        }
        progress(std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0));
        std::thread::sleep(Duration::from_millis(250));
    }
    progress(std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0));
    std::fs::rename(&part, dest).map_err(|e| format!("Could not save {}: {e}", dest.display()))
}

/// SHA-256 of a file, as lowercase hex.
pub fn sha256(path: &Path) -> Result<String, String> {
    let mut f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Checks a downloaded file; a file that does not match is removed.
pub fn verify(path: &Path, expected: &str) -> Result<(), String> {
    let got = sha256(path)?;
    if got.eq_ignore_ascii_case(expected.trim()) {
        Ok(())
    } else {
        let _ = std::fs::remove_file(path);
        Err("The downloaded file did not match what was expected, so it was removed. Try again later.".into())
    }
}

fn curl() -> &'static str {
    // Windows 10 and later ship curl.exe in System32.
    if cfg!(windows) {
        "curl.exe"
    } else {
        "curl"
    }
}

#[cfg(windows)]
fn no_window(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn no_window(_: &mut Command) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_listed_https_hosts_are_allowed() {
        assert!(allowed(
            "https://github.com/nwh212/Audiform/releases/download/v1/x.zip"
        ));
        assert!(allowed("https://huggingface.co/a/b/resolve/main/m.ckpt"));
        assert!(!allowed("http://github.com/x"));
        assert!(!allowed("https://github.com.evil.example/x"));
        assert!(!allowed("https://example.com/m.ckpt"));
    }

    #[test]
    fn files_are_checked_against_their_hash() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.bin");
        std::fs::write(&p, b"abc").unwrap();
        assert_eq!(
            sha256(&p).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert!(verify(
            &p,
            "BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD"
        )
        .is_ok());
        assert!(verify(&p, &"0".repeat(64)).is_err());
        assert!(!p.exists());
    }
}
