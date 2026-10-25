<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Altered Audio Editor.

  A MIDI clip's notes, on a piano roll. Click empty space to add a note
  one grid step long; drag a note's middle to move it, drag either end to
  stretch it that way, and double-click or right-click it to delete it
  (hold the right button and drag to erase every note it passes over).
  Each note sounds through the track's instrument as it is drawn, picked
  up, moved to another pitch or given a new velocity, and the keys on the
  left play their note when clicked.
  Shift-click picks several, Shift-drag on empty space picks all in a
  box; moving or stretching one picked note moves or stretches them all.
  Copy and paste (Ctrl+C, Ctrl+V) paste at the last spot clicked. Snap
  turns the grid on and off. The velocity lane below sets how hard each
  note plays.
  Getting around: the mouse wheel scrolls up and down the keys, Shift
  and the wheel scrolls along time, Ctrl and the wheel zooms time around
  the pointer, Alt and the wheel makes the keys taller or shorter. Drag
  with the middle button to pan either way. The bar under the roll shows
  the part in view: drag it to scroll, drag its ends to zoom, and
  double-click it to fit the clip. The wheel over the bar ruler zooms too. Done (or Escape) keeps the changes as one undoable edit;
  Cancel drops them.
-->
<script lang="ts" module>
  /** Copied notes, kept while the app runs, so they paste into any clip. */
  let clipboard: import("../../stores/session").NoteDoc[] = [];
  /** Key height, kept while the app runs. */
  let lastRow = 14;
</script>
<script lang="ts">
  import type { MidiClipDoc, NoteDoc, TempoDoc } from "../../stores/session";
  import { barAt, barBeats, segmentAtBeat } from "../../stores/tempo";
  import { tick } from "svelte";
  import { GRIDS, MIN_NOTE, addNote, copyNotes, gridBeats, isBlackKey, moveNotes, noteName, pasteNotes, pitchRange, removeNotes, resizeNotes, resizeNotesStart, setVelocity, snap, snapDown, spanOf } from "./midi";

  interface Props {
    clip: MidiClipDoc;
    tempo: TempoDoc;
    /** The instrument's name, for the title. */
    instrument: string | null;
    /** The notes' color: the clip's, or its track's. */
    color?: string;
    onApply: (clip: MidiClipDoc) => void;
    onClose: () => void;
    /** Plays a note through the track's instrument: pitch, velocity (1 to 127), length in seconds. */
    onAudition?: (pitch: number, velocity: number, seconds: number) => void;
  }

  const { clip: original, tempo, instrument, color = "var(--accent)", onApply, onClose, onAudition = () => {} }: Props = $props();

  /** Hears a note, held for its length (within reason). */
  function hear(n: NoteDoc) {
    onAudition(n.pitch, n.velocity, Math.min(1, Math.max(0.15, (n.length * 60) / segmentAtBeat(tempo, original.start + n.start).bpm)));
  }

  // svelte-ignore state_referenced_locally
  let clip = $state<MidiClipDoc>(structuredClone($state.snapshot(original)) as MidiClipDoc);
  let picked = $state<Set<number>>(new Set());
  let grid = $state(0.25);
  /** Snap to the grid while drawing, moving and stretching. */
  let snapOn = $state(true);
  /** Where Paste puts notes: the last spot clicked on the grid, in beats. */
  let cursor = $state<number | null>(null);
  let hasCopy = $state(clipboard.length > 0);
  /** Shift-drag on empty space: the box being drawn, in pixels. */
  let box = $state<{ x0: number; y0: number; x1: number; y1: number } | null>(null);
  let zoom = $state(64);
  /** Length (beats) of the next note added: the last one drawn or resized. */
  let drawLength = $state(0.25);

  /** Key height in pixels. */
  let row = $state(lastRow);
  $effect(() => {
    lastRow = row;
  });
  const KEYS_W = 46;
  const RULER_H = 18;
  const ZOOM_MIN = 4;
  const ZOOM_MAX = 600;
  const ROW_MIN = 6;
  const ROW_MAX = 36;
  const step = $derived(snapOn ? gridBeats(grid, tempo, original.start) : 0);
  /** Beats in a bar where the clip starts. */
  const barLen = $derived(barBeats(segmentAtBeat(tempo, original.start)));
  const changed = $derived(JSON.stringify($state.snapshot(clip)) !== JSON.stringify(original));
  /** The roll's width in pixels, keys included. */
  let viewW = $state(800);
  /** The grid runs two bars past the clip's end (or its last note), so there is room to work. */
  const widthPx = $derived(
    Math.max((Math.max(clip.length, ...clip.notes.map((n) => n.start + n.length), cursor ?? 0) + 2 * barLen) * zoom, viewW - KEYS_W),
  );

  let scroller = $state<HTMLDivElement | null>(null);
  /** The scroll position and view size, kept for the zoom bar. */
  let scrollX = $state(0);
  const totalBeats = $derived(widthPx / zoom);

  /** Zooms time so the whole clip is in view. */
  function fit() {
    if (!scroller) return;
    const room = scroller.clientWidth - KEYS_W - 16;
    zoom = Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, room / Math.max(0.25, clip.length)));
    scroller.scrollLeft = 0;
  }

  let opened = false;
  $effect(() => {
    if (!scroller || opened) return;
    opened = true;
    // Open on the notes there are, or around middle C.
    const [lo, hi] = pitchRange(original.notes);
    const mid = (lo + hi) / 2;
    scroller.scrollTop = (127 - mid) * row - scroller.clientHeight / 2;
    // And zoomed so the whole clip fits.
    fit();
  });

  /** Zooms time to `next` pixels a beat, keeping the beat under `px` (pixels into the view) where it is. */
  async function zoomTime(next: number, px = (viewW - KEYS_W) / 2) {
    if (!scroller) return;
    const z = Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, next));
    const beat = (scroller.scrollLeft + px) / zoom;
    zoom = z;
    await tick();
    scroller.scrollLeft = beat * z - px;
  }

  /** Makes the keys `next` pixels tall, keeping the key under `py` (pixels into the view) where it is. */
  async function zoomKeys(next: number, py = scroller ? scroller.clientHeight / 2 : 0) {
    if (!scroller) return;
    const r = Math.round(Math.min(ROW_MAX, Math.max(ROW_MIN, next)));
    if (r === row) return;
    const key = (scroller.scrollTop + py - RULER_H) / row;
    row = r;
    await tick();
    scroller.scrollTop = key * r - py + RULER_H;
  }

  function onRollWheel(e: WheelEvent) {
    if (!scroller) return;
    const rect = scroller.getBoundingClientRect();
    const d = e.deltaY || e.deltaX;
    if (e.ctrlKey || e.metaKey) {
      e.preventDefault();
      void zoomTime(zoom * (d < 0 ? 1.2 : 1 / 1.2), Math.max(0, e.clientX - rect.left - KEYS_W));
    } else if (e.altKey) {
      e.preventDefault();
      void zoomKeys(row + (d < 0 ? 2 : -2), e.clientY - rect.top);
    } else if (e.shiftKey) {
      e.preventDefault();
      scroller.scrollLeft += d;
    }
    // A plain wheel scrolls up and down the keys.
  }

  function onRulerWheel(e: WheelEvent) {
    if (!scroller) return;
    e.preventDefault();
    e.stopPropagation();
    const rect = scroller.getBoundingClientRect();
    if (e.shiftKey) scroller.scrollLeft += e.deltaY || e.deltaX;
    else void zoomTime(zoom * ((e.deltaY || e.deltaX) < 0 ? 1.2 : 1 / 1.2), Math.max(0, e.clientX - rect.left - KEYS_W));
  }

  /** Middle button: drag to pan either way. */
  function onRollDown(e: PointerEvent) {
    if (e.button !== 1 || !scroller) return;
    e.preventDefault();
    const el = scroller;
    const x0 = e.clientX;
    const y0 = e.clientY;
    const l0 = el.scrollLeft;
    const t0 = el.scrollTop;
    el.style.cursor = "grabbing";
    const move = (ev: PointerEvent) => {
      el.scrollLeft = l0 - (ev.clientX - x0);
      el.scrollTop = t0 - (ev.clientY - y0);
    };
    const up = () => {
      el.style.cursor = "";
      window.removeEventListener("pointermove", move, true);
      window.removeEventListener("pointerup", up, true);
    };
    window.addEventListener("pointermove", move, true);
    window.addEventListener("pointerup", up, true);
  }

  // The zoom bar: the part of the clip in view, in beats.
  const shownFrom = $derived(scrollX / zoom);
  const shownTo = $derived((scrollX + Math.max(1, viewW - KEYS_W)) / zoom);
  let zoomBar = $state<HTMLDivElement | null>(null);

  function onZoomBarDown(e: PointerEvent, part: "move" | "start" | "end" | "track") {
    if (e.button !== 0 || !zoomBar || !scroller) return;
    e.preventDefault();
    e.stopPropagation();
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    const barW = zoomBar.getBoundingClientRect().width;
    const total = totalBeats;
    const perPx = total / Math.max(1, barW);
    const a0 = shownFrom;
    const b0 = shownTo;
    const x0 = e.clientX;
    const room = Math.max(1, viewW - KEYS_W);
    if (part === "track") {
      // Jump so the clicked spot is in the middle.
      const r = zoomBar.getBoundingClientRect();
      const at = ((e.clientX - r.left) / barW) * total;
      scroller.scrollLeft = (at - (b0 - a0) / 2) * zoom;
      return;
    }
    const move = async (ev: PointerEvent) => {
      if (!scroller) return;
      const d = (ev.clientX - x0) * perPx;
      if (part === "move") {
        scroller.scrollLeft = (a0 + d) * zoom;
        return;
      }
      // An end: zoom so the other end stays put.
      let a = a0;
      let b = b0;
      if (part === "start") a = Math.min(b0 - room / ZOOM_MAX, Math.max(0, a0 + d));
      else b = Math.max(a0 + room / ZOOM_MAX, b0 + d);
      const z = Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, room / (b - a)));
      zoom = z;
      await tick();
      scroller.scrollLeft = (part === "start" ? b * z - room : a * z);
    };
    const up = () => {
      el.removeEventListener("pointermove", move);
      el.removeEventListener("pointerup", up);
      el.removeEventListener("pointercancel", up);
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up);
    el.addEventListener("pointercancel", up);
  }

  /** Bar numbers along the top, counted as in the session. */
  const firstBar = $derived(barAt(tempo, original.start).bar);
  const rulerBars = $derived.by(() => {
    const out: { x: number; label: string | null }[] = [];
    const n = Math.ceil(totalBeats / barLen) + 1;
    // Label every bar, or every few when bars are narrow.
    const every = Math.max(1, 2 ** Math.ceil(Math.log2(Math.max(1, 34 / (barLen * zoom)))));
    for (let k = 0; k < n; k++) out.push({ x: k * barLen * zoom, label: k % every === 0 ? String(firstBar + k + 1) : null });
    return out;
  });

  const yOf = (pitch: number) => (127 - pitch) * row;
  const xOf = (beat: number) => beat * zoom;

  /** Where a pointer is on the grid: beat (unsnapped) and pitch. */
  function at(e: MouseEvent, el: HTMLElement): { beat: number; pitch: number } {
    const r = el.getBoundingClientRect();
    const beat = Math.max(0, (e.clientX - r.left) / zoom);
    const pitch = Math.min(127, Math.max(0, 127 - Math.floor((e.clientY - r.top) / row)));
    return { beat, pitch };
  }

  function noteAt(beat: number, pitch: number): number {
    for (let i = clip.notes.length - 1; i >= 0; i--) {
      const n = clip.notes[i]!;
      if (n.pitch === pitch && beat >= n.start && beat < n.start + n.length) return i;
    }
    return -1;
  }

  /** Notes are kept in order; after a change, pick the same notes again by value. */
  function setNotes(next: NoteDoc[], keep: NoteDoc[] = []) {
    const sortedNotes = [...next].sort((a, b) => a.start - b.start || a.pitch - b.pitch);
    clip = { ...clip, notes: sortedNotes };
    picked = new Set(keep.map((k) => sortedNotes.indexOf(k)).filter((i) => i >= 0));
  }

  /** Right button held and dragged: erases every note it passes over. */
  function startErase(e: PointerEvent, el: HTMLElement) {
    e.preventDefault();
    const eraseAt = (ev: PointerEvent) => {
      const p = at(ev, el);
      const i = noteAt(p.beat, p.pitch);
      if (i >= 0) setNotes(removeNotes(clip.notes, new Set([i])));
    };
    eraseAt(e);
    const up = () => {
      window.removeEventListener("pointermove", eraseAt, true);
      window.removeEventListener("pointerup", up, true);
    };
    window.addEventListener("pointermove", eraseAt, true);
    window.addEventListener("pointerup", up, true);
  }

  function onGridDown(e: PointerEvent) {
    if (e.button === 2) {
      startErase(e, e.currentTarget as HTMLElement);
      return;
    }
    if (e.button !== 0) return;
    const el = e.currentTarget as HTMLElement;
    const p = at(e, el);
    let i = noteAt(p.beat, p.pitch);
    const startX = e.clientX;
    const startY = e.clientY;
    cursor = step > 0 ? snapDown(p.beat, step) : p.beat;
    let mode: "move" | "end" | "start";
    if (i < 0 && e.shiftKey) {
      // Shift-drag on empty space: pick every note the box touches.
      const r = el.getBoundingClientRect();
      const x0 = e.clientX - r.left;
      const y0 = e.clientY - r.top;
      const before = new Set(picked);
      box = { x0, y0, x1: x0, y1: y0 };
      const moveBox = (ev: PointerEvent) => {
        if (!box) return;
        box = { ...box, x1: ev.clientX - r.left, y1: ev.clientY - r.top };
        const [bx0, bx1] = [Math.min(box.x0, box.x1), Math.max(box.x0, box.x1)];
        const [by0, by1] = [Math.min(box.y0, box.y1), Math.max(box.y0, box.y1)];
        const inside = new Set(before);
        clip.notes.forEach((n, k) => {
          const nx0 = xOf(n.start);
          const nx1 = xOf(n.start + n.length);
          const ny0 = yOf(n.pitch);
          if (nx1 >= bx0 && nx0 <= bx1 && ny0 + row >= by0 && ny0 <= by1) inside.add(k);
        });
        picked = inside;
      };
      const endBox = () => {
        window.removeEventListener("pointermove", moveBox, true);
        window.removeEventListener("pointerup", endBox, true);
        box = null;
      };
      window.addEventListener("pointermove", moveBox, true);
      window.addEventListener("pointerup", endBox, true);
      return;
    }
    if (i < 0) {
      // A new note, one grid step (or the last length drawn), on the grid.
      const n: NoteDoc = { pitch: p.pitch, velocity: 100, start: snapDown(p.beat, step), length: step > 0 ? Math.max(step, drawLength) : drawLength };
      const notes = addNote(clip.notes, n);
      clip = { ...clip, notes };
      i = notes.findIndex((x) => x.start === n.start && x.pitch === n.pitch);
      picked = new Set([i]);
      mode = "end";
      hear(n);
    } else {
      const n = clip.notes[i]!;
      // Near either end stretches that way; the middle moves.
      const px = e.clientX - el.getBoundingClientRect().left;
      const zone = Math.min(6, (n.length * zoom) / 3);
      mode = xOf(n.start + n.length) - px <= zone ? "end" : px - xOf(n.start) <= zone ? "start" : "move";
      if (e.shiftKey) {
        const next = new Set(picked);
        if (next.has(i)) next.delete(i);
        else next.add(i);
        picked = next;
        return;
      }
      if (!picked.has(i)) picked = new Set([i]);
      hear(n);
    }
    const base = clip.notes;
    const sel = new Set(picked);
    // The note grabbed: the picked ones follow it, by the same amount.
    const grabbed = base[i]!;
    let heardPitch = grabbed.pitch;
    const move = (ev: PointerEvent) => {
      const dBeats = (ev.clientX - startX) / zoom;
      if (mode === "move") {
        // The grabbed note lands on the grid; the rest keep their places around it.
        const d = step > 0 ? snap(grabbed.start + dBeats, step) - grabbed.start : dBeats;
        const ds = -Math.round((ev.clientY - startY) / row);
        clip = { ...clip, notes: moveNotes(base, sel, d, ds) };
        // Moved to another pitch: hear it there.
        const now = Math.min(127, Math.max(0, grabbed.pitch + ds));
        if (now !== heardPitch) {
          heardPitch = now;
          hear({ ...grabbed, pitch: now });
        }
      } else if (mode === "end") {
        const end = grabbed.start + grabbed.length + dBeats;
        const target = step > 0 ? Math.max(grabbed.start + step, snap(end, step)) : Math.max(grabbed.start + MIN_NOTE, end);
        clip = { ...clip, notes: resizeNotes(base, sel, target - (grabbed.start + grabbed.length)) };
      } else {
        const end = grabbed.start + grabbed.length;
        const start = grabbed.start + dBeats;
        const target = Math.max(0, step > 0 ? Math.min(end - step, snap(start, step)) : Math.min(end - MIN_NOTE, start));
        clip = { ...clip, notes: resizeNotesStart(base, sel, target - grabbed.start) };
      }
    };
    const up = () => {
      window.removeEventListener("pointermove", move, true);
      window.removeEventListener("pointerup", up, true);
      const keep = [...sel].map((k) => clip.notes[k]!).filter(Boolean);
      if (mode !== "move" && clip.notes[i]) drawLength = clip.notes[i]!.length;
      setNotes(clip.notes, keep);
    };
    window.addEventListener("pointermove", move, true);
    window.addEventListener("pointerup", up, true);
  }

  function deleteAt(e: MouseEvent) {
    e.preventDefault();
    // A right-click already erased on the button press (see startErase).
    if (e.type === "contextmenu") return;
    const p = at(e, e.currentTarget as HTMLElement);
    const i = noteAt(p.beat, p.pitch);
    if (i >= 0) setNotes(removeNotes(clip.notes, new Set([i])));
  }

  function deletePicked() {
    if (picked.size) setNotes(removeNotes(clip.notes, picked));
  }

  function copyPicked(cut = false) {
    if (!picked.size) return;
    clipboard = copyNotes(clip.notes, picked);
    hasCopy = true;
    if (cut) deletePicked();
  }

  /** Pastes at the last spot clicked (or right after the copied notes), then moves the spot past them. */
  function paste() {
    if (!clipboard.length) return;
    const firstPicked = picked.size ? Math.min(...[...picked].map((k) => clip.notes[k]?.start ?? 0)) : 0;
    const at = cursor ?? firstPicked + spanOf(clipboard);
    const r = pasteNotes(clip.notes, clipboard, at);
    // Pasted past the end: the clip grows to the next bar line, so they play.
    const end = at + spanOf(clipboard);
    const bar = barLen;
    clip = { ...clip, notes: r.notes, length: end > clip.length ? Math.ceil(end / bar - 1e-9) * bar : clip.length };
    picked = r.pasted;
    cursor = at + spanOf(clipboard);
  }

  function onVelocityDown(e: PointerEvent) {
    if (e.button !== 0) return;
    const el = e.currentTarget as HTMLElement;
    const set = (ev: PointerEvent) => {
      const r = el.getBoundingClientRect();
      const beat = (ev.clientX - r.left) / zoom;
      const v = Math.round((1 - (ev.clientY - r.top) / r.height) * 127);
      // The note under the pointer, or every picked note when it is one of them.
      const i = clip.notes.findIndex((n) => beat >= n.start && beat < n.start + Math.max(n.length, 4 / zoom));
      if (i < 0) return;
      const which = picked.has(i) ? picked : new Set([i]);
      clip = { ...clip, notes: setVelocity(clip.notes, which, v) };
    };
    set(e);
    const up = (ev: PointerEvent) => {
      window.removeEventListener("pointermove", set, true);
      window.removeEventListener("pointerup", up, true);
      // Hear the note at its new velocity.
      const r = el.getBoundingClientRect();
      const beat = (ev.clientX - r.left) / zoom;
      const n = clip.notes.find((x) => beat >= x.start && beat < x.start + Math.max(x.length, 4 / zoom));
      if (n) hear(n);
    };
    window.addEventListener("pointermove", set, true);
    window.addEventListener("pointerup", up, true);
  }

  const pickedVelocity = $derived(picked.size ? Math.round([...picked].reduce((s, i) => s + (clip.notes[i]?.velocity ?? 0), 0) / picked.size) : null);

  function done() {
    if (changed) onApply($state.snapshot(clip) as MidiClipDoc);
    else onClose();
  }

  function onKey(e: KeyboardEvent) {
    const t = e.target as HTMLElement | null;
    if (t && (t.tagName === "INPUT" || t.tagName === "SELECT")) {
      if (e.key === "Escape") t.blur();
      return;
    }
    if (e.key === "Escape") done();
    else if (e.key === "Delete" || e.key === "Backspace") {
      e.preventDefault();
      deletePicked();
    } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "a") {
      e.preventDefault();
      picked = new Set(clip.notes.map((_, i) => i));
    } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "c") {
      e.preventDefault();
      copyPicked();
    } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "x") {
      e.preventDefault();
      copyPicked(true);
    } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "v") {
      e.preventDefault();
      paste();
    }
  }

  const pitches = Array.from({ length: 128 }, (_, k) => 127 - k);
</script>

<svelte:window onkeydown={onKey} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={done}>
  <div class="dialog" style:--note={color} role="dialog" aria-modal="true" aria-labelledby="pr-title" tabindex="-1" onclick={(e) => e.stopPropagation()}>
    <header>
      <h1 id="pr-title">Edit Notes</h1>
      <label>Name <input type="text" class="name" bind:value={clip.name} /></label>
      <label title="The clip's length in beats. Notes past the end are not played.">
        Length
        <input type="number" min="0.25" step="1" value={clip.length} onchange={(e) => (clip = { ...clip, length: Math.max(0.25, Number(e.currentTarget.value) || clip.length) })} />
        beats
      </label>
      <label title="Snap notes to the grid while drawing, moving and stretching">
        <input type="checkbox" bind:checked={snapOn} />
        Snap
      </label>
      <label>
        Grid
        <select bind:value={grid} disabled={!snapOn}>
          {#each GRIDS.filter((g) => g.beats >= 0) as g (g.beats)}<option value={g.beats}>{g.label}</option>{/each}
        </select>
      </label>
      <label title="How hard the picked notes play, 1 to 127">
        Velocity
        <input
          type="number"
          min="1"
          max="127"
          disabled={!picked.size}
          value={pickedVelocity ?? ""}
          onchange={(e) => (clip = { ...clip, notes: setVelocity(clip.notes, picked, Number(e.currentTarget.value) || 100) })}
        />
      </label>
      <span class="zoom" role="group" aria-label="Zoom">
        <button type="button" title="Zoom out in time (Ctrl and the mouse wheel)" aria-label="Zoom out in time" onclick={() => void zoomTime(zoom / 1.5)}>−</button>
        <button type="button" title="Zoom in in time (Ctrl and the mouse wheel)" aria-label="Zoom in in time" onclick={() => void zoomTime(zoom * 1.5)}>+</button>
        <button type="button" title="Fit the clip in view (or double-click the bar under the roll)" onclick={fit}>Fit</button>
        <button type="button" title="Shorter keys (Alt and the mouse wheel)" aria-label="Shorter keys" onclick={() => void zoomKeys(row - 2)}>
          <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3 5.5h10M3 10.5h10" stroke="currentColor" stroke-width="1.3" /><path d="M8 1.5v3l-1.5-1.5M8 4.5l1.5-1.5M8 14.5v-3l-1.5 1.5M8 11.5l1.5 1.5" fill="none" stroke="currentColor" stroke-width="1.1" /></svg>
        </button>
        <button type="button" title="Taller keys (Alt and the mouse wheel)" aria-label="Taller keys" onclick={() => void zoomKeys(row + 2)}>
          <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3 5.5h10M3 10.5h10" stroke="currentColor" stroke-width="1.3" /><path d="M8 4.5v-3L6.5 3M8 1.5L9.5 3M8 11.5v3l-1.5-1.5M8 14.5l1.5-1.5" fill="none" stroke="currentColor" stroke-width="1.1" /></svg>
        </button>
      </span>
      <button type="button" disabled={!picked.size} title="Copy the picked notes (Ctrl+C)" onclick={() => copyPicked()}>Copy</button>
      <button type="button" disabled={!hasCopy} title="Paste at the last spot clicked (Ctrl+V)" onclick={paste}>Paste</button>
      <button type="button" disabled={!picked.size} title="Delete the picked notes (Delete)" onclick={deletePicked}>Delete</button>
    </header>

    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="roll"
      bind:this={scroller}
      bind:clientWidth={viewW}
      onscroll={() => (scrollX = scroller?.scrollLeft ?? 0)}
      onwheel={onRollWheel}
      onpointerdown={onRollDown}
      onauxclick={(e) => e.preventDefault()}
    >
      <div class="keys" style:height="{128 * row + RULER_H}px">
        <div class="keys-corner" style:height="{RULER_H}px"></div>
        {#each pitches as p (p)}
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
          <div class="key" class:black={isBlackKey(p)} class:c={p % 12 === 0} style:height="{row}px" title={noteName(p)} onpointerdown={(e) => e.button === 0 && onAudition(p, 100, 0.4)}>{p % 12 === 0 && row >= 9 ? noteName(p) : ""}</div>
        {/each}
      </div>
      <div class="area">
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="ruler" style:width="{widthPx}px" style:height="{RULER_H}px" onwheel={onRulerWheel} title="Bars. Mouse wheel here zooms time (Shift scrolls).">
          {#each rulerBars as b, k (k)}
            <span class="tick" class:labeled={b.label !== null} style:left="{b.x}px">{b.label ?? ""}</span>
          {/each}
        </div>
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="grid"
          style:width="{widthPx}px"
          style:height="{128 * row}px"
          style:--beat="{zoom}px"
          style:--bar="{zoom * barLen}px"
          style:--row="{row}px"
          onpointerdown={onGridDown}
          ondblclick={deleteAt}
          oncontextmenu={deleteAt}
        >
          {#each pitches as p (p)}
            {#if isBlackKey(p)}<div class="row-black" style:top="{yOf(p)}px" style:height="{row}px"></div>{/if}
          {/each}
          <div class="past" style:left="{xOf(clip.length)}px" title="Past the clip's end: notes here are not played. Lengthen the clip to hear them."></div>
          <div class="end" style:left="{xOf(clip.length)}px" title="The clip ends here"></div>
          {#if cursor !== null}<div class="cursor" style:left="{xOf(cursor)}px" title="Paste goes here"></div>{/if}
          {#if box}
            <div class="box" style:left="{Math.min(box.x0, box.x1)}px" style:top="{Math.min(box.y0, box.y1)}px" style:width="{Math.abs(box.x1 - box.x0)}px" style:height="{Math.abs(box.y1 - box.y0)}px"></div>
          {/if}
          {#each clip.notes as n, i (i)}
            <div
              class="note"
              class:picked={picked.has(i)}
              class:past={n.start >= clip.length}
              style:left="{xOf(n.start)}px"
              style:top="{yOf(n.pitch) + 1}px"
              style:width="{Math.max(3, n.length * zoom - 1)}px"
              style:height="{row - 2}px"
              style:--vel={n.velocity / 127}
              title="{noteName(n.pitch)}, velocity {n.velocity}"
            >
              <span class="h l"></span><span class="h r"></span>
            </div>
          {/each}
        </div>
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="velocity" style:width="{widthPx}px" onpointerdown={onVelocityDown} title="Velocity: drag a bar up or down">
          {#each clip.notes as n, i (i)}
            <div class="bar" class:picked={picked.has(i)} style:left="{xOf(n.start)}px" style:height="{(n.velocity / 127) * 100}%"></div>
          {/each}
        </div>
      </div>
    </div>

    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="zoombar" bind:this={zoomBar} onpointerdown={(e) => onZoomBarDown(e, "track")} ondblclick={fit} title="The part of the clip in view. Drag to scroll, drag an end to zoom, double-click to fit the clip.">
      <div
        class="thumb"
        style:left="{(Math.max(0, shownFrom) / Math.max(totalBeats, 1e-9)) * 100}%"
        style:width="{Math.min(1, (shownTo - shownFrom) / Math.max(totalBeats, 1e-9)) * 100}%"
        onpointerdown={(e) => onZoomBarDown(e, "move")}
      >
        <span class="edge s" onpointerdown={(e) => onZoomBarDown(e, "start")}></span>
        <span class="edge e" onpointerdown={(e) => onZoomBarDown(e, "end")}></span>
      </div>
    </div>

    <footer>
      <span class="note-text">
        {clip.notes.length}
        {clip.notes.length === 1 ? "note" : "notes"}{instrument ? `. Plays on ${instrument}.` : ". Pick an instrument in the track header to hear them."} Click to add. Drag the middle to move, an end to stretch. Shift-click or Shift-drag to pick several. Double-click to delete.
      </span>
      <button type="button" onclick={onClose}>Cancel</button>
      <button type="button" class="primary" onclick={done}>Done</button>
    </footer>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 5000;
    display: grid;
    place-items: center;
    background: rgba(0, 0, 0, 0.5);
  }
  .dialog {
    display: grid;
    grid-template-rows: auto 1fr auto auto;
    gap: 8px;
    width: min(1100px, 96vw);
    height: min(720px, 90vh);
    padding: 12px;
    border: 1px solid var(--border-strong);
    background: var(--surface-2);
    box-shadow: 0 16px 40px rgba(0, 0, 0, 0.6);
  }
  header,
  footer {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    align-items: center;
  }
  footer {
    justify-content: flex-end;
  }
  h1 {
    margin: 0 8px 0 0;
    font-size: 14px;
    font-weight: 600;
  }
  label {
    display: flex;
    gap: 5px;
    align-items: center;
    color: var(--text-dim);
    font-size: var(--font-size-small);
  }
  input,
  select,
  button {
    height: 24px;
    box-sizing: border-box;
    padding: 0 8px;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    background: var(--surface-1);
    color: var(--text);
    font: inherit;
  }
  input[type="number"] {
    width: 64px;
  }
  input.name {
    width: 140px;
  }
  button {
    background: var(--surface-3);
  }
  .primary {
    border-color: var(--accent);
  }
  .zoom {
    display: flex;
    gap: 2px;
  }
  .roll {
    display: flex;
    min-height: 0;
    overflow-x: hidden;
    overflow-y: auto;
    border: 1px solid var(--border);
    background: var(--surface-1);
  }
  .keys {
    position: sticky;
    left: 0;
    z-index: 2;
    flex: none;
    width: 46px;
    background: var(--surface-3);
  }
  .keys-corner {
    position: sticky;
    top: 0;
    z-index: 3;
    border-bottom: 1px solid var(--border-strong);
    background: var(--surface-2);
  }
  .ruler {
    position: sticky;
    top: 0;
    z-index: 2;
    box-sizing: border-box;
    border-bottom: 1px solid var(--border-strong);
    background: var(--surface-2);
    cursor: zoom-in;
    overflow: hidden;
  }
  .ruler .tick {
    position: absolute;
    top: 0;
    bottom: 0;
    padding-left: 3px;
    border-left: 1px solid color-mix(in srgb, var(--text) 18%, transparent);
    color: var(--text-dim);
    font-family: var(--font-mono);
    font-size: 10px;
    line-height: 17px;
    white-space: nowrap;
  }
  .ruler .tick:not(.labeled) {
    top: 10px;
  }
  .zoombar {
    position: relative;
    height: 14px;
    border: 1px solid var(--border);
    border-radius: 3px;
    background: var(--surface-1);
    cursor: pointer;
  }
  .zoombar .thumb {
    position: absolute;
    top: 1px;
    bottom: 1px;
    min-width: 14px;
    box-sizing: border-box;
    border: 1px solid var(--border-strong);
    border-radius: 3px;
    background: color-mix(in srgb, var(--accent) 30%, var(--surface-3));
    cursor: grab;
    touch-action: none;
  }
  .zoombar .edge {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 6px;
    background: color-mix(in srgb, var(--text) 25%, transparent);
    cursor: ew-resize;
  }
  .zoombar .edge.s {
    left: 0;
    border-radius: 2px 0 0 2px;
  }
  .zoombar .edge.e {
    right: 0;
    border-radius: 0 2px 2px 0;
  }
  .zoom button svg {
    width: 12px;
    height: 12px;
    vertical-align: middle;
  }
  .key {
    cursor: pointer;
    box-sizing: border-box;
    padding-left: 4px;
    border-bottom: 1px solid var(--border);
    background: #e8e8e8;
    color: #333;
    font-size: 9px;
    line-height: 1;
    display: flex;
    align-items: center;
    overflow: hidden;
  }
  .key.black {
    background: #2a2a2a;
  }
  .key.c {
    border-bottom-color: #888;
  }
  .area {
    position: relative;
    flex: none;
  }
  .grid {
    position: relative;
    background-image:
      linear-gradient(to right, color-mix(in srgb, var(--text) 22%, transparent) 1px, transparent 1px),
      linear-gradient(to right, color-mix(in srgb, var(--text) 8%, transparent) 1px, transparent 1px),
      linear-gradient(to bottom, color-mix(in srgb, var(--text) 6%, transparent) 1px, transparent 1px);
    background-size:
      var(--bar) 100%,
      var(--beat) 100%,
      100% var(--row);
    cursor: crosshair;
  }
  .row-black {
    position: absolute;
    right: 0;
    left: 0;
    background: color-mix(in srgb, black 18%, transparent);
    pointer-events: none;
  }
  .end {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 2px;
    background: var(--accent);
    pointer-events: none;
  }
  .past {
    position: absolute;
    top: 0;
    right: 0;
    bottom: 0;
    background: rgba(0, 0, 0, 0.3);
    pointer-events: none;
  }
  .cursor {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 1px;
    background: color-mix(in srgb, var(--text) 55%, transparent);
    pointer-events: none;
  }
  .box {
    position: absolute;
    border: 1px dashed var(--text);
    background: color-mix(in srgb, var(--accent) 15%, transparent);
    pointer-events: none;
  }
  .note {
    position: absolute;
    box-sizing: border-box;
    border: 1px solid color-mix(in srgb, var(--note) 70%, black);
    border-radius: 2px;
    background: color-mix(in srgb, var(--note) calc(45% + 55% * var(--vel)), var(--surface-1));
    cursor: grab;
  }
  /* The ends stretch the note (the grid works out which end from where the pointer is). */
  .note .h {
    position: absolute;
    top: 0;
    bottom: 0;
    width: min(6px, 33%);
    cursor: ew-resize;
  }
  .note .l {
    left: 0;
  }
  .note .r {
    right: 0;
  }
  .note.picked {
    border-color: var(--text);
    box-shadow: 0 0 0 1px var(--text);
  }
  .note.past {
    opacity: 0.35;
  }
  .velocity {
    position: sticky;
    bottom: 0;
    z-index: 1;
    height: 60px;
    border-top: 1px solid var(--border-strong);
    background: var(--surface-2);
    cursor: ns-resize;
  }
  .bar {
    position: absolute;
    bottom: 0;
    width: 4px;
    background: var(--note);
  }
  .bar.picked {
    background: var(--text);
  }
  .note-text {
    flex: 1;
    color: var(--text-dim);
    font-size: var(--font-size-small);
  }
</style>
