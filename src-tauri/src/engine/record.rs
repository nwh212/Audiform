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
    takes: Vec<(
        Take,
        Vec<usize>,
        hound::WavWriter<std::io::BufWriter<std::fs::File>>,
    )>,
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
        let mut takes = Vec::new();
        for (t, chans) in targets {
            let channels = chans.len().clamp(1, 2) as u16;
            let path = crate::export::mixdown::unique_path(dir, &t.name, "wav");
            let spec = hound::WavSpec {
                channels,
                sample_rate: rate,
                bits_per_sample: 32,
                sample_format: hound::SampleFormat::Float,
            };
            let w = hound::WavWriter::create(&path, spec).map_err(|e| e.to_string())?;
            takes.push((
                Take {
                    key: t.key.clone(),
                    path,
                    sample_rate: rate,
                    channels,
                    length: 0,
                },
                chans.clone(),
                w,
            ));
        }
        Ok(Self {
            takes,
            device_channels: device_channels.max(1),
        })
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
        let mut out = Vec::new();
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

/// Mixes the monitored targets' channels to stereo for the speakers. A
/// mono target goes to both sides; a stereo one keeps its sides.
pub struct MonitorMix {
    picks: Vec<Vec<usize>>,
    device_channels: usize,
}

impl MonitorMix {
    /// None when no target asks to be monitored.
    pub fn new(targets: &[(RecordTarget, Vec<usize>)], device_channels: usize) -> Option<Self> {
        let picks: Vec<Vec<usize>> = targets
            .iter()
            .filter(|(t, c)| t.monitor && !c.is_empty())
            .map(|(_, c)| c.clone())
            .collect();
        (!picks.is_empty()).then_some(Self {
            picks,
            device_channels: device_channels.max(1),
        })
    }

    /// Appends one stereo frame (left, right) per whole input frame.
    pub fn mix(&self, interleaved: &[f32], out: &mut Vec<f32>) {
        for frame in interleaved.chunks_exact(self.device_channels) {
            let (mut l, mut r) = (0f32, 0f32);
            for chans in &self.picks {
                let at = |i: usize| {
                    chans
                        .get(i)
                        .and_then(|&c| frame.get(c))
                        .copied()
                        .unwrap_or(0.0)
                };
                if chans.len() == 1 {
                    l += at(0);
                    r += at(0);
                } else {
                    l += at(0);
                    r += at(1);
                }
            }
            out.push(l.clamp(-1.0, 1.0));
            out.push(r.clamp(-1.0, 1.0));
        }
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
        if groups.is_empty() {
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
        let mut threads = Vec::new();
        for (i, (device, list)) in groups.into_iter().enumerate() {
            let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();
            let s = shared.clone();
            let dir = dir.clone();
            let first = i == 0;
            let handle = thread::Builder::new()
                .name("record".into())
                .spawn(move || run(device, list, dir, s, first, ready_tx))
                .map_err(|e| e.to_string())?;
            match ready_rx.recv_timeout(Duration::from_secs(5)) {
                Ok(Ok(())) => threads.push(handle),
                Ok(Err(e)) => {
                    shared.stop.store(true, Ordering::SeqCst);
                    let _ = handle.join();
                    for t in threads {
                        let _ = t.join();
                    }
                    return Err(e);
                }
                Err(_) => {
                    shared.stop.store(true, Ordering::SeqCst);
                    for t in threads {
                        let _ = t.join();
                    }
                    return Err("The audio input did not respond.".into());
                }
            }
        }
        *cur = Some(Running {
            shared,
            writing,
            threads,
        });
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
        RecordStatus {
            recording: r.writing,
            monitoring: !r.writing,
            rate,
            live,
            seconds: r.shared.frames.load(Ordering::Relaxed) as f64 / f64::from(rate),
            peak: f32::from_bits(r.shared.peak.swap(0, Ordering::Relaxed)),
            overruns: r.shared.overruns.load(Ordering::Relaxed),
            monitor_note: r.shared.monitor_note.lock().ok().and_then(|n| n.clone()),
        }
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

fn run(
    device: Option<String>,
    targets: Vec<(RecordTarget, Vec<usize>)>,
    dir: Option<PathBuf>,
    shared: Arc<Shared>,
    first: bool,
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
    let mut writer = match dir {
        Some(d) => match TakeWriter::create(&d, &targets, rate, channels) {
            Ok(w) => Some(w),
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
    }
    let _ = ready.send(Ok(()));

    // Input monitoring. A system-audio take is already what the speakers
    // play, so it is never sent back to them.
    let mix = if loopback {
        None
    } else {
        MonitorMix::new(&targets, channels)
    };
    let mut monitor: Option<(Producer<f32>, cpal::Stream)> = None;
    if mix.is_some() {
        match open_monitor(rate) {
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
            if let (Some(m), Some((p, _))) = (mix.as_ref(), monitor.as_mut()) {
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
fn open_monitor(rate: u32) -> Result<(Producer<f32>, cpal::Stream), String> {
    let host = crate::engine::host::host();
    let dev = super::output::output_device(&host)
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
mod tests {
    use super::*;

    #[test]
    fn routes_follow_the_track_input() {
        assert_eq!(
            route(None, 1),
            Some(Route {
                device: None,
                channels: vec![0]
            })
        );
        assert_eq!(
            route(None, 2),
            Some(Route {
                device: None,
                channels: vec![0, 1]
            })
        );
        assert_eq!(route(Some("none"), 2), None);
        assert_eq!(
            route(Some("Mono: Yeti Stereo Microphone 2"), 1),
            Some(Route {
                device: Some("Yeti Stereo Microphone".into()),
                channels: vec![1]
            })
        );
        assert_eq!(
            route(Some("Stereo: Scarlett 2i2 USB 1-2"), 2),
            Some(Route {
                device: Some("Scarlett 2i2 USB".into()),
                channels: vec![0, 1]
            })
        );
        // Garbage falls back to the default input.
        assert_eq!(
            route(Some("Mono: nothing"), 1),
            Some(Route {
                device: None,
                channels: vec![0]
            })
        );
    }

    #[test]
    fn monitor_mixes_only_marked_targets_to_stereo() {
        let mk = |key: &str, monitor: bool| RecordTarget {
            key: key.into(),
            input: None,
            channels: 1,
            name: key.into(),
            monitor,
        };
        assert!(MonitorMix::new(&[(mk("a", false), vec![0])], 3).is_none());
        // Device has 3 channels: a mono mic on 0 (monitored), a stereo pair
        // on 1-2 (monitored), and nothing else.
        let m = MonitorMix::new(
            &[
                (mk("mic", true), vec![0]),
                (mk("pair", true), vec![1, 2]),
                (mk("quiet", false), vec![0]),
            ],
            3,
        )
        .unwrap();
        let mut out = Vec::new();
        m.mix(&[0.1, 0.2, 0.3, 0.9, 0.9, 0.9], &mut out);
        assert_eq!(out.len(), 4);
        assert!((out[0] - 0.3).abs() < 1e-6 && (out[1] - 0.4).abs() < 1e-6);
        assert_eq!(out[2], 1.0, "sums are clamped");
    }

    #[test]
    fn live_tap_measures_each_target() {
        let mk = |key: &str| RecordTarget {
            key: key.into(),
            input: None,
            channels: 1,
            name: key.into(),
            monitor: false,
        };
        let mut tap = LiveTap::new(&[(mk("a"), vec![0]), (mk("b"), vec![1])], 2);
        let mut live = HashMap::new();
        let mut frames = Vec::new();
        for i in 0..LIVE_BLOCK + 10 {
            frames.extend_from_slice(&[if i == 5 { 0.8 } else { 0.1 }, -0.25]);
        }
        tap.push(&frames, &mut live);
        assert!((live["a"].peak - 0.8).abs() < 1e-6);
        // One block of 32 frames per channel (min, max), as the peak cache would store it.
        assert_eq!(live["a"].channels, 1);
        assert_eq!(live["a"].blocks, vec![3277, 26214]);
        assert_eq!(live["b"].blocks, vec![-8192, -8192]);
    }

    #[test]
    fn writer_splits_device_channels_into_takes() {
        let dir = tempfile::tempdir().unwrap();
        let mk = |key: &str| RecordTarget {
            key: key.into(),
            input: None,
            channels: 1,
            name: key.into(),
            monitor: false,
        };
        let mut w = TakeWriter::create(
            dir.path(),
            &[
                (mk("left"), vec![0]),
                (mk("right"), vec![1]),
                (mk("both"), vec![0, 1]),
                (mk("missing"), vec![5]),
            ],
            48_000,
            2,
        )
        .unwrap();
        // Three stereo frames: (0.1, -0.1), (0.2, -0.2), (0.3, -0.3).
        w.push(&[0.1, -0.1, 0.2, -0.2, 0.3, -0.3]).unwrap();
        let takes = w.finish().unwrap();
        assert_eq!(takes.len(), 4);
        assert!(takes
            .iter()
            .all(|t| t.length == 3 && t.sample_rate == 48_000));
        let read = |t: &Take| -> Vec<f32> {
            hound::WavReader::open(&t.path)
                .unwrap()
                .samples::<f32>()
                .map(|s| s.unwrap())
                .collect()
        };
        assert_eq!(read(&takes[0]), vec![0.1, 0.2, 0.3]);
        assert_eq!(read(&takes[1]), vec![-0.1, -0.2, -0.3]);
        assert_eq!(read(&takes[2]), vec![0.1, -0.1, 0.2, -0.2, 0.3, -0.3]);
        assert_eq!(read(&takes[3]), vec![0.0, 0.0, 0.0]);
        assert_eq!(takes[2].channels, 2);
    }

    #[test]
    fn pads_system_audio_only_when_well_behind() {
        assert_eq!(silence_to_add(48_000, 48_000, 48_000), 0);
        assert_eq!(
            silence_to_add(52_000, 48_000, 48_000),
            0,
            "under 100 ms behind"
        );
        assert_eq!(silence_to_add(96_000, 48_000, 48_000), 48_000 - 2_400);
        assert_eq!(silence_to_add(0, 10, 48_000), 0);
    }
}
