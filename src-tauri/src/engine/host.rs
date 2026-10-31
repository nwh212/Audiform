// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Which audio driver the app uses: the system's own (WASAPI on Windows,
//! Core Audio on macOS, ALSA on Linux) or ASIO on Windows. ASIO is only
//! there in builds made with the `asio` feature (the release builds); the
//! system driver is always the fallback, so a missing or busy ASIO
//! driver never leaves the app without sound.
//!
//! With ASIO, one driver is in use: the one picked in Settings (or the
//! first likely one installed). Only that driver is ever loaded; the
//! others installed are listed by name from the registry and never
//! touched, since loading a driver that misbehaves can take the app down.
//! Listing devices on ASIO means asking that one driver for its channels.
//!
//! Also here: the audio log (`audio.log` in the app's config folder),
//! which records what driver, device, rate and buffer each stream opened
//! with and every fault, and the guard that keeps a fault on the audio
//! thread from closing the app.
//!
//! ASIO is a trademark of Steinberg Media Technologies GmbH. The ASIO
//! SDK is used under its GPLv3 license option.

use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;

static USE_ASIO: AtomicBool = AtomicBool::new(false);
/// The ASIO driver picked in Settings; None picks one.
static ASIO_DRIVER: Mutex<Option<String>> = Mutex::new(None);
/// Frames per ASIO buffer; 0 leaves the driver's own setting.
static ASIO_BUFFER: AtomicU32 = AtomicU32::new(0);
static LOG_PATH: Mutex<Option<PathBuf>> = Mutex::new(None);
/// Set when a fault on an audio thread was caught (see [`guard`]).
static FAULT: AtomicBool = AtomicBool::new(false);

/// True when this build can talk to ASIO drivers.
pub const ASIO_BUILT: bool = cfg!(all(target_os = "windows", feature = "asio"));

/// Buffer sizes offered for ASIO, in frames.
pub const ASIO_BUFFERS: [u32; 6] = [64, 128, 256, 512, 1024, 2048];

/// Picks ASIO (true) or the system driver (false) from the next stream opened.
pub fn set_asio(on: bool) {
    USE_ASIO.store(on && ASIO_BUILT, Ordering::Relaxed);
}

pub fn asio() -> bool {
    USE_ASIO.load(Ordering::Relaxed)
}

/// Picks the ASIO driver by name (None lets the app pick) and the buffer
/// size in frames (0 keeps the driver's own). Takes effect from the next
/// stream opened.
pub fn set_asio_driver(name: Option<String>, buffer: u32) {
    if let Ok(mut d) = ASIO_DRIVER.lock() {
        *d = name.filter(|n| !n.trim().is_empty());
    }
    let buffer = if ASIO_BUFFERS.contains(&buffer) {
        buffer
    } else {
        0
    };
    ASIO_BUFFER.store(buffer, Ordering::Relaxed);
}

/// The ASIO driver in use: the picked one when it is installed, else the
/// first installed that is not a wrapper or virtual driver, else the
/// first installed. None when none is installed.
pub fn asio_driver() -> Option<String> {
    let installed = asio_drivers();
    pick_driver(ASIO_DRIVER.lock().ok().and_then(|d| d.clone()), &installed)
}

fn pick_driver(chosen: Option<String>, installed: &[String]) -> Option<String> {
    if let Some(c) = chosen {
        if installed.iter().any(|n| n == &c) {
            return Some(c);
        }
    }
    // Wrappers over the system driver and virtual cables: fine when picked
    // on purpose, a poor guess otherwise (an interface's own driver is).
    const UNLIKELY: [&str; 7] = [
        "asio4all",
        "virtual",
        "generic low latency",
        "realtek",
        "fl studio",
        "vb-",
        "voicemeeter",
    ];
    installed
        .iter()
        .find(|n| {
            let l = n.to_lowercase();
            !UNLIKELY.iter().any(|u| l.contains(u))
        })
        .or_else(|| installed.first())
        .cloned()
}

/// Frames per ASIO buffer picked in Settings; 0 is the driver's own.
pub fn asio_buffer() -> u32 {
    ASIO_BUFFER.load(Ordering::Relaxed)
}

/// The buffer size to open streams with: the picked ASIO size on ASIO,
/// otherwise the device's default.
pub fn buffer_size() -> cpal::BufferSize {
    match asio_buffer() {
        n if asio() && n > 0 => cpal::BufferSize::Fixed(n),
        _ => cpal::BufferSize::Default,
    }
}

/// The host to open devices on now.
pub fn host() -> cpal::Host {
    host_for(asio())
}

/// The ASIO host when `asio` (and it is built in and loads), else the system one.
#[allow(unused_variables)]
pub fn host_for(asio: bool) -> cpal::Host {
    #[cfg(all(target_os = "windows", feature = "asio"))]
    if asio {
        if let Ok(h) = cpal::host_from_id(cpal::HostId::Asio) {
            return h;
        }
    }
    cpal::default_host()
}

/// The ASIO device: the one driver in use, loaded on its own. None when
/// ASIO is not built in or in use, no driver is installed, the driver
/// does not load, or another driver is still loaded.
pub fn asio_device() -> Option<cpal::Device> {
    if !asio() {
        return None;
    }
    #[cfg(all(target_os = "windows", feature = "asio"))]
    {
        let name = asio_driver()?;
        let host = cpal::host_from_id(cpal::HostId::Asio).ok()?;
        let dev = host.asio_device_named(&name);
        if dev.is_none() {
            log(&format!("ASIO driver \"{name}\" did not open"));
        }
        return dev;
    }
    #[allow(unreachable_code)]
    None
}

/// Names of the ASIO drivers installed, or empty when ASIO is not built
/// in or none are installed.
pub fn asio_drivers() -> Vec<String> {
    // Read the names from the registry. Asking each driver to load itself
    // can crash the whole app when a driver misbehaves, so nothing is
    // loaded here; a driver is only opened when the user picks ASIO.
    #[cfg(all(target_os = "windows", feature = "asio"))]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        if let Ok(out) = std::process::Command::new("reg")
            .args(["query", r"HKLM\SOFTWARE\ASIO"])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
        {
            let text = String::from_utf8_lossy(&out.stdout);
            let mut names: Vec<String> = text
                .lines()
                .filter_map(|l| {
                    l.trim()
                        .rsplit_once('\\')
                        .map(|(_, n)| n.trim().to_string())
                })
                .filter(|n| !n.is_empty() && !n.eq_ignore_ascii_case("ASIO"))
                .collect();
            names.sort();
            names.dedup();
            return names;
        }
    }
    Vec::new()
}

/// Where the audio log goes. Set once at startup.
pub fn set_log_path(path: PathBuf) {
    if let Ok(mut p) = LOG_PATH.lock() {
        *p = Some(path);
    }
}

pub fn log_path() -> Option<PathBuf> {
    LOG_PATH.lock().ok().and_then(|p| p.clone())
}

/// Appends one line to the audio log, with the time. Never called from
/// an audio thread. Kept under about a megabyte: the file starts over.
pub fn log(line: &str) {
    let Some(path) = log_path() else {
        return;
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let too_big = std::fs::metadata(&path).is_ok_and(|m| m.len() > 1_000_000);
    let f = std::fs::OpenOptions::new()
        .create(true)
        .append(!too_big)
        .write(too_big)
        .truncate(too_big)
        .open(&path);
    if let Ok(mut f) = f {
        let _ = writeln!(f, "{now} {line}");
    }
}

/// One line about a stream about to open, for the log.
pub fn describe(
    what: &str,
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    format: cpal::SampleFormat,
) -> String {
    use cpal::traits::DeviceTrait;
    let name = device
        .description()
        .map(|d| d.name().to_string())
        .unwrap_or_else(|_| "?".into());
    let buffer = match config.buffer_size {
        cpal::BufferSize::Fixed(n) => n.to_string(),
        cpal::BufferSize::Default => "default".into(),
    };
    format!(
        "{what}: {} on \"{name}\", {} Hz, {} channels, {format}, buffer {buffer}",
        if asio() { "ASIO" } else { "system" },
        config.sample_rate,
        config.channels
    )
}

/// A sample rate a device reported, made usable: some ASIO drivers say 0
/// until a stream runs, which would plan playback at 0 Hz. 48 kHz then
/// (the stream sets the driver to it when it can).
pub fn sane_rate(rate: u32) -> u32 {
    if rate == 0 {
        48_000
    } else {
        rate
    }
}

/// The same for a whole device setting.
pub fn sane_config(c: cpal::SupportedStreamConfig) -> cpal::SupportedStreamConfig {
    if c.sample_rate() == 0 {
        cpal::SupportedStreamConfig::new(
            c.channels(),
            sane_rate(0),
            *c.buffer_size(),
            c.sample_format(),
        )
    } else {
        c
    }
}

/// Runs an output callback so that a fault in it (a bug, or a driver
/// handing over something unexpected) leaves silence and a note in the
/// log instead of closing the app. The panic hook writes the log line.
pub fn guard_out<T: cpal::SizedSample>(out: &mut [T], f: impl FnOnce(&mut [T])) {
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(out))).is_err() {
        out.fill(T::EQUILIBRIUM);
        FAULT.store(true, Ordering::Relaxed);
    }
}

/// Sets up the engine's worker threads once, at startup: one per core,
/// or `AUDIFORM_THREADS` of them when set (for measuring). They spread
/// a block's tracks and buses over the cores (see the mix graph).
pub fn init_threads() {
    let _ = rayon::ThreadPoolBuilder::new()
        .num_threads(worker_threads())
        .thread_name(|k| format!("engine-{k}"))
        .build_global();
}

/// Threads the engine spreads a block's tracks over: the cores, or
/// `AUDIFORM_THREADS` when set (for measuring).
pub fn worker_threads() -> usize {
    std::env::var("AUDIFORM_THREADS")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|&n| n >= 1)
        .unwrap_or_else(|| std::thread::available_parallelism().map_or(4, std::num::NonZero::get))
}

/// The same for an input callback.
pub fn guard_in(f: impl FnOnce()) {
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).is_err() {
        FAULT.store(true, Ordering::Relaxed);
    }
}

/// True once a fault was caught; cleared on reading.
pub fn take_fault() -> bool {
    FAULT.swap(false, Ordering::Relaxed)
}

/// Writes every panic to the audio log as well, so a fault caught on an
/// audio thread (or any other) leaves its message and where it happened.
pub fn install_panic_log() {
    let before = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        log(&format!(
            "panic on thread \"{}\": {info}",
            thread.name().unwrap_or("?")
        ));
        before(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asio_stays_off_when_not_built() {
        set_asio(true);
        assert_eq!(asio(), ASIO_BUILT);
        set_asio(false);
        assert!(!asio());
    }

    #[test]
    fn the_driver_guess_skips_wrappers_and_virtual_cables() {
        let installed: Vec<String> = [
            "ASIO4ALL v2",
            "FL Studio ASIO",
            "Generic Low Latency ASIO Driver",
            "Realtek ASIO",
            "SSL ASIO Driver 1",
            "SSL ASIO Driver 2",
            "VB-Matrix VASIO-128",
            "Voicemeeter Virtual ASIO",
        ]
        .map(String::from)
        .to_vec();
        assert_eq!(
            pick_driver(None, &installed).as_deref(),
            Some("SSL ASIO Driver 1")
        );
        // Picked on purpose: any installed driver.
        assert_eq!(
            pick_driver(Some("ASIO4ALL v2".into()), &installed).as_deref(),
            Some("ASIO4ALL v2")
        );
        // Picked but gone: the guess again.
        assert_eq!(
            pick_driver(Some("Old Box".into()), &installed).as_deref(),
            Some("SSL ASIO Driver 1")
        );
        // Only wrappers installed: the first.
        let only: Vec<String> = vec!["ASIO4ALL v2".into()];
        assert_eq!(pick_driver(None, &only).as_deref(), Some("ASIO4ALL v2"));
        assert_eq!(pick_driver(None, &[]), None);
    }

    #[test]
    fn a_zero_rate_becomes_48k() {
        assert_eq!(sane_rate(0), 48_000);
        assert_eq!(sane_rate(44_100), 44_100);
        set_asio_driver(None, 100);
        assert_eq!(asio_buffer(), 0);
        set_asio_driver(None, 256);
        assert_eq!(asio_buffer(), 256);
        set_asio_driver(None, 0);
    }

    #[test]
    fn a_fault_in_a_callback_leaves_silence() {
        let mut out = [0.5f32; 8];
        guard_out(&mut out, |o| {
            o[0] = 1.0;
            panic!("driver handed over nothing");
        });
        assert!(out.iter().all(|v| *v == 0.0));
        assert!(take_fault());
        assert!(!take_fault());
    }
}
