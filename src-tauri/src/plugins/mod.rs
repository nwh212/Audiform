// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

//! VST3 plug-ins in Voice Chains.
//!
//! Every plug-in runs in a host process of its own (this executable,
//! started again with `--plugin-host`; see the `altered-plughost` crate),
//! so a plug-in that crashes or hangs takes down only its host. A chain
//! slot holding a plug-in has the effect id `vst3:<class id>`; its
//! controls are stored like any effect's (`p<param id>`, normalized 0 to
//! 1) and everything set in the plug-in's own window as `plugin_state`.
//!
//! - [`scan`] finds `.vst3` bundles in the standard folders and the
//!   user's own, and asks a host process what each holds (a bundle that
//!   crashes the scan is listed as failed, nothing more). Results are
//!   cached in `plugins.json` in the app's config folder.
//! - [`instance`] runs plug-ins: one host process per chain slot, kept
//!   while the session uses it, so its window, settings and sound carry
//!   on across stops and starts.

pub mod instance;

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock, RwLock};
use std::time::{Duration, Instant, UNIX_EPOCH};

use altered_plughost::protocol::{ClassInfo, ScanResult};
use serde::{Deserialize, Serialize};

use crate::dsp::effects::EffectInfo;

/// Effect ids of plug-ins start with this.
pub const PREFIX: &str = "vst3:";

/// The menu group plug-ins are listed under.
pub const CATEGORY: &str = "Plug-ins (VST3)";

/// Longest a bundle may take to scan before it is listed as failed.
const SCAN_TIMEOUT: Duration = Duration::from_secs(60);

/// A plug-in the scan found.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Found {
    pub bundle: PathBuf,
    pub info: ClassInfo,
}

/// A bundle that could not be read, and why.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Failed {
    pub bundle: PathBuf,
    pub error: String,
}

/// What `plugins.json` holds.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Registry {
    #[serde(default)]
    pub schema_version: u32,
    /// Folders the user added, searched after the standard ones.
    #[serde(default)]
    pub folders: Vec<PathBuf>,
    /// Standard folders the user turned off.
    #[serde(default)]
    pub skipped: Vec<PathBuf>,
    #[serde(default)]
    pub plugins: Vec<Found>,
    #[serde(default)]
    pub failed: Vec<Failed>,
    /// Each scanned bundle's modified time, so a rescan skips the unchanged.
    #[serde(default)]
    pub stamps: BTreeMap<PathBuf, u64>,
}

fn registry() -> &'static RwLock<Registry> {
    static R: OnceLock<RwLock<Registry>> = OnceLock::new();
    R.get_or_init(|| RwLock::new(Registry::default()))
}

fn config_file() -> &'static Mutex<Option<PathBuf>> {
    static F: Mutex<Option<PathBuf>> = Mutex::new(None);
    &F
}

/// Loads `plugins.json` from the app's config folder. Missing or damaged
/// files give an empty list (scan again to fill it).
pub fn load(config_dir: &Path) {
    let file = config_dir.join("plugins.json");
    let reg = std::fs::read(&file)
        .ok()
        .and_then(|b| serde_json::from_slice::<Registry>(&b).ok())
        .unwrap_or_default();
    if let Ok(mut f) = config_file().lock() {
        *f = Some(file);
    }
    if let Ok(mut r) = registry().write() {
        *r = reg;
    }
}

fn save(reg: &Registry) {
    let Some(file) = config_file().lock().ok().and_then(|f| f.clone()) else {
        return;
    };
    if let Some(dir) = file.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(text) = serde_json::to_vec_pretty(reg) {
        let tmp = file.with_extension("json.tmp");
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(&tmp, &file);
        }
    }
}

/// The registry as it is now.
pub fn snapshot() -> Registry {
    registry().read().map(|r| r.clone()).unwrap_or_default()
}

/// Replaces the registry (tests, and the scan).
pub fn set(reg: Registry) {
    if let Ok(mut r) = registry().write() {
        *r = reg;
    }
}

/// The folders plug-in installers use on this system.
pub fn standard_folders() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if cfg!(windows) {
        if let Some(p) = std::env::var_os("CommonProgramFiles") {
            v.push(PathBuf::from(p).join("VST3"));
        }
        if let Some(p) = std::env::var_os("LOCALAPPDATA") {
            v.push(
                PathBuf::from(p)
                    .join("Programs")
                    .join("Common")
                    .join("VST3"),
            );
        }
    } else if cfg!(target_os = "macos") {
        v.push(PathBuf::from("/Library/Audio/Plug-Ins/VST3"));
        if let Some(h) = std::env::var_os("HOME") {
            v.push(PathBuf::from(h).join("Library/Audio/Plug-Ins/VST3"));
        }
    } else {
        if let Some(h) = std::env::var_os("HOME") {
            v.push(PathBuf::from(h).join(".vst3"));
        }
        v.push(PathBuf::from("/usr/lib/vst3"));
        v.push(PathBuf::from("/usr/local/lib/vst3"));
    }
    v
}

/// Every folder searched: the standard ones the user kept, then theirs.
pub fn search_folders(reg: &Registry) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = standard_folders()
        .into_iter()
        .filter(|f| !reg.skipped.contains(f))
        .collect();
    for f in &reg.folders {
        if !v.contains(f) {
            v.push(f.clone());
        }
    }
    v
}

/// Adds or removes a folder of the user's own, or turns a standard one
/// off or on again. Saved at once.
pub fn set_folder(folder: &Path, on: bool) -> Registry {
    let mut reg = snapshot();
    let standard = standard_folders().iter().any(|f| f == folder);
    if standard {
        reg.skipped.retain(|f| f != folder);
        if !on {
            reg.skipped.push(folder.to_path_buf());
        }
    } else {
        reg.folders.retain(|f| f != folder);
        if on {
            reg.folders.push(folder.to_path_buf());
        }
    }
    save(&reg);
    set(reg.clone());
    reg
}

/// `.vst3` bundles (folders or single files) under `folder`, up to a few
/// levels deep (installers often add a vendor folder).
pub fn find_bundles(folder: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, depth: u32, out: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        let mut entries: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
        entries.sort();
        for p in entries {
            let is_bundle = p
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("vst3"));
            if is_bundle {
                out.push(p);
            } else if p.is_dir() && depth > 0 {
                walk(&p, depth - 1, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(folder, 4, &mut out);
    out
}

fn stamp(p: &Path) -> u64 {
    std::fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs())
}

/// The program that hosts plug-ins: this executable, unless a test says otherwise.
pub fn host_program() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("ALTERED_PLUGIN_HOST") {
        return Some(PathBuf::from(p));
    }
    std::env::current_exe().ok()
}

/// Asks a host process what one bundle holds.
pub fn scan_bundle(bundle: &Path) -> ScanResult {
    let fail = |e: String| ScanResult {
        classes: Vec::new(),
        error: Some(e),
    };
    let Some(exe) = host_program() else {
        return fail("The plug-in host could not be found.".into());
    };
    let mut child = match Command::new(exe)
        .arg("--plugin-scan")
        .arg(bundle)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return fail(format!("Could not start the plug-in host: {e}")),
    };
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() > SCAN_TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                return fail("It took too long to load and was skipped.".into());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => return fail(e.to_string()),
        }
    }
    let mut text = String::new();
    if let Some(mut out) = child.stdout.take() {
        use std::io::Read;
        let _ = out.read_to_string(&mut text);
    }
    match serde_json::from_str::<ScanResult>(text.trim()) {
        Ok(r) => r,
        Err(_) => fail("It crashed while loading, so it was skipped.".into()),
    }
}

/// What a scan did.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanReport {
    pub scanned: usize,
    pub found: usize,
    pub failed: usize,
}

/// Scans every search folder. Unchanged bundles are skipped unless `all`.
/// `progress` gets (done, total, bundle).
pub fn scan(all: bool, mut progress: impl FnMut(usize, usize, &Path)) -> ScanReport {
    let mut reg = snapshot();
    let mut bundles: Vec<PathBuf> = Vec::new();
    for f in search_folders(&reg) {
        for b in find_bundles(&f) {
            if !bundles.contains(&b) {
                bundles.push(b);
            }
        }
    }
    let present: HashSet<&PathBuf> = bundles.iter().collect();
    reg.plugins.retain(|p| present.contains(&p.bundle));
    reg.failed.retain(|p| present.contains(&p.bundle));
    reg.stamps.retain(|b, _| present.contains(b));
    let mut report = ScanReport::default();
    let total = bundles.len();
    for (i, b) in bundles.iter().enumerate() {
        progress(i, total, b);
        let s = stamp(b);
        if !all && reg.stamps.get(b) == Some(&s) {
            continue;
        }
        report.scanned += 1;
        reg.plugins.retain(|p| &p.bundle != b);
        reg.failed.retain(|p| &p.bundle != b);
        let r = scan_bundle(b);
        for info in r.classes {
            reg.plugins.push(Found {
                bundle: b.clone(),
                info,
            });
        }
        if let Some(error) = r.error {
            reg.failed.push(Failed {
                bundle: b.clone(),
                error,
            });
        }
        reg.stamps.insert(b.clone(), s);
    }
    progress(total, total, Path::new(""));
    reg.plugins.sort_by_key(|p| p.info.name.to_lowercase());
    report.found = reg.plugins.len();
    report.failed = reg.failed.len();
    reg.schema_version = 1;
    save(&reg);
    set(reg);
    report
}

pub fn is_plugin(effect: &str) -> bool {
    effect.starts_with(PREFIX)
}

/// The plug-in for an effect id, if it is installed.
pub fn find(effect: &str) -> Option<Found> {
    let cid = effect.strip_prefix(PREFIX)?;
    registry()
        .read()
        .ok()?
        .plugins
        .iter()
        .find(|p| p.info.cid.eq_ignore_ascii_case(cid))
        .cloned()
}

/// Effect descriptions name their fields with `&'static str`; plug-in
/// names are only known at run time, so each distinct string is kept
/// once for the life of the app.
fn intern(s: &str) -> &'static str {
    static SET: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();
    let set = SET.get_or_init(|| Mutex::new(HashSet::new()));
    let Ok(mut set) = set.lock() else {
        return "";
    };
    if let Some(found) = set.get(s) {
        return found;
    }
    let leaked: &'static str = Box::leak(s.to_string().into_boxed_str());
    set.insert(leaked);
    leaked
}

/// The control key for a plug-in parameter.
pub fn param_key(id: u32) -> String {
    format!("p{id}")
}

pub fn param_id(key: &str) -> Option<u32> {
    key.strip_prefix('p')?.parse().ok()
}

/// A plug-in as an effect, with its controls (normalized, 0 to 1).
pub fn effect_info(found: &Found) -> EffectInfo {
    let i = &found.info;
    EffectInfo {
        id: intern(&format!("{PREFIX}{}", i.cid)),
        name: intern(&i.name),
        category: CATEGORY,
        description: intern(&if i.vendor.is_empty() {
            "A VST3 plug-in. Open its window with the window button.".to_string()
        } else {
            format!(
                "A VST3 plug-in by {}. Open its window with the window button.",
                i.vendor
            )
        }),
        params: i
            .params
            .iter()
            .map(|p| {
                let mut info = crate::dsp::effects::num(
                    intern(&param_key(p.id)),
                    intern(&p.title),
                    intern(&p.units),
                    0.0,
                    1.0,
                    p.default.clamp(0.0, 1.0),
                    if p.steps > 0 {
                        1.0 / f64::from(p.steps)
                    } else {
                        0.001
                    },
                );
                if p.hidden {
                    info = info.hidden();
                }
                info
            })
            .collect(),
    }
}

/// Every installed plug-in, as effects.
pub fn catalog() -> Vec<EffectInfo> {
    snapshot().plugins.iter().map(effect_info).collect()
}

/// A plug-in's look-ahead from the scan, without starting it.
pub fn scanned_latency(effect: &str) -> usize {
    find(effect).map_or(0, |f| f.info.latency as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_bundles_a_few_levels_down() {
        let dir = tempfile::tempdir().unwrap();
        let v = dir.path().join("Vendor").join("Sub");
        std::fs::create_dir_all(v.join("Deep.vst3").join("Contents")).unwrap();
        std::fs::write(dir.path().join("Single.VST3"), b"x").unwrap();
        std::fs::write(dir.path().join("readme.txt"), b"x").unwrap();
        let found = find_bundles(dir.path());
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(found.iter().any(|p| p.ends_with("Deep.vst3")));
    }

    #[test]
    fn plug_ins_become_effects() {
        let f = Found {
            bundle: "x.vst3".into(),
            info: ClassInfo {
                cid: "00112233445566778899AABBCCDDEEFF".into(),
                name: "Tuner".into(),
                vendor: "Acme".into(),
                version: "1".into(),
                sub_categories: "Fx".into(),
                params: vec![
                    altered_plughost::protocol::ParamInfo {
                        id: 7,
                        title: "Speed".into(),
                        units: "ms".into(),
                        steps: 0,
                        default: 0.25,
                        hidden: false,
                    },
                    altered_plughost::protocol::ParamInfo {
                        id: 9,
                        title: "Meter".into(),
                        units: String::new(),
                        steps: 4,
                        default: 0.0,
                        hidden: true,
                    },
                ],
                latency: 64,
                has_editor: true,
            },
        };
        let e = effect_info(&f);
        assert_eq!(e.id, "vst3:00112233445566778899AABBCCDDEEFF");
        assert_eq!(e.category, CATEGORY);
        assert_eq!(e.params[0].key, "p7");
        assert_eq!(e.params[0].default, 0.25);
        assert!(e.params[1].hidden);
        assert_eq!(e.params[1].step, 0.25);
        assert_eq!(param_id("p7"), Some(7));
        assert_eq!(param_id("gain"), None);
    }
}
