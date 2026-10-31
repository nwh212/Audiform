// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Preview playback for Drive Manager and Libraries: one file at a
//! time on the default output device. Also short generated sounds, such
//! as a note played through a MIDI track's instrument while editing.
//!
//! Three threads:
//! - the caller, which starts and stops playback;
//! - a decode thread, which reads the file, converts it to the device's
//!   rate and channel count, and fills a lock-free ring buffer;
//! - the device's audio thread, which only copies from the ring buffer.
//!   It never allocates, locks, or touches the disk.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, StreamConfig};
use rtrb::{Consumer, Producer, RingBuffer};
use serde::Serialize;

use crate::codecs::AudioReader;

/// Seconds of audio buffered ahead of the device.
const BUFFER_SECONDS: f64 = 0.5;

/// State shared between the three threads.
#[derive(Default)]
struct Shared {
    stop: AtomicBool,
    /// Decode thread reached the end of the file.
    decoded_all: AtomicBool,
    /// Device has played everything.
    finished: AtomicBool,
    /// Frames sent to the device since playback started.
    played: AtomicU64,
    /// Highest sample per channel since the last status call, as f32 bits.
    peak: [AtomicU32; 2],
}

struct Running {
    path: PathBuf,
    shared: Arc<Shared>,
    start_seconds: f64,
    rate: u32,
    thread: Option<JoinHandle<()>>,
}

impl Running {
    fn stop(&mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// What the interface shows for preview playback.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewStatus {
    pub playing: bool,
    /// Highest sample left and right since the last status, 0 to 1.
    pub peak: [f32; 2],
    pub path: Option<PathBuf>,
    /// Seconds from the start of the file.
    pub position: f64,
}

/// Fills a block with interleaved samples at the device's rate and
/// channel count; false once there is nothing more.
pub type Fill = Box<dyn FnMut(&mut Vec<f32>) -> bool + Send>;

/// Makes a [`Fill`] once the device's channel count and rate are known.
pub type MakeFill = Box<dyn FnOnce(usize, u32) -> Result<Fill, String> + Send>;

/// What a preview plays.
enum Input {
    File(PathBuf, f64),
    Made(MakeFill),
}

/// Owns the current preview, if any. Kept in Tauri state.
#[derive(Default)]
pub struct PreviewPlayer {
    current: Mutex<Option<Running>>,
}

impl PreviewPlayer {
    /// Stops any preview and starts `path` at `start_seconds`.
    pub fn play(&self, path: &Path, start_seconds: f64) -> Result<(), String> {
        self.start(
            Input::File(path.to_path_buf(), start_seconds),
            path,
            start_seconds,
        )
    }

    /// Stops any preview and plays generated sound (see [`MakeFill`]).
    pub fn play_made(&self, make: MakeFill) -> Result<(), String> {
        self.start(Input::Made(make), Path::new(""), 0.0)
    }

    fn start(&self, input: Input, path: &Path, start_seconds: f64) -> Result<(), String> {
        let mut current = self.current.lock().map_err(|e| e.to_string())?;
        if let Some(mut r) = current.take() {
            r.stop();
        }

        let shared = Arc::new(Shared::default());
        let (ready_tx, ready_rx) = mpsc::channel::<Result<u32, String>>();
        let thread_shared = shared.clone();
        let thread = thread::Builder::new()
            .name("preview".into())
            .spawn(move || run(input, thread_shared, ready_tx))
            .map_err(|e| e.to_string())?;

        // Wait until the device is open so errors reach the caller.
        let rate = match ready_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(rate)) => rate,
            Ok(Err(e)) => {
                let _ = thread.join();
                return Err(e);
            }
            Err(_) => {
                shared.stop.store(true, Ordering::SeqCst);
                return Err("The audio device did not respond.".into());
            }
        };
        *current = Some(Running {
            path: path.to_path_buf(),
            shared,
            start_seconds,
            rate,
            thread: Some(thread),
        });
        Ok(())
    }

    pub fn stop(&self) {
        if let Ok(mut current) = self.current.lock() {
            if let Some(mut r) = current.take() {
                r.stop();
            }
        }
    }

    pub fn status(&self) -> PreviewStatus {
        let Ok(mut current) = self.current.lock() else {
            return PreviewStatus {
                playing: false,
                peak: [0.0, 0.0],
                path: None,
                position: 0.0,
            };
        };
        if current
            .as_ref()
            .is_some_and(|r| r.shared.finished.load(Ordering::SeqCst))
        {
            if let Some(mut r) = current.take() {
                r.stop();
            }
        }
        match current.as_ref() {
            Some(r) => PreviewStatus {
                playing: true,
                peak: [
                    f32::from_bits(r.shared.peak[0].swap(0, Ordering::Relaxed)),
                    f32::from_bits(r.shared.peak[1].swap(0, Ordering::Relaxed)),
                ],
                path: Some(r.path.clone()),
                position: r.start_seconds
                    + r.shared.played.load(Ordering::Relaxed) as f64 / f64::from(r.rate.max(1)),
            },
            None => PreviewStatus {
                playing: false,
                peak: [0.0, 0.0],
                path: None,
                position: 0.0,
            },
        }
    }
}

impl Drop for PreviewPlayer {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Body of the preview thread. Owns the device stream, which on some
/// platforms must stay on the thread that created it.
fn run(input: Input, shared: Arc<Shared>, ready: mpsc::Sender<Result<u32, String>>) {
    let Some(device) = super::output::output_device_any() else {
        let _ = ready.send(Err("No audio output device found.".into()));
        return;
    };
    let supported = match device.default_output_config() {
        Ok(c) => super::host::sane_config(c),
        Err(e) => {
            let _ = ready.send(Err(format!("The audio device is not available: {e}")));
            return;
        }
    };
    let format = supported.sample_format();
    let mut config: StreamConfig = supported.config();
    config.buffer_size = super::host::buffer_size();
    super::host::log(&super::host::describe("preview", &device, &config, format));
    let rate = config.sample_rate;
    let channels = usize::from(config.channels);

    let source: Result<Fill, String> = match input {
        Input::File(path, start) => file_fill(&path, start, channels, rate),
        Input::Made(make) => make(channels, rate),
    };
    let source = match source {
        Ok(s) => s,
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };

    let capacity = ((f64::from(rate) * BUFFER_SECONDS) as usize).max(4096) * channels;
    let (producer, consumer) = RingBuffer::<f32>::new(capacity);

    let decode_shared = shared.clone();
    let decoder = thread::Builder::new()
        .name("preview-decode".into())
        .spawn(move || decode_loop(source, producer, decode_shared));
    let decoder = match decoder {
        Ok(d) => d,
        Err(e) => {
            let _ = ready.send(Err(e.to_string()));
            return;
        }
    };

    let stream = match format {
        SampleFormat::F32 => build::<f32>(&device, config, consumer, shared.clone(), channels),
        SampleFormat::I16 => build::<i16>(&device, config, consumer, shared.clone(), channels),
        SampleFormat::I32 => build::<i32>(&device, config, consumer, shared.clone(), channels),
        SampleFormat::U16 => build::<u16>(&device, config, consumer, shared.clone(), channels),
        other => Err(format!("Unsupported output sample format: {other}")),
    };
    let stream = match stream.and_then(|s| s.play().map(|_| s).map_err(|e| e.to_string())) {
        Ok(s) => s,
        Err(e) => {
            shared.stop.store(true, Ordering::SeqCst);
            let _ = decoder.join();
            let _ = ready.send(Err(format!("Could not start playback: {e}")));
            return;
        }
    };
    let _ = ready.send(Ok(rate));

    while !shared.stop.load(Ordering::SeqCst) && !shared.finished.load(Ordering::SeqCst) {
        thread::sleep(Duration::from_millis(15));
    }
    drop(stream);
    shared.stop.store(true, Ordering::SeqCst);
    let _ = decoder.join();
}

/// A file, read at the device's rate and channel count from `start` seconds.
fn file_fill(path: &Path, start: f64, channels: usize, rate: u32) -> Result<Fill, String> {
    let mut reader = AudioReader::open(path, channels, Some(rate)).map_err(|e| e.to_string())?;
    if start > 0.0 {
        reader.seek(start).map_err(|e| e.to_string())?;
    }
    Ok(Box::new(move |block: &mut Vec<f32>| {
        matches!(reader.read_into(block), Ok(true))
    }))
}

fn decode_loop(mut source: Fill, mut producer: Producer<f32>, shared: Arc<Shared>) {
    let mut block = Vec::with_capacity(8192);
    loop {
        if shared.stop.load(Ordering::SeqCst) {
            return;
        }
        block.clear();
        if !source(&mut block) {
            shared.decoded_all.store(true, Ordering::SeqCst);
            return;
        }
        let mut rest: &[f32] = &block;
        while !rest.is_empty() {
            if shared.stop.load(Ordering::SeqCst) {
                return;
            }
            let (_, left) = producer.push_partial_slice(rest);
            rest = left;
            if !rest.is_empty() {
                thread::sleep(Duration::from_millis(5));
            }
        }
    }
}

fn build<T>(
    device: &cpal::Device,
    config: StreamConfig,
    mut consumer: Consumer<f32>,
    shared: Arc<Shared>,
    channels: usize,
) -> Result<cpal::Stream, String>
where
    T: SizedSample + FromSample<f32>,
{
    device
        .build_output_stream(
            config,
            move |out: &mut [T], _| {
                super::host::guard_out(out, |out| fill(out, &mut consumer, &shared, channels));
            },
            |e| super::host::log(&format!("preview stream error: {e}")),
            None,
        )
        .map_err(|e| e.to_string())
}

/// Audio thread body: copy from the ring buffer, silence on underrun.
fn fill<T>(out: &mut [T], consumer: &mut Consumer<f32>, shared: &Shared, channels: usize)
where
    T: SizedSample + FromSample<f32>,
{
    let mut real = 0usize;
    let mut peak = [0f32; 2];
    let ch = channels.max(1);
    for (i, s) in out.iter_mut().enumerate() {
        match consumer.pop() {
            Ok(v) => {
                *s = T::from_sample(v);
                real += 1;
                let side = if ch == 1 { 0 } else { (i % ch).min(1) };
                peak[side] = peak[side].max(v.abs());
            }
            Err(_) => *s = T::from_sample(0.0f32),
        }
    }
    if ch == 1 {
        peak[1] = peak[0];
    }
    // Non-negative floats order the same as their bits, so fetch_max works.
    shared.peak[0].fetch_max(peak[0].to_bits(), Ordering::Relaxed);
    shared.peak[1].fetch_max(peak[1].to_bits(), Ordering::Relaxed);
    shared
        .played
        .fetch_add((real / channels.max(1)) as u64, Ordering::Relaxed);
    if real == 0 && shared.decoded_all.load(Ordering::SeqCst) {
        shared.finished.store(true, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_copies_then_pads_with_silence_and_finishes() {
        let (mut p, mut c) = RingBuffer::<f32>::new(16);
        for v in [0.1, 0.2, 0.3, 0.4] {
            p.push(v).unwrap();
        }
        let shared = Shared::default();
        let mut out = [9.0f32; 6];
        fill(&mut out, &mut c, &shared, 2);
        assert_eq!(out, [0.1, 0.2, 0.3, 0.4, 0.0, 0.0]);
        assert_eq!(shared.played.load(Ordering::Relaxed), 2);
        assert_eq!(f32::from_bits(shared.peak[0].load(Ordering::Relaxed)), 0.3);
        assert_eq!(f32::from_bits(shared.peak[1].load(Ordering::Relaxed)), 0.4);
        assert!(!shared.finished.load(Ordering::SeqCst));

        shared.decoded_all.store(true, Ordering::SeqCst);
        fill(&mut out, &mut c, &shared, 2);
        assert!(shared.finished.load(Ordering::SeqCst));
    }

    #[test]
    fn fill_converts_to_integer_formats() {
        let (mut p, mut c) = RingBuffer::<f32>::new(4);
        p.push(1.0).unwrap();
        p.push(-1.0).unwrap();
        let shared = Shared::default();
        let mut out = [0i16; 2];
        fill(&mut out, &mut c, &shared, 2);
        assert_eq!(out[0], i16::MAX);
        assert!(out[1] <= -i16::MAX);
    }

    #[test]
    fn status_is_idle_by_default() {
        let p = PreviewPlayer::default();
        assert!(!p.status().playing);
        p.stop();
    }
}
