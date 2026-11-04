// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

//! Add-ons: folders the user drops into `addons` in the app config folder.
//! Each holds an `addon.json` manifest, a panel page and, if it does
//! work, a tool program. Audiform shows the page in its own window (a
//! frame on the `addon` scheme, served from here, with no network and no
//! access to the rest of the app) and runs the tool as a child process
//! when the page asks. The tool's output lines go out as `addon-output`
//! events: `{ id, line, err }`. One tool runs at a time.
//!
//! Nothing here is downloaded or shipped: whoever has an add-on folder
//! installs it by hand. See `docs/addons.md`.

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

/// Set to stop the running tool.
#[derive(Default)]
pub struct AddonCancel(pub Arc<AtomicBool>);

/// True while a tool runs: one at a time, whatever the pages ask.
#[derive(Default)]
pub struct AddonBusy(pub Arc<AtomicBool>);

/// Clears the busy flag when a run ends, however it ends.
struct BusyGuard(Arc<AtomicBool>);

impl Drop for BusyGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Relaxed);
    }
}

/// A page or picture in an add-on larger than this is not served.
const MAX_SERVED_BYTES: u64 = 32 * 1024 * 1024;
/// A manifest larger than this is ignored.
const MAX_MANIFEST_BYTES: u64 = 64 * 1024;
/// More add-ons than this are ignored.
const MAX_ADDONS: usize = 32;
/// Output lines kept for the caller once the tool has finished.
const KEPT_LINES: usize = 200;

/// What the frame may do: run scripts and styles from its own folder (or
/// inline), show its own pictures and sounds, and nothing else. No
/// network, no forms, no frames of its own.
const FRAME_CSP: &str = "default-src 'none'; \
    script-src 'self' 'unsafe-inline' addon: http://addon.localhost; \
    style-src 'self' 'unsafe-inline' addon: http://addon.localhost; \
    img-src 'self' data: blob: addon: http://addon.localhost; \
    font-src 'self' data: addon: http://addon.localhost; \
    media-src 'self' blob: addon: http://addon.localhost; \
    form-action 'none'; frame-src 'none'; base-uri 'none'";

/// `addon.json` as written by the add-on's author.
#[derive(Debug, Deserialize)]
struct Manifest {
    id: String,
    name: String,
    #[serde(default)]
    version: String,
    /// The window title. The name when left out.
    #[serde(default)]
    title: String,
    /// The page shown in the window, relative to the folder.
    #[serde(default = "default_entry")]
    entry: String,
    /// The tool program and its fixed arguments, relative to the folder
    /// or found on the system path. Empty when the add-on has no tool.
    #[serde(default)]
    command: Vec<String>,
    #[serde(default)]
    description: String,
}

fn default_entry() -> String {
    "panel.html".into()
}

/// One installed add-on, as the interface sees it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AddonInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub title: String,
    pub entry: String,
    pub description: String,
    pub has_tool: bool,
    pub folder: PathBuf,
}

/// What the tool said and how it ended.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddonRunResult {
    /// The exit code, or null when the tool was stopped by a signal.
    pub code: Option<i32>,
    /// The last output lines, oldest first.
    pub lines: Vec<String>,
}

#[derive(Serialize, Clone)]
struct OutputLine<'a> {
    id: &'a str,
    line: String,
    err: bool,
}

/// Ids are folder names: short, lower case letters, digits and dashes.
fn valid_id(id: &str) -> bool {
    let bytes = id.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 40
        && bytes[0].is_ascii_alphanumeric()
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
}

/// A relative path inside an add-on: plain components only, no `..`,
/// no root, no drive.
fn safe_relative(rel: &str) -> Option<PathBuf> {
    if rel.is_empty() || rel.contains('\\') || rel.contains(':') {
        return None;
    }
    let mut out = PathBuf::new();
    for c in Path::new(rel).components() {
        match c {
            Component::Normal(n) => out.push(n),
            Component::CurDir => {}
            _ => return None,
        }
    }
    (!out.as_os_str().is_empty()).then_some(out)
}

fn read_manifest(folder: &Path) -> Option<Manifest> {
    let path = folder.join("addon.json");
    let meta = fs::metadata(&path).ok()?;
    if !meta.is_file() || meta.len() > MAX_MANIFEST_BYTES {
        return None;
    }
    let text = fs::read_to_string(&path).ok()?;
    serde_json::from_str(&text).ok()
}

/// The add-ons directly inside `dir`, by name. A missing folder is an empty list.
fn list_in(dir: &Path) -> Result<Vec<AddonInfo>, String> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("Could not read the Add-ons folder: {e}")),
    };
    let mut found: Vec<AddonInfo> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| {
            let folder = entry.path();
            let folder_name = entry.file_name().into_string().ok()?;
            let m = read_manifest(&folder)?;
            // The folder name is the id, so two copies can never clash.
            if m.id != folder_name || !valid_id(&m.id) || m.name.trim().is_empty() {
                return None;
            }
            safe_relative(&m.entry)?;
            Some(AddonInfo {
                id: m.id,
                title: if m.title.trim().is_empty() {
                    m.name.trim().to_string()
                } else {
                    m.title.trim().to_string()
                },
                name: m.name.trim().to_string(),
                version: m.version,
                entry: m.entry,
                description: m.description,
                has_tool: !m.command.is_empty(),
                folder,
            })
        })
        .collect();
    found.sort_by_key(|a| a.name.to_lowercase());
    found.truncate(MAX_ADDONS);
    Ok(found)
}

fn addons_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|dir| dir.join("addons"))
        .map_err(|e| e.to_string())
}

/// The Add-ons folder, made if it is not there yet.
#[tauri::command]
pub fn addons_dir(app: AppHandle) -> Result<PathBuf, String> {
    let dir = addons_path(&app)?;
    fs::create_dir_all(&dir).map_err(|e| format!("Could not make the Add-ons folder: {e}"))?;
    Ok(dir)
}

/// Opens the Add-ons folder in the system file manager.
#[tauri::command]
pub fn addons_open_dir(app: AppHandle) -> Result<(), String> {
    let dir = addons_dir(app)?;
    tauri_plugin_opener::open_path(&dir, None::<&str>).map_err(|e| e.to_string())
}

/// Opens a folder inside the add-on's own folder in the system file
/// manager, making it first if needed. `rel` is relative to the add-on;
/// nothing outside it can be opened.
#[tauri::command]
pub fn addon_open_folder(app: AppHandle, id: String, rel: String) -> Result<(), String> {
    if !valid_id(&id) {
        return Err("Not an add-on id.".into());
    }
    let dir = addons_path(&app)?.join(&id);
    if !dir.is_dir() {
        return Err(format!("No add-on called {id} is installed."));
    }
    let path = match rel.trim() {
        "" | "." => dir,
        r => dir.join(safe_relative(r).ok_or("Not a folder in the add-on.")?),
    };
    fs::create_dir_all(&path).map_err(|e| format!("Could not make the folder: {e}"))?;
    tauri_plugin_opener::open_path(&path, None::<&str>).map_err(|e| e.to_string())
}

/// The installed add-ons.
#[tauri::command]
pub fn addons_list(app: AppHandle) -> Result<Vec<AddonInfo>, String> {
    list_in(&addons_path(&app)?)
}

/// Stops the running tool, if any.
#[tauri::command]
pub fn addon_cancel(cancel: State<'_, AddonCancel>) {
    cancel.0.store(true, Ordering::Relaxed);
}

/// Shows an Open dialog for the add-on's page: one file or a folder. Null on cancel.
#[tauri::command]
pub async fn addon_pick(
    app: AppHandle,
    folder: bool,
    title: String,
) -> Result<Option<PathBuf>, String> {
    let parent = app.get_webview_window("main");
    tauri::async_runtime::spawn_blocking(move || {
        let mut dialog = app.dialog().file().set_title(if title.trim().is_empty() {
            "Choose".to_string()
        } else {
            title
        });
        if let Some(w) = parent.as_ref() {
            dialog = dialog.set_parent(w);
        }
        if folder {
            dialog.blocking_pick_folder()
        } else {
            dialog.blocking_pick_file()
        }
        .and_then(|f| f.as_path().map(Path::to_path_buf))
    })
    .await
    .map_err(|e| e.to_string())
}

/// Runs the add-on's tool with `args` after its fixed arguments, `input`
/// on its standard input, and the add-on folder as its working folder.
/// Output lines go out as `addon-output` events while it runs.
#[tauri::command]
pub async fn addon_run(
    app: AppHandle,
    cancel: State<'_, AddonCancel>,
    busy: State<'_, AddonBusy>,
    id: String,
    args: Vec<String>,
    input: String,
) -> Result<AddonRunResult, String> {
    let dir = addons_path(&app)?;
    let info = list_in(&dir)?
        .into_iter()
        .find(|a| a.id == id)
        .ok_or_else(|| format!("No add-on called {id} is installed."))?;
    let manifest = read_manifest(&info.folder).ok_or("The add-on's manifest could not be read.")?;
    if manifest.command.is_empty() {
        return Err(format!("{} has no tool to run.", info.name));
    }
    if busy.0.swap(true, Ordering::AcqRel) {
        return Err("An add-on tool is already running. Wait for it to finish or stop it.".into());
    }
    let guard = BusyGuard(busy.0.clone());
    // A stop asked for before this run does not apply to it.
    cancel.0.store(false, Ordering::Relaxed);
    let flag = cancel.0.clone();
    let folder = info.folder.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = guard;
        run_tool(&app, &folder, &id, &manifest.command, &args, &input, &flag)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The program to start: a bare name is found on the system path; a
/// path is taken relative to the add-on folder.
fn program_path(folder: &Path, program: &str) -> PathBuf {
    let p = Path::new(program);
    if p.is_absolute() || !program.contains(['/', '\\']) {
        p.to_path_buf()
    } else {
        folder.join(p)
    }
}

fn run_tool(
    app: &AppHandle,
    folder: &Path,
    id: &str,
    command: &[String],
    args: &[String],
    input: &str,
    cancel: &AtomicBool,
) -> Result<AddonRunResult, String> {
    let mut cmd = Command::new(program_path(folder, &command[0]));
    cmd.args(&command[1..])
        .args(args)
        .current_dir(folder)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Could not start {}: {e}", command[0]))?;
    if let Some(mut stdin) = child.stdin.take() {
        let text = input.to_string();
        // Written on its own thread: a tool that never reads must not block us.
        std::thread::spawn(move || {
            let _ = stdin.write_all(text.as_bytes());
        });
    }
    let (tx, rx) = mpsc::channel::<(String, bool)>();
    if let Some(out) = child.stdout.take() {
        let tx = tx.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                if tx.send((line, false)).is_err() {
                    break;
                }
            }
        });
    }
    if let Some(err) = child.stderr.take() {
        std::thread::spawn(move || {
            for line in BufReader::new(err).lines().map_while(Result::ok) {
                if tx.send((line, true)).is_err() {
                    break;
                }
            }
        });
    } else {
        drop(tx);
    }
    let mut kept: Vec<String> = Vec::new();
    loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Stopped.".into());
        }
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok((line, err)) => {
                if !err {
                    if kept.len() == KEPT_LINES {
                        kept.remove(0);
                    }
                    kept.push(line.clone());
                }
                let _ = app.emit("addon-output", OutputLine { id, line, err });
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                let status = child.wait().map_err(|e| e.to_string())?;
                return Ok(AddonRunResult {
                    code: status.code(),
                    lines: kept,
                });
            }
        }
    }
}

/// Undoes `%xx` escapes. Bad escapes are kept as they are.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = &s[i + 1..i + 3];
            if let Ok(v) = u8::from_str_radix(hex, 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn mime_of(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("html" | "htm") => "text/html; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        Some("wav") => "audio/wav",
        Some("mp3") => "audio/mpeg",
        Some("txt" | "md") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// The file an `addon` scheme path names: `/<id>/<file in the add-on>`.
fn served_file(dir: &Path, uri_path: &str) -> Result<PathBuf, String> {
    let decoded = percent_decode(uri_path);
    let rest = decoded.trim_start_matches('/');
    let (id, rel) = rest.split_once('/').ok_or("No file named.")?;
    if !valid_id(id) {
        return Err("Not an add-on id.".into());
    }
    let rel = safe_relative(rel).ok_or("Not a file in the add-on.")?;
    let path = dir.join(id).join(rel);
    let meta = fs::metadata(&path).map_err(|_| "Not found.".to_string())?;
    if !meta.is_file() {
        return Err("Not a file.".into());
    }
    if meta.len() > MAX_SERVED_BYTES {
        return Err("Too large.".into());
    }
    Ok(path)
}

fn plain(status: u16, text: &str) -> tauri::http::Response<Vec<u8>> {
    tauri::http::Response::builder()
        .status(status)
        .header("Content-Type", "text/plain; charset=utf-8")
        .header("Content-Security-Policy", FRAME_CSP)
        .body(text.as_bytes().to_vec())
        .unwrap_or_default()
}

/// Answers a request on the `addon` scheme with a file from an add-on's
/// folder, under the frame's content security policy.
pub fn serve(
    app: &AppHandle,
    request: tauri::http::Request<Vec<u8>>,
) -> tauri::http::Response<Vec<u8>> {
    let dir = match addons_path(app) {
        Ok(d) => d,
        Err(e) => return plain(500, &e),
    };
    let path = match served_file(&dir, request.uri().path()) {
        Ok(p) => p,
        Err(e) => return plain(404, &e),
    };
    match fs::read(&path) {
        Ok(bytes) => tauri::http::Response::builder()
            .status(200)
            .header("Content-Type", mime_of(&path))
            .header("Content-Security-Policy", FRAME_CSP)
            .header("Cache-Control", "no-store")
            .body(bytes)
            .unwrap_or_else(|_| plain(500, "Could not build the response.")),
        Err(e) => plain(500, &format!("Could not read the file: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make(dir: &Path, id: &str, manifest: &str) {
        let folder = dir.join(id);
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("addon.json"), manifest).unwrap();
        fs::write(folder.join("panel.html"), "<p>hi</p>").unwrap();
    }

    #[test]
    fn lists_add_ons_whose_folder_matches_their_id() {
        let dir = tempfile::tempdir().unwrap();
        make(
            dir.path(),
            "voice-tool",
            r#"{"id":"voice-tool","name":"Voice Tool","command":["python","run.py"]}"#,
        );
        make(dir.path(), "other", r#"{"id":"mismatch","name":"Other"}"#);
        make(dir.path(), "Bad Name", r#"{"id":"Bad Name","name":"Bad"}"#);
        make(
            dir.path(),
            "escape",
            r#"{"id":"escape","name":"Escape","entry":"../x.html"}"#,
        );
        make(
            dir.path(),
            "plain",
            r#"{"id":"plain","name":"Plain","title":"A Plain Window"}"#,
        );
        fs::write(dir.path().join("notes.txt"), "x").unwrap();
        let list = list_in(dir.path()).unwrap();
        let ids: Vec<_> = list.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, ["plain", "voice-tool"]);
        assert!(list[1].has_tool);
        assert!(!list[0].has_tool);
        assert_eq!(list[0].title, "A Plain Window");
        assert_eq!(list[1].title, "Voice Tool");
        assert_eq!(list[1].entry, "panel.html");
    }

    #[test]
    fn a_missing_folder_is_an_empty_list() {
        let dir = tempfile::tempdir().unwrap();
        assert!(list_in(&dir.path().join("nope")).unwrap().is_empty());
    }

    #[test]
    fn serves_only_files_inside_an_add_on() {
        let dir = tempfile::tempdir().unwrap();
        make(
            dir.path(),
            "voice-tool",
            r#"{"id":"voice-tool","name":"Voice Tool"}"#,
        );
        fs::write(dir.path().join("secret.txt"), "x").unwrap();
        assert!(served_file(dir.path(), "/voice-tool/panel.html").is_ok());
        assert!(served_file(dir.path(), "/voice-tool/panel%2Ehtml").is_ok());
        assert!(served_file(dir.path(), "/voice-tool/../secret.txt").is_err());
        assert!(served_file(dir.path(), "/voice-tool/%2E%2E/secret.txt").is_err());
        assert!(served_file(dir.path(), "/secret.txt").is_err());
        assert!(served_file(dir.path(), "/voice-tool/").is_err());
        assert!(served_file(dir.path(), "/voice-tool/addon.json").is_ok());
        assert!(served_file(dir.path(), "/voice-tool/missing.css").is_err());
    }

    #[test]
    fn decodes_percent_escapes() {
        assert_eq!(percent_decode("a%20b%2Fc"), "a b/c");
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%zz"), "%zz");
    }

    #[test]
    fn relative_paths_stay_inside() {
        assert_eq!(safe_relative("a/b.html"), Some(PathBuf::from("a/b.html")));
        assert_eq!(safe_relative("./a.html"), Some(PathBuf::from("a.html")));
        assert!(safe_relative("../a.html").is_none());
        assert!(safe_relative("/a.html").is_none());
        assert!(safe_relative("a\\b.html").is_none());
        assert!(safe_relative("C:/a.html").is_none());
        assert!(safe_relative("").is_none());
    }

    #[test]
    fn bare_programs_use_the_path_and_relative_ones_the_folder() {
        let folder = Path::new("/addons/x");
        assert_eq!(program_path(folder, "python"), PathBuf::from("python"));
        assert_eq!(program_path(folder, "./run.sh"), folder.join("./run.sh"));
        assert_eq!(program_path(folder, "bin/tool"), folder.join("bin/tool"));
    }

    #[test]
    fn mime_types_cover_pages_and_pictures() {
        assert!(mime_of(Path::new("a.HTML")).starts_with("text/html"));
        assert_eq!(mime_of(Path::new("a.png")), "image/png");
        assert_eq!(mime_of(Path::new("a.bin")), "application/octet-stream");
    }
}
