<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Altered Audio Editor.

  A MIDI track's lane: its clips over a bar and beat grid. Double-click
  empty space for a new one-bar clip; double-click a clip to edit its
  notes. Drag a clip to move it and its right edge to change its length;
  both snap to beats. Drag a clip up or down onto another MIDI track to
  move it there (Alt: a copy); its notes go along and that track's
  instrument plays them. Clips and notes are in beats, so they follow
  the session tempo.
-->
<script lang="ts">
  import type { MidiClipDoc, TempoDoc, TrackDoc } from "../../stores/session";
  import { beatsToSamples, clipAtBeat, moveClips, newClip, pitchRange, resizeClip, samplesToBeats, snap, sorted } from "./midi";
  import { xOf, type Viewport } from "./timeline";

  interface Props {
    track: TrackDoc;
    tempo: TempoDoc;
    rate: number;
    view: Viewport;
    width: number;
    height: number;
    /** The track's color, as CSS. */
    color: string;
    selected: ReadonlySet<string>;
    /** Chooses clips (none for empty space); the track becomes the chosen one. */
    onSelect: (ids: string[]) => void;
    /** Replaces the track's clips as one undoable edit. */
    onSetClips: (clips: MidiClipDoc[], label: string) => void;
    /** Opens a clip's notes. */
    onOpen: (clipId: string) => void;
    /** Right-click: on a clip, or on empty space at a beat. */
    onMenu: (e: MouseEvent, clip: MidiClipDoc | null, beat: number) => void;
    /** Clips dragged onto another MIDI track: moved (or copied) there, `delta` beats later. */
    onTransfer?: (ids: string[], delta: number, toTrack: string, copy: boolean) => void;
    /** The MIDI track a drag is over, for highlighting it; null when none. */
    onDragOver?: (trackId: string | null) => void;
    /** Highlighted as the place a dragged clip would land. */
    dropTarget?: boolean;
  }

  const { track, tempo, rate, view, width, height, color, selected, onSelect, onSetClips, onOpen, onMenu, onTransfer = () => {}, onDragOver = () => {}, dropTarget = false }: Props = $props();

  /** The MIDI track lane under a point, if any. */
  function midiTrackAt(x: number, y: number): string | null {
    return document.elementFromPoint(x, y)?.closest<HTMLElement>("[data-midi-lane]")?.dataset.midiLane ?? null;
  }

  const clips = $derived(track.midi_clips ?? []);
  const xOfBeat = (b: number) => xOf(beatsToSamples(b, tempo, rate), view);
  const beatAt = (x: number) => samplesToBeats(view.scroll + x * view.spp, tempo, rate);
  const pxPerBeat = $derived(beatsToSamples(1, tempo, rate) / view.spp);

  /** A drag in progress: clips moving by `delta` beats, or one clip's new length. */
  let drag = $state<{ ids: Set<string>; delta: number } | null>(null);
  let sizing = $state<{ id: string; length: number } | null>(null);

  function shown(c: MidiClipDoc): { start: number; length: number } {
    if (drag?.ids.has(c.id)) return { start: Math.max(0, c.start + drag.delta), length: c.length };
    if (sizing?.id === c.id) return { start: c.start, length: sizing.length };
    return c;
  }

  function localX(e: MouseEvent, el: HTMLElement) {
    return e.clientX - el.getBoundingClientRect().left;
  }

  const EDGE = 6;

  function onDown(e: PointerEvent) {
    if (e.button !== 0) return;
    const lane = e.currentTarget as HTMLElement;
    const x = localX(e, lane);
    const hit = clipAtBeat(clips, beatAt(x));
    if (!hit) {
      onSelect([]);
      return;
    }
    e.stopPropagation();
    const right = xOfBeat(hit.start + hit.length);
    const startX = e.clientX;
    if (right - x <= EDGE) {
      sizing = { id: hit.id, length: hit.length };
    } else {
      const ids = e.shiftKey || e.ctrlKey || e.metaKey ? new Set([...selected, hit.id]) : selected.has(hit.id) ? new Set(selected) : new Set([hit.id]);
      onSelect([...ids]);
      drag = { ids: new Set([...ids].filter((id) => clips.some((c) => c.id === id))), delta: 0 };
    }
    let over: string | null = null;
    const move = (ev: PointerEvent) => {
      const beats = (ev.clientX - startX) / Math.max(1e-9, pxPerBeat);
      if (sizing) sizing = { ...sizing, length: Math.max(0.25, snap(hit.length + beats, 1) || 1) };
      if (drag) {
        drag = { ...drag, delta: snap(beats, 1) };
        // Over another MIDI track: it lights up as the place the clips land.
        const t = midiTrackAt(ev.clientX, ev.clientY);
        const next = t && t !== track.id ? t : null;
        if (next !== over) {
          over = next;
          onDragOver(over);
        }
      }
    };
    const up = (ev: PointerEvent) => {
      window.removeEventListener("pointermove", move, true);
      window.removeEventListener("pointerup", up, true);
      if (sizing && sizing.length !== hit.length) onSetClips(resizeClip(clips, sizing.id, sizing.length), "Clip Length");
      if (drag && over) onTransfer([...drag.ids], drag.delta, over, ev.altKey);
      else if (drag && drag.delta !== 0) onSetClips(moveClips(clips, drag.ids, drag.delta), drag.ids.size > 1 ? "Move Clips" : "Move Clip");
      if (over) onDragOver(null);
      sizing = null;
      drag = null;
    };
    window.addEventListener("pointermove", move, true);
    window.addEventListener("pointerup", up, true);
  }

  function onDouble(e: MouseEvent) {
    const lane = e.currentTarget as HTMLElement;
    const beat = beatAt(localX(e, lane));
    const hit = clipAtBeat(clips, beat);
    if (hit) {
      onOpen(hit.id);
      return;
    }
    const c = newClip(crypto.randomUUID(), beat, tempo);
    onSetClips(sorted([...clips, c]), "New MIDI Clip");
    onSelect([c.id]);
  }

  function onContext(e: MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
    const beat = beatAt(localX(e, e.currentTarget as HTMLElement));
    const hit = clipAtBeat(clips, beat);
    if (hit && !selected.has(hit.id)) onSelect([hit.id]);
    onMenu(e, hit, beat);
  }

  /** Bar and beat lines, drawn as a background that scrolls with the view. */
  const grid = $derived.by(() => {
    const beat = pxPerBeat;
    const bar = beat * Math.max(1, tempo.beats_per_bar);
    const offset = -((view.scroll / view.spp) % bar);
    return { beat, bar, offset, beats: beat >= 6 };
  });

  function noteBoxes(c: MidiClipDoc) {
    const [lo, hi] = pitchRange(c.notes);
    const span = Math.max(12, hi - lo + 1);
    const low = lo - Math.floor((span - (hi - lo + 1)) / 2);
    return c.notes
      .filter((n) => n.start < c.length)
      .map((n) => ({
        x: (n.start / c.length) * 100,
        w: Math.max(0.4, (Math.min(n.length, c.length - n.start) / c.length) * 100),
        y: (1 - (n.pitch - low + 1) / span) * 100,
        h: 100 / span,
      }));
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="midi-lane"
  class:drop={dropTarget}
  data-midi-lane={track.id}
  style:height="{height}px"
  style:--grid-bar="{grid.bar}px"
  style:--grid-beat="{grid.beat}px"
  style:--grid-x="{grid.offset}px"
  style:--track-color={color}
  class:beats={grid.beats}
  onpointerdown={onDown}
  ondblclick={onDouble}
  oncontextmenu={onContext}
>
  {#each clips as c (c.id)}
    {@const s = shown(c)}
    {@const left = xOfBeat(s.start)}
    {@const w = xOfBeat(s.start + s.length) - left}
    {#if left + w >= 0 && left <= width}
      <div
        class="clip"
        class:selected={selected.has(c.id)}
        class:muted={!!c.muted}
        style:left="{left}px"
        style:width="{Math.max(2, w)}px"
        style:--clip-color={c.color ? (c.color.startsWith("--") ? `var(${c.color})` : c.color) : color}
        title="{c.name}: {c.notes.length} {c.notes.length === 1 ? 'note' : 'notes'}. Double-click to edit the notes."
      >
        <div class="title">{c.name}{c.muted ? " (muted)" : ""}</div>
        {#if height > 30}
          <svg class="notes" viewBox="0 0 100 100" preserveAspectRatio="none" aria-hidden="true">
            {#each noteBoxes(c) as b, k (k)}
              <rect x={b.x} y={b.y} width={b.w} height={b.h} />
            {/each}
          </svg>
        {/if}
        <div class="edge" title="Drag to change the clip's length"></div>
      </div>
    {/if}
  {/each}
  {#if clips.length === 0 && height > 30}
    <span class="hint">{track.instrument ? "Double-click to make a MIDI clip." : "Pick an instrument in the track header, then double-click here to make a MIDI clip."}</span>
  {/if}
</div>

<style>
  .midi-lane {
    position: relative;
    overflow: hidden;
    box-sizing: border-box;
    border-bottom: 3px solid var(--track-gap);
    background-image: linear-gradient(to right, color-mix(in srgb, var(--text) 13%, transparent) 1px, transparent 1px);
    background-size: var(--grid-bar) 100%;
    background-position: var(--grid-x) 0;
    user-select: none;
  }
  .midi-lane.drop {
    box-shadow: inset 0 0 0 2px var(--accent);
  }
  .midi-lane.beats {
    background-image:
      linear-gradient(to right, color-mix(in srgb, var(--text) 13%, transparent) 1px, transparent 1px),
      linear-gradient(to right, color-mix(in srgb, var(--text) 5%, transparent) 1px, transparent 1px);
    background-size:
      var(--grid-bar) 100%,
      var(--grid-beat) 100%;
    background-position:
      var(--grid-x) 0,
      var(--grid-x) 0;
  }
  .clip {
    position: absolute;
    top: 1px;
    bottom: 1px;
    overflow: hidden;
    box-sizing: border-box;
    border: 1px solid color-mix(in srgb, var(--clip-color) 80%, black);
    border-radius: 2px;
    background: color-mix(in srgb, var(--clip-color) 28%, var(--surface-1));
    cursor: grab;
  }
  .clip.selected {
    border-color: var(--text);
    box-shadow: inset 0 0 0 1px var(--text);
  }
  .clip.muted {
    opacity: 0.45;
  }
  .title {
    padding: 1px 4px;
    overflow: hidden;
    background: color-mix(in srgb, var(--clip-color) 55%, var(--surface-1));
    color: var(--text);
    font-size: 10px;
    line-height: 13px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .notes {
    position: absolute;
    inset: 16px 2px 3px;
    width: calc(100% - 4px);
    height: calc(100% - 19px);
  }
  .notes rect {
    fill: color-mix(in srgb, var(--clip-color) 40%, white);
  }
  .edge {
    position: absolute;
    top: 0;
    right: 0;
    bottom: 0;
    width: 6px;
    cursor: ew-resize;
  }
  .hint {
    position: absolute;
    top: 50%;
    left: 12px;
    transform: translateY(-50%);
    color: var(--text-faint);
    font-size: var(--font-size-small);
    pointer-events: none;
  }
</style>
