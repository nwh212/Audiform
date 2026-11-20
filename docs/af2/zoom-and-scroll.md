# Zoom and scroll: handoff guide for Audiform 2

This guide lists every zoom and scroll behavior in Audiform 1, so Audiform 2 (a pure Rust rewrite) can keep the ones the project lead liked. Everything here was read from the Audiform 1 code at commit `042577e1ddee7b9718d82aea7947f9aab6edd628` (tag "Version 1.0.0"). Where the code and the docs disagree, the code wins and the guide says so. Where something is unclear, the guide says that too.

Links go to GitHub at that commit. Commits are linked as `https://github.com/nwh212/Audiform/commit/<sha>`.

**Licence.** Audiform 1 is GPL-3.0-or-later. The files quoted here carry `Copyright (c) The GeekCast Radio Network, LLC.` (for example `timeline.ts`, `spectrum.ts`, `EditorPanel.svelte`, `WaveformView.svelte`, `PianoRollDialog.svelte`, `settings.ts`, `peaks.ts`, `src-tauri/src/peaks/mod.rs`). `TrackScrollbar.svelte`, `navigation.ts` and `view-tools.svelte.ts` also carry `Copyright (c) The Audiform contributors.` Any code copied or closely ported into Audiform 2 keeps GPL-3.0-or-later and keeps the copyright lines of the file it came from. The formulas are explained so they can be reimplemented, but a port that follows the code closely is still best treated as derived work.

---

## Contents

1. The short version
2. Master table: what each input does in each area
3. Horizontal (time) zoom
4. Vertical zoom (track heights, amplitude, frequency, piano roll keys)
5. Weighting: how wheel deltas are read
6. Scroll and Follow Playhead
7. Rendering performance tied to zoom
8. History, what the project lead asked for, and bugs we hit
9. Recommended design for Audiform 2, with an ordered checklist and tests
10. File map

---

## 1. The short version

- Time zoom is one number per view: **samples per pixel** (`spp`, a float) plus a **scroll** (the session sample at the left edge). The Multitrack and the Waveform editor share these units and the same pure functions in `timeline.ts`.
- Limits: closest zoom `MIN_SPP = 1/32` (32 pixels per sample, single samples drawn as dots on a line). Farthest zoom in the Multitrack is 4 hours across the lanes, or the content plus 10% when longer. In the Waveform editor it is the whole file exactly.
- Wheel zoom keeps the sample under the pointer at the same pixel: `anchor = scroll + x*spp; scroll' = max(0, anchor - x*spp')`. The lead asked for "at the pointer" everywhere. A setting can switch the lanes and waveform to anchor on the playhead instead. Rulers always anchor on the pointer.
- One wheel event is one zoom step, whatever its size. The step is `1 + zoomStep/100` (default 25%, so x1.25 out and x0.8 in). Keyboard zoom is x2 per press.
- Vertical scroll in the Multitrack is a plain DOM scroll of a box that holds the track headers and lanes together. Horizontal scroll is virtual (the `scroll` number). This split shapes a lot of the code.
- Track heights are per track, 40 to 600 px, folded tracks 30 px. Several gestures make every open track the same height ("even zoom"). The track scrollbar on the right is a vertical zoom bar with draggable ends.
- The Waveform editor adds amplitude zoom (x1 to x16, doubling) and a spectral view with its own frequency zoom (anchored at the pointer, never past 0 Hz or Nyquist, never narrower than 40 Hz), on a scale that bends from linear to log in five steps.
- The piano roll uses a native scroll box with pixels per beat (4 to 600, default 64) and key height (6 to 36 px, default 14).
- Smoothness came from: applying wheel steps once per animation frame, never resizing a canvas unless its size changed, keeping the same view object when only the playhead moved, telling only timeline views about view changes, culling off-screen lanes, and loading finer peak levels only when the zoom needs them.
- Follow Playhead pages the view at 92% of the width. Any scroll or zoom you make while playing "lets go". It picks up again when the playhead is back in view and you have left the view alone for 400 ms (Multitrack), or when playback restarts or the playhead is moved.

---

## 2. Master table: what each input does in each area

"Step" means one zoom step per wheel event. "rAF" means the change is gathered and applied once per animation frame. Modifier priority is the order the code checks; the first match wins. Unless said otherwise, Shift+wheel uses `deltaY` and a sideways wheel uses `deltaX`.

### 2.1 Multitrack editor

The handler is `onWheel` in [EditorPanel.svelte#L701-L740](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/EditorPanel.svelte#L701-L740). It is attached to the time ruler canvas and to the vertical scroll box (`.scroller`) that holds headers and lanes. The overview strip and the track scrollbar have their own handlers. The headers and lanes sit in one vertical scroll box, so they always scroll up and down together; the overview, ruler, tempo strip and metronome row sit above it and never scroll vertically. The divider between headers and lanes (`.hresize`) drags the header column wider or narrower; double-click restores the default width.

| Area | Plain wheel | Ctrl+wheel | Shift+wheel | Alt+wheel | Alt+Shift+wheel | Sideways wheel / trackpad two-finger | Drag | Double-click |
|---|---|---|---|---|---|---|---|---|
| Lanes (clips, empty lane) | Scrolls the tracks up and down (DOM `scrollTop`, deltaMode normalized) | Time zoom, step, rAF, anchored at pointer (or playhead per setting) | Scrolls time, rAF | Track under pointer taller or shorter (x1.12 per event), commit 300 ms after the last event | Amplitude zoom of the waveforms (x2 / ÷2, 1 to 16) | Horizontal component dominant: scrolls time, rAF. Vertical: as plain wheel | Left: tool action (move, razor, select). Middle: pans time and tracks together (`startPan`). Near the top or bottom edge during a clip, box or reorder drag: auto-scrolls the tracks | With Move or Slip tool, on a clip: opens the clip's source (not a zoom) |
| Track headers (left column) | Every open track taller or shorter, evenly (same as the track scrollbar wheel), anchored at the pointer's place on the scrollbar math (see 4.1) | Nothing (no handler; see 8.3) | Nothing | Header's track taller or shorter (x1.12) | All tracks scaled by a live factor (0.2 to 8), commit after 300 ms | Horizontal dominant: nothing | Drag header to reorder; drag bottom edge to resize one track; drag an automation lane's bottom edge to resize it | Track name: rename (not a zoom) |
| Header effect slot list | Scrolls the tracks (deltaMode lines x16) and stops the event | Falls through to the header rule (nothing) | Falls through | Falls through (one track resize) | Falls through | Falls through | | |
| Time ruler | Time zoom, step, rAF, **always at the pointer** | Same | Scrolls time, rAF | Track resize code runs, but finds no track under the pointer, so nothing | Amplitude zoom | Scrolls time, rAF | Left click or drag: scrub the playhead (rAF batched seek). Drag a marker flag: move it. Middle: nothing (see 8.3) | |
| Overview strip (above ruler) | Time zoom x1.25 / x0.8, at the session spot under the pointer, **not** batched, not using the zoom step setting | Same as plain | Same as plain | Same as plain | Same as plain | Same as plain (uses `deltaY` sign only) | Drag window body: scroll. Drag window edge (5 px): zoom with the other edge fixed. Click outside: center window there, then drag | Zoom to Session (fit time) |
| Zoom Out Full button (end of overview row) | | | | | | | | Click: fit time and fit every track into the lanes' height |
| Track scrollbar (right edge, 14 px) | Every open track the same height, x1.12 per event, anchored at the track under the pointer; one undo step 350 ms after the last event | Same | Same | Same | Same | Same (uses `deltaY` sign) | Drag thumb: scroll. Drag a thumb end (within 6 px): vertical zoom, every open track the same height, other end held. Click outside thumb: jump there, then drag | Fit every open track into view at one height |
| Automation lanes (under a track) | Same as lanes | Same as lanes | Same as lanes | Nothing on the lane side: Alt+wheel looks for the closest `[data-lane]` or `[data-header]`, and automation lanes render after the track's `[data-lane]` box closes, so no track is found. Bus lanes: also nothing (buses are not tracks) | Same as lanes | Same as lanes | Draw points; drag bottom edge (in the header) to set lane height (40 to 600) | Automation lane's own double-click (points) |
| Tempo strip, metronome row | No wheel handler. They sit outside the scroll box, so a wheel there does nothing | | | | | | | |
| Transport bar zoom buttons | Zoom In / Out (x2 at view center), Zoom to Selection, "Zoom Out Full" (this button only fits time), Amplitude In / Out, Taller / Shorter Tracks (x1.25 / x0.8) | | | | | | | |

Trackpad pinch: the code never handles pinch as its own gesture. In Chromium based webviews (WebView2, and WebKitGTK behaves similarly) a pinch arrives as a wheel event with `ctrlKey` set, so it lands in the Ctrl+wheel branch and zooms time at the pointer, one step per event. This is from how webviews work, not from Audiform code or tests, so verify on each platform.

### 2.2 Waveform editor

Handler: `onWheel` in [WaveformView.svelte#L716-L735](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/WaveformView.svelte#L716-L735). Attached to the ruler, the waveform area (which includes the level scale on the right) and, through `onTimeWheel`, the spectrum body.

| Area | Plain wheel | Ctrl+wheel | Shift+wheel | Alt+wheel | Sideways wheel | Drag | Double-click |
|---|---|---|---|---|---|---|---|
| Waveform | If "wheel zooms" is on (default): time zoom, step, at pointer (or playhead per setting). Off: scrolls time | Time zoom | Scrolls time | Amplitude zoom x2 / ÷2 (1 to 16) | Scrolls time (horizontal dominant) | Select a time range, or drag a selection edge | Select All |
| Level scale (right, 46 px) | Same as waveform (the pointer x is clamped to the waveform width) | Same | Same | Same | Same | | |
| Time ruler | Time zoom, **always at the pointer, even with "wheel zooms" off** | Same | Same as plain (zoom) | Amplitude zoom | Zooms (see bug note 8.3: `deltaY` of 0 reads as zoom in) | Click or drag: move the playhead (seek) | |
| Spectrum body | Same as waveform | Same | Same | Same (amplitude of the waveform, not the spectrum) | Same | Spectral tools (box, lasso, brush, spot) or time select | |
| Frequency scale (right of spectrum) | Frequency zoom x1.25 / x0.8 around the frequency under the pointer | Same (modifiers ignored) | Same | Same | Same | Drag up or down: pan through frequencies at the same zoom (only when zoomed) | |
| Horizontal scrollbar thumb | | | | | | Drag: scroll time | |
| Divider between waveform and spectrum | | | | | | Drag: share of height, 0.12 to 0.88 | |

Right-click on the spectrum: Zoom In (Frequency) x0.5, Zoom Out (Frequency) x2, Zoom Reset (Frequency), Hertz or Notes, More Logarithmic, More Linear, Full Logarithmic, Full Linear, spectral resolution, colors.

### 2.3 Piano roll (MIDI clip editor dialog)

Handlers: `onRollWheel` and `onRulerWheel` in [PianoRollDialog.svelte#L295-L319](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/PianoRollDialog.svelte#L295-L319). The roll is a native scroll box. The keys column is `position: sticky; left: 0`, the bar ruler is sticky at the top, the velocity or controller lane is sticky at the bottom. So all three areas are inside the same scroll box and get the same wheel handler, except the ruler.

| Area | Plain wheel | Ctrl or Cmd+wheel | Shift+wheel | Alt+wheel | Sideways wheel | Drag | Double-click |
|---|---|---|---|---|---|---|---|
| Note grid | Native browser scroll (up and down the keys) | Time zoom x1.2 per event, at the pointer | `scrollLeft += deltaY \|\| deltaX` (raw) | Key height ±2 px, at the pointer | Native horizontal scroll | Left: draw, select, erase, move or stretch notes. Middle: pan both ways | Delete note |
| Keys column | Same as grid | Same (anchor x clamped to 0) | Same | Same | Same | Click a key: audition | |
| Velocity / pan / controller lane | Same as grid | Same | Same | Same | Same | Drag bars or draw controller moves | |
| Bar ruler | Time zoom x1.2 at the pointer (stops propagation) | Same | Scrolls time | Zoom (Alt ignored) | Zoom | | |
| Zoom bar (under the roll) | No wheel handler | | | | | Drag thumb: scroll. Drag an end: zoom, other end held. Click beside: jump, centered | Fit the clip |
| Header buttons | − / + time zoom x1.5, Fit, Shorter / Taller keys (±2 px) | | | | | | |

There is no follow during the piano roll's own playback. A play line moves, but the view does not scroll to it.

### 2.4 Other panels

None of these zoom a view. They use the wheel to change values. Listed so Audiform 2 knows nothing was missed.

| Panel | Wheel | Notes |
|---|---|---|
| Parametric EQ graph (`EqGraph.svelte`) | Over a node: band width x1.15 per event, commit 350 ms after the last | Graph range is a toggle, ±15 dB (30) or ±48 dB (96), saved in localStorage. No pan or zoom of the axes. Frequency axis is fixed 20 Hz to 20 kHz log |
| Noise shape graph | Over a node: width x1.15, commit after 350 ms | |
| Band EQ sliders | ±0.5 dB per event (Ctrl 0.1, Shift 3), commit after the wheel stops | |
| VSlider, ValueField, BigKnob | 0.01 of travel per event (Ctrl 0.002, Shift 0.05), commit after the wheel stops | |
| Mixer faders | dB steps, clamped −60 to +12 | |
| Time Info fields | Moves the time by `step` x 10 per event (Ctrl x1, Shift x100). `step` is the editor's samples per pixel, so one pixel of drag is one pixel of timeline | Editing the visible range's start or end sets the Multitrack view (`viewFor`, min spp 1/64, see 8.3) |
| 3D Space room | Wheel over a dot raises or lowers it; Shift+wheel turns a stereo pair | The room view scales to the panel size (`size = max(160, ...)`), no zoom |
| 3D Space faders | One step per event | |
| Phase Wheel | No wheel. Its size follows the panel; details hide when the panel is under 300 x 220 | |
| Levels | No wheel. Many channels in a short panel go into columns; below 11 px a row they go "tight" (bars only) | |

---

## 3. Horizontal (time) zoom

### 3.1 Units and the core functions

The view type is shared by both editors ([timeline.ts#L12-L17](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/timeline.ts#L12-L17)):

```ts
export interface Viewport {
  /** Session samples per screen pixel. */
  spp: number;
  /** Session sample at the left edge. */
  scroll: number;
}

export const MIN_SPP = 1 / 32;      // 32 px per sample
export const SAMPLE_SPP = MIN_SPP;  // same in the Waveform editor
export const MAX_SPP = 1_048_576;   // about 6 h across 1,000 px at 48 kHz

export function xOf(sample: number, v: Viewport): number {
  return (sample - v.scroll) / v.spp;
}
export function sampleAt(x: number, v: Viewport): number {
  return Math.max(0, Math.round(v.scroll + x * v.spp));
}
```

The saved form is `ViewDoc` / Rust `SessionView` ([model.rs#L1414-L1438](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/session/model.rs#L1414-L1438)): `samples_per_pixel: f64` (default 1024), `scroll: u64`, `scroll_y: u32`, `playhead: u64`, `selection`. Note that `scroll` is a whole sample. `setView` rounds it (`Math.round`). At 32 px per sample that rounding moves the left edge up to half a sample, which is 16 px. `scroll_y` is saved but the interface never writes it (always 0); vertical scroll is not restored on open.

When the session sample rate changes, the core rescales `scroll`, `playhead`, `selection` and multiplies `samples_per_pixel` by `to/from`, so the same time stays in view ([ops/settings.rs#L33-L42](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/session/ops/settings.rs#L33-L42)).

### 3.2 Limits

| View | Closest (min spp) | Farthest (max spp) |
|---|---|---|
| Multitrack, wheel and keys | `MIN_SPP` = 1/32 | `maxSppFor(width, contentEnd, rate)` |
| Multitrack, overview wheel and edge drag | `MIN_SPP` | `MAX_SPP` = 1,048,576 (not `maxSppFor`, see 8.3) |
| Multitrack, `fit` (Zoom to Session, Selection) | `MIN_SPP` | `MAX_SPP` |
| Waveform editor | `SAMPLE_SPP` = 1/32 | `outermost() = fit(total, width, 1).spp`: the file fills the view exactly |
| Time Info "view" fields | 1/64 (`viewFor`) | none |

`maxSppFor` ([timeline.ts#L37-L45](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/timeline.ts#L37-L45)):

```ts
export const DEFAULT_MAX_SECONDS = 4 * 3600;
export function maxSppFor(width: number, contentEnd: number, rate: number): number {
  const span = Math.max(DEFAULT_MAX_SECONDS * rate, contentEnd * 1.1);
  return Math.min(MAX_SPP * 8, Math.max(MIN_SPP, span / Math.max(1, width)));
}
```

So the Multitrack zooms out to 4 hours, or to the content plus 10% when the content is longer, capped at `MAX_SPP * 8`. Scroll never goes below 0. In the Multitrack scroll has **no upper limit**: you can scroll past the content. In the Waveform editor `setView` clamps `scroll` to `total - width*spp`, so the view never runs past the end of the file ([WaveformView.svelte#L172-L190](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/WaveformView.svelte#L172-L190)).

### 3.3 Anchoring: the formula

`zoomAround` ([timeline.ts#L47-L57](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/timeline.ts#L47-L57)) keeps the sample under `anchorX` at the same pixel:

```ts
export function zoomAround(v: Viewport, factor: number, anchorX: number,
                           maxSpp = MAX_SPP, minSpp = MIN_SPP): Viewport {
  const spp = Math.min(maxSpp, Math.max(minSpp, v.spp * factor));
  const anchor = v.scroll + anchorX * v.spp;
  return { spp, scroll: Math.max(0, anchor - anchorX * spp) };
}
```

In words: `anchor = scroll + x * spp`, then `scroll' = max(0, anchor - x * spp')`. The clamp happens on `spp` first, so at a limit the step is smaller and the anchor still holds. The `max(0, ...)` clamp breaks the anchor near the session start, on purpose (you cannot see before 0). Factor above 1 zooms out.

`zoomSteps` ([timeline.ts#L59-L76](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/timeline.ts#L59-L76)) applies `n` gathered steps at once as `factor ** n`. A test proves this equals `n` single steps (same spp, same scroll, same anchor sample) ([timeline.test.ts#L43-L52](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/timeline.test.ts#L43-L52)). This is only exact away from the clamps: if the sum crosses a limit, the per-step version would have stopped earlier. The difference does not matter in practice.

### 3.4 Where each time zoom anchors

| Gesture | Anchor |
|---|---|
| Wheel over the Multitrack lanes (Ctrl, or plain over the ruler) | `wheelAnchorX(pointerX, width, playheadX, overRuler)`: pointer, unless Settings says playhead; ruler always pointer |
| Wheel over the Waveform editor (plain with "wheel zooms", Ctrl, ruler) | Same rule |
| Overview strip wheel | The session sample under the pointer **on the strip**, mapped into the lanes: `sample = stripX/stripW * total; anchorX = clamp((sample - scroll)/spp, 0, lanesW)`. If that sample is off screen the anchor sits at the lanes' left or right edge |
| Multitrack keyboard and transport Zoom In / Out | Middle of the lanes (`w / 2`), x2 per press ([navigation.ts#L21-L27](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/app/navigation.ts#L21-L27)) |
| Waveform keyboard and transport Zoom In / Out | The playhead, clamped into view (`clamp(xOf(playhead), 0, width)`), x2 per press ([WaveformView.svelte#L243-L258](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/WaveformView.svelte#L243-L258)) |
| Overview edge drag | The opposite edge stays put |
| Piano roll | Pointer for wheel; middle of the view for buttons; opposite end for zoom bar ends |

The anchor setting ([settings.ts#L383-L402](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/stores/settings.ts#L383-L402)):

```ts
export function wheelAnchorX(mouse: number, width: number, playheadX: number, exact = false): number {
  if (exact || settings.current.wheelAnchor !== "playhead") return mouse;
  return Math.max(0, Math.min(width, playheadX));
}

export function zoomRatios(step = settings.current.zoomStep) {
  const out = 1 + step / 100;
  return { zoomIn: 1 / out, zoomOut: out };
}
```

Settings > General: "Each wheel step zooms by N %" (5 to 100, step 5, default 25), "Wheel zoom centers on: the mouse pointer (as over the time ruler) / the playhead" (default pointer), and "wheel zooms" (default on, now Waveform editor only). A playhead off screen is clamped to the nearest edge, so "playhead" mode zooms at the edge in that case.

A third anchor, "between the pointer and the middle", existed from 2026-10-25 to 2026-11-02. It used `mouse + (width/2 - mouse) * 0.4`. The lead said it felt wrong and drifted; it was removed and a saved "between" now reads as pointer ([settings.ts#L232-L233](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/stores/settings.ts#L232-L233), commit [37885f6](https://github.com/nwh212/Audiform/commit/37885f6)). Do not bring it back.

### 3.5 Step factors per source

| Source | Factor per event or press | Uses the zoomStep setting? |
|---|---|---|
| Multitrack wheel (lanes with Ctrl, ruler) | `1 + zoomStep/100` (default 1.25 out, 0.8 in) | Yes |
| Waveform wheel | Same | Yes |
| Overview strip wheel | 1.25 / 0.8 | No |
| Keyboard `=`, `-`, `Ctrl+=`, `Ctrl+-`, transport buttons | 0.5 / 2 | No |
| Piano roll wheel (Ctrl, ruler) | 1.2 / (1/1.2) | No |
| Piano roll buttons | 1.5 / (1/1.5) | No |
| Spectrum frequency wheel | 1.25 / 0.8 on the span | No |
| Spectrum menu Zoom In / Out (Frequency) | 0.5 / 2 | No |

Held zoom keys repeat: `view.zoom-in` and `view.zoom-out` are in `REPEATABLE` in [dispatch.ts#L14](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/keymap/dispatch.ts#L14). Every other action fires once per press.

### 3.6 Fit, Zoom to Selection, Zoom Out Full

`fit` ([timeline.ts#L78-L87](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/timeline.ts#L78-L87)):

```ts
export function fit(length: number, width: number, room = 1.05): Viewport {
  if (length <= 0 || width <= 0) return { spp: 1024, scroll: 0 };
  return { spp: Math.min(MAX_SPP, Math.max(MIN_SPP, (length * room) / width)), scroll: 0 };
}
```

| Command | Multitrack | Waveform editor |
|---|---|---|
| Zoom to Session (`view.zoom-session`, key `\`, overview double-click, transport "Zoom Out Full" button) | `fit(contentEnd, width)` with 5% room after the end, scroll 0 ([view-tools.svelte.ts#L72-L82](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/app/view-tools.svelte.ts#L72-L82)). Disabled when there are no clips | `fit(total, width, 1)`: exactly the file, scroll 0 |
| Zoom to Selection (`view.zoom-selection`, `Shift+/` and `?`, transport) | `fit(span, w)` (5% room), scroll `start - span*0.025`, so about 2.5% margin each side ([view-tools.svelte.ts#L165-L177](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/app/view-tools.svelte.ts#L165-L177)) | `fit(span, width)` (5% room), scroll `start - span*0.05`: 5% before, about 0% after |
| Zoom Out Full (`view.zoom-out-full`, `Shift+\` and `\|`, button at the end of the overview row) | Every listed track set to `floor((scrollerHeight - 8) / trackCount)`, clamped 40 to 600, and unfolded, as one undo step; then Zoom to Session ([EditorPanel.svelte#L1657-L1668](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/EditorPanel.svelte#L1657-L1668)). Time zoom runs after the edit so the edit's result does not replace the new view | The action is registered by the Multitrack panel, so it resizes the (hidden) Multitrack tracks; its time part calls `view.zoom-session`, which the Waveform editor overrides to fit the file. Unclear if intended |
| Zoom Reset (All Axes) (`view.zoom-all-axes`, `Ctrl+\`) | Not built. Menu item shown disabled, "Not built yet." | Same |
| Zoom In Full | Does not exist. The approved shortcut table in `CLAUDE.md` names "Zoom reset, zoom in full, zoom out full" for `\`, `Shift+/`, `Shift+\`, `Ctrl+\`, but the code maps `Shift+/` to Zoom to Selection | |

Opening a file in the Waveform editor fits it. Because the panel can still be growing right after switching from the Multitrack, it fits again on every width change until you zoom or scroll ([WaveformView.svelte#L192-L207](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/WaveformView.svelte#L192-L207)). If the file gets shorter while the whole of it was in view (scroll 0 and spp at least 95% of the old fit), it fits again ([WaveformView.svelte#L209-L222](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/WaveformView.svelte#L209-L222)). Opening or importing files into an empty session also runs Zoom to Session.

### 3.7 Ruler ticks that add as you zoom

`rulerTicks` ([timeline.ts#L88-L141](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/timeline.ts#L88-L141)) picks the first labeled step (seconds) from a fixed list whose pixel gap is at least `minGap = 90`:

```
0.0001 0.0002 0.0005 0.001 0.002 0.005 0.01 0.02 0.05 0.1 0.2 0.5
1 2 5 10 15 20 30 60 120 180 300 600 900 1200 1800 3600 7200   (else 14400)
```

Unlabeled minor ticks divide each step by 5, or by 3 or 4 where 5 would land on odd times (`MINOR_DIVISIONS`). Ticks are generated by integer index (`k * minor`) to avoid float drift. Labels show only the decimals the step needs (`formatTime`). In Bars and Beats, `barTicks` picks from `[0.25, 0.5, 1, bar, 2 bars, ... 64 bars]` beats with `minGap = 70`, across tempo and time signature changes. `gridLines` draws bar lines and beat lines only when beats are at least 6 px apart, otherwise every 2^k bars. The Waveform editor draws a faint vertical line at each labeled tick.

---

## 4. Vertical zoom

### 4.1 Multitrack track heights

Limits and constants:

| Name | Value | Where |
|---|---|---|
| Track height range | 40 to 600 px | inline clamps in EditorPanel |
| Folded track | `COLLAPSED_H = 30` (name row only) | [timeline.ts#L421-L430](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/timeline.ts#L421-L430) |
| Gap under each lane | `TRACK_GAP = 3` | timeline.ts |
| Automation lane | own height, else half its track's, clamped `LANE_MIN = 40` to `LANE_MAX = 600`; `AUTO_H = 54` when the owner is unknown | [automation.ts#L16-L22](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/automation.ts#L16-L22) |
| Wheel step | x1.12 or ÷1.12 per event | EditorPanel |
| Keyboard / transport step | x1.25 (`Alt+=`, Taller Tracks), x0.8 (`Alt+-`, Shorter Tracks) | [EditorPanel.svelte#L1670-L1688](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/EditorPanel.svelte#L1670-L1688) |

Heights live in the document (`track.height`, `track.collapsed`), so every committed change is an undo step labeled "Track Height". During a gesture a preview is shown, then one edit is committed. The shown height is ([EditorPanel.svelte#L1632-L1634](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/EditorPanel.svelte#L1632-L1634)):

```ts
const shownHeight = (t) =>
  resizing?.id === t.id ? resizing.h               // one track being resized
  : t.collapsed ? trackHeight(t)                    // folded: 30
  : uniformPreview !== null ? uniformPreview        // every open track the same
  : heightPreview !== 1 ? clamp(40, 600, round(t.height * heightPreview))  // all scaled
  : t.height;
```

The gestures:

1. **Proportional, all tracks** (`resizeTracks(factor)`): `Alt+=`, `Alt+-`, Taller / Shorter Tracks buttons. Each track keeps its own height times the factor, clamped. Folded tracks keep their stored height (they are included in the list, but stay folded).
2. **Even, all open tracks** (`onVbarWheel`): wheel over the track scrollbar or plain wheel over the headers. Every open (not folded) track gets the same height: `h = clamp(40, 600, round(cur * 1.12^±1))`, where `cur` is the live preview or the first open track's height. So the first wheel event over mixed heights snaps them all to one height. Committed as one batch 350 ms after the last event ([EditorPanel.svelte#L750-L792](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/EditorPanel.svelte#L750-L792)).
3. **Even, by dragging a thumb end** on the track scrollbar (`onVbarPointer`): the thumb covers the share of the content that fits the view. Dragging an end changes that share, and the track height is solved from it:
   ```ts
   const content = sum of header row heights;               // everything in the rows
   const fixed = content - sum(open tracks' shown heights);  // folded tracks, buses, automation lanes
   share = clamp(max(8, span0 + d) / barHeight, ..., 1);
   h = clamp(40, 600, round((scrollerH / share - fixed) / open.length));
   ```
   On release, one batch edit, then `scrollTop` is set so the end you did not drag stays where it was ([EditorPanel.svelte#L1555-L1613](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/EditorPanel.svelte#L1555-L1613)).
4. **Fit all open tracks** (double-click the track scrollbar, `fitAllTracks`): `h = clamp(40, 600, floor((scrollerH - fixed) / open.length))`, accounting for folded tracks, buses and lanes.
5. **Zoom Out Full**: `h = floor((scrollerH - 8) / tracks.length)` over all listed tracks, unfolds them. This one does **not** subtract buses or automation lanes, so with those showing the tracks do not quite fit.
6. **One track by its bottom edge** (`startResize`): live height `h0 + dy`, clamped, one edit on release.
7. **One track by Alt+wheel** over its header or lane (`wheelResizeTracks`): x1.12 per event, one edit 300 ms after the last; it also unfolds the track.
8. **All tracks scaled by Alt+Shift+wheel over a header**: live `heightPreview *= 1.12^±1` (0.2 to 8), committed through `resizeTracks(f)` after 300 ms.
9. **Fold / unfold** with the header arrow: 30 px.
10. **One automation lane by its bottom edge** (`resizeLane`), 40 to 600, one "Lane Height" edit. Lanes without their own height follow half their track's height, so even zoom changes them too; lanes with a set height do not scale.

**Anchoring of even wheel zoom.** It keeps the session spot under the pointer, read from the pointer's place on the scrollbar ([EditorPanel.svelte#L764-L774](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/EditorPanel.svelte#L764-L774)):

```ts
const share  = clamp01((e.clientY - bar.top) / bar.height);   // where on the bar
const spot   = share * sc.scrollHeight;                         // that place in the rows
const offset = clamp(0, sc.clientHeight, spot - sc.scrollTop);  // its pixel in the view
uniformPreview = h;
await nextFrame();
sc.scrollTop = share * sc.scrollHeight - offset;                // same share, same pixel
```

When the wheel is over the headers, `e.currentTarget` is the scroll box, not the scrollbar. The same formula then reads the pointer's y within the visible box as a share of the whole bar height, which is not the same as "the track under the pointer". Observed from the code, not tested; treat as approximate.

The other vertical gestures (proportional keys, Alt+wheel one track, Zoom Out Full) do not adjust `scrollTop`, so the content above the pointer moves.

**The track scrollbar** ([TrackScrollbar.svelte](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/TrackScrollbar.svelte)) draws a colored segment per listed track, sized by `shownHeight + lanesHeight` over the rows' height, and a thumb at `scrollTop / rowsH`, height `scrollerH / rowsH`. The thumb is always shown (full height when everything fits). Grips at both ends. The pointer turns into a resize cursor within 7 px of an end; a press counts as an end within 6 px inside or 2 px outside. Buses are in the rows but have no colored segment.

### 4.2 Amplitude zoom (display only)

Both editors have `ampZoom`, 1 to 16, doubled or halved per step (`amp-in` / `amp-out`). It scales the drawn waveform only, never playback.

- Multitrack: Alt+Shift+wheel over the lanes or ruler, or the transport buttons. Not saved; resets when the panel remounts.
- Waveform editor: Alt+wheel anywhere on the waveform, ruler or spectrum body, or the transport buttons.
- In the Waveform editor the level grid and scale use `MARKS = [0, -3, -6, -9, -12, -18, -24]` dB. Each mark's height is `10^(dB/20) * ampZoom`; marks above full scale are dropped, and labels closer than 11 px are skipped. So zooming in shows quieter marks with more room. The zoom is centered on the zero line of each channel; there is no anchor at the pointer and no vertical scroll ([WaveformView.svelte#L372-L387](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/WaveformView.svelte#L372-L387), [#L565-L582](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/WaveformView.svelte#L565-L582)).
- Drawing clamps each sample to ±1 after scaling, so loud parts flatten at the lane edge.

### 4.3 Spectral view frequency zoom

All in [spectrum.ts](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/spectrum.ts), unit-tested in [spectrum.test.ts](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/spectrum.test.ts). Two axes exist on purpose:

- **The edits' axis** (fixed): log from `MIN_HZ = 30` to Nyquist. `freqAt(frac)` and `fracOf(hz)`. Selections (box, lasso, brush) are stored on this axis, so they stay on the same sounds whatever the view does. It must match `src-tauri/src/dsp/spectral.rs`.
- **The view's axis**: `FreqView { lo, hi (null = Nyquist), log }`. `log` runs 0 (linear in Hz) to 1 (log, even in pitch), in menu steps `[0, 0.25, 0.5, 0.75, 1]`.

The view opens as `FULL_VIEW = { lo: 0, hi: null, log: 0.25 }`, a quarter of the way to log, so 1 kHz sits about an eighth of the way up and 10 kHz about two thirds. The state lives in a store for the run of the app, across files ([spectrum-view.ts](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/stores/spectrum-view.ts)).

**The bent scale** ([spectrum.ts#L60-L75](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/spectrum.ts#L60-L75)):

```ts
const LOG_FLOOR = 10;
export function viewFreq(frac: number, v: FreqView, nyquist: number): number {
  const [lo, hi] = viewRange(v, nyquist);
  const t = clamp01(frac);
  const k = clamp01(v.log);
  const l0 = Math.max(lo, LOG_FLOOR);
  const top = Math.max(hi, l0 + 1);
  const a = k * Math.log(top / l0);       // bend strength
  const start = lo + (l0 - lo) * k;
  if (a < 1e-6) return start + (top - start) * t;               // linear
  return start + ((top - start) * Math.expm1(a * t)) / Math.expm1(a);
}
```

At `k = 1` this is a true log scale from `l0` to `top`. At `k = 0` it is linear from `lo`. In between, each step moves the middle of the view by a similar number of octaves (the reason for this `expm1` form, from commit [fb81c36](https://github.com/nwh212/Audiform/commit/fb81c36); the earlier blend stayed mostly linear until the end). The inverse `viewFrac` uses 40 rounds of bisection, since the curve only rises. The core gets the row edges in Hz (`rowEdges(rows, v, nyquist)`) and renders the spectrogram on exactly that scale.

**Zoom anchored at the pointer** ([spectrum.ts#L97-L131](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/spectrum.ts#L97-L131)):

```ts
export const MIN_SPAN = 40; // Hz
export function zoomFreq(v, hz, factor, nyquist) {
  const [lo, hi] = viewRange(v, nyquist);
  const at = clamp(lo, hi, hz);
  if (v.log >= 0.5) {                      // pitch-like: zoom in ratios
    const l0 = Math.max(lo, LOG_FLOOR);
    const a = Math.max(at, l0);
    nlo = a * Math.pow(l0 / a, factor);
    nhi = a * Math.pow(hi / a, factor);
    if (lo < LOG_FLOOR && factor >= 1) nlo = Math.min(nlo, lo);
  } else {                                 // Hz-like: zoom linearly
    nlo = at - (at - lo) * factor;
    nhi = at + (hi - at) * factor;
  }
  if (nhi - nlo < MIN_SPAN) { /* center a 40 Hz window, kept inside 0..Nyquist */ }
  nlo = Math.max(v.log >= 0.5 ? Math.min(MIN_HZ, nlo) : 0, nlo);
  nhi = Math.min(nyquist, nhi);
  // Zoomed all the way out: back to the full range (hi = null).
}
```

The pointer frequency is read through the view scale: `hz = viewFreq(1 - yInBand / bandHeight, sv, nyquist)`. Factor 0.8 per wheel event in, 1.25 out. The top never passes Nyquist of the file's rate; the bottom never passes 0 Hz (30 Hz floor on pitch-like scales). The test checks the anchor holds to two decimals in fraction terms, 60 zoom-ins never go below 40 Hz, and 60 zoom-outs come back to `hi = null`.

**Pan by dragging the scale** (`panFreq`, [spectrum.ts#L133-L156](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/spectrum.ts#L133-L156)): only when zoomed. `d` is pointer movement as a share of the band height, down is positive and brings higher frequencies in, "as if pulling the scale". Linear: shift by `(hi - lo) * d`. Pitch-like: multiply both ends by `(hi/l0)^d`. Stops at the floor and at Nyquist. Cursor is a hand over the scale when there is somewhere to go.

**Changing the scale** keeps the range, except that going from linear-ish (below 0.5) to pitch-like with the full range shown moves `lo` from 0 to 30, and back ([SpectrumView.svelte#L201-L215](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/SpectrumView.svelte#L201-L215)).

**Ruler marks that add as you zoom** (`freqTicks`, [spectrum.ts#L158-L190](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/spectrum.ts#L158-L190)), `minGap = 16` px:

- Near linear (`log < 0.02`): even steps of 1, 2 or 5 x 10^n, about one per `2 * minGap` px, then thinned so labels are at least 16 px apart.
- Any bend: go through round steps from coarse to fine (`5, 2, 1` x 10^e, e from `ceil(log10(hi))` down to −2, skipping a step that would make over 4,000 marks). Add a mark only if the whole step fits there (distance to `f ± step` at least 16 px) and it is at least 16 px from every mark already kept. So the low end, which a log scale packs, gets sparse round marks, and zooming in keeps adding round-numbered marks.
- Note names mode: every C, or every note when B4 to C5 are at least 14 px apart, each at least 14 px from the last.
- Labels: `1.5k` style above 1 kHz; one decimal below 100 Hz.

Multichannel files show one band per channel. Zoom, pan and tools work inside a band; a drag stays in the band it started in.

### 4.4 Piano roll key height

`row` (px per key), `ROW_MIN = 6`, `ROW_MAX = 36`, default 14, kept across dialog opens in a module-level `lastRow` for the run of the app. Alt+wheel adds or removes 2 px per event (additive, not a factor), at the key under the pointer ([PianoRollDialog.svelte#L284-L293](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/PianoRollDialog.svelte#L284-L293)):

```ts
const key = (scroller.scrollTop + py - RULER_H) / row;   // fractional key under the pointer
row = r;
await tick();                                            // let the DOM grow
scroller.scrollTop = key * r - py + RULER_H;
```

Key names show only on C keys and only when `row >= 9`. On open, the roll scrolls so the middle of the clip's pitch range (or middle C) is centered, then fits time.

---

## 5. Weighting: how wheel deltas are read

This is the weakest part of Audiform 1 and the first thing to improve in Audiform 2.

### 5.1 Zoom uses only the sign

Every zoom handler reads only `Math.sign(deltaY)` (or `deltaY || deltaX`): one event is one step, whatever its size.

```ts
queueWheel(e.deltaY > 0 ? 1 : -1, 0, x, overRuler);   // Multitrack
zoomAround(view, e.deltaY > 0 ? r.zoomOut : r.zoomIn, at, ...); // Waveform
```

Effects:

- A classic mouse notch (Windows WebView2 reports about 100 px per notch) gives one 25% step. Good.
- A trackpad or a high-resolution wheel sends many small events per gesture, each a full step. Zoom is very fast and hard to control there. The rAF batching (5.3) does not change the total; it only lands the steps in one frame.
- `deltaY === 0` reads as "zoom in" wherever the code uses `deltaY > 0 ? out : in`. In the Multitrack the horizontal-dominant check runs first, so this cannot happen there. It can in the Waveform ruler and the overview strip (see 8.3).

There is no acceleration, no inertia handling, no smoothing animation, and no fractional zoom.

### 5.2 Scroll uses the raw value, with partial deltaMode handling

| Scroll | Unit handling |
|---|---|
| Multitrack tracks, plain wheel (`scrollTracks`) | `deltaMode` 1 (lines) x40 px, 2 (pages) x `clientHeight`, 0 (pixels) x1 ([EditorPanel.svelte#L742-L748](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/EditorPanel.svelte#L742-L748)) |
| Header effect slots | lines x16, else raw pixels |
| Multitrack time, Shift or sideways | raw delta in pixels, times `spp`; deltaMode ignored |
| Waveform time | raw delta times `spp`; deltaMode ignored |
| Piano roll Shift+wheel | raw `deltaY \|\| deltaX` added to `scrollLeft` |
| Piano roll plain wheel, sideways | native browser scrolling |

Line height is 40 px in one place and 16 px in another. In practice Chromium webviews almost always send pixels, so this rarely shows. Audiform 2 should normalize once.

The Multitrack plain wheel scrolls the tracks in code, not natively, because in the app the native scroll "did nothing" ([decisions.md, 2026-11-06](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/docs/decisions.md#L2668-L2671)). The scroll box hides its native scrollbar (`scrollbar-width: none`); the track scrollbar replaces it.

### 5.3 Frame batching

Only the Multitrack batches ([EditorPanel.svelte#L671-L699](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/EditorPanel.svelte#L671-L699)):

```ts
let wheelQueue: { steps: number; scrollPx: number; x: number; overRuler: boolean } | null = null;
function queueWheel(steps, scrollPx, x, overRuler) {
  if (wheelQueue) {                    // already waiting for a frame: add to it
    wheelQueue.steps += steps;
    wheelQueue.scrollPx += scrollPx;
    wheelQueue.x = x;                  // latest pointer wins
    wheelQueue.overRuler ||= overRuler;
    return;
  }
  wheelQueue = { steps, scrollPx, x, overRuler };
  requestAnimationFrame(() => {
    const q = wheelQueue; wheelQueue = null;
    let v = { spp: view.spp, scroll: view.scroll + q.scrollPx * view.spp };  // scroll first
    if (q.steps !== 0) {
      const at = wheelAnchorX(q.x, lanesEl.clientWidth, xOf(playhead, v), q.overRuler);
      v = zoomSteps(v, q.steps > 0 ? r.zoomOut : r.zoomIn, Math.abs(q.steps), at,
                    maxSppFor(lanesWidth, contentEnd, rate));
    }
    setView({ spp: v.spp, scroll: v.scroll });
  });
}
```

Note: opposite steps in one frame cancel (net steps). Scroll is applied before zoom. Not batched: the overview strip wheel, the Waveform editor wheel, the spectrum frequency wheel, the piano roll, track height wheels (those are cheap previews with a debounced commit instead).

### 5.4 Debounced commits

Gestures that change the document (heights, EQ width, values) preview live and commit one undoable edit after the wheel goes quiet: 300 ms (`wheelResizeTracks`), 350 ms (`onVbarWheel`, EQ and noise shape widths), and similar for value controls. View changes (zoom, scroll) are not edits and are never undoable. They reach the core 300 ms after the last change (`setView` timer, [session.ts#L876-L896](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/stores/session.ts#L876-L896)), with `flushView()` when playback needs the selection now.

### 5.5 Per-section sensitivity summary

| Section | What one wheel event does |
|---|---|
| Multitrack time zoom | x(1 + zoomStep%) (default 1.25) |
| Overview time zoom | x1.25 |
| Waveform time zoom | x(1 + zoomStep%) |
| Piano roll time zoom | x1.2 |
| Spectrum frequency zoom | span x1.25 |
| Track height (wheel) | x1.12 |
| Key height | ±2 px |
| Amplitude | x2 |
| Multitrack track scroll | delta px (lines x40, pages x view height) |
| Time scroll (both editors) | delta px worth of time |

---

## 6. Scroll and Follow Playhead

### 6.1 Multitrack

Follow Playhead is an editor toggle (on by default, saved with editor settings). The rule, from [EditorPanel.svelte#L370-L422](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/EditorPanel.svelte#L370-L422):

```ts
let followHeld = false;   // the user has taken the view
let userViewAt = 0;       // ms of the last user view change

// on every playhead or view change while playing with Follow on:
const x = xOf(playhead, view);
if (followHeld) {
  if (x >= 0 && x <= lanesWidth && performance.now() - userViewAt > 400) followHeld = false;
  return;
}
if (x > lanesWidth * 0.92 || x < 0) setView({ scroll: playhead - lanesWidth * 0.08 * view.spp }, true);

function setView(v, auto = false) {
  if (!auto && playing) {
    if (v.playhead !== undefined) followHeld = false;           // moving the playhead resumes
    else if (scroll or spp changed) { followHeld = true; userViewAt = performance.now(); }
  }
  ...
}
```

So:

- Paging: when the playhead passes 92% of the width (or is off the left), the view jumps so the playhead sits at 8%. It is a page jump, not smooth scrolling.
- Any user scroll or zoom while playing (wheel, overview drag, overview wheel, middle-drag pan, keyboard zoom through `setView` in the panel) sets `followHeld`.
- Following resumes when: the playhead is in view and the user has not moved the view for 400 ms; playback starts; the playhead is moved; Follow Playhead is switched on. The last one is only on the switch itself: an earlier version re-armed it on any editor state change, which pulled an overview drag straight back ([decisions.md#L1390](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/docs/decisions.md#L1387-L1391)).
- Caveat: keyboard zoom (`zoomBy`), Zoom to Session and Zoom to Selection call `session.setView` directly, not the panel's `setView`, so they do not set `followHeld`. If such a zoom leaves the playhead past 92%, the follow pages at once.
- Clicking in the lanes while playing does not move the playhead; only the ruler does.

### 6.2 Waveform editor

Simpler and stricter ([WaveformView.svelte#L224-L237](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/WaveformView.svelte#L224-L237)): it always follows (no toggle). It pages when the playhead passes `width - 20` px (or off the left), placing it 20 px from the left. A user scroll or zoom while playing holds the follow until playback starts again or the playhead is moved. There is no 400 ms "back in view" resume here.

### 6.3 Store rule that prevents a follow loop

`setView` with a view equal to the current one returns without telling anyone ([session.ts#L883-L897](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/stores/session.ts#L883-L897), `sameView` at [#L618-L628](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/stores/session.ts#L618-L628)). Without it, the follow at the end of a file (where it cannot scroll further) set the same view forever and froze the window. Audiform 2 must keep "no change, no notify".

### 6.4 Scrubbing and dragging the playhead

Ruler click or drag shows the line at the pointer at once (`dragHead`) and seeks at most once per frame while stopped; while playing, playback jumps there on release ([EditorPanel.svelte#L583-L622](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/EditorPanel.svelte#L583-L622)). The Waveform ruler seeks on every move while stopped.

### 6.5 Auto-scroll during drags

`edgeScrollY` ([EditorPanel.svelte#L1093-L1114](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/EditorPanel.svelte#L1093-L1114)): every 30 ms, if the pointer is within a 36 px band of the top or bottom of the tracks, scroll by `min(30, 3 + depth/2)` px, faster closer to the edge, and replay the last pointer move so the drag follows. Used by clip drags, box select and track or bus reorder. There is **no horizontal** edge scroll in time, although backlog item 3o asked for one.

---

## 7. Rendering performance tied to zoom

These are the fixes that made zoom feel smooth on the lead's 32-track session. Each was measured on real sessions; the numbers are in decisions.md.

1. **One view change per frame** for wheel zoom and scroll (5.3). Commit [e1b471f](https://github.com/nwh212/Audiform/commit/e1b471f).
2. **Never resize a canvas unless its size changed.** Setting `canvas.width` throws the bitmap away. Lane, ruler and overview canvases compare first and use `clearRect` otherwise ([LaneCanvas.svelte#L166-L178](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/LaneCanvas.svelte#L166-L178)). Same commit.
3. **Same view object while zoom and scroll are unchanged** ([EditorPanel.svelte#L332-L342](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/EditorPanel.svelte#L332-L342)). A playhead move used to make a new `view` object, so every lane repainted: 1.46 million `fillRect` calls in ten ruler clicks, tasks of 1.2 to 1.5 s. Commit [5003294](https://github.com/nwh212/Audiform/commit/5003294).
4. **Waveform columns as one path per channel**, filled once, not one `fillRect` per pixel. Same commit.
5. **View changes reach only the timeline views.** `subscribeView` beside `subscribe`; a zoom no longer makes the Mixer, headers, Files, Properties and the rest re-derive ([session.ts#L700-L728](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/stores/session.ts#L700-L728)). Commit [c2fa164](https://github.com/nwh212/Audiform/commit/c2fa164).
6. **Playhead painted on top of a cached picture.** The Waveform editor draws the waveform into an offscreen `waveBase` canvas and paints the playhead over a copy, so a playhead move does not redraw the waveform. The spectral view does the same with `base`, and keeps the palette-colored spectrogram bitmap (`tinted`) until the image or palette changes ([SpectrumView.svelte#L316-L367](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/SpectrumView.svelte#L316-L367)).
7. **Off-screen lanes do not draw.** An `IntersectionObserver` with `rootMargin: "100% 0px"` (one screen above and below) marks lanes on screen; only those paint ([EditorPanel.svelte#L1520-L1545](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/EditorPanel.svelte#L1520-L1545)).
8. **A lane redraws only for its own files** (a `peakKey` built from its clips' peak entries), not when any file in the session finishes loading. Commit [a1773f3](https://github.com/nwh212/Audiform/commit/a1773f3).
9. **Peak level picked by zoom.** The peak cache (`.alpk`) holds min and max per channel per block at levels of 256, 4,096 and 65,536 frames, plus a finer first level of 32 frames for files up to 10 minutes or 64 frames up to an hour ([peaks/mod.rs#L35-L53](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/peaks/mod.rs#L35-L53)). The view computes `framesPerPx = spp * srcRate / sessionRate / stretchRatio` and picks the coarsest level whose block is no wider than one pixel (`pickLevel`, [timeline.ts#L329-L344](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/timeline.ts#L329-L344)).
10. **Levels load on demand under a budget.** A file's summary and coarsest level load first; finer levels load when a view asks (`peaks.level(path, framesPerPixel)` returns the best loaded level now and queues the right one; a redraw follows when it lands). Loaded fine levels stay under `PEAK_BUDGET = 96 MB`; the least recently drawn fine levels go first; a file's coarsest level always stays ([peaks.ts#L29-L108](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/stores/peaks.ts#L29-L108)). On a 56-track test, 368 MB fetched on open became 32 MB. Commit [8c841e8](https://github.com/nwh212/Audiform/commit/8c841e8).
11. **Exact samples when zoomed far in.** Below `SAMPLE_VIEW = 8` source frames per pixel, both editors draw from the file's real samples (a line through them, a dot on each when 6 px apart), through gain, envelope and fades ([exact-samples.ts](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/stores/exact-samples.ts)). Each file keeps one stretch: three times the visible span, starting one span before. Looped, reversed and stretched clips keep the peak drawing. The peak drawing stays up while samples load.
12. **Spectrogram fetched after the view settles.** The spectral view asks the core for a new image 90 ms after the last view change (pitch line 120 ms), with a sequence number so late answers are dropped. Meanwhile the old image is stretched by sample position so it lines up while zooming in time ([SpectrumView.svelte#L94-L126](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/SpectrumView.svelte#L94-L126)). During a frequency zoom the old image is not re-mapped vertically, so it is briefly wrong in frequency.

---

## 8. History, what the project lead asked for, and bugs we hit

### 8.1 The wheel rule went back and forth: final state and why

The plain wheel over the Multitrack lanes changed meaning seven times. The final rule (2026-11-05 and 2026-11-06) is what the code does today. The quotes below are from [docs/decisions.md](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/docs/decisions.md).

| Date | Rule | Asked by |
|---|---|---|
| 2026-10-14 | "Zoom: Ctrl+= and Ctrl+- (the approved shortcuts), Ctrl+mouse wheel around the pointer, Zoom to Session. Shift+wheel scrolls sideways." | Plan |
| 2026-10-19 | Plain wheel over the lanes zooms; over headers scrolls; setting to turn it off | Report from 0.1.4 testing |
| 2026-10-21 | "Ctrl+wheel over track headers (one track) and Ctrl+Shift+wheel (all tracks) were requested by the project lead and are approved." | Lead |
| 2026-10-22 | "Mouse wheel rule set by the project lead: lanes scroll tracks, ruler zooms time, track headers resize tracks. The 'wheel zooms over the timeline' setting is gone." Alt+wheel over lanes resizes the track; Alt+Shift+wheel zooms the waveform | Lead |
| 2026-10-24 | Plain wheel over lanes zooms at the pointer again; headers scroll; setting back | Tester |
| 2026-10-24 (later) | Ruler and overview zoom; lanes and headers scroll; track scrollbar zooms vertically; "wheel zooms" for the Waveform editor only | |
| 2026-10-25 | Ruler always zooms at the pointer; track scrollbar wheel anchors at the track under the pointer; wheel over headers makes every track taller or shorter (Alt: one track) | |
| 2026-11-02 | "The project lead wants the mouse wheel over the session's tracks to zoom in time, as it does over the ruler." | Lead |
| 2026-11-02 | "The lead wants the wheel over the tracks, the clips and the waveform to zoom exactly as it does over the time ruler." Pointer anchor everywhere; "between" gone | Lead |
| 2026-11-05 | "The project lead: the wheel over clips and tracks is meant to scroll vertically, not zoom." Ctrl+wheel anywhere and the wheel over ruler or overview zoom | Lead |
| 2026-11-06 | "The plain wheel over the track headers zooms the tracks vertically again, every open track taller or shorter evenly, as the track scrollbar's wheel does (the project lead)." | Lead |

What the lead clearly likes, stated more than once:

- **Zoom at the pointer, exactly like the ruler.** "The lead found Ctrl+wheel zoom over the tracks drifting, while the same zoom over the time ruler felt right." The 40% "between" anchor "felt wrong" and was removed.
- **Plain wheel over clips scrolls tracks vertically** (latest word, 2026-11-05).
- **Ruler and overview: wheel zooms time.**
- **Headers and track scrollbar: wheel zooms all tracks evenly.**
- **Alt+wheel: one track taller or shorter.**
- **A vertical zoom bar** (tester request, kept): "Dragging an end zooms vertically like the timeline's zoom bar."
- **Overview strip** with drag to scroll, drag edges to zoom, double-click to fit.
- **Smooth zoom on big sessions** ("The lead found horizontal zoom sluggish on a 32-track session").
- **Spectral scale drag**: "The project lead: grabbing the zoomed frequency list should drag up and down the frequencies."
- **Follow Playhead that lets go** when you look elsewhere while playing.
- **Waveform editor fits the file exactly** (no 5% room): "a 6:20 file showed to 6:39".

Requests restated from other editors (the project rules forbid copying another product's design, so they were restated as behavior): piano roll zoom and scroll (2026-10-25), spectral zoom and right-click options (2026-10-25), vertical zoom bar (2026-10-24).

Approved shortcuts (from `CLAUDE.md`): `Ctrl+=`, `Ctrl+-` zoom; `=`, `-` zoom horizontally; `Alt+=`, `Alt+-` zoom vertically; `\`, `Shift+/`, `Shift+\`, `Ctrl+\` for "Zoom reset, zoom in full, zoom out full (selected track, all axes)". The keymap ([default.json](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/keymap/default.json)) maps them as: `view.zoom-in` `Ctrl+=` `=`; `view.zoom-out` `Ctrl+-` `-`; `view.zoom-session` `\`; `view.zoom-selection` `Shift+/` `?`; `view.zoom-out-full` `Shift+\` `|`; `view.zoom-all-axes` `Ctrl+\` (not built); `view.zoom-in-vertical` `Alt+=`; `view.zoom-out-vertical` `Alt+-`. Ask the lead what "zoom in full" and "selected track" should mean before Audiform 2 binds `Shift+/`.

### 8.2 Bugs we hit and how they were fixed

| Symptom | Cause | Fix | Where |
|---|---|---|---|
| Horizontal zoom sluggish on a 32-track session (vertical scroll fine) | Each wheel event changed the view; trackpads send many per frame; each view change rewrote the document and resized every lane canvas | Gather wheel steps and scroll per frame (`zoomSteps`); resize canvases only when the size changed | [e1b471f](https://github.com/nwh212/Audiform/commit/e1b471f), decisions 2026-11-01 |
| A ruler click froze the window for over a second | New `view` object on every playhead move repainted every lane; one `fillRect` per pixel | Keep the view object; one path per channel | [5003294](https://github.com/nwh212/Audiform/commit/5003294) |
| Zoom made unrelated panels re-derive | View lived in the document; every subscriber heard it | `subscribeView` for view changes only | [c2fa164](https://github.com/nwh212/Audiform/commit/c2fa164) |
| `effect_update_depth_exceeded`, window stuck 70 to 90 s into Waveform playback | Follow at the file end set the same view again; the store notified; the follow ran again, forever | `setView` returns early when nothing changed | [360c121](https://github.com/nwh212/Audiform/commit/360c121) |
| Small scrolls while playing were pulled back | Follow only stopped once the playhead left the screen | Any user scroll or zoom while playing holds the follow | [973393d](https://github.com/nwh212/Audiform/commit/973393d), [48e7c0c](https://github.com/nwh212/Audiform/commit/48e7c0c) |
| Dragging the overview while playing snapped back to the playhead | Follow re-armed on any editor state change | Re-arm only on the toggle switch, playback start, playhead move, or 400 ms idle with the playhead in view | [7557623](https://github.com/nwh212/Audiform/commit/7557623) |
| The view kept jumping back to the playhead, even with Follow off | An old "keep playhead on screen" rule from before Follow Playhead was still there | Removed; Follow alone pages | [932e944](https://github.com/nwh212/Audiform/commit/932e944) |
| Ctrl+wheel zoom over the tracks drifted; over the ruler it felt right | Lanes used the 40% "between" anchor | Pointer anchor everywhere; option removed | [f167f45](https://github.com/nwh212/Audiform/commit/f167f45), [37885f6](https://github.com/nwh212/Audiform/commit/37885f6) |
| Plain wheel over clips did nothing in the app | Left to the browser's native scroll, which did not scroll the box in the webview | `scrollTracks` sets `scrollTop` itself, normalizing lines and pages | [64f6f54](https://github.com/nwh212/Audiform/commit/64f6f54), [bbbf0cb](https://github.com/nwh212/Audiform/commit/bbbf0cb) |
| A 3:17 FLAC looked 15 minutes long in the Waveform editor | Fit ran while the panel was still narrow after switching editors | Refit on width changes until the user zooms or scrolls | decisions 2026-10-19 (in squashed history before [fc2ee08](https://github.com/nwh212/Audiform/commit/fc2ee08)) |
| Waveform zoom out showed empty time past the end; zoom in stopped too early | Generic limits | Max zoom out = file fills view; min 1/32 spp; exact samples below 8 frames per px | [6a84803](https://github.com/nwh212/Audiform/commit/6a84803) |
| A 6:20 file showed to 6:39 | Fit added 5% room | Waveform fits with room 1; scroll clamped to the file end | decisions 2026-10-27 |
| Zoomed-in Waveform went blank on Windows | Exact-sample bytes arrive as ArrayBuffer, typed view or plain array depending on webview | Shared `rawBytes` helper; peak drawing stays up when samples fail | [fab0280](https://github.com/nwh212/Audiform/commit/fab0280) |
| Blocky waveforms zoomed in on short files | Finest level was 256 frames | 32-frame level up to 10 min, 64 up to 1 h (cache version 2) | decisions 2026-10-20 |
| Editor view over 2 GB of memory | Every peak level of every file loaded on open | On-demand levels under a 96 MB budget | [8c841e8](https://github.com/nwh212/Audiform/commit/8c841e8) |
| Playhead triangle on the ruler drifted right of the line | Ruler canvas stretched wider than the lanes by `flex: 1` | Ruler keeps exactly the lanes' width | [48e7c0c](https://github.com/nwh212/Audiform/commit/48e7c0c) |
| Spectral scale steps stayed mostly linear until the last step | Linear blend between linear and log | `expm1` bend so each step moves the middle by a similar number of octaves | [fb81c36](https://github.com/nwh212/Audiform/commit/fb81c36) |
| Few frequency marks when zoomed in on a log scale | Fixed mark list | Coarse-to-fine round marks added wherever they fit | [22f3b8e](https://github.com/nwh212/Audiform/commit/22f3b8e) |
| Bass filled most of the spectral view | Opened fully log | Opens a quarter log (`FULL_VIEW.log = 0.25`) | [79527e0](https://github.com/nwh212/Audiform/commit/79527e0) |
| Vertical zoom bar thumb missing when tracks fit; ends scaled each track by its own size | First version | Thumb always shown; ends set one height for all | [68ab396](https://github.com/nwh212/Audiform/commit/68ab396) |
| Wheel over header effect slots resized tracks | Header rule caught it | Slots scroll the tracks and stop the event | [245f7e5](https://github.com/nwh212/Audiform/commit/245f7e5) |
| Zoom Out Full and Mix Down ignored MIDI-only sessions | Session end measured audio clips only | Use `sessionEnd` with MIDI | [13b4837](https://github.com/nwh212/Audiform/commit/13b4837) |

Other commits on this area: [93c285c](https://github.com/nwh212/Audiform/commit/93c285c) (frequency zoom; pointer on rulers and track scrollbar), [be0bbd1](https://github.com/nwh212/Audiform/commit/be0bbd1) (scale drag), [5b812e9](https://github.com/nwh212/Audiform/commit/5b812e9) (piano roll zoom and scroll), [3947b28](https://github.com/nwh212/Audiform/commit/3947b28) (vertical zoom keys, Zoom Out Full), [9cc265e](https://github.com/nwh212/Audiform/commit/9cc265e) and [8325dee](https://github.com/nwh212/Audiform/commit/8325dee) (header wheel zooms tracks evenly), [2862255](https://github.com/nwh212/Audiform/commit/2862255) (edge auto-scroll), [32a69ed](https://github.com/nwh212/Audiform/commit/32a69ed) (smooth playhead drag), [e508a11](https://github.com/nwh212/Audiform/commit/e508a11) and [f4fa57e](https://github.com/nwh212/Audiform/commit/f4fa57e) (wheel rule flip and a revert to 0.7.1).

### 8.3 Open issues found while writing this (not fixed in Audiform 1)

From reading the code; none were run. Check before relying on them.

1. **Trackpads zoom too fast.** One step per event, sign only (5.1).
2. **Overview wheel ignores the zoom step setting and the 4-hour cap**, and is not frame-batched. Overview edge drag also uses `MAX_SPP` instead of `maxSppFor` ([OverviewStrip.svelte#L132-L138](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/OverviewStrip.svelte#L132-L138), [timeline.ts#L385-L413](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/timeline.ts#L385-L413)).
3. **`deltaY === 0` zooms in** on the Waveform ruler and the overview strip, so a sideways trackpad swipe there zooms in instead of scrolling.
4. **Ctrl+wheel and Ctrl+Shift+wheel over track headers** were approved by the lead (2026-10-21), but the code gives them no action (Alt is used instead). Ask which the lead still wants.
5. **Middle-button drag on the Multitrack ruler** does nothing, though a comment says "The middle mouse button still slides the timeline". Middle-drag works over lanes.
6. **Scroll stored as whole samples** shifts the view up to 16 px at the closest zoom.
7. **Line height** is 40 px in `scrollTracks` and 16 px in the effect slot list.
8. **Time Info view fields** allow 1/64 spp, past `MIN_SPP` (1/32).
9. **Zoom Out Full** does not subtract buses and automation lanes, so tracks may not fit; the scrollbar double-click does.
10. **The transport bar's "Zoom Out Full" button** runs Zoom to Session (time only), while the overview row's button and `Shift+\` do both axes. Same name, two behaviors.
11. **Keyboard zoom does not hold Follow Playhead** (6.1 caveat).
12. **Vertical zoom keys and Zoom Out Full in the Waveform editor** change the hidden Multitrack's track heights.
13. **`scroll_y` is never saved**, so vertical scroll is not restored on open.
14. **Header wheel anchor** uses scroll box geometry as if it were the scrollbar (4.1).
15. **No horizontal edge auto-scroll** during clip drags (asked for as backlog 3o).
16. **Piano roll has no follow** during its own playback.

---

## 9. Recommended design for Audiform 2

The goal: one zoom and viewport model, in plain Rust with no UI types, reused by every view. The UI layer only turns platform events into model inputs and draws what the model says.

### 9.1 Pieces

**1. `Axis`: one scrolled, zoomed 1D axis.** Use it for time (samples), track stack (pixels of content), frequency (through a mapping), piano keys and beats.

```rust
/// Content units per screen pixel, and the content position at the near edge.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Axis {
    pub units_per_px: f64,   // f64, never rounded
    pub offset: f64,         // f64: keep sub-sample scroll so the anchor holds at 32 px/sample
}

pub struct AxisLimits {
    pub min_units_per_px: f64,      // closest zoom
    pub max_units_per_px: f64,      // farthest zoom (may depend on content and width)
    pub min_offset: f64,            // usually 0
    pub max_offset: Option<f64>,    // Some(content_end - width*upp) for the Waveform editor; None for the Multitrack
}

impl Axis {
    pub fn to_px(&self, u: f64) -> f64 { (u - self.offset) / self.units_per_px }
    pub fn to_units(&self, px: f64) -> f64 { self.offset + px * self.units_per_px }

    /// Keeps the content under `anchor_px` at the same pixel.
    pub fn zoom_around(&self, factor: f64, anchor_px: f64, lim: &AxisLimits, width_px: f64) -> Axis {
        let upp = (self.units_per_px * factor).clamp(lim.min_units_per_px, lim.max_units_per_px);
        let anchor = self.to_units(anchor_px);
        Axis { units_per_px: upp, offset: anchor - anchor_px * upp }.clamped(lim, width_px)
    }
    pub fn pan_px(&self, dpx: f64, lim: &AxisLimits, width_px: f64) -> Axis { /* offset += dpx*upp, clamp */ }
    pub fn fit(len: f64, width_px: f64, room: f64, lim: &AxisLimits) -> Axis { /* room 1.05 or 1.0 */ }
    pub fn show_range(a: f64, b: f64, width_px: f64, margin: f64, lim: &AxisLimits) -> Axis { /* zoom to selection */ }
}
```

Store `offset` as `f64` in memory and in the session file (or store both a sample and a fraction). Round only when converting to a sample index for audio.

**2. `FreqScale`: the bent frequency mapping.** Port `viewFreq`, `viewFrac`, `zoomFreq`, `panFreq`, `freqTicks`, `rowEdges` as pure functions on `FreqView { lo, hi: Option<f64>, log: f64 }`. Keep the edits' fixed log axis separate, exactly as now.

**3. `WheelInput`: normalize once.**

```rust
pub enum DeltaMode { Pixel, Line, Page }
pub struct RawWheel { pub dx: f64, pub dy: f64, pub mode: DeltaMode, pub mods: Mods, pub pos: (f64, f64) }

pub struct WheelNorm {
    pub line_px: f64,          // pick one value, for example 40
    pub page_px: f64,          // the view height
    pub px_per_notch: f64,     // for example 100 (a Windows mouse notch)
}

/// Pixels moved, and fractional zoom notches (sign = direction). Trackpads give small
/// fractions, a mouse gives about 1 per notch. Keep the remainder between events if
/// you want whole steps; or use fractional steps with factor.powf(notches).
pub fn normalize(w: &RawWheel, n: &WheelNorm) -> (f64, f64, f64) { /* (dx_px, dy_px, notches) */ }
```

Use `factor.powf(notches)` for zoom, so a trackpad zooms in proportion to finger travel and a mouse still gives one 25% step per notch. Treat pinch (platform magnify events, or Ctrl+wheel in a webview) as notches too, with its own scale. Treat a zero `dy` as "no zoom", never as zoom in.

**4. `FrameAccumulator`.** Collect normalized input until the next frame; apply once. Keep the last pointer position as the anchor. Net out opposite steps. Apply scroll before zoom (as now).

**5. `GestureMap`: a table, not nested ifs.** Key: `(Region, Modifiers, Dominant axis)`. Value: an action (`ZoomTime { anchor: Pointer | Playhead | Center }`, `ScrollTime`, `ScrollTracks`, `ZoomTracksEven`, `ResizeOneTrack`, `ScaleAllTracks`, `ZoomAmplitude`, `ZoomFreq`, `ScrollKeys`, `ZoomKeys`, `None`). Fill it from section 2 (final state). This makes the lead's next rule change a one-line edit and lets you unit-test every cell. Keep the setting "wheel zooms" (Waveform only) and "anchor: pointer or playhead" as inputs to the table.

**6. `TrackStack`: vertical layout.** Heights per row (track, bus, automation lane, folded), limits 40 to 600 (folded 30), and the vertical `Axis` over the summed content. Operations: `scale_all(f)`, `even_all(h)`, `even_from_share(share, view_h)` (the zoom bar end formula), `fit_all(view_h)`, `resize_one(id, h)`. Each takes an anchor (content y under the pointer, or the opposite bar end) and returns the new offset so that point stays put. Fix the Zoom Out Full fit by counting fixed rows. Commit height changes as one undo step after a quiet period (300 ms), preview live before.

**7. `Follow`: a small state machine.**

```rust
pub enum FollowState { Off, Following, Held { since_user_move: Instant } }
// inputs: play_started, playhead_moved_by_user, toggle_on, user_view_change(at), tick(playhead_px, width, now)
// page when playhead_px > 0.92*width or < 0, to 0.08*width (Multitrack), or width-20 -> 20 (Waveform)
// Held -> Following when playhead in view and now - since_user_move > 400 ms
```

Every view change carries a source (`User` or `Follow`). All user zoom paths, including keyboard and Zoom to Selection, go through the same entry point so they all hold the follow.

**8. `ViewStore`: change notification.** Notify only on a real change (`!=`). Keep view listeners apart from document listeners. Debounce sending the view to the engine (300 ms) with a flush.

**9. Drawing helpers.** `ruler_ticks(axis, width, rate, min_gap)`, `bar_ticks`, `grid_lines`, `pick_peak_level(levels, frames_per_px)`, `exact_sample_threshold = 8`. Cache: one bitmap per lane keyed by (view, height, peak level, clip set, colors); draw the playhead as an overlay. Cull rows outside one screen above and below. Load finer peak levels on demand under a budget.

### 9.2 Ordered checklist

1. `Axis` with `to_px`, `to_units`, `zoom_around`, `pan_px`, `fit`, `show_range`, clamps. Tests first.
2. Time limits: `MIN = 1/32`, Multitrack max from `max(4 h * rate, content * 1.1) / width`, Waveform max = file fits exactly, Waveform offset clamped to the end.
3. `WheelInput` normalization and `FrameAccumulator`. Fractional notches. Zero delta means no zoom.
4. `GestureMap` with the final table from section 2, plus the two settings. Wire the Multitrack ruler, lanes, headers, scrollbar, overview.
5. Overview strip on the shared `Axis` (drag body, drag edges with `max` from step 2, click to center, wheel through the accumulator with the setting's step, double-click fit).
6. `TrackStack` with the even and proportional zooms, the zoom bar ends, fit, and anchoring. One undo step per gesture.
7. Keyboard actions: zoom in and out x2 (decide: center or pointer or playhead; Audiform 1 uses center in the Multitrack and playhead in the Waveform editor), Zoom to Session, Zoom to Selection, Zoom Out Full (both axes), vertical in and out x1.25 / x0.8. Key repeat for zoom in and out. Ask the lead about "zoom in full", "zoom reset" and "selected track".
8. `Follow` state machine for both editors.
9. Waveform editor: amplitude zoom 1 to 16 with dB marks, refit on width changes until the user zooms, refit when the file shrinks.
10. `FreqScale` and the spectral view: wheel zoom on the scale at the pointer, drag pan, five log steps, Hz or notes marks, reset.
11. Piano roll on the same `Axis` types (beats per pixel, keys) with the wheel table from 2.3 and a zoom bar.
12. Rendering: frame-batched redraw, bitmap cache per lane, playhead overlay, row culling, peak levels by zoom with on-demand loading and a budget, exact samples below 8 frames per pixel.
13. Fix the open issues in 8.3 that the lead agrees with.

### 9.3 Tests to write (port of Audiform 1's, plus the gaps)

From Audiform 1 ([timeline.test.ts](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/timeline.test.ts), [spectrum.test.ts](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/spectrum.test.ts)):

- `x_of` / `sample_at` round trip; `sample_at` never below 0.
- `zoom_around(v, 0.5, 200)` halves spp and keeps the sample at x = 200.
- Clamp at `MIN_SPP`; scroll clamped to 0 when zooming out near the start.
- `n` gathered steps equal `n` single steps (spp to 9 places, scroll to 6, anchor sample).
- `fit(60 s at 48 kHz, 1000)` gives `48000*60*1.05/1000`; `fit(0, w)` gives the default 1024.
- Ruler: 1,000 px over one minute labels every 10 s; labels at least 90 px apart; at 4 hours still at least 5 labels and minor ticks.
- `max_spp_for`: 4 hours for short content; content x 1.1 for 6 hours; zooming by 1e9 stops at the cap.
- Bars and beats ticks and grid lines through tempo and time signature changes.
- `pick_level` picks the coarsest level that still resolves a pixel; empty gives none.
- Overview window placement; edge drag keeps the other edge.
- Frequency: full view matches the edits' axis; `view_frac(view_freq(f)) == f` for log 0, 0.5, 1; zoom keeps 1 kHz in place; never narrower than 40 Hz; zooming out returns to `hi = None`; marks spaced at least 16 px; finer round marks in a narrow log band; the open view puts 1 kHz between 8% and 16% and 10 kHz above 60%; pan by half a view in Hz and by ratios; pan stops at the floor and at Nyquist.
- Store: a same-view set notifies no one; a view change reaches view listeners only.

New tests Audiform 1 lacks:

- Wheel normalization: line and page modes; a 100 px mouse notch gives one notch; 20 trackpad events of 5 px give the same total zoom as one notch; zero `dy` gives no zoom.
- Accumulator: opposite notches in one frame cancel; anchor is the last pointer.
- Gesture table: one test per cell of section 2's final table.
- Track stack: even wheel zoom keeps the content y under the pointer; zoom bar end drag keeps the other end; fit with buses, lanes and folded rows fits exactly; limits 40 to 600.
- Follow: pages at 92% to 8%; a user scroll holds; resumes after 400 ms with the playhead in view; keyboard zoom also holds; toggle on resumes at once.
- Sub-sample scroll: at 32 px per sample, 100 wheel zooms in and out at a fixed pointer leave the anchor within 0.01 px.

---

## 10. File map

| File | What it holds for zoom and scroll |
|---|---|
| [src/panels/editor/timeline.ts](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/timeline.ts) | `Viewport`, `MIN_SPP`, `MAX_SPP`, `maxSppFor`, `zoomAround`, `zoomSteps`, `fit`, ruler ticks, bar ticks, grid lines, `pickLevel`, `peakRange`, overview window math, `COLLAPSED_H`, `TRACK_GAP` |
| [src/panels/editor/timeline.test.ts](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/timeline.test.ts) | Tests for the above |
| [src/panels/editor/EditorPanel.svelte](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/EditorPanel.svelte) | Multitrack wheel router, frame queue, track scroll, Follow Playhead, `setView`, track height gestures, track scrollbar presses, Zoom Out Full, vertical zoom actions, middle-drag pan, edge auto-scroll, lane culling, amplitude zoom |
| [src/panels/editor/OverviewStrip.svelte](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/OverviewStrip.svelte) | Overview strip: drag, edge zoom, click to jump, wheel zoom, double-click fit |
| [src/panels/editor/TrackScrollbar.svelte](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/TrackScrollbar.svelte) | Track scrollbar drawing and end hover (presses handled in EditorPanel) |
| [src/panels/editor/HeaderFx.svelte](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/HeaderFx.svelte) | Effect slot list: wheel scrolls the tracks |
| [src/panels/editor/LaneCanvas.svelte](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/LaneCanvas.svelte) | Lane drawing: canvas size check, peak level by zoom, exact samples, `ampZoom` |
| [src/panels/editor/ruler-draw.ts](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/ruler-draw.ts) | Multitrack ruler drawing |
| [src/panels/editor/automation.ts](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/automation.ts) | Automation lane height limits and default |
| [src/panels/editor/rows.ts](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/rows.ts) | Row order of tracks and buses (no zoom logic; tells you what the vertical stack holds) |
| [src/panels/editor/TransportBar.svelte](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/TransportBar.svelte) | Zoom buttons and `ZoomKind` |
| [src/panels/editor/WaveformView.svelte](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/WaveformView.svelte) | Waveform editor wheel, zoom commands, limits, refit, follow, amplitude zoom and dB scale, scrollbar, cached picture |
| [src/panels/editor/SpectrumView.svelte](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/SpectrumView.svelte) | Spectral view: scale wheel zoom, scale drag pan, right-click scale menu, debounced fetch, cached bitmaps |
| [src/panels/editor/spectrum.ts](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/spectrum.ts) | Frequency axes, bent scale, `zoomFreq`, `panFreq`, `freqTicks`, `rowEdges` |
| [src/panels/editor/spectrum.test.ts](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/spectrum.test.ts) | Tests for the above |
| [src/stores/spectrum-view.ts](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/stores/spectrum-view.ts) | Spectral view state, FFT sizes |
| [src/panels/editor/PianoRollDialog.svelte](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/PianoRollDialog.svelte) | Piano roll zoom, key height, wheel, middle-drag, zoom bar, fit |
| [src/panels/editor/midi.ts](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/midi.ts) | Note editing helpers (grid, snap); no zoom logic |
| [src/stores/settings.ts](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/stores/settings.ts) | `zoomStep`, `wheelAnchor`, `wheelZooms`, `wheelAnchorX`, `zoomRatios` |
| [src/app/settings/SettingsDialog.svelte](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/app/settings/SettingsDialog.svelte) | The three settings' labels (lines 241 to 249) |
| [src/stores/editor.ts](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/stores/editor.ts) | `width` of the lanes for zoom commands, `followPlayhead` |
| [src/stores/session.ts](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/stores/session.ts) | `ViewDoc`, `sameView`, `setView` (no-change rule, 300 ms to core), `subscribeView`, `flushView` |
| [src-tauri/src/session/model.rs](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/session/model.rs) | `SessionView` saved in the session file |
| [src-tauri/src/session/ops/settings.rs](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/session/ops/settings.rs) | View rescaled on sample rate change |
| [src/app/navigation.ts](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/app/navigation.ts) | `zoomBy` (keyboard zoom at center) |
| [src/app/view-tools.svelte.ts](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/app/view-tools.svelte.ts) | `view.zoom-in`, `view.zoom-out`, `view.zoom-session`, `view.zoom-selection` |
| [src/app/wave-tools.svelte.ts](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/app/wave-tools.svelte.ts) | Waveform overrides of the four zoom actions (lines 1072 to 1075) |
| [src/app/menus/model.ts](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/app/menus/model.ts) | View menu zoom items (lines 451 to 458) |
| [src/keymap/default.json](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/keymap/default.json) | Zoom key bindings |
| [src/keymap/dispatch.ts](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/keymap/dispatch.ts) | Key repeat allowed for zoom in and out |
| [src/stores/peaks.ts](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/stores/peaks.ts) | On-demand peak levels, 96 MB budget |
| [src/stores/exact-samples.ts](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/stores/exact-samples.ts) | Exact samples below 8 frames per pixel |
| [src-tauri/src/peaks/mod.rs](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/peaks/mod.rs) | Peak cache format and levels (32 or 64, 256, 4,096, 65,536 frames) |
| [src/panels/time-info/time-info.ts](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/time-info/time-info.ts) | `viewRange`, `viewFor` (view from typed start and end) |
| [src/panels/voice-chain/EqGraph.svelte](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/voice-chain/EqGraph.svelte) | EQ graph: ±15 or ±48 dB range toggle; wheel sets band width |
| [docs/decisions.md](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/docs/decisions.md) | Every decision quoted in section 8 (search "wheel", "zoom", "follow") |
| [docs/release-notes-0.7.1.md](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/docs/release-notes-0.7.1.md), [0.8.0](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/docs/release-notes-0.8.0.md) | User-facing wording of smooth zoom, peak memory, and the wheel rule |
