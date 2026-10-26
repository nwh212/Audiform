// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

//! VST3 plug-in presets: `.vstpreset` files in the standard preset
//! folders, which every VST3 host reads and writes. So presets saved for
//! a plug-in in another program show up here, and presets saved here
//! show up there.
//!
//! A `.vstpreset` file is a header ("VST3", a version, the plug-in's class
//! id as 32 hex characters and where the chunk list starts), the chunks,
//! and the chunk list ("List", a count, then each chunk's id, offset and
//! size). "Comp" is the plug-in's settings, "Cont" its controller's.

use std::path::{Path, PathBuf};

use altered_plughost::protocol::{base64_decode, base64_encode};
use serde::Serialize;

/// A preset found on disk.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PresetFile {
    pub name: String,
    pub path: PathBuf,
    /// In the user's own folder, so it can be deleted here.
    pub own: bool,
}

/// The user's own preset folder: where presets are saved.
pub fn user_folder() -> Option<PathBuf> {
    if cfg!(windows) {
        let home = std::env::var_os("USERPROFILE")?;
        Some(PathBuf::from(home).join("Documents").join("VST3 Presets"))
    } else if cfg!(target_os = "macos") {
        let home = std::env::var_os("HOME")?;
        Some(PathBuf::from(home).join("Library/Audio/Presets"))
    } else {
        let home = std::env::var_os("HOME")?;
        Some(PathBuf::from(home).join(".vst3").join("presets"))
    }
}

/// Every folder presets are looked for in: the user's, then shared ones.
fn folders() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = user_folder().into_iter().collect();
    if cfg!(windows) {
        if let Some(p) = std::env::var_os("PROGRAMDATA") {
            v.push(PathBuf::from(p).join("VST3 Presets"));
        }
    } else if cfg!(target_os = "macos") {
        v.push(PathBuf::from("/Library/Audio/Presets"));
    } else {
        v.push(PathBuf::from("/usr/share/vst3/presets"));
        v.push(PathBuf::from("/usr/local/share/vst3/presets"));
    }
    v
}

/// The class id as the plug-in reports it, and as Windows stores the
/// same id (its first eight bytes in a different order). Files may use either.
fn class_ids(cid: &str) -> [String; 2] {
    let up = cid.to_ascii_uppercase();
    let b = up.as_bytes();
    let pair = |i: usize| &up[i * 2..i * 2 + 2];
    let swapped = if b.len() == 32 {
        [3, 2, 1, 0, 5, 4, 7, 6, 8, 9, 10, 11, 12, 13, 14, 15]
            .iter()
            .map(|&i| pair(i))
            .collect()
    } else {
        up.clone()
    };
    [up, swapped]
}

/// The class id a preset file is for, from its header.
fn class_of(head: &[u8]) -> Option<String> {
    if head.len() < 48 || &head[..4] != b"VST3" {
        return None;
    }
    std::str::from_utf8(&head[8..40])
        .ok()
        .map(str::to_ascii_uppercase)
}

/// The presets for the plug-in with class id `cid`, by name.
pub fn list(cid: &str) -> Vec<PresetFile> {
    let ids = class_ids(cid);
    let own = user_folder();
    let mut out = Vec::new();
    for root in folders() {
        let mine = own.as_ref() == Some(&root);
        walk(&root, 0, &mut |path| {
            let Ok(mut f) = std::fs::File::open(path) else {
                return;
            };
            let mut head = [0u8; 48];
            if std::io::Read::read_exact(&mut f, &mut head).is_err() {
                return;
            }
            if class_of(&head).is_some_and(|c| ids.contains(&c)) {
                out.push(PresetFile {
                    name: path
                        .file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    path: path.to_path_buf(),
                    own: mine,
                });
            }
        });
    }
    out.sort_by_key(|a| a.name.to_lowercase());
    out
}

/// Calls `f` for each `.vstpreset` under `dir`, a few folders deep.
fn walk(dir: &Path, depth: usize, f: &mut impl FnMut(&Path)) {
    if depth > 5 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, depth + 1, f);
        } else if p
            .extension()
            .is_some_and(|x| x.eq_ignore_ascii_case("vstpreset"))
        {
            f(&p);
        }
    }
}

/// A preset file's settings, in the form the plug-in host keeps them.
pub fn read(path: &Path) -> Result<String, String> {
    let data = std::fs::read(path).map_err(|e| format!("Could not read the preset: {e}"))?;
    let (comp, cont) = parse(&data).ok_or("This is not a VST3 preset file.")?;
    Ok(state_of(&comp, &cont))
}

fn state_of(comp: &[u8], cont: &[u8]) -> String {
    let mut out = Vec::with_capacity(comp.len() + cont.len() + 4);
    out.extend_from_slice(&(comp.len() as u32).to_le_bytes());
    out.extend_from_slice(comp);
    out.extend_from_slice(cont);
    base64_encode(&out)
}

/// The "Comp" and "Cont" chunks of a preset file.
fn parse(data: &[u8]) -> Option<(Vec<u8>, Vec<u8>)> {
    class_of(data.get(..48)?)?;
    let at = |i: usize, n: usize| data.get(i..i + n);
    let list = i64::from_le_bytes(at(40, 8)?.try_into().ok()?) as usize;
    if at(list, 4)? != b"List" {
        return None;
    }
    let count = i32::from_le_bytes(at(list + 4, 4)?.try_into().ok()?).max(0) as usize;
    let (mut comp, mut cont) = (None, Vec::new());
    for k in 0..count.min(64) {
        let e = list + 8 + k * 20;
        let id = at(e, 4)?;
        let off = i64::from_le_bytes(at(e + 4, 8)?.try_into().ok()?) as usize;
        let size = i64::from_le_bytes(at(e + 12, 8)?.try_into().ok()?) as usize;
        let body = at(off, size)?.to_vec();
        match id {
            b"Comp" => comp = Some(body),
            b"Cont" => cont = body,
            _ => {}
        }
    }
    Some((comp?, cont))
}

/// Saves settings (as the plug-in host keeps them) as a preset file in
/// the user's folder, under the plug-in's maker and name. Returns its path.
pub fn save(
    cid: &str,
    vendor: &str,
    plugin: &str,
    name: &str,
    state: &str,
) -> Result<PathBuf, String> {
    let raw = base64_decode(state).ok_or("Those settings could not be read.")?;
    let n = u32::from_le_bytes(
        raw.get(..4)
            .ok_or("Empty settings.")?
            .try_into()
            .map_err(|_| "Empty settings.")?,
    ) as usize;
    let comp = raw.get(4..4 + n).ok_or("Damaged settings.")?;
    let cont = raw.get(4 + n..).unwrap_or_default();
    let safe = |s: &str| {
        let t: String = s
            .chars()
            .map(|c| {
                if "<>:\"/\\|?*".contains(c) || c.is_control() {
                    ' '
                } else {
                    c
                }
            })
            .collect();
        let t = t.trim().to_string();
        if t.is_empty() {
            "Preset".to_string()
        } else {
            t
        }
    };
    let dir = user_folder()
        .ok_or("There is no preset folder on this system.")?
        .join(safe(vendor))
        .join(safe(plugin));
    std::fs::create_dir_all(&dir).map_err(|e| format!("Could not make the preset folder: {e}"))?;
    let path = dir.join(format!("{}.vstpreset", safe(name)));
    std::fs::write(&path, build(cid, comp, cont))
        .map_err(|e| format!("Could not save the preset: {e}"))?;
    Ok(path)
}

/// A preset file for class id `cid` holding these chunks.
fn build(cid: &str, comp: &[u8], cont: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"VST3");
    out.extend_from_slice(&1i32.to_le_bytes());
    let mut id = cid.to_ascii_uppercase().into_bytes();
    id.resize(32, b'0');
    out.extend_from_slice(&id);
    let list_at = 48 + comp.len() + cont.len();
    out.extend_from_slice(&(list_at as i64).to_le_bytes());
    out.extend_from_slice(comp);
    out.extend_from_slice(cont);
    out.extend_from_slice(b"List");
    let chunks: Vec<(&[u8; 4], usize, usize)> = if cont.is_empty() {
        vec![(b"Comp", 48, comp.len())]
    } else {
        vec![
            (b"Comp", 48, comp.len()),
            (b"Cont", 48 + comp.len(), cont.len()),
        ]
    };
    out.extend_from_slice(&(chunks.len() as i32).to_le_bytes());
    for (id, off, size) in chunks {
        out.extend_from_slice(id);
        out.extend_from_slice(&(off as i64).to_le_bytes());
        out.extend_from_slice(&(size as i64).to_le_bytes());
    }
    out
}

/// Deletes a preset file, only from the user's own folder.
pub fn delete(path: &Path) -> Result<(), String> {
    let own = user_folder().ok_or("There is no preset folder on this system.")?;
    if !path.starts_with(&own)
        || !path
            .extension()
            .is_some_and(|x| x.eq_ignore_ascii_case("vstpreset"))
    {
        return Err("Only presets in your own preset folder can be deleted here.".into());
    }
    std::fs::remove_file(path).map_err(|e| format!("Could not delete the preset: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_preset_file_round_trips() {
        let cid = "0123456789ABCDEF0123456789ABCDEF";
        let file = build(cid, b"component", b"controller");
        assert_eq!(class_of(&file[..48]).as_deref(), Some(cid));
        let (comp, cont) = parse(&file).unwrap();
        assert_eq!(comp, b"component");
        assert_eq!(cont, b"controller");
        let state = state_of(&comp, &cont);
        let raw = base64_decode(&state).unwrap();
        assert_eq!(&raw[4..13], b"component");
    }

    #[test]
    fn matches_either_byte_order() {
        let ids = class_ids("00112233445566778899AABBCCDDEEFF");
        assert!(ids.contains(&"33221100554477668899AABBCCDDEEFF".to_string()));
    }

    #[test]
    fn rejects_other_files() {
        assert!(parse(b"not a preset at all, not even close to one....").is_none());
    }
}
