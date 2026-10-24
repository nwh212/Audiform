<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Altered Audio Editor.

  A MIDI clip's notes, on a piano roll. Click empty space to add a note
  one grid step long; drag a note to move it, drag its right edge to
  change its length, and double-click or right-click it to delete it.
  Shift-click picks several. The velocity lane below sets how hard each
  note plays. Done (or Escape) keeps the changes as one undoable edit;
  Cancel drops them.
-->
<script lang="ts">
  import type { MidiClipDoc, NoteDoc, TempoDoc } from "../../stores/session";
  import { GRIDS, MIN_NOTE, addNote, gridBeats, isBlackKey, moveNotes, noteName, pitchRange, removeNotes, resizeNotes, setVelocity, snap, snapDown } from "./midi";

  interface Props {
    clip: MidiClipDoc;
    tempo: TempoDoc;
    /** The instrument's name, for the title. */
    instrument: string | null;
    onApply: (clip: MidiClipDoc) => void;
    onClose: () => void;
  }

  const { clip: original, tempo, instrument, onApply, onClose }: Props = $props();

  // svelte-ignore state_referenced_locally
  let clip = $state<MidiClipDoc>(structuredClone($state.snapshot(original)) as MidiClipDoc);
  let picked = $state<Set<number>>(new Set());
  let grid = $state(0.25);
  let zoom = $state(64);
  /** Length (beats) of the next note added: the last one drawn or resized. */
  let drawLength = $state(0.25);

  const ROW = 14;
  const KEYS = 56;
  const step = $derived(gridBeats(grid, tempo));
  const changed = $derived(JSON.stringify($state.snapshot(clip)) !== JSON.stringify(original));
  const bars = $derived(Math.ceil(clip.length / tempo.beats_per_bar));
  const widthPx = $derived(Math.max(clip.length, bars * tempo.beats_per_bar) * zoom);

  let scroller = $state<HTMLDivElement | null>(null);
  $effect(() => {
    if (!scroller) return;
    // Open on the notes there are, or around middle C.
    const [lo, hi] = pitchRange(original.notes);
    const mid = (lo + hi) / 2;
    scroller.scrollTop = (127 - mid) * ROW - scroller.clientHeight / 2;
    // And zoomed so the whole clip fits.
    const room = scroller.clientWidth - 60;
    zoom = Math.min(160, Math.max(16, room / Math.max(1, original.length)));
  });

  const yOf = (pitch: number) => (127 - pitch) * ROW;
  const xOf = (beat: number) => beat * zoom;

  /** Where a pointer is on the grid: beat (unsnapped) and pitch. */
  function at(e: MouseEvent, el: HTMLElement): { beat: number; pitch: number } {
    const r = el.getBoundingClientRect();
    const beat = Math.max(0, (e.clientX - r.left) / zoom);
    const pitch = Math.min(127, Math.max(0, 127 - Math.floor((e.clientY - r.top) / ROW)));
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

  function onGridDown(e: PointerEvent) {
    if (e.button !== 0) return;
    const el = e.currentTarget as HTMLElement;
    const p = at(e, el);
    let i = noteAt(p.beat, p.pitch);
    const startX = e.clientX;
    const startY = e.clientY;
    let mode: "move" | "size";
    if (i < 0) {
      // A new note, one grid step (or the last length drawn), on the grid.
      const n: NoteDoc = { pitch: p.pitch, velocity: 100, start: snapDown(p.beat, step), length: step > 0 ? Math.max(step, drawLength) : drawLength };
      const notes = addNote(clip.notes, n);
      clip = { ...clip, notes };
      i = notes.findIndex((x) => x.start === n.start && x.pitch === n.pitch);
      picked = new Set([i]);
      mode = "size";
    } else {
      const n = clip.notes[i]!;
      const edgePx = xOf(n.start + n.length) - (e.clientX - el.getBoundingClientRect().left);
      mode = edgePx <= 5 ? "size" : "move";
      if (e.shiftKey) {
        const next = new Set(picked);
        if (next.has(i)) next.delete(i);
        else next.add(i);
        picked = next;
        return;
      }
      if (!picked.has(i)) picked = new Set([i]);
    }
    const base = clip.notes;
    const sel = new Set(picked);
    const move = (ev: PointerEvent) => {
      const dBeats = (ev.clientX - startX) / zoom;
      if (mode === "move") {
        const d = step > 0 ? snap(dBeats, step) : dBeats;
        const ds = -Math.round((ev.clientY - startY) / ROW);
        clip = { ...clip, notes: moveNotes(base, sel, d, ds) };
      } else {
        const n = base[[...sel][0] ?? 0]!;
        const end = n.start + n.length + dBeats;
        const target = step > 0 ? Math.max(n.start + step, snap(end, step)) : Math.max(n.start + MIN_NOTE, end);
        clip = { ...clip, notes: resizeNotes(base, sel, target - (n.start + n.length)) };
      }
    };
    const up = () => {
      window.removeEventListener("pointermove", move, true);
      window.removeEventListener("pointerup", up, true);
      const keep = [...sel].map((k) => clip.notes[k]!).filter(Boolean);
      if (mode === "size" && keep[0]) drawLength = keep[0].length;
      setNotes(clip.notes, keep);
    };
    window.addEventListener("pointermove", move, true);
    window.addEventListener("pointerup", up, true);
  }

  function deleteAt(e: MouseEvent) {
    e.preventDefault();
    const p = at(e, e.currentTarget as HTMLElement);
    const i = noteAt(p.beat, p.pitch);
    if (i >= 0) setNotes(removeNotes(clip.notes, new Set([i])));
  }

  function deletePicked() {
    if (picked.size) setNotes(removeNotes(clip.notes, picked));
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
    const up = () => {
      window.removeEventListener("pointermove", set, true);
      window.removeEventListener("pointerup", up, true);
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
    }
  }

  const pitches = Array.from({ length: 128 }, (_, k) => 127 - k);
</script>

<svelte:window onkeydown={onKey} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={done}>
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="pr-title" tabindex="-1" onclick={(e) => e.stopPropagation()}>
    <header>
      <h1 id="pr-title">Edit Notes</h1>
      <label>Name <input type="text" class="name" bind:value={clip.name} /></label>
      <label title="The clip's length in beats. Notes past the end are not played.">
        Length
        <input type="number" min="0.25" step="1" value={clip.length} onchange={(e) => (clip = { ...clip, length: Math.max(0.25, Number(e.currentTarget.value) || clip.length) })} />
        beats
      </label>
      <label>
        Grid
        <select bind:value={grid}>
          {#each GRIDS as g (g.beats)}<option value={g.beats}>{g.label}</option>{/each}
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
      <span class="zoom">
        <button type="button" title="Zoom out" aria-label="Zoom out" onclick={() => (zoom = Math.max(12, zoom / 1.5))}>−</button>
        <button type="button" title="Zoom in" aria-label="Zoom in" onclick={() => (zoom = Math.min(400, zoom * 1.5))}>+</button>
      </span>
      <button type="button" disabled={!picked.size} onclick={deletePicked}>Delete Notes</button>
    </header>

    <div class="roll" bind:this={scroller}>
      <div class="keys" style:height="{128 * ROW}px">
        {#each pitches as p (p)}
          <div class="key" class:black={isBlackKey(p)} class:c={p % 12 === 0} style:height="{ROW}px">{p % 12 === 0 ? noteName(p) : ""}</div>
        {/each}
      </div>
      <div class="area">
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="grid"
          style:width="{widthPx}px"
          style:height="{128 * ROW}px"
          style:--beat="{zoom}px"
          style:--bar="{zoom * tempo.beats_per_bar}px"
          style:--row="{ROW}px"
          onpointerdown={onGridDown}
          ondblclick={deleteAt}
          oncontextmenu={deleteAt}
        >
          {#each pitches as p (p)}
            {#if isBlackKey(p)}<div class="row-black" style:top="{yOf(p)}px" style:height="{ROW}px"></div>{/if}
          {/each}
          <div class="end" style:left="{xOf(clip.length)}px" title="The clip ends here"></div>
          {#each clip.notes as n, i (i)}
            <div
              class="note"
              class:picked={picked.has(i)}
              class:past={n.start >= clip.length}
              style:left="{xOf(n.start)}px"
              style:top="{yOf(n.pitch) + 1}px"
              style:width="{Math.max(3, n.length * zoom - 1)}px"
              style:height="{ROW - 2}px"
              style:--vel={n.velocity / 127}
              title="{noteName(n.pitch)}, velocity {n.velocity}"
            ></div>
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

    <footer>
      <span class="note-text">
        {clip.notes.length}
        {clip.notes.length === 1 ? "note" : "notes"}{instrument ? `. Plays on ${instrument}.` : ". Pick an instrument in the track header to hear them."} Click to add, drag to move, drag the right edge to resize, double-click to delete.
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
    grid-template-rows: auto 1fr auto;
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
    overflow: auto;
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
  .key {
    box-sizing: border-box;
    padding-left: 4px;
    border-bottom: 1px solid var(--border);
    background: #e8e8e8;
    color: #333;
    font-size: 9px;
    line-height: 13px;
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
  .note {
    position: absolute;
    box-sizing: border-box;
    border: 1px solid color-mix(in srgb, var(--accent) 70%, black);
    border-radius: 2px;
    background: color-mix(in srgb, var(--accent) calc(45% + 55% * var(--vel)), var(--surface-1));
    cursor: grab;
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
    background: var(--accent);
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
