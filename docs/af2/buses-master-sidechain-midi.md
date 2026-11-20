# Audiform 2 handoff: buses, master, sends, outputs, sidechain and MIDI

This guide carries over what Audiform 1 learned while building its mix graph and its MIDI support. It is written for the engineer building Audiform 2 in pure Rust. It describes what Audiform 1 does, with real names and values from its code, and ends with a recommended design and an ordered checklist.

Source: the Audiform 1 repository, `nwh212/Audiform` at commit `042577e1ddee7b9718d82aea7947f9aab6edd628` (tag 1.0.0, the last version on this codebase). All links below point at that commit.

Base URL for links: `https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/`

**Licence.** Audiform 1 is GPL-3.0-or-later. Most files carry `Copyright (c) The GeekCast Radio Network, LLC.`, some also or only `Copyright (c) The Audiform contributors.` Any code copied or closely ported into Audiform 2 keeps the GPL-3.0-or-later licence and every copyright line of the file it came from. Code written fresh from the behavior described here does not carry those lines, but if you lift a function body, keep the header.

**Covered elsewhere.** VST3 hosting (the out-of-process host, the pipe protocol, scanning, plug-in windows), 3D Space, and ASIO each have their own guide. This guide mentions them only where they touch the mix graph or MIDI.

**How sure this is.** Everything here was read from the code and `docs/decisions.md`. Where the code and the docs disagree, or where something looks like a bug that was never reported, it is marked "From reading the code" or "Unclear".

---

## Contents

1. Data model
2. The mix graph
3. The master track
4. Sidechaining
5. Solo and mute
6. Automation
7. MIDI
8. Bugs we hit and how we fixed them
9. Recommended design for Audiform 2
10. File map

---

## 1. Data model

All of this lives in [`src-tauri/src/session/model.rs`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/session/model.rs). The session is versioned JSON (`format: "altaudio.session"`, `schema_version: 1`), saved as `.audfrm` (older `.altaudio` files have the same contents). JSON Schemas are generated with `schemars` into `/schemas`.

### 1.1 Units, the one rule to remember

- Audio clip positions, lengths, fades, markers and **every automation point** are in **samples at the session rate** (`u64`).
- Audio clip source offsets are in samples at the **source file's** rate.
- **MIDI clips, notes and controller moves are in beats** (`f64`, quarter notes), with no PPQ. A beat is always a quarter note, even in 6/8 (a 6/8 bar is 3 beats).
- Tempo changes are placed at **bar numbers** (`u32`, from the session start, 0 is the first bar).
- Levels are dB (`f64`). Pan is -100 (left) to 100 (right) on tracks, buses and sends. Note pan is -1 to 1.

The model comment at the top says it plainly ([model.rs#L5-L15](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/session/model.rs#L5-L15)).

### 1.2 Session

```rust
pub struct Session {
    pub format: String, pub schema_version: u32, pub id: Id, pub name: String,
    pub sample_rate: u32, pub bit_depth: u16, /* created, modified */
    pub media: Vec<MediaRef>,
    pub tracks: Vec<Track>,   // audio and MIDI tracks, top to bottom
    pub buses: Vec<Bus>,      // bus tracks, in their own list
    pub master: MasterTrack,
    #[serde(default, skip_serializing_if = "Tempo::is_default")]
    pub tempo: Tempo,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub space: Option<Space>, // 3D Space sessions only
    /* markers, playlist, view, modes, libraries, template_id, metadata */
}
```

`Id` is a `Uuid` (v4). Everything that can be referenced (tracks, buses, clips, sends, effect slots, lanes, tempo changes) has one. Routing refers to ids, not indexes. The engine turns ids into indexes (`routing::Node::Track(usize)` / `Node::Bus(usize)`) each time it builds.

### 1.3 Track

[model.rs#L533](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/session/model.rs#L533). The fields that matter here:

| Field | Type | Default | Notes |
| --- | --- | --- | --- |
| `kind` | `TrackKind` (`audio`, `midi`) | `audio` | Left out of the file for audio tracks. |
| `channels` | `ChannelLayout` (`mono`, `stereo`) | per call | MIDI tracks are always stereo (`Track::midi`). |
| `volume_db` | f64 | 0.0 | Clamped to -96..24 when turned into gain. |
| `pan` | f64 | 0.0 | -100..100, a balance law (see 2.6). |
| `muted`, `solo` | bool | false | |
| `polarity_inverted` | bool | false | Multiplies the track by -1. |
| `mono` | bool | false | Folds L and R after the chain, before fader and pan. |
| `automation_mode` | `AutomationMode` | `Read` | Read, Write, Latch, Touch. Left out when Read. |
| `record_armed`, `monitor_input`, `monitor_dry` | bool | false | Arming is reused for MIDI tracks. |
| `input` | `Option<String>` | None | Audio: hardware input name. MIDI: None means every MIDI input, a port name means one, `"none"` means none. |
| `output` | `Output` | `Master` | Where the track goes. |
| `sends` | `Vec<Send>` | empty | Aux sends to buses. |
| `effects` | `EffectChain` | default | The track's Voice Chain (later called Audio FX). |
| `clips` | `Vec<Clip>` | empty | Audio clips. |
| `instrument` | `Option<EffectSlot>` | None | MIDI tracks: the VST3 instrument. |
| `midi_clips` | `Vec<MidiClip>` | empty | MIDI tracks. Left out when empty. |
| `envelopes` | `Vec<Envelope>` | empty | Automation lanes. |
| `speaker` | `Option<usize>` | None | `#[serde(skip)]`, playback only (3D Space speaker feeds). |

A MIDI track is a full track: chain, fader, pan, mute, solo, mono, output, sends, and it can be a sidechain key source. The op layer refuses audio clips on MIDI tracks and MIDI clips or instruments on audio tracks (`OpError::WrongTrackKind`: "Audio clips go on audio tracks, MIDI clips on MIDI tracks.").

### 1.4 Bus

[model.rs#L674](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/session/model.rs#L674). A bus has no clips and no inputs of its own. Tracks and other buses output or send to it.

```rust
pub struct Bus {
    pub id: Id, pub name: String, pub channels: ChannelLayout,
    pub color: Option<String>, pub height: u32,        // height 72 by default
    pub volume_db: f64, pub pan: f64, pub muted: bool, pub solo: bool,
    pub output: Output,
    pub sends: Vec<Send>,                              // to other buses
    pub effects: EffectChain,
    pub envelopes: Vec<Envelope>,
    pub automation_mode: AutomationMode,               // skipped when Read
    pub position: Option<u32>,  // display only: shown below this many tracks
    pub mono: bool,
    pub space: Option<SpacePos>, pub room: bool,       // 3D Space only
}
```

`position` is display only: buses live in their own list, and `position` says how many tracks sit above the bus row in the Editor, so moving a track never drags a bus along. Keep that idea; it avoids a mixed list of tracks and buses in the model.

### 1.5 Output and Send

```rust
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Output { #[default] Master, Bus { bus_id: Id } }

pub struct Send {
    pub id: Id, pub bus_id: Id,
    pub level_db: f64,   // clamped -96..12 by the op
    pub pan: f64,        // the send's own pan, -100..100
    pub pre_fader: bool,
    pub enabled: bool,
}
```

[model.rs#L482-L501](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/session/model.rs#L482-L501). New sends from the interface start post-fader at 0 dB, centered, on (`newSend` in `src/panels/mixer/routing.ts`). The model has no hardware outputs: a track goes to the master or a bus, nothing else.

### 1.6 MasterTrack

```rust
pub struct MasterTrack {
    pub channels: ChannelLayout, pub height: u32,
    pub volume_db: f64,
    pub effects: EffectChain,
    pub envelopes: Vec<Envelope>,
}
```

No pan, no mute, no solo, no sends. The master's Mono button is a listening check on the audio thread and is **not saved** (see 3). `master.envelopes` is filled by `.sesx` import, but **the engine never plays master envelopes** (nothing reads them outside import and ops). Decide in Audiform 2 whether the master gets automation; Audiform 1 never finished it.

### 1.7 Effect slots and the sidechain key field

```rust
pub struct EffectSlot {
    pub id: Id,
    pub effect: String,          // "altaudio.dynamics", or "vst3:<class id>" for plug-ins
    pub enabled: bool,
    pub params: BTreeMap<String, f64>,  // plug-in params as "p<id>", normalized 0..1
    pub preset: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sidechain: Option<Id>,   // track or bus id whose sound drives the detector
    pub plugin_state: Option<String>,   // base64 VST3 state
    pub data: Option<String>,
    pub position: Option<u32>,
    pub routing: Option<SlotRouting>,
}
```

[model.rs#L1352-L1395](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/session/model.rs#L1352-L1395). The key is **one id per slot**. The key is the source's sound after its chain and before its fader. Only effects with a detector use it, and only on track and bus chains (and the master chain). Clip chains never take a key. A key may keep pointing at a removed track: the effect then listens to its own input, and undoing the removal brings the ducking back.

An instrument is also an `EffectSlot` (on `Track::instrument`), which is why instrument settings, plug-in state and parameter automation all work the same way as effects.

### 1.8 MIDI clips, notes and controller moves

[model.rs#L366-L436](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/session/model.rs#L366-L436):

```rust
pub struct Note {
    pub pitch: u8,      // 0..127, 60 is middle C (shown as C4)
    pub velocity: u8,   // 1..127
    pub start: f64,     // beats from the clip's start
    pub length: f64,    // beats
    #[serde(default, skip_serializing_if = "is_zero_u8")]
    pub channel: u8,    // 0..15, shown as 1..16 and as the note's color
    #[serde(default, skip_serializing_if = "is_zero_f32")]
    pub pan: f32,       // -1..1, per-note pan expression; 0 sends none
}

pub struct MidiClip {
    pub id: Id, pub name: String,
    pub start: f64,     // timeline position in beats
    pub length: f64,    // beats; notes past the end are not played
    pub muted: bool, pub color: Option<String>,
    pub gain_db: f64,   // scales velocity, clamped -96..24
    pub notes: Vec<Note>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub controls: Vec<ControlEvent>,
}

pub struct ControlEvent {
    pub at: f64,        // beats from the clip's start
    pub controller: u8, // 0..127 as in MIDI, 128 channel aftertouch, 129 pitch bend
    pub value: f32,     // 0..1 (pitch bend: 0.5 is the middle)
    pub channel: u8,    // 0..15
}
```

The op that writes clips (`SetMidiClips`, the whole list for a track) tidies each clip ([ops/midi.rs#L42-L72](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/session/ops/midi.rs#L42-L72)): pitch to 0..127, velocity to 1..127, start at least 0, length at least 1/64 beat for notes and 1/16 beat for clips, channel to 0..15, pan to -1..1 (NaN to 0), notes sorted by start then pitch. It does not tidy `controls`; the playback plan clamps them instead (`controller.min(129)`, value to 0..1).

Note that there is no separate "pattern" or "region pool": a MIDI clip owns its notes. Duplicating a clip copies its notes with new ids for the clip.

### 1.9 Tempo map

[model.rs#L127-L344](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/session/model.rs#L127-L344):

```rust
pub struct Tempo {
    pub bpm: f64,               // quarter notes per minute, default 120
    pub beats_per_bar: u32,     // top of the time signature, default 4
    #[serde(default = "quarter", skip_serializing_if = "is_quarter")]
    pub beat_unit: u32,         // bottom of the time signature, default 4
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changes: Vec<TempoChange>,
}
pub struct TempoChange { pub id: Id, pub bar: u32 /* >= 1 */, pub bpm: f64,
                         pub beats_per_bar: u32, pub beat_unit: u32 }
pub const BPM_MIN: f64 = 20.0;
pub const BPM_MAX: f64 = 400.0;
```

Design choices worth keeping:

- **Changes sit on bar lines only.** That keeps the map trivial to lay out: each stretch starts where the previous one's bars end, so a change before it never leaves a later one mid-bar.
- **No tempo ramps.** Each segment has one tempo.
- `Tempo::tidy()` makes the map valid: one change per bar (the later one wins), sorted, none at bar 0, bpm clamped to 20..400 (non-finite becomes 120), `beats_per_bar` 1..32, `beat_unit` rounded to a power of two 1..32 (`tidy_beat_unit`), bar capped at 100,000. `SetTempo` replaces the whole map as one undoable step and runs `tidy()`.
- `segments()` yields `TempoSegment { bar, beat, seconds, bpm, beats_per_bar, beat_unit }`. A bar holds `beats_per_bar * 4 / beat_unit` quarter notes (`bar_beats`); one beat of the signature is `4 / beat_unit` quarters (`unit_beats`, 0.5 in 6/8).
- Conversions: `seconds(beats)`, `samples(beats, rate)` (rounded), `beats(samples, rate)`. These walk the segments linearly each call. Fine for a few changes; Audiform 2 could cache a segment table.

The interface has a matching TypeScript copy in [`src/stores/tempo.ts`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/stores/tempo.ts) (`segments`, `beatsToSeconds`, `secondsToBeats`, `barStart`, `barAt`, `nearestBar`, `addChange`, `moveChange`, `removeChange`). Two copies of the same math drifted risk; in a pure Rust app, keep one.

### 1.10 Automation lanes

```rust
pub enum EnvelopeTarget { Volume, Pan, EffectParam { slot_id: Id, param: String }, Space { param: SpaceParam } }
pub enum EnvelopeCurve { Linear, Hold, Spline }
pub struct EnvelopePoint { pub position: u64 /* session samples */, pub value: f64, pub curve: EnvelopeCurve }
pub struct Envelope { pub id: Id, pub target: EnvelopeTarget, pub enabled: bool,
                      pub points: Vec<EnvelopePoint>, pub height: Option<u32> }
```

Volume lanes hold dB, pan lanes -100..100, effect lanes the effect's own parameter units. Track and bus lanes are absolute session positions; clip lanes are relative to the clip's left edge. **The engine ignores `curve`**: track and bus lanes play straight lines between points (`engine/automation.rs` doc comment: "Values hold flat before the first point and after the last, and move in straight lines between points").

Automation points are in samples while MIDI is in beats. Changing the tempo moves MIDI but not automation. That is a known asymmetry; see 9 for a suggestion.

### 1.11 Serialization and migration notes

- `Document` ([session/document.rs](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/session/document.rs)) reads `schema_version` and runs `migrate_step` per version before deserializing. Session is still at version 1: every MIDI, bus and tempo field was added as **optional with a default and `skip_serializing_if` at the default**, so old files open unchanged and new files stay small. Buses, outputs and sends were in the format as scaffolding from the first phase, so adding the engine for them needed no version bump.
- Loading never refuses a file for bad routing. [`session/validate.rs`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/session/validate.rs) reports issues (outputs and sends to missing buses, bus output loops) and the engine still plays a damaged file (see 2.3).
- If Audiform 2 reads Audiform 1 sessions, map: `Track.kind`, `instrument`, `midi_clips`, `MidiClip.controls`, `Note.channel`, `Note.pan`, `Tempo.beat_unit`, `Tempo.changes`, `EffectSlot.sidechain`, `Track/Bus.automation_mode`, `Bus.position`, `mono`. All are optional in the JSON.

---

## 2. The mix graph

Main file: [`src-tauri/src/engine/graph.rs`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/engine/graph.rs). Levels: [`engine/graph/levels.rs`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/engine/graph/levels.rs). Order and loops: [`session/routing.rs`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/session/routing.rs).

### 2.1 Threads and where the graph runs

Audiform 1 has three threads in playback ([transport.rs#L5-L23](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/engine/transport.rs#L5-L23)):

1. The caller (a Tauri command) starts, stops and syncs playback.
2. A **render thread** ("transport") renders clips and runs the mix graph in blocks of `BLOCK = 2048` frames, staying `AHEAD_SECONDS = 0.5` ahead of the device, and pushes into lock-free `rtrb` rings.
3. The **device audio thread** (cpal callback) pops the rings, applies gains from atomics, applies the master fader, adds the metronome, writes the output, records meters. It never allocates, locks or does I/O.

The render thread **does** allocate, lock (`try_lock` on mailboxes) and read files. That was a deliberate trade: it runs half a second ahead, so a slow block is absorbed by the ring. Every effect, bus, send and sidechain runs there. Mixdown runs the very same `Renderer` + `Graph` on a worker thread, so export sounds exactly like playback.

### 2.2 Node types

There are only two node types in routing: `Node::Track(i)` and `Node::Bus(i)`. The master is not a node; it is the sink. In the graph each node and the master is a `Strip`:

```rust
struct Strip {
    id: Id, mono: bool,
    fx: Option<Fx>,         // the chain; None when it does nothing
    in_delay: Delay,        // tracks: holds the clips back to this strip's input time
    input: Vec<f32>,        // summed input (buses) or clip audio (tracks)
    pre: Vec<f32>,          // after the chain, before the fader: pre-fader sends and keys read this
    keys: Vec<Tap>,         // sidechain taps
    routes: Vec<Route>,     // the output first, then each send in session order
    peak: [f32; 2],
    auto: Option<StripAuto>,
    /* 3D Space fields: diffuse, room, bed_in, bed_delay, steer_in */
}
enum Dest { Bus(usize), Master, Direct(usize) }
struct Route { dest: Dest, delay: Delay }
struct Tap { src: Node, id: Id, delay: Delay, buf: Vec<f32> }
```

Buffers are interleaved stereo `f32` everywhere in the graph (3D Space beds are interleaved with one channel per speaker).

### 2.3 Topological order and cycle prevention

[routing.rs#L48-L184](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/session/routing.rs#L48-L184).

Edges are of two kinds, and **both count for order and loops**:

- `routes(s)`: each output to a bus, and **each send, on or off**. Counting disabled sends means turning a send on can never create a loop, so the toggle needs no check and no rebuild.
- `keys(s)`: each sidechain key, as (source, the node whose chain listens). A track keyed by itself is a loop.

`order(s)` returns the nodes and the edges it kept:

```rust
pub fn order(s: &Session) -> (Vec<Node>, Vec<(Node, Node)>) {
    // tracks first, then buses, in session order
    let mut kept = Vec::new();
    for e in edges(s) {               // routes, then keys
        if e.0 != e.1 && !reaches(&kept, e.1, e.0) {
            kept.push(e);             // an edge that would close a loop is dropped
        }
    }
    // Kahn's algorithm, keeping the session's own order among equals
    ...
}
```

Three layers of loop safety:

1. **Edits refuse loops.** [`ops::apply`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/session/ops/mod.rs#L278-L286) applies the op, and if the op can change routing (`changes_routing`: output changes, `SetSends`, `SetEffects`, `RestoreTrack`) and `has_loop(s)` is true, it applies the inverse and returns `OpError::RoutingLoop` ("That routing would feed the sound back into itself."). The session is unchanged. Unknown key sources fail with `NoKeySource`, unknown send buses with `NoBus`.
2. **The interface hides choices that would loop** (output pickers and "Listen to" lists leave them out).
3. **A damaged file still plays.** `order` drops the edge that closes the loop; the graph sends that route to the master instead (`main_dest` falls back to `Dest::Master` when the edge is not in `kept`), and a send whose edge was dropped keeps its slot (so level indexes line up) but plays nowhere. Test: `a_damaged_loop_still_plays` in [graph/tests.rs](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/engine/graph/tests.rs).

`reaches` and `has_loop` are O(E^2) with `Vec::contains`. Fine for tens of tracks; for hundreds, use a proper DFS with a visited bitset.

### 2.4 Two shapes: plain and routed

This is the most important design decision in Audiform 1's mixer, decided 2026-10-20 ([decisions.md#L894](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/docs/decisions.md#L894)) and built 2026-10-24 ([decisions.md#L1018](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/docs/decisions.md#L1018)).

`routing::is_routed(s)` ([routing.rs#L168-L184](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/session/routing.rs#L168-L184)) is true when any of:

- the session is a 3D Space session;
- any track or bus has a playing automation lane (enabled, with at least one point);
- the master chain is active;
- any track outputs to a bus or has an **enabled** send;
- any track chain has a sidechain key.

A bus nobody feeds changes nothing.

- **Plain** (`routed == false`): the graph runs only the track chains and hands each track out on its own ring (`Dest::Direct(i)`), before its fader. The audio thread applies fader, pan, mute and solo from atomics, so they react within one device buffer.
- **Routed** (`routed == true`): the graph mixes everything (track faders, sends, buses, master chain) and hands out one stereo ring: the mix before the master fader. Track and bus faders then lag by the render-ahead time (about 0.5 s). The master fader stays on the audio thread and stays instant. The Mixer shows "Faders lag ½ s" under the master while this is so.

Why: the audio thread can never allocate or lock, and effects can (several allocate, plug-ins talk over pipes). A chain that hears faded audio must run after the faders, so the faders must run where the chain runs. The other option considered (bus and master chains before their own faders only) made faders feel dead on the strip they control. See section 9 for how Audiform 2 might avoid the lag altogether.

### 2.5 Latency alignment (plug-in delay compensation)

The graph aligns every path so the whole mix is delayed by one fixed amount, `lead`, and the renderer runs that far ahead ([graph.rs#L413-L560](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/engine/graph.rs#L413-L560)). The algorithm, in order of `order`:

```rust
// in_t[n]: when node n's input happens; out_t[n]: when its output happens,
// both counted in frames from the clips leaving the renderer.
for &n in &order {
    let mut t = 0;
    for (src, dst) in routing::routes(s) {          // every feeder kept in `kept`
        if dst == n && kept.contains(&(src, dst)) { t = t.max(out_t[src]); }
    }
    if routed { for (src, _) in keys_of(st, Some(n)) { t = t.max(out_t[src]); } }
    in_t[n] = t;
    out_t[n] = t + chain_latency(n);
    if main_dest(n) is Master { master_in = master_in.max(out_t[n]); }
}
// keys on the master chain also push master_in
let lead = master_in + master_chain_latency (+ STEER_FRAME in 3D Space);
```

Then each delay line is sized:

- a track's `in_delay = in_t[n]` (a track whose input waits for a key must be held back);
- each route's delay = `input_time(dest) - out_t[n]`, where `input_time(Bus b) = in_t[b]` and `input_time(Master) = master_in`;
- each key tap's delay = `in_t[listener] - out_t[source]`.

Inside a chain, a key is further delayed by the latency of the stages before the keyed stage (`ChainProcessor::with_channels` pushes `Delay::new(before)` per keyed slot), so key and signal meet at the detector in step. Clip chains handle their own latency in the renderer: they read their input early by the latency and are primed.

`Mix::new` primes the graph by rendering `lead` frames into a sink first, so the first block handed out lines up with the timeline exactly. Tests that pin this down: `track_chains_keep_time_with_look_ahead` and `buses_sum_their_inputs_in_step_with_direct_tracks` (a direct track with a 1 ms limiter, a track through a bus with a 5 ms limiter, and a pre-fader send all land on frame 5,500).

When a chain's latency changes during playback (a plug-in reports a new delay, or a chain edit changes look-ahead), the chain cannot be swapped in place and playback restarts from the same spot (see 2.9).

### 2.6 Levels: faders, pans, sends

`levels(s)` ([levels.rs#L222-L306](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/engine/graph/levels.rs#L222-L306)) turns the session into `Levels { tracks: Vec<NodeLevel>, buses: Vec<NodeLevel> }`, a pure function of the session. Each `NodeLevel` has `main: [f32; 2]` (left and right gain for the output route), `sends: Vec<[f32; 2]>`, `mono`, and a 3D `place`.

```rust
fn db(v: f64) -> f32 { 10f64.powf(v.clamp(-96.0, 24.0) / 20.0) as f32 }

/// Pan as a balance: centered plays both sides at full level, and
/// panning turns the far side down, so a centered voice plays at
/// exactly the level the fader shows.
pub fn balance(pan: f64) -> [f32; 2] {
    let pan = (pan.clamp(-100.0, 100.0) / 100.0) as f32;
    let left = if pan > 0.0 { 1.0 - pan } else { 1.0 };
    let right = if pan < 0.0 { 1.0 + pan } else { 1.0 };
    [left, right]
}
```

Send rules (exact):

- `on = if heard { sign } else { 0.0 }` (sign is -1 when the track's polarity is inverted).
- main = `on * fader * balance(pan)`.
- each send: `g = if enabled { on * db(level_db) } else { 0 }`; if **post-fader**, `g *= fader`; then `g * balance(send.pan)`.
- **A post-fader send follows the fader but not the track's pan** (it has its own). **A pre-fader send ignores both.** **Every send follows mute and solo.**
- A volume lane that plays sets the fader to 0 dB in `levels` (the lane takes the fader's place); a pan lane sets pan to 0 (center). Post-fader sends then follow the volume lane (`StripAuto::apply` applies volume to route 0 and every post-fader send, pan only to route 0).

The audio taken for every route and every key is `pre`: the chain output, before the fader, after the Mono fold.

### 2.7 Running a block

`Graph::process` ([graph.rs#L685-L955](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/engine/graph.rs#L685)):

1. Clear bus inputs and the master input.
2. `run_tracks_ahead`: tracks with a chain and **no key** run their chains in parallel (see 2.8).
3. Walk `order`. For each node: if it did not run ahead, prepare its input (tracks: copy clips, `in_delay`, mono fold; buses: mono fold of the summed input), `fill_keys` (copy each source's `pre`, run the tap delay), run the chain into `pre` (`run_strip_at` moves automated effect params every 256 frames).
4. At the first bus in the order, `run_buses_ahead` runs every bus whose feeders are all done, in parallel.
5. For each route: copy `pre` to scratch, ramp from the last block's level to the new one (`ramp`, linear across the block, so faders never click), apply automation, meter route 0, run the route delay, add into the destination's input (or the master input, or the direct output).
6. Master: `fill_keys`, run the master chain, copy `pre` to `out[0]`.
7. `applied = levels.clone()` so the next block ramps from here.

### 2.8 Parallelism

Audiform 1 uses rayon (`par_iter_mut`) in three places:

- Renderer: every track's clips and its MIDI instrument render in parallel ([render.rs#L395-L440](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/engine/render.rs#L395-L440)).
- `run_tracks_ahead`: track chains with no key, when there are at least two ([graph.rs#L1089-L1126](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/engine/graph.rs#L1089-L1126)). A keyed track waits for the serial order, since its key is another track's output.
- `run_buses_ahead`: buses that have a chain, no key, are not 3D Buses, and are not fed by a bus still pending ([graph.rs#L1128-L1172](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/engine/graph.rs#L1128-L1172)). This is a single "wave", not a full level-by-level schedule.

Plug-ins need the song position and tempo as thread locals, so `run_strips_parallel` copies `block_position()` and `block_tempo()` into each worker before running. Strips are moved out with `std::mem::take` so each worker owns one.

Plug-ins each live in a host process and the wait is a pipe round trip, so running them in parallel was the big win for heavy sessions (commit [a04adeb](https://github.com/nwh212/Audiform/commit/a04adeb), "Dense sessions: tracks and buses on every core").

### 2.9 Rebuild versus live edit

[`engine/plan.rs`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/engine/plan.rs) splits every session change into three kinds. `Transport::sync(session)` ([transport.rs#L609-L663](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/engine/transport.rs#L609-L663)) decides per edit:

1. **Levels** (faders, pans, mutes, solos, send levels, send on/off, clip gains): never rebuild. Plain sessions store gains into atomics; routed sessions put a whole new `Levels` into a mailbox (`Mutex<Option<Levels>>`) that the render thread takes with `try_lock` and passes to `Graph::set_levels`, which ramps over the next block. `set_levels` ignores a `Levels` whose shape (track count, send counts) differs.
2. **Chains** (`plan::chains`: a fingerprint per chain from `chain_hash`): swapped in without a restart while each chain keeps its latency. `chain_update` compares old and new latency with `chain_latency`; equal latency goes to the render thread via `send_chains` (pending updates merge, last one wins per id). `swap_fx_ch` first tries `ChainProcessor::retune` (same effects in the same slots: turn the running effects' dials, keep their state); otherwise it builds a new chain and warms it up on `WARM_SECONDS = 0.1` of the old chain's input history plus its latency (plug-in chains skip the warm-up, since a plug-in keeps running and already heard that audio).
3. **Layout** (`plan::layout`: `Layout { tracks, routing, midi }`): anything that moves audio in time or changes who feeds whom. A change stops and restarts playback from the current position.

`routing_hash` ([plan.rs#L762-L822](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/engine/plan.rs#L762-L822)) covers: whether routed, the 3D layout and listen mode, **every track and bus envelope (serialized JSON)**, every output, every send's id, bus and pre/post (not its level or on/off), every key source, and for routed sessions each bus's id, mono, output, sends and keys, and the master's keys. Putting envelopes in the hash is what made automation edits audible during playback, and also what made automation writing restart playback 20 times a second (see 6.3).

`midi_hash` ([plan.rs#L832-L871](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/engine/plan.rs#L832-L871)) covers the tempo map, each MIDI track's id, each instrument's slot id and effect id (not its settings: those reach the running instrument directly), and each clip's id, start, length, muted, and each note's pitch, velocity, start and length.

**From reading the code:** `midi_hash` leaves out `Note.channel`, `Note.pan`, `MidiClip.gain_db` and `MidiClip.controls`. Editing a controller lane, a note's channel or pan, or a MIDI clip's level while playing therefore changes nothing in the running plan until the next restart or play. No report of this was found. Audiform 2 should fingerprint everything the plan reads, or better, diff the plan itself.

### 2.10 Real-time rules as Audiform 1 applied them

- Audio thread: atomics only (`AtomicU32` holding `f32` bits for gains and the master; `fetch_max` on bits for peaks, since positive floats order like their bits). Reads rings with `pop`. No allocation, no locks, no I/O. A panic on the audio thread is caught (`host::take_fault`) and playback stops rather than continuing over garbage.
- Render thread: `try_lock` only on mailboxes, so the UI thread can never block it. It does allocate (`Vec` per block in `run_tracks_ahead`, `keys` slices, `Arc<Place>` per block for 3D automation), opens files, and waits on plug-in pipes. All of that is absorbed by the half-second ring.
- Streams close on silence: before any output stream closes, it writes silence for a few device buffers (`host::Hush`, `host::QuietStream`), because ASIO drivers replay whatever a closed stream last left in their buffers (commit [e33da0b](https://github.com/nwh212/Audiform/commit/e33da0b)).

### 2.11 Meters

Routed meters come from the render thread, half a second early. `Shared::chain_meters` queues `(rendered frame count, GraphMeters)` and `played_meters` releases them only once the device has played that far, so meters line up with what is heard. A status read with no newly played block returns the last reading again, otherwise meters flickered to silence between reads ([decisions.md#L1377](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/docs/decisions.md#L1377), commit [2c5d02e](https://github.com/nwh212/Audiform/commit/2c5d02e)).

---

## 3. The master track

- **Master chain** runs last in the graph, on the render thread, with its own sidechain keys (any track or bus can key it). An active master chain makes the session routed.
- **Master fader** is applied on the device audio thread from an atomic (`Shared::master`, `plan::master_gain`: `10^(volume_db.clamp(-96, 24)/20)`). It is instant in both shapes. From reading [transport.rs#L1285-L1312](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/engine/transport.rs#L1285-L1312): the master gain is **not ramped**, it steps once per device buffer. Track and bus levels on the render thread are ramped. Audiform 2 should ramp the master too (a one-pole smoother per sample is enough).
- **Order on the audio thread:** sum (plain: each track times its gain; routed: the single mix), times master, optional audition gain, **mono check** (L and R averaged; listening only, never saved, never in exports), master peak meter, scope capture (Phase Wheel), then the **metronome click is added** after all of that, so the click is never metered, never faded and never in an export ([transport.rs#L1312-L1333](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/engine/transport.rs#L1312-L1333)).
- **Mixdown** applies the master gain after the graph, the same as playback (`export/mixdown.rs`, `master = plan::master_gain(&s)` then multiply). Clip-only mixes (Merge Clips) zero the master volume and chain.
- **Why track faders lag and the master does not:** see 2.4. Lead's reasoning (2026-10-20): the master fader is the "panic" control, it must react at once; and it sits after every chain so it can live on the audio thread at no cost.
- **3D Space (briefly):** in a 3D session the master chain runs on the speaker bed (one channel per speaker, `Fx::with_channels(..., &bed_names)`), and the stereo listening mix is folded from the bed (`bed_fold`), so what you hear matches the export. The master fader multiplies every speaker channel. A 3D Bus (`Bus.room`) runs its chain on its own bed and feeds the master's bed. See the 3D Space guide.

---

## 4. Sidechaining

### 4.1 Which effects take a key

`KEYED` in [dsp/effects/mod.rs#L53-L59](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/dsp/effects/mod.rs#L53-L59):

```rust
pub const KEYED: [&str; 4] = [
    "altaudio.dynamics",       // Compressor
    "altaudio.breath_gate",    // Gate
    "altaudio.expander",       // Expander / Gate
    "altaudio.dynamics_curve", // Dynamics Curve
];
```

The `Processor` trait has `fn process_keyed(&mut self, buf: &mut [f32], key: &[f32])` with a default that ignores the key and calls `process`. Keyed effects implement it by running their detector on the key instead of the input. The Compressor's loop is the clearest example ([dynamics.rs#L201-L216](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/dsp/effects/dynamics.rs#L201-L216)):

```rust
fn run(&mut self, buf: &mut [f32], key: Option<&[f32]>) {
    for (i, f) in buf.as_chunks_mut::<2>().0.iter_mut().enumerate() {
        let k = key.map_or([f[0], f[1]], |k| [k[i * 2], k[i * 2 + 1]]);
        if self.linked {
            let g = self.follow(0, k[0].abs().max(k[1].abs()));
            f[0] *= g; f[1] *= g;
        } else { /* per side */ }
    }
}
```

The Limiter also implements `process_keyed`, but only internally (a limiter on a speaker bed listens to the loudest channel of the whole bed so all speakers turn down together). It is not in `KEYED`, so users cannot route a key to it.

### 4.2 How a key is routed

1. The user picks "Listen to" in the effect window, or a "Sidechain: <effect> on <track>" entry in a track's send list (the same setting seen from the source's side), or a row in the Inputs and Outputs header view. All write `EffectSlot.sidechain = Some(source_id)`.
2. `routing::keys` adds an edge (source, listener). `has_loop` refuses loops, including a strip keyed by itself.
3. `Graph::new` builds a `Tap` per distinct key source of each strip's chain (`keys_of`), with a delay that lines the source's `out_t` up with the listener's `in_t`.
4. Each block, `fill_keys` copies the source strip's `pre` (after chain, before fader) and runs the tap delay. Because the order puts the source first, its `pre` for this block is ready.
5. `ChainProcessor::process_keyed(buf, &[(Id, &[f32])])` finds the key for each keyed stage by id, delays it by the latency of the earlier stages, and calls `process_keyed`. A stage whose source has no key here (missing track, key not routed in this context) listens to its own input.

Key properties to keep:

- **Pre-fader key.** Turning the voice down or muting it does not change the ducking. Test `sidechain_ducks_music_under_the_voice` mutes the voice and still expects the duck, and puts a 5 ms limiter on the voice to prove the alignment.
- **Keys are stereo** and only reach stages that run on two channels.
- **Clip chains take no key.**
- **Removing a bus unkeys effects that listened to it**, as part of the same undo step (`remove_bus` in [ops/buses.rs#L183-L262](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/session/ops/buses.rs#L183-L262)). Removing a track leaves the key in place.
- **The live MIDI voice drops keyed effects** from its copy of the chain (it has nothing to listen to).

### 4.3 Plug-in sidechain inputs

[`plugins/instance.rs`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/plugins/instance.rs). A VST3 plug-in with a second audio input can take a key ([decisions.md#L1886](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/docs/decisions.md#L1886)). The key goes to the plug-in host as its own frame (`KEY`) just before the audio block it belongs to (`process_keyed_ch`), stereo, pre-fader, delayed like built-in keys, used for that block only.

Three lessons, each a real bug (see 8):

- **Keep the sidechain input off until a key is routed.** Turning on every input and feeding silence made compressors hear silence and apply only makeup gain (a session played about 18 dB loud). The host now activates only the main input at load, turns the sidechain bus on when a key arrives (switching the plug-in off and on, as VST3 requires) and off again after a second without one.
- **Measure the keyed latency.** Some plug-ins (a vocal aligner) report a larger delay with the sidechain on. At load the host turns the input on briefly, reads `keyed_latency`, turns it off. `Host::latency_for(keyed)` uses it when the slot has a `sidechain`.
- **Follow latency reported after load.** `live_latency` (written on LatencyChanged) wins over both once reported.

```rust
pub fn latency_for(&self, keyed: bool) -> usize {
    let live = self.live_latency.load(Ordering::SeqCst);
    if live != NO_REPORT { return live; }
    if keyed { self.keyed_latency } else { self.latency }
}
```

---

## 5. Solo and mute

### 5.1 Engine rule

`audible(s)` in [levels.rs#L145-L205](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/engine/graph/levels.rs#L145-L205):

- With nothing soloed, everything not muted is heard.
- With any solo, start from every soloed track and bus and walk the routes (outputs and sends) **both forward and backward**. Everything reached is heard (unless muted). So soloing a bus plays everything feeding it; soloing a track plays it through its bus and its reverb send.
- Mute always wins.
- Sidechain keys are **not** routes, so solo does not walk them. Keys are pre-fader and are computed even for silent strips (the chain runs, then its level is 0), so a soloed ducked track still ducks under a muted or unsoloed voice.

A muted track still runs its chain (so its key and its plug-ins' state continue). Mute is a gain of 0 on every route.

### 5.2 Interface rules on top

- **Bus solo writes feeders' solo flags.** `busOp` in [`src/panels/mixer/routing.ts`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/mixer/routing.ts#L132-L157) soloing or unsoloing a bus does the same to every track and bus whose **output** reaches it (`feeders` leaves sends out), in one undoable batch. The engine would already play them; writing the flags makes the buttons show it.
- **Exclusive solo** (Settings > Multitrack, `soloExclusive`, default off): soloing a track or bus unsolos every other track and bus in the same step (`trackOp` in [`src/panels/mixer/solo.ts`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/mixer/solo.ts)).
- **Ctrl+click (Cmd+click) on a track's S** is "Solo Track Only": exclusive solo for that click whatever the setting (`soloOnly` in `TrackHeaders.svelte` and `MixerPanel.svelte`). In the Mixer, Ctrl+click is only wired for tracks, not buses.
- **Ctrl+Shift+click** on M, S, R or Monitor sets it on all tracks (approved shortcut in CLAUDE.md).
- **Greyed clips:** `silentTracks` draws clips of tracks not heard (muted, or not soloed and not feeding a soloed bus through output or send).

### 5.3 Edge cases

- A send to a soloed bus counts: the sender is heard through that send. Its own output to the master is heard too (the forward walk marks the sender heard as a node, and every route of a heard node plays). Unclear whether that is intended; the decision log says only "soloing a track plays it through its bus and its send buses".
- Pre-fader sends follow mute and solo even though they ignore the fader.
- Stems (`MixdownRequest.track`) keep only that track and clear every mute and solo, so a stem plays even when muted. Buses stay, so a stem goes through its bus and the master chain. **From reading the code:** a stem or a track bounce removes the other tracks, so a ducked track whose key source is another track bounces with its compressor listening to its own input, not ducked. No report found; worth handling in Audiform 2 (keep key sources rendering but silent).
- Recording: a track being recorded has its own audio clips left out of playback (`commands/live.rs`, `heard()`), on a copy of the session; MIDI takes do not silence a MIDI track's existing clips (overdub).

---

## 6. Automation

### 6.1 Playing lanes

[`engine/automation.rs`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/engine/automation.rs). `StripAuto::new(envs, sends, session_rate, out_rate)` builds lanes from envelopes that are enabled and have points. Positions convert from session samples to output frames.

- **Volume lane takes the fader's place.** `levels()` uses 0 dB for the fader when `automates_volume` (so the fader is "set aside"), and `StripAuto::fill` computes a per-frame gain from the lane in dB; at or below `SILENT_DB = -59.9` it is silence. Applied to route 0 and every post-fader send.
- **Pan lane takes the pan's place**, applied to route 0 only (sends have their own pan).
- **Effect lanes** call `chain.set_param(slot, param, value)` every 256 frames (`run_strip_at`, `STEP = 256`). Only gliding controls are offered for lanes (not switches, lists, or controls that change latency).
- **3D Space lanes** rebuild the `Place` once per block at mid-block.
- Any playing lane makes the session routed (2.4).
- Track and bus lanes only. Master envelopes are not played (1.6).

The "envelope takes the fader's place" choice means: while a volume lane plays, moving the fader does nothing audible unless the track is in a writing mode (where 6.3 takes the lane out of what plays). How the interface shows this was not checked for this guide; the 2026-10-25 lanes entry lists "the fader and pan dials following the lanes while playing" as later work.

### 6.2 Write modes

Decided 2026-11-03 ([decisions.md#L2401](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/docs/decisions.md#L2401)). Lives entirely in the interface: [`src/stores/auto-write.ts`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/stores/auto-write.ts).

- Every edit passes through `SessionStore.rewrite` before it runs. `capturedValues(doc, op)` recognizes control moves: `set_track`/`set_bus` with `volume_db`, `pan` or `space` changes, and `set_effects` where only parameter values changed (same slots, same ids, same effects).
- While playing, a move on an owner in Write, Latch or Touch becomes the move plus the lane's new points, as one edit labelled "Write Automation"; the whole pass merges into one undo step.
- **Touch:** writes while the control is held (moved within `TOUCH_RELEASE_MS = 300` ms), then the lane plays again.
- **Latch:** writes from the first move and holds the last value until play stops.
- **Write:** every lane the owner has starts recording its control's value the moment play starts; a move changes what is recorded.
- A lane is created for a control that has none.
- `TICK_MS = 50`: held values are carried to the playhead every 50 ms (`writeSpan` replaces points between the last written spot and the playhead with one point).
- At pass end, `thinPoints` thins the written stretch by furthest-point splitting with tolerance 1/400 of the lane's range.
- Points land at the resolution of the transport's playhead polling, about 30 times a second. That is the main weakness of doing it in the UI.

### 6.3 Writing without restarting playback: `Transport.live`, `without_live`, and the send throttle

This was a real bug ([decisions.md#L2685](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/docs/decisions.md#L2685), commit [42cdaea](https://github.com/nwh212/Audiform/commit/42cdaea)). Symptom: "writing automations lag severely". Cause: envelopes are part of `routing_hash`, so every lane edit restarted playback; the writer sent a lane edit every 50 ms, so playback restarted about 20 times a second with ever-growing point lists.

The fix has three parts:

1. **`Transport.live: Mutex<Vec<(Id, Id)>>`**, the (owner, lane) pairs being written. The interface sets it (`transport_set_live`, from `AutoWriter.onLive`) when a pass begins and when a Touch pass lets go; `stop()` clears it.
2. **`without_live(session, live)`** ([transport.rs#L440-L459](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/engine/transport.rs#L440-L459)) returns a copy of the session with those lanes removed. `sync` always works on that copy. So while a lane is written, its control (fader, pan, spot, effect setting) is heard live through the normal level and retune paths, and the lane's growing points never touch the fingerprint. Playback restarts once when the set changes.

   ```rust
   fn without_live(session: &Session, live: &[(Id, Id)]) -> Option<Session> {
       if live.is_empty() { return None; }
       let mut s = session.clone();
       let drop_live = |owner: Id, envs: &mut Vec<Envelope>| {
           envs.retain(|e| !live.contains(&(owner, e.id)));
       };
       for t in &mut s.tracks { drop_live(t.id, &mut t.envelopes); }
       for b in &mut s.buses { drop_live(b.id, &mut b.envelopes); }
       Some(s)
   }
   ```

   Test: `lanes_being_written_are_left_out_of_what_plays` in [transport/tests.rs](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/engine/transport/tests.rs#L337-L357).
3. **Send throttle:** the written lanes go to the session at most every `SEND_MS = 200` ms (they still need to draw), and `holdOnce` drops the middle of three equal points at the write head, so a held value adds no point per step.

Subtle point: removing a volume lane from the played copy also removes the "volume lane takes the fader's place" rule, so the fader is heard live. That is exactly what Touch and Latch need.

---

## 7. MIDI

### 7.1 Model recap

Beats, f64, no PPQ (1.8). Tempo map with changes at bars (1.9). A MIDI track holds `midi_clips` and one optional `instrument` slot. The instrument's audio is the track's input, and from there it is a normal track: Voice Chain, fader, pan, sends, buses, sidechain source.

### 7.2 From beats to frames: the MIDI plan

`midi_plan` ([plan.rs#L422-L478](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/engine/plan.rs#L422-L478)) converts every unmuted clip's notes to output frames once, at plan build:

```rust
let level = 10f32.powf(c.gain_db.clamp(-96.0, 24.0) as f32 / 20.0);
for n in &c.notes {
    if n.start >= c.length || n.length <= 0.0 { continue; }      // past the clip end: not played
    let on  = tempo.samples(c.start + n.start, out_rate);
    let off = tempo.samples(c.start + (n.start + n.length).min(c.length), out_rate); // cut at the clip end
    notes.push(NotePlan { on, off: off.max(on + 1), pitch: n.pitch.min(127),
        velocity: (n.velocity.clamp(1,127) as f32 / 127.0 * level).clamp(1.0/127.0, 1.0),
        channel: n.channel.min(15), pan: n.pan.clamp(-1.0, 1.0) });
}
// notes sorted by on; offs: (frame, pitch, channel) sorted; controls: (frame, controller, value) sorted
```

`MidiPlan { instrument, tempo, notes, offs, controls }`. A track with no instrument gets no plan and is silent; nothing else breaks. `plan::midi_end` gives the session end including MIDI clips, so playback and export reach the last MIDI clip (that was a bug once, see 8).

### 7.3 Playing through an instrument in playback and mixdown

`Playing` in [render.rs#L462-L560](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/engine/render.rs#L462-L560):

- **Opened lazily** on the track's first block, from output frame `start`.
- **Controller chase:** the last value of each controller before `start` is sent at open (so a pedal already down is down).
- **Note chase:** notes with `on < start < off` are started at offset 0 in the first block (starting in the middle of a note plays it from there).
- **Instrument latency:** notes go to the instrument `inst.latency` frames early. At open, `Playing` renders `latency` frames into a sink, so the first sound heard is the sound at `start`. This keeps the mix graph unchanged: an instrument with look-ahead is aligned in the renderer, like clip chains.
- **Per block:** binary search (`partition_point`) for offs and ons in `[from, to)`, build `NoteEvent { offset, on, pitch, velocity, channel, pan }`, sort by `(offset, on)` so **at the same frame a note ends before the next starts** (a repeated note re-triggers), send controller moves due in the block, then `inst.play(events, buf)`.
- **Jumps** (loop or skip): every `Playing` is dropped, so every note is let go, and reopened at the new spot. Plug-in effects are paused and resumed on a jump; instruments are switched off and on so held notes stop.
- **Controllers** become plug-in parameters through the instrument's VST3 MIDI mapping (`Host::control_for(controller) -> Option<u32>`), and are queued as parameter changes sent with the next block. A controller the instrument does not map is ignored.

**From reading the code:** controller changes are sent at the start of the block they fall in, not at their frame (the parameter changes carry no offset). The playback render block is 2048 frames and the mixdown block 8192 frames, so a sustain pedal press can land up to about 43 ms (playback) or 170 ms (export at 48 kHz) early. Notes are sample accurate. Audiform 2 should send parameter changes with sample offsets (VST3 parameter queues support points), or split blocks at controller events.

**Mixdown:** `render_mix` runs inside `plugins::instance::fresh(...)`, so plug-ins and instruments get hosts of their own and an export never disturbs playback. If playback's own host for a slot is idle, the export borrows it (already loaded, settings as heard). A fresh instrument copy is warmed up on silence for `INSTRUMENT_WARMUP = 2.5 s` at about playback pace (15 ms sleeps between 1024-frame blocks), because sample-based instruments load their sounds in the background and would otherwise be silent at the start of the export.

**Tempo to plug-ins:** each block tells a plug-in the tempo segment playing at its position (`set_block_tempo`, `segment_at_seconds`), and where that segment began, so musical position and bar position stay right across changes.

### 7.4 Live input

[`engine/midi_live.rs`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/engine/midi_live.rs), commands in [`commands/midi.rs`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/commands/midi.rs). Uses `midir` (MIT). Client name "Audiform", connection name "Audiform input".

**Ports and per-track input.** `inputs()` lists port names. Each armed MIDI track listens to one input or all (`LiveTrack.input: None`); `"none"` in the session means the interface does not send the track at all. `MidiLive::set(tracks)` opens exactly the ports the armed tracks need (all ports if any track wants all, or step input is on) and closes the rest. Port names are matched as strings; if a device is renamed or unplugged, it simply does not open (and an error is logged).

**Arming.** The interface (`syncMidi` in `src/app/recorder.svelte.ts`) sends the armed MIDI tracks with their instrument, chain, gain and pan. Arming, disarming or a new input is sent at once; any other change (a fader drag) after 400 ms of rest, so a drag does not rebuild the voice on every step.

**Two paths for what is played:**

- **Live:** a `Voice` per armed track with an enabled instrument: a **dedicated copy** of the instrument (`instance::dedicated`) and of the chain (keyed slots removed), so playback's instances are undisturbed. Rendered on a "midi-live" thread `BLOCK = 128` frames at a time into an `rtrb` ring feeding its own output stream (`record::open_monitor`). Works whether or not the session plays. The track's gain and balance pan are applied; the voice is rebuilt only when the instrument or chain changes (`LiveTrack::sound()`), otherwise gains update in place.
- **Recorded:** see 7.6.

**Queue depth.** At least two device buffers plus a block: `ahead_for(buffer) = max(2 * buffer + BLOCK, AHEAD)` with `AHEAD = 256`, and 1024 assumed when the buffer size is not fixed. The first version held a fixed 256 frames and stuttered at 512-frame ASIO buffers (commit [e33da0b](https://github.com/nwh212/Audiform/commit/e33da0b)).

**Stall recovery.** If the queue stays full for `STALL = 500 ms` (the device stopped pulling, seen with ASIO drivers when another stream opens), the output stream is reopened and "midi live: the output stalled; opened it again" is logged. Not confirmed on hardware; listed as a known issue in the 1.0.0 notes.

### 7.5 The Keys state machine and the sustain pedal

`Keys::feed(msg, out)` ([midi_live.rs#L76-L196](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/engine/midi_live.rs#L76-L196)) turns raw MIDI bytes into `KeyEvent::{On, Off, Control}` per track:

```rust
pub struct Keys {
    pub pass_pedal: bool,   // the instrument maps CC 64 itself
    pedal: [bool; 16],      // per channel
    down: Vec<(u8, u8)>,    // keys pressed, (pitch, channel)
    held: Vec<(u8, u8)>,    // released while the pedal was down, still sounding
}
```

| Message | Behavior |
| --- | --- |
| `0x9n` note on, velocity > 0 | If the key is already down or held, emit Off first (re-strike). Push to `down`, emit On with velocity/127. |
| `0x8n`, or `0x9n` velocity 0 | Ignored if the key is not down. Remove from `down`. If the channel's pedal is down, move to `held`; else emit Off. |
| CC 64, `pass_pedal == true` | Emit `Control { controller: 64, value }`. Notes end when keys come up; the instrument sustains. |
| CC 64, `pass_pedal == false` | Pedal state is `value >= 64`. On release, emit Off for every held key on that channel. No Control is emitted. |
| CC 120 or 123 | All Sound Off and All Notes Off: Off for every down and held key on that channel. |
| Other CC | `Control { controller, value: v/127 }`. |
| `0xDn` channel aftertouch | `Control { controller: 128, value: v/127 }`. |
| `0xEn` pitch bend | `Control { controller: 129, value: (msb << 7 | lsb) / 16383 }`. |

`release_all` ends every note and resets the pedals (used when a track switches input).

**`pass_pedal` is decided per instrument:** when a voice starts, `takes_pedal = host.control_for(64).is_some()`, and `MidiLive::set` copies that into each route's `Keys.pass_pedal`. So the pedal is **passed through as CC 64 when the instrument's MIDI mapping has controller 64**, and **held in Audiform** (notes kept sounding, as a piano would) when it does not. In the second case a recording gets longer notes instead of CC 64 moves. A track with no instrument never sets `pass_pedal`, so it holds notes. Decided 2026-11-06 ([decisions.md#L2702](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/docs/decisions.md#L2702)): "sustain in other music software is not made by stretching notes but by sending the pedal as its controller (64)".

Playback of a clip's CC 64 moves goes through the same `control_for` map; an instrument without a mapping ignores CC 64 in playback (the notes are not held). Unclear whether that mismatch was intended: a clip with CC 64 on a non-mapping instrument plays short notes in playback but would have recorded long notes live.

Tests worth porting as they are: `keys_start_and_end_notes`, `the_pedal_holds_notes_until_it_comes_up`, `an_instrument_that_takes_the_pedal_gets_it_as_a_controller`, `controllers_aftertouch_and_pitch_bend_pass_on`, `routes_listen_to_their_own_input`, `live_queue_covers_two_device_buffers`.

### 7.6 Recording takes into clips

Core side: `record_start(tracks)` sets `t0 = Instant::now()` and a `Capture` per armed track; `record_track(track, true)` adds a capture for a track armed mid-recording (times still from `t0`); `record_stop()` closes open notes at the stop time and returns `MidiTake { track_id, notes: Vec<TakeNote>, controls: Vec<TakeControl> }`, times in **seconds from `t0`**. Tracks that played nothing are left out.

Interface side: `midiTakeClip` in [`src/app/midi-take.ts`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/app/midi-take.ts):

- Note times become session samples: `at + seconds * rate`, where `at` is the recording's start sample (the same start audio takes use; when recording starts while already playing, `at` is the average of the playhead before and after the inputs start).
- **Punch:** with a punch range, only notes that **start** inside it are kept.
- Samples become beats through the tempo map.
- **Bar alignment:** the clip runs from the start of the bar the first note starts in to the end of the bar the last note ends in (a note ending exactly on a bar line does not add a bar).
- Velocity back to 1..127; length at least `MIN_NOTE = 1/64` beat; channel kept when not 0.
- Controller moves inside the clip span come along; moves before the clip start are dropped (so a pedal pressed before the first bar's start is lost; from reading the code).
- One `set_midi_clips` op per track, with any audio takes, as one undo step.
- Existing MIDI clips keep playing during a MIDI take (overdub), unlike audio takes.

Limits Audiform 1 never solved:

- No recording latency offset for MIDI (audio takes have one).
- **From reading the code:** take times come from a wall clock (`Instant`) started when recording starts, while the timeline position comes from the device clock. When recording starts from stop, `transport.play(at)` runs after `midi_record_start`, so any play start delay (plug-ins loading) shifts the take early. Audiform 2 should stamp MIDI input against the audio device clock.
- Takes are not quantized automatically.

### 7.7 Controller lanes

[`src/panels/editor/midi-controls.ts`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/midi-controls.ts). `AFTERTOUCH = 128`, `PITCH_BEND = 129`. Lanes offered first: Sustain Pedal (64), Mod Wheel (1), Expression (11), Volume (7), Pan (10), Breath (2), Sostenuto (66), Soft Pedal (67), Pitch Bend, Aftertouch, plus any controller the clip already uses (marked with a dot).

- Drawn as stepped lines. Drag to draw (`paintControl`: moves in the dragged span give way to one at the end), right-drag to erase (`eraseControls`), Alt for off the grid.
- `isSwitch(id)`: 64..=69 are pedals, only up (0) or down (1); a pedal move is kept only where the value changes.
- `shapeValue`: 128 steps for CCs, 16384 for pitch bend. `valueText`: pedals "up"/"down", pitch bend as -8192..8191, others 0..127.

### 7.8 Piano roll features worth keeping

[`src/panels/editor/PianoRollDialog.svelte`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/PianoRollDialog.svelte) (its header comment is a complete feature list), logic in [`src/panels/editor/midi.ts`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/midi.ts):

- **Editing is one undo step:** the dialog works on a copy; Done or Escape commits the whole clip with one `set_midi_clips`, Cancel drops it. Inside, Ctrl+Z and Ctrl+Y step through the dialog's own history.
- **Tools:** Draw (click adds a note one grid step long; keep the button down to move it, never stretches), Select (click, box), Erase (click or drag). In every tool: drag the middle to move, either end to stretch that end (`resizeNotesStart` keeps the end), double-click or right-click to delete, right-drag to erase.
- **Grid** (`GRIDS`): 1 bar (the bar length where the clip starts, through the tempo map), 1/4, 1/8, 1/16, 1/32, 1/8 triplet, 1/16 triplet, Off. Snap switch; Alt drags freely.
- **Multi-select:** Shift-click, Shift-drag box. Moving or stretching one picked note does the same to all, with the grabbed note landing on the grid.
- **Copy, cut, paste** (Ctrl+C/X/V inside the dialog) at the last spot clicked; the clipboard lasts while the app runs, so notes paste into any clip. Pasting past the end lengthens the clip to the next bar line. The grid runs two bars past the clip end, shaded.
- **Hearing notes:** each note sounds through the track's instrument as it is drawn, picked, moved to another pitch, or given a new velocity; the keys on the left play on click; Play (or Space) plays the clip alone from its start or the last spot clicked, Loop repeats it (`instance::audition` and `instance::sequence`).
- **Lane under the roll:** Velocity, Pan, or a controller (7.7).
- **Channels:** a channel picker (swatches); `CHANNEL_COLORS` gives each channel its color (channel 1 uses the clip color).
- **Tools panel** (acts on picked notes, or all when none are picked):
  - velocity set, scale (percent), randomize; pan set, randomize;
  - **Humanize:** each note's start moves by up to N ms either way at random, never before the clip start; lengths stay (`humanize(notes, idx, beats, rand)`; the dialog converts ms to beats);
  - **Quantize** to the grid with strength 1..100% and an Ends option (`quantize`, below);
  - **Legato:** each picked note runs to the next later start among the picked, chords together;
  - **Transpose:** -1/+1 semitone, -12/+12.
- **Clip-level quantize** in the MIDI clip right-click menu: 1/4 down to 1/32 and two triplet grids.
- **Navigation:** wheel scrolls keys, Shift+wheel scrolls time, Ctrl+wheel zooms time at the pointer, Alt+wheel changes key height (kept while the app runs), middle-drag pans, a bar ruler (session bar numbers through tempo changes), a zoom bar under the roll (drag, drag ends, double-click to fit).
- **Clip operations on the lane:** double-click empty space for a one-bar clip; drag moves (snaps to beats); drag the right edge to change length; **Alt-drag the right edge to stretch** (every note's position and length scale; snaps to quarter beats, Ctrl frees it); drag onto another MIDI track to move, Alt to copy (new ids).

Quantize, exactly:

```ts
export function quantize(notes, indexes, grid, strength = 1, ends = false) {
  if (grid <= 0) return notes;
  const k = Math.min(1, Math.max(0, strength));
  const toward = (b) => b + (snap(b, grid) - b) * k;
  return notes.map((n, i) => {
    if (!indexes.has(i)) return n;
    const start = Math.max(0, toward(n.start));
    const end = toward(n.start + n.length);
    // An end that lands on (or before) its start keeps one grid step.
    const length = !ends ? n.length
      : end - start > 1e-9 ? end - start
      : Math.max(MIN_NOTE, grid * k + n.length * (1 - k));
    return { ...n, start, length };
  });
}
```

### 7.9 Step input

- `MidiLive::step_listen(true)` opens **every** input and keeps a separate `StepTap` with its own `Keys`; pressed keys are stored with `at` (seconds since step input started), whether or not any track is armed.
- The piano roll polls `step_take()` every 40 ms.
- `stepChords(keys, chordSecs = 0.06)`: keys within 60 ms of the first of a group are one chord.
- Each chord lands at the last spot clicked, as long as the last note drawn, on the picked channel, and the spot moves on by that length.

### 7.10 Bounce MIDI track to audio

Right-click a MIDI lane: Bounce Track to New Track; on a clip, Bounce Clip (or selected clips) to New Track. `export_bounce` ([commands/export.rs#L250](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/commands/export.rs#L250)) renders through the mixdown path with `track: Some(id)` (or `clips`), with track, bus and master processing as it plays, into a 32-bit float WAV in a "Bounced" folder beside the session. The new track lands right below the source, named after it; mono tracks bounce to mono files.

The span comes from the interface (`bounceRange` in `src/app/export-tools.svelte.ts`): MIDI clip spans are converted from beats to samples through the tempo map. The first version treated beats as samples, so a MIDI track bounced only a sliver (commit [44dc803](https://github.com/nwh212/Audiform/commit/44dc803)).

Edge case from reading the code: a bounce runs the track alone, so instrument tails past the last clip's end are cut at the range end, and a keyed effect loses its key source (5.3).

### 7.11 Metronome

[`engine/metronome.rs`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/engine/metronome.rs).

- `Settings { on, level_db (-60..6, default -6), sound: Tick|Wood|Beep|Shaker, accent (default true), subdivide: None|Halves|Triplets|Quarters }`. Stored as atomics, read once per block. App settings, not session data.
- The **render thread** makes the click for each block at that block's timeline position (`Click::render(start, out)`), so it follows loops, skips and tempo changes. It finds the next click with `next_from(frame, sub)`, walking the tempo segments; a jump (start not equal to the expected frame) re-finds it.
- Clicks fall on **the time signature's beat unit** (`unit_beats`, so eighths in 6/8), the first of each bar is `Kind::Bar` (higher with accent), subdivisions are `Kind::Sub`. Each click is 80 ms (`LENGTH = 0.08`). Sounds are synthesized in code.
- The click travels on its own ring to the audio thread and is added after the master fader, meters and scope. Never in a mixdown.
- "Recording only" is switched on just before a recording's playback starts so the first beat is heard. The Waveform editor's single-file playback has no click (`play_without_click`).
- Interface: a fixed Metronome row under the ruler (not a track in the session), with mute, level, sound, tempo and signature at the playhead, and a Tempo strip of flags for changes.

---

## 8. Bugs we hit and how we fixed them

Commit links: `https://github.com/nwh212/Audiform/commit/<hash>`.

| # | Symptom | Cause | Fix | Where |
| --- | --- | --- | --- | --- |
| 1 | Writing automation "lags severely". | Envelopes are in `routing_hash`; the writer sent lane edits every 50 ms; playback restarted ~20 times a second. | `Transport.live` + `without_live`; send lanes at most every 200 ms; `holdOnce`. | [42cdaea](https://github.com/nwh212/Audiform/commit/42cdaea), transport.rs, auto-write.ts |
| 2 | An imported session played about 18 dB louder than in the editor that wrote it. | Plug-in host turned on every input and fed silence to sidechain inputs; compressors heard silence and applied only makeup gain. Present since the first VST3 hosting. | Only the main input on at load; sidechain input turned on when a key arrives, off after a second without one. Test plug-in now keys from its own input when the sidechain is off. | [a66f5de](https://github.com/nwh212/Audiform/commit/a66f5de), plugins/instance.rs, plughost |
| 3 | A vocal aligner keyed from another track played late. | Latency was read with the sidechain input off; the plug-in reports more with it on. | Host measures `keyed_latency` at load; `latency_for(keyed)` uses it when the slot has a key. | [e3e5c9b](https://github.com/nwh212/Audiform/commit/e3e5c9b) |
| 4 | Plug-ins that settle on another delay after loading played late. | The reported LatencyChanged value was thrown away. | `live_latency` on the host wins once reported; playback re-aligns (restart). | [0576641](https://github.com/nwh212/Audiform/commit/0576641) |
| 5 | Short pause or pop on every effect dial move. | Every settings change rebuilt the chain and warmed it up on 0.1 s of history. | `ChainProcessor::retune`: same effects in the same slots keep running, only changed params move. | [2a34486](https://github.com/nwh212/Audiform/commit/2a34486), engine/fx.rs |
| 6 | Instruments silent until their window was opened. | The plug-in host ran the Windows message queue only while a window was open; many instruments finish loading through it. | Host runs the queue every pass while plug-ins are loaded. | [0a502a4](https://github.com/nwh212/Audiform/commit/0a502a4) |
| 7 | MIDI mixdowns silent at the start, or slow. | Fresh export copies of sample instruments still loading; also a request check ran a whole cancelled render starting every plug-in. | Warm-up on silence for 2.5 s; mixdowns borrow idle playback plug-ins; request check no longer renders. | [1ffdfd8](https://github.com/nwh212/Audiform/commit/1ffdfd8), [0a502a4](https://github.com/nwh212/Audiform/commit/0a502a4) |
| 8 | Mixdown and Go to End greyed out with only MIDI tracks. | Session end measured audio clips only. | `plan::midi_end` counted everywhere the end is used. | [13b4837](https://github.com/nwh212/Audiform/commit/13b4837) |
| 9 | A MIDI track bounced only a sliver. | Clip spans in beats were treated as samples. | Spans through the tempo map. | [44dc803](https://github.com/nwh212/Audiform/commit/44dc803) |
| 10 | Live MIDI "extremely stuttery, even with 512 on ASIO". | Live player held a fixed 256 to 384 frames; a 512-frame buffer asks 512 per callback. | `ahead_for`: two device buffers plus a block. | [e33da0b](https://github.com/nwh212/Audiform/commit/e33da0b) |
| 11 | After Stop, audio "drags and glitches and keeps playing" on ASIO with a MIDI track armed. | ASIO keeps running while any stream is open and replays what a closed stream last left. | Every stream writes silence for a few buffers before closing (`Hush`, `QuietStream`). | [e33da0b](https://github.com/nwh212/Audiform/commit/e33da0b) |
| 12 | Instrument not sounding while recording MIDI (ASIO). | Likely: driver stops serving the live stream when another opens. Not confirmed. | Reopen the live output after 500 ms full queue. Still a known issue in 1.0.0. | [ba6529b](https://github.com/nwh212/Audiform/commit/ba6529b) |
| 13 | Meters blinked toward silence in routed sessions. | A status read before the next played block got an empty reading. | Return the last reading when no new block played. | [2c5d02e](https://github.com/nwh212/Audiform/commit/2c5d02e) |
| 14 | Adding a MIDI track froze the Editor. | The same plug-in installed twice gave two registry entries with one class id; Svelte threw on duplicate keys. | Registry keeps one entry per class id. | [4dccff6](https://github.com/nwh212/Audiform/commit/4dccff6) |
| 15 | No reverb heard from a VST3 reverb on a MIDI track (tester report). | Not reproduced. Found on the way: when a plug-in reported an error from process, the host passed dry sound through even if the plug-in had written output. | Keep what the plug-in wrote; pass through only when nothing was written. Original report still open. | decisions.md 2026-10-24 |
| 16 | A vocal aligner restarted its capture whenever the playhead moved. | Plug-in effects were switched off and on at every jump. | Effects are paused and resumed on a jump; instruments still switched off and on so notes stop. | [7cf3672](https://github.com/nwh212/Audiform/commit/7cf3672) |
| 17 | Cold Play took 17 to 25 s with several sessions open. | Every edit stopped other sessions' plug-ins; Play then loaded them one by one. | Keep plug-ins of every open session running; start missing ones side by side at Play. | [c8707e0](https://github.com/nwh212/Audiform/commit/c8707e0) |
| 18 | Instrument picker and automation mode list never opened in the header. | A press on the header started a row drag unless on a "control"; dropdowns did not count. | Dropdowns, labels and links count as controls. | decisions.md 2026-10-24 tester round |

Possible bugs found while writing this guide (from reading the code; not reported, not verified):

- `midi_hash` misses note channel and pan, clip `gain_db` and `controls` (2.9).
- Controller changes are block-quantized (7.3).
- MIDI take timing uses a wall clock started before playback (7.6).
- Stems and bounces drop sidechain key sources (5.3).
- The master fader is not ramped (3).
- Master envelopes are stored but never played (1.6).

---

## 9. Recommended design for Audiform 2

### 9.1 Keep

- **The data model shape:** tracks (audio and MIDI, one struct with a kind), buses in their own list with a display `position`, one master. `Output::{Master, Bus}`, `Send { level_db, pan, pre_fader, enabled }`, `EffectSlot.sidechain: Option<Id>`. Ids everywhere, indexes only at build time.
- **Beats for MIDI, f64, no PPQ.** It made stretch, tempo changes and quantize trivial. Keep quarter-note beats even in compound meters.
- **Tempo changes at bar lines, no ramps** (until someone asks), with `tidy()`.
- **Loops refused at edit time, tolerated at load time.** Count disabled sends and keys as edges.
- **Topological order including keys**, and the `in_t` / `out_t` latency alignment with a single `lead`. It is simple and sample exact. Port the tests.
- **Pre-fader keys**, delayed inside the chain by earlier stages' latency.
- **Send law:** post follows fader not pan, pre ignores both, all follow mute and solo; balance pan.
- **Solo walks routes both ways; mute wins; keys are not routes.**
- **Three kinds of change:** levels (ramped, no rebuild), chains (swap or retune, no rebuild while latency holds), layout (rebuild). Plus the `live` lane set while writing automation.
- **Instruments aligned in the renderer** (notes sent early by their latency) so the graph treats a MIDI track like an audio track.
- **Keys state machine** with per-instrument pedal pass-through, and its tests.
- **Bar-aligned takes**, punch keeps notes starting inside, overdub for MIDI.
- **Metronome rendered at the block's timeline position, added after the master fader.**
- **Close streams on silence.**

### 9.2 Change

1. **Remove the plain/routed split and the half-second fader lag.** The lag exists because the graph runs half a second ahead. Options, best first:
   - Run the whole graph on a real-time thread at device block size, with every effect real-time safe (preallocated, no locks), and plug-ins out of process with a low-latency shared-memory ring and a deadline (or in process if Audiform 2 accepts the crash risk; see the VST3 guide). Faders, sends, mutes become per-block parameters read from atomics or a lock-free command queue, smoothed per sample. This is the professional answer and removes "Faders lag ½ s".
   - If plug-ins must stay behind pipes, keep a render-ahead thread but apply **all** gains (track faders, sends, bus faders) as the last stage on the audio thread: render the graph as per-strip "pre-fader" stems and mix them at device time. That only works without bus chains after faders, so it is a partial answer. Audiform 1 chose not to do this.
   - At minimum, ramp the master fader per sample.
2. **Graph as data, edited live.** Instead of `layout` fingerprints that restart playback, build a new graph on a worker, then swap it in at a block boundary with an atomic pointer swap (keeping running effect state by id: move `Fx` instances from the old graph into the new one when their chain is unchanged). Routing edits (add a send, change an output) then never stop playback. Only latency changes need a re-prime, and even those can crossfade.
3. **Full parallel schedule.** Compute levels of the DAG (all nodes whose inputs are ready) and run each level in parallel, including keyed strips once their source is done, instead of one "ahead" wave for tracks and one for buses.
4. **Preallocate everything** in graph build: tap buffers, key slices (`&[(Id, &[f32])]` built per block allocates a `Vec`), scratch, `Place`. Make the per-block path allocation free even if it is not on the device thread, so it can move there later.
5. **Sample-accurate events.** Send instrument parameter changes (controllers, automation) with sample offsets. VST3 parameter queues take points with offsets. Split blocks at events for built-in effects, or make automation per-sample.
6. **Stamp MIDI input with the audio clock.** Record events with the device frame counter (or a monotonic time mapped to it at each callback), and apply a MIDI input offset setting like audio takes have.
7. **Diff plans, not hashes.** Fingerprint everything the engine reads, or compute the MIDI plan and compare it. Avoid hand-maintained hashes that miss fields (see `midi_hash`).
8. **Automation in the engine.** Move Write/Latch/Touch recording into Rust: the engine knows the exact frame a fader value arrived at, so points land sample accurate instead of at UI polling resolution, and the "lane being written plays from the live control" rule becomes a per-lane state (Reading, Writing) in the strip, with no session copy.
9. **Honor envelope curves** (Hold, Spline) or drop them from the model. Decide on master automation.
10. **Consider beats for automation on MIDI-heavy sessions,** or offer "lane follows tempo". Today a tempo change moves notes but not automation.
11. **One tempo implementation.** Audiform 1 had Rust and TypeScript copies. In pure Rust, keep a cached segment table with binary search.
12. **Stems and bounces keep key sources rendering** (silent, but feeding their taps).
13. **Clip controllers before the take start:** carry the last value of each controller at the clip start into the take (pedal already down).

### 9.3 Ordered implementation checklist, with tests

Write the test first for each step; most can be ported from Audiform 1's names.

1. **Model types** (Track, Bus, Master, Output, Send, EffectSlot.sidechain, MidiClip, Note, ControlEvent, Tempo, Envelope) with serde defaults and skip-at-default. Tests: round trip; an Audiform 1 `.audfrm` with buses, sends, keys, MIDI and tempo changes loads; defaults omitted on save.
2. **Tempo map:** `tidy`, segments, beats/seconds/samples both ways, bar math. Tests: 6/8 bar is 3 beats; a change at bar 2 lands where bar 2 starts; round trip beats to samples to beats within a sample; out-of-order changes ignored; `tidy_beat_unit` rounding.
3. **Routing:** routes (disabled sends count), keys, `order` (Kahn, session order among equals), `has_loop`, damaged-file kept edges. Tests: port `order_puts_feeders_first_and_finds_loops`, `plain_sessions_are_not_routed` (if you keep a fast path).
4. **Ops with loop refusal and undo:** add/remove/restore/move bus, set bus, set sends (clamp level -96..12, pan -100..100), outputs, set effects with keys; remove bus repoints feeders to master, drops sends, unkeys, all in one inverse. Test: port `buses_route_send_key_and_undo_in_one_step`.
5. **Levels:** db clamp, balance, audible (solo both ways, mute wins), send law, polarity, automation set-aside. Tests: post-fader send follows fader not pan; pre-fader ignores both; soloing a bus plays its feeders (`soloing_a_bus_plays_what_feeds_it`); soloing a track plays its send bus; muted key source still ducks.
6. **Graph build:** strips, taps, routes, `in_t`/`out_t`, `lead`, delays. Tests: port `track_chains_keep_time_with_look_ahead`, `buses_sum_their_inputs_in_step_with_direct_tracks` (frame 5,500 case), `a_damaged_loop_still_plays`.
7. **Graph process:** order walk, ramps, mono fold after chain, meters. Tests: `levels_change_live_and_ramp` (halfway down the ramp at mid-block), `mono_folds_a_track_after_its_chain_and_before_its_pan`, `loops_play_through_the_lead`.
8. **Sidechain:** `Processor::process_keyed`, chain-internal key delay, KEYED effects. Tests: port `sidechain_key_drives_the_detector_after_earlier_look_ahead` and `sidechain_ducks_music_under_the_voice` (voice muted, 5 ms limiter on the voice, duck starts with the voice and not before).
9. **Live edits:** level mailbox or atomics, chain retune, chain swap with warm-up, graph swap. Tests: `chains_swap_in_while_playing`, `retune_keeps_the_effects_running`, `chain_updates_merge_until_picked_up`, `gain_updates_apply_without_restart`.
10. **Master:** chain, fader on the output stage with smoothing, mono check, click after master. Tests: `the_click_plays_after_the_master_fader_and_meters`, `mono_check_folds_what_plays`, a fader step produces no discontinuity larger than the ramp allows.
11. **Parallel schedule.** Test: same output bit for bit with 1 thread and N threads on a session with buses, sends and keys.
12. **Automation playback:** volume and pan lanes take the fader's and pan's place, effect lanes, post-fader sends follow the volume lane. Tests from `engine/automation.rs`; a lane edit while playing is heard without a restart.
13. **Automation writing** (in the engine): Read, Write, Latch, Touch (300 ms release), thinning with 1/400 tolerance, a pass is one undo step. Tests: Touch writes only while held; Latch holds until stop; Write covers every lane from play start; a lane being written plays from the live control; no restart while writing.
14. **MIDI plan:** notes cut at clip end, notes past the end skipped, velocity times clip gain clamped to 1/127..1, offs before ons at the same frame, controls sorted. Tests: start and end on the exact sample; tempo change moves notes; clip gain scales velocity; muted clip silent.
15. **Instrument playback:** note chase, controller chase, latency priming, jump releases notes, sample-accurate controller events. Tests: from the middle of a note it sounds; a pedal down before the start is down; an instrument with latency lines up with an audio click; CC 64 at frame N reaches the plug-in at frame N.
16. **Mixdown with MIDI and buses:** same graph, fresh or borrowed instruments, warm-up, MIDI end counted. Tests: MIDI-only session mixes down; export equals playback render sample for sample; stem through a bus keeps its key.
17. **MIDI input:** port listing, per-track input, Keys state machine, pass-pedal per instrument mapping, live voice with dedicated instances, queue depth from device buffer, stall reopen. Tests: port all `midi_live.rs` tests.
18. **MIDI recording:** device-clock stamps, bar-aligned clip, punch, overdub, controls carried, late-armed tracks. Tests: port `midi-take.test.ts` cases; a take started from stop with a 2 s play start delay still lands on the beat.
19. **Piano roll logic** (pure functions, unit tested): quantize (strength, ends), legato, humanize, transpose, step chords (60 ms), paste lengthening to next bar, stretch clip.
20. **Bounce:** track and clip bounce through tempo-mapped spans, new track below, mono for mono. Test: MIDI-only track bounce covers its full span.
21. **Metronome:** clicks on the beat unit, bar accent, subdivisions, follows loops and tempo changes, never exported. Tests from `engine/metronome.rs`.

---

## 10. File map

Paths are relative to the repository root. Links: base URL + path.

| Area | File | What it holds |
| --- | --- | --- |
| Model | `src-tauri/src/session/model.rs` | Session, Track, Bus, MasterTrack, Output, Send, EffectSlot (sidechain), MidiClip, Note, ControlEvent, Tempo, TempoChange, Envelope, AutomationMode |
| Model tests | `src-tauri/src/session/model/tests.rs` | Round trips and defaults |
| Versioning | `src-tauri/src/session/document.rs` | `schema_version`, `migrate_step` |
| Validation | `src-tauri/src/session/validate.rs` | Missing buses, bus loops, reported not refused |
| Routing | `src-tauri/src/session/routing.rs` | Node, routes, keys, order (Kahn), has_loop, is_routed |
| Ops | `src-tauri/src/session/ops/mod.rs` | `apply` with loop refusal, OpError |
| Ops | `src-tauri/src/session/ops/buses.rs` | Bus ops, SetSends, remove_bus with full undo |
| Ops | `src-tauri/src/session/ops/tracks.rs` | AddTrack (midi flag), SetTrack (output, instrument, input) |
| Ops | `src-tauri/src/session/ops/midi.rs` | SetMidiClips (tidy), SetTempo |
| Ops | `src-tauri/src/session/ops/settings.rs` | Master volume, envelope ops |
| Plan | `src-tauri/src/engine/plan.rs` | ClipPlan, MidiPlan, layout, routing_hash, midi_hash, chains, gains |
| Renderer | `src-tauri/src/engine/render.rs` | Clip rendering, `Playing` (instrument playback), jumps |
| Graph | `src-tauri/src/engine/graph.rs` | Strip, Route, Tap, latency alignment, process, parallel waves, Mix |
| Levels | `src-tauri/src/engine/graph/levels.rs` | balance, audible (solo), levels (send law) |
| Graph tests | `src-tauri/src/engine/graph/tests.rs` | Alignment, buses, solo, sidechain, ramps, swaps, damaged loop |
| Chains | `src-tauri/src/engine/fx.rs` | Fx, swap_fx, retune-first, warm-up |
| Effects | `src-tauri/src/dsp/effects/mod.rs` | KEYED, Processor::process_keyed, ChainProcessor (key delays) |
| Effects | `src-tauri/src/dsp/effects/dynamics.rs` | Compressor, Gate, Limiter keyed detectors |
| Effects | `src-tauri/src/dsp/effects/expander.rs`, `curve_dynamics.rs` | Expander, Dynamics Curve keyed |
| Automation | `src-tauri/src/engine/automation.rs` | StripAuto, automates_volume/pan, SILENT_DB |
| Transport | `src-tauri/src/engine/transport.rs` | Threads, Shared atomics, mailboxes, sync, without_live, audio callback, master fader |
| Transport tests | `src-tauri/src/engine/transport/tests.rs` | Gains live, click after master, live lanes left out |
| Metronome | `src-tauri/src/engine/metronome.rs` | Click, Settings, next_from |
| MIDI live | `src-tauri/src/engine/midi_live.rs` | inputs, Keys, Capture, Voice, Player, ahead_for, STALL, step input |
| MIDI commands | `src-tauri/src/commands/midi.rs` | midi_inputs, midi_live_set, record start/add/stop, step listen/take |
| Plug-ins | `src-tauri/src/plugins/instance.rs` | Host (latency_for, keyed_latency, control_for, process_keyed_ch), Instrument, audition, sequence |
| Mixdown | `src-tauri/src/export/mixdown.rs` | prepare (stems, clip-only), render_mix, session_end |
| Bounce | `src-tauri/src/commands/export.rs` | export_bounce |
| Tempo (UI) | `src/stores/tempo.ts` | TS tempo map copy |
| MIDI logic (UI) | `src/panels/editor/midi.ts` | GRIDS, quantize, legato, humanize, stepChords, clip ops |
| Controllers (UI) | `src/panels/editor/midi-controls.ts` | CC list, 128/129, paint/erase |
| Piano roll | `src/panels/editor/PianoRollDialog.svelte` | All piano roll features |
| Takes (UI) | `src/app/midi-take.ts` | midiTakeClip: bar alignment, punch, controls |
| Recorder (UI) | `src/app/recorder.svelte.ts` | Arming, syncMidi, take placement |
| Automation writing | `src/stores/auto-write.ts` | AutoWriter, capturedValues, writeSpan, thinPoints, holdOnce |
| Solo (UI) | `src/panels/mixer/solo.ts`, `src/panels/mixer/routing.ts` | Exclusive solo, bus solo feeders, silentTracks, sends |
| Bounce (UI) | `src/app/export-tools.svelte.ts` | bounceRange, bounce |
| Decisions | `docs/decisions.md` | 2026-10-20 (#L890), buses (#L1018), MIDI (#L1052), metronome (#L1291), automation lanes (#L1347), piano roll tools (#L1472), plug-in sidechain (#L1886), sidechain off (#L2269), write modes (#L2401), keyed latency (#L2489), MIDI input (#L2529), MIDI editing (#L2539), live MIDI queue (#L2661), writing without restarts (#L2685), CC 64 (#L2702) |
| Release notes | `docs/release-notes-0.8.0.md`, `docs/release-notes-1.0.0.md` | MIDI recording, controllers, known ASIO issue |
