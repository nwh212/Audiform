# VST3 plug-in hosting: a handoff guide for Audiform 2

This guide is for the engineer building Audiform 2, a pure Rust rewrite of the Audiform audio editor. It explains how Audiform 1 hosts VST3 effects and instruments, what went wrong along the way, and what to keep or change.

Everything here comes from the Audiform 1 source at commit `042577e1ddee7b9718d82aea7947f9aab6edd628` (public as `nwh212/Audiform`). Links below point at that commit. When the code or the decisions log does not say something, this guide says so.

Links:

- Files: `https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/<path>`
- Commits: `https://github.com/nwh212/Audiform/commit/<hash>`

---

## 0. Licences first

**Audiform 1 code is GPL-3.0-or-later.** The licence is in [`LICENSE`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/LICENSE) (GPL v3). Most plug-in files carry this header:

```rust
// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.
```

Two files in this area carry `Copyright (c) The Audiform contributors.` instead: `src-tauri/src/plugins/presets.rs` and `src-tauri/src/dsp/effects/routing.rs`.

If Audiform 2 copies or adapts any of this code (the `plughost` crate is a strong candidate, see section 9), the copied files keep GPL-3.0-or-later and keep their copyright lines. Add your own line under them if you change them. Audiform 2 as a whole then has to be GPL-compatible.

**The VST3 SDK.** [`ATTRIBUTION.md`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/ATTRIBUTION.md) says:

> `vst3` (bindings generated from the VST 3 SDK headers) and `com-scrape-types` | MIT or Apache-2.0 | Hosting VST3 plug-ins, in a separate process (`src-tauri/plughost`). The VST 3 SDK itself is MIT licensed by Steinberg (since 2025). VST is a trademark of Steinberg Media Technologies GmbH; we do not use the VST logo. VST2 is not supported (its SDK is no longer licensed).

So: the Rust bindings (`vst3` crate, coupler-rs) are MIT or Apache-2.0, the SDK headers they come from are MIT since 2025, VST2 is out, and the VST logo is not used. ATTRIBUTION.md also lists `libloading` (ISC) and `windows-sys` (MIT or Apache-2.0).

**The test plug-in** (`src-tauri/plughost/testplug/src/lib.rs`) is adapted from the `vst3` crate's `examples/gain.rs` (MIT or Apache-2.0, Copyright (c) Micah Johnston). The file header and ATTRIBUTION.md credit it. Keep that credit if you reuse it. It is never shipped.

ASIO (also Steinberg) is a separate topic: ATTRIBUTION.md says the ASIO SDK is used under its GPLv3 option. Not covered here.

---

## 1. Architecture

### 1.1 The big picture

```
 Audiform app process (no unsafe code; #![forbid(unsafe_code)] via [lints.rust])
 ├─ plugins/mod.rs      scanner, registry (plugins.json)
 ├─ plugins/instance.rs Host (one plug-in), Conn (one host process), pool, Instrument
 ├─ dsp/effects/*       ChainProcessor; a plug-in is one Processor stage
 └─ engine/*            renderer, mix graph, transport, live MIDI
        │  stdin/stdout pipes, framed binary + JSON
        ▼
 Host process: the same Audiform executable, started with --plugin-host
 (crate altered-plughost, the ONLY crate allowed unsafe)
 ├─ stdin thread "plugin-audio": reads frames, runs IAudioProcessor::process
 └─ main thread: loads plug-ins, controller calls, state, editor windows

 Scan process: the same executable with --plugin-scan <bundle>
```

The app's `main.rs` checks the command line first ([`src-tauri/src/main.rs`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/main.rs)):

```rust
// Started again as a plug-in host or scanner: see the plughost crate.
if let Some(code) = altered_plughost::main_with_args(&args) {
```

and [`plughost/src/lib.rs#L46-L56`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/plughost/src/lib.rs#L46-L56):

```rust
pub fn main_with_args(args: &[String]) -> Option<i32> {
    let mode = args.get(1).map(String::as_str);
    if matches!(mode, Some("--plugin-host" | "--plugin-scan")) {
        init_com();   // OleInitialize on Windows
    }
    match mode {
        Some("--plugin-host") => Some(serve()),
        Some("--plugin-scan") => Some(scan(Path::new(args.get(2).map_or("", String::as_str)))),
        _ => None,
    }
}
```

Why the same executable: "No second program to install or sign" (decisions log, 2026-10-24 "VST3 plug-ins"). Tests point the app at a different host with the env var `ALTERED_PLUGIN_HOST` ([`plugins/mod.rs#L347-L352`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/plugins/mod.rs#L347-L352)). The host crate also builds a tiny stand-alone binary, `src/bin/altered-plughost.rs`, used only by the host crate's own tests.

### 1.2 Why out of process

From the decisions log (2026-10-24, "VST3 plug-ins"):

- A plug-in that crashes or hangs takes down only its host process. The editor sees the pipe close and bypasses that slot.
- Talking to plug-ins needs `unsafe`, which the app crate forbids (`[lints.rust] unsafe_code = "forbid"` in `src-tauri/Cargo.toml`). So it lives in its own crate.
- A plug-in window that hangs never freezes the editor, because the window lives in the host process.

### 1.3 One process per maker (the "Run together" choice)

At first every plug-in had its own process. Two things changed that:

1. **Plug-ins that must find each other.** Some makers' plug-ins talk while they play, for example an immersive panner per track and a renderer on the mix. That only works inside one process. Added 2026-10-25 ([55fdd08](https://github.com/nwh212/Audiform/commit/55fdd08)). Fiedler Audio was ticked by default.
2. **Memory.** "Each copy of a plug-in had a host process of its own, and each process loads the maker's runtime again (a Waves shell, iZotope's framework, UA's), tens of MB per copy before the plug-in itself." On the lead's 32-track session hosts used 2.4 GB. So from 2026-11-01 every maker's plug-ins run together by default ([190c25e](https://github.com/nwh212/Audiform/commit/190c25e)).

The Plug-in Manager lists makers with a "Run together" checkbox. Unticking a maker gives each of its plug-ins its own process. The cost of sharing: a crash or hang in one plug-in stops every plug-in in that process (they are all bypassed; the session carries on).

How the app picks a process ([`instance.rs#L306-L326`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/plugins/instance.rs#L306-L326)):

```rust
fn conn_for(vendor: &str, rate: u32) -> Result<Arc<Conn>, String> {
    let maker = vendor.trim().to_lowercase();
    let shared = !separate_makers().lock().is_ok_and(|s| s.contains(&maker));
    if !shared {
        return Conn::spawn();
    }
    let key = (SCOPE.with(Cell::get), maker, rate);
    ...
}
```

The key is `(scope, maker lower case, sample rate)`. Scope 0 is playback. Each mixdown gets a new scope number (see `fresh`, section 3.1), so a mixdown never joins playback's processes. The map holds `Weak<Conn>`, so a process ends when its last plug-in is dropped.

A plug-in with a blank vendor takes the vendor from the factory's `PFactoryInfo` ([`module.rs#L115-L132`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/plughost/src/module.rs#L115-L132)). Without that, grouping by maker broke for such plug-ins.

Registry fields: `separate_makers: Option<Vec<String>>` in `plugins.json`. An older field `shared_makers` is read once for migration and never written again ([`plugins/mod.rs#L87-L127`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/plugins/mod.rs#L87-L127)). The Tauri command `plugins_set_shared(makers)` takes the makers that run together. Changing the choice drops playback's running plug-ins of the changed makers so they start the new way next time (`set_separate_makers`, [`instance.rs#L262-L284`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/plugins/instance.rs#L262-L284)). The interface fetches their state first.

### 1.4 The IPC protocol, in full

Source of truth: [`plughost/src/protocol.rs`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/plughost/src/protocol.rs). Transport is the host process's **stdin and stdout pipes**. No shared memory. Stderr is set to null in host mode (`Conn::spawn`) and is piped and read in scan mode.

Why pipes, from the decisions log: "Playback renders half a second ahead on the render thread, so a pipe round trip per block costs nothing audible, and it needs no unsafe code on the editor's side."

#### Framing

Every message is one frame: a kind byte, a little-endian `u32` payload length, then the payload. Frames over 256 MiB are refused as invalid.

```rust
pub const CONTROL: u8 = 1;  // JSON: Request (to host) or Message (from host)
pub const AUDIO: u8 = 2;    // one audio block, binary
pub const SELECT: u8 = 3;   // u32 LE: which plug-in in the process the next frames are for
pub const KEY: u8 = 4;      // sidechain key for the next AUDIO block: interleaved stereo f32
pub const MAX_BLOCK: usize = 8_192; // largest block a host processes per call; bigger ones are split
```

`SELECT` works in both directions. Frames before any `SELECT` are for plug-in 0, so a process with one plug-in never sends one. Both sides only write a `SELECT` when the target changes (`write_for` in `lib.rs`, `Conn::write` in `instance.rs`). Plug-in ids within a process come from `Conn::next_id` (an `AtomicU32` counter starting at 0).

#### AUDIO payload (app to host)

Written by `encode_audio_ch` ([`protocol.rs#L407-L437`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/plughost/src/protocol.rs#L407-L437)):

| Field | Type | Notes |
| --- | --- | --- |
| position | `i64` LE | Timeline frame of the first sample. `-1` = unknown, the plug-in keeps counting on. |
| frames | `u32` | |
| channels | `u32` | 2, or the plug-in's own layout width. Host clamps to 1..64. |
| n changes | `u32` | |
| each change | `u32` id, `f64` value | Normalized 0..1. |
| m notes | `u32` | |
| each note | `u32` offset, `u8` on, `u8` pitch, `f32` velocity 0..1, `u8` channel 0..15, `f32` pan -1..1 | 15 bytes each. |
| samples | `f32` LE, interleaved | `frames * channels` values. |

Note: the module doc comment at the top of `protocol.rs` describes the AUDIO layout without the leading `position` field. That comment is stale. The code (and the round-trip test `frames_and_audio_round_trip`) is right.

#### AUDIO payload (host to app)

Just the processed samples, interleaved `f32` LE, same frame and channel count. No header, no sequence number.

#### KEY payload

Interleaved stereo `f32`, the same number of frames as the next AUDIO block for the selected plug-in. Sent just before that AUDIO frame. It counts for that one block only. The host drops a key whose length does not match `frames * 2`.

#### CONTROL: requests (app to host)

`serde` enum tagged by `"req"`, snake_case ([`protocol.rs#L236-L302`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/plughost/src/protocol.rs#L236-L302)):

| Request | Fields | Answer |
| --- | --- | --- |
| `load` | `path`, `cid` (32 hex), `rate` (f64), `state` (Option, base64), `params` (Vec<(u32,f64)>), `main` (Option<u64>) | `loaded` or `error` |
| `reset` | | `ok` |
| `get_state` | | `state` |
| `set_state` | `data` | `ok` or `error` |
| `open_editor` | `title`, `owner` (u64 HWND, 0 = none, omitted when 0), `bypassed` (omitted when false) | `ok` or `error` |
| `close_editor` | | `ok` |
| `editor_bypassed` | `bypassed` | `ok` |
| `hide_editor` | `hidden` | `ok` |
| `unload` | | `ok` |
| `tempo` | `bpm`, `beats_per_bar`, `beat_unit` (default 4), `start` (timeline frame), `start_music` (quarter notes) | `ok` |

`main` on `load`: missing = stereo, `Some(0)` = the plug-in's own layout, `Some(mask)` = a VST3 `SpeakerArrangement` bit mask to ask for.

#### CONTROL: messages (host to app)

Tagged by `"msg"` ([`protocol.rs#L304-L361`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/plughost/src/protocol.rs#L304-L361)). Replies: `loaded`, `state { data }`, `ok`, `error { message }`. Events (unasked):

| Event | Meaning |
| --- | --- |
| `param_edited { id, value }` | A control moved in the plug-in's own window (`IComponentHandler::performEdit`). |
| `latency_changed { latency }` | `restartComponent(kLatencyChanged)`. Value is `getLatencySamples()` at that moment. |
| `editor_closed` | The user closed the plug-in window. |
| `bypass_picked { bypassed }` | The power button on the window's bottom strip was clicked. |
| `dirty` | `restartComponent(kParamValuesChanged or kReloadComponent)`: settings changed that are not controls; fetch state to keep them. |
| `space_pressed { record }` | Space (Shift+Space = record) in the plug-in window, outside a text box. |

`loaded` carries: `latency`, `keyed_latency` (Option, only when there is a sidechain), `params: Vec<ParamInfo>`, `has_editor`, `instrument`, `inputs`/`outputs` (speaker names of the main buses), `sidechain`, `midi_map: Vec<(u8, u32)>`.

New fields were added with `#[serde(default)]` so an older host still parses. There is no explicit protocol version.

#### Example on the wire

```json
{"req":"open_editor","title":"Vocal"}
{"msg":"param_edited","id":3,"value":0.5}
```

(both from the unit test `json_messages_are_tagged`).

#### Settings blob format

`Plugin::state()` ([`plugin.rs#L356-L373`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/plughost/src/plugin.rs#L356-L373)) returns base64 of:

```
u32 LE  component state length
bytes   IComponent::getState
bytes   IEditController::getState (only when the controller is a separate object)
```

The protocol module has its own small base64 (`base64_encode`/`base64_decode`). This exact format is what Audiform 1 sessions store in `EffectSlot.plugin_state`. **Audiform 2 should read it unchanged** if it wants to open Audiform 1 sessions.

### 1.5 Threads inside the host process

From the crate doc and [`lib.rs#L113-L512`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/plughost/src/lib.rs#L113-L512):

- **Stdin thread** (`plugin-audio`, `audio_loop`): reads every frame. It owns the `Audio` half of each plug-in and calls `process` itself, so it is the plug-in's audio thread. It handles `reset`, `tempo` and `unload` directly. Everything else goes to the main thread over an `mpsc` channel (`ToMain`).
- **Main thread** (`main_loop`): loads plug-ins, sets controller values, gets and sets state, runs editor windows, pumps the Windows message queue. It waits on the channel with short timeouts: 10 ms with a window open, 15 ms with plug-ins loaded, 50 ms otherwise ("so their timers keep their pace").

A `load` is synchronous on the stdin thread: it sends `ToMain::Load` and blocks on a reply channel until the main thread has a `(Plugin, Audio)` pair, then keeps the `Audio`. **Inference from the code, not a reported bug:** while one plug-in loads in a shared process, the stdin thread processes no audio for the other plug-ins in that process. The app side bypasses late blocks (section 3.7), and a plug-in more than `STALL_LIMIT` (20 s) behind is killed, so a very slow load (licence checks) could in theory stop its siblings. Worth designing around in Audiform 2.

Control changes in an AUDIO block are applied by the processor and also forwarded to the main thread (`ToMain::Changes`) which calls `IEditController::setParamNormalized`, so the plug-in's own window shows them.

Edits from the plug-in's window go the other way: `Handler::performEdit` pushes `Edit::Param` into a shared `Vec`. The main loop drains it, pushes `(id, value)` into the `Audio::from_editor` queue (picked up at the start of the next block) and sends `param_edited` to the app.

`GetState` first flushes waiting changes on the stdin thread (`a.flush(&[])`) so the saved state includes them, then the main thread reads state.

Drop order matters and is encoded in struct field order: in `Loaded`, the window drops before the plug-in; in `Plugin`, plug-in objects drop before the module.

### 1.6 Process start, stop, crash and restart (app side)

`Conn::spawn` ([`instance.rs#L106-L202`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/plugins/instance.rs#L106-L202)) starts the host with `--plugin-host`, piped stdin/stdout, null stderr, and a reader thread (`plugin-pipe`). The reader routes each frame to a `Route` (by the last `SELECT`): AUDIO to an `mpsc::Sender<Vec<f32>>`, replies to a reply channel, events into an `Arc<Mutex<Events>>`.

When the pipe closes (crash or exit), the reader marks the `Conn` dead and writes `"The plug-in crashed and is bypassed."` into every route's `Events.stopped`, then clears the routes so anyone waiting on a channel hears at once.

Timeouts ([`instance.rs#L37-L49`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/plugins/instance.rs#L37-L49)):

```rust
pub const PROCESS_TIMEOUT: Duration = Duration::from_secs(4);   // one block
const STALL_LIMIT: Duration = Duration::from_secs(20);          // behind on blocks before "hung"
const LOAD_TIMEOUT: Duration = Duration::from_secs(90);         // licence checks can be slow
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);      // state, windows
```

A request that times out kills the process: `"The plug-in stopped answering and is bypassed."` A failed pipe write: `"The plug-in crashed and is bypassed."` `Conn::kill` kills the child and marks every plug-in in it stopped.

`Host::drop` sends `Unload` (if alive) and removes its route. Others in a shared process carry on. `Conn::drop` kills and waits for the child.

**The "stopped plug-in" notice.** `instance::take_events()` gathers `Events` from every pooled host plus start failures. The Tauri command `plugin_events` returns them. The interface (`src/app/plugin-sync.ts`, polled every 200 ms by `PluginSync.start()`) calls `onStopped`, which marks the slot (`stores/plugins.ts`, `stoppedPlugins`) and shows: "A plug-in stopped: {why} Restart it from its effect window." The slot shows red with the reason. A stopped plug-in opens the generic controls window instead of its own window, so the Restart button is reachable.

**Restart.** `plugin_restart(slot_id)` calls `instance::restart`, which removes the slot from the pool and clears its failure. The plug-in starts again at the next playback ("The plug-in starts again with the next playback.").

**While stopped**, `PluginProcessor` passes the dry signal through a `Delay` of the plug-in's latency, so the track stays in time.

---

## 2. Scanning and the registry

### 2.1 Where plug-ins are looked for

`standard_folders()` ([`plugins/mod.rs#L249-L276`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/plugins/mod.rs#L249-L276)):

- Windows: `%CommonProgramFiles%\VST3`, `%LOCALAPPDATA%\Programs\Common\VST3`
- macOS: `/Library/Audio/Plug-Ins/VST3`, `~/Library/Audio/Plug-Ins/VST3` (listed, but macOS bundles are refused at load: "Plug-ins on macOS are not supported yet.")
- Linux: `~/.vst3`, `/usr/lib/vst3`, `/usr/local/lib/vst3`

Users can turn standard folders off (`skipped`) and add their own (`folders`). `find_bundles` walks 4 levels deep, sorted, and takes anything with extension `.vst3` (case-insensitive), folder or single file.

Inside a bundle, `library_path` ([`module.rs#L43-L77`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/plughost/src/module.rs#L43-L77)) picks `Contents/x86_64-win/<stem>.vst3` on Windows and `Contents/x86_64-linux/<stem>.so` on Linux (or `aarch64-*`). A single-file `.vst3` on Windows is loaded directly.

### 2.2 Loading a module

`Module::open` loads the library with `libloading`, calls `InitDll` (Windows) or `ModuleEntry(null)` (Linux; "a null one is allowed"), then `GetPluginFactory`. `Module::drop` drops the factory, calls `ExitDll`/`ModuleExit`, then **forgets** the library instead of unloading it: "Many plug-ins leave threads running; unloading under them can crash the host on the way out."

Classes come from `IPluginFactory2::getClassInfo2` when available, else `getClassInfo`. Only category `"Audio Module Class"` counts.

### 2.3 The scan process

`scan_bundle` ([`plugins/mod.rs#L410-L474`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/plugins/mod.rs#L410-L474)) runs `<exe> --plugin-scan <bundle>` per bundle, `SCAN_THREADS = 4` at a time, each with `SCAN_TIMEOUT = 60 s`.

The scan process prints one line per step to stdout, each `@@altered-scan ` plus JSON of `ScanLine`:

```rust
pub enum ScanLine {
    Found(Vec<ClassInfo>),          // names first, before any plug-in starts
    Read(ClassInfo),                // one plug-in read in full
    Failed { cid: String, error: String },
    Error(String),                  // bundle could not be opened
    Done,
}
```

And stage lines on stderr, `@@altered-stage <what>` ("loading its file", "creating it", "starting it", "setting up its audio", "switching it on", "measuring its delay with the sidechain on", ...), so a crash can be placed.

`parse_scan_output` ([`protocol.rs#L165-L234`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/plughost/src/protocol.rs#L165-L234)) rebuilds the result even when the process died: plug-ins named in `Found` but never `Read` are kept with a `note` like "Not fully read. It crashed while switching it on: a memory access violation (code C0000005). It may still work; if it crashes when used, it is bypassed." Windows exit codes become words via `crash_reason` (access violation, DLL not found, not a 64-bit plug-in, and so on). Non-prefixed text a plug-in prints is ignored.

Rules learned the hard way (section 8 has the stories):

- Read the pipes **while** the scan runs (a `Drained` reader thread per pipe).
- A scan that printed `"done"` is finished, even if it then hangs on exit. Kill it.
- After the scan process ends, wait at most 2 s for the pipes to close (helper programs may keep them open).
- Open each bundle **once** and share the `Arc<Module>`; never close it before answering. In the scan, plug-ins are kept alive with `std::mem::forget(keep)` and the process ends with `std::process::exit(0)`.
- `stop_scan()` sets a flag that kills running scan processes; unread bundles stay unstamped for next time.

### 2.4 Instrument or effect

Two signals, either is enough ([`lib.rs#L552-L555`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/plughost/src/lib.rs#L552-L555) and [`plugin.rs#L258-L260`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/plughost/src/plugin.rs#L258-L260)):

```rust
// from the class's sub-categories
let named = |subs: &str| {
    let s = subs.to_ascii_lowercase();
    s.contains("instrument") && !s.contains("fx")
};
// from its buses, after loading
let instrument = events && ins.is_empty(); // has an event input and no audio input
```

`Found` (names only) uses the sub-category test. `Read` uses `named(..) || p.instrument`. Effects list in Voice Chains (`catalog()`), instruments only on MIDI tracks (`instruments()`).

### 2.5 What a scan records

`ClassInfo`: `cid` (32 hex), `name`, `vendor`, `version`, `sub_categories` (for example `"Fx|Pitch Shift"`), `params: Vec<ParamInfo>`, `latency`, `has_editor`, `instrument`, `sidechain`, `note`.

Each plug-in is fully loaded in the scan at 48 kHz (`Plugin::load_from(module, &cid, 48_000.0, None, None)`), so `latency` is the stereo, 48 kHz figure.

`has_editor` is `p.controller.is_some()`. It does **not** check that `createView` works. So "has a window" can be wrong; the open request then fails with "This plug-in has no window of its own." and the interface shows generic controls.

`ParamInfo.hidden` is set for `kIsReadOnly | kIsHidden | kIsProgramChange`.

If a running plug-in shows a sidechain the scan missed, `note_sidechain(cid)` updates and saves the registry.

### 2.6 The cache: `plugins.json`

In the app's config folder. Struct `Registry` ([`plugins/mod.rs#L69-L96`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/plugins/mod.rs#L69-L96)): `schema_version`, `folders`, `skipped`, `plugins: Vec<Found { bundle, info }>`, `failed: Vec<Failed { bundle, error }>`, `stamps: BTreeMap<PathBuf, u64>` (bundle modified time, seconds), `shared_makers` (legacy), `separate_makers`.

- `SCHEMA_VERSION = 4`. History: 2 = instruments listed, 3 = earlier scans crashed on many plug-ins, 4 = sidechain noted. An older file clears `stamps` and sets a "stale" flag; the interface asks `plugins_needs_rescan` and scans once by itself.
- A rescan skips bundles whose stamp is unchanged, unless "Rescan All". Failed bundles and partly read ones (`note.is_some()`) are retried every scan.
- `dedupe` keeps one entry per class id (case-insensitive), preferring a fully read copy. Two entries with one id broke the interface's lists.
- Saved atomically (write `plugins.json.tmp`, rename).

Scale note: the project lead had 910 plug-ins with 162,753 controls; `plugins.json` was 31 MB. The interface once held thirty copies of the catalog (one per sends row). Fetch it once and share it.

### 2.7 Effect ids and control keys

- A slot holding a plug-in has effect id `vst3:<32 hex class id>` (`PREFIX = "vst3:"`).
- Controls are stored like built-in effect settings: key `p<param id>` (`param_key`, `param_id`), value normalized 0..1.
- `effect_info` turns a `Found` into an `EffectInfo` for the generic UI: each param as a 0..1 number with step `1/steps` (or `0.001` when continuous), hidden ones marked. If the scan read no params, a running copy's params are used (`live_params`).
- Class id byte order differs on Windows (COM GUID layout) and elsewhere. Tests ask the registry by name instead of hard-coding ids. `.sesx` import tries both layouts; preset lookup matches both.

---

## 3. Processing

### 3.1 Who owns which host: pool, fresh, dedicated, borrow-only

All in [`plugins/instance.rs`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/plugins/instance.rs). Key types:

```rust
struct Pooled {
    effect: String,
    rate: u32,
    main: Option<u64>,          // layout asked for (see main_of)
    host: Arc<Mutex<Host>>,
}
fn pool() -> &'static Mutex<HashMap<Id, Pooled>>   // by slot id
```

- **Playback** uses the pool: one host per chain slot, kept as long as any open session uses it. The plug-in's window, settings and inner state carry on across stops, starts and chain edits.
- **`fresh(f)`**: mixdowns and stems. Hosts started inside it are not pooled and get a new shared-process scope. A mixdown first tries to **borrow** playback's host for a slot (`borrowable`: same effect, rate and layout, and `Arc::strong_count == 1`, meaning nothing is playing through it). That sounds identical to playback and starts at once.
- **`dedicated(f)`**: input monitoring and live MIDI. Always a copy of its own, never playback's ("one plug-in cannot serve two streams").
- **`borrow_only(f)`**: pictures at the playhead while stopped (spectrum behind an EQ, Phase Wheel, Visualizer). Uses idle playback hosts at playback's rate; skips anything not running. Nothing is started, stopped or restarted.

These are thread-local flags (`FRESH`, `OWN`, `SCOPE`, `BORROW_ONLY`, `LAYOUT`), which is simple but implicit. Audiform 2 should pass a context value instead.

`host_for(slot, rate)` ([`instance.rs#L987-L1086`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/plugins/instance.rs#L987-L1086)) is the one entry point. For playback it guarantees one start per slot (a `starting()` set; a second caller waits up to `LOAD_TIMEOUT` for the first). It reuses a pooled host when `fits`:

```rust
fn fits(e: &Pooled, slot: &EffectSlot, rate: u32) -> bool {
    e.effect == slot.effect
        && e.rate == rate
        && (e.main == main_of(slot) || (LAYOUT.with(Cell::get).is_none() && !wide_of(slot)))
}
```

The second clause means: a caller with no layout in mind (a plug-in window opening) takes whatever layout is running, so it never restarts a playing plug-in. Otherwise a mismatch logs `plug-in restart: "<name>" ran at X Hz, wanted at Y Hz` and starts a new host.

Background starts: `warm(slots)` after every edit or open, and `start_all(slots, rate)` when Play starts, both with up to `available_parallelism().clamp(2, 8)` workers. Play then waits for the slowest plug-in, not the sum. A resting check every 5 s (`spawn_plugin_rest` in `commands/session.rs`) stops plug-ins nobody needs (session set aside for a minute, track playing a pre-render), never one with its window open.

### 3.2 Block sizes and sample rates

There is no fixed block size. The host takes whatever arrives and splits anything over `MAX_BLOCK = 8192` into chunks; control changes go with the first chunk only. Seen in the code:

| Caller | Block | Source |
| --- | --- | --- |
| Playback render thread | 2048 frames, rendered 0.5 s ahead | `engine/transport.rs` (`BLOCK`, `AHEAD_SECONDS`) |
| Mixdown | 8192 | `export/mixdown.rs` (`BLOCK`) |
| Clip chain with automation | 256-frame steps | `engine/render.rs` (`Fx::run_at`, `STEP`) |
| Live MIDI playing | 128 frames, 256 kept ahead | `engine/midi_live.rs` |
| Note audition | 512 | `instance.rs` (`Audition::BLOCK`) |
| Instrument warm-up for exports | 1024 | `INSTRUMENT_WARMUP` loop |

`setupProcessing` always declares `maxSamplesPerBlock: MAX_BLOCK` and `kSample32`. The sample rate is the one given in `load`. A host runs at one rate for its life; a different rate means a new host (and, since 2026-11-01, plug-ins started ahead of Play use the rate the output stream opened at, to stop restart ping-pong).

Round-trip latency: the repo has **no measured number** for one pipe round trip. Evidence it is small: live MIDI plays instruments through the pipe in 128-frame blocks with about 5 ms kept ahead at 48 kHz, and works.

### 3.3 Plugin load sequence (host side)

`Plugin::load_from` ([`plughost/src/plugin.rs#L154-L343`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/plughost/src/plugin.rs#L154-L343)) in order:

1. `factory.createInstance(cid, IComponent::IID)`; `component.initialize(host)` with a minimal `IHostApplication` (`HostApp`: name "Audiform", `createInstance` for `IMessage` and `IAttributeList` only).
2. Controller: `component.cast::<IEditController>()` (single object), else `getControllerClassId` + create + `initialize` (separate). Separate halves are joined with `IConnectionPoint::connect` both ways.
3. State: saved state is split into component and controller parts. `component.setState`, then (separate controller) `setComponentState` and `setState`. With no saved state the component's own `getState` is handed to `setComponentState`. `setComponentHandler(Handler)`.
4. `setup_buses` (section 3.6).
5. `setupProcessing { processMode: kRealtime, kSample32, maxSamplesPerBlock: 8192, sampleRate }`.
6. `setActive(1)`, `setProcessing(1)`, `getLatencySamples()`.
7. If there is a sidechain: switch off, activate the extra inputs, switch on, read latency (`keyed_latency`), then switch them off again.
8. Read params (`getParameterCount`/`getParameterInfo`) and the MIDI map.
9. Back on the main loop: apply `params` from the request with `setParamNormalized` and also `Audio::flush(&params)` (see below).

`Audio::flush` sends a **zero-length** `process` call with only `inputParameterChanges` set. VST3 allows this. It makes control values count in saved state before playback starts.

### 3.4 Process mode: realtime always

Every `ProcessSetup` and `ProcessData` in the host uses `kRealtime`. `kOffline` and `kPrefetch` appear nowhere in the code. This includes mixdowns.

What exists instead is a **pacing** option: Settings > Effects > "Mix down sessions that use VST3 plug-ins in real time" (off by default; `MixdownRequest.realtime`). With it on, a mixdown sleeps so it stays no more than a block ahead of the wall clock ([`export/mixdown.rs`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/export/mixdown.rs), around line 296). The decisions log (2026-10-25) explains why: "Some plug-ins only work at normal speed: pitch correction, immersive panners and other tools that follow the clock or talk to each other while playing."

So "realtime vs offline" in Audiform 1 means "paced vs as fast as possible", never a VST3 process mode change. Whether some plug-ins would render better if told `kOffline` in faster-than-real-time mixdowns is untested. Audiform 2 could try `kOffline` for unpaced mixdowns behind a switch, but note that `setupProcessing` can only change while the plug-in is inactive, so it means a separate host (the mixdown already uses fresh hosts or borrows; borrowing would have to stay `kRealtime`).

### 3.5 Song position and tempo

Every block carries the timeline position of its first frame. The app reads it from a thread-local set by the graph or renderer (`set_block_position`, `block_position`). `-1` (previews, auditions) means "keep counting on".

`Host::process_keyed_ch` also checks the tempo map (`BLOCK_TEMPO`, set by `set_block_tempo`) and sends a `Tempo` request when playback crosses into a new `TempoSegment`.

The host fills `ProcessContext` every block ([`plugin.rs#L863-L882`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/plughost/src/plugin.rs#L863-L882)):

```rust
ctx.state = (kPlaying | kTempoValid | kTimeSigValid | kProjectTimeMusicValid | kBarPositionValid) as u32;
ctx.sampleRate = self.rate;
ctx.projectTimeSamples = self.time;
ctx.continousTimeSamples = self.time;
ctx.tempo = t.bpm;
ctx.projectTimeMusic = t.start_music + (self.time - t.start) as f64 / self.rate * t.bpm / 60.0;
let bar = f64::from(t.beats_per_bar) * 4.0 / f64::from(t.beat_unit);
ctx.barPositionMusic = t.start_music + ((ctx.projectTimeMusic - t.start_music) / bar).floor().max(0.0) * bar;
ctx.timeSigNumerator = t.beats_per_bar as i32;
ctx.timeSigDenominator = t.beat_unit as i32;
```

Gaps worth fixing in Audiform 2: `kPlaying` is always set (even for auditions and the zero-length flush); no `kCycleActive`/loop points; no `kSystemTimeValid`; `continousTimeSamples` equals project time, so it jumps on a seek instead of running on.

Before 2026-10-25 every playback start told plug-ins the song started at 0. The position field fixed timing tools and renderers that follow the playhead ([55fdd08](https://github.com/nwh212/Audiform/commit/55fdd08)).

### 3.6 Bus arrangements and surround (up to 7.1.4)

**Host side.** `setup_buses` ([`plugin.rs#L440-L548`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/plughost/src/plugin.rs#L440-L548)):

- Activate the first event input (if any), the main audio input, and every audio output. Extra audio inputs (sidechains) stay **off** until a key arrives.
- Main buses: `main == None` asks `kStereo`; `Some(0)` keeps the plug-in's current arrangement; `Some(mask)` asks that mask. Other buses keep their own arrangement.
- If the plug-in refused a surround mask, try `alternates(n)` (other common layouts with the same speaker count), then the plug-in's own main layout if it has that count. Otherwise it stays as it settled (usually stereo) and the app routes L and R.

```rust
/// Bits: L R C LFE Ls Rs Lc Rc Cs Sl Sr Tc Tfl Tfc Tfr Trl Trc Trr.
fn alternates(n: u32) -> Vec<SpeakerArrangement> {
    match n {
        6  => vec![bits(&[0,1,2,3,4,5]), bits(&[0,1,2,3,9,10])],
        7  => vec![bits(&[0,1,2,3,4,5,8]), bits(&[0,1,2,3,9,10,8])],
        8  => vec![bits(&[0,1,2,3,4,5,9,10]), bits(&[0,1,2,3,4,5,6,7])],   // 7.1 music, 7.1 cinema
        10 => vec![bits(&[0,1,2,3,4,5,12,14,15,17]), bits(&[0,1,2,3,9,10,12,14,15,17])],
        12 => vec![bits(&[0,1,2,3,4,5,9,10,12,14,15,17]), bits(&[0,1,2,3,4,5,6,7,12,14,15,17])],
        _ => Vec::new(),
    }
}
```

The host reports what it got back as speaker names (`speaker_names`), bit order = channel order: `L R C LFE Ls Rs Lc Rc Cs Lss Rss Tc Ltf Ctf Rtf Ltr Ctr Rtr LFE2 M`, numbered for unnamed bits. Mono is reported as `M`.

Channel count per bus is `arr.count_ones()`. Mapping inside `Audio::block`: main input channel for channel; stereo into a mono input is averaged; a mono output fills both sides of a stereo stream; channels the plug-in has no output for keep what came in.

**App side.** Audiform's own channel names (`dsp::channels::labels`): 6 = `L R C LFE Ls Rs`; 8 = `L R C LFE Lrs Rrs Lss Rss`; 12 = `L R C LFE Lrs Rrs Lss Rss Ltf Rtf Ltr Rtr`.

`arrangement_for(names)` ([`instance.rs#L911-L944`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/plugins/instance.rs#L911-L944)) maps those to VST3 bits, `None` for stereo or less or any unknown name:

```rust
"L" => 0, "R" => 1, "C" => 2, "LFE" => 3,
"Ls" | "Lrs" => 4, "Rs" | "Rrs" => 5, "Lc" => 6, "Rc" => 7, "Cs" => 8,
"Lss" => 9, "Rss" => 10, "Tc" => 11, "Ltf" => 12, "Ctf" => 13, "Rtf" => 14,
"Ltr" => 15, "Ctr" => 16, "Rtr" => 17,
```

`ChainProcessor::with_channels` computes this once per chain and builds each slot inside `with_layout(layout, ..)`. `main_of(slot)` picks: `Some(0)` if the slot has "use the plug-in's own channels" (`routing.wide`); `None` (stereo) if the slot has hand-set routing; else the chain's layout.

After loading, `Host` keeps `inputs`/`outputs` from the `loaded` message **only when a layout was asked** (`main.is_some()`); otherwise they stay `["L","R"]`. `PluginProcessor::channels()` returns them, and `routing::resolve` maps plug-in channels to chain channels by name ([`dsp/effects/routing.rs#L30-L58`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/dsp/effects/routing.rs#L30-L58)), with aliases:

```rust
"Ls" if has("Lss") => &["Lrs"],   // with side surrounds too, these are the rears
"Rs" if has("Rss") => &["Rrs"],
"Ls" => &["Lss", "Lrs"],          // a 5.1 surround sits at the side in a 7.1 room
"Rs" => &["Rss", "Rrs"],
"Lss" => &["Ls"], "Rss" => &["Rs"], "Lrs" => &["Ls"], "Rrs" => &["Rs"],
"Lc" => &["Lss", "Lrs"],          // a 7.1 "cinema" plug-in's center fronts stand in for the sides
"Rc" => &["Rss", "Rrs"],
"Ltf" | "Ltr" => &["Ltm"], "Rtf" | "Rtr" => &["Rtm"], "Ltm" => &["Ltf"], "Rtm" => &["Rtf"],
"M" => &["C", "L"],
```

A stage that is not "straight" gets a `Route`: its input channels are gathered from the chain, its outputs replace their targets, and untouched chain channels pass through a `Delay` of the stage's latency.

Log line on every layout request: `plug-in layout: "<name>" asked for 12 speakers, took L R C LFE Ls Rs Lss Rss ... in and ... out`.

Known limits (release notes 1.0.0): Waveform editor playback folds a surround file to stereo before the rack, so plug-ins hear L and R while playing there; Apply runs every channel. In 3D Space sessions a clip with an own-layout plug-in feeds a hidden 3D Bus so the plug-in hears the whole bed (`engine/speaker_feed.rs`). None of this was tried with a real surround plug-in: "Not tried with a real surround plug-in here (the test plug-in is stereo)" (2026-11-06).

### 3.7 The per-block round trip (app side)

`Host::process_keyed_ch` ([`instance.rs#L582-L653`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/plugins/instance.rs#L582-L653)):

1. If blocks are still owed (`late > 0`), drain any late answers. If still behind, bypass this block (return false). If behind for more than `STALL_LIMIT`, kill it.
2. Record sent changes in `known`.
3. Maybe send `Tempo`.
4. Send `KEY` (if any), then `AUDIO`.
5. Wait up to `PROCESS_TIMEOUT` (4 s). On timeout: `late += 1`, bypass. Wrong length: bypass.

This "late" logic came from a real bug: opening a plug-in window during playback held the plug-in's audio for a few seconds and it got killed (section 8).

`PluginProcessor` ([`instance.rs#L1358-L1474`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/plugins/instance.rs#L1358-L1474)) is the chain stage. During playback (`live`) it takes the host lock with `lock_briefly` (30 ms), because another thread may hold it for a request (opening a window). If it cannot, the block is bypassed. A mixdown waits on the lock so nothing is left out.

A chain with plug-ins is not warmed up again on old audio when swapped during playback (`swap_fx_ch` in [`engine/fx.rs`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/engine/fx.rs#L124-L125)): "Plug-ins keep running between chains; they already heard it." Settings-only changes go through `ChainProcessor::retune`, which calls `update` on each stage; for a plug-in that queues only the values the plug-in does not have (`Host::differing`).

Plug-ins on different tracks run side by side: tracks whose chains hold a plug-in and listen to no other track run their chains on separate threads (rayon) before the rest of the mix (decisions 2026-10-24, "loading speed, plug-ins side by side").

### 3.8 Latency: report, change, re-measure, keyed

Three numbers live on `Host`:

- `latency`: `getLatencySamples()` right after activation, sidechain off.
- `keyed_latency`: same, with the sidechain inputs on (measured once at load, section 3.3 step 7).
- `live_latency`: the last value from a `latency_changed` event after load (`NO_REPORT = usize::MAX` until then).

```rust
pub fn latency_for(&self, keyed: bool) -> usize {
    let live = self.live_latency.load(Ordering::SeqCst);
    if live != NO_REPORT { return live; }
    if keyed { self.keyed_latency } else { self.latency }
}
```

`keyed` is `slot.sidechain.is_some()`. A slot that is not running uses the scan's figure (`scanned_latency`).

The mix graph compensates every chain's latency sample-exactly. When a plug-in reports a change, the reader thread stores it and sets `Events.latency_changed`. The interface (`onLatency` in `App.svelte`) restarts playback from the current position so every track lines up again. Waveform editor Apply runs four 4096-frame blocks of silence through a throwaway chain and sleeps 80 ms before reading latency (`settled_latency` in `commands/wave_fx.rs`): "A plug-in tells the host about a changed delay on its own thread."

Instruments: notes are sent early by the instrument's latency, and `Playing::open` primes the instrument by playing `latency` frames into a sink ([`engine/render.rs`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/engine/render.rs), `Playing`).

Why `keyed_latency` exists: VocAlign-style plug-ins report a bigger delay once the key input is on. Since sidechains stay off until a key arrives, the load-time value was the unkeyed one, and the aligned track came out late (section 8).

### 3.9 Sidechain inputs

- Scan sets `sidechain = ins.len() > 1` (more than one audio input bus).
- The UI shows "Listen to" for such plug-ins. `EffectSlot.sidechain` names the source track or bus.
- The key is the source's sound after its chain and before its fader, held back inside the chain by the latency of the stages before this one (`ChainProcessor.keys`, `Delay`). Keys are stereo and only reach stages that run on two channels (or a whole-bed stage).
- The host gets the key as a `KEY` frame. Bus 1 gets it; a mono bus 1 gets the average. Other extra buses get silence.
- `Audio::follow_key` ([`plugin.rs#L747-L774`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/plughost/src/plugin.rs#L747-L774)) turns the extra inputs on when a key starts arriving and off after one second without one. Activating a bus needs the plug-in inactive, so it does `setProcessing(0)`, `setActive(0)`, `activateBus`, `setActive(1)`, `setProcessing(1)`. A tail is lost at that moment.

Why off by default: a compressor whose sidechain input is on listens to it. Fed silence, it never compressed, and imported sessions played about 18 dB too loud (section 8).

### 3.10 Reset

`Audio::reset` ([`plugin.rs#L978-L989`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/plughost/src/plugin.rs#L978-L989)): effects get `setProcessing(0)` then `setProcessing(1)` (pause and resume). Instruments also get `setActive(0)`/`setActive(1)` so held notes stop. Effects used to be switched off and on too; a vocal aligner restarted its capture and its window closed on every playhead jump.

### 3.11 Errors from `process`

If `process` returns an error but wrote something non-zero to the main output, the output is kept. Only an error with nothing written passes the input through untouched. "Some plug-ins report an error yet fill their outputs."

---

## 4. Editors (plug-in windows)

### 4.1 Own window vs generic controls

The plug-in's own window opens **only on Windows**. On Linux and macOS, `editor::open` returns "Plug-in windows open on Windows for now. The plug-in's controls are in its effect window." Linux needs `IRunLoop` and X11 embedding; macOS needs NSView and bundle loading. Neither exists in Audiform 1.

Generic controls: every plug-in also has the app's own effect window with a slider per visible parameter, built from `effect_info` (keys `p<id>`, 0..1). It is reached with the ☰ button or "Show Controls" in the slot's right-click menu.

Rule in the Voice Chain panel ([`src/panels/voice-chain/VoiceChainPanel.svelte`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/voice-chain/VoiceChainPanel.svelte), `openSlot`):

```ts
function openSlot(s: EffectSlotDoc) {
  const info = byId.get(s.effect);
  if (info?.plugin && info.hasWindow && !$stoppedPlugins.has(s.id)) openPluginWindow(s);
  else openWindow(s.id);
}
```

So: has a window and is not stopped = own window; else generic controls.

**The "opened generic first" bug.** Clicking a slot opened the plug-in's window from the start (2026-10-24, [6dc976c](https://github.com/nwh212/Audiform/commit/6dc976c)). But two other paths still showed the generic list first: adding a plug-in from the Effects menu, and Replace With. Fixed 2026-10-26 in [50e6b5e](https://github.com/nwh212/Audiform/commit/50e6b5e) (decisions 2026-10-28: "Plug-ins open in their own window when added from the Effects menu or by Replace With; before, those paths showed the built-in controls list first."). Lesson for Audiform 2: have one "open slot" function and call it from every path.

Opening a window calls `host_for` first, so it starts the plug-in if needed (`plugin_open_window` in `commands/plugins.rs`), at the device rate, taking whatever layout is running. After opening, the interface reloads the plug-in lists because "Opening it may show inputs the scan missed (a sidechain)".

### 4.2 Window structure (Windows)

`editor::win::open` ([`editor.rs#L438-L590`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/plughost/src/editor.rs#L438-L590)):

1. `controller.createView(ViewType::kEditor)`; check `isPlatformTypeSupported(kPlatformTypeHWND)`.
2. Register two classes: `AlteredAudioPluginWindow` (top level, own `proc_`) and `AudiformPluginView` (child, `DefWindowProcW`).
3. Style `WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_CLIPCHILDREN`, plus `WS_THICKFRAME` if `view.canResize()`.
4. Pick DPI mode (section 4.4). Size = `view.getSize()` (minimum 100 x 60) plus the strip height, through `AdjustWindowRectExForDpi`.
5. `CreateWindowExW` **owned by the app's main window** (`owner` HWND from Tauri's `hwnd()`): it stays above the main window, minimizes with it, and clicking the main window never buries it.
6. Child window for the plug-in above the strip; `view.setFrame(Frame)`; `view.attached(child, kPlatformTypeHWND)`.
7. Show and `SetForegroundWindow`.

`Frame::resizeView` records `Edit::Resize(w, h)` and calls `view.onSize`. The main loop resizes the window to fit. `WM_SIZE` refits the child and calls `onSize` if resizable. `WM_DPICHANGED` (moving to another monitor) tells the plug-in the new scale and resizes.

Drop: `view.setFrame(null)`, `view.removed()`, clear the user-data pointer, `DestroyWindow`.

Each window keeps its own `State` (boxed, pointer in `GWLP_USERDATA`). Before that, plug-ins sharing a process shared one "closed" flag, so closing one window closed others.

### 4.3 The bottom strip with a bypass button

A 22-pixel (at 96 DPI) strip under the plug-in, painted with GDI: a power ring, "On" or "Bypassed", and "Audiform" on the right. Clicking the left part (`button_width = strip * 4`) flips bypass and records it (`take_bypass` gives `bypass_picked`). The app commits it as an undoable "Bypass Effect" / "Turn On Effect" step. The other direction: when the rack row's power changes (including undo), the interface calls `plugin_show_bypassed`, which sends `editor_bypassed`.

Space handling: the window's message pump swallows Space (no Ctrl or Alt) unless the focused control's class name contains "edit" or "text", and reports `space_pressed` (with Shift = record). The app then plays or stops, so Space works while a plug-in window has focus.

Close prompts: plug-in windows sit above the main window and covered the unsaved-changes prompt. Now `plugin_hide_windows(true)` hides them (`SW_HIDE`) while prompts show and shows them again (`SW_SHOWNOACTIVATE`).

`pump_messages()` runs the thread's Windows message queue on every main-loop pass **while any plug-in is loaded, window or not**. Many plug-ins finish loading through window messages (timers, background sample loading); instruments stayed silent until their window was opened.

### 4.4 DPI scaling (100/125/150% vs 115%)

History in commits:

1. First version: per-monitor DPI aware, plug-in told the scale via `IPlugViewContentScaleSupport`.
2. 2026-10-28 to 10-29: several resize fixes (re-read size after attach, snap resizes, "Fit to plug-in" button, size logging) made some windows grow on their own. All reverted to the 0.4.0 window code: [9206e12](https://github.com/nwh212/Audiform/commit/9206e12), [7c4abf6](https://github.com/nwh212/Audiform/commit/7c4abf6), [a37fc2c](https://github.com/nwh212/Audiform/commit/a37fc2c), [a2b688a](https://github.com/nwh212/Audiform/commit/a2b688a) (revert), [a135670](https://github.com/nwh212/Audiform/commit/a135670), [b6dbf81](https://github.com/nwh212/Audiform/commit/b6dbf81), [eb496f5](https://github.com/nwh212/Audiform/commit/eb496f5) (revert again).
3. [63fb22f](https://github.com/nwh212/Audiform/commit/63fb22f) (2026-10-29): windows made **not** DPI aware ("as in most DAWs"). Windows stretches them at 125%, so a plug-in that does not scale itself still shows whole. Release 0.5.1: "Plug-in windows open at the right size on screens set to more than 100 percent scaling."
4. [58d1f94](https://github.com/nwh212/Audiform/commit/58d1f94) (2026-11-04): a tester at 115% saw a Waves plug-in drawn small in the bottom-left of the stretched window with black around it. The lead's 125% machine must not change. So: pick per scale.

```rust
/// AUDIFORM_PLUGIN_DPI=aware or =stretch overrides this, for testing.
pub fn scale_aware(dpi: u32, override_: Option<&str>) -> bool {
    match override_.map(str::trim) {
        Some(v) if v.eq_ignore_ascii_case("aware") => return true,
        Some(v) if v.eq_ignore_ascii_case("stretch") => return false,
        _ => {}
    }
    let percent = (dpi * 100 + 48) / 96;
    !percent.is_multiple_of(25)
}
```

At a 25% step: `SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_UNAWARE)` before creating the window, layout at 96 DPI. Otherwise `PER_MONITOR_AWARE_V2`, real pixels, and `setContentScaleFactor(dpi / 96)`. The monitor's real DPI is read while the thread is temporarily DPI aware (`monitor_dpi`), "so Windows does not answer 96 for everyone". Unit test: 96, 120, 144, 168, 192 stretch; 110, 106, 130 aware.

### 4.5 Repaint and cut-off issues (the Ozone question)

The brief asked about Ozone being cut off or not repainting. I could not find Ozone named in any code comment, commit message or plug-in decision. The only Ozone mention is "9 tracks with Ozone-style chains" in a performance entry (2026-10-28, "Heavy sessions start fewer plug-ins"), which is about load, not drawing.

What the history does show about cut-off and repaint:

- Release 0.5.0: "Plug-in windows can always be resized, and they re-read their size once open (fixes windows drawn too small)." ([9206e12](https://github.com/nwh212/Audiform/commit/9206e12))
- [7c4abf6](https://github.com/nwh212/Audiform/commit/7c4abf6): "plug-in windows repaint fully on resize and follow a plug-in that cannot resize" (paint the whole window first so a window bigger than the plug-in shows plain background, not leftovers).
- These and the follow-ups were reverted because windows grew on their own. What stands now: `WM_ERASEBKGND` returns 1 (the plug-in covers its part, the strip paints itself), `fit_child` calls `InvalidateRect` on the top window after moving the child, and resizing is driven by `resizeView` and `WM_SIZE` only.
- The 115% "drawn small with black around it" case is the DPI fix above.

If an Ozone-specific report exists, it is outside the repo. Treat plug-in window sizing as an area to test with real commercial plug-ins (iZotope, Waves, FabFilter, UAD) at 100%, 115%, 125% and 150%, and on two monitors with different scales.

---

## 5. State and presets

### 5.1 Getting state into the session

- `EffectSlot.plugin_state: Option<String>` holds the blob from section 1.4. Controls live in `EffectSlot.params` as `p<id>`.
- The interface polls `plugin_events` (every 200 ms). For a slot with `edits`, `dirty` or `window_closed`, it fetches `plugin_state(slot_id)` (`Host::state`, a `GetState` round trip) and commits one undoable "Plug-in Settings" edit (`syncOps` in `src/app/plugin-sync.ts`). Edits also set `preset: null`.
- `captureAll()` fetches every running plug-in's state before Save, Save As, exports, parking a session and changing "Run together".

### 5.2 Sending state back, without echoes

Undo, presets and imports change `plugin_state` in the session. `instance::push_states(before, after)` sends the new state to running hosts in the background (`commands/session.rs` calls it after edits). `Host::set_state` skips:

- the exact state it last reported or was given (`last_state`);
- any state whose hash matches one the plug-in itself reported in the last `REPORT_ECHO = 3 s`. Otherwise "A plug-in that reports changes often (as with moving the playhead) had its settings saved into the session and sent straight back, sometimes as a stale copy" ([0058388](https://github.com/nwh212/Audiform/commit/0058388)). Some plug-ins rebuild their window on `setState`.

Controls get the same treatment: `known` holds `(value, Option<Instant>)` per param; a value moved in the plug-in's window within `TURNING = 1500 ms` is not sent back from the session, which lags behind ("sending its copy would make the knob jump"; [b68651b](https://github.com/nwh212/Audiform/commit/b68651b)).

### 5.3 Loading with state

`Host::start` passes `state` and `params` in the `load` request. State goes in before activation (section 3.3). Then params are applied on top. So for a slot with both, the `p<id>` values win over the same controls inside the blob.

### 5.4 Presets (`.vstpreset`)

[`plugins/presets.rs`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/plugins/presets.rs). Shared with every VST3 host.

- Folders: user `Documents/VST3 Presets` (Windows), `~/Library/Audio/Presets` (macOS), `~/.vst3/presets` (Linux); shared `%PROGRAMDATA%\VST3 Presets`, `/Library/Audio/Presets`, `/usr/share/vst3/presets`, `/usr/local/share/vst3/presets`. Walked 5 levels deep.
- File: `"VST3"`, `i32` version 1, class id as 32 ASCII hex, `i64` offset of the chunk list, chunks, then `"List"`, `i32` count, each `(4-byte id, i64 offset, i64 size)`. `"Comp"` = component state, `"Cont"` = controller state.
- Matching accepts both class id byte orders (`class_ids`).
- `read(path)` returns our blob format; the slot's `plugin_state` is set and the running plug-in takes it live through `push_states` (no restart; undo works the same way).
- `save` writes to `<user folder>/<vendor>/<plugin>/<name>.vstpreset`, from the running plug-in's state or the slot's saved one. `delete` only inside the user folder.

### 5.5 `.sesx` import and export

[`src-tauri/src/sesx/mod.rs`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/sesx/mod.rs) and `export.rs`.

- Component id in the file: `"3:"` plus four groups of eight hex digits. `vst3_effect` builds both byte layouts and picks whichever a scanned plug-in uses, else this platform's (`com` layout on Windows, plain elsewhere).
- State in the file (`presetChunkData`): base64 of `u64` component length, `u64` controller length, then both parts. `chunk_to_state` converts to our `u32` form; `state_to_chunk` does the reverse for export.
- Plug-ins not installed still come in with their state and are named in the import notice.
- Some of the other editor's built-in effects and one vocal strip plug-in are replaced by Audiform built-ins with mapped settings (Parametric EQ, Phase Curve, Limiter, Vox Strip).

### 5.6 Rack preset import

`read_rack_presets` turns any XML element holding `<slot>` children into a chain preset, named from the nearest `name` or `presetName` attribute above it. It uses the same `read_rack` as `.sesx` import, so VST3 plug-ins keep their state, the replacements above apply, `powered="false"` becomes a bypassed slot, and the notice lists "Not found on this computer (install or scan them in the Plug-in Manager): ...". Chain presets now keep `plugin_state` and `enabled` (decisions 2026-10-31).

---

## 6. MIDI for instruments

### 6.1 Notes

`NoteEvent { offset, on, pitch, velocity (0..1), channel (0..15), pan (-1..1) }` rides in the AUDIO frame. The host turns each into a VST3 `Event` on bus 0 ([`plugin.rs#L605-L663`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/plughost/src/plugin.rs#L605-L663)):

- `kNoteOnEvent` / `kNoteOffEvent`, `tuning 0`, `length 0`, `noteId = channel * 128 + pitch`.
- A note-on with non-zero pan is followed by a `kNoteExpressionValueEvent` with `kPanTypeID`, value `pan * 0.5 + 0.5`.
- Events are split across `MAX_BLOCK` chunks by offset. Events go only to plug-ins with an event input (`inputEvents` is null otherwise).

Renderer rules ([`engine/render.rs`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/engine/render.rs), `Playing`): notes are sorted by `(offset, on)`, so at one frame a note ends before the next starts. Starting mid-note "chases" it (plays it at offset 0). Jumps and loops drop the `Playing` and start fresh, which resets the instrument (all notes off).

`Instrument::start` resets the host, sends tempo, and for a **fresh** copy (exports) runs `INSTRUMENT_WARMUP = 2.5 s` of silence in 1024-frame blocks, sleeping 15 ms between, so sample-based instruments finish loading. Then it resets again.

Auditions (`audition`, `sequence`) send note-offs for anything still sounding when dropped, so a cut-short preview never hangs a note.

### 6.2 Controllers via IMidiMapping

VST3 instruments do not take MIDI CC events. The host must map controllers to parameters with `IMidiMapping`. At load ([`plugin.rs#L668-L680`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/plughost/src/plugin.rs#L668-L680)):

```rust
unsafe fn read_midi_map(c: &ComPtr<IEditController>) -> Vec<(u8, u32)> {
    let Some(m) = c.cast::<IMidiMapping>() else { return Vec::new(); };
    let mut out = Vec::new();
    for cc in 0..130i16 {               // 0..127 CCs, 128 aftertouch, 129 pitch bend
        let mut id = 0;
        if m.getMidiControllerAssignment(0, 0, cc, &mut id) == kResultOk {
            out.push((cc as u8, id));
        }
    }
    out
}
```

Only event bus 0 and MIDI channel 1 (index 0) are asked. Controllers on other MIDI channels use the channel-1 map. That is a simplification; a full host would ask per channel.

`Host::control_for(controller)` looks it up. `Instrument::control(controller, value)` queues `(param id, value)` into `pending`, which goes out with the next block as a parameter change at **sample offset 0**. Controller values: CC `value / 127`; aftertouch `value / 127` as 128; pitch bend 14-bit `/ 16383` as 129 (`engine/midi_live.rs`).

Playback (`Playing::play`) sends every controller move due in the block at its start. When playback starts mid-clip, the last value of each controller before the start is sent first (a pedal already down).

### 6.3 Sustain pedal (CC 64)

Decision 2026-11-06 ([ba6529b](https://github.com/nwh212/Audiform/commit/ba6529b)): send the pedal as controller 64 when the instrument maps it. For one that does not map 64, Audiform holds the notes itself, the way a piano does, and a recording then has the longer notes (`pass_pedal` in `midi_live.rs`). CC 120 and 123 (all sound off, all notes off) are handled locally as "release all".

`MidiClip.controls` stores moves as (beats, controller 0..129, value 0..1, channel). The piano roll's bottom lane edits them.

Note: the test synth does not implement `IMidiMapping`, so the app test "the pedal holds it" covers the local-hold path, not CC 64 forwarding.

---

## 7. Automation of plug-in parameters

Plug-in controls are ordinary effect settings (`p<id>`), so they use the same automation lanes as built-in effects.

- Clip chains: `Fx::run_at` ([`engine/render.rs#L36-L55`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/engine/render.rs#L36-L55)) splits the block into 256-frame steps and calls `ChainProcessor::set_param(slot, key, value)` before each step.
- `ChainProcessor::set_param` updates the stage's `Params` and calls `update`. `PluginProcessor::update` always returns `true`; it queues values the plug-in does not already have (`differing`), and they go out with the next block at offset 0.
- So plug-in automation is stepped at 256 frames and sent as one point per parameter per block, at sample offset 0. No ramps, no sample-accurate points.
- Track and bus chain automation goes through the mix graph; I did not trace whether its step is also 256 frames (look in `engine/graph.rs`).

Automation writing (release 0.8.0): Write, Latch and Touch record any effect setting, plug-ins included, as it moves. For plug-ins, moves arrive as `param_edited` events. `IComponentHandler::beginEdit`/`endEdit` are ignored by the host (they return `kResultOk` and do nothing), so Touch cannot know when the user grabbed or let go of a plug-in knob from the plug-in's own window. Audiform 2 should forward begin and end edits.

`outputParameterChanges` is always null, so the app never reads values the plug-in itself writes during processing (meters, gain reduction).

---

## 8. Bugs we hit and how we fixed them

Each item: symptom, cause, fix, link.

1. **Scans hung for a minute per bundle and found nothing.** The scanner waited for the scan process to exit before reading its output. Plug-ins with many controls print more than a pipe holds (about 4 KB on Windows), so both sides waited. Fix: read pipes on threads while the scan runs; test with a 300 KB answer. [b421914](https://github.com/nwh212/Audiform/commit/b421914)
2. **"It crashed while loading, so it was skipped" for most commercial plug-ins.** The scan opened each bundle twice; closing the second copy called `ExitDll` under the first. The SDK reference counts these calls, JUCE-based plug-ins do not. Fix: open once, share `Arc<Module>`, close nothing before answering, never unload the library. [2854776](https://github.com/nwh212/Audiform/commit/2854776)
3. **Bulk scan froze.** Helper programs (licence services) inherited the scan's pipes and kept them open after it exited; also plug-ins that hang while unloading. Fix: after exit wait at most 2 s for output; a scan that printed `"done"` is killed and counted done; Stop Scan. [8a886a2](https://github.com/nwh212/Audiform/commit/8a886a2)
4. **Two copies of one plug-in broke MIDI tracks and the Editor lists.** Same class id in two folders. Fix: `dedupe` by class id. [4dccff6](https://github.com/nwh212/Audiform/commit/4dccff6)
5. **Reverb on a MIDI track seemed silent.** Could not reproduce, but found: the host passed dry audio through when `process` returned an error even if the plug-in had written output. Fix: keep the output if anything non-zero was written. [d9d4262](https://github.com/nwh212/Audiform/commit/d9d4262)
6. **No sound from instruments on first Play.** Play waited 5 s for readiness; real instruments take longer to load (licence checks, samples). Fix: start plug-ins in the background as soon as they are in the session (`warm`), one start per slot, Play waits up to 30 s. [19ffa34](https://github.com/nwh212/Audiform/commit/19ffa34)
7. **Instruments silent until their window was opened.** The host only pumped Windows messages while a window was open; many plug-ins finish loading through messages. Fix: `pump_messages()` every loop pass while plug-ins are loaded. [0a502a4](https://github.com/nwh212/Audiform/commit/0a502a4)
8. **Instruments silent at the start of exports.** A fresh copy was still loading samples. Fix: 2.5 s warm-up on silence at playback pace, then reset. [1ffdfd8](https://github.com/nwh212/Audiform/commit/1ffdfd8)
9. **Exports slow and different from playback; a request check started every plug-in.** Fix: exports borrow idle playback hosts; the request check no longer runs a full render. [0a502a4](https://github.com/nwh212/Audiform/commit/0a502a4)
10. **Song position always 0 at play start.** Fix: position in every AUDIO block. [55fdd08](https://github.com/nwh212/Audiform/commit/55fdd08)
11. **Plug-in window hidden behind the main window.** Fix: owned window (`owner` HWND). [dd3cf44](https://github.com/nwh212/Audiform/commit/dd3cf44)
12. **Plug-in windows covered the unsaved-changes prompt; closing one window closed others in the same process.** Fix: `HideEditor`; per-window `State`. Shipped with the Audiform strip in 0.4.0 ([9204329](https://github.com/nwh212/Audiform/commit/9204329)).
13. **Opening generic controls instead of the plug-in window** when adding from the Effects menu or Replace With. [50e6b5e](https://github.com/nwh212/Audiform/commit/50e6b5e)
14. **Windows firewall prompt** because plug-ins listen on the network inside the host process (licence checks, same-maker discovery). Fix: installer adds an inbound block rule for Audiform; Effects > Stop Network Prompts From Plug-ins checks and adds it from inside the app (one admin prompt, offered once). Loopback is not filtered, so same-process and same-machine discovery still works. [50e6b5e](https://github.com/nwh212/Audiform/commit/50e6b5e), [5377c34](https://github.com/nwh212/Audiform/commit/5377c34)
15. **Plug-in windows grew on their own / drawn too small / not repainted.** A series of resize fixes, all reverted to the 0.4.0 code. See section 4.4 and 4.5. Lesson: do not poll or second-guess plug-in sizes; trust `getSize`, `resizeView` and `onSize`.
16. **Cut off at 125%** (plug-ins that do not scale themselves). Fix: not DPI aware, Windows stretches. [63fb22f](https://github.com/nwh212/Audiform/commit/63fb22f)
17. **Drawn small with black around it at 115%.** Fix: DPI aware plus content scale only at in-between scales. [58d1f94](https://github.com/nwh212/Audiform/commit/58d1f94)
18. **Settings sent back stale.** Plug-ins that report often had their own state echoed back. Fix: `REPORT_ECHO` hash window. [0058388](https://github.com/nwh212/Audiform/commit/0058388)
19. **Knobs jittered when turned quickly in the plug-in window.** The session's lagging copy was sent back. Fix: `TURNING` window in `differing`. [b68651b](https://github.com/nwh212/Audiform/commit/b68651b)
20. **Space did nothing in plug-in windows.** Fix: catch Space in the host's pump and send `space_pressed`. [b68651b](https://github.com/nwh212/Audiform/commit/b68651b)
21. **Vocal aligner restarted its capture and its window closed on every playhead jump.** Reset switched effects off and on. Fix: pause and resume (`setProcessing` only) for effects; instruments still `setActive` off and on. [7cf3672](https://github.com/nwh212/Audiform/commit/7cf3672)
22. **Plug-in killed when its window opened during playback.** Opening a window held audio a few seconds; after 4 s it counted as hung. Fix: bypass late blocks and catch up; kill only after 20 s behind; `lock_briefly` 30 ms in playback. [73a307e](https://github.com/nwh212/Audiform/commit/73a307e)
23. **Cold Play took 17 to 25 s.** Plug-ins of sessions set aside were stopped on every edit, and slots loaded one after another. Fix: keep plug-ins of every open session; start them side by side at Play; log every stop with a reason. [c8707e0](https://github.com/nwh212/Audiform/commit/c8707e0)
24. **Plug-ins restarted every few seconds ("ran at 48000 Hz, wanted at 44100 Hz"), each restart a new licence check.** First blamed on the device default rate (partly right: starts ahead of Play now use the stream's rate). Real cause: pictures at the playhead rendered the mix at the session's rate through playback's pool. Fix: `borrow_only`. [3fe0b9b](https://github.com/nwh212/Audiform/commit/3fe0b9b)
25. **Host memory 2.4 GB.** A process per plug-in loads each maker's runtime again. Fix: one process per maker by default. [190c25e](https://github.com/nwh212/Audiform/commit/190c25e)
26. **Imported sessions played 18 dB too loud.** All inputs were activated and the sidechain fed silence, so compressors never compressed. Fix: only the main input active; extra inputs follow the key (`follow_key`). [a66f5de](https://github.com/nwh212/Audiform/commit/a66f5de)
27. **VocAlign keyed track came out late.** Latency read with the key input off. Fix: measure `keyed_latency` at load. [e3e5c9b](https://github.com/nwh212/Audiform/commit/e3e5c9b)
28. **Plug-ins that change latency after loading played late.** The `LatencyChanged` value was thrown away. Fix: keep `live_latency`; restart playback to re-align; settle before reading in Waveform Apply. Ported from Altered Audio Editor d6611fb. [0576641](https://github.com/nwh212/Audiform/commit/0576641)
29. **A 7.1 plug-in on a 7.1 file passed only L and R.** Plug-ins ran stereo unless "own channels" was on. Fix: ask surround chains' layout ([c3bea30](https://github.com/nwh212/Audiform/commit/c3bea30)); then try alternates (7.1 music vs cinema, etc.) before stereo, and log what was taken ([bc11c6e](https://github.com/nwh212/Audiform/commit/bc11c6e)). Not verified with a real surround plug-in.
30. **Surround plug-in heard one channel at a time on a surround clip in 3D Space.** Each channel played on its own hidden mono track. Fix: hidden 3D Bus per track or clip (`engine/speaker_feed.rs`). See decisions 2026-11-05.

---

## 9. Recommended design for Audiform 2 (pure Rust)

### 9.1 Keep nearly as is

The `altered-plughost` crate is small (about 3,800 lines of source, plus about 500 lines of tests and a 1,000-line test plug-in), self-contained, and battle-tested against real plug-ins. Its only dependencies are `vst3 = "0.3"` (lockfile: 0.3.0), `libloading = "0.8"`, `serde`, `serde_json`, and `windows-sys = "0.59"` on Windows. It uses `slice::as_chunks` and `u32::is_multiple_of`, so it needs a recent stable Rust.

Keep (copy with its GPL headers):

- **Out-of-process hosting by re-executing the main binary** with `--plugin-host` / `--plugin-scan`. One signed binary, no sidecar to ship.
- **One process per (scope, maker, rate)** with an opt-out per maker.
- **The framing and message set** (section 1.4). It already supports several plug-ins per process, keys, notes, tempo, layouts.
- **The scan line protocol** with stage lines and `parse_scan_output`, `crash_reason`.
- **`Plugin` / `Audio` split** (main thread vs audio thread) and drop order.
- **`setup_buses` with `alternates`**, `speaker_names`, main-only activation, `follow_key`, keyed latency measurement.
- **`reset` semantics** (pause/resume effects, deactivate instruments).
- **Zero-length `process` to flush parameter changes.**
- **Keep the output when `process` errors but wrote audio.**
- **Never unload plug-in libraries; `forget` them.** Open each bundle once per process.
- **`pump_messages` while any plug-in is loaded** (Windows).
- **DPI rule `scale_aware`** and the owned top-level window with a child for the plug-in.
- **State blob format** (`u32` component length + component + controller, base64), effect id `vst3:<hex>`, control keys `p<id>`. This keeps Audiform 1 sessions, presets and `.sesx` conversions working.
- **The test plug-in** (gain, 32-sample latency, crash switch, crash-on-start env var, sidechain env var, empty view env var, test synth). Keep its MIT/Apache credit.

App-side logic worth porting (rewrite into your own structure, but keep the behavior): the late-block catch-up, `lock_briefly` for live playback, `differing` with `TURNING`, `REPORT_ECHO`, `latency_for`, one start per slot, background warm-up with a worker pool, borrow idle playback hosts for exports, `borrow_only` for analysis renders, `dedicated` for monitoring, the 2.5 s instrument warm-up for fresh export copies, dry bypass through a latency `Delay` when a plug-in stops.

### 9.2 Change

Protocol and robustness:

- **Add a version handshake** (`Hello { version }`) as the first frame each way. Audiform 1 relied on `serde(default)`.
- **Add a block sequence number** to AUDIO in both directions. The app now infers matching from `late` counts.
- **Add request ids** to CONTROL replies. Today replies are matched by order on a per-plug-in channel. It works because requests on one `Host` are serialized by a `Mutex`, but ids make it obvious and allow concurrent requests.
- **Do not block the stdin thread on `Load`** in a shared process. Load on the main thread and let the audio thread keep serving other plug-ins (see 1.5).
- **Per-block allocations in the host.** `Audio::process` and `block` build `Vec`s and new `ComWrapper`s (`ParamChanges`, `EventList`) for every chunk. Fine at 2048-frame blocks; worth pre-allocating if Audiform 2 wants small blocks for monitoring.
- **Consider shared memory for audio** only if you need low-latency monitoring through plug-ins. Pipes were enough for 2048-frame playback and 128-frame live MIDI. If you do it, keep pipes for control and use a shared ring per plug-in with an event or futex for wake-ups. This adds `unsafe` on the app side, which Audiform 1 avoided on purpose.
- **Replace thread-local context flags** (`FRESH`, `OWN`, `SCOPE`, `BORROW_ONLY`, `LAYOUT`, `BLOCK_POS`, `BLOCK_TEMPO`) with an explicit context passed into chain building and processing.
- **Replace global statics** (`pool()`, `failures()`, `starting()`, `shared_conns()`, `separate_makers()`, the registry) with one owned `PluginManager` struct.

VST3 completeness:

- **Sample-accurate parameter changes**: send `(id, offset, value)` and build real `IParamValueQueue` points. Today every point is at offset 0.
- **Forward `beginEdit` / `endEdit`** for Touch automation.
- **Read `outputParameterChanges`** for meters if the UI wants them.
- **Optionally use `kOffline`** for unpaced mixdowns, behind a switch, in fresh hosts only.
- **ProcessContext**: set `kPlaying` only when playing, keep `continousTimeSamples` monotonic, add cycle and system time.
- **`getTailSamples`** so a bypassed or removed plug-in can ring out, and exports can extend past the last clip.
- **IMidiMapping per MIDI channel** (Audiform 1 only asks channel index 0).
- **Check `canProcessSampleSize(kSample32)`** and handle a refusal. Audiform 1 assumes 32-bit.
- **`IPlugInterfaceSupport`, `IContextMenu`, `IProgress`** on the host object, as newer plug-ins ask for them. Audiform 1's `HostApp` only implements `IHostApplication`.
- **Editors on Linux and macOS.** Linux: implement `Steinberg::Linux::IRunLoop` on the `IPlugFrame` and embed in an X11 window (for example with `x11rb`). macOS: load bundles through CFBundle, call `bundleEntry`/`bundleExit`, embed into an `NSView` (for example with `objc2`). Audiform 1 has neither.
- **`has_editor` from the scan** should mean "createView returned a view", not "has a controller". You can check this in the scan process by creating and releasing a view (some plug-ins misbehave without a frame; test carefully).

UX:

- One "open slot" function used by every path (section 4.1).
- Fetch the plug-in catalog once and share it; with 900 plug-ins it is tens of MB.

### 9.3 Crates

- `vst3` (coupler-rs/vst3-rs), MIT or Apache-2.0, bindings generated from the SDK headers. Audiform 1 uses 0.3.0 and its `ComPtr`, `ComWrapper`, `Class` traits throughout. Keeping it makes the host crate copy-paste. Check for a newer release and read its changelog before upgrading; the code casts several C enum types that differ between Windows and other systems (`#![allow(clippy::unnecessary_cast)]` in `lib.rs`).
- `libloading` for the module. `windows-sys` for windows and COM init.
- For writing your own test plug-ins, the `vst3` crate's examples are the base Audiform 1 used.
- I did not evaluate other VST3 binding crates. If you consider one, check its licence: the VST3 SDK moved to MIT in 2025 (ATTRIBUTION.md), and older Rust bindings may predate that.

### 9.4 Ordered implementation checklist, with tests

1. **Host crate skeleton.** Copy `protocol.rs` (framing, messages, base64, scan parse) and its unit tests. Add the version handshake. Test: round trips, tagged JSON, scan output survives noise and crashes.
2. **Module loading.** `module.rs`: library path per platform, entry/exit calls, factory, classes with vendor fallback, never unload. Test: open the test bundle, list classes.
3. **Test plug-ins.** Port `testplug` (gain + crash + sidechain + view + synth). Build it from tests into a temp bundle (see `bundle()` in `tests/plugins.rs` and `test_bundle()` in `plughost/tests/host.rs`, which serializes concurrent builds).
4. **Scan mode.** `--plugin-scan`: Found, then Read per class, stage lines, `forget` and `exit(0)`. Test: scan gives effect + instrument; missing bundle is an error; `ALTERED_TESTPLUG_CRASH_ON_START` gives a named-but-noted class with "crashed while ..." text.
5. **App scanner and registry.** Threads draining pipes, 4 at a time, 60 s timeout, 2 s pipe grace, stop flag, stamps, retries, dedupe, schema version, atomic save. Tests: fake host scripts (big output, helper holding pipes, hang after done, stop part way) all return in seconds; duplicate ids listed once.
6. **Load and process (stereo).** `Plugin::load_from`, `Audio::process`, splitting over `MAX_BLOCK`, zero-length flush. Test: impulse comes out 32 samples late at the set gain; a change applies in the same block; a block over 8192 is continuous; params at load are in state at once.
7. **State.** get/set state, blob format compatible with Audiform 1. Test: state from one host restores the same sound in a new host.
8. **Crash isolation.** App `Conn` with reader thread, dead flag, stopped events, timeouts, dry bypass delayed by latency. Test: the crash switch (`p1 = 1.0`) leaves the mix playing dry and in time.
9. **Pool, fresh, borrow, dedicated, borrow-only** (as an explicit context). Tests: two slots from one maker share one process (`shared_processes_started` style counter); unused hosts stop; an export uses an idle playback host; analysis renders start nothing.
10. **Latency compensation in the graph.** Test: click lands on the same sample with and without the plug-in, in playback and export.
11. **Instruments.** Notes in AUDIO, event conversion, chase, priming by latency, reset on jump, warm-up for fresh copies, auditions that always release. Tests from `plays_notes_on_an_instrument_at_their_frames` and the MIDI part of `plug_ins_scan_mix_keep_settings_and_survive_crashes` (note starts and stops on the exact sample, plays from the middle, follows tempo).
12. **Tempo and song position.** Position per block, tempo segments. Test: a plug-in that reports `projectTimeMusic` (extend the test plug-in) sees the right beat after a seek and after a tempo change.
13. **MIDI mapping.** Read `IMidiMapping`, controller to param at block start, last-value chase at start, CC 64 pass or local hold. Add `IMidiMapping` to the test synth so CC 64 forwarding is tested (Audiform 1 never tested it).
14. **Sidechain.** KEY frame, follow-key activation, keyed latency. Tests from `a_plugin_with_a_sidechain_input_hears_the_key`: no key means the plug-in hears its own input; a key reaches bus 1; a key counts for one block only. Add: keyed latency differs (make the test plug-in report a different latency when bus 1 is active).
15. **Surround.** `arrangement_for`, `setup_buses` alternates, speaker names, name-based routing with aliases. Unit tests exist for both (`layout_tests`, `routing.rs` tests). Add a test plug-in that only accepts 7.1 cinema to test the alternates path, which Audiform 1 never tested.
16. **Late-block catch-up and lock_briefly.** Test: a plug-in that sleeps 6 s in one `process` is bypassed, then plays again, and is not killed.
17. **Editors.** Windows first: owned window, child, strip with bypass, Space, hide, DPI rule (unit test `scale_aware`). Then Linux (IRunLoop) and macOS. Test by hand with real plug-ins at 100/115/125/150% and across two monitors.
18. **Events to the session.** Param edits, dirty, window closed, bypass picked, latency changed, stopped; echo and turning guards. Unit-test the guards with a fake clock.
19. **Presets and imports.** `.vstpreset` read/write/list (round-trip and byte-order tests exist in `presets.rs`), `.sesx` chunk conversion, rack preset import.
20. **Real-time paced mixdown option.** Test: a 1 s paced mixdown takes about 1 s (`a_realtime_mixdown_takes_as_long_as_the_audio` in `export/mixdown/tests.rs`).
21. **Optional:** a test against a real open-source plug-in (Audiform 1 used Dragonfly Plate Reverb via `REVERB_DIR`, test `a_real_reverb_rings_on_a_midi_track`, ignored by default).

---

## 10. File map

All paths are relative to the repo root. Full links: `https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/<path>`.

| File | What it holds |
| --- | --- |
| `src-tauri/plughost/Cargo.toml` | Host crate manifest and why it is separate. |
| `src-tauri/plughost/src/lib.rs` | `main_with_args`, `serve` (stdin audio thread + main loop), `scan`, `stage`, COM init. |
| `src-tauri/plughost/src/protocol.rs` | Frames, `Request`, `Message`, `ClassInfo`, `ParamInfo`, `NoteEvent`, AUDIO encode/decode, scan lines, `parse_scan_output`, `crash_reason`, base64. |
| `src-tauri/plughost/src/plugin.rs` | `Plugin` (main thread) and `Audio` (audio thread): load, state, buses, layouts, alternates, speaker names, notes, MIDI map, process, flush, follow_key, reset. |
| `src-tauri/plughost/src/module.rs` | Bundle library path, `Module` open/drop, factory classes, cid hex helpers. |
| `src-tauri/plughost/src/com.rs` | Host-side COM objects: `HostApp`, `MemStream`, `ParamChanges`/`ParamQueue`, `EventList`, `Handler` (edits, restartComponent), `Frame` (resizeView), `Message`, `Attributes`. |
| `src-tauri/plughost/src/editor.rs` | `EditorWindow` trait, `pump_messages`, `scale_aware`, Windows window with the bypass strip. |
| `src-tauri/plughost/src/bin/altered-plughost.rs` | Stand-alone host for the crate's tests. |
| `src-tauri/plughost/tests/host.rs` | End-to-end host tests: scan, process, state, crash, notes, several plug-ins per process, sidechain. |
| `src-tauri/plughost/testplug/` | Test plug-in bundle source (gain, crash switch, sidechain, view, synth). |
| `src-tauri/src/main.rs` | Re-exec entry into the host crate. |
| `src-tauri/src/plugins/mod.rs` | Scanner, `Registry` / `plugins.json`, folders, dedupe, `effect_info`, `catalog`, `instruments`, makers. |
| `src-tauri/src/plugins/instance.rs` | `Conn`, `Host`, pool, `fresh`/`dedicated`/`borrow_only`, `with_layout`, `arrangement_for`, `main_of`, `fits`, `host_for`, `warm`, `start_all`, events, `PluginProcessor`, `Instrument`, auditions. |
| `src-tauri/src/plugins/presets.rs` | `.vstpreset` list, read, save, delete. |
| `src-tauri/src/commands/plugins.rs` | Tauri commands: registry, scan, folders, makers, open/close window, bypass, hide, events, state, restart, MIDI audition, presets. |
| `src-tauri/src/commands/session.rs` | Warm-up after edits, `push_states`, `retain`, the 5 s resting check. |
| `src-tauri/src/commands/wave_fx.rs` | Waveform Apply, `settled_latency`. |
| `src-tauri/src/commands/peaks.rs` | `mix_before` inside `borrow_only`. |
| `src-tauri/src/dsp/effects/mod.rs` | `build` (plug-in slots become `PluginProcessor`), `ChainProcessor` (layout, routing, keys, `retune`, `set_param`), `chain_latency`. |
| `src-tauri/src/dsp/effects/routing.rs` | Name-based channel routing and speaker aliases. |
| `src-tauri/src/dsp/channels.rs` | Audiform's channel names per count (`labels`). |
| `src-tauri/src/engine/fx.rs` | Running chain wrapper; no re-warm for chains with plug-ins. |
| `src-tauri/src/engine/render.rs` | Clip rendering, automation steps, MIDI `Playing` (notes, chase, controls, priming). |
| `src-tauri/src/engine/graph.rs` | Mix graph; sets block position and tempo for plug-ins. |
| `src-tauri/src/engine/transport.rs` | Playback block size and look-ahead; `start_all` at Play. |
| `src-tauri/src/engine/midi_live.rs` | Live MIDI input, sustain handling, controllers to params, dedicated instrument hosts. |
| `src-tauri/src/engine/record.rs` | Monitoring chains with dedicated plug-in copies. |
| `src-tauri/src/engine/speaker_feed.rs` | Hidden 3D Buses so own-layout plug-ins hear whole surround clips. |
| `src-tauri/src/export/mixdown.rs` | Mixdown inside `fresh`; real-time pacing. |
| `src-tauri/src/sesx/mod.rs`, `export.rs` | `.sesx` plug-in ids, state chunk conversion, rack preset import. |
| `src-tauri/tests/plugins.rs` | App end-to-end: scan, mix, shared process, state, crash, missing, MIDI, live MIDI, audition. |
| `src/app/plugin-sync.ts` | Polls events, brings edits/state/bypass into the session, `captureAll`. |
| `src/app/App.svelte` | `PluginSync` wiring: stopped notice, Space, latency restart, power sync, real-time mixdown setting. |
| `src/app/PluginManagerDialog.svelte` | Plug-in Manager: folders, scan, results, Run together. |
| `src/panels/voice-chain/VoiceChainPanel.svelte` | `openSlot`, `openPluginWindow`, Show Controls, Restart, rack preset import notice. |
| `src/stores/plugins.ts` | `pluginsVersion`, `stoppedPlugins`. |
| `docs/decisions.md` | Entries from 2026-10-24 "VST3 plug-ins" to 2026-11-06 "MIDI controllers". |
| `ATTRIBUTION.md` | `vst3` crate and SDK licence, test plug-in credit. |

---

## 11. Things I could not determine

- **Ozone repaint or cut-off.** No code, commit or decision names Ozone in that context. See 4.5.
- **Pipe round-trip time.** Never measured in the repo.
- **Whether `kOffline` would help any plug-in.** Never tried.
- **Track and bus automation step size** for plug-ins in `engine/graph.rs` (clip chains use 256 frames). Not traced.
- **Real surround plug-in behavior.** The alternates and layout code were not run against a real surround plug-in, per the decisions log.
- **Windows editor code at first** was only compile-checked for Windows ("it has not run on Windows yet", 2026-10-24); later entries show it in use by testers and the lead.
