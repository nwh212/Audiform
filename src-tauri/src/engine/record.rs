// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Recording: one file per armed track, from the input each track picked.
//!
//! Threads: each input device gets a thread that opens its stream. The
//! device callback only copies samples into a lock-free ring buffer;
//! the same thread drains the ring every few milliseconds and writes
//! each track's channels to its own 32-bit float WAV file.
//!
//! Input monitoring: targets marked `monitor` are also mixed to stereo
//! and sent to the default output, so the talent hears themselves.
//! Software monitoring adds some delay (the device buffers plus about
//! 10 to 30 ms here); an interface's own direct monitoring has none.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, StreamConfig};
use rtrb::{Consumer, Producer, RingBuffer};

use crate::dsp::effects::ChainProcessor;
use crate::session::model::EffectChain;
use serde::{Deserialize, Serialize};

/// One thing to record: usually an armed track.
#[derive(Debug, Clone, Deserialize)]
pub struct RecordTarget {
    /// Who the take is for (a track id, or "wave" for the Waveform editor).
    pub key: String,
    /// The track's input: "Mono: <device> <n>", "Stereo: <device> <a>-<b>",
    /// "none", or None for the default input.
    pub input: Option<String>,
    /// 1 or 2: what the take holds when the input is the default.
    pub channels: u16,
    /// File name, without extension.
    pub name: String,
    /// Also play this input through the speakers.
    #[serde(default)]
    pub monitor: bool,
    /// The track's Voice Chain. With `monitor`, the player hears the
    /// built-in effects in it, as they were when monitoring started.
    #[serde(default)]
    pub chain: Option<EffectChain>,
}

/// A finished take.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Take {
    pub key: String,
    pub path: PathBuf,
    pub sample_rate: u32,
    pub channels: u16,
    /// Frames written.
    pub length: u64,
    /// Frames into the recording at which this take began: 0 for a track
    /// armed from the start, later for one armed while recording.
    #[serde(default)]
    pub start: u64,
}

/// Which device and which of its channels (0-based) a target records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    /// None is the default input device.
    pub device: Option<String>,
    pub channels: Vec<usize>,
}

/// Works out a target's device and channels. None means "do not record".
pub fn route(input: Option<&str>, take_channels: u16) -> Option<Route> {
    let default = || Route {
        device: None,
        channels: if take_channels >= 2 {
            vec![0, 1]
        } else {
            vec![0]
        },
    };
    let Some(s) = input else {
        return Some(default());
    };
    if s == "none" {
        return None;
    }
    let parse = |rest: &str, stereo: bool| -> Option<Route> {
        let (name, chans) = rest.rsplit_once(' ')?;
        let channels = if stereo {
            let (a, b) = chans.split_once('-')?;
            vec![
                a.parse::<usize>().ok()?.checked_sub(1)?,
                b.parse::<usize>().ok()?.checked_sub(1)?,
            ]
        } else {
            vec![chans.parse::<usize>().ok()?.checked_sub(1)?]
        };
        Some(Route {
            device: Some(name.to_string()),
            channels,
        })
    };
    if let Some(rest) = s.strip_prefix("Mono: ") {
        return parse(rest, false).or_else(|| Some(default()));
    }
    if let Some(rest) = s.strip_prefix("Stereo: ") {
        return parse(rest, true).or_else(|| Some(default()));
    }
    Some(default())
}

/// Writes one device's interleaved input to each take's file.
pub struct TakeWriter {
    dir: PathBuf,
    rate: u32,
    takes: Vec<(
        Take,
        Vec<usize>,
        hound::WavWriter<std::io::BufWriter<std::fs::File>>,
    )>,
    /// Takes already closed (their track was disarmed while recording).
    done: Vec<Take>,
    device_channels: usize,
}

impl TakeWriter {
    pub fn create(
        dir: &Path,
        targets: &[(RecordTarget, Vec<usize>)],
        rate: u32,
        device_channels: usize,
    ) -> Result<Self, String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let mut w = Self {
            dir: dir.to_path_buf(),
            rate,
            takes: Vec::new(),
            done: Vec::new(),
            device_channels: device_channels.max(1),
        };
        for (t, chans) in targets {
            w.add(t, chans, 0)?;
        }
        Ok(w)
    }

    /// Starts a take for `target`, `start` frames into the recording. Each
    /// target gets its own file, even when several share an input.
    pub fn add(&mut self, t: &RecordTarget, chans: &[usize], start: u64) -> Result<(), String> {
        let channels = chans.len().clamp(1, 2) as u16;
        let path = crate::export::mixdown::unique_path(&self.dir, &t.name, "wav");
        let spec = hound::WavSpec {
            channels,
            sample_rate: self.rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let w = hound::WavWriter::create(&path, spec).map_err(|e| e.to_string())?;
        self.takes.push((
            Take {
                key: t.key.clone(),
                path,
                sample_rate: self.rate,
                channels,
                length: 0,
                start,
            },
            chans.to_vec(),
            w,
        ));
        Ok(())
    }

    /// Ends the take for `key`, if this device is recording it.
    pub fn close(&mut self, key: &str) -> Result<(), String> {
        while let Some(i) = self.takes.iter().position(|(t, _, _)| t.key == key) {
            let (take, _, w) = self.takes.remove(i);
            w.finalize().map_err(|e| e.to_string())?;
            self.done.push(take);
        }
        Ok(())
    }

    pub fn is_recording(&self, key: &str) -> bool {
        self.takes.iter().any(|(t, _, _)| t.key == key)
    }

    /// Writes whole frames from `interleaved`. Channels the device does
    /// not have record silence.
    pub fn push(&mut self, interleaved: &[f32]) -> Result<(), String> {
        let n = self.device_channels;
        for frame in interleaved.chunks_exact(n) {
            for (take, chans, w) in &mut self.takes {
                for &c in chans.iter() {
                    w.write_sample(frame.get(c).copied().unwrap_or(0.0))
                        .map_err(|e| e.to_string())?;
                }
                take.length += 1;
            }
        }
        Ok(())
    }

    pub fn finish(self) -> Result<Vec<Take>, String> {
        let mut out = self.done;
        for (take, _, w) in self.takes {
            w.finalize().map_err(|e| e.to_string())?;
            out.push(take);
        }
        Ok(out)
    }
}

/// Frames per live waveform block. Same as the finest level of the
/// finished file's peak cache, so what is drawn while recording is the
/// data the finished waveform will use.
pub const LIVE_BLOCK: usize = 32;

/// Live level and waveform for one target, filled by the disk thread.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Live {
    /// Peak since the last status call, 0 to 1.
    pub peak: f32,
    /// Channels per block (1 or 2).
    pub channels: usize,
    /// Per block of [`LIVE_BLOCK`] frames since the last call, per channel:
    /// min then max as 16-bit values, scaled and rounded as the peak cache does.
    pub blocks: Vec<i16>,
}

/// One target's running block: its device channels and each channel's min and max so far.
struct Tap {
    key: String,
    chans: Vec<usize>,
    lo: Vec<f32>,
    hi: Vec<f32>,
    count: usize,
}

/// Measures each target's channels for meters and the live waveform.
pub struct LiveTap {
    taps: Vec<Tap>,
    device_channels: usize,
}

impl LiveTap {
    pub fn new(targets: &[(RecordTarget, Vec<usize>)], device_channels: usize) -> Self {
        Self {
            taps: targets
                .iter()
                .map(|(t, c)| Tap {
                    key: t.key.clone(),
                    chans: c.iter().copied().take(2).collect(),
                    lo: vec![f32::MAX; c.len().min(2)],
                    hi: vec![f32::MIN; c.len().min(2)],
                    count: 0,
                })
                .collect(),
            device_channels: device_channels.max(1),
        }
    }

    pub fn add(&mut self, t: &RecordTarget, c: &[usize]) {
        self.taps.push(Tap {
            key: t.key.clone(),
            chans: c.iter().copied().take(2).collect(),
            lo: vec![f32::MAX; c.len().min(2)],
            hi: vec![f32::MIN; c.len().min(2)],
            count: 0,
        });
    }

    pub fn remove(&mut self, key: &str) {
        self.taps.retain(|t| t.key != key);
    }

    /// Adds whole frames; finished blocks and peaks go into `live`.
    pub fn push(&mut self, interleaved: &[f32], live: &mut HashMap<String, Live>) {
        let n = self.device_channels;
        for tap in &mut self.taps {
            let entry = live.entry(tap.key.clone()).or_default();
            entry.channels = tap.chans.len();
            for frame in interleaved.chunks_exact(n) {
                for (k, &c) in tap.chans.iter().enumerate() {
                    let v = frame.get(c).copied().unwrap_or(0.0);
                    tap.lo[k] = tap.lo[k].min(v);
                    tap.hi[k] = tap.hi[k].max(v);
                    entry.peak = entry.peak.max(v.abs());
                }
                tap.count += 1;
                if tap.count == LIVE_BLOCK {
                    for k in 0..tap.chans.len() {
                        entry.blocks.push(crate::peaks::to_i16(tap.lo[k]));
                        entry.blocks.push(crate::peaks::to_i16(tap.hi[k]));
                        tap.lo[k] = f32::MAX;
                        tap.hi[k] = f32::MIN;
                    }
                    tap.count = 0;
                }
            }
        }
    }
}

/// The chain a monitored track's player hears: the track's own built-in
/// effects. A VST3 plug-in is left out, since it runs in another process
/// and its delay would be heard, and so is an effect that listens to
/// another track.
fn monitor_chain(chain: Option<&EffectChain>, rate: u32) -> Option<ChainProcessor> {
    let mut c = chain?.clone();
    c.slots
        .retain(|s| !crate::plugins::is_plugin(&s.effect) && s.sidechain.is_none());
    ChainProcessor::new(&c, rate)
}

/// Mixes the monitored targets' channels to stereo for the speakers. A
/// mono target goes to both sides; a stereo one keeps its sides. A target
/// with a Voice Chain is heard through its built-in effects.
pub struct MonitorMix {
    picks: Vec<Vec<usize>>,
    chains: Vec<Option<ChainProcessor>>,
    device_channels: usize,
    side: Vec<f32>,
    sum: Vec<f32>,
}

impl MonitorMix {
    /// None when no target asks to be monitored.
    pub fn new(
        targets: &[(RecordTarget, Vec<usize>)],
        device_channels: usize,
        rate: u32,
    ) -> Option<Self> {
        let monitored: Vec<&(RecordTarget, Vec<usize>)> = targets
            .iter()
            .filter(|(t, c)| t.monitor && !c.is_empty())
            .collect();
        let picks: Vec<Vec<usize>> = monitored.iter().map(|(_, c)| c.clone()).collect();
        let chains = monitored
            .iter()
            .map(|(t, _)| monitor_chain(t.chain.as_ref(), rate))
            .collect();
        (!picks.is_empty()).then_some(Self {
            picks,
            chains,
            device_channels: device_channels.max(1),
            side: Vec::new(),
            sum: Vec::new(),
        })
    }

    /// Appends one stereo frame (left, right) per whole input frame.
    pub fn mix(&mut self, interleaved: &[f32], out: &mut Vec<f32>) {
        let frames = interleaved.len() / self.device_channels;
        self.sum.clear();
        self.sum.resize(frames * 2, 0.0);
        for (chans, chain) in self.picks.iter().zip(self.chains.iter_mut()) {
            self.side.clear();
            for frame in interleaved.chunks_exact(self.device_channels) {
                let at = |i: usize| {
                    chans
                        .get(i)
                        .and_then(|&c| frame.get(c))
                        .copied()
                        .unwrap_or(0.0)
                };
                if chans.len() == 1 {
                    self.side.push(at(0));
                    self.side.push(at(0));
                } else {
                    self.side.push(at(0));
                    self.side.push(at(1));
                }
            }
            if let Some(c) = chain {
                c.process(&mut self.side);
            }
            for (s, v) in self.sum.iter_mut().zip(&self.side) {
                *s += *v;
            }
        }
        out.extend(self.sum.iter().map(|v| v.clamp(-1.0, 1.0)));
    }
}

/// Most monitor audio allowed to wait for the speakers, in seconds. More
/// than this and new audio is dropped, so the delay cannot creep up.
const MONITOR_MAX_QUEUE: f64 = 0.08;

struct Shared {
    stop: AtomicBool,
    /// Why monitoring could not start, if it could not.
    monitor_note: Mutex<Option<String>>,
    live: Mutex<HashMap<String, Live>>,
    /// Frames captured by the first device, for the elapsed time.
    frames: AtomicU64,
    rate: AtomicU32,
    /// Input peak since the last status call, as f32 bits.
    peak: AtomicU32,
    overruns: AtomicU64,
}

struct Running {
    shared: Arc<Shared>,
    /// False when only metering armed inputs.
    writing: bool,
    threads: Vec<JoinHandle<Result<Vec<Take>, String>>>,
    /// Each open device and the line to its thread, for tracks armed or
    /// disarmed while recording.
    devices: Vec<(Option<String>, mpsc::Sender<Cmd>)>,
    dir: Option<PathBuf>,
    started: Instant,
    /// Whether a device thread keeps the recording's clock yet.
    clock: bool,
}

/// A change to what a running device thread records.
enum Cmd {
    Add(RecordTarget, Vec<usize>),
    Remove(String),
}

/// One input device and the targets (with their channels) it feeds.
type DeviceGroup = (Option<String>, Vec<(RecordTarget, Vec<usize>)>);

/// Recording state, as the interface shows it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordStatus {
    pub recording: bool,
    /// Armed inputs are open for their meters, without recording.
    pub monitoring: bool,
    /// Device sample rate of the first input.
    pub rate: u32,
    /// Per target: level and new live waveform blocks.
    pub live: HashMap<String, Live>,
    pub seconds: f64,
    /// Input peak since the last call, 0 to 1.
    pub peak: f32,
    pub overruns: u64,
    /// Set when input monitoring was asked for but could not start.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub monitor_note: Option<String>,
}

#[derive(Default)]
pub struct Recorder {
    current: Mutex<Option<Running>>,
}

impl Recorder {
    /// Starts recording every target that has an input into `dir`. With
    /// no `dir`, only opens the inputs for their meters.
    pub fn start(&self, targets: Vec<RecordTarget>, dir: Option<PathBuf>) -> Result<(), String> {
        let mut cur = self.current.lock().map_err(|e| e.to_string())?;
        match cur.as_ref() {
            Some(r) if r.writing => return Err("Already recording.".into()),
            Some(_) => {
                // Close the meters first: some systems open an input only once.
                if let Some(r) = cur.take() {
                    r.shared.stop.store(true, Ordering::SeqCst);
                    for t in r.threads {
                        let _ = t.join();
                    }
                }
            }
            None => {}
        }
        let writing = dir.is_some();
        let asked = !targets.is_empty();
        // Group targets by device.
        let mut groups: Vec<DeviceGroup> = Vec::new();
        for t in targets {
            let Some(r) = route(t.input.as_deref(), t.channels) else {
                continue;
            };
            match groups.iter_mut().find(|(d, _)| *d == r.device) {
                Some((_, v)) => v.push((t, r.channels)),
                None => groups.push((r.device, vec![(t, r.channels)])),
            }
        }
        // Recording may start with nothing armed: tracks armed later start
        // their takes then. Metering needs something to meter.
        if groups.is_empty() && (asked || !writing) {
            return Err("No armed track has an input. Pick one on the track's input menu.".into());
        }
        let shared = Arc::new(Shared {
            stop: AtomicBool::new(false),
            monitor_note: Mutex::new(None),
            live: Mutex::new(HashMap::new()),
            frames: AtomicU64::new(0),
            rate: AtomicU32::new(0),
            peak: AtomicU32::new(0),
            overruns: AtomicU64::new(0),
        });
        let started = Instant::now();
        let mut running = Running {
            shared: shared.clone(),
            writing,
            threads: Vec::new(),
            devices: Vec::new(),
            dir: dir.clone(),
            started,
            clock: false,
        };
        for (device, list) in groups {
            if let Err(e) = spawn_device(&mut running, device, list, false) {
                shared.stop.store(true, Ordering::SeqCst);
                for t in running.threads {
                    let _ = t.join();
                }
                return Err(e);
            }
        }
        *cur = Some(running);
        Ok(())
    }

    /// While recording, starts a take for a track armed just now, from
    /// this moment. Opens its input if no other track is using it.
    pub fn add(&self, target: RecordTarget) -> Result<(), String> {
        let mut cur = self.current.lock().map_err(|e| e.to_string())?;
        let Some(r) = cur.as_mut().filter(|r| r.writing) else {
            return Err("Not recording.".into());
        };
        let Some(route) = route(target.input.as_deref(), target.channels) else {
            return Ok(());
        };
        if let Some((_, tx)) = r.devices.iter().find(|(d, _)| *d == route.device) {
            return tx
                .send(Cmd::Add(target, route.channels))
                .map_err(|_| "The recording input stopped.".to_string());
        }
        spawn_device(r, route.device, vec![(target, route.channels)], true)
    }

    /// While recording, ends the take of a track disarmed just now. The
    /// take is kept and placed when recording stops.
    pub fn remove(&self, key: &str) -> Result<(), String> {
        let cur = self.current.lock().map_err(|e| e.to_string())?;
        if let Some(r) = cur.as_ref().filter(|r| r.writing) {
            for (_, tx) in &r.devices {
                let _ = tx.send(Cmd::Remove(key.to_string()));
            }
        }
        Ok(())
    }

    /// Closes the meters, if only metering. Recording is left alone.
    pub fn stop_monitor(&self) {
        let Ok(mut cur) = self.current.lock() else {
            return;
        };
        if cur.as_ref().is_some_and(|r| !r.writing) {
            if let Some(r) = cur.take() {
                r.shared.stop.store(true, Ordering::SeqCst);
                for t in r.threads {
                    let _ = t.join();
                }
            }
        }
    }

    /// Stops and returns the takes.
    pub fn stop(&self) -> Result<Vec<Take>, String> {
        let mut cur = self.current.lock().map_err(|e| e.to_string())?;
        let Some(r) = cur.take() else {
            return Ok(Vec::new());
        };
        r.shared.stop.store(true, Ordering::SeqCst);
        let mut takes = Vec::new();
        let mut first_err = None;
        for t in r.threads {
            match t.join() {
                Ok(Ok(v)) => takes.extend(v),
                Ok(Err(e)) => first_err = first_err.or(Some(e)),
                Err(_) => {
                    first_err =
                        first_err.or(Some("The recording thread stopped unexpectedly.".into()))
                }
            }
        }
        match first_err {
            Some(e) if takes.is_empty() => Err(e),
            _ => Ok(takes),
        }
    }

    pub fn status(&self) -> RecordStatus {
        let idle = RecordStatus {
            recording: false,
            monitoring: false,
            rate: 0,
            live: HashMap::new(),
            seconds: 0.0,
            peak: 0.0,
            overruns: 0,
            monitor_note: None,
        };
        let Ok(cur) = self.current.lock() else {
            return idle;
        };
        let Some(r) = cur.as_ref() else {
            return idle;
        };
        let rate = r.shared.rate.load(Ordering::Relaxed).max(1);
        let live = r
            .shared
            .live
            .lock()
            .map(|mut m| std::mem::take(&mut *m))
            .unwrap_or_default();
        // Before any input is open (recording started with nothing
        // armed), time is counted by the clock.
        let seconds = if r.clock {
            r.shared.frames.load(Ordering::Relaxed) as f64 / f64::from(rate)
        } else {
            r.started.elapsed().as_secs_f64()
        };
        RecordStatus {
            recording: r.writing,
            monitoring: !r.writing,
            rate: if r.clock { rate } else { 0 },
            live,
            seconds,
            peak: f32::from_bits(r.shared.peak.swap(0, Ordering::Relaxed)),
            overruns: r.shared.overruns.load(Ordering::Relaxed),
            monitor_note: r.shared.monitor_note.lock().ok().and_then(|n| n.clone()),
        }
    }
}

/// Opens one input device on its own thread for `list`, and waits until it
/// runs. `late` for a device opened while recording: its takes start
/// where the recording has got to.
fn spawn_device(
    r: &mut Running,
    device: Option<String>,
    list: Vec<(RecordTarget, Vec<usize>)>,
    late: bool,
) -> Result<(), String> {
    let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();
    let (cmd_tx, cmd_rx) = mpsc::channel::<Cmd>();
    let s = r.shared.clone();
    let dir = r.dir.clone();
    // The first device opened keeps the recording's clock.
    let first = !r.clock;
    let since = if late { Some(r.started) } else { None };
    let dev = device.clone();
    let handle = thread::Builder::new()
        .name("record".into())
        .spawn(move || run(dev, list, dir, s, first, since, cmd_rx, ready_tx))
        .map_err(|e| e.to_string())?;
    match ready_rx.recv_timeout(Duration::from_secs(5)) {
        Ok(Ok(())) => {
            r.threads.push(handle);
            r.devices.push((device, cmd_tx));
            if first {
                r.clock = true;
            }
            Ok(())
        }
        Ok(Err(e)) => {
            let _ = handle.join();
            Err(e)
        }
        Err(_) => Err("The audio input did not respond.".into()),
    }
}

/// Inputs named with this prefix record what the computer plays (system
/// audio): a call, a video, anything coming out of that output. Windows
/// only, where the system supports it directly.
pub const SYSTEM_AUDIO_PREFIX: &str = "System audio: ";

/// The device for `name`, and whether it captures system audio.
fn find_device(name: Option<&str>) -> Result<(cpal::Device, bool), String> {
    let host = crate::engine::host::host();
    match name {
        None => host
            .default_input_device()
            .map(|d| (d, false))
            .ok_or_else(|| {
                "No audio input found. Plug in a microphone or pick an input on the track."
                    .to_string()
            }),
        Some(n) => {
            if let Some(out) = n.strip_prefix(SYSTEM_AUDIO_PREFIX) {
                if !cfg!(windows) {
                    return Err("Recording system audio works on Windows only for now.".into());
                }
                return host
                    .output_devices()
                    .map_err(|e| e.to_string())?
                    .find(|d| d.description().ok().is_some_and(|x| x.name() == out))
                    .map(|d| (d, true))
                    .ok_or_else(|| format!("The output \"{out}\" is not connected."));
            }
            host.input_devices()
                .map_err(|e| e.to_string())?
                .find(|d| d.description().ok().is_some_and(|x| x.name() == n))
                .map(|d| (d, false))
                .ok_or_else(|| format!("The input \"{n}\" is not connected."))
        }
    }
}

/// Frames of silence to add so a system-audio take keeps time. Windows
/// sends nothing while nothing plays, so without this the quiet parts
/// would vanish and the take would drift out of sync with the mics. Pads
/// only once more than 100 ms is missing, and stops 50 ms short so the
/// sound arriving next is never pushed late.
fn silence_to_add(elapsed_frames: u64, got_frames: u64, rate: u32) -> u64 {
    let behind = elapsed_frames.saturating_sub(got_frames);
    if behind > u64::from(rate) / 10 {
        behind - u64::from(rate) / 20
    } else {
        0
    }
}

#[allow(clippy::too_many_arguments)]
fn run(
    device: Option<String>,
    targets: Vec<(RecordTarget, Vec<usize>)>,
    dir: Option<PathBuf>,
    shared: Arc<Shared>,
    first: bool,
    since: Option<Instant>,
    commands: mpsc::Receiver<Cmd>,
    ready: mpsc::Sender<Result<(), String>>,
) -> Result<Vec<Take>, String> {
    let fail = |e: String| {
        let _ = ready.send(Err(e.clone()));
        Err(e)
    };
    let (dev, loopback) = match find_device(device.as_deref()) {
        Ok(d) => d,
        Err(e) => return fail(e),
    };
    let config = if loopback {
        dev.default_output_config()
    } else {
        dev.default_input_config()
    };
    let supported = match config {
        Ok(c) => c,
        Err(e) => return fail(format!("The audio input is not available: {e}")),
    };
    let format = supported.sample_format();
    let config: StreamConfig = supported.config();
    let rate = config.sample_rate;
    let channels = usize::from(config.channels);
    // A device opened while recording starts where the recording is.
    let base = since.map_or(0, |t| (t.elapsed().as_secs_f64() * f64::from(rate)) as u64);
    let mut writer = match dir {
        Some(d) => match TakeWriter::create(&d, &[], rate, channels) {
            Ok(mut w) => {
                for (t, c) in &targets {
                    if let Err(e) = w.add(t, c, base) {
                        return fail(format!("Could not create the recording file: {e}"));
                    }
                }
                Some(w)
            }
            Err(e) => return fail(format!("Could not create the recording file: {e}")),
        },
        None => None,
    };
    let mut tap = LiveTap::new(&targets, channels);
    // Two seconds of headroom between the device and the disk.
    let (producer, mut consumer) = RingBuffer::<f32>::new((rate as usize * channels * 2).max(8192));
    let stream = match format {
        SampleFormat::F32 => build::<f32>(&dev, config, producer, shared.clone()),
        SampleFormat::I16 => build::<i16>(&dev, config, producer, shared.clone()),
        SampleFormat::I32 => build::<i32>(&dev, config, producer, shared.clone()),
        SampleFormat::U16 => build::<u16>(&dev, config, producer, shared.clone()),
        other => Err(format!("Unsupported input sample format: {other}")),
    };
    let stream = match stream.and_then(|s| s.play().map(|_| s).map_err(|e| e.to_string())) {
        Ok(s) => s,
        Err(e) => return fail(format!("Could not open the input: {e}")),
    };
    if first {
        shared.rate.store(rate, Ordering::Relaxed);
        shared.frames.store(base, Ordering::Relaxed);
    }
    let _ = ready.send(Ok(()));

    // Input monitoring. A system-audio take is already what the speakers
    // play, so it is never sent back to them.
    let mut mix = if loopback {
        None
    } else {
        MonitorMix::new(&targets, channels, rate)
    };
    // An ASIO driver opens for one program at a time, so ASIO monitoring
    // plays on the input's own driver.
    let input_name = dev.description().ok().map(|d| d.name().to_string());
    let mut monitor: Option<(Producer<f32>, cpal::Stream)> = None;
    if mix.is_some() {
        match open_monitor(rate, input_name.as_deref()) {
            Ok(v) => monitor = Some(v),
            Err(e) => {
                if let Ok(mut n) = shared.monitor_note.lock() {
                    *n = Some(e);
                }
            }
        }
    }
    let max_queue = (MONITOR_MAX_QUEUE * f64::from(rate)) as usize * 2;
    let mut mixed: Vec<f32> = Vec::new();

    let mut block = vec![0f32; 4096 * channels];
    let mut drain = |consumer: &mut Consumer<f32>,
                     writer: &mut Option<TakeWriter>,
                     tap: &mut LiveTap,
                     monitor: &mut Option<(Producer<f32>, cpal::Stream)>|
     -> Result<u64, String> {
        let mut frames = 0u64;
        loop {
            let n = consumer.slots().min(block.len()) / channels * channels;
            if n == 0 {
                return Ok(frames);
            }
            frames += (n / channels) as u64;
            for v in block[..n].iter_mut() {
                *v = consumer.pop().unwrap_or(0.0);
            }
            if let Some(w) = writer.as_mut() {
                w.push(&block[..n])?;
                if first {
                    shared
                        .frames
                        .fetch_add((n / channels) as u64, Ordering::Relaxed);
                }
            }
            if let Ok(mut live) = shared.live.lock() {
                tap.push(&block[..n], &mut live);
            }
            if let (Some(m), Some((p, _))) = (mix.as_mut(), monitor.as_mut()) {
                mixed.clear();
                m.mix(&block[..n], &mut mixed);
                let queued = p.buffer().capacity() - p.slots();
                if queued + mixed.len() <= max_queue.max(mixed.len()) {
                    for &v in &mixed {
                        if p.push(v).is_err() {
                            break;
                        }
                    }
                }
            }
        }
    };
    let started = Instant::now();
    let mut got = 0u64;
    let mut quiet = Vec::new();
    let mut result = Ok(0);
    while !shared.stop.load(Ordering::SeqCst) {
        match drain(&mut consumer, &mut writer, &mut tap, &mut monitor) {
            Ok(n) => got += n,
            Err(e) => {
                result = Err(e);
                break;
            }
        }
        // Tracks armed or disarmed while recording.
        while let Ok(cmd) = commands.try_recv() {
            match cmd {
                Cmd::Add(t, c) => {
                    if let Some(w) = writer.as_mut() {
                        if !w.is_recording(&t.key) {
                            if let Err(e) = w.add(&t, &c, base + got) {
                                if let Ok(mut n) = shared.monitor_note.lock() {
                                    *n = Some(format!("Could not start recording {}: {e}", t.name));
                                }
                                continue;
                            }
                        }
                    }
                    tap.remove(&t.key);
                    tap.add(&t, &c);
                }
                Cmd::Remove(key) => {
                    if let Some(w) = writer.as_mut() {
                        if let Err(e) = w.close(&key) {
                            result = Err(e);
                        }
                    }
                    tap.remove(&key);
                }
            }
        }
        if loopback {
            let elapsed = (started.elapsed().as_secs_f64() * f64::from(rate)) as u64;
            let pad = silence_to_add(elapsed, got, rate);
            if pad > 0 {
                quiet.clear();
                quiet.resize(pad as usize * channels, 0.0);
                if let Some(w) = writer.as_mut() {
                    if let Err(e) = w.push(&quiet) {
                        result = Err(e);
                        break;
                    }
                    if first {
                        shared.frames.fetch_add(pad, Ordering::Relaxed);
                    }
                }
                if let Ok(mut live) = shared.live.lock() {
                    tap.push(&quiet, &mut live);
                }
                got += pad;
            }
        }
        thread::sleep(Duration::from_millis(10));
    }
    drop(stream);
    if result.is_ok() {
        result = drain(&mut consumer, &mut writer, &mut tap, &mut monitor);
    }
    drop(monitor);
    let takes = match writer {
        Some(w) => w.finish()?,
        None => Vec::new(),
    };
    result.map(|_| takes)
}

/// Opens the default output at the input's rate for monitoring. Returns
/// the ring that feeds it (stereo frames) and the running stream.
fn open_monitor(
    rate: u32,
    input_name: Option<&str>,
) -> Result<(Producer<f32>, cpal::Stream), String> {
    let host = crate::engine::host::host();
    // With ASIO, the output of the driver the input is on; the output picked
    // in Settings may belong to another driver, which cannot be opened too.
    let same_driver = if crate::engine::host::asio() {
        input_name.and_then(|n| {
            host.output_devices()
                .ok()?
                .find(|d| d.description().ok().is_some_and(|x| x.name() == n))
        })
    } else {
        None
    };
    let dev = same_driver
        .or_else(|| super::output::output_device(&host))
        .ok_or("Input monitoring needs an output: no speakers or headphones found.")?;
    let mut best: Option<cpal::SupportedStreamConfig> = None;
    for c in dev.supported_output_configs().map_err(|e| e.to_string())? {
        if c.min_sample_rate() > rate || c.max_sample_rate() < rate {
            continue;
        }
        let score = |c: &cpal::SupportedStreamConfigRange| {
            (c.channels() >= 2, c.sample_format() == SampleFormat::F32)
        };
        let better = best.as_ref().is_none_or(|b| {
            let bb = (b.channels() >= 2, b.sample_format() == SampleFormat::F32);
            score(&c) > bb
        });
        if better {
            best = Some(c.with_sample_rate(rate));
        }
    }
    let supported = best.ok_or_else(|| {
        format!("Input monitoring is off: the output does not play at {rate} Hz, the input's rate. Set both to the same rate in the system sound settings.")
    })?;
    let format = supported.sample_format();
    let config: StreamConfig = supported.config();
    // A second of room; the writer keeps it far emptier than that.
    let (producer, consumer) = RingBuffer::<f32>::new((rate as usize * 2).max(8192));
    let stream = match format {
        SampleFormat::F32 => build_out::<f32>(&dev, config, consumer),
        SampleFormat::I16 => build_out::<i16>(&dev, config, consumer),
        SampleFormat::I32 => build_out::<i32>(&dev, config, consumer),
        SampleFormat::U16 => build_out::<u16>(&dev, config, consumer),
        other => Err(format!("Unsupported output sample format: {other}")),
    }?;
    stream.play().map_err(|e| e.to_string())?;
    Ok((producer, stream))
}

fn build_out<T>(
    device: &cpal::Device,
    config: StreamConfig,
    mut consumer: Consumer<f32>,
) -> Result<cpal::Stream, String>
where
    T: SizedSample + FromSample<f32>,
{
    let out_channels = usize::from(config.channels).max(1);
    device
        .build_output_stream(
            config,
            move |data: &mut [T], _| {
                // Audio thread: copy only. Silence when the ring runs dry.
                for frame in data.chunks_mut(out_channels) {
                    let (l, r) = if consumer.slots() >= 2 {
                        (consumer.pop().unwrap_or(0.0), consumer.pop().unwrap_or(0.0))
                    } else {
                        (0.0, 0.0)
                    };
                    for (i, s) in frame.iter_mut().enumerate() {
                        let v = match (out_channels, i) {
                            (1, _) => (l + r) * 0.5,
                            (_, 0) => l,
                            (_, 1) => r,
                            _ => 0.0,
                        };
                        *s = T::from_sample(v);
                    }
                }
            },
            |e| eprintln!("monitor stream error: {e}"),
            None,
        )
        .map_err(|e| e.to_string())
}

fn build<T>(
    device: &cpal::Device,
    config: StreamConfig,
    mut producer: Producer<f32>,
    shared: Arc<Shared>,
) -> Result<cpal::Stream, String>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    device
        .build_input_stream(
            config,
            move |data: &[T], _| {
                // Audio thread: copy only. No allocation, no locks, no disk.
                let mut peak = 0f32;
                for &s in data {
                    let v: f32 = s.to_sample();
                    peak = peak.max(v.abs());
                    if producer.push(v).is_err() {
                        shared.overruns.fetch_add(1, Ordering::Relaxed);
                        break;
                    }
                }
                shared.peak.fetch_max(peak.to_bits(), Ordering::Relaxed);
            },
            |e| eprintln!("record stream error: {e}"),
            None,
        )
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests;
