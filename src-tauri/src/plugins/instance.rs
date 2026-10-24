// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

//! Running plug-ins.
//!
//! [`Host`] is one plug-in in its own host process, driven over pipes.
//! Playback keeps one host per chain slot in a pool, so the plug-in's
//! window, settings and sound carry on across stops, starts and chain
//! edits. Mixdowns get fresh hosts of their own (see [`fresh`]), loaded
//! with the slot's saved settings, so an export never disturbs playback.
//!
//! When a host crashes, hangs (no answer within [`PROCESS_TIMEOUT`]) or
//! fails to start, its slot passes audio through untouched and reports
//! the problem; the editor and every other plug-in carry on.

use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::Duration;

use altered_plughost::protocol::{
    decode_samples, encode_audio, read_frame, write_frame, Message, ParamInfo, Request, AUDIO,
    CONTROL,
};
use serde::Serialize;

use super::{find, host_program, param_id, Found};
use crate::dsp::effects::{Params, Processor};
use crate::session::model::{EffectSlot, Id};

/// Longest a plug-in may take to process one block before it counts as hung.
pub const PROCESS_TIMEOUT: Duration = Duration::from_secs(4);

/// Longest a plug-in may take to load (licence checks can be slow).
const LOAD_TIMEOUT: Duration = Duration::from_secs(90);

/// Longest a plug-in may take to answer a request (settings, its window).
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

/// What a plug-in reported since the last look, for the interface.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Events {
    /// Controls moved in the plug-in's own window: (parameter key, value).
    pub edits: Vec<(String, f64)>,
    /// Settings changed that are not controls; save its state to keep them.
    pub dirty: bool,
    pub window_closed: bool,
    /// Its look-ahead changed; playback must re-align.
    pub latency_changed: bool,
    /// It crashed or stopped answering, and is bypassed.
    pub stopped: Option<String>,
}

impl Events {
    fn is_empty(&self) -> bool {
        *self == Events::default()
    }
}

pub struct Host {
    child: Child,
    stdin: ChildStdin,
    audio: Receiver<Vec<f32>>,
    replies: Receiver<Message>,
    events: Arc<Mutex<Events>>,
    dead: Arc<AtomicBool>,
    /// The control values the plug-in has: sent from here or moved in its window.
    known: Arc<Mutex<HashMap<u32, f64>>>,
    pub latency: usize,
    pub params: Vec<ParamInfo>,
    pub has_editor: bool,
    buf: Vec<u8>,
}

impl Host {
    /// Starts a host and loads `found` into it.
    pub fn start(
        found: &Found,
        rate: u32,
        state: Option<String>,
        params: Vec<(u32, f64)>,
    ) -> Result<Host, String> {
        let exe = host_program().ok_or("The plug-in host could not be found.")?;
        let mut child = Command::new(exe)
            .arg("--plugin-host")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Could not start the plug-in host: {e}"))?;
        let stdin = child.stdin.take().ok_or("No pipe to the plug-in host.")?;
        let mut stdout = child
            .stdout
            .take()
            .ok_or("No pipe from the plug-in host.")?;
        let (atx, arx) = mpsc::channel();
        let (rtx, rrx) = mpsc::channel();
        let events = Arc::new(Mutex::new(Events::default()));
        let dead = Arc::new(AtomicBool::new(false));
        let known = Arc::new(Mutex::new(
            params.iter().copied().collect::<HashMap<u32, f64>>(),
        ));
        let (ev, dd, kn) = (events.clone(), dead.clone(), known.clone());
        thread::Builder::new()
            .name("plugin-pipe".into())
            .spawn(move || {
                let mut buf = Vec::new();
                loop {
                    match read_frame(&mut stdout, &mut buf) {
                        Ok(Some(AUDIO)) => {
                            let mut s = Vec::new();
                            decode_samples(&buf, &mut s);
                            if atx.send(s).is_err() {
                                break;
                            }
                        }
                        Ok(Some(CONTROL)) => {
                            let Ok(m) = serde_json::from_slice::<Message>(&buf) else {
                                continue;
                            };
                            let Ok(mut e) = ev.lock() else { break };
                            match m {
                                Message::ParamEdited { id, value } => {
                                    if let Ok(mut k) = kn.lock() {
                                        k.insert(id, value);
                                    }
                                    e.edits.push((super::param_key(id), value));
                                }
                                Message::Dirty => e.dirty = true,
                                Message::EditorClosed => {
                                    e.window_closed = true;
                                    e.dirty = true;
                                }
                                Message::LatencyChanged { .. } => e.latency_changed = true,
                                reply => {
                                    drop(e);
                                    if rtx.send(reply).is_err() {
                                        break;
                                    }
                                }
                            }
                        }
                        _ => break,
                    }
                }
                if !dd.swap(true, Ordering::SeqCst) {
                    if let Ok(mut e) = ev.lock() {
                        e.stopped
                            .get_or_insert_with(|| "The plug-in crashed and is bypassed.".into());
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        let mut host = Host {
            child,
            stdin,
            audio: arx,
            replies: rrx,
            events,
            dead,
            known,
            latency: 0,
            params: Vec::new(),
            has_editor: false,
            buf: Vec::new(),
        };
        let load = Request::Load {
            path: found.bundle.to_string_lossy().into_owned(),
            cid: found.info.cid.clone(),
            rate: f64::from(rate),
            state,
            params,
        };
        match host.request(&load, LOAD_TIMEOUT) {
            Some(Message::Loaded {
                latency,
                params,
                has_editor,
            }) => {
                host.latency = latency as usize;
                host.params = params;
                host.has_editor = has_editor;
                Ok(host)
            }
            Some(Message::Error { message }) => Err(message),
            _ => Err(format!("{} did not load.", found.info.name)),
        }
    }

    pub fn is_dead(&self) -> bool {
        self.dead.load(Ordering::SeqCst)
    }

    fn stop(&mut self, why: &str) {
        if !self.dead.swap(true, Ordering::SeqCst) {
            if let Ok(mut e) = self.events.lock() {
                e.stopped.get_or_insert_with(|| why.to_string());
            }
        }
        let _ = self.child.kill();
    }

    /// Sends a request and waits for its reply.
    pub fn request(&mut self, r: &Request, timeout: Duration) -> Option<Message> {
        if self.is_dead() {
            return None;
        }
        let json = serde_json::to_vec(r).ok()?;
        if write_frame(&mut self.stdin, CONTROL, &json).is_err() {
            self.stop("The plug-in crashed and is bypassed.");
            return None;
        }
        match self.replies.recv_timeout(timeout) {
            Ok(m) => Some(m),
            Err(_) => {
                self.stop("The plug-in stopped answering and is bypassed.");
                None
            }
        }
    }

    /// Processes interleaved stereo in place. False (and `io` untouched)
    /// when the plug-in is gone.
    pub fn process(&mut self, changes: &[(u32, f64)], io: &mut [f32]) -> bool {
        if self.is_dead() || io.is_empty() {
            return false;
        }
        if !changes.is_empty() {
            if let Ok(mut k) = self.known.lock() {
                k.extend(changes.iter().copied());
            }
        }
        encode_audio(&mut self.buf, changes, io);
        if write_frame(&mut self.stdin, AUDIO, &self.buf).is_err() {
            self.stop("The plug-in crashed and is bypassed.");
            return false;
        }
        match self.audio.recv_timeout(PROCESS_TIMEOUT) {
            Ok(out) if out.len() == io.len() => {
                io.copy_from_slice(&out);
                true
            }
            Ok(_) => false,
            Err(RecvTimeoutError::Timeout) => {
                self.stop("The plug-in stopped answering and is bypassed.");
                false
            }
            Err(RecvTimeoutError::Disconnected) => false,
        }
    }

    /// The values in `want` the plug-in does not have yet.
    pub fn differing(&self, want: &[(u32, f64)]) -> Vec<(u32, f64)> {
        let Ok(k) = self.known.lock() else {
            return want.to_vec();
        };
        want.iter()
            .filter(|(id, v)| k.get(id) != Some(v))
            .copied()
            .collect()
    }

    pub fn take_events(&self) -> Events {
        self.events
            .lock()
            .map(|mut e| std::mem::take(&mut *e))
            .unwrap_or_default()
    }

    pub fn state(&mut self) -> Option<String> {
        match self.request(&Request::GetState, REQUEST_TIMEOUT)? {
            Message::State { data } => Some(data),
            _ => None,
        }
    }

    pub fn open_window(&mut self, title: &str) -> Result<(), String> {
        match self.request(
            &Request::OpenEditor {
                title: title.into(),
            },
            REQUEST_TIMEOUT,
        ) {
            Some(Message::Ok) => Ok(()),
            Some(Message::Error { message }) => Err(message),
            _ => Err("The plug-in did not open its window.".into()),
        }
    }

    pub fn close_window(&mut self) {
        let _ = self.request(&Request::CloseEditor, REQUEST_TIMEOUT);
    }

    pub fn reset(&mut self) {
        let _ = self.request(&Request::Reset, REQUEST_TIMEOUT);
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.stdin.flush();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct Pooled {
    effect: String,
    rate: u32,
    host: Arc<Mutex<Host>>,
}

fn pool() -> &'static Mutex<HashMap<Id, Pooled>> {
    static P: OnceLock<Mutex<HashMap<Id, Pooled>>> = OnceLock::new();
    P.get_or_init(|| Mutex::new(HashMap::new()))
}

thread_local! {
    static FRESH: Cell<bool> = const { Cell::new(false) };
}

/// Runs `f` with fresh plug-in hosts instead of playback's (a mixdown).
pub fn fresh<T>(f: impl FnOnce() -> T) -> T {
    let was = FRESH.with(|c| c.replace(true));
    let out = f();
    FRESH.with(|c| c.set(was));
    out
}

fn slot_params(slot: &EffectSlot) -> Vec<(u32, f64)> {
    slot.params
        .iter()
        .filter_map(|(k, v)| param_id(k).map(|id| (id, v.clamp(0.0, 1.0))))
        .collect()
}

/// The host for a slot: playback's pooled one, started if needed, or a
/// fresh one inside [`fresh`]. Errors are reported on the slot.
pub fn host_for(slot: &EffectSlot, rate: u32) -> Option<Arc<Mutex<Host>>> {
    let found = find(&slot.effect)?;
    let fresh = FRESH.with(|c| c.get());
    if !fresh {
        if let Ok(p) = pool().lock() {
            if let Some(e) = p.get(&slot.id) {
                if e.effect == slot.effect && e.rate == rate {
                    return Some(e.host.clone());
                }
            }
        }
    }
    let host = match Host::start(&found, rate, slot.plugin_state.clone(), slot_params(slot)) {
        Ok(h) => Arc::new(Mutex::new(h)),
        Err(e) => {
            report_failure(slot.id, e);
            return None;
        }
    };
    if !fresh {
        if let Ok(mut p) = pool().lock() {
            p.insert(
                slot.id,
                Pooled {
                    effect: slot.effect.clone(),
                    rate,
                    host: host.clone(),
                },
            );
        }
    }
    Some(host)
}

fn failures() -> &'static Mutex<HashMap<Id, String>> {
    static F: OnceLock<Mutex<HashMap<Id, String>>> = OnceLock::new();
    F.get_or_init(|| Mutex::new(HashMap::new()))
}

fn report_failure(slot: Id, why: String) {
    if let Ok(mut f) = failures().lock() {
        f.insert(slot, why);
    }
}

/// A slot's playback host, if one is running.
pub fn running(slot: Id) -> Option<Arc<Mutex<Host>>> {
    pool().lock().ok()?.get(&slot).map(|e| e.host.clone())
}

/// A slot's look-ahead: its running host's, or the scan's.
pub fn latency(slot: &EffectSlot) -> usize {
    if let Some(h) = running(slot.id) {
        if let Ok(h) = h.lock() {
            return h.latency;
        }
    }
    super::scanned_latency(&slot.effect)
}

/// Stops the hosts of slots no longer in the session.
pub fn retain(slots: &HashSet<Id>) {
    if let Ok(mut p) = pool().lock() {
        p.retain(|id, _| slots.contains(id));
    }
}

/// Stops a slot's host, so the next playback starts it again (after a crash).
pub fn restart(slot: Id) {
    if let Ok(mut p) = pool().lock() {
        p.remove(&slot);
    }
    if let Ok(mut f) = failures().lock() {
        f.remove(&slot);
    }
}

/// Everything plug-ins reported since the last call, by slot.
pub fn take_events() -> Vec<(Id, Events)> {
    let mut out: Vec<(Id, Events)> = Vec::new();
    let hosts: Vec<(Id, Arc<Mutex<Host>>)> = pool()
        .lock()
        .map(|p| p.iter().map(|(id, e)| (*id, e.host.clone())).collect())
        .unwrap_or_default();
    for (id, h) in hosts {
        // A host busy processing is checked next time.
        if let Ok(h) = h.try_lock() {
            let e = h.take_events();
            if !e.is_empty() {
                out.push((id, e));
            }
        }
    }
    if let Ok(mut f) = failures().lock() {
        for (id, why) in f.drain() {
            match out.iter_mut().find(|(i, _)| *i == id) {
                Some((_, e)) => e.stopped = Some(why),
                None => out.push((
                    id,
                    Events {
                        stopped: Some(why),
                        ..Events::default()
                    },
                )),
            }
        }
    }
    out
}

/// A plug-in in a Voice Chain. Several chain builds (after edits) may
/// share one running host; each sends only the values it lacks.
pub struct PluginProcessor {
    host: Arc<Mutex<Host>>,
    latency: usize,
    pending: Vec<(u32, f64)>,
    /// When the plug-in has stopped, the dry sound still arrives as late
    /// as the plug-in's would have, so the track stays in time.
    bypass: crate::dsp::effects::Delay,
}

/// The processor for a plug-in slot, or None when it is not installed or
/// would not start (the chain then skips it).
pub fn processor(slot: &EffectSlot, rate: u32) -> Option<PluginProcessor> {
    let host = host_for(slot, rate)?;
    let (latency, pending) = {
        let h = host.lock().ok()?;
        (h.latency, h.differing(&slot_params(slot)))
    };
    Some(PluginProcessor {
        host,
        latency,
        pending,
        bypass: crate::dsp::effects::Delay::new(latency),
    })
}

impl Processor for PluginProcessor {
    fn process(&mut self, buf: &mut [f32]) {
        let done = match self.host.lock() {
            Ok(mut h) => h.process(&self.pending, buf),
            Err(_) => false,
        };
        if done {
            self.pending.clear();
        } else {
            self.bypass.run(buf);
        }
    }

    fn latency(&self) -> usize {
        self.latency
    }

    fn reset(&mut self) {
        if let Ok(mut h) = self.host.lock() {
            h.reset();
        }
    }

    fn update(&mut self, p: &Params) -> bool {
        let want: Vec<(u32, f64)> = p
            .values()
            .filter_map(|(k, v)| param_id(k).map(|id| (id, v)))
            .collect();
        if let Ok(h) = self.host.lock() {
            for c in h.differing(&want) {
                self.pending.retain(|(id, _)| *id != c.0);
                self.pending.push(c);
            }
        }
        true
    }

    fn is_plugin(&self) -> bool {
        true
    }
}
