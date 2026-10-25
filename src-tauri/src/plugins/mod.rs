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

use altered_plughost::protocol::{parse_scan_output, ClassInfo, ScanExit, ScanResult, SCAN_PREFIX};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::dsp::effects::EffectInfo;

/// Effect ids of plug-ins start with this.
pub const PREFIX: &str = "vst3:";

/// The menu group plug-ins are listed under.
pub const CATEGORY: &str = "Plug-ins (VST3)";

/// Longest a bundle may take to scan before it is listed as failed.
const SCAN_TIMEOUT: Duration = Duration::from_secs(60);

/// Bundles read at the same time.
const SCAN_THREADS: usize = 4;

/// Version of `plugins.json`. 2: instruments are listed too. 3: scans
/// before it crashed on many plug-ins, so everything is read again.
const SCHEMA_VERSION: u32 = 3;

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
    /// Makers whose plug-ins run together in one process (lower case).
    /// None until changed: then a built-in list of makers known to need it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shared_makers: Option<Vec<String>>,
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
    let mut reg = reg;
    if reg.schema_version < SCHEMA_VERSION {
        // Older scans left instruments out or failed: the next scan reads every bundle again.
        reg.stamps.clear();
    }
    dedupe(&mut reg.plugins);
    if let Ok(mut f) = config_file().lock() {
        *f = Some(file);
    }
    instance::set_shared_makers(
        reg.shared_makers
            .clone()
            .unwrap_or_else(instance::default_shared_makers),
    );
    if let Ok(mut r) = registry().write() {
        *r = reg;
    }
}

/// Sets the makers whose plug-ins run together in one process, and saves it.
pub fn set_shared_makers(makers: Vec<String>) -> Registry {
    let mut reg = snapshot();
    reg.shared_makers = Some(makers.clone());
    instance::set_shared_makers(makers);
    save(&reg);
    set(reg.clone());
    reg
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

/// Set to stop a scan in progress (see [`stop_scan`]).
static STOP_SCAN: AtomicBool = AtomicBool::new(false);

/// Stops the scan in progress: running scan processes are ended and no
/// more bundles start. What was read so far is kept.
pub fn stop_scan() {
    STOP_SCAN.store(true, Ordering::SeqCst);
}

/// Bytes a pipe has given so far, filled by a thread of its own.
struct Drained {
    bytes: Arc<Mutex<Vec<u8>>>,
    closed: Arc<AtomicBool>,
}

impl Drained {
    fn start(pipe: Option<Box<dyn std::io::Read + Send>>) -> Drained {
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let closed = Arc::new(AtomicBool::new(pipe.is_none()));
        if let Some(mut p) = pipe {
            let (b, c) = (bytes.clone(), closed.clone());
            std::thread::spawn(move || {
                let mut buf = [0u8; 8192];
                loop {
                    match p.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => match b.lock() {
                            Ok(mut v) => v.extend_from_slice(&buf[..n]),
                            Err(_) => break,
                        },
                    }
                }
                c.store(true, Ordering::SeqCst);
            });
        }
        Drained { bytes, closed }
    }

    fn text(&self) -> String {
        self.bytes
            .lock()
            .map(|v| String::from_utf8_lossy(&v).into_owned())
            .unwrap_or_default()
    }

    /// Whether the scan has printed its last line.
    fn finished(&self) -> bool {
        let done = format!("{SCAN_PREFIX}\"done\"");
        self.bytes
            .lock()
            .map(|v| String::from_utf8_lossy(&v).contains(&done))
            .unwrap_or(false)
    }
}

/// Asks a host process what one bundle holds. None when the scan was stopped.
pub fn scan_bundle(bundle: &Path) -> Option<ScanResult> {
    let fail = |e: String| {
        Some(ScanResult {
            classes: Vec::new(),
            error: Some(e),
        })
    };
    let Some(exe) = host_program() else {
        return fail("The plug-in host could not be found.".into());
    };
    let mut child = match Command::new(exe)
        .arg("--plugin-scan")
        .arg(bundle)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return fail(format!("Could not start the plug-in host: {e}")),
    };
    // Read while it runs: a plug-in with many controls prints more than
    // the pipe holds, and would otherwise wait on us forever. Plug-ins
    // may print text of their own (not always UTF-8); that is skipped.
    let out = Drained::start(child.stdout.take().map(|p| Box::new(p) as _));
    let err = Drained::start(child.stderr.take().map(|p| Box::new(p) as _));
    let start = Instant::now();
    let exit = loop {
        if STOP_SCAN.load(Ordering::SeqCst) {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        // Finished: done, even if the plug-in hangs on the way out.
        if out.finished() {
            let _ = child.kill();
            let _ = child.wait();
            break ScanExit::Code(0);
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                break match status.code() {
                    Some(c) => ScanExit::Code(c),
                    None => ScanExit::Killed,
                }
            }
            Ok(None) if start.elapsed() > SCAN_TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                break ScanExit::TimedOut;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => return fail(e.to_string()),
        }
    };
    // Helper programs a plug-in started can hold its pipes open after it
    // has gone; take what came within a moment rather than wait on them.
    let until = Instant::now() + Duration::from_secs(2);
    while Instant::now() < until
        && !(out.closed.load(Ordering::SeqCst) && err.closed.load(Ordering::SeqCst))
    {
        std::thread::sleep(Duration::from_millis(20));
    }
    Some(parse_scan_output(&out.text(), &err.text(), exit))
}

/// What a scan did.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanReport {
    pub scanned: usize,
    pub found: usize,
    pub failed: usize,
    /// Stopped before the end; the rest are read at the next scan.
    pub stopped: bool,
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
    let todo: Vec<(&PathBuf, u64)> = bundles
        .iter()
        .map(|b| (b, stamp(b)))
        .filter(|(b, s)| {
            // Bundles that failed, or were only partly read, are tried again every time.
            let retry = reg.failed.iter().any(|f| &f.bundle == *b)
                || reg
                    .plugins
                    .iter()
                    .any(|p| &p.bundle == *b && p.info.note.is_some());
            all || retry || reg.stamps.get(*b) != Some(s)
        })
        .collect();
    let total = todo.len();
    STOP_SCAN.store(false, Ordering::SeqCst);
    progress(0, total, Path::new(""));
    // A few bundles at a time: most of a scan is waiting on plug-ins
    // starting up (and their licence checks).
    let next = std::sync::atomic::AtomicUsize::new(0);
    let (tx, rx) = std::sync::mpsc::channel();
    let results: Vec<(usize, ScanResult)> = std::thread::scope(|scope| {
        for _ in 0..SCAN_THREADS.min(total) {
            let (tx, next, todo) = (tx.clone(), &next, &todo);
            scope.spawn(move || loop {
                let i = next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let Some((b, _)) = todo.get(i) else { break };
                if STOP_SCAN.load(Ordering::SeqCst) || tx.send((i, b.as_path(), None)).is_err() {
                    break;
                }
                // A stopped bundle is left unread (and unstamped) for next time.
                let Some(r) = scan_bundle(b) else { break };
                if tx.send((i, b.as_path(), Some(r))).is_err() {
                    break;
                }
            });
        }
        drop(tx);
        let mut done = Vec::new();
        for (i, b, r) in rx {
            match r {
                // Started: show the newest bundle being read.
                None => progress(done.len(), total, b),
                Some(r) => {
                    done.push((i, r));
                    progress(done.len(), total, b);
                }
            }
        }
        done
    });
    report.stopped = STOP_SCAN.swap(false, Ordering::SeqCst);
    let mut results = results;
    results.sort_by_key(|(i, _)| *i);
    for (i, r) in results {
        let (b, s) = todo[i];
        report.scanned += 1;
        reg.plugins.retain(|p| &p.bundle != b);
        reg.failed.retain(|p| &p.bundle != b);
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
    dedupe(&mut reg.plugins);
    reg.plugins.sort_by_key(|p| p.info.name.to_lowercase());
    report.found = reg.plugins.len();
    report.failed = reg.failed.len();
    reg.schema_version = SCHEMA_VERSION;
    save(&reg);
    set(reg);
    report
}

/// One entry per plug-in. The same plug-in is often installed twice (in
/// two scanned folders, or an old copy beside a new one); both have the
/// same class id, so they are the same effect. The fully read copy is
/// kept, else the first. (Two entries with one id broke the interface's
/// lists.)
fn dedupe(plugins: &mut Vec<Found>) {
    plugins.sort_by_key(|p| p.info.note.is_some());
    let mut seen = HashSet::new();
    plugins.retain(|p| seen.insert(p.info.cid.to_ascii_uppercase()));
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

/// Every installed effect plug-in, as effects (instruments are left out:
/// they make sound rather than process it).
pub fn catalog() -> Vec<EffectInfo> {
    snapshot()
        .plugins
        .iter()
        .filter(|p| !p.info.instrument)
        .map(effect_info)
        .collect()
}

/// Every installed instrument, for MIDI tracks.
pub fn instruments() -> Vec<EffectInfo> {
    snapshot()
        .plugins
        .iter()
        .filter(|p| p.info.instrument)
        .map(effect_info)
        .collect()
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

    /// A plug-in with many controls prints more than a pipe holds; the
    /// scan must still read it (it used to wait for the scanner to exit,
    /// which waited for the scan to read: every bundle timed out).
    #[cfg(unix)]
    #[test]
    fn reads_a_big_scan_without_waiting_on_it() {
        use altered_plughost::protocol::{ScanLine, SCAN_PREFIX};
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let class = ClassInfo {
            cid: "00".into(),
            name: "Big".into(),
            vendor: "v".repeat(300_000),
            version: "1".into(),
            sub_categories: "Fx".into(),
            params: vec![],
            latency: 0,
            has_editor: false,
            instrument: false,
            note: None,
        };
        let line = |l: &ScanLine| format!("{SCAN_PREFIX}{}\n", serde_json::to_string(l).unwrap());
        let json = format!(
            "{}{}{}",
            line(&ScanLine::Found(vec![class.clone()])),
            line(&ScanLine::Read(class)),
            line(&ScanLine::Done)
        );
        std::fs::write(dir.path().join("out.json"), json).unwrap();
        let script = dir.path().join("host.sh");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\ncat '{}'\n",
                dir.path().join("out.json").display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::env::set_var("ALTERED_PLUGIN_HOST", &script);
        let start = Instant::now();
        let r = scan_bundle(Path::new("x.vst3")).unwrap();
        assert_eq!(r.error, None);
        assert_eq!(r.classes[0].vendor.len(), 300_000);
        assert!(start.elapsed() < Duration::from_secs(10));

        // The checks below swap the same host setting, so they run here in turn.
        let out = dir.path().join("out.json").display().to_string();
        let run = |body: &str| {
            std::fs::write(&script, format!("#!/bin/sh\n{body}\n")).unwrap();
            let start = Instant::now();
            let r = scan_bundle(Path::new("x.vst3"));
            (r, start.elapsed())
        };

        // A helper program the plug-in started keeps the pipes open after
        // the scan has gone: the scan still ends at once.
        let (r, took) = run(&format!("sleep 30 &\ncat '{out}'"));
        assert_eq!(r.unwrap().classes.len(), 1);
        assert!(took < Duration::from_secs(5), "{took:?}");

        // The plug-in hangs on the way out after the scan finished: done anyway.
        let (r, took) = run(&format!("cat '{out}'\nsleep 30"));
        assert_eq!(r.unwrap().error, None);
        assert!(took < Duration::from_secs(5), "{took:?}");

        // Stop Scan ends a bundle that would take long, with no result.
        let stopper = std::thread::spawn(|| {
            std::thread::sleep(Duration::from_millis(300));
            stop_scan();
        });
        let (r, took) = run("sleep 30");
        stopper.join().unwrap();
        STOP_SCAN.store(false, Ordering::SeqCst);
        assert!(r.is_none());
        assert!(took < Duration::from_secs(5), "{took:?}");
        std::env::remove_var("ALTERED_PLUGIN_HOST");
    }

    #[test]
    fn the_same_plug_in_twice_is_listed_once() {
        let f = |bundle: &str, cid: &str, note: Option<&str>| Found {
            bundle: bundle.into(),
            info: ClassInfo {
                cid: cid.into(),
                name: "Verb".into(),
                vendor: String::new(),
                version: String::new(),
                sub_categories: String::new(),
                params: vec![],
                latency: 0,
                has_editor: true,
                instrument: false,
                note: note.map(Into::into),
            },
        };
        let mut list = vec![
            f("a/Verb.vst3", "00AA", Some("Not fully read.")),
            f("b/Verb.vst3", "00aa", None),
            f("c/Other.vst3", "00BB", None),
        ];
        dedupe(&mut list);
        assert_eq!(
            list.iter()
                .map(|p| p.bundle.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
            ["b/Verb.vst3", "c/Other.vst3"]
        );
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
                instrument: false,
                note: None,
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
