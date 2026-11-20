// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) The Audiform contributors.

//! Checking and unpacking a downloaded engine.

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::Error;

/// The engine program's file name on this computer.
pub const EXE: &str = if cfg!(windows) {
    "audiform-separate.exe"
} else {
    "audiform-separate"
};

/// Checks a file against a SHA-256 (hex, either case).
pub fn verify(path: &Path, sha256: &str) -> Result<(), Error> {
    let mut f = File::open(path)
        .map_err(|e| Error::Install(format!("Could not read {}: {e}", path.display())))?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = f
            .read(&mut buf)
            .map_err(|e| Error::Install(e.to_string()))?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    let got: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
    if got.eq_ignore_ascii_case(sha256.trim()) {
        Ok(())
    } else {
        Err(Error::Install(
            "The download is damaged (its checksum does not match). Try again.".into(),
        ))
    }
}

/// Unpacks the engine and its licence files from `zip` into `dir`,
/// replacing an older engine there only once the new one is complete.
/// Returns the engine's path.
pub fn unpack(zip: &Path, dir: &Path) -> Result<PathBuf, Error> {
    let fail = |e: &dyn std::fmt::Display| {
        Error::Install(format!("The engine download could not be unpacked: {e}"))
    };
    let file = File::open(zip).map_err(|e| fail(&e))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| fail(&e))?;
    std::fs::create_dir_all(dir).map_err(|e| fail(&e))?;
    for name in [EXE, "LICENSE.txt", "THIRD-PARTY-NOTICES.txt"] {
        let mut entry = match archive.by_name(name) {
            Ok(e) => e,
            Err(_) if name != EXE => continue,
            Err(_) => {
                return Err(Error::Install(
                    "The download does not hold the engine.".into(),
                ))
            }
        };
        let dest = dir.join(name);
        let tmp = dest.with_extension("new");
        {
            let mut out = File::create(&tmp).map_err(|e| fail(&e))?;
            std::io::copy(&mut entry, &mut out).map_err(|e| fail(&e))?;
            out.flush().map_err(|e| fail(&e))?;
        }
        #[cfg(unix)]
        if name == EXE {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))
                .map_err(|e| fail(&e))?;
        }
        std::fs::rename(&tmp, &dest).map_err(|e| fail(&e))?;
    }
    Ok(dir.join(EXE))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verifies_and_unpacks() {
        let dir = std::env::temp_dir().join(format!("af-client-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let zip_path = dir.join("engine.zip");
        {
            let mut w = zip::ZipWriter::new(File::create(&zip_path).unwrap());
            let o = zip::write::SimpleFileOptions::default();
            w.start_file(EXE, o).unwrap();
            w.write_all(b"program").unwrap();
            w.start_file("LICENSE.txt", o).unwrap();
            w.write_all(b"licence").unwrap();
            w.finish().unwrap();
        }
        let sum: String = Sha256::digest(std::fs::read(&zip_path).unwrap())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        verify(&zip_path, &sum.to_uppercase()).unwrap();
        assert!(verify(&zip_path, "00").is_err());
        let exe = unpack(&zip_path, &dir.join("engine")).unwrap();
        assert_eq!(std::fs::read(&exe).unwrap(), b"program");
        assert!(dir.join("engine").join("LICENSE.txt").is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
