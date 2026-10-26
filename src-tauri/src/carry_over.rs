// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

//! The first time Audiform starts, it copies the settings Altered Audio
//! Editor kept on this computer (preferences, keyboard shortcuts,
//! workspaces, the plug-in list, templates), so nothing has to be set up
//! again. Audiform has its own app id, so the two keep separate folders;
//! the old folder is only read, never changed, so Altered Audio Editor
//! keeps working if it is still installed.

use std::path::Path;

/// Altered Audio Editor's app id: the name of its settings folders.
const OLD_ID: &str = "com.thegcrn.alteredaudioeditor";

/// Copies the folder beside `dir` named after the old app id into `dir`,
/// when `dir` does not exist yet. Errors are ignored: starting fresh is
/// the fallback.
pub fn carry_over(dir: &Path) {
    if dir.exists() {
        return;
    }
    let Some(old) = dir.parent().map(|p| p.join(OLD_ID)) else {
        return;
    };
    if old.is_dir() {
        let _ = copy_dir(&old, dir);
    }
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_once_and_leaves_the_old_folder() {
        let root = tempfile::tempdir().unwrap();
        let old = root.path().join(OLD_ID);
        std::fs::create_dir_all(old.join("workspaces")).unwrap();
        std::fs::write(old.join("settings.json"), "{\"a\":1}").unwrap();
        std::fs::write(old.join("workspaces").join("mine.json"), "{}").unwrap();
        let new = root.path().join("io.github.nwh212.audiform");
        carry_over(&new);
        assert_eq!(
            std::fs::read_to_string(new.join("settings.json")).unwrap(),
            "{\"a\":1}"
        );
        assert!(new.join("workspaces").join("mine.json").is_file());
        assert!(old.join("settings.json").is_file());
        // Once Audiform has its own folder, nothing is copied again.
        std::fs::write(old.join("settings.json"), "{\"a\":2}").unwrap();
        carry_over(&new);
        assert_eq!(
            std::fs::read_to_string(new.join("settings.json")).unwrap(),
            "{\"a\":1}"
        );
    }
}
