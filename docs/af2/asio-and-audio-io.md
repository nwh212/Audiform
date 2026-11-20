# ASIO and audio device I/O: handoff guide for Audiform 2

This guide is for whoever builds audio device I/O in Audiform 2 (the pure Rust rewrite). It describes how Audiform 1 does it, what went wrong on real hardware, how each problem was fixed, and what to keep or change.

Source: Audiform 1 at commit [`042577e`](https://github.com/nwh212/Audiform/tree/042577e1ddee7b9718d82aea7947f9aab6edd628) (version 1.0.0, the last release on that codebase). All links below point at that commit.

Short link prefix used in this guide:

- `A1/` = `https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/`
- Commits: `https://github.com/nwh212/Audiform/commit/<hash>`

**Licence note.** Audiform 1 is GPL-3.0-or-later. Most files named here carry `Copyright (c) The GeekCast Radio Network, LLC.` (from Altered Audio Editor); `speaker_feed.rs` and `midi_live.rs` carry `Copyright (c) The Audiform contributors.`. Any code you copy into Audiform 2 keeps the GPL-3.0-or-later licence and keeps its copyright lines. The patched cpal in `vendor/cpal` is Apache-2.0 (upstream cpal); keep its notice. The ASIO SDK is used under Steinberg's GPLv3 option (see section 5).

**Hardware caveat.** Almost all ASIO testing was done by the project lead on one interface (an SSL 2+ MKII, with 15 ASIO drivers installed on that PC). The development workspace was Linux, so ASIO code was only compiled on Windows CI, never run there. Several fixes are marked in `docs/decisions.md` as "a guess at the cause; not tried on hardware". This guide says so where it applies.

---

## Contents

1. Architecture overview
2. ASIO specifics
3. WASAPI, ALSA and other hosts
4. Multichannel output
5. Build and CI
6. Bugs we hit and how we fixed them
7. Recommended design for Audiform 2
8. File map

---

## 1. Architecture overview

### 1.1 Library stack

- **cpal 0.18.2**, patched, vendored at [`src-tauri/vendor/cpal`](https://github.com/nwh212/Audiform/tree/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/vendor/cpal) and wired in through `[patch.crates-io]` in [`src-tauri/Cargo.toml`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/Cargo.toml#L179-L187). cpal's ASIO backend uses `asio-sys 0.4.0`.
- **rtrb 0.4**: single producer, single consumer lock-free ring buffers of `f32`. Every stream is fed through one.
- **midir 0.11**: MIDI inputs (only relevant here because the live MIDI player owns an output stream).
- **rayon**: worker pool for the mix graph (not on the audio thread). Set up once in `host::init_threads()`, one thread per core, or `AUDIFORM_THREADS`.
- Cargo feature `asio = ["cpal/asio"]`, off by default, on in release builds.

### 1.2 The one rule for the audio thread

Every device callback in Audiform 1 does only this: read atomics, pop or push an rtrb ring, convert samples, write meters with `fetch_max` on atomics. No allocation, no locks, no I/O, no logging. Scratch buffers are allocated before `build_output_stream` and moved into the closure. From [`transport.rs` `build`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/engine/transport.rs#L1207-L1247):

```rust
// Scratch for the audio thread, allocated here, never in the callback.
let n = consumers.len();
let mut gains = vec![[0f32; 2]; n];
let mut peaks = vec![[0f32; 2]; n];
device.build_output_stream(
    config,
    move |out: &mut [T], _| {
        super::host::guard_out(out, |out| {
            if shared.hush.silenced(out) {
                return;
            }
            mix(out, channels, &mut consumers, &mut click, &shared,
                &mut gains, &mut peaks, &mut routing);
        });
    },
    |e| super::host::log(&format!("play stream error: {e}")),
    None,
)
```

Three wrappers appear in every callback:

1. `host::guard_out` / `host::guard_in`: `catch_unwind` around the body. A panic leaves a silent buffer and sets a global `FAULT` flag. The panic hook writes the message to `audio.log`.
2. `Hush::silenced`: once a stream is closing, the callback writes silence and returns (see 2.6).
3. The body itself.

Known exceptions to "no locks" are worth knowing about:

- The live MIDI player's render thread (not the device callback) locks `Mutex<Vec<Voice>>` and the plug-in `Host` mutex. That is fine because it is not the device thread.
- cpal's own ASIO callback takes `asio_streams.lock()` every buffer (see 2.6). That is upstream code; the comment says it only contends when streams are recreated.
- Atomics carry `f32` values as bits (`f32::to_bits`). Meters use `fetch_max` on the bits: non-negative floats order like their bit patterns.

### 1.3 Threads and streams per feature

Each feature that makes sound opens **its own cpal stream** on **its own thread**, and that thread both creates and drops the stream. The comment in `preview.rs` gives the reason: the stream "on some platforms must stay on the thread that created it" (`cpal::Stream` is not `Send` on every host).

| Feature | Owner type | Threads | Stream(s) | Ring(s) |
| --- | --- | --- | --- | --- |
| Multitrack playback | `engine::transport::Transport` | caller; `transport` (owns the stream); `transport-render`; the device thread | 1 output | one stereo ring per track (plain session) or one for the routed mix; a click ring; a bed ring for Speakers listen mode |
| Recording and input meters | `engine::record::Recorder` | caller; one `record` thread per input device (owns the input stream and the monitor stream, drains the ring, writes WAV); optional `monitor-plugins` | 1 input per device, plus 1 monitor output | input ring: 2 seconds of all device channels; monitor ring: 1 second stereo, kept under 80 ms |
| Input monitoring | inside the `record` thread | as above | output via `record::open_monitor` | stereo |
| File and generated-sound preview | `engine::preview::PreviewPlayer` | caller; preview thread (owns the stream); `preview-decode`; device thread | 1 output | 0.5 s of device channels |
| Live MIDI player | `engine::midi_live::Player` | `midi-live` (renders 128-frame blocks, owns the stream); device thread | 1 output via `record::open_monitor` | stereo, filled to `ahead_for(buffer)` frames |

So on one machine you can have up to four output streams and one input stream open at once, all on the same device. On ASIO they are all on the same driver. This is the source of most of the ASIO bugs in section 6.

### 1.4 Playback (`transport.rs`)

Startup, from [`start` and `run`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/engine/transport.rs#L713-L996):

1. `Transport::play_with_click` stops the current `Running` (joins its thread), expands 3D Space clips (`speaker_feed::expanded`), then calls `start`.
2. `start` spawns the `transport` thread running `run`, and waits on an `mpsc` channel for `Result<u32, String>` (the output rate). Timeout: 5 s, or 30 s when the session has plug-ins. On timeout it sets `stop` and returns "The audio device did not respond."
3. `run` picks the device (`output::output_device_any()`), works out how many outputs it needs (stereo pair or speaker map, see section 4), gets a config from `output::config_for(&device, wanted)`, and sets `config.buffer_size = host::buffer_size()`.
4. It logs the stream with `host::describe("play", ...)`, notes the rate (`output::note_rate`, `scope::set_rate`).
5. It builds the renderer and mix graph at the **device** rate (`plan::to_out` converts session sample positions to output frames).
6. Ring size: `cap = (out_rate * AHEAD_SECONDS).max(BLOCK * 2) * 2` floats, with `AHEAD_SECONDS = 0.5` and `BLOCK = 2048`. So half a second of stereo.
7. It **fills every ring before the stream starts** ("so playback starts clean"), then spawns `transport-render`, then builds and plays the stream, then sends `Ok(out_rate)`.
8. It logs a timing line ("play start: ... chains ... ms, priming ... ms, first 0.5 s of audio ... ms, device ... ms").
9. It sleeps in 15 ms steps until `shared.stop`, then `shared.hush.settle()`, `drop(stream)`, joins the render thread.

The render loop (`render_loop`) renders `BLOCK` frames when every ring has room for one more block, else sleeps 5 ms. It also checks `host::take_fault()`: if an audio-thread panic was caught, playback stops with a log line. It logs render load (average and worst share of real time) every 10 s.

The audio thread (`mix`) takes `avail = min(slots/2)` over every ring, mixes `min(avail, frames)` frames with per-track gains read from atomics, applies the master gain, the mono check, the level audition, pushes to the scope, adds the click after the master fader and meters, writes to the mapped outputs, fills the rest with silence and counts an underrun if short. `shared.played` (frames sent to the device) is the clock the UI reads for the playhead.

Live changes:

- Plain sessions: track gains are atomics, heard on the next buffer.
- Routed sessions (buses, sends, master chain, sidechain): levels go through a `Mutex<Option<Levels>>` that the render thread picks up with `try_lock`, so they are heard after the half second already rendered. The master fader stays an atomic, so it is instant.
- Anything that changes timing or routing restarts playback in place (`Transport::sync`).

### 1.5 Recording and input monitoring (`record.rs`)

From [`Recorder::start`, `spawn_device`, `run`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/engine/record.rs#L694-L1115):

1. Each `RecordTarget` has an `input` string. `route()` parses it to a `Route { device, channels }` (see 2.10). Targets are grouped by device.
2. If only meters were open (no `dir`), they are closed first: "some systems open an input only once".
3. One `record` thread per device. It finds the device (`find_device`), takes `default_input_config()` (or `default_output_config()` for Windows loopback), runs it through `host::sane_config`, sets `buffer_size = host::buffer_size()`, logs it, and opens the input stream.
4. The input callback only converts samples to `f32`, pushes to the ring, counts overruns, and records a peak. Ring size: `(rate * channels * 2).max(8192)` floats (two seconds).
5. The thread drains the ring every 10 ms into `TakeWriter` (one 32-bit float WAV per target, via `hound`), `LiveTap` (meters and 32-frame min/max blocks for the live waveform) and `MonitorMix`.
6. The first device opened "keeps the recording's clock" (`shared.frames`). A device opened later (a track armed during a take) starts its takes at `since.elapsed() * rate` frames.

Input monitoring: when any target has `monitor`, the thread calls `open_monitor(rate, input_name)` and pushes the stereo mix into its ring. The queue is capped by `MONITOR_MAX_QUEUE = 0.08` seconds: if pushing would exceed it, the new audio is **dropped**, so latency cannot creep up. Monitoring with plug-ins starts with built-in effects only, while a `monitor-plugins` thread builds the full chain; it swaps in when ready.

`open_monitor` ([L1119-L1178](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/engine/record.rs#L1119-L1178)) is shared by monitoring and the live MIDI player. Key points:

- It opens the output **the same way playback does**: `output::config_for(&dev, wanted)` with `wanted = max(stereo_out) + 1`, and accepts it only if its rate equals the input rate. Otherwise it searches `supported_output_configs()` for one that covers the rate, preferring 2+ channels and F32.
- If no config runs at the input's rate, monitoring is off with: "Input monitoring is off: the output does not play at {rate} Hz, the input's rate."
- The callback (`build_out`) writes left and right to the outputs playback uses (`OutputMap.stereo`), silence elsewhere, and mono-sums for a 1-channel device.
- It returns `(Producer<f32>, host::QuietStream)`.

### 1.6 Previews (`preview.rs`)

Simplest stream: a decode thread fills a ring of `max(rate * 0.5, 4096) * channels` floats at the device's own rate and channel count; the callback copies, pads with silence on underrun, and sets `finished` once the decoder is done and the ring is empty. Opened with `default_output_config()` (no wide config, so outputs 1 and 2 only). Ends with `hush.settle()` then `drop(stream)`. `record_start` stops any preview first.

### 1.7 Live MIDI player (`midi_live.rs`)

[`Player::start` and `run`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/engine/midi_live.rs#L554-L662):

- Rate: `output::device_rate()` (the rate the last playback stream opened at, else the device default). The player is restarted when that rate changes.
- Output: `record::open_monitor(rate, None)`.
- The `midi-live` thread renders `BLOCK = 128` frames at a time from each armed track's own copy of its instrument and effects, and keeps `ahead_for(buffer)` frames queued (see 2.8).
- It watches its own queue and reopens the stream if the device stops pulling (see 2.9).

### 1.8 Opening and closing: the lifecycle pattern

Every output stream follows the same shape. Copy this shape.

```
spawn owner thread
  pick device, pick config, set buffer size, log it
  create rings; pre-fill if it is playback
  build stream (callback owns consumer + Arc<Shared>), play()
  send Ok(rate) on a ready channel   <- caller waits with a timeout
  loop: sleep until stop flag
  hush.settle()                      <- write silence for 3 callbacks
  drop(stream)
  join helper threads
```

The caller's `stop()` sets the flag and joins the owner thread, so when `stop()` returns, the device has taken silent buffers and the stream is gone.

---

## 2. ASIO specifics

All ASIO state lives in [`engine/host.rs`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/engine/host.rs) as globals:

```rust
static USE_ASIO: AtomicBool = AtomicBool::new(false);
static ASIO_DRIVER: Mutex<Option<String>> = Mutex::new(None);
static ASIO_BUFFER: AtomicU32 = AtomicU32::new(0);   // 0 = driver's own
static LOG_PATH: Mutex<Option<PathBuf>> = Mutex::new(None);
static FAULT: AtomicBool = AtomicBool::new(false);
pub const ASIO_BUILT: bool = cfg!(all(target_os = "windows", feature = "asio"));
pub const ASIO_BUFFERS: [u32; 6] = [64, 128, 256, 512, 1024, 2048];
```

Settings take effect "from the next stream opened". The UI calls `set_audio_driver(asio, driver, buffer)` on startup and on every settings change, and then reopens armed inputs (frontend, [`App.svelte`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/app/App.svelte#L2481)). While recording, a notice says the change applies when recording stops.

### 2.1 The driver picker

Listing drivers **loads nothing**. Names come from the registry by running `reg query HKLM\SOFTWARE\ASIO` with `CREATE_NO_WINDOW` and taking the last path component of each line ([`asio_drivers`, L160-L189](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/engine/host.rs#L160-L189)). Sorted, deduped, "ASIO" itself filtered out.

Picking: the chosen name if installed, else the first installed name that does not contain any of these (case-insensitive), else the first installed ([`pick_driver`, L79-L104](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/engine/host.rs#L79-L104)):

```rust
const UNLIKELY: [&str; 7] = [
    "asio4all", "virtual", "generic low latency", "realtek",
    "fl studio", "vb-", "voicemeeter",
];
```

Reason: with no default device, upstream cpal opened the first driver alphabetically (ASIO4ALL), not the interface's own driver. The test `the_driver_guess_skips_wrappers_and_virtual_cables` pins this.

In Audiform 2, read the registry directly (the `winreg` crate, or `windows-sys` `RegEnumKeyExW`) instead of spawning `reg.exe`. Note: 32-bit drivers register under `HKLM\SOFTWARE\WOW6432Node\ASIO`; Audiform 1 does not read that key, and a 64-bit app cannot load 32-bit drivers anyway, so that is correct.

### 2.2 Loading only the chosen driver (the cpal patch)

Upstream cpal enumerates ASIO devices by loading every installed driver in turn and stops at the first one already loaded. On the lead's PC with 15 drivers that listed no devices and crashed on Play. Even earlier (0.1.7), the Settings > Devices page closed the app instantly; the suspect was every driver loading itself, and "a native crash in a driver cannot be caught".

The patch, described in [`vendor/cpal/AUDIFORM.md`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/vendor/cpal/AUDIFORM.md):

- `src/host/asio/device.rs`: `Devices::named(asio, name)` builds an iterator over a one-element driver list.
- `src/host/asio/mod.rs`: `Host::device_named(&self, name) -> Option<Device>`.
- `src/platform/mod.rs`: `Host::asio_device_named` on the platform `Host` (Windows, `asio` feature).

```rust
// vendor/cpal/src/host/asio/device.rs
pub fn named(asio: Arc<sys::Asio>, name: &str) -> Result<Self, Error> {
    com::com_initialized();
    Ok(Self {
        asio,
        drivers: vec![name.to_string()].into_iter(),
        current_driver: None,
    })
}
```

Audiform calls it through `host::asio_device()`:

```rust
let name = asio_driver()?;
let host = cpal::host_from_id(cpal::HostId::Asio).ok()?;
let dev = host.asio_device_named(&name);
if dev.is_none() { log(&format!("ASIO driver \"{name}\" did not open")); }
```

ASIO allows one loaded driver per process. cpal keeps a process-wide `GLOBAL_ASIO: OnceLock<Arc<sys::Asio>>`. `device_named` returns None if another driver is still loaded.

On ASIO the device list is **one entry named after the driver**, for both inputs and outputs (`commands/devices.rs` `asio_inputs` / `asio_outputs`). The output name the user picked for the system driver is ignored on ASIO.

### 2.3 Sample rate handling ("rate guard")

There is no sample-rate picker. Streams open at the device's default config rate.

- **Zero rate guard.** Some drivers report 0 Hz until a stream runs (cpal's own comment names Realtek ASIO). `host::sane_rate(0) == 48_000`, and `host::sane_config` rebuilds a `SupportedStreamConfig` with 48 kHz. cpal's `check_config` then calls `driver.set_sample_rate(48000)` if the driver can do it. Without this, playback was planned at 0 Hz.
- **cpal sets the driver's rate.** In [`check_config`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/vendor/cpal/src/host/asio/stream.rs#L1130-L1206), if the requested rate differs from `driver.sample_rate()` and `can_sample_rate` is true, it calls `set_sample_rate`. Otherwise `UnsupportedConfig`. So the first stream effectively sets the rate for every later stream on that driver.
- **Rate change during a take.** When the driver asks for a restart, the recorder re-reads the default config and, if the rate changed, stops the take with: "The audio driver switched from {rate} Hz to {r} Hz while recording, so the take stopped there." (see 2.11).
- **Session rate vs device rate.** Playback renders at the device rate and converts positions with `plan::to_out` / `plan::to_session`. `output::note_rate` remembers the rate the last playback stream opened at; `output::device_rate()` returns it so plug-ins started before Play run at the same rate and are not restarted.
- **Monitoring needs equal rates.** `open_monitor` refuses an output that cannot run at the input's rate (message in 1.5).

### 2.4 Buffer size control

```rust
pub fn buffer_size() -> cpal::BufferSize {
    match asio_buffer() {
        n if asio() && n > 0 => cpal::BufferSize::Fixed(n),
        _ => cpal::BufferSize::Default,
    }
}
```

- Only applied on ASIO. On WASAPI/ALSA every stream uses `BufferSize::Default`.
- Values outside `ASIO_BUFFERS` are stored as 0 (driver's own).
- cpal's `check_config` rejects a `Fixed` size outside the driver's `buffersize_range()`, or not on its step (`BufferPreference::Stepped`), with `UnsupportedConfig`. Audiform 1 does not query the range first; it offers the fixed list and lets the open fail. Audiform 2 should ask the driver for its range and offer only valid sizes.
- With ASIO, the **first** stream that creates the driver's buffers fixes the size. Later streams on the same driver reuse it (`get_or_create_output_stream` returns the existing `buffer_size`).
- cpal handles `kAsioBufferSizeChange` by updating `buffer_size` and resizing its interleave buffer in the callback.

### 2.5 Crash catching and logging

- `guard_out` / `guard_in` (`catch_unwind`) around every callback body. A caught fault leaves silence and sets `FAULT`. `transport-render` polls `take_fault()` and stops playback with "playback stopped after a fault on the audio thread (see the panic above)". This catches Rust panics only; a native crash inside a driver still takes the process down.
- `install_panic_log()` chains a panic hook that writes `panic on thread "<name>": <info>` to the audio log. Thread names (`transport`, `transport-render`, `record`, `midi-live`, `preview-decode`, `engine-N`) make these readable.
- `host::log(line)` appends `"<unix seconds> <line>"` to `audio.log` in the app config folder. When the file passes 1,000,000 bytes it is truncated and started over. Never called from an audio thread.
- `host::describe(what, device, config, format)` gives lines like `play: ASIO on "SSL ASIO Driver", 48000 Hz, 4 channels, f32, buffer 256`.
- Every stream error callback logs (`play stream error`, `record stream error`, `monitor stream error`, `preview stream error`).
- Settings shows the log path; the UI can append its own error trail with the `log_line` command (capped at 2000 chars).
- The audio log was the main debugging tool for the project lead's reports. It found the 64-frame crash, the 88.2 kHz reset requests and the 17 to 25 s plug-in cold starts. Keep it.

### 2.6 Several streams on one ASIO driver: Hush and QuietStream

**Symptom** (0.7.x, [decision 2026-11-06](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/docs/decisions.md#L2661-L2665)): after Stop, the audio "drags and glitches and keeps playing without progressing", but only on ASIO while a MIDI track was armed or an input was monitored.

**How cpal ASIO shares a driver.** ASIO gives a process one set of output buffers. cpal keeps one `streams.output` per driver and every cpal output `Stream` registers a callback on it. In each buffer switch, the first callback to run silences the ASIO buffer and later callbacks **add** their samples ([stream.rs L586-L600](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/vendor/cpal/src/host/asio/stream.rs#L586-L600)). Dropping a `Stream` only removes its callback (`Drop for Stream` calls `driver.remove_callback`); the driver keeps running while any other stream is open.

**Cause, as recorded.** "An ASIO driver keeps running while any stream is open on it, and it replays whatever a closed stream last left in its buffers." Why exactly the stale buffer replays is not proven from the code. A plausible reading: the ASIO double buffer keeps the last samples written; if no remaining callback silences that half (for example a stream that is not `Playing` returns before the silence step), the driver plays it again on every switch. Treat the mechanism as unconfirmed; the fix worked in practice.

**Fix** ([`host.rs` L273-L329](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/engine/host.rs#L273-L329), commit [`e33da0b`](https://github.com/nwh212/Audiform/commit/e33da0b)): before a stream closes, its callback writes silence for a few buffers.

```rust
#[derive(Default)]
pub struct Hush {
    on: AtomicBool,
    calls: AtomicU32,
}

impl Hush {
    /// In the output callback: true (with `out` silenced) once the
    /// stream is closing, so the callback does nothing else.
    pub fn silenced<T: cpal::SizedSample>(&self, out: &mut [T]) -> bool {
        if !self.on.load(Ordering::Relaxed) {
            return false;
        }
        out.fill(T::EQUILIBRIUM);
        self.calls.fetch_add(1, Ordering::Relaxed);
        true
    }

    /// Starts the silence and waits until the device has taken a few
    /// silent buffers (or a moment has passed, for a stream that has
    /// stopped calling back). Then the stream can be dropped.
    pub fn settle(&self) {
        self.on.store(true, Ordering::SeqCst);
        let t = Instant::now();
        while self.calls.load(Ordering::Relaxed) < 3
            && t.elapsed() < Duration::from_millis(400)
        {
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}

/// An output stream that ends on silence when dropped.
pub struct QuietStream { hush: Arc<Hush>, stream: Option<cpal::Stream> }

impl Drop for QuietStream {
    fn drop(&mut self) {
        self.hush.settle();
        self.stream.take();
    }
}
```

Constants: 3 silent callbacks, 400 ms cap, 2 ms poll. Used by:

- playback: `Shared.hush`, `shared.hush.settle(); drop(stream);` at the end of `transport::run`;
- preview: same pattern in `preview::run`;
- monitoring and live MIDI: `open_monitor` returns a `QuietStream`, so dropping it settles.

Cost: Stop takes up to about 3 buffers (at 2048 frames and 44.1 kHz about 140 ms), capped at 400 ms if the stream stopped calling back.

### 2.7 Stream width on a shared driver (cpal patch, part 2)

**Symptom** (2026-10-31): ASIO at a 64-frame buffer crashed on the first Record. **Cause:** monitoring opened the driver's output with 2 channels; Play then asked for 4 (the interface's outputs). cpal reused the 2-channel ASIO stream and read past its buffers. **Fix** (commit [`c8707e0`](https://github.com/nwh212/Audiform/commit/c8707e0)), two parts:

1. The patched cpal refuses a stream wider than the one already open on the driver ([stream.rs L874-L888 and L930-L942](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/vendor/cpal/src/host/asio/stream.rs#L874-L942)):

```rust
match streams.output {
    // Audiform: see the input side.
    Some(ref output) if output.buffer_infos.len() < num_channels => {
        Err(Error::with_message(
            ErrorKind::DeviceBusy,
            format!("The ASIO output is already open with {} channels; {} were asked for.",
                output.buffer_infos.len(), num_channels),
        ))
    }
    Some(ref output) => Ok(output.buffer_size as usize),
    None => { /* prepare_output_stream ... */ }
}
```

2. `open_monitor` now opens the output exactly as playback does (`output::config_for` with the same `wanted` channel count), so the two never differ. Rule for Audiform 2: **every output stream on one ASIO driver must ask for the same channel count.** Better: open one output stream per driver and mix everything into it (section 7).

### 2.8 Buffer-size-aware queue depth (`ahead_for`)

**Symptom:** live MIDI "extremely stuttery, even with 512 on ASIO". **Cause:** the live player kept a fixed 256 to 384 frames queued, and a 512-frame ASIO buffer pulls 512 per callback, so the queue ran dry every buffer. **Fix** (commit [`e33da0b`](https://github.com/nwh212/Audiform/commit/e33da0b)):

```rust
/// Frames rendered at a time.
const BLOCK: usize = 128;
/// Frames kept waiting for the speakers, at least: about 5 ms at 48 kHz.
const AHEAD: usize = 256;

/// Frames to keep waiting for a device that takes `buffer` frames per
/// callback: two of its buffers and a block, so a callback never finds
/// the queue short (which is heard as stutter at larger ASIO buffers).
fn ahead_for(buffer: Option<u32>) -> usize {
    // Without a fixed size, allow for a large shared-mode buffer.
    let b = buffer.map_or(1024, |n| n as usize);
    (2 * b + BLOCK).max(AHEAD)
}
```

Test `live_queue_covers_two_device_buffers`: 64 gives 256, 512 gives 1152, 2048 gives 4224, `None` gives at least 2048.

Weakness: `buffer` is what Audiform **asked** for, not what the driver uses. With `BufferSize::Default` it guesses 1024. Audiform 2 should use the real callback size (the length of the first callback's buffer, or the ASIO driver's preferred size) instead.

### 2.9 Live MIDI stall and reopen

**Symptom:** the instrument did not seem to play notes while recording MIDI. **Guessed cause** (not confirmed on hardware): "With ASIO a driver can serve only some of the streams opened on it at once, and the live player can be the one left waiting." **Fix** (commit [`ba6529b`](https://github.com/nwh212/Audiform/commit/ba6529b), which also carries the MIDI controller work): the `midi-live` thread watches its queue. If the queue stays full (`waiting >= ahead`) and the device has not taken any samples for `STALL = 500 ms`, it opens a new stream via `open_monitor` and swaps it in (the old `QuietStream` drops and settles).

```rust
let waiting = (capacity - slots) / 2;
if waiting >= ahead || slots < BLOCK * 2 {
    if waiting >= ahead && taken.elapsed() > STALL {
        taken = Instant::now();
        match super::record::open_monitor(rate, None) {
            Ok((p, st)) => {
                drop(std::mem::replace(&mut stream, st));
                out = p;
                capacity = out.buffer().capacity();
                last_slots = out.slots();
                super::host::log("midi live: the output stalled; opened it again");
            }
            Err(e) => super::host::log(&format!("midi live: could not reopen the output: {e}")),
        }
    }
    thread::sleep(Duration::from_millis(1));
    continue;
}
```

`taken` is updated whenever `out.slots()` grows (the consumer popped). Listed as a known issue in [release notes 1.0.0](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/docs/release-notes-1.0.0.md): "an instrument may not sound while you record MIDI on some ASIO drivers ... not confirmed on hardware".

### 2.10 How input channels are picked per track

A track's input is a string. Format, parsed by [`record::route`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/engine/record.rs#L101-L140):

- `None` (absent): default input; channels `[0, 1]` if the take wants 2, else `[0]`.
- `"none"`: do not record.
- `"Mono: <device> <n>"`: device name, 1-based channel n.
- `"Stereo: <device> <a>-<b>"`: device name, 1-based pair.
- `"System audio: <output name>"` (Windows only, `SYSTEM_AUDIO_PREFIX`): loopback capture of that output.
- Anything unparsable falls back to the default.

The device name can contain spaces; the parser splits on the **last** space (`rsplit_once(' ')`). The UI builds the options from `audio_inputs()` (name plus channel count): one Mono entry per channel and Stereo pairs `1-2, 3-4, ...` ([`track-menus.ts` L278-L281](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/track-menus.ts#L278-L281)). Odd pairs like 2-3 are not offered but would parse.

On ASIO (`find_device`), the device name must be the driver in use (or absent); a track naming another ASIO driver gets: "The input "X" is not the ASIO driver in use (Y). Pick the driver's inputs on the track, or switch the driver in Settings > Audio Devices." No second driver is loaded.

All targets on one device share one input stream opened with **all** of the device's default input channels; each take picks its channels out of the interleaved frames (`TakeWriter::push`). A channel the device lacks records silence. Every target gets its own file even when several share an input.

Persisting device names as strings is fragile (renames, two identical interfaces). Audiform 2 should store a structured input: `{ host, device_id_or_name, channels: Vec<u16> }`.

### 2.11 Driver reset requests and high-rate, small-buffer restarts (known issue)

cpal turns `kAsioResetRequest` and `kAsioResyncRequest` into a `StreamInvalidated` error, debounced by `ASIO_EVENT_DEBOUNCE = 500 ms`, and ignores them while the stream is `Starting` (some drivers fire spurious requests during `start()`).

**Symptom** (2026-11-01): at 88.2 kHz on the SSL, recording stopped two or three seconds in; the log showed "Stream reset was requested by the ASIO driver". Audiform only logged it, and the driver stopped sending sound.

**Fix for recording** (commit [`58eff18`](https://github.com/nwh212/Audiform/commit/58eff18)): the input error callback sets a `reset` flag on `StreamInvalidated`. The `record` thread then, in [L999-L1055](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/engine/record.rs#L999-L1055):

1. drops the input stream and the monitor (they share the driver),
2. drains what is left,
3. re-reads the device rate; if it changed, stops the take with a message,
4. opens a new ring and stream with the same config, reopens the monitor,
5. writes silence for the time since sound last arrived (`heard.elapsed() * rate` frames) so the take stays in time,
6. logs "record: the driver asked for a restart; the input started again".

**Not fixed for playback.** The transport's error callback only logs. The lead's log shows reset requests during playback at 48 kHz too. Known issue in 0.7.1, 0.8.0 and 1.0.0 release notes: "On ASIO, very small buffers at 88.2 or 96 kHz can make some interfaces' drivers ask for restarts, heard as short gaps. Use a larger buffer at high sample rates. Playback does not restart on its own yet; stop and play again." Audiform 2 must handle `StreamInvalidated` (and device loss) for every stream, not just input.

### 2.12 One program at a time, and the fallback to the system driver

ASIO drivers usually open for one process. Two consequences handled in Audiform 1:

- **Fallback for output.** `output::output_device_any()` tries the chosen host, then the system host:

  ```rust
  pub fn output_device_any() -> Option<cpal::Device> {
      let host = crate::engine::host::host();
      output_device(&host).or_else(|| output_device(&cpal::default_host()))
  }
  ```

  Playback and preview use it, so a busy ASIO driver never leaves the app silent. Monitoring and live MIDI use `output_device(&host)` only (no fallback), because they must stay on the input's driver.
- **Retry empty lists.** `commands/devices.rs` `retry_if_empty`: on ASIO, if the lookup returns nothing but drivers are installed, try 4 more times, 400 ms apart. Cases: right after an update restart (the old copy still holds the driver), or after a plug-in crash. The system driver is never retried. The UI also keeps the last non-empty list per driver and has a Refresh Devices button.

Note: `devices.rs` flips the global `USE_ASIO` temporarily to list ASIO devices (`set_asio(true)` ... `set_asio(was)`). That races with any stream opening at the same moment. Avoid global toggles in Audiform 2.

---

## 3. WASAPI, ALSA and other hosts

What differed, from the code:

- **Host choice.** `host::host_for(asio)` returns `cpal::host_from_id(HostId::Asio)` when ASIO is built and wanted, else `cpal::default_host()`: WASAPI on Windows, Core Audio on macOS, ALSA on Linux. cpal's JACK and PulseAudio features are not enabled.
- **Buffer size.** Never set on system hosts (`BufferSize::Default`). Decisions list "ASIO and WASAPI buffer, latency and sample rate controls" as planned; only the ASIO buffer was built.
- **Device names.** On system hosts the user picks an output by its `description().name()`; on ASIO the picked name is ignored. `outputs()` dedups by name (`dedup_by(|a, b| a.name == b.name)`); the code does not say which host produced duplicates.
- **Inputs listing.** System hosts list every input device with more than 0 default input channels, default first. On Windows, every output also appears as `"System audio: <name>"`.
- **System audio (WASAPI loopback).** `find_device` returns the **output** device with `loopback = true`, and the record thread opens an input stream on it with `default_output_config()`. cpal's WASAPI backend does loopback when you build an input stream on an output device. Windows sends nothing while the output is silent, so the recorder pads silence by the wall clock (`silence_to_add`): only once more than 100 ms is missing, and it stops 50 ms short so arriving audio is never pushed late. Loopback inputs are never monitored. Not supported on other platforms ("Recording system audio works on Windows only for now.").
- **Exclusive vs shared mode.** Audiform 1 uses whatever cpal's WASAPI does by default (shared mode). There is no exclusive-mode option.
- **Several streams.** On WASAPI shared mode, several output streams on one device mix in the system mixer, so the ASIO problems in 2.6 and 2.7 do not appear there.
- **Rates.** In WASAPI shared mode, input and output may run at different mix rates; that is why monitoring refuses mismatched rates with a message pointing at the system sound settings.
- **Linux.** Needs ALSA headers (`libasound2-dev`) to build; CI installs them. No Linux-specific audio code exists. Linux builds are marked experimental and no Linux-specific audio bugs are recorded in `docs/decisions.md`. Under PipeWire or PulseAudio, cpal's ALSA host talks to their ALSA plug-in; nothing in Audiform 1 depends on that.
- **macOS.** Needed a microphone usage string and an audio-input entitlement, or input is silently denied (decision 2026-10-20). Untested otherwise.
- **Opening the system sound settings.** `open_sound_settings` tries `ms-settings:sound` then `mmsys.cpl` (Windows), the Sound preference pane (macOS), `pavucontrol`, `gnome-control-center sound`, `systemsettings kcm_pulseaudio` (Linux).

---

## 4. Multichannel output

### 4.1 OutputMap

[`engine/output.rs`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/engine/output.rs#L17-L41):

```rust
pub struct OutputMap {
    pub stereo: [u16; 2],                         // 0-based device outputs for L and R
    pub speakers: std::collections::HashMap<String, u16>, // speaker name -> output
}
impl Default for OutputMap { fn default() -> Self { Self { stereo: [0, 1], speakers: HashMap::new() } } }
impl OutputMap {
    /// The output for speaker `name`, the `index`th of its layout.
    pub fn speaker(&self, name: &str, index: usize) -> usize {
        self.speakers.get(name).map_or(index, |&o| usize::from(o))
    }
}
```

Set from Settings > Channel Routing via `set_output_map(stereo, speakers)`. A speaker not named plays on the output at its place in the layout ("L on 1, R on 2, C on 3...").

### 4.2 Speaker layouts

From [`dsp/space.rs` `speakers`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/dsp/space.rs#L122-L132), in multichannel WAV order:

| Layout | Speakers (names in the code) |
| --- | --- |
| Stereo20 | L R |
| Surround51 | L R C LFE Ls Rs |
| Surround512 | L R C LFE Ls Rs Ltm Rtm |
| Surround71 | L R C LFE Lrs Rrs Lss Rss |
| Surround712 | L R C LFE Lrs Rrs Lss Rss Ltm Rtm |
| Surround514 | L R C LFE Ls Rs Ltf Rtf Ltr Rtr |
| Surround714 | L R C LFE Lrs Rrs Lss Rss Ltf Rtf Ltr Rtr |

### 4.3 Speakers listen mode

`SpaceListen` has three values: `Stereo` (fold by position), `Binaural` (headphones), `Speakers` ("each speaker to its own output of the audio device"). Added in commit [`b65448d`](https://github.com/nwh212/Audiform/commit/b65448d).

In `transport::run`, when `listen == Speakers`, it builds `bed_out: Vec<usize>` (one output per speaker via `omap.speaker(name, k)`), and asks for `wanted = max(bed_out) + 1` outputs. `output::config_for` returns the device default if it has enough channels, else the widest supported config at the default rate (preferring the default sample format):

```rust
pub fn config_for(device: &cpal::Device, channels: usize) -> Option<cpal::SupportedStreamConfig> {
    let def = super::host::sane_config(device.default_output_config().ok()?);
    if usize::from(def.channels()) >= channels { return Some(def); }
    let rate = def.sample_rate();
    let wide = device.supported_output_configs().ok()?
        .filter(|r| r.min_sample_rate() <= rate && rate <= r.max_sample_rate())
        .max_by_key(|r| (r.channels(), r.sample_format() == def.sample_format()))?;
    if wide.channels() <= def.channels() { return Some(def); }
    Some(super::host::sane_config(wide.with_sample_rate(rate)))
}
```

If the device still has fewer outputs than wanted, speakers mapped past the end are silently dropped (`frame.get_mut(o)` returns None). There is no warning.

The mix graph always produces a speaker "bed" in 3D Space sessions (`Graph::bed()`, interleaved, **before** the master fader). The render thread pushes it to a separate ring of `cap / 2 * nb` floats. The audio thread writes each bed channel to its output times the master gain, and puts the click on the stereo pair, added to whatever speaker is on that output ([`mix`, L1337-L1359](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/engine/transport.rs#L1337-L1359)). Meters, phase wheel and scope still read the stereo fold.

Tests (`engine/transport/tests.rs`): `speakers_play_on_their_mapped_outputs` (L on 3, R on 1, C on 2; output 4 silent; the stereo fold is not played), `stereo_plays_on_the_picked_pair`, `extra_device_channels_stay_silent`.

### 4.4 Folding

- **Stereo listen mode:** the graph folds the bed with per-speaker `bed_fold: Vec<[f32; 2]>` gains.
- **Clip fold-down (Source Channels):** a file with more than 2 channels plays the standard fold-down by default: fronts straight, center and surrounds 3 dB down, LFE left out. Per clip, a grid of left and right amounts per source channel overrides it.
- **Mono device:** playback writes only L to output 0 when the device has 1 channel; monitoring writes `(l + r) * 0.5`.
- **Mono check:** a global `MONO_CHECK` atomic folds to mono on the audio thread (listening only).

### 4.5 speaker_feed.rs (multichannel clips to speakers)

[`speaker_feed.rs`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/engine/speaker_feed.rs) is not device mapping. Before playback or export it rewrites a 3D Space session so each channel of a clip with more than 2 channels plays from a hidden track pinned to one speaker (stable derived ids so live edits still reach the copies). If a plug-in needs every channel at once (`routing.wide`), the copies feed a hidden 3D Bus that runs the effects. The rewritten session then flows into the same bed and `OutputMap` path. Mentioned here only because `Transport::play_with_click` and `sync` call `speaker_feed::expanded` before starting.

### 4.6 What a 7.1.4 rig needs

12 outputs. The device must expose them in one config. On ASIO that is the driver's full output count (cpal's `default_output_config` on ASIO reports all channels). Monitoring and live MIDI must open the same width as playback (2.7). Previews open `default_output_config()` and write their own channel count, so they play on outputs 1 and 2 only.

---

## 5. Build and CI

### 5.1 How the ASIO build works

- Cargo feature: `asio = ["cpal/asio"]` in [`src-tauri/Cargo.toml`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/Cargo.toml#L80-L83). Comment: "Needs LLVM (for bindgen) to build; the ASIO SDK is fetched by cpal's build script."
- The SDK is found by **asio-sys 0.4.0's** `build.rs` (not cpal's):
  1. If `CPAL_ASIO_DIR` is set, use that folder.
  2. Else if `%TEMP%\asio_sdk` exists, use it (cached from a previous build).
  3. Else download `https://www.steinberg.net/asiosdk` with PowerShell `Invoke-WebRequest` to `%TEMP%\asio_sdk.zip`, `Expand-Archive`, and rename the extracted `asio*` folder to `%TEMP%\asio_sdk`.
- It then compiles the SDK's C++ glue into `libasio.a` (on MSVC it runs `vcvarsall.bat` itself if `VCINSTALLDIR` is not set) and generates bindings with `bindgen`, which needs libclang. On non-Windows targets it writes stub bindings and does nothing.
- Links `advapi32`, `ole32`, `user32`.
- The vendored cpal's own `build.rs` sets `cfg(asio)` when `CPAL_ASIO_DIR` is set, but nothing in the vendored source uses `cfg(asio)`; the code is gated on `feature = "asio"` and `target_os = "windows"`. So `CPAL_ASIO_DIR` is optional.

### 5.2 CI exactly

All three workflows use `windows-latest`, `dtolnay/rust-toolchain@stable`, `Swatinem/rust-cache@v2` (workspace `src-tauri`), and set only one env var for ASIO:

```yaml
env:
  LIBCLANG_PATH: C:\Program Files\LLVM\bin
```

LLVM is preinstalled on GitHub's Windows runners. No `CPAL_ASIO_DIR`: the SDK downloads each build (or comes from `%TEMP%` if cached).

- [`release.yml` L63-L89](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/.github/workflows/release.yml#L63-L89): step `asio` runs `npm run tauri build -- --features asio` with `continue-on-error: true`. If it fails, a second step builds without the feature, and a `NO-ASIO.txt` file is dropped in the output; the publish job turns that into a `::warning::The ASIO build failed; the Windows installer has no ASIO.`
- [`test-build.yml` L52-L61](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/.github/workflows/test-build.yml#L52-L61): same with/without pattern.
- [`ci.yml` L63-L95](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/.github/workflows/ci.yml#L63-L95): job `windows-asio` runs `cargo clippy --all-targets --features asio -- -D warnings` (separate cache key `asio`). This is the only check that the ASIO code compiles.
- Linux jobs install `libasound2-dev`.

### 5.3 Local build on Windows

1. Install Visual Studio Build Tools (MSVC C++).
2. Install LLVM and set `LIBCLANG_PATH` to its `bin` folder (for example `C:\Program Files\LLVM\bin`).
3. Optional: download the ASIO SDK yourself, unzip it, and set `CPAL_ASIO_DIR` to the folder that holds `common/`, `host/` and so on. This avoids a network fetch inside the build and pins the SDK version.
4. `cargo build --features asio`.

For Audiform 2, I recommend pinning the SDK: download it in CI as its own step with a checked SHA-256, set `CPAL_ASIO_DIR`, and fail loudly instead of shipping a silent no-ASIO fallback, or keep the fallback but make it visible in the app ("This build has no ASIO") as Audiform 1 does via `ASIO_BUILT`.

### 5.4 Licensing

- ASIO SDK 2.3 is dual licensed by Steinberg: proprietary or GPLv3. Audiform uses the GPLv3 option ([`ATTRIBUTION.md` L71](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/ATTRIBUTION.md#L71)). That fits a GPL-3.0-or-later app; the combined Windows binary is effectively GPLv3.
- Trademark: "ASIO is a trademark of Steinberg Media Technologies GmbH." Use the name only to say what is supported. Never use the ASIO logo.
- cpal and asio-sys are Apache-2.0. The patched cpal must keep its notice and say it is modified (AUDIFORM.md does that).
- The SDK is not committed to the repo; it is fetched at build time.

---

## 6. Bugs we hit and how we fixed them

Commit `fc2ee08` is the first commit in the public history (the history was restarted for the public release), so things "present since fc2ee08" have no separate commit.

| # | Symptom | Cause | Fix | Where |
| --- | --- | --- | --- | --- |
| 1 | App closed instantly when Settings > Devices opened, even with no interface (0.1.7). | Leading suspect: listing ASIO drivers made every installed driver load, in parallel lookups; a native crash in a driver cannot be caught. | Lookups run one after another; driver names come from the registry (`reg query HKLM\SOFTWARE\ASIO`); a driver loads only when ASIO is turned on. Unverified cause. | `host::asio_drivers`; decisions 2026-10-20 and 2026-10-21; present since `fc2ee08` |
| 2 | Playback failed with "No audio output device found" after recording on ASIO. | ASIO drivers open for one program at a time; recording held it. | Playback and preview try the chosen host, then the system host (`output_device_any`). | `output.rs` L121-L128; decision 2026-10-22; `fc2ee08` |
| 3 | Device lists came back empty right after an update restart or a plug-in crash. | The old process still held the ASIO driver. | `retry_if_empty`: 4 retries, 400 ms apart, ASIO only. UI keeps the last non-empty list and has Refresh Devices. | `commands/devices.rs` L24-L43; 0.5.0 |
| 4 | First real ASIO test: no devices listed, crash on Play (15 drivers installed). | Upstream cpal loads every driver to enumerate and stops at the first already loaded; with no default it opened ASIO4ALL. | Patched cpal `asio_device_named`; driver picker; `pick_driver` skips wrappers and virtual cables. | `vendor/cpal`, `host.rs`; commit [`2991ad5`](https://github.com/nwh212/Audiform/commit/2991ad5); decision 2026-10-31 |
| 5 | Playback planned at 0 Hz on some drivers. | Driver reports rate 0 until a stream runs. | `sane_rate`/`sane_config` use 48 kHz; cpal sets it on the driver. | `host.rs` L248-L271; `2991ad5` |
| 6 | A panic in a callback (bug or odd driver data) would close the app. | Panics on the audio thread unwind into the driver. | `guard_out`/`guard_in` with `catch_unwind`; silence out; `FAULT` stops playback; panic hook logs. | `host.rs` L331-L385; `2991ad5` |
| 7 | Crash on first Record at a 64-frame buffer. | Monitor opened ASIO output at 2 channels, Play asked for 4; cpal reused the 2-channel stream and read past its buffers. | cpal refuses a wider stream (`DeviceBusy`); monitoring opens output the same width as playback. | `vendor/cpal/src/host/asio/stream.rs` L874-L942, `record::open_monitor`; commit [`c8707e0`](https://github.com/nwh212/Audiform/commit/c8707e0) |
| 8 | Changing the ASIO driver or buffer did nothing for armed tracks until re-arm. | Settings applied only to the next stream opened. | The UI reopens armed inputs after `set_audio_driver`; while recording, a notice. | commit [`feb960f`](https://github.com/nwh212/Audiform/commit/feb960f) |
| 9 | Recording at 88.2 kHz on ASIO stopped after 2 to 3 seconds. | Driver sent a reset request; Audiform only logged it; driver stopped sending audio. | Record thread restarts input and monitor on `StreamInvalidated`, pads the gap with silence, stops the take if the rate changed. | `record.rs` L999-L1055 and `build`; commit [`58eff18`](https://github.com/nwh212/Audiform/commit/58eff18) (then `1af4f6c`, a clippy fix) |
| 10 | Same reset requests during playback cause gaps. | Transport does not handle `StreamInvalidated`. | **Not fixed.** Known issue: use a larger buffer at 88.2/96 kHz; stop and play again. | release notes 0.7.1, 0.8.0, 1.0.0 |
| 11 | After Stop, audio "drags and glitches and keeps playing" on ASIO with a MIDI track armed or monitoring on. | Driver keeps running for the other stream and replays the closed stream's last buffers. | `Hush`/`QuietStream`: 3 silent callbacks (max 400 ms) before every output stream drops. | `host.rs` L273-L329; commit [`e33da0b`](https://github.com/nwh212/Audiform/commit/e33da0b) |
| 12 | Live MIDI stuttered even at a 512 ASIO buffer. | Fixed 256 to 384 frame queue vs 512-frame callbacks. | `ahead_for(buffer) = max(2*buffer + 128, 256)`. | `midi_live.rs` L554-L565; `e33da0b` |
| 13 | Instrument silent while recording MIDI on ASIO. | Guess: driver stops serving the live player's stream when another stream opens. | Reopen the live player's stream after 500 ms with a full queue and no consumption. Unconfirmed on hardware. | `midi_live.rs` L607-L662; commit [`ba6529b`](https://github.com/nwh212/Audiform/commit/ba6529b) |
| 14 | Plug-ins restarted at Play. | Plug-ins warmed up at the device default rate; playback opened at another rate (ASIO at a set rate). | `output::note_rate` / `device_rate` remember the last playback rate. | `output.rs` L130-L151; commit [`fa889fc`](https://github.com/nwh212/Audiform/commit/fa889fc) |
| 15 | System audio takes drifted out of sync with mic takes. | WASAPI loopback sends nothing during silence. | `silence_to_add`: pad by wall clock when more than 100 ms behind, stop 50 ms short. | `record.rs` L788-L796; test `pads_system_audio_only_when_well_behind`; `fc2ee08` |
| 16 | Monitoring latency crept up over time. | Unbounded monitor queue. | `MONITOR_MAX_QUEUE = 0.08` s; extra audio dropped. | `record.rs` L446; `fc2ee08` |
| 17 | 10 to 25 s from Play to sound in heavy sessions. | Not a device problem: plug-ins cold-started one after another. Found through the timing lines in `audio.log`. | Plug-ins start side by side; timing logged per stage. | `transport::run`; commit [`c8707e0`](https://github.com/nwh212/Audiform/commit/c8707e0); decisions 2026-10-31 |

---

## 7. Recommended design for Audiform 2

### 7.1 Keep

- **The audio-thread rule**: no allocation, locks, I/O or logging in callbacks. Pre-allocate scratch. Atomics for gains and meters (`f32` bits, `fetch_max` for peaks).
- **rtrb rings** between a render thread and the device thread, with render ahead and pre-fill before `play()`.
- **Load only the chosen ASIO driver**, list names from the registry, guess away from wrappers (`pick_driver` and its test).
- **`sane_rate`** for drivers that report 0 Hz.
- **`catch_unwind` around callbacks**, a fault flag polled off the audio thread, and a panic hook that logs.
- **The audio log**: driver, device, rate, channels, format, buffer for every stream; every stream error; play-start timings; periodic render load. It paid for itself many times.
- **End streams on silence** before dropping them (Hush), at least for ASIO.
- **Queue depth from the real buffer size** (`ahead_for`), not a fixed number.
- **Handle `StreamInvalidated`** by restarting the stream and padding takes with silence; stop a take on a rate change.
- **Pad loopback captures by the clock.**
- **Cap the monitor queue** (80 ms) and drop instead of letting latency grow.
- **Fallback to the system host** for playback when ASIO is busy, and say so in the UI.
- **OutputMap** semantics: stereo pair plus per-speaker outputs, unnamed speakers on their layout index.

### 7.2 Do differently

1. **One output stream per device, one input stream per device.** This is the biggest change. Audiform 1 opened up to four output streams on one ASIO driver and spent most of its ASIO bug budget on that: replayed buffers (6.11), width mismatches (6.7), stalls (6.13). Build a single `AudioDevice` engine that owns the device's one output callback and mixes these sources into it, each through its own rtrb ring with its own gain:
   - playback (multitrack, already rendered ahead),
   - preview,
   - input monitoring,
   - live instruments (MIDI),
   - metronome.
   
   With one stream there is nothing to replay, the channel count is fixed once, and nothing can be starved by the driver. Sources come and go by sending a new source over a lock-free command queue (rtrb of commands, or a triple buffer of the source list). Removing a source fades it out over one buffer on the audio thread, which also removes the click Hush was hiding.
2. **Run input and output in one duplex callback on ASIO.** ASIO delivers input and output in the same buffer switch. Monitoring could then be sample-accurate with latency of one buffer instead of "device buffers plus 10 to 30 ms". cpal does not offer a duplex callback; options are (a) keep cpal and pass input to output through a small ring (what A1 does), (b) write a thin ASIO layer on `asio-sys` directly with one callback for both directions, or (c) use cpal's ASIO but rely on the shared buffer switch. Option (b) also removes the need for the cpal patch. Decide early, because it shapes the engine API.
3. **No global mutable settings.** Replace `USE_ASIO`, `ASIO_DRIVER`, `ASIO_BUFFER`, `CHOICE`, `MAP`, `LAST_RATE` with an `AudioConfig` struct owned by the engine. Changing it is an explicit `reconfigure(config)` call that closes and reopens the device stream(s) in a defined order. Device listing takes a host argument instead of flipping a global (`devices.rs` race in 2.12).
4. **Structured device identity.** Store `{ host: HostKind, device: String (name or id), channels: Vec<u16> }` for track inputs, not `"Mono: <name> <n>"`. Keep a parser for A1 strings to read old sessions.
5. **Query real capabilities.** On ASIO ask for `buffersize_range()` (min, max, preferred, granularity) and offer only valid sizes. Show the driver's actual buffer size and input/output latency (cpal ASIO knows hardware latency) in Settings. Offer a sample-rate picker that calls `can_sample_rate`.
6. **Real buffer size everywhere.** Record the frame count of the first callback and use it for queue depths and latency display.
7. **Device change and loss.** Handle `StreamInvalidated` and device-not-available errors for every stream, with an automatic reopen (with backoff) and a clear status. A1 handles it only for input.
8. **Clock source.** A1 uses `played` frames from the output callback for playback position and the first input device's frame count for recording. With one duplex stream, use one device clock for both, and align takes by known latency instead of the manual offset.
9. **No Tauri.** In A1 the UI polled `transport_status` / `record_status` commands. In A2, publish meters and position from a non-real-time thread at UI rate (for example a `crossbeam` channel or an atomics snapshot read by the UI each frame). Keep the audio engine a library crate with no UI dependency, so it can be tested headless.
10. **Pin the ASIO SDK** in CI (download step with SHA-256, `CPAL_ASIO_DIR`), and keep `ASIO_BUILT` visible in the UI.

### 7.3 Ordered implementation checklist

Each step lists the tests to write. Tests that need no device should run on Linux CI; device tests can be `#[ignore]` and run by hand.

1. **Engine crate skeleton.** `AudioConfig`, `HostKind { System, Asio }`, error type, `audio.log` writer (size-capped at about 1 MB, timestamped), panic hook.
   - Tests: log rotation at the cap; config round-trip through serde.
2. **Host and device discovery.** System host via cpal. ASIO: registry listing, `pick_driver` with the `UNLIKELY` list, load one driver by name (port the cpal patch or use `asio-sys` directly).
   - Tests: port `the_driver_guess_skips_wrappers_and_virtual_cables`; registry parsing from a captured `reg query` text or a mocked key list; `asio_stays_off_when_not_built`.
3. **Config choice.** Port `config_for` (default if wide enough, else widest at the default rate, prefer default format), `sane_rate`, buffer size validation against the driver range.
   - Tests: `a_zero_rate_becomes_48k`; a fake list of `SupportedStreamConfigRange` values to check widest selection and format preference; buffer validation (min, max, step).
4. **Single output stream with a source mixer.** Device callback mixes N source rings into the device channels via an output map; fades sources in and out; counts underruns; writes silence when a source runs dry. Wrap in `catch_unwind`.
   - Tests (pure function on slices, like A1's `mix` tests): gains and meters; underrun count; mapped outputs (port `speakers_play_on_their_mapped_outputs`, `stereo_plays_on_the_picked_pair`, `extra_device_channels_stay_silent`); a removed source fades to zero within one buffer; a panic inside leaves silence and sets the fault flag (port `a_fault_in_a_callback_leaves_silence`); no allocation in the callback (a test allocator that panics on alloc inside the callback scope).
5. **Stream lifecycle.** Owner thread per device; ready channel with timeout; reconfigure closes on silence (keep Hush as a safety net for ASIO) and reopens; `StreamInvalidated` triggers reopen with backoff; fallback to the system host for playback.
   - Tests: a fake backend trait (`trait AudioBackend { fn open_output(...); fn open_input(...); }`) with a mock that can emit errors and stop calling back; check reopen happens, check settle waits for 3 callbacks or 400 ms; check fallback order.
6. **Playback source.** Render thread, rings sized from the real buffer and a render-ahead time, pre-fill before start, position clock from frames played, live gains via atomics.
   - Tests: port `position_follows_loops_and_skips`, `render_meters_wait_for_the_device_to_play_them`, `mono_check_folds_what_plays`, `the_click_plays_after_the_master_fader_and_meters`.
7. **Preview source.** Decode thread into a ring at device rate and channels; finished flag.
   - Tests: port `fill_copies_then_pads_with_silence_and_finishes`, `fill_converts_to_integer_formats`.
8. **Input stream and recording.** One input stream per device opened with all channels; per-take channel picking; WAV writers off the audio thread; live meters and 32-frame peak blocks; late-armed tracks start at the right frame; reset handling with gap padding and rate-change stop; loopback padding.
   - Tests: port `routes_follow_the_track_input` (plus the new structured form and the A1 string parser), `writer_splits_device_channels_into_takes`, `tracks_armed_while_recording_get_their_own_takes`, `live_tap_measures_each_target`, `pads_system_audio_only_when_well_behind`; a mock input that raises `StreamInvalidated` and checks the take length equals wall time.
9. **Input monitoring as a source.** Monitor mix (mono to both sides, fader and pan law from `monitor_gains`, chain), queue cap 80 ms, rate check with a clear message. On ASIO consider the duplex path.
   - Tests: port `monitor_mixes_only_marked_targets_to_stereo`, `monitor_applies_the_tracks_built_in_effects`; queue never exceeds the cap under a slow consumer.
10. **Live instrument source.** Render in small blocks (A1: 128) with queue depth `max(2 * buffer + block, 256)` from the real buffer size. With a single stream the stall reopen should not be needed; keep a watchdog log line if the consumer stops.
    - Tests: port `live_queue_covers_two_device_buffers` (with the real buffer size as input).
11. **Multichannel.** Output map UI and storage; speaker bed to outputs; warn when the device has fewer outputs than the map needs (A1 silently drops them).
    - Tests: mapping beyond device width produces a warning value; 7.1.4 (12 speakers) maps 1:1 by default.
12. **Settings and diagnostics.** Driver picker, buffer list from the driver, sample-rate picker, actual latency shown, log path, Refresh Devices, retry-if-empty on ASIO (4 x 400 ms).
    - Tests: port `the_system_driver_is_looked_up_once`, `a_list_that_has_items_is_not_asked_for_again`.
13. **CI.** Windows job with `--features asio`, pinned SDK, `LIBCLANG_PATH`; Linux job with `libasound2-dev`; headless engine tests on both.
14. **Hardware checklist (manual, needs a real ASIO interface).** Play, Stop, Play repeatedly with monitoring on and a MIDI track armed (no replay, no stutter); 64-frame buffer then Record then Play (no crash); 88.2 and 96 kHz at 64 and 128 frames (resets handled for playback and recording); unplug and replug the interface during playback; start a second app that holds the driver, then Play (fallback); a 12-output rig in Speakers mode.

---

## 8. File map

Paths are relative to the repo root. Links use the `A1/` prefix from the top of this guide.

| File | What it does | Key types and functions |
| --- | --- | --- |
| [`src-tauri/src/engine/host.rs`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/engine/host.rs) | Driver choice (system or ASIO), ASIO driver and buffer settings, registry listing, audio log, fault guard, Hush, worker pool | `ASIO_BUILT`, `ASIO_BUFFERS`, `set_asio`, `asio`, `set_asio_driver`, `asio_driver`, `pick_driver`, `asio_buffer`, `buffer_size`, `host`, `host_for`, `asio_device`, `asio_drivers`, `log`, `describe`, `sane_rate`, `sane_config`, `Hush`, `QuietStream`, `guard_out`, `guard_in`, `take_fault`, `install_panic_log`, `init_threads`, `worker_threads` |
| [`src-tauri/src/engine/output.rs`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/engine/output.rs) | Which output plays, output map, wide config choice, last playback rate | `OutputMap`, `set_map`, `map`, `config_for`, `output_channels`, `set_choice`, `choice`, `output_device`, `output_device_any`, `note_rate`, `device_rate` |
| [`src-tauri/src/engine/transport.rs`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/engine/transport.rs) | Multitrack playback: render thread, rings, the output stream, mixing, meters, clock, live updates, restart in place | `Transport` (`play`, `play_without_click`, `stop`, `status`, `sync`), `Shared`, `Clock`, `Jump`, `Routing`, `start`, `run`, `render_loop`, `render_block`, `build`, `mix`, `AHEAD_SECONDS = 0.5`, `BLOCK = 2048`, `MONO_CHECK` |
| [`src-tauri/src/engine/transport/tests.rs`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/engine/transport/tests.rs) | Unit tests on `mix` and the clock | `mix_applies_gains_meters_and_counts_frames`, `speakers_play_on_their_mapped_outputs`, `stereo_plays_on_the_picked_pair`, `extra_device_channels_stay_silent`, `position_follows_loops_and_skips` |
| [`src-tauri/src/engine/record.rs`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/engine/record.rs) | Recording per input device, take files, live meters, input monitoring, driver reset handling, loopback padding, the shared monitor output | `RecordTarget`, `Take`, `Route`, `route`, `TakeWriter`, `LiveTap`, `MonitorMix`, `Recorder` (`start`, `add`, `remove`, `stop_monitor`, `stop`, `status`), `spawn_device`, `find_device`, `SYSTEM_AUDIO_PREFIX`, `silence_to_add`, `run`, `open_monitor`, `build_out`, `build`, `MONITOR_MAX_QUEUE = 0.08`, `LIVE_BLOCK = 32` |
| [`src-tauri/src/engine/record/tests.rs`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/engine/record/tests.rs) | Unit tests for routes, writer, taps, monitor mix, loopback padding | `routes_follow_the_track_input`, `writer_splits_device_channels_into_takes`, `pads_system_audio_only_when_well_behind` |
| [`src-tauri/src/engine/midi_live.rs`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/engine/midi_live.rs) | MIDI inputs, live instrument voices with their own output stream, stall reopen, MIDI takes | `MidiLive`, `Player`, `Voice`, `Keys`, `run`, `ahead_for`, `BLOCK = 128`, `AHEAD = 256`, `STALL = 500 ms` |
| [`src-tauri/src/engine/preview.rs`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/engine/preview.rs) | File and generated-sound preview on its own stream | `PreviewPlayer`, `Shared`, `run`, `decode_loop`, `build`, `fill`, `BUFFER_SECONDS = 0.5` |
| [`src-tauri/src/engine/speaker_feed.rs`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/engine/speaker_feed.rs) | Rewrites 3D Space sessions so multichannel clips play each channel from its speaker | `expanded`, `plays_to_speakers`, `target`, `stand_in` |
| [`src-tauri/src/dsp/space.rs`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/dsp/space.rs) | Speaker layouts and their order | `speakers`, `channel_mask`, `layout_info` |
| [`src-tauri/src/engine/graph.rs`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/engine/graph.rs) | Mix graph on the render thread; produces the stereo mix and the speaker bed | `Graph`, `Mix`, `bed`, `bed_channels`, `lead` |
| [`src-tauri/src/commands/devices.rs`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/commands/devices.rs) | Tauri commands: list inputs and outputs, driver info, set driver, set output and map, open system sound settings, log line | `AudioInput`, `AudioOutput`, `AudioDriver`, `retry_if_empty`, `audio_inputs`, `audio_outputs`, `audio_driver`, `set_audio_driver`, `set_audio_output`, `set_output_map`, `output_channels`, `log_line`, `open_sound_settings` |
| [`src-tauri/src/commands/record.rs`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/commands/record.rs) | Record commands; stops preview before recording; the UI starts playback right after | `record_start`, `record_monitor`, `record_add`, `record_remove`, `record_stop`, `record_status` |
| [`src-tauri/src/lib.rs`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/lib.rs#L63-L85) | Startup: worker pool, audio log path, panic hook | `init_threads`, `set_log_path`, `install_panic_log` |
| [`src-tauri/vendor/cpal/AUDIFORM.md`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/vendor/cpal/AUDIFORM.md) | What the cpal patch changes | |
| [`src-tauri/vendor/cpal/src/host/asio/device.rs`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/vendor/cpal/src/host/asio/device.rs#L157-L167) | One-driver enumeration | `Devices::named` |
| [`src-tauri/vendor/cpal/src/host/asio/mod.rs`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/vendor/cpal/src/host/asio/mod.rs#L49-L54) | One-driver device | `Host::device_named`, `GLOBAL_ASIO` |
| [`src-tauri/vendor/cpal/src/host/asio/stream.rs`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/vendor/cpal/src/host/asio/stream.rs) | ASIO streams: shared buffers, silence-then-add mixing, width guard, reset events, rate and buffer checks | `get_or_create_input_stream`, `get_or_create_output_stream`, `check_config`, `ASIO_EVENT_DEBOUNCE = 500 ms` |
| [`src-tauri/vendor/cpal/src/platform/mod.rs`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/vendor/cpal/src/platform/mod.rs#L918-L929) | Public entry point for the patch | `Host::asio_device_named` |
| [`src-tauri/Cargo.toml`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/Cargo.toml) | `asio` feature, cpal patch, rtrb, midir | `[features] asio`, `[patch.crates-io] cpal` |
| [`.github/workflows/release.yml`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/.github/workflows/release.yml) | Release builds: ASIO first, fallback without, NO-ASIO warning | step `asio`, `LIBCLANG_PATH` |
| [`.github/workflows/test-build.yml`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/.github/workflows/test-build.yml) | Windows test builds, same pattern | |
| [`.github/workflows/ci.yml`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/.github/workflows/ci.yml) | `windows-asio` clippy job; Linux job with `libasound2-dev` | |
| [`docs/decisions.md`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/docs/decisions.md) | Reasons and hardware reports. ASIO entries: L857 (0.1.5), L906 and L934 (Devices crash), L956 (fallback), L1426-L1431 (Channel Routing), L2097-L2105 (one driver), L2132 (width crash), L2140 (reopen on change), L2282-L2285 (reset restart), L2661-L2665 (Hush, ahead_for), L2708-L2712 (MIDI stall) | |
| [`ATTRIBUTION.md`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/ATTRIBUTION.md#L70-L71) | cpal patch and ASIO SDK licence lines | |

---

## Open questions (not determinable from the code)

- **Exactly why a closed ASIO stream's buffers replay.** The fix is recorded and worked for the reporter; the mechanism in cpal's shared-buffer callback is plausible but not proven here.
- **Whether the MIDI stall reopen fixes the "instrument silent while recording MIDI" report.** The decision log calls the cause a guess, and the release notes keep it as a known issue.
- **Whether the Devices-page crash (0.1.7) was the ASIO driver load.** Recorded as "leading suspect; unverified".
- **Which host produced duplicate output names** that `outputs()` dedups.
- **ASIO input and output latency.** cpal reads hardware output latency, but Audiform 1 never shows or uses it; the recording offset is a manual setting measured with a clap.
- **Linux and macOS device behaviour.** No audio bugs specific to them are recorded; those builds are experimental and mostly untested.
