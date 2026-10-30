// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

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

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, TryRecvError};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::Duration;

use altered_plughost::protocol::{
    decode_samples, encode_audio_ch, encode_samples, read_frame, write_frame, Message, NoteEvent,
    ParamInfo, Request, AUDIO, CONTROL, KEY, SELECT,
};
use serde::Serialize;

use super::{find, host_program, param_id, Found};
use crate::dsp::effects::{Params, Processor};
use crate::session::model::{EffectSlot, Id, Tempo, TempoSegment};

/// Longest a plug-in may take to process one block before it counts as hung.
pub const PROCESS_TIMEOUT: Duration = Duration::from_secs(4);

/// Longest a plug-in may stay behind on its blocks (say, while it opens
/// its window during playback) before it counts as hung. Until then it is
/// bypassed block by block and catches up.
const STALL_LIMIT: Duration = Duration::from_secs(20);

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
    /// The power button on its window was clicked: bypassed or not now.
    pub bypass: Option<bool>,
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

/// One host process and its pipes. Usually it runs one plug-in; plug-ins
/// from a maker set to run together (see [`set_shared_makers`]) share one,
/// so they can find each other (an immersive panner and its renderer, for
/// example). A crash or hang stops every plug-in in the process.
struct Conn {
    child: Mutex<Child>,
    /// The pipe in, and which plug-in its last frames were for.
    stdin: Mutex<(ChildStdin, u32)>,
    routes: Arc<Mutex<HashMap<u32, Route>>>,
    dead: Arc<AtomicBool>,
    next_id: std::sync::atomic::AtomicU32,
}

/// Where one plug-in's answers go.
struct Route {
    audio: mpsc::Sender<Vec<f32>>,
    replies: mpsc::Sender<Message>,
    events: Arc<Mutex<Events>>,
    known: Arc<Mutex<HashMap<u32, f64>>>,
}

impl Conn {
    fn spawn() -> Result<Arc<Conn>, String> {
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
        let routes: Arc<Mutex<HashMap<u32, Route>>> = Arc::new(Mutex::new(HashMap::new()));
        let dead = Arc::new(AtomicBool::new(false));
        let (rt, dd) = (routes.clone(), dead.clone());
        thread::Builder::new()
            .name("plugin-pipe".into())
            .spawn(move || {
                let mut buf = Vec::new();
                let mut cur = 0u32;
                while let Ok(Some(kind)) = read_frame(&mut stdout, &mut buf) {
                    if kind == SELECT {
                        if let Some(b) = buf.get(..4) {
                            cur = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
                        }
                        continue;
                    }
                    let Ok(routes) = rt.lock() else { break };
                    // Answers for a plug-in already unloaded are dropped.
                    let Some(r) = routes.get(&cur) else { continue };
                    match kind {
                        AUDIO => {
                            let mut s = Vec::new();
                            decode_samples(&buf, &mut s);
                            let _ = r.audio.send(s);
                        }
                        CONTROL => {
                            let Ok(m) = serde_json::from_slice::<Message>(&buf) else {
                                continue;
                            };
                            let Ok(mut e) = r.events.lock() else { continue };
                            match m {
                                Message::ParamEdited { id, value } => {
                                    if let Ok(mut k) = r.known.lock() {
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
                                Message::BypassPicked { bypassed } => e.bypass = Some(bypassed),
                                reply => {
                                    drop(e);
                                    let _ = r.replies.send(reply);
                                }
                            }
                        }
                        _ => {}
                    }
                }
                // The process ended: every plug-in in it is bypassed, and
                // anyone waiting on an answer hears at once (the channels close).
                let first = !dd.swap(true, Ordering::SeqCst);
                if let Ok(mut routes) = rt.lock() {
                    if first {
                        for r in routes.values() {
                            if let Ok(mut e) = r.events.lock() {
                                e.stopped.get_or_insert_with(|| {
                                    "The plug-in crashed and is bypassed.".into()
                                });
                            }
                        }
                    }
                    routes.clear();
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Arc::new(Conn {
            child: Mutex::new(child),
            stdin: Mutex::new((stdin, 0)),
            routes,
            dead,
            next_id: std::sync::atomic::AtomicU32::new(0),
        }))
    }

    /// Writes a frame for plug-in `id`. False when the process is gone.
    fn write(&self, id: u32, kind: u8, payload: &[u8]) -> bool {
        let Ok(mut g) = self.stdin.lock() else {
            return false;
        };
        let (pipe, selected) = &mut *g;
        if *selected != id {
            if write_frame(pipe, SELECT, &id.to_le_bytes()).is_err() {
                return false;
            }
            *selected = id;
        }
        write_frame(pipe, kind, payload).is_ok()
    }

    /// Stops the process: every plug-in in it is bypassed with `why`.
    fn kill(&self, why: &str) {
        if !self.dead.swap(true, Ordering::SeqCst) {
            if let Ok(routes) = self.routes.lock() {
                for r in routes.values() {
                    if let Ok(mut e) = r.events.lock() {
                        e.stopped.get_or_insert_with(|| why.to_string());
                    }
                }
            }
        }
        if let Ok(mut c) = self.child.lock() {
            let _ = c.kill();
        }
    }
}

impl Drop for Conn {
    fn drop(&mut self) {
        if let Ok(mut g) = self.stdin.lock() {
            let _ = g.0.flush();
        }
        if let Ok(mut c) = self.child.lock() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

/// Makers known to need their plug-ins in one process: their immersive
/// tools (a panner per track and a renderer on the mix) talk to each other.
pub fn default_shared_makers() -> Vec<String> {
    vec!["fiedler audio".to_string()]
}

/// Makers whose plug-ins run together in one process, lower case.
fn shared_makers() -> &'static Mutex<HashSet<String>> {
    static S: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(default_shared_makers().into_iter().collect()))
}

/// Sets the makers whose plug-ins run together in one process (from the
/// Plug-in Manager). Playback's running plug-ins from a maker whose choice
/// changed stop, so they start again the new way at the next playback
/// (from the settings saved in the session; fetch those first).
pub fn set_shared_makers(makers: Vec<String>) {
    let new: HashSet<String> = makers
        .into_iter()
        .map(|m| m.trim().to_lowercase())
        .filter(|m| !m.is_empty())
        .collect();
    let changed: HashSet<String> = match shared_makers().lock() {
        Ok(mut s) => {
            let changed = s.symmetric_difference(&new).cloned().collect();
            *s = new;
            changed
        }
        Err(_) => return,
    };
    if changed.is_empty() {
        return;
    }
    if let Ok(mut p) = pool().lock() {
        p.retain(|_, e| {
            find(&e.effect).is_none_or(|f| !changed.contains(&f.info.vendor.trim().to_lowercase()))
        });
    }
}

/// The makers whose plug-ins run together.
pub fn shared_maker_list() -> Vec<String> {
    let mut v: Vec<String> = shared_makers()
        .lock()
        .map(|s| s.iter().cloned().collect())
        .unwrap_or_default();
    v.sort();
    v
}

/// Running shared processes: (scope, maker, rate) to the process, while
/// any of its plug-ins runs. Scope 0 is playback; each mixdown has its own.
type SharedConns = Mutex<HashMap<(u64, String, u32), std::sync::Weak<Conn>>>;
fn shared_conns() -> &'static SharedConns {
    static C: OnceLock<SharedConns> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The process a plug-in by `vendor` runs in: a shared one for makers set
/// to run together, otherwise a new one of its own.
fn conn_for(vendor: &str, rate: u32) -> Result<Arc<Conn>, String> {
    let maker = vendor.trim().to_lowercase();
    let shared = shared_makers().lock().is_ok_and(|s| s.contains(&maker));
    if !shared {
        return Conn::spawn();
    }
    let key = (SCOPE.with(Cell::get), maker, rate);
    let mut map = shared_conns().lock().map_err(|e| e.to_string())?;
    map.retain(|_, w| w.strong_count() > 0);
    if let Some(c) = map.get(&key).and_then(std::sync::Weak::upgrade) {
        if !c.dead.load(Ordering::SeqCst) {
            return Ok(c);
        }
    }
    let c = Conn::spawn()?;
    SHARED_STARTED.fetch_add(1, Ordering::SeqCst);
    map.insert(key, Arc::downgrade(&c));
    Ok(c)
}

static SHARED_STARTED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// How many shared processes have been started (for tests).
pub fn shared_processes_started() -> u64 {
    SHARED_STARTED.load(Ordering::SeqCst)
}

/// One plug-in running in a host process, driven over its pipes.
pub struct Host {
    conn: Arc<Conn>,
    id: u32,
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
    key_buf: Vec<u8>,
    rate: u32,
    /// The stretch of the tempo map the plug-in was last told.
    told: Option<TempoSegment>,
    /// The main input's and output's speakers by name: L and R unless the
    /// plug-in runs with its own layout.
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    /// The settings the plug-in last reported or was given, so the same
    /// settings coming back from the session are not sent again.
    last_state: Option<String>,
    /// Settings the plug-in itself reported in the last moments (when, and a
    /// hash). They come back through the session a beat later, by which
    /// time the plug-in may have moved on; sending them back would set the
    /// plug-in to a stale copy of itself (and some rebuild their window).
    reported: Vec<(std::time::Instant, u64)>,
    /// Blocks sent whose answer has not come back yet, and since when.
    late: usize,
    late_since: Option<std::time::Instant>,
}

fn state_hash(data: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    data.hash(&mut h);
    h.finish()
}

/// How long a plug-in's own report counts as still travelling back.
const REPORT_ECHO: Duration = Duration::from_secs(3);

impl Host {
    /// Starts `found` in a host process (its own, or its maker's shared one).
    pub fn start(
        found: &Found,
        rate: u32,
        state: Option<String>,
        params: Vec<(u32, f64)>,
        wide: bool,
    ) -> Result<Host, String> {
        let conn = conn_for(&found.info.vendor, rate)?;
        let id = conn.next_id.fetch_add(1, Ordering::SeqCst);
        let (atx, arx) = mpsc::channel();
        let (rtx, rrx) = mpsc::channel();
        let events = Arc::new(Mutex::new(Events::default()));
        let known = Arc::new(Mutex::new(
            params.iter().copied().collect::<HashMap<u32, f64>>(),
        ));
        conn.routes.lock().map_err(|e| e.to_string())?.insert(
            id,
            Route {
                audio: atx,
                replies: rtx,
                events: events.clone(),
                known: known.clone(),
            },
        );
        let mut host = Host {
            dead: conn.dead.clone(),
            conn,
            id,
            audio: arx,
            replies: rrx,
            events,
            known,
            latency: 0,
            params: Vec::new(),
            has_editor: false,
            buf: Vec::new(),
            key_buf: Vec::new(),
            rate,
            told: None,
            last_state: state.clone(),
            reported: Vec::new(),
            late: 0,
            late_since: None,
            inputs: vec!["L".into(), "R".into()],
            outputs: vec!["L".into(), "R".into()],
        };
        let load = Request::Load {
            path: found.bundle.to_string_lossy().into_owned(),
            cid: found.info.cid.clone(),
            rate: f64::from(rate),
            state,
            params,
            main: wide.then_some(0),
        };
        match host.request(&load, LOAD_TIMEOUT) {
            Some(Message::Loaded {
                latency,
                params,
                has_editor,
                inputs,
                outputs,
                sidechain,
                ..
            }) => {
                if sidechain && !found.info.sidechain {
                    super::note_sidechain(&found.info.cid);
                }
                host.latency = latency as usize;
                host.params = params;
                host.has_editor = has_editor;
                if wide {
                    host.inputs = inputs;
                    host.outputs = outputs;
                }
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
        self.conn.kill(why);
    }

    /// Sends a request and waits for its reply.
    pub fn request(&mut self, r: &Request, timeout: Duration) -> Option<Message> {
        if self.is_dead() {
            return None;
        }
        let json = serde_json::to_vec(r).ok()?;
        if !self.conn.write(self.id, CONTROL, &json) {
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

    /// Processes interleaved stereo in place, playing `notes` (for
    /// instruments). False (and `io` untouched) when the plug-in is gone.
    pub fn process(&mut self, changes: &[(u32, f64)], notes: &[NoteEvent], io: &mut [f32]) -> bool {
        self.process_ch(changes, notes, io, 2)
    }

    /// Channels per frame the plug-in's audio goes in and out with:
    /// stereo, or its own layout's (the larger of input and output).
    pub fn width(&self) -> usize {
        self.inputs.len().max(self.outputs.len()).max(1)
    }

    /// [`Host::process`] for `channels` interleaved channels.
    pub fn process_ch(
        &mut self,
        changes: &[(u32, f64)],
        notes: &[NoteEvent],
        io: &mut [f32],
        channels: usize,
    ) -> bool {
        self.process_keyed_ch(changes, notes, io, channels, None)
    }

    /// [`Host::process_ch`] with a sidechain key: interleaved stereo, as
    /// many frames as `io`. A plug-in with a second audio input hears it
    /// there; others ignore it.
    pub fn process_keyed_ch(
        &mut self,
        changes: &[(u32, f64)],
        notes: &[NoteEvent],
        io: &mut [f32],
        channels: usize,
        key: Option<&[f32]>,
    ) -> bool {
        if self.is_dead() || io.is_empty() {
            return false;
        }
        // Behind: take the late answers that came in, and bypass this block
        // until all are back, so the next answer belongs to the next block.
        while self.late > 0 {
            match self.audio.try_recv() {
                Ok(_) => self.late -= 1,
                Err(TryRecvError::Empty) => {
                    if self.late_since.is_some_and(|t| t.elapsed() > STALL_LIMIT) {
                        self.stop("The plug-in stopped answering and is bypassed.");
                    }
                    return false;
                }
                Err(TryRecvError::Disconnected) => return false,
            }
        }
        self.late_since = None;
        if !changes.is_empty() {
            if let Ok(mut k) = self.known.lock() {
                k.extend(changes.iter().copied());
            }
        }
        let position = block_position();
        if position >= 0 {
            // Tempo changes reach the plug-in as playback crosses them.
            if let Some(seg) = BLOCK_TEMPO.with(|t| {
                t.borrow()
                    .as_ref()
                    .map(|t| t.segment_at_seconds(position as f64 / f64::from(self.rate.max(1))))
            }) {
                if self.told != Some(seg) {
                    self.tell_tempo(seg);
                }
            }
        }
        if let Some(k) = key.filter(|k| k.len() == io.len() / channels.max(1) * 2) {
            encode_samples(&mut self.key_buf, k);
            if !self.conn.write(self.id, KEY, &self.key_buf) {
                self.stop("The plug-in crashed and is bypassed.");
                return false;
            }
        }
        encode_audio_ch(&mut self.buf, position, changes, notes, io, channels);
        if !self.conn.write(self.id, AUDIO, &self.buf) {
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
                // Busy for a moment (a window opening, a licence check):
                // bypassed until it catches up, stopped only past STALL_LIMIT.
                self.late += 1;
                self.late_since.get_or_insert_with(std::time::Instant::now);
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
            Message::State { data } => {
                self.last_state = Some(data.clone());
                let now = std::time::Instant::now();
                self.reported
                    .retain(|(t, _)| now.duration_since(*t) < REPORT_ECHO);
                self.reported.push((now, state_hash(&data)));
                Some(data)
            }
            _ => None,
        }
    }

    /// Gives the plug-in saved settings (a preset, or an undo), unless it
    /// already has exactly these.
    pub fn set_state(&mut self, data: &str) -> Result<(), String> {
        if self.last_state.as_deref() == Some(data) {
            return Ok(());
        }
        let (now, hash) = (std::time::Instant::now(), state_hash(data));
        if self
            .reported
            .iter()
            .any(|(t, h)| *h == hash && now.duration_since(*t) < REPORT_ECHO)
        {
            return Ok(());
        }
        match self.request(&Request::SetState { data: data.into() }, REQUEST_TIMEOUT) {
            Some(Message::Ok) => {
                self.last_state = Some(data.into());
                Ok(())
            }
            Some(Message::Error { message }) => Err(message),
            _ => Err("The plug-in did not answer.".into()),
        }
    }

    /// Opens the plug-in's window, owned by `owner` (the main window's
    /// handle on Windows, 0 for none) so it stays above it.
    pub fn open_window(&mut self, title: &str, owner: u64, bypassed: bool) -> Result<(), String> {
        match self.request(
            &Request::OpenEditor {
                title: title.into(),
                owner,
                bypassed,
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

    /// Sets the power button on the plug-in's window, if it is open.
    pub fn show_bypassed(&mut self, bypassed: bool) {
        let _ = self.request(&Request::EditorBypassed { bypassed }, REQUEST_TIMEOUT);
    }

    pub fn hide_window(&mut self, hidden: bool) {
        let _ = self.request(&Request::HideEditor { hidden }, REQUEST_TIMEOUT);
    }

    pub fn reset(&mut self) {
        let _ = self.request(&Request::Reset, REQUEST_TIMEOUT);
    }

    /// Tells the plug-in the tempo and time signature where playback starts.
    pub fn set_tempo(&mut self, tempo: &Tempo) {
        let seg = tempo.segments().next().expect("never empty");
        self.tell_tempo(seg);
    }

    fn tell_tempo(&mut self, seg: TempoSegment) {
        self.told = Some(seg);
        let _ = self.request(
            &Request::Tempo {
                bpm: seg.bpm,
                beats_per_bar: seg.beats_per_bar,
                beat_unit: seg.beat_unit,
                start: (seg.seconds * f64::from(self.rate)).round() as i64,
                start_music: seg.beat,
            },
            REQUEST_TIMEOUT,
        );
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        // Others in a shared process carry on; the process ends with its last plug-in.
        if !self.is_dead() {
            if let Ok(json) = serde_json::to_vec(&Request::Unload) {
                let _ = self.conn.write(self.id, CONTROL, &json);
            }
        }
        if let Ok(mut r) = self.conn.routes.lock() {
            r.remove(&self.id);
        }
    }
}

thread_local! {
    /// The timeline frame of the block being rendered on this thread, for
    /// plug-ins' song position; -1 when unknown (previews, auditions).
    static BLOCK_POS: Cell<i64> = const { Cell::new(-1) };
}

thread_local! {
    /// The tempo map of the mix being rendered on this thread, so
    /// plug-ins follow its tempo and time signature changes.
    static BLOCK_TEMPO: RefCell<Option<Arc<Tempo>>> = const { RefCell::new(None) };
}

/// Sets the tempo map of the mix rendered on this thread.
pub fn set_block_tempo(tempo: Option<Arc<Tempo>>) {
    BLOCK_TEMPO.with(|t| *t.borrow_mut() = tempo);
}

/// The tempo map of the mix rendered on this thread.
pub fn block_tempo() -> Option<Arc<Tempo>> {
    BLOCK_TEMPO.with(|t| t.borrow().clone())
}

/// Sets the timeline frame of the block about to be rendered on this thread.
pub fn set_block_position(frame: i64) {
    BLOCK_POS.with(|c| c.set(frame));
}

/// The timeline frame of the block being rendered on this thread (-1 when unknown).
pub fn block_position() -> i64 {
    BLOCK_POS.with(Cell::get)
}

struct Pooled {
    effect: String,
    rate: u32,
    wide: bool,
    host: Arc<Mutex<Host>>,
}

fn pool() -> &'static Mutex<HashMap<Id, Pooled>> {
    static P: OnceLock<Mutex<HashMap<Id, Pooled>>> = OnceLock::new();
    P.get_or_init(|| Mutex::new(HashMap::new()))
}

thread_local! {
    static FRESH: Cell<bool> = const { Cell::new(false) };
    /// Which shared processes plug-ins join: 0 for playback, a new number per mixdown.
    static SCOPE: Cell<u64> = const { Cell::new(0) };
}

/// Runs `f` with fresh plug-in hosts instead of playback's (a mixdown).
pub fn fresh<T>(f: impl FnOnce() -> T) -> T {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let was = FRESH.with(|c| c.replace(true));
    let scope = SCOPE.with(|c| c.replace(NEXT.fetch_add(1, Ordering::SeqCst)));
    let out = f();
    FRESH.with(|c| c.set(was));
    SCOPE.with(|c| c.set(scope));
    out
}

/// True when the slot's plug-in runs with its own channel layout.
fn wide_of(slot: &EffectSlot) -> bool {
    slot.routing.as_ref().is_some_and(|r| r.wide)
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
    if fresh {
        if let Some(h) = borrowable(slot, rate) {
            return Some(h);
        }
    } else {
        // One start per slot: if it is already starting (in the background,
        // see `warm`), wait for that one instead of starting a second.
        let deadline = std::time::Instant::now() + LOAD_TIMEOUT;
        loop {
            if let Ok(p) = pool().lock() {
                if let Some(e) = p.get(&slot.id) {
                    if e.effect == slot.effect && e.rate == rate && e.wide == wide_of(slot) {
                        return Some(e.host.clone());
                    }
                }
            }
            let mut s = starting().lock().ok()?;
            if !s.contains(&slot.id) || std::time::Instant::now() > deadline {
                s.insert(slot.id);
                break;
            }
            drop(s);
            thread::sleep(Duration::from_millis(50));
        }
    }
    let started = Host::start(
        &found,
        rate,
        slot.plugin_state.clone(),
        slot_params(slot),
        wide_of(slot),
    );
    if !fresh {
        if let Ok(mut s) = starting().lock() {
            s.remove(&slot.id);
        }
    }
    let host = match started {
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
                    wide: wide_of(slot),
                    host: host.clone(),
                },
            );
        }
    }
    Some(host)
}

/// A mixdown may use playback's host for a slot when nothing is playing
/// through it: it is already loaded (an instrument's sounds and all) and
/// holds the settings as heard, so the export sounds the same and starts
/// at once. Only the pool holds it then; playback holds a second handle
/// while it plays.
fn borrowable(slot: &EffectSlot, rate: u32) -> Option<Arc<Mutex<Host>>> {
    let p = pool().lock().ok()?;
    let e = p.get(&slot.id)?;
    (e.effect == slot.effect
        && e.rate == rate
        && e.wide == wide_of(slot)
        && Arc::strong_count(&e.host) == 1)
        .then(|| e.host.clone())
}

/// True when `host` is playback's own (borrowed by a mixdown), not a fresh copy.
fn is_pooled(slot: Id, host: &Arc<Mutex<Host>>) -> bool {
    pool()
        .lock()
        .is_ok_and(|p| p.get(&slot).is_some_and(|e| Arc::ptr_eq(&e.host, host)))
}

/// Slots whose playback host is being started right now.
fn starting() -> &'static Mutex<HashSet<Id>> {
    static S: OnceLock<Mutex<HashSet<Id>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Starts the playback hosts of `slots` in the background, so a plug-in
/// or instrument that takes a while to load (licence checks, samples) is
/// ready before Play rather than holding playback up. Slots already
/// running or starting are left alone.
pub fn warm(slots: Vec<EffectSlot>) {
    let todo: Vec<EffectSlot> = slots
        .into_iter()
        .filter(|s| running(s.id).is_none() && !starting().lock().is_ok_and(|g| g.contains(&s.id)))
        .collect();
    if todo.is_empty() {
        return;
    }
    let _ = thread::Builder::new()
        .name("plugin-warm".into())
        .spawn(move || {
            let rate = crate::engine::output::device_rate();
            // A few at a time: a session with dozens of heavy plug-ins
            // loading all at once slows the whole computer, the editor too.
            let workers = thread::available_parallelism()
                .map_or(2, |n| (n.get() / 2).clamp(2, 6))
                .min(todo.len());
            let mut todo = todo;
            todo.reverse();
            let queue = Arc::new(Mutex::new(todo));
            let handles: Vec<_> = (0..workers)
                .filter_map(|_| {
                    let queue = queue.clone();
                    thread::Builder::new()
                        .name("plugin-warm".into())
                        .spawn(move || loop {
                            let next = queue.lock().ok().and_then(|mut q| q.pop());
                            let Some(s) = next else { break };
                            let _ = host_for(&s, rate);
                        })
                        .ok()
                })
                .collect();
            for h in handles {
                let _ = h.join();
            }
        });
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

/// The controls of a running copy of plug-in `effect`, for plug-ins whose
/// scan could not read them (they are learned when it runs).
pub fn live_params(effect: &str) -> Option<Vec<ParamInfo>> {
    let hosts: Vec<Arc<Mutex<Host>>> = pool()
        .lock()
        .ok()?
        .values()
        .filter(|p| p.effect.eq_ignore_ascii_case(effect))
        .map(|p| p.host.clone())
        .collect();
    hosts.into_iter().find_map(|h| {
        h.try_lock()
            .ok()
            .map(|h| h.params.clone())
            .filter(|p| !p.is_empty())
    })
}

/// Running plug-ins whose settings in the session changed (a preset, an
/// undo) get the new settings, in the background. `before` holds each
/// plug-in slot's settings before the change.
pub fn push_states(before: &HashMap<Id, Option<String>>, after: &HashMap<Id, Option<String>>) {
    let todo: Vec<(Arc<Mutex<Host>>, String)> = after
        .iter()
        .filter(|(id, s)| before.get(id).is_some_and(|b| b != *s))
        .filter_map(|(id, s)| Some((running(*id)?, s.clone()?)))
        .collect();
    if todo.is_empty() {
        return;
    }
    let _ = thread::Builder::new()
        .name("plugin-state".into())
        .spawn(move || {
            for (h, s) in todo {
                if let Ok(mut h) = h.lock() {
                    if let Err(e) = h.set_state(&s) {
                        eprintln!("plug-in settings not applied: {e}");
                    }
                }
            }
        });
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
/// Hides every open plug-in window for a while, or shows them again.
pub fn hide_windows(hidden: bool) {
    let hosts: Vec<Arc<Mutex<Host>>> = pool()
        .lock()
        .map(|p| p.values().map(|e| e.host.clone()).collect())
        .unwrap_or_default();
    for h in hosts {
        if let Ok(mut h) = h.lock() {
            h.hide_window(hidden);
        }
    }
}

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
    /// Its main input's and output's speakers, and the channels per frame
    /// its audio travels with.
    ins: Vec<String>,
    outs: Vec<String>,
    width: usize,
    pending: Vec<(u32, f64)>,
    /// When the plug-in has stopped, the dry sound still arrives as late
    /// as the plug-in's would have, so the track stays in time.
    bypass: crate::dsp::effects::Delay,
    /// Playing live: when the plug-in is busy with a request (its window
    /// opening), the block is bypassed instead of holding playback up. A
    /// mixdown waits instead, so nothing is left out of the file.
    live: bool,
}

/// The host's lock if it comes free within a few milliseconds (quick
/// reads from the interface pass), None while a long request holds it.
fn lock_briefly(host: &Mutex<Host>) -> Option<std::sync::MutexGuard<'_, Host>> {
    let deadline = std::time::Instant::now() + Duration::from_millis(30);
    loop {
        match host.try_lock() {
            Ok(h) => return Some(h),
            Err(std::sync::TryLockError::Poisoned(_)) => return None,
            Err(std::sync::TryLockError::WouldBlock) => {
                if std::time::Instant::now() > deadline {
                    return None;
                }
                thread::sleep(Duration::from_millis(1));
            }
        }
    }
}

/// The processor for a plug-in slot, or None when it is not installed or
/// would not start (the chain then skips it).
pub fn processor(slot: &EffectSlot, rate: u32) -> Option<PluginProcessor> {
    let host = host_for(slot, rate)?;
    let (latency, pending, ins, outs, width) = {
        let h = host.lock().ok()?;
        (
            h.latency,
            h.differing(&slot_params(slot)),
            h.inputs.clone(),
            h.outputs.clone(),
            h.width(),
        )
    };
    Some(PluginProcessor {
        host,
        latency,
        ins,
        outs,
        width,
        pending,
        bypass: crate::dsp::effects::Delay::with_channels(latency, width),
        live: !FRESH.with(|c| c.get()),
    })
}

impl Processor for PluginProcessor {
    fn process(&mut self, buf: &mut [f32]) {
        self.process_keyed(buf, &[]);
    }

    fn process_keyed(&mut self, buf: &mut [f32], key: &[f32]) {
        let key = (!key.is_empty()).then_some(key);
        let host = if self.live {
            lock_briefly(&self.host)
        } else {
            self.host.lock().ok()
        };
        let done = match host {
            Some(mut h) => h.process_keyed_ch(&self.pending, &[], buf, self.width, key),
            None => false,
        };
        if done {
            self.pending.clear();
        } else {
            self.bypass.run(buf);
        }
    }

    fn channels(&self) -> (Vec<String>, Vec<String>) {
        (self.ins.clone(), self.outs.clone())
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

/// A MIDI track's instrument, played by the renderer: notes in, sound out.
/// When it is missing or stops, the track is silent.
/// How long an instrument started for an export runs on silence first,
/// so instruments that load their sounds in the background are ready.
const INSTRUMENT_WARMUP: Duration = Duration::from_millis(2_500);

pub struct Instrument {
    host: Option<Arc<Mutex<Host>>>,
    pub latency: usize,
    pending: Vec<(u32, f64)>,
}

impl Instrument {
    /// Starts (or picks up) the slot's instrument, told the session's
    /// tempo, with every note let go.
    pub fn start(slot: &EffectSlot, rate: u32, tempo: &Tempo) -> Instrument {
        let host = host_for(slot, rate);
        // A copy just started for a mixdown needs to load; playback's own is ready.
        let fresh =
            FRESH.with(|c| c.get()) && host.as_ref().is_some_and(|h| !is_pooled(slot.id, h));
        let (latency, pending) = match host.as_ref().and_then(|h| h.lock().ok()) {
            Some(mut h) => {
                h.reset();
                h.set_tempo(tempo);
                let pending = h.differing(&slot_params(slot));
                if fresh {
                    // A copy just started for an export: sample-based
                    // instruments load their sounds in the background, and
                    // would be silent at first. Run it on silence, at about
                    // the pace of playback, so they finish loading.
                    let mut buf = vec![0.0f32; 1_024 * 2];
                    let started = std::time::Instant::now();
                    let mut first = true;
                    while started.elapsed() < INSTRUMENT_WARMUP {
                        let changes: &[(u32, f64)] = if first { &pending } else { &[] };
                        first = false;
                        buf.fill(0.0);
                        if !h.process(changes, &[], &mut buf) {
                            break;
                        }
                        thread::sleep(Duration::from_millis(15));
                    }
                    h.reset();
                    (h.latency, Vec::new())
                } else {
                    (h.latency, pending)
                }
            }
            None => (0, Vec::new()),
        };
        Instrument {
            host,
            latency,
            pending,
        }
    }

    /// Plays one block into `io` (stereo; zeroed here), with `notes` at
    /// their frame offsets.
    pub fn play(&mut self, notes: &[NoteEvent], io: &mut [f32]) {
        io.fill(0.0);
        let Some(host) = &self.host else { return };
        let done = match host.lock() {
            Ok(mut h) => h.process(&self.pending, notes, io),
            Err(_) => false,
        };
        if done {
            self.pending.clear();
        } else {
            io.fill(0.0);
        }
    }
}

/// Plays one note through a MIDI track's instrument, for hearing notes
/// while editing them: the note held for `seconds`, then let go, then a
/// short tail. Starting the instrument can take a while, so this waits
/// for it here, before any sound plays. If the preview is cut short (by
/// the next note, say), the note is still let go, so it never hangs on.
pub fn audition(
    slot: &EffectSlot,
    pitch: u8,
    velocity: f32,
    channel: u8,
    seconds: f64,
) -> Result<crate::engine::preview::MakeFill, String> {
    let note = SeqNote {
        pitch,
        velocity,
        channel,
        pan: 0.0,
        on: 0.0,
        off: seconds.clamp(0.05, 8.0),
    };
    sequence(slot, vec![note])
}

/// A note of a [`sequence`], with its times in seconds from the start.
#[derive(Debug, Clone, Copy, serde::Deserialize)]
pub struct SeqNote {
    pub pitch: u8,
    /// 0 to 1.
    pub velocity: f32,
    #[serde(default)]
    pub channel: u8,
    #[serde(default)]
    pub pan: f32,
    pub on: f64,
    pub off: f64,
}

/// Plays notes through a MIDI track's instrument on their own (a MIDI
/// clip heard alone, from the piano roll), then a short tail. Stopped
/// early, every note still sounding is let go.
pub fn sequence(
    slot: &EffectSlot,
    notes: Vec<SeqNote>,
) -> Result<crate::engine::preview::MakeFill, String> {
    let rate = crate::engine::output::device_rate();
    let host = host_for(slot, rate).ok_or("The instrument could not start.")?;
    if host.lock().map_err(|e| e.to_string())?.is_dead() {
        return Err("The instrument stopped. Restart it from the track.".into());
    }
    let pending = host
        .lock()
        .map(|h| h.differing(&slot_params(slot)))
        .unwrap_or_default();
    let frame = |s: f64| (s.max(0.0) * f64::from(rate)) as usize;
    let mut events: Vec<(usize, NoteEvent)> = Vec::new();
    for n in &notes {
        let (on, off) = (frame(n.on), frame(n.off).max(frame(n.on) + 1));
        let ev = |on_: bool| NoteEvent {
            offset: 0,
            on: on_,
            pitch: n.pitch.min(127),
            velocity: if on_ { n.velocity.clamp(0.0, 1.0) } else { 0.0 },
            channel: n.channel.min(15),
            pan: if on_ { n.pan.clamp(-1.0, 1.0) } else { 0.0 },
        };
        events.push((on, ev(true)));
        events.push((off, ev(false)));
    }
    // In time order; at the same frame a note ends before the next starts.
    events.sort_by_key(|(f, e)| (*f, e.on));
    let total = events.last().map_or(0, |e| e.0) + rate as usize * 2 / 5;
    Ok(Box::new(move |channels: usize, _device_rate: u32| {
        let mut seq = Audition {
            host,
            events,
            next: 0,
            sounding: Vec::new(),
            total,
            pos: 0,
            pending,
            stereo: Vec::new(),
        };
        Ok(
            Box::new(move |block: &mut Vec<f32>| seq.fill(block, channels))
                as crate::engine::preview::Fill,
        )
    }))
}

struct Audition {
    host: Arc<Mutex<Host>>,
    /// Note starts and ends by frame, in order.
    events: Vec<(usize, NoteEvent)>,
    next: usize,
    /// Notes started and not yet let go: (pitch, channel).
    sounding: Vec<(u8, u8)>,
    total: usize,
    pos: usize,
    pending: Vec<(u32, f64)>,
    stereo: Vec<f32>,
}

impl Audition {
    const BLOCK: usize = 512;

    fn fill(&mut self, block: &mut Vec<f32>, channels: usize) -> bool {
        if self.pos >= self.total {
            return false;
        }
        let n = Self::BLOCK.min(self.total - self.pos);
        let mut notes = Vec::new();
        while let Some((f, e)) = self.events.get(self.next).copied() {
            if f >= self.pos + n {
                break;
            }
            self.next += 1;
            if e.on {
                self.sounding.push((e.pitch, e.channel));
            } else {
                self.sounding.retain(|x| *x != (e.pitch, e.channel));
            }
            notes.push(NoteEvent {
                offset: f.saturating_sub(self.pos) as u32,
                ..e
            });
        }
        self.stereo.clear();
        self.stereo.resize(n * 2, 0.0);
        let ok = match self.host.lock() {
            Ok(mut h) => h.process(&self.pending, &notes, &mut self.stereo),
            Err(_) => false,
        };
        self.pending.clear();
        if !ok {
            return false;
        }
        for f in self.stereo.as_chunks::<2>().0 {
            for c in 0..channels.max(1) {
                block.push(if channels == 1 {
                    (f[0] + f[1]) * 0.5
                } else {
                    f[c.min(1)]
                });
            }
        }
        self.pos += n;
        true
    }
}

impl Drop for Audition {
    fn drop(&mut self) {
        if self.sounding.is_empty() {
            return;
        }
        let off: Vec<NoteEvent> = self
            .sounding
            .iter()
            .map(|&(pitch, channel)| NoteEvent {
                offset: 0,
                on: false,
                pitch,
                velocity: 0.0,
                channel,
                pan: 0.0,
            })
            .collect();
        let mut buf = vec![0.0; 64 * 2];
        if let Ok(mut h) = self.host.lock() {
            h.process(&[], &off, &mut buf);
        }
    }
}
