// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

//! VST3 plug-in hosting, in its own process.
//!
//! The editor starts itself again with `--plugin-host` for every plug-in
//! it runs, and with `--plugin-scan <bundle>` to list what a bundle
//! holds. A plug-in that crashes, hangs or misbehaves takes down only that
//! process: the editor sees the pipe close and carries on without it.
//!
//! Threads in a host process:
//! - the main thread loads the plug-in, sets controls, saves settings and
//!   runs the plug-in's own window, as VST3 asks of a host's main thread;
//! - the stdin thread reads requests and processes audio blocks (it is
//!   the plug-in's audio thread), handing everything else to the main thread.
//!
//! See [`protocol`] for what goes over the pipes.

// VST3's C types differ between Windows and other systems (signed or
// unsigned enums, pointer types), so casts that do nothing on one system
// are needed on another.
#![allow(clippy::unnecessary_cast)]

pub mod com;
pub mod editor;
pub mod module;
pub mod plugin;
pub mod protocol;

use std::io::{self, Write};
use std::path::Path;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use com::Edit;
use editor::EditorWindow;
use plugin::{Audio, Plugin, SendPtr};
use protocol::{read_frame, write_frame, Message, Request, ScanLine, AUDIO, CONTROL};

/// Runs a host or a scan when the command line asks for one. Returns the
/// exit code, or None for a normal start of the editor.
pub fn main_with_args(args: &[String]) -> Option<i32> {
    let mode = args.get(1).map(String::as_str);
    if matches!(mode, Some("--plugin-host" | "--plugin-scan")) {
        init_com();
    }
    match mode {
        Some("--plugin-host") => Some(serve()),
        Some("--plugin-scan") => Some(scan(Path::new(args.get(2).map_or("", String::as_str)))),
        _ => None,
    }
}

/// Windows plug-ins expect the host's main thread to have COM and OLE
/// started (for their windows, drag and drop, licence checks and the
/// like), as every Windows host does.
#[cfg(windows)]
fn init_com() {
    // SAFETY: called once, on the main thread, before any plug-in loads.
    unsafe {
        windows_sys::Win32::System::Ole::OleInitialize(std::ptr::null());
    }
}

#[cfg(not(windows))]
fn init_com() {}

type Out = Arc<Mutex<io::Stdout>>;

/// Control changes waiting for the audio thread.
type Queue = Arc<Mutex<Vec<(u32, f64)>>>;

fn send(out: &Out, m: &Message) {
    if let (Ok(json), Ok(mut o)) = (serde_json::to_vec(m), out.lock()) {
        let _ = write_frame(&mut *o, CONTROL, &json);
    }
}

enum ToMain {
    Load {
        req: Request,
        reply: Sender<Option<SendPtr<Audio>>>,
    },
    Request(Request),
    /// Control values the editor set, to show on the plug-in's controller.
    Changes(Vec<(u32, f64)>),
    Quit,
}

/// Host mode: one plug-in, driven over stdin and stdout.
pub fn serve() -> i32 {
    let out: Out = Arc::new(Mutex::new(io::stdout()));
    let (tx, rx) = mpsc::channel::<ToMain>();
    let audio_out = out.clone();
    let reader = thread::Builder::new()
        .name("plugin-audio".into())
        .spawn(move || audio_loop(audio_out, tx));
    main_loop(&out, rx);
    if let Ok(r) = reader {
        let _ = r.join();
    }
    0
}

fn audio_loop(out: Out, tx: Sender<ToMain>) {
    let mut stdin = io::stdin().lock();
    let mut audio: Option<Audio> = None;
    let mut buf = Vec::new();
    let mut changes = Vec::new();
    let mut notes = Vec::new();
    let mut samples = Vec::new();
    let mut reply = Vec::new();
    loop {
        match read_frame(&mut stdin, &mut buf) {
            Ok(Some(AUDIO)) => {
                if protocol::decode_audio(&buf, &mut changes, &mut notes, &mut samples).is_none() {
                    break;
                }
                if let Some(a) = audio.as_mut() {
                    a.process(&changes, &notes, &mut samples);
                }
                if !changes.is_empty() {
                    let _ = tx.send(ToMain::Changes(changes.clone()));
                }
                protocol::encode_samples(&mut reply, &samples);
                let Ok(mut o) = out.lock() else { break };
                if write_frame(&mut *o, AUDIO, &reply).is_err() {
                    break;
                }
            }
            Ok(Some(CONTROL)) => {
                let Ok(req) = serde_json::from_slice::<Request>(&buf) else {
                    send(
                        &out,
                        &Message::Error {
                            message: "Bad request.".into(),
                        },
                    );
                    continue;
                };
                match req {
                    Request::Load { .. } => {
                        let (rtx, rrx) = mpsc::channel();
                        if tx.send(ToMain::Load { req, reply: rtx }).is_err() {
                            break;
                        }
                        audio = rrx.recv().ok().flatten().map(|a| a.0);
                    }
                    Request::GetState => {
                        // Changes still waiting count in the saved settings.
                        if let Some(a) = audio.as_mut() {
                            a.flush(&[]);
                        }
                        if tx.send(ToMain::Request(Request::GetState)).is_err() {
                            break;
                        }
                    }
                    Request::Tempo { bpm, beats_per_bar } => {
                        if let Some(a) = audio.as_mut() {
                            a.set_tempo(bpm, beats_per_bar as i32);
                        }
                        send(&out, &Message::Ok);
                    }
                    Request::Reset => {
                        if let Some(a) = audio.as_mut() {
                            a.reset();
                        }
                        send(&out, &Message::Ok);
                    }
                    other => {
                        if tx.send(ToMain::Request(other)).is_err() {
                            break;
                        }
                    }
                }
            }
            _ => break,
        }
    }
    // Processing stops before the plug-in is torn down on the main thread.
    drop(audio);
    let _ = tx.send(ToMain::Quit);
}

fn main_loop(out: &Out, rx: mpsc::Receiver<ToMain>) {
    let mut plugin: Option<Plugin> = None;
    let mut to_audio: Option<Queue> = None;
    let mut window: Option<Box<dyn EditorWindow>> = None;
    loop {
        if let Some(w) = window.as_mut() {
            if !w.pump() {
                window = None;
                send(out, &Message::EditorClosed);
            }
        }
        if let Some(p) = &plugin {
            let edits: Vec<Edit> = p
                .edits
                .lock()
                .map(|mut e| std::mem::take(&mut *e))
                .unwrap_or_default();
            for e in edits {
                match e {
                    Edit::Param(id, v) => {
                        if let Some(q) = &to_audio {
                            if let Ok(mut q) = q.lock() {
                                q.push((id, v));
                            }
                        }
                        send(out, &Message::ParamEdited { id, value: v });
                    }
                    Edit::Latency => send(
                        out,
                        &Message::LatencyChanged {
                            latency: p.latency_now(),
                        },
                    ),
                    Edit::Dirty => send(out, &Message::Dirty),
                    Edit::Resize(w, h) => {
                        if let Some(win) = window.as_mut() {
                            win.resize(w, h);
                        }
                    }
                }
            }
        }
        let wait = if window.is_some() { 10 } else { 50 };
        let cmd = match rx.recv_timeout(Duration::from_millis(wait)) {
            Ok(c) => c,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => break,
        };
        match cmd {
            ToMain::Load { req, reply } => {
                let Request::Load {
                    path,
                    cid,
                    rate,
                    state,
                    params,
                } = req
                else {
                    let _ = reply.send(None);
                    continue;
                };
                match Plugin::load(Path::new(&path), &cid, rate, state.as_deref()) {
                    Ok((p, mut a)) => {
                        for (id, v) in &params {
                            p.set_param(*id, *v);
                        }
                        a.flush(&params);
                        send(
                            out,
                            &Message::Loaded {
                                latency: p.latency,
                                params: p.params.clone(),
                                has_editor: p.controller.is_some(),
                                instrument: p.instrument,
                            },
                        );
                        to_audio = Some(a.from_editor.clone());
                        plugin = Some(p);
                        let _ = reply.send(Some(SendPtr(a)));
                    }
                    Err(message) => {
                        send(out, &Message::Error { message });
                        let _ = reply.send(None);
                    }
                }
            }
            ToMain::Request(Request::GetState) => match &plugin {
                Some(p) => send(out, &Message::State { data: p.state() }),
                None => send(
                    out,
                    &Message::Error {
                        message: "No plug-in loaded.".into(),
                    },
                ),
            },
            ToMain::Request(Request::OpenEditor { title }) => {
                if let Some(w) = window.as_mut() {
                    w.raise();
                    send(out, &Message::Ok);
                    continue;
                }
                let opened = match plugin
                    .as_ref()
                    .and_then(|p| p.controller.as_ref().map(|c| (c, p.edits.clone())))
                {
                    Some((c, edits)) => editor::open(c, &title, edits),
                    None => Err("This plug-in has no window of its own.".into()),
                };
                match opened {
                    Ok(w) => {
                        window = Some(w);
                        send(out, &Message::Ok);
                    }
                    Err(message) => send(out, &Message::Error { message }),
                }
            }
            ToMain::Request(Request::CloseEditor) => {
                window = None;
                send(out, &Message::Ok);
            }
            ToMain::Request(_) => send(out, &Message::Ok),
            ToMain::Changes(list) => {
                if let Some(p) = &plugin {
                    for (id, v) in list {
                        p.set_param(id, v);
                    }
                }
            }
            ToMain::Quit => break,
        }
    }
    drop(window);
    drop(plugin);
    let _ = io::stdout().flush();
}

/// Says on stderr which step a plug-in is at, so a crash can be told
/// apart from a hang and placed (see [`protocol::parse_scan_output`]).
pub fn stage(what: &str) {
    let mut e = io::stderr();
    let _ = writeln!(e, "{}{what}", protocol::STAGE_PREFIX);
    let _ = e.flush();
}

fn scan_line(line: &ScanLine) {
    let mut o = io::stdout();
    let _ = writeln!(
        o,
        "{}{}",
        protocol::SCAN_PREFIX,
        serde_json::to_string(line).unwrap_or_default()
    );
    let _ = o.flush();
}

/// Scan mode: prints what the bundle holds, one [`ScanLine`] at a time:
/// the plug-ins by name first, then each one as it is read, so a crash
/// part way loses only what it had not reached.
pub fn scan(bundle: &Path) -> i32 {
    stage("loading its file");
    let module = match module::Module::open(bundle) {
        Ok(m) => Arc::new(m),
        Err(e) => {
            scan_line(&ScanLine::Error(e));
            scan_line(&ScanLine::Done);
            std::process::exit(0)
        }
    };
    stage("listing what is inside");
    let classes: Vec<_> = module
        .classes()
        .into_iter()
        .filter(|c| c.category == module::AUDIO_CLASS)
        .collect();
    let named = |subs: &str| {
        let s = subs.to_ascii_lowercase();
        s.contains("instrument") && !s.contains("fx")
    };
    scan_line(&ScanLine::Found(
        classes
            .iter()
            .map(|c| protocol::ClassInfo {
                cid: module::cid_hex(&c.cid),
                name: c.name.clone(),
                vendor: c.vendor.clone(),
                version: c.version.clone(),
                sub_categories: c.sub_categories.clone(),
                params: Vec::new(),
                latency: 0,
                // Unknown until it runs; the window button says so if there is none.
                has_editor: true,
                instrument: named(&c.sub_categories),
                note: None,
            })
            .collect(),
    ));
    // Plug-ins stay alive until the process ends: closing one can take
    // the bundle's shared code down with it in some plug-ins.
    let mut keep = Vec::new();
    for c in classes {
        let cid = module::cid_hex(&c.cid);
        stage(&format!("loading {}", c.name));
        match Plugin::load_from(module.clone(), &cid, 48_000.0, None) {
            Ok((p, a)) => {
                scan_line(&ScanLine::Read(protocol::ClassInfo {
                    cid,
                    instrument: named(&c.sub_categories) || p.instrument,
                    name: c.name,
                    vendor: c.vendor,
                    version: c.version,
                    sub_categories: c.sub_categories,
                    params: p.params.clone(),
                    latency: p.latency,
                    has_editor: p.controller.is_some(),
                    note: None,
                }));
                keep.push((p, a));
            }
            Err(e) => scan_line(&ScanLine::Failed {
                cid,
                error: format!("{}: {e}", c.name),
            }),
        }
    }
    scan_line(&ScanLine::Done);
    std::mem::forget(keep);
    // Plug-ins may leave threads behind; leave without waiting for them.
    std::process::exit(0)
}
