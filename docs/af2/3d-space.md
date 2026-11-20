# 3D Space: handoff guide for Audiform 2

This guide explains how Audiform 1 does 3D Space (object-based surround mixing). It is written for the engineer building Audiform 2, a pure Rust rewrite. Everything here was read from the Audiform 1 code at commit `042577e1ddee7b9718d82aea7947f9aab6edd628` (tag "Version 1.0.0"). Where the code and the docs disagree, the code wins and the guide says so. Where something is unclear, the guide says that too.

Links point at GitHub, for example
`https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/dsp/space.rs`.
Below, `AF1:path#Lx-Ly` is shorthand for that URL with the path and line anchor. Commits are linked as `https://github.com/nwh212/Audiform/commit/<sha>`.

**Licence.** Audiform 1 is GPL-3.0-or-later. Many of the files here carry `Copyright (c) The GeekCast Radio Network, LLC.` (for example `dsp/space.rs`, `dsp/binaural.rs`, `dsp/diffuse.rs`, `engine/graph.rs`, `engine/graph/levels.rs`, `export/wav_multi.rs`, `src/panels/space/*`); others say `Copyright (c) The Audiform contributors.` (for example `engine/speaker_feed.rs`, `dsp/channels.rs`, `dsp/effects/routing.rs`, `dsp/effects/wide.rs`). Any code copied or closely ported into Audiform 2 keeps GPL-3.0-or-later and keeps the copyright lines of the file it came from. The formulas below are described so they can be reimplemented, but a reimplementation that follows the code closely is still best treated as derived work.

**Naming.** Audiform's rules forbid other products' feature names. Use the Audiform names: 3D Space, Spatialize, Place Elements, Spread, 3D Bus, Pass Through to Speakers, Source Channels. Neutral interop wording ("WAV channel mask order", "7.1.4 bed") is fine.

---

## Contents

1. Overview in one page
2. Data model
3. Speaker layouts, channel order, WAV masks, LFE
4. The panning law (object to speaker gains)
5. Spread and Spatialize (upmix modes, Place Elements)
6. Signal flow in the engine
7. Automation of spots
8. Export and loudness
9. Room view UX worth keeping
10. Bugs we hit and how we fixed them (plus open issues found while writing this)
11. Recommended design for Audiform 2 and an ordered checklist with tests
12. File map

---

## 1. Overview in one page

- A session becomes a 3D Space session when `session.space` is `Some(Space { layout, listen })`. Turning it off sets it to `None` but keeps every track's and bus's `space` spot, so one session holds a stereo mix (pans) and a surround mix (spots). Turning it back on restores the last layout (the UI remembers it per session id while the app runs, `src/panels/space/space.ts` `spaceToggle`).
- Each track and each bus that feeds the master (or feeds or sends to a 3D Bus) has a spot: `SpacePos { x, y, z, size, width, rotation, lfe, spread, spatialize, spatialize_mode }`. Spots are stored in room units, never as speaker gains, so the layout can change and any export can pick any layout.
- Tracks and buses stay **stereo** inside. Their effects run in stereo. Only after the chain and fader is the stereo signal placed into a multichannel **bed** (one channel per speaker of the layout).
- A mono source is one point. A stereo source is two points (its L and R), `width` either side of the spot, turned by `rotation`.
- Placement is a three-step equal-power interpolation: height between floor and ceiling layers, then between rows of speakers (front, side, back), then between the two speakers either side in the row. Size blends toward all speakers of the layers in use and renormalizes to unit power. The LFE channel never gets placed sound; only an explicit LFE send.
- Spread (any source) and Spatialize (stereo sources) add decorrelated or steered copies to the bed. Place Elements is an STFT upmix with 2048 frames of latency; every placed sound is delayed by the same amount so all stays in time.
- Multichannel clips (more than two channels) by default bypass placement: each channel goes straight to its speaker by name, through hidden per-channel copies of the track ("speaker feed").
- A 3D Bus receives placed sound on its own bed, runs its effects on the bed, and adds the bed to the master bed.
- The master chain runs on the bed. Listening is one of: Stereo (fold), Binaural (spherical head model), Speakers (each speaker on its own device output, mapped in Settings).
- Export writes the bed as WAV with a channel mask (or FLAC for 2.0, 5.1, 7.1), or a 2.0 or binaural stereo file.

---

## 2. Data model

Source: `AF1:src-tauri/src/session/model.rs#L762-L933` and the TypeScript mirror `AF1:src/stores/session.ts#L326-L359`.

### 2.1 Session level

```rust
// session/model.rs (GPL-3.0-or-later, (c) The GeekCast Radio Network, LLC.)
#[serde(default, skip_serializing_if = "Option::is_none")]
pub space: Option<Space>,          // on Session

pub struct Space {
    pub layout: SpaceLayout,
    #[serde(default)]
    pub listen: SpaceListen,       // default Stereo
}

pub enum SpaceLayout {             // serialized as strings
    #[serde(rename = "2.0")]   Stereo20,
    #[serde(rename = "5.1")]   Surround51,
    #[serde(rename = "7.1")]   Surround71,
    #[serde(rename = "5.1.2")] Surround512,
    #[serde(rename = "7.1.2")] Surround712,
    #[serde(rename = "5.1.4")] Surround514,
    #[serde(rename = "7.1.4")] Surround714,
}

#[serde(rename_all = "snake_case")]
pub enum SpaceListen { #[default] Stereo, Binaural, Speakers }
```

JSON example: `"space": {"layout": "7.1.4", "listen": "binaural"}`. The TS type is `SpaceDoc { layout: SpaceLayout; listen?: "stereo" | "binaural" | "speakers" }`. (The task brief calls it SpaceDoc; in Rust it is `Space`.)

`SpaceLayout as u128` is hashed into the playback "routing fingerprint" (`AF1:src-tauri/src/engine/plan.rs#L764-L776`), so changing layout or listen mode restarts playback in place.

### 2.2 The spot (`SpacePos`)

On `Track.space`, `Bus.space` (both `Option<SpacePos>`, skipped when `None`). `None` means the default spot.

| Field | Type | Range (`tidy`) | Default | Serialized when | Meaning |
|---|---|---|---|---|---|
| `x` | f64 | -1..1 (non-finite to 0) | 0 | always | left (-1) to right (1) |
| `y` | f64 | -1..1 (non-finite to 1) | 1 | always | back (-1) to front (1) |
| `z` | f64 | -1..1 (non-finite to 0) | 0 | always (`#[serde(default)]` on read) | ear level 0, ceiling 1, below ear level down to -1 |
| `size` | f64 | 0..1 | 0 | always (default on read) | 0 point, 1 fills the room |
| `width` | f64 | 0..1 | 1 (`full_width()`) | always (default 1 on read) | stereo only: how far each side sits from the spot |
| `rotation` | f64 | -180..180 | 0 | when non-zero | stereo only: degrees, clockwise seen from above; 90 puts L in front |
| `lfe` | f64 | 0..1 | 0 | when non-zero | LFE send level (full level at 1) |
| `spread` | f64 | 0..1 | 0 | when non-zero | share sent as a diffuse field |
| `spatialize` | f64 | 0..1 | 0 | when non-zero | stereo only: upmix amount |
| `spatialize_mode` | enum | | `side_spread` | when not default | `side_spread`, `matrix`, `ambience`, `height_lift`, `steer` (UI label "Place Elements") |

`SpacePos::default()` is "front center, as wide as the front speakers": `x 0, y 1, z 0, size 0, width 1`, everything else 0. `tidy()` clamps every value and replaces non-finite ones with defaults (`AF1:src-tauri/src/session/model.rs#L915-L933`). The engine always calls `tidy()` before computing gains (`Place::at`). The UI has the same `tidyPos` (`AF1:src/panels/space/space.ts#L59-L76`), which drops `spatialize_mode` when it is `side_spread` or unknown.

Note: the doc comment on the TS type says z is "floor 0 to ceiling 1"; the real range is -1 to 1.

### 2.3 Bus flag

```rust
/// A 3D Bus (3D Space sessions only) ...
#[serde(default, skip_serializing_if = "std::ops::Not::not")]
pub room: bool,
```

Outside a 3D Space session a `room` bus is a plain stereo bus (`AF1:src-tauri/src/engine/graph.rs#L274-L276`).

### 2.4 Clip channel handling

```rust
pub struct ChannelMix {               // Clip.channel_mix: Option<ChannelMix>
    pub sends: Vec<ChannelSend>,      // one per file channel, in file order
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub to_speakers: bool,            // "Pass Through to Speakers"
}
pub struct ChannelSend { pub left: f32, pub right: f32 } // -1..1, negative flips polarity
```

If `channel_mix` is `None`, a clip of a file with more than two channels plays to speakers in a 3D session (`plays_to_speakers`, `AF1:src-tauri/src/engine/speaker_feed.rs#L34-L40`). Setting a Source Channels preset in a 3D session turns it off.

### 2.5 Playback-only field

`Track.speaker: Option<usize>` is `#[serde(skip)]`. It is set only on hidden copies made by `speaker_feed::expanded` and pins that copy to one speaker index.

### 2.6 Effect slot routing (for surround plug-ins)

```rust
pub struct SlotRouting {
    pub wide: bool,                   // plug-in runs with its own channel layout
    pub inputs: Vec<Option<u8>>,      // per effect input, the chain channel heard
    pub outputs: Vec<Option<u8>>,     // per effect output, the chain channel replaced
}
```

`EffectSlot.routing: Option<SlotRouting>`; `None` maps by speaker name (`AF1:src-tauri/src/dsp/effects/routing.rs#L1-L60`).

### 2.7 Automation target

```rust
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EnvelopeTarget { Volume, Pan, EffectParam { slot_id, param }, Space { param: SpaceParam } }

#[serde(rename_all = "snake_case")]
pub enum SpaceParam { X, Y, Z, Size, Width, Rotation, Lfe, Spread, Spatialize }
// range(): X, Y, Z -> (-1, 1); Rotation -> (-180, 180); the rest (0, 1)
```

JSON: `{"type":"space","param":"x"}`. `spatialize_mode` is not automatable.

### 2.8 Ops

- `Op::SetSpace { space }` replaces `session.space` and returns the old value as its inverse (`AF1:src-tauri/src/session/ops/settings.rs#L68-L70`).
- Spot edits are ordinary `set_track` / `set_bus` with `changes: { space: SpacePos }`. The frontend merges consecutive single-key `space` changes on the same id into one undo step (`mergeKey`, `AF1:src/stores/session.ts#L915-L930`).

---

## 3. Speaker layouts, channel order, WAV masks, LFE

Source: `AF1:src-tauri/src/dsp/space.rs#L26-L137`.

### 3.1 The room

The room is a box. x: left -1 to right 1. y: back -1 to front 1. z: floor (ear level) 0 to ceiling 1. Every speaker sits on the box surface. The LFE speaker has no position (`at: None`).

### 3.2 Speakers

| Name | WAV mask bit | Position (x, y, z) | Azimuth (clockwise from front, `atan2(x, y)`) |
|---|---|---|---|
| L | 0x1 | (-1, 1, 0) | -45 |
| R | 0x2 | (1, 1, 0) | 45 |
| C | 0x4 | (0, 1, 0) | 0 |
| LFE | 0x8 | none | n/a |
| Ls (5.x) | 0x10 | (-1, -1, 0) | -135 |
| Rs (5.x) | 0x20 | (1, -1, 0) | 135 |
| Lrs (7.x rear) | 0x10 | (-1, -1, 0) | -135 |
| Rrs (7.x rear) | 0x20 | (1, -1, 0) | 135 |
| Lss (7.x side) | 0x200 | (-1, 0, 0) | -90 |
| Rss (7.x side) | 0x400 | (1, 0, 0) | 90 |
| Ltm (top middle, .2) | 0x1000 | (-1, 0, 1) | -90 |
| Rtm | 0x4000 | (1, 0, 1) | 90 |
| Ltf (top front, .4) | 0x1000 | (-1, 1, 1) | -45 |
| Rtf | 0x4000 | (1, 1, 1) | 45 |
| Ltr (top rear, .4) | 0x8000 | (-1, -1, 1) | -135 |
| Rtr | 0x20000 | (1, -1, 1) | 135 |

The room is square, so front speakers are at 45 degrees, not 30, and the 5.1 surrounds at 135, not 110. The .sesx 5.1 importer bends angles to match (section 3.6).

### 3.3 Channel order per layout (file order, ascending WAV mask bit)

```rust
SpaceLayout::Stereo20    => &[FL, FR],
SpaceLayout::Surround51  => &[FL, FR, FC, LFE, BL, BR],
SpaceLayout::Surround512 => &[FL, FR, FC, LFE, BL, BR, TML, TMR],
SpaceLayout::Surround712 => &[FL, FR, FC, LFE, BL7, BR7, SL, SR, TML, TMR],
SpaceLayout::Surround71  => &[FL, FR, FC, LFE, BL7, BR7, SL, SR],
SpaceLayout::Surround514 => &[FL, FR, FC, LFE, BL, BR, TFL, TFR, TBL, TBR],
SpaceLayout::Surround714 => &[FL, FR, FC, LFE, BL7, BR7, SL, SR, TFL, TFR, TBL, TBR],
```

| Layout | Channels | Order | `channel_mask` |
|---|---|---|---|
| 2.0 | 2 | L R | 0x3 |
| 5.1 | 6 | L R C LFE Ls Rs | 0x3F |
| 7.1 | 8 | L R C LFE Lrs Rrs Lss Rss | 0x63F |
| 5.1.2 | 8 | L R C LFE Ls Rs Ltm Rtm | 0x503F |
| 7.1.2 | 10 | L R C LFE Lrs Rrs Lss Rss Ltm Rtm | 0x563F |
| 5.1.4 | 10 | L R C LFE Ls Rs Ltf Rtf Ltr Rtr | 0x2D03F |
| 7.1.4 | 12 | L R C LFE Lrs Rrs Lss Rss Ltf Rtf Ltr Rtr | 0x2D63F |

The mask is the sum of the speakers' bits (`channel_mask`). Tests assert 5.1 = 0x3F and 7.1.4 = 0x2D63F (`AF1:src-tauri/src/dsp/space.rs#L359-L370`).

Interop notes (these are observations, not things AF1 discussed):
- This is WAV mask order: in 7.x the **rear** pair (0x10, 0x20) comes before the **side** pair (0x200, 0x400). Atmos-style 7.1.4 bed ordering used by many film tools is L R C LFE Lss Rss Lrs Rrs Ltf Rtf Ltr Rtr (sides before rears). A reader that honours the mask gets it right; a reader that assumes the film order swaps sides and rears. Consider offering both orders in Audiform 2's export.
- 5.1 uses the "back" bits 0x10/0x20 (mask 0x3F), not the "side" bits (0x60F).
- WAV has no top-middle bits, so 5.1.2 and 7.1.2 tag Ltm/Rtm with the top-front bits 0x1000/0x4000. A mask-only reader will think they are top-front.
- FLAC export is allowed only for 2.0, 5.1 and 7.1 (`bed_flac_ok`), whose FLAC standard orders match the orders above.

The TypeScript side mirrors this in `channelNames` and `speakerSpots` (`AF1:src/panels/space/space.ts#L14-L46`, `#L125-L144`). Note `speakerSpots` lists 7.1 speakers as sides then rears for drawing; `channelNames` gives the real file order.

Multichannel **source files** use their own name table by channel count (`AF1:src-tauri/src/dsp/channels.rs#L16-L38`): 1 M; 2 L R; 3 L R C; 4 L R Ls Rs; 5 L R C Ls Rs; 6 L R C LFE Ls Rs; 7 L R C LFE Cs Ls Rs; 8 L R C LFE Lrs Rrs Lss Rss; 10 L R C LFE Ls Rs Ltf Rtf Ltr Rtr; 12 the 7.1.4 order. Any other count is numbered "1".."n". AF1 does not read the WAV mask of an imported file to name channels; it goes by count only. A 5.1.2 or 7.1.2 file (8 or 10 channels) is therefore named as 7.1 or 5.1.4. Audiform 2 should read the mask when present.

### 3.4 LFE handling

- Placed sound never reaches LFE (no position).
- Each spot's `lfe` send adds `(L + R) * lfe * 0.5` to the LFE channel (`Place::at`: `lfe = Some((c, pos.lfe * 0.5))`; graph adds `(v[0] + v[1]) * g`). For a mono strip L = R, so `lfe = 1` is full level.
- After all tracks and buses are summed, the whole LFE channel of the master bed goes through 6 biquads: a 120 Hz 8th-order Butterworth low pass (4 sections) and a 20 Hz 4th-order Butterworth high pass (2 sections) (`AF1:src-tauri/src/engine/graph.rs#L186-L218`, applied at `#L974-L982`). This also filters LFE channels of pass-through multichannel clips and 3D Bus output. The filter runs before the master chain.
- The UI LFE fader shows dB: `20*log10(v)` or "Off".
- 2.0 has no LFE, so the send does nothing.
- Fold to stereo and binaural: stereo fold leaves LFE out (`bed_fold` is `[0,0]` for LFE); binaural sends LFE to both ears at 0.5 with no cues.
- Default fold-down of a multichannel *file* in a stereo context drops LFE (mastering practice).

### 3.5 Below ear level

`z < 0` is allowed. Layouts have no lower speakers, so the floor layer plays it (`share(z.max(0.0))`). The decisions log says the headphone mix "tilts it down a little" (2026-10-25); the binaural code only sees speaker channels and has no such tilt. **Unclear / not implemented as described.**

### 3.6 Importing 5.1 panners (.sesx)

`AF1:src-tauri/src/sesx/mod.rs#L126-L310`. A 5.1 panner's direction (degrees clockwise from front), radius, stereo spread and LFE become a spot. Directions are bent piecewise-linearly so 30 degrees lands on the front corner (45) and 110 lands on the back corner (135), or with "beside" surrounds on the side wall (90); 180 stays 180. A point at radius r toward angle a in the square room is `(r*sin a / k, r*cos a / k)` with `k = max(|sin a|, |cos a|)`. Stereo: each side gets its own direction (angle -/+ spread/2); the spot is their midpoint, width half their distance, rotation `atan2(-dy, dx)` in degrees. Drawn panner lines become space automation lanes, with extra points every 5 degrees so a spin goes round the room instead of cutting across it.

---

## 4. The panning law (object to speaker gains)

Source: `point_gains`, `source_gains`, `sides`, `fold_gains` in `AF1:src-tauri/src/dsp/space.rs#L160-L295`. AF1 calls it "our own method".

### 4.1 Building block: equal-power share

```
share(t) = (cos(t' * pi/2), sin(t' * pi/2)),   t' = clamp(t, 0, 1)
```

`between(keys, v)`: for sorted `keys`, return the two neighbours of `v` with `share((v - k_i) / (k_{i+1} - k_i))`. Below the first key or above the last, the end key gets 1.

### 4.2 `point_gains(layout, x, y, z, size)` step by step

1. **Height.** If the layout has any speaker with z > 0.5: `(floor, top) = share(max(z, 0))`. Otherwise `(1, 0)` (a raised sound stays on the floor speakers at full level).
2. **Rows.** For each layer (floor z=0, top z=1) with gain > 0: take that layer's placed speakers, collect their distinct y values sorted ("rows"), and split by `between(rows, y)`.
3. **Within a row.** For each used row, sort its speakers by x and split by `between(xs, x)`.
4. Accumulate: `g[speaker] += layer_gain * row_gain * x_gain`.

So for a speaker s at layer Lyr, row Row:

```
g_s = H_Lyr(z) * R_Row(y | rows of Lyr) * X_s(x | speakers in Row)
```

Every factor pair is (cos, sin), so `sum g_s^2 = 1` before size.

Consequences worth knowing:
- In 5.1 and up, the front row has L, C, R. A source at x = 0, y = 1 plays from C alone, not a phantom L/R. Halfway between L and C (x = -0.5) is -3 dB on each (tested).
- In 2.0 there is one row (L, R): plain equal-power pan by x; y and z do nothing.
- 5.1 floor rows: y = -1 (Ls, Rs) and y = 1 (L, C, R). A source at y = 0 is split equal power between front row and back row.
- 7.1 floor rows: -1 (Lrs, Rrs), 0 (Lss, Rss), 1 (L, C, R).
- 5.1.2 / 7.1.2 top layer has one row at y = 0 (Ltm, Rtm): all raised sound goes there regardless of y.
- 5.1.4 / 7.1.4 top rows: -1 (Ltr, Rtr) and 1 (Ltf, Rtf).
- If a row lacks a speaker at x = 0 (back rows), a centered back source is a phantom between the two.

### 4.3 Size

```
used = placed speakers of the layers in use (top ones if top > 0, floor ones if floor > 0)
u = 1 / sqrt(|used|)
g_s = (1 - size) * g_s + (s in used ? size * u : 0)
g = g / ||g||_2           (back to unit power)
```

So size 1 is equal gain on every speaker of the active layers. Size is a level blend, not decorrelation (Spread is the decorrelated one).

### 4.4 Mono and stereo sources

`source_gains(layout, pos, stereo)`:
- **Mono** (track `channels == Mono` or the track's Mono button on): one point at (x, y, z, size); the gains are halved and used for both channels of the internal stereo buffer, which carries the same signal on both sides. Net result: unit power.
- **Stereo**: two points from `sides(pos)`, both at the spot's z and size. Left channel uses the left point's gains, right channel the right point's.

```
a = rotation in radians
(dx, dy) = (width * cos a, -width * sin a)
left  = (clamp(x - dx), clamp(y - dy))
right = (clamp(x + dx), clamp(y + dy))        // clamp to -1..1
```

At rotation 90, left is in front of the spot and right behind it (tested). With width 1 and spot at front center, L lands on (-1, 1) (the L speaker) and R on (1, 1). Sides are clamped to the room, so a wide pair near a wall squashes against it.

### 4.5 Where Spread and Spatialize take power

`Place::at` multiplies the placed (direct) gains and the direct fold by `field.direct` (section 5), so total power stays near constant as Spread or Spatialize go up.

### 4.6 Folding to stereo for listening

Two different folds exist. Be aware of both.

**Per-object fold** (used when the master has no chain):

```
lr(x) = share((x + 1) / 2)                    // (cos, sin)
mono:   both channels get [l*0.5, r*0.5] with (l, r) = lr(pos.x)
stereo: left channel -> lr(left.x), right channel -> lr(right.x)
```

This ignores y, z and size. A centered mono source folds to 0.707 on each side: -3 dB compared with a plain stereo session, whose pan law is a balance (centered = full level both sides, `balance()` in `levels.rs`).

**Bed fold** (used when the master chain runs, and for 3D Bus output): each speaker channel c is folded by where the speaker stands, using `Place::speaker(layout, c).fold` summed over both sides: speaker at x gives `share((x+1)/2)`, LFE gives `[0, 0]` (`AF1:src-tauri/src/engine/graph.rs#L324-L334`). L folds to [1, 0], C to [0.707, 0.707], Ls to [1, 0], Lss to [1, 0], and so on.

These are not identical: size widens the bed fold but not the per-object fold, and a centered source at front plays from C (bed fold 0.707/0.707) versus the per-object fold which is also 0.707/0.707, so most cases agree, but a sized or raised sound does not. In Audiform 2, pick one: always fold from the bed (simplest, "what you hear is what exports").

### 4.7 Tests in AF1

`AF1:src-tauri/src/dsp/space.rs#L297-L371`: a point on a speaker plays only from it; power stays 1 for all layouts at several points with size 0 and 0.6; halfway L-C is -3 dB each; no-height layouts keep a raised sound on the floor; rotation 90 moves sides; masks; mono fold at x = -1 gives [0.5, 0].

---

## 5. Spread and Spatialize

Source: `AF1:src-tauri/src/dsp/diffuse.rs` (field at `#L554-L686`, Steer at `#L177-L551`) and the graph loop `AF1:src-tauri/src/engine/graph.rs#L847-L966`.

### 5.1 Decorrelator

Four outputs from one input. Each path: a pre-delay, then three Schroeder all-passes with g = 0.6:

```
allpass: d = buf[at]; v = x + g*d; buf[at] = v; out = d - g*v
```

Delay tables (ms), indexed by `i = (k + seed*4) % 8` for tap k:

```
PRE = [5.3, 9.7, 13.1, 17.9, 7.1, 11.3, 15.7, 19.3]
AP  = [[1.31,3.71,7.93],[1.79,4.33,8.69],[2.11,5.03,9.41],[2.53,5.77,10.13],
       [1.13,3.97,8.27],[1.97,4.61,9.07],[2.39,5.39,9.83],[2.71,6.11,10.61]]
```

Test: each copy keeps level within 10% on white noise, and copies correlate below 0.1.

Per track or bus (`Diffuser::new(rate, k)`, k = strip index): `spread` seed k%2, `side` seed 1-k%2, `left` seed 2+k%2, `right` seed 3-k%2, plus a 12 ms plain delay for Matrix, plus a `Steer` (Place Elements) built up front. All are built when the graph is built so playback never allocates. Speaker c uses tap `c % 4`.

### 5.2 `field(layout, z, spread, spatialize, mode, stereo)`

Output (`Field`): per-speaker `spread` gains, per-speaker signed `side` gains, `center: Option<(idx, gain)>`, per-speaker `fold`, `direct` (what the placed sound keeps), `steer` (Place Elements amount), flags.

`spat = stereo ? clamp(spatialize) : 0` (Spatialize does nothing on mono sources).

**Spread** (any source, signal = mid = 0.5(L + R)):

```
lift = has_top ? clamp(z, 0, 1) : 0
w(top speaker)   = 2 * min(lift, 0.5)
w(floor speaker) = 1 - max(lift - 0.5, 0)
w(LFE) = 0
spread_gain_c = sqrt(spread) * w_c / ||w||
direct *= sqrt(1 - spread)
```

At ear level only floor speakers; at z = 0.5 both layers fully; at z = 1 the top full and the floor half.

**Place Elements (`steer`)**: `steer = sqrt(spat)`, `direct *= sqrt(1 - spat)` (equal power, see bug list), return early (no side or center feed).

**Other Spatialize modes** (signal per mode below):

```
heights_only = (mode == HeightLift) && has_top
eligible speaker: heights_only ? z > 0.5 : (y < 0.5 || z > 0.5)   // not the front row
w_c = +1 if x < -0.1, -1 if x > 0.1, else 0 (center-line speakers stay out)
side_gain_c = spat * 0.8 * w_c / ||w||
center = front-row floor speaker with |x| < 0.1, gain spat * 0.5   (fed the mid)
direct *= 1 - 0.3 * spat
```

In 2.0 nothing is eligible, so only the fold (listening) sees anything; the decisions log says "In 2.0 it only adds the decorrelated side to the fold". Reading the code, with no eligible speakers `side` is all zero and 2.0 has no C, so in 2.0 Spatialize only lowers `direct`. **Unclear whether that log line is still true.**

### 5.3 What the engine sends per mode

Per frame, `mid = 0.5(L+R)`, `side = 0.5(L-R)`:

| Mode | Signal on speaker c |
|---|---|
| SideSpread, HeightLift | `side_decorr[c%4] * side_gain_c` (sign gives left in phase, right inverted) |
| Matrix | `delay12ms(side) * side_gain_c` (no smearing, one plain delay) |
| Ambience | `left_decorr(L)[c%4] * w` if w > 0, `right_decorr(R)[c%4] * (-w)` if w < 0 (both in phase) |
| Steer | handled by `Steer`, below |
| (all) plus Spread | `spread_decorr(mid)[c%4] * spread_gain_c` |
| (all but Steer) plus center | `mid * center_gain` on C |

Each contribution goes to the bed (or the 3D Bus's bed) and is folded to stereo with `field.fold` (speaker x, equal power) into the listening mix. The field only runs on a strip's main output (route 0), because the decorrelators keep state once per block. Sends to a 3D Bus get the spot only, no Spread or Spatialize.

### 5.4 Place Elements (`Steer`), the STFT upmix

Constants: `STEER_FRAME = 2048`, hop = 512 (75% overlap), sine window on analysis and synthesis, `BINS = 1025`, 72 direction steps, bins below 120 Hz untouched, `FRONT_AMBIENCE = 0.55`, smoothing `A = 0.7` per hop (about 30 ms at 48 kHz). Output is exactly `STEER_FRAME` frames late.

Setup per layout:
- `front[side]`: the first floor speaker with y >= 0.5 and |x| > 0.1 on each side (L, R).
- `round[c]`: every other placed speaker with |x| > 0.1 that is not in the front floor row (y < 0.5 or z > 0.5), assigned to its side, gain `sqrt((1 - 0.55) / count_on_that_side)`.
- `dirs[d]`: for d in 0..72, `point_gains(layout, sin th, cos th, 0, 0)` normalized to unit power, th = d/72 * 2pi. (A unit circle, which means corner speakers at radius sqrt 2 are reached by interpolation.)
- `turn[c][k]`: per round speaker, a phase per bin: random knots every 8 bins, smoothstep between them taking the short way round, 0 below bin 8. Deterministic xorshift seeded per strip.

Per hop, per bin k (`L_k`, `R_k` spectra):

```
if k < low_bins (120 Hz):   out[front L] += L_k; out[front R] += R_k; continue

Pll = A*Pll + (1-A)|L|^2 ; Prr likewise ; Plr = A*Plr + (1-A) L conj(R)
then sum each over k-w..k+w with w = clamp(k/96, 1, 3)     -> el, er, slr
cross  = |slr|
alike  = clamp(cross / sqrt(el*er), 0, 1)                   // coherence
pan    = clamp((er - el) / (el + er), -1, 1)
behind = clamp(-Re(slr)/cross, 0, 1) * alike                // opposite phase
hf     = clamp((k - bin(4 kHz)) / bins(6 kHz), 0, 1)
m      = alike ^ (1 - 0.5*hf)                               // lean to direct up top
a      = sqrt(max(1 - m^2, 0))                              // ambient share
dl, dr = L_k*m, R_k*m
unit   = slr / cross  (or 1)
direct = (dl + dr*unit) scaled to magnitude sqrt(|dl|^2 + |dr|^2)
front  = sign(pan) * |pan|^0.8 * 80          // degrees
side   = |pan| < 0.05 ? 1 : sign(pan)
deg    = (front + side * behind * (180 - |front|)) mod 360
gains  = lerp(dirs[floor(pos)], dirs[ceil(pos)], frac), pos = deg/360*72
out[c] += direct * gains_c
ambient: out[front side] += (side spectrum)*a * (has_round[side] ? sqrt(0.55) : 1)
         out[round c]    += (its side spectrum)*a * g_c * turn[c][k]
```

Then inverse FFT per speaker, window, scale `0.5 / STEER_FRAME`, overlap-add. The engine multiplies each output by `field.steer` and adds to the bed and the listening fold.

Tests: a centered tone ends up loudest on C; hard left reaches the left of the room at least 50 times more than R; alike but out of phase goes behind (back energy more than 4 times L+R+C). There is an ignored measurement test `measure_place_elements` that runs a music file through 7.1 and prints per-band level and SNR of the stereo fold against the source (`AF1:src-tauri/src/dsp/diffuse.rs`, run with `--ignored --nocapture` and env `STEER_FILE`).

### 5.5 Latency alignment

In a 3D session the graph's total lead includes `STEER_FRAME` (`AF1:src-tauri/src/engine/graph.rs#L452-L460`), and every placed route (to the master or into a 3D Bus) is delayed by `STEER_FRAME` extra (`held`, `#L466-L485`). Place Elements reads the signal *before* that delay (`steer_in`) because it adds its own. A 3D Bus's output to the master is not delayed again (its inputs already were). Cost: every 3D session has 2048 more frames of render lead, even with Place Elements unused.

---

## 6. Signal flow in the engine

Main file: `AF1:src-tauri/src/engine/graph.rs`. Levels and `Place`: `AF1:src-tauri/src/engine/graph/levels.rs`.

### 6.1 Always routed

`routing::is_routed` returns true for any 3D session (`AF1:src-tauri/src/session/routing.rs#L171-L184`). So the whole mix runs on the render thread (about half a second ahead of the device) and the master fader is applied on the audio thread.

### 6.2 Pre-pass: speaker feed (multichannel clips)

Before playback, export and peak drawing, the session is passed through `speaker_feed::expanded` (`AF1:src-tauri/src/engine/speaker_feed.rs#L141-L304`). It returns `Cow::Borrowed` when nothing changes.

For each audio track, clips that `plays_to_speakers` leave the track. Each channel k of such a clip (named by `channels::labels(n)`) gets a `Target`:
1. The layout has a speaker of that name: `Speaker(index)`.
2. Name is LFE and the layout has none: dropped.
3. Some other layout has that speaker (looked up in 7.1.4, 5.1, 5.1.2): `Spot(position)`, placed with the panning law (size 0, width 0).
4. Unknown order: channel k to speaker k if it exists.

Channels are grouped by (target, destination) into hidden mono tracks: a clone of the track with `channels = Mono`, pan 0, pan and space lanes removed, `speaker = Some(i)` or `space = Some(spot)`. Each copy's clip has a `ChannelMix` with only channel k at 1.0 to both sides. Ids are derived by XOR with fixed salts so live edits find the same copies across rebuilds:

```rust
one.id = Uuid::from_u128(c.id.as_u128() ^ (0xc1a0_0000_u128 + k as u128 + 1)); // per channel clip copy
h.id   = Uuid::from_u128(t.id.as_u128() ^ (0x5eed_0000_u128 + i as u128 + 1)); // per hidden track
```

A pinned copy's `Place::speaker(layout, c)` puts 0.5 on speaker c for each side (mono strip, so unit gain) and folds by the speaker's x.

**Hidden 3D Bus for surround plug-ins.** If the track outputs to the master and its chain (or a clip's chain) has an enabled slot with `routing.wide == true` (`needs_all_channels`, `#L79-L86`; this lives in `speaker_feed.rs`, not `engine/fx.rs`), a hidden bus with `room = true` takes over:
- Track-level: one bus `"{track} (speakers)"`, id `t.id ^ 0xb05_0000`, with the track's fader, mute, sends, automation mode, effects (same slot ids, so the plug-in window still works) and Volume/EffectParam lanes. The copies output to it at 0 dB with empty chains.
- Clip-level: a bus per such clip, id `c.id ^ 0xc0b5_0000`, running the clip's effects then a *second instance* of the track's effects (slot ids `slot.id ^ salt`), with clip effect lanes shifted by `clip.start` and the track's lanes remapped to the new slot ids. Track input and output gains fold into the chain's output gain.
- Tracks that output to a bus are left alone ("a 3D Bus can only go to the master").

### 6.3 Levels and Place (live)

`levels(s)` (`AF1:src-tauri/src/engine/graph/levels.rs#L226-L306`) builds per track and bus a `NodeLevel { main: [l, r], sends, mono, place: Option<Arc<Place>> }`. In a 3D session the pan multiplier is `[1, 1]` (the spot replaces the pan). `place` is set when the node is "placed": it outputs to the master, or outputs to or sends to a `room` bus (`placed`, `#L206-L220`). Pinned copies use `Place::speaker`; others `Place::at(layout, spot, stereo)` where `stereo = channels != Mono && !mono`. 3D Buses themselves get no place.

`Place` holds `bed: [Vec<f32>; 2]` (per side, per speaker), `fold: [[f32; 2]; 2]`, `lfe: Option<(idx, gain)>`, `field: Option<Field>`, plus the layout, tidy spot and stereo flag (so automation can rebuild it). Levels are sent live: moving a dot is heard without a restart, because spots are not in the routing fingerprint.

### 6.4 Per block (`Graph::process`, `#L695-L1035`)

1. Clear bus inputs, 3D Bus `bed_in` buffers, master input, and the master `bed` (always made in 3D sessions, because its channels are metered).
2. For each node in routing order, run its chain (stereo), then for each route (0 = main output, then sends): copy `pre`, ramp the fader gain from last block's level to this block's, apply volume/pan automation, meter.
3. If the route is the main output to the master, or any route into a 3D Bus, and the node has a place (automated spot if any):
   - Fold to stereo with `p.fold` into `master.input` (skipped for 3D Bus routes; their stereo comes later from their bed).
   - Add `L * p.bed[0][c] + R * p.bed[1][c]` to each bed channel c (master bed or the 3D Bus's `bed_in`).
   - Add LFE send.
   - On route 0 only: Spread / Spatialize field, then Place Elements.
   - Skip the plain stereo add.
4. Otherwise add the stereo signal to the destination bus input or master input as in a plain session.
5. 3D Bus nodes run `run_room_bus` instead (`#L1040-L1085`): set automated effect params, run the chain over `bed_in` with `Fx::with_channels(..., bed_names)`, compute the fader ramp and volume automation as a per-frame gain from a stereo buffer of ones (average of L and R), delay the bed by `bed_delay` (frames of its main route delay, so it lines up), add to the master bed and fold with `bed_fold` into `master.input`.
6. After the loop: LFE filter on the bed's LFE channel. If the master has a chain: run it on the bed (with sidechain keys), then rebuild `master.input` as the bed fold. Update per-speaker peaks.
7. Output: with Binaural, `out = Binaural::process(bed)`; else `out = master.input` (the stereo fold). The bed is kept for exports and Speakers listening (`Graph::bed()`), before the master fader.

Key point: without a master chain, the stereo listening mix is the sum of per-object folds; with one, it is the fold of the processed bed. Both include Spread/Spatialize/steer contributions.

### 6.5 Master chain on the bed

The master chain is built with `Fx::with_channels(&s.master.effects, rate, &bed_names)` where `bed_names` are the layout's speaker names. Live chain edits use `swap_fx_ch` with the same names (`AF1:src-tauri/src/engine/fx.rs#L96-L139`). Chain meters show the first two channels.

How built-in effects behave on more than two channels is in `AF1:src-tauri/src/dsp/effects/wide.rs` (header comment `#L5-L31`):
- **Reverb** (`verb.rs`, `Reverb::wide`): one room; every speaker's input feed is L [1,0], R [0,1], C/Cs/M [0.707,0.707], other L* [0.707,0], other R* [0,0.707], LFE none; each output speaker takes its own row of the 8x8 Hadamard mix of the tail lines (rows in order 1,2,4,7,3,5,6,0; row 0 is the plain sum, used last), then its own short all-pass ((0.9 + 0.61 a) ms, g 0.5); total tail gain `sqrt(2 / n)` so the whole bed's tail is as loud as the stereo tail; early reflections dealt among speakers; LFE gets nothing (`AF1:src-tauri/src/dsp/effects/verb.rs#L302-L380`).
- **Convolution Reverb** (`reverb.rs`, `ConvBed`): same input feeds; L and R outputs get the IR's two sides, C gets `0.35 (wl + wr)`, heights get their side at 0.6 through a decorrelating delay of 9 + 0.7 i ms, rears (`*rs`) 0.8 at 13 + 0.5 i ms, others (sides, 5.x surrounds) 0.85 at 7 + 0.5 i ms; LFE none; dry delayed by the convolution latency (`AF1:src-tauri/src/dsp/effects/reverb.rs#L293-L350`).
- **Echo / Delay and Modulation**: native on the bed; speakers in a "ring" (floor clockwise from front, then heights, LFE excluded; `ring()` in `wide.rs#L302-L340`). Each speaker has its own line. Room spread s (default 50%) feeds back `own*(1-s) + (prev+next)*s/2`; ring times pulled apart up to 6% (`MAX_APART = 0.06`). Ping-pong passes each repeat to the previous speaker in the ring. Modulation offsets each speaker's sweep phase around the ring.
- **Gain**: every channel.
- **Linked dynamics** (Dynamics, Limiter, Breath Gate): stereo copies per speaker group, all keyed from the loudest channel of the bed (or the sidechain), so the image holds.
- **Other effects**: stereo copies per speaker pair by name (L/R, Ls/Rs, Lss/Rss, Lrs/Rrs, Ltf/Rtf, Ltr/Rtr, Ltm/Rtm), lone speakers (C) as a copy fed the same on both sides; numbered channels pair in order (`groups()`, `wide.rs#L134-L170`). Effects in `SKIP_LFE` (echo, pitch shift, phase curve, modulation, amp, tube preamp/opto, vox strip, speech leveler) leave LFE alone, delayed by their latency.
- **L/R only** (`STEREO_ONLY`): Channel Mixer, Phase Rotate, Center Channel Extractor, Stereo Width, Center Remover. (A brief per-pair version of the extractor and width was reverted on 2026-10-31.)
- **Plug-ins**: asked for the bed's layout on their main buses; if refused, other layouts with as many speakers are tried, then stereo on L/R (decisions 2026-11-06). `routing.wide` forces the plug-in's own layout. Default routing maps by name with fallbacks (`routing.rs#L28-L60`): Ls to Lss or Lrs, Lc/Rc (cinema 7.1) to the side pair, Ltf/Ltr to Ltm and back, M to C or L.

Tracks and buses stay stereo in 3D sessions. "A bed-wide reverb belongs on the master" (or on a 3D Bus).

### 6.6 Meters

- `GraphMeters.bed` is each speaker's peak since last read, taken after the master chain, before the master fader. The transport multiplies by the master gain and reports `bed_meters` in layout order (`AF1:src-tauri/src/engine/transport.rs#L427-L429`, `#L561-L564`). The UI shows a row per channel in Levels and narrow bars on the master strip, also while stopped.
- Track and bus meters stay stereo (taken on route 0 before placement). Pinned copies are not shown on their source track's meter (known limit).
- Flicker fix: a status read with no new played block repeats the last reading (commit `2c5d02e`).

### 6.7 Binaural listening

Source: `AF1:src-tauri/src/dsp/binaural.rs`. A spherical head model with no measured HRTF data. Constants: head radius `a = 0.0875 m`, `c = 343 m/s`, `ALPHA_MIN = 0.1`, `THETA_MIN = 150` degrees.

Per speaker (not LFE):
- Raise heights: `v = normalize([x, y, z*0.7])` (rooms about half as high as wide, so raised speakers sit about 35 degrees up).
- Per ear (side = -1 left, +1 right): `theta = acos(clamp(v.x * side))` (angle to the ear axis).
- Delay (Woodworth-style): `extra = theta < 90deg ? -a/c * cos(theta) : a/c * (theta - pi/2)`; `delay = round((a/c + extra) * rate)` samples.
- Head shadow: one-pole one-zero shelf `H(s) = (1 + s*alpha/(2 w0)) / (1 + s/(2 w0))`, `w0 = c/a`, by bilinear transform with `k = 2*rate` (no prewarp), where
  `alpha(theta_deg) = (1 + ALPHA_MIN/2) + (1 - ALPHA_MIN/2) * cos(theta_deg / THETA_MIN * pi)`.
- Gain 0.7 per path.
- Height "air" for z > 0.5: `lp = (1-c)x + c*lp`, `c = exp(-2 pi 6000 / rate)`, `y = x + 0.35 (x - lp)`.
- LFE: delay 0, gain 0.5 to each ear, no filter.
- Output per ear: sum of paths, then a level shelf `with_top(1 / alpha(90))` that undoes the shadow a sound straight ahead gets (both ears at 90 degrees). `alpha(90)` is about 0.756 (-2.4 dB), which matched the measured loss before the fix.

Delays use a power-of-two ring buffer per speaker (at least 64). Tests: a left surround reaches the left ear first and with 1.5 times the energy; C reaches both ears identically; a centered impulse is flat within 0.5 dB from about 190 Hz to 16 kHz.

Limits (from the decisions log): front/back and height cues are weak; it is a first version.

### 6.8 Speakers listening and output mapping

Source: `AF1:src-tauri/src/engine/output.rs#L17-L41` and `AF1:src-tauri/src/engine/transport.rs#L787-L830`, `#L1330-L1370`.

```rust
pub struct OutputMap { pub stereo: [u16; 2], pub speakers: HashMap<String, u16> }
// speaker(name, index) = speakers.get(name) or index  (L on output 1, R on 2, C on 3...)
```

With `listen == Speakers`, playback maps each layout speaker to a device output, opens the device with at least `max(output) + 1` channels (default config if it has enough, else the widest config at the default rate), and on the audio thread writes `bed_sample * master_gain` to each mapped output. The metronome click goes to the stereo pair, added to whatever speaker shares that output. Meters, phase wheel and scope still read the stereo fold. File preview and input monitoring stay on outputs 1 and 2. The map is set in Settings > Channel Routing.

---

## 7. Automation of spot parameters

### 7.1 Engine

Source: `AF1:src-tauri/src/engine/automation.rs`.

- `StripAuto::new` collects `EnvelopeTarget::Space { param }` lanes into `space: Vec<(SpaceParam, ParamLane)>` (enabled lanes with at least one point).
- Once per graph block, at the **middle** of the block, `place_at(base, pos)` copies the base spot, sets each automated param to `lane.value_at(pos)`, and builds a fresh `Arc<Place>` with `Place::at` (`AF1:src-tauri/src/engine/graph.rs#L764-L768`). Unautomated params keep the spot's value, so dragging a dot still works for them.
- `ParamLane::value_at` is linear between points, flat outside (`AF1:src-tauri/src/engine/plan.rs#L163-L178`). It ignores the point's curve type (see open issues).
- Block size is 2048 frames in playback and 8192 in export (`transport.rs#L67`, `mixdown.rs#L28`), and the graph block equals the render block. The resulting gains are applied as constants across the block, with no ramp. So automated spots step every 43 ms in playback and every 170 ms in export at 48 kHz (see open issues).
- Editing any lane restarts playback in place: track and bus envelopes are part of the routing fingerprint (`plan.rs#L777-L787`).
- Pinned speaker copies drop Pan and Space lanes.
- Writing automation (Write, Latch, Touch): the UI captures each changed spot field from a `set_track`/`set_bus` op as a separate space lane value (`AF1:src/stores/auto-write.ts#L58-L72`). Lanes being written are excluded from playback so the live control is heard and playback does not restart on each point (decisions 2026-11-06).
- Lane picker in a 3D session: X, Y, Z, Size, Spread, LFE for all; Width, Rotation, Spatialize only for stereo owners; Pan is hidden (`AF1:src/panels/editor/automation.ts#L39-L49`, `#L89`).

### 7.2 Nodes follow automation in the UI

`automatedPos(pos, envelopes, at)` replaces each enabled space lane's param with `laneAt(points, at)` (`AF1:src/panels/space/space.ts#L199-L226`). `at` is the transport position while playing, else the playhead. While a dot is being dragged, the live dragged spot is shown instead. Tested in `AF1:src/panels/space/space.test.ts`.

---

## 8. Export and loudness

### 8.1 Choosing what to write

`space_out(session, req)` (`AF1:src-tauri/src/export/mixdown.rs#L100-L134`):
- Not a whole mix (a single track stem, clip only, dry, or `channels == 1`): `None`. The session is still 3D but forced to `listen = Stereo`; the stereo fold is written.
- `req.binaural`: stereo output, listen Binaural.
- Otherwise `layout = req.space_layout.unwrap_or(session layout)`, listen Stereo; 2.0 writes stereo, others write the bed.

The render (`render_mix_inner`, `#L272-L345`) sets the session's space to that layout, expands speaker feeds, builds a routed `Graph`, `set_want_bed`, and for bed exports sinks `graph.bed()` multiplied by the master gain. Speaker layout is chosen per export, independent of the session layout.

### 8.2 Writers

- Bed as WAV: `MultiWav` (`AF1:src-tauri/src/export/wav_multi.rs`): `WAVE_FORMAT_EXTENSIBLE` (0xFFFE), fmt chunk size 40, cbSize 22, valid bits = bits, the layout's channel mask, subformat GUID PCM (1) or IEEE float (3) with the standard tail, 16 or 24-bit PCM with TPDF dither (two xorshift uniforms, difference scaled to 1 LSB) or 32-bit float. Float files carry a `fact` chunk with the frame count. Sizes are written in `finish`; files over 4 GB data fail with a message (no RF64).
- Bed as FLAC: only 2.0, 5.1, 7.1 (8 channels max, standard orders), through the normal encoder.
- Other formats are refused for a bed with a plain message.
- 2.0 and binaural use the normal stereo encoder in any format.

### 8.3 Loudness

- Normalize (bring to a LUFS target) is stereo and mono only. The Export dialog disables it for multichannel beds and the request docs say "a 3D Space speaker bed is left as mixed" (`mixdown.rs#L76-L80`, `AF1:src/app/ExportDialog.svelte#L174-L196`).
- The loudness meter is BS.1770 via the `ebur128` crate, two channels (`AF1:src-tauri/src/export/loudness.rs#L110-L139`). No multichannel channel weights (surround +1.5 dB, LFE excluded) are used anywhere.
- The Export dialog warns that browser-based players fold multichannel files into stereo with no turn-down (measured: 12 channels at -12 dBFS came out at about +1.2 dBFS), and suggests 24-bit for sharing.
- See open issue 10.B: Check Loudness on a whole 3D mix in a non-2.0 layout appears to feed bed samples into the stereo meter.

---

## 9. Room view UX worth keeping

Source: `AF1:src/panels/space/SpacePanel.svelte`, `SpaceFader.svelte`, `space.ts`.

- **View**: the room from above, front at the top, a square with 10% margin (`toView`, `fromView`). Lit floor, distance rings at 1/3, 2/3, 1, center lines, "FRONT" label, a listener head facing front. Ear-level speakers as rounded squares with a cone; height speakers as diamonds offset inward from their corners, named on the outside so dots never cover them.
- **Which sources show**: tracks and buses that feed the master or feed/send to a 3D Bus. Tracks with no audio or MIDI clips are hidden. 3D Buses are not placed and not shown.
- **Track buttons**: a scrolling list beside the room, one colored button per placed source; click picks it (same as picking in the editor).
- **Stacking**: the picked source is drawn last so one of several in the same spot can be grabbed.
- **Dots**: mono is one dot. Dot radius grows with room size, height (`1 + max(0,z)*0.5`) and size (`+ size*1.2`). A ring marks raised sources. A tag under each dot shows the name, height as an up or down percentage, and "LFE" when the send is on.
- **Stereo pair**: a center diamond moves the pair. L and R dots: drag sets width as the distance from the center (clamped to 1); Alt also turns the pair toward the pointer. A zero-width pair still draws its dots at least 2.2 units apart so they can be grabbed. When picked, two turn arrows across the pair's axis rotate it as the pointer goes round the center.
- **Wheel**: over a dot, z +/-0.05; Shift+wheel turns a pair by 5 degrees.
- **Snapping** (toggle on the bar, saved as `settings.spaceSnap`, on by default; Alt disables while dragging): spot to a speaker within 0.09 room units (1.5 times the 0.06 tolerance), else each axis to the quarter grid within 0.06; width to quarter steps within 0.04; rotation to 15 degree steps within 4 degrees (`snapSpot`, `snapAngle`).
- **Drag edits** are sent one at a time; while one is in flight only the latest is queued (`sendPos`). The view shows the live spot immediately. Consecutive edits merge into one undo step.
- **Side panel** for the picked source: Height (-100%..100%), Size, Spread, then for stereo Width, Rotation, Spatialize and a Method list (with tooltips per mode), LFE send (shown in dB), Left/right, Back/front, and "Back to Front Center". Faders: drag, Ctrl for fine (0.1 times), wheel, arrow keys (Shift for 10 steps), double-click to the origin/default.
- **Bar**: 3D Space on/off toggle, Layout, Listen (Stereo speakers, Headphones, Speakers each on its own output), Snap, Mix Down to New File (bed opened in the Waveform editor), Export Mixdown.
- **Stereo session**: the panel explains 3D Space and offers a layout picker and "Turn On 3D Space" (default 7.1.4).
- **Waveform editor mode**: the panel says to switch to Multitrack.
- Pan controls are replaced by a 3D button or tag in track headers and the Mixer in 3D sessions.

---

## 10. Bugs we hit and how we fixed them

### 10.A Fixed in AF1

| # | Symptom | Cause | Fix | Where |
|---|---|---|---|---|
| 1 | Changing a track's volume while playing scrambled levels and placement of tracks with 5.1 clips until restart | All per-channel hidden clip copies kept the clip's id; live clip gain updates are found by id, so every copy got the first copy's channel mix | Each copy gets a stable derived id (`c.id ^ (0xc1a0_0000 + k + 1)`); test that a mid-play change matches a fresh start | `speaker_feed.rs`, `graph/tests.rs` `a_level_change_in_a_3d_session...`; commit `eed2229` |
| 2 | Surround plug-ins did nothing useful on multichannel clips | Each channel played through its own mono hidden track with its own effect copy, so a 5.1 plug-in heard one channel | Hidden 3D Bus per track (or per clip) when a `wide` slot is present; runs effects on the bed | `speaker_feed.rs`; commit `581dfd9` |
| 3 | A 7.1 plug-in on a 7.1 file passed only L and R | Plug-ins ran stereo unless "own channels" was on | Ask plug-ins for the chain's layout; then try other same-size layouts; then stereo | plug-in host; commits `c3bea30`, `bc11c6e` |
| 4 | Master chain in a 3D session only changed the listening mix | Initially the master chain was not run at all in 3D | Master chain runs on the bed, listening folded from processed bed | `graph.rs`; commit `0634cb2` |
| 5 | Place Elements at 50% lost about 2.4 dB everywhere | Placed sound and steered sound were crossfaded with plain gains, but they differ per speaker | Equal-power: `steer = sqrt(spat)`, `direct *= sqrt(1-spat)` | `diffuse.rs field`; commit `1085e1f` |
| 6 | Place Elements at 100%: mids 1.2 to 1.8 dB short, lows steered around and 1.8 dB heavy in fold, voice air treated as ambience, ambience smeared | Plain gain split, random per-bin phase on ambience, no low-band bypass, no HF lean | Below 120 Hz untouched; power kept between direct and ambient; direct phase from smoothed cross power; smoothing across frequency; interpolate between direction steps; lean to direct 4 to 10 kHz; slowly wandering phase turns; frame 1024 to 2048 | `diffuse.rs Steer`; commit `1085e1f` |
| 7 | Centered voice lost about 2.4 dB of top on headphones | Head shadow at 90 degrees on both ears | Per-ear inverse shelf `with_top(1/alpha(90))` | `binaural.rs`; commit `1085e1f` |
| 8 | LFE send carried too much above the sub band | 24 dB/oct low pass only | 8th-order 120 Hz low pass plus 4th-order 20 Hz high pass | `graph.rs lfe_filters`; commit `9cc265e` |
| 9 | 7.1.4 WAV played very loud and noisy in a browser-based chat app | Browser fold adds all channels with no make-up; likely also float multichannel WAV misread as integer | `fact` chunk on float files; Export dialog warning; suggest 24-bit | `wav_multi.rs`, `ExportDialog.svelte`; commits `34cc20a`, `72f588f` |
| 10 | Track, bus and speaker meters flickered in 3D sessions | Status read before the next block played got an empty reading | Hold the last reading | commit `2c5d02e` |
| 11 | Writing automation lagged badly | Each lane point edit restarted playback (about 20 times a second) | Lanes being written are told to the engine and left out of the fingerprint | decisions 2026-11-06; commit `d1ea3fd` |
| 12 | One of several sources at one spot could not be grabbed | Draw order | Picked source drawn last | commit `6b0a439` |
| 13 | Many speaker rows did not fit the Levels panel | Layout | Rows share the panel height | commit `7bd5182` |
| 14 | 5.1 .sesx import: panner Center was briefly mapped to size | Misread: it limits the center speaker | Reverted; Center is left out | commit `5dafcda` |

Use `git log --oneline -- <file>` on the files in section 12 for more; the 3D-related history starts at `b80b194` (first 3D Space commit).

### 10.B Open issues found while writing this guide (verify before relying on them)

1. **Spot changes step per block with no ramp.** `NodeLevel.place` gains (live dot moves and automated spots) are applied as constants per block. Fader gains are ramped (`ramp()`), placement gains are not. With 8192-frame export blocks, automated movement steps every 170 ms at 48 kHz. Expect zipper noise on fast moves. In Audiform 2, interpolate per-speaker gains across the block (or use small sub-blocks, for example 64 frames).
2. **Check Loudness in a 3D session.** `loudness::measure` calls `render_mix` with the request as given. For a whole-mix check without `space_layout` in a non-2.0 session, `space_out` chooses a bed, so the sink gets interleaved bed frames (6 to 12 channels) while the meter is created for 2 channels. The App calls `loudnessCheck({ range })` with no layout (`AF1:src/app/App.svelte#L715`). The numbers are likely wrong. No test covers it.
3. **Automation curve types ignored by the engine.** `automation::lane` drops `EnvelopePoint.curve`; `ParamLane::value_at` is always linear. The UI's `laneAt` honours `hold`. So room dots and audio can disagree on hold points. (This affects all track and bus lanes, not only space.)
4. **Two different stereo folds** (section 4.6): per-object fold ignores size and height, bed fold does not. Listening changes slightly when a master effect is added.
5. **Stereo-listen level differs from a plain session**: a centered mono track is -3 dB in 3D Stereo listening relative to a plain stereo session's balance pan.
6. **Height tilt for z < 0 on headphones** is described in the decisions log but not present in `binaural.rs`.
7. **2.0 Spatialize**: the log says it adds the decorrelated side to the fold; the code appears to only reduce `direct`.
8. **Source file channel names by count only**: 8 and 10-channel files with height layouts (5.1.2, 7.1.2) are misnamed; the WAV mask is not read.
9. **Every 3D session pays 2048 frames of extra lead** for Place Elements, even when it is unused.
10. **Comment drift**: `dsp/space.rs` module docs say playback "folds each sound to stereo"; listening can also be binaural or speakers, and folds from the bed when a master chain exists.

---

## 11. Recommended design for Audiform 2 (pure Rust)

### 11.1 Shape

Keep AF1's good ideas:
- Spots in room units, layout chosen at render time.
- Tracks and buses stay stereo; place after the chain.
- Equal-power axis-by-axis panning (simple, predictable, unit power, works for any box layout).
- Bed as the single source of truth: master chain, meters, export, binaural, speakers, and stereo listening all derive from the bed.
- Speaker feed for multichannel clips, but as a first-class graph feature instead of hidden session copies.

Change:
- Fold to stereo from the bed always (drop the per-object fold).
- Ramp placement gains per sample (or per 32 to 64 frame sub-block). Keep previous and target gain vectors per source.
- Make "speaker-pinned source" and "multichannel source" real graph node types rather than cloned tracks with derived ids. Ids then stop being a correctness concern.
- Read the WAV channel mask on import; fall back to count-based names.
- Offer both WAV-mask order and an Atmos-style bed order on export, and RF64 for files over 4 GB.
- Multichannel loudness (BS.1770 channel weights: L, R, C 1.0; surrounds 1.41; LFE excluded). The `ebur128` crate supports per-channel types.
- Only add Place Elements latency when a strip actually uses it (rebuild lead on change), or accept the fixed cost knowingly.
- Honour automation curve types in the engine.

### 11.2 Suggested modules

```
space/
  layout.rs      SpaceLayout, Speaker {name, mask, pos}, speakers(), channel_mask(), lfe_index()
  pos.rs         SpacePos (serde as in AF1), tidy(), SpaceParam
  pan.rs         share(), between(), point_gains(), sides(), source_gains()
  fold.rs        bed_fold(layout) -> Vec<[f32;2]>
  field.rs       Field, field()
  decorr.rs      Decorrelator, Delay
  steer.rs       Place Elements (realfft)
  binaural.rs    spherical head renderer
  lfe.rs         LFE band filter (biquad cascade)
engine/
  place.rs       Place (per-source gain vectors, ramp state)
  bed.rs         Bed buffer, 3D Bus strip, master-on-bed
  speaker_feed.rs multichannel source node
export/
  wav_ext.rs     extensible WAV + RF64
```

Keep the realtime rules: allocate decorrelators, steer buffers and bed buffers when the graph is built; recompute `Place` on the render thread, never the device thread; send new `Place` values through the lock-free path.

### 11.3 Ordered checklist with tests

1. **Model and serde.** `Space`, `SpaceLayout` (string names), `SpaceListen`, `SpacePos` with the defaults and skip rules in 2.2, `Bus.room`, `ChannelMix.to_speakers`, `EnvelopeTarget::Space`. Tests: round-trip AF1 JSON samples (`{"layout":"7.1.4","listen":"binaural"}`, a spot with only x/y), missing fields take defaults (width 1, y 1), `tidy` clamps and replaces NaN.
2. **Layouts.** Speaker table, orders, masks. Tests: 5.1 mask 0x3F, 7.1.4 0x2D63F, channel counts 2/6/8/8/10/10/12, LFE index 3 except 2.0.
3. **Panning.** `point_gains`, size, `sides`, `source_gains`. Tests (port AF1's): on-speaker point is 1.0 on that speaker; power 1 for all layouts at several points with size 0 and 0.6; L-C midpoint -3 dB; no-height layouts keep raised sound on floor; rotation 90 moves L front, R back; LFE never placed.
4. **Bed in the graph.** Place mono/stereo sources into a bed after fader; per-sample gain ramp. Tests: a click at x=-1,y=1 lands only on L; moving a source mid-block produces no discontinuity larger than the ramp allows; total power constant during a sweep.
5. **Fold and listening.** `bed_fold`, stereo out from the bed. Tests: L folds to [1,0], C to [0.707,0.707], LFE to [0,0].
6. **LFE send and filter.** `lfe * 0.5 * (L+R)`, 120 Hz LR8 low pass + 20 Hz 4th-order high pass on the bed's LFE channel. Tests: port `lfe_keeps_only_the_low_band` (gain at 60 Hz near 0 dB, at 1 kHz far down, at 10 Hz down).
7. **Master chain on the bed** with name-based routing and the multichannel effect wrappers (pairs, lone, linked dynamics, skip-LFE, L/R-only). Tests: port `every_built_in_effect_runs_on_a_7_1_4_bed`, linked limiter turns all speakers down together, EQ reaches every speaker.
8. **Meters per speaker** after master chain, before master fader (multiplied by master gain for display).
9. **Multichannel clips to speakers.** Name targets, missing-speaker spot fallback, LFE drop, unknown-order index mapping. Tests: 6-channel file in 7.1.4 pins L R C LFE and places Ls/Rs at the back corners; 12-channel file in 5.1 places heights by spot; stereo and opted-out clips unchanged.
10. **3D Bus.** Placed inputs into its own bed, effects on the bed, fader as per-frame gain, delay to align, add to master bed. Tests: port `a_3d_bus_runs_its_effects_on_the_whole_room` (reverb reaches every speaker but LFE); with 3D off it is a plain stereo bus.
11. **Surround plug-in path** for multichannel clips (hidden 3D Bus equivalent). Test: a room reverb on the track reaches the rear speakers from a front-left click.
12. **Spread and Spatialize** (decorrelator, field, four modes). Tests: port `copies_keep_level_but_differ`, `each_spatialize_method_feeds_its_own_speakers`, `the_matrix_delay_is_a_plain_delay`, `fields_keep_power_and_spread_where_they_should`.
13. **Place Elements** with latency compensation across all placed routes. Tests: port `place_elements_puts_each_part_where_it_sits`; a click through a steered and an unsteered track lands on the same frame; keep the ignored measurement test.
14. **Binaural.** Tests: port the three binaural tests (left ear first and louder for Ls, identical ears for C, centered impulse flat within 0.5 dB).
15. **Speakers listening and output map.** Tests: port `speakers_play_on_their_mapped_outputs` from `engine/transport/tests.rs`; default map index order.
16. **Spot automation** with per-sample (or sub-block) evaluation and curve types. Tests: port `a_space_lane_moves_the_spot`; hold curve holds; automated move is smooth.
17. **Export.** Extensible WAV with mask, PCM 16/24 with TPDF dither, float with `fact`, RF64 above 4 GB, FLAC for 2.0/5.1/7.1, optional film order. Tests: port the mixdown tests (5.1 file channel levels, 7.1.4 has 12 channels, 2.0 has 2; FLAC for 5.1/7.1 and refused for 7.1.4); header fields read back.
18. **Loudness.** Multichannel BS.1770 for beds; loudness of the fold for stereo; normalize allowed for beds if wanted (decide). Test: a 5.1 bed of pink noise on L only measures the same as stereo L only; LFE content does not change the reading.
19. **Room view UI** (section 9), automation following, snapping, stacking, track buttons, hide empty tracks.
20. **Import**: .sesx 5.1 panners (angle bend, stereo sides, automation lanes every 5 degrees). Port `five_one_panners_become_room_spots`.

---

## 12. File map

All paths relative to the AF1 repo root. Base URL: `https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/`.

| File | Lines | What it holds |
|---|---|---|
| `src-tauri/src/dsp/space.rs` | 371 | Speakers, layouts, masks, `point_gains`, `sides`, `source_gains`, `fold_gains`, tests |
| `src-tauri/src/dsp/diffuse.rs` | 954 | Decorrelator, Matrix delay, `Diffuser`, Place Elements `Steer`, `Field`/`field()`, tests |
| `src-tauri/src/dsp/binaural.rs` | 278 | Spherical head binaural renderer, tests |
| `src-tauri/src/dsp/channels.rs` | 166 | File channel names by count, default fold-down, `ChannelMix` application |
| `src-tauri/src/dsp/effects/wide.rs` | 527 | Built-in effects on beds: groups, ring, linked, skip-LFE, stereo-only, tests |
| `src-tauri/src/dsp/effects/routing.rs` | 162 | Effect channel routing by speaker name, fallbacks |
| `src-tauri/src/dsp/effects/verb.rs` | 849 | Reverb, native bed mode (`Bed`, Hadamard rows) |
| `src-tauri/src/dsp/effects/reverb.rs` | 667 | Convolution Reverb, bed upmix (`ConvBed`) |
| `src-tauri/src/dsp/effects/echo.rs` | 860 | Delay/Echo, bed rings, room spread, ping-pong round the room |
| `src-tauri/src/dsp/effects/modulation.rs` | 767 | Modulation, bed ring phase offsets |
| `src-tauri/src/dsp/effects/mod.rs` | 1167 | `ChainProcessor::with_channels` |
| `src-tauri/src/engine/graph.rs` | 1381 | Mix graph: bed, placement, 3D Bus, LFE filter, master on bed, binaural, `Mix` |
| `src-tauri/src/engine/graph/levels.rs` | 306 | `NodeLevel`, `Place`, `placed()`, `levels()` |
| `src-tauri/src/engine/graph/tests.rs` | | LFE band, 3D Bus, live level in 3D, surround plug-ins |
| `src-tauri/src/engine/speaker_feed.rs` | 498 | Multichannel clips to speakers, hidden 3D Buses, tests |
| `src-tauri/src/engine/automation.rs` | 240 | `StripAuto`, `place_at`, tests |
| `src-tauri/src/engine/plan.rs` | 1112 | `ParamLane::value_at`, routing fingerprint |
| `src-tauri/src/engine/fx.rs` | 139 | `Fx::with_channels`, `swap_fx_ch` |
| `src-tauri/src/engine/output.rs` | | `OutputMap`, device config with enough outputs |
| `src-tauri/src/engine/transport.rs` | | Speakers listen mode, bed queue, bed meters |
| `src-tauri/src/engine/transport/tests.rs` | | `speakers_play_on_their_mapped_outputs` |
| `src-tauri/src/session/model.rs` | 1464 | `Space`, `SpaceLayout`, `SpaceListen`, `SpacePos`, `SpatializeMode`, `SpaceParam`, `ChannelMix`, `SlotRouting`, `Bus.room`, `Track.speaker` |
| `src-tauri/src/session/ops/settings.rs` | | `Op::SetSpace` |
| `src-tauri/src/session/routing.rs` | | `is_routed` (3D always routed) |
| `src-tauri/src/export/mixdown.rs` | 572 | `space_out`, bed render, `mixdown_bed`, FLAC bed, normalize |
| `src-tauri/src/export/mixdown/tests.rs` | | Bed export tests |
| `src-tauri/src/export/wav_multi.rs` | 162 | Extensible WAV writer |
| `src-tauri/src/export/loudness.rs` | | Stereo BS.1770 measurement |
| `src-tauri/src/sesx/mod.rs` | | 5.1 panner import to spots and lanes |
| `src/stores/session.ts` | | TS types `SpaceDoc`, `SpacePosDoc`, `SpaceLayout`, `SpaceParam`, `set_space` op, undo merging |
| `src/stores/auto-write.ts` | | Automation writing captures spot fields |
| `src/panels/space/SpacePanel.svelte` | 777 | Room view |
| `src/panels/space/SpaceFader.svelte` | 199 | Horizontal fader |
| `src/panels/space/space.ts` | 226 | Speaker spots, view transforms, sides, snapping, toggle memory, `automatedPos` |
| `src/panels/space/space.test.ts` | 29 | Automation following tests |
| `src/panels/editor/automation.ts` | | Space lanes in the lane picker |
| `src/app/ExportDialog.svelte` | | Speakers choice, bed formats, warning, normalize disabled |
| `docs/decisions.md` | 2740 | Entries 2026-10-25 to 2026-11-06 on 3D Space (see headings listed in section 10) |
| `docs/release-notes-0.7.0.md`, `0.8.0.md`, `1.0.0.md` | | 5.1 import, surround plug-ins, known issues |

Decisions log headings most worth reading: "3D Space sessions" (2026-10-25), "3D Space: rotation, low-frequency send, lower heights, every channel metered", "3D Space: Spread, Spatialize, pair handles, faders, snapping", "3D Space automation", "Channel Routing: speakers on their own outputs", "Multichannel clips play each channel from its speaker in 3D Space", "Pass Through to Speakers, Spatialize methods", "Effect channel routing, and the master chain on a 3D Space master", "Warning on speaker-layout exports", "Built-in effects on every multichannel layout", "3D Bus", "Spatialize: Place Elements", "Place Elements and headphone listening sound closer to the source", "5.1 sessions from .sesx come in as 3D Space sessions", "Surround plug-ins hear every channel of a surround clip", "Plug-ins on surround chains are asked for the surround layout".
