<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  The tempo strip under the Multitrack ruler: a flag at each tempo and
  time signature change. Drag a flag along the bars to move it,
  double-click it to change its tempo or time signature, right-click it
  for more. Double-click an empty spot to add a change at the nearest bar.
-->
<script lang="ts">
  import type { TempoDoc } from "../../stores/session";
  import { addChange, barStart, beatsToSeconds, bpmText, meterText, moveChange, nearestBar, parseMeter, removeChange, secondsToBeats, segments, editSegment, BPM_MAX, BPM_MIN, type TempoSegment } from "../../stores/tempo";
  import type { Viewport } from "./timeline";
  import type { ContextItem } from "../../app/ContextMenu.svelte";

  interface Props {
    tempo: TempoDoc;
    view: Viewport;
    width: number;
    rate: number;
    onChange: (t: TempoDoc, label: string) => void;
    onMenu: (e: MouseEvent, items: ContextItem[]) => void;
  }

  const { tempo, view, width, rate, onChange, onMenu }: Props = $props();

  const xOfSeconds = (s: number) => (s * rate - view.scroll) / view.spp;
  const secondsAt = (x: number) => (view.scroll + x * view.spp) / rate;
  const flags = $derived(segments(tempo).slice(1));

  let strip = $state<HTMLDivElement | null>(null);
  /** A flag being dragged, and the bar it would land on. */
  let drag = $state<{ id: string; bar: number } | null>(null);
  let editing = $state<{ seg: TempoSegment; x: number; bpm: string; meter: string } | null>(null);

  /** The bar (from 1) nearest the pointer, counted as if change `id` were not there. */
  function barNear(clientX: number, id: string | null): number {
    if (!strip) return 1;
    const t = id ? removeChange(tempo, id) : tempo;
    const x = clientX - strip.getBoundingClientRect().left;
    return Math.max(1, nearestBar(t, secondsToBeats(t, Math.max(0, secondsAt(x)))));
  }

  function xOfBar(bar: number, id: string): number {
    const t = removeChange(tempo, id);
    return xOfSeconds(beatsToSeconds(t, barStart(t, bar)));
  }

  function onFlagDown(e: PointerEvent, seg: TempoSegment) {
    if (e.button !== 0 || !seg.id) return;
    e.stopPropagation();
    const id = seg.id;
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    const startX = e.clientX;
    let moved = false;
    const move = (ev: PointerEvent) => {
      if (!moved && Math.abs(ev.clientX - startX) < 3) return;
      moved = true;
      drag = { id, bar: barNear(ev.clientX, id) };
    };
    const up = () => {
      el.removeEventListener("pointermove", move);
      el.removeEventListener("pointerup", up);
      el.removeEventListener("pointercancel", up);
      const d = drag;
      drag = null;
      if (moved && d && d.bar !== seg.bar) onChange(moveChange(tempo, id, d.bar), "Move Tempo Change");
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up);
    el.addEventListener("pointercancel", up);
  }

  function openEdit(seg: TempoSegment, x: number) {
    editing = { seg, x: Math.max(0, Math.min(width - 190, x)), bpm: bpmText(seg.bpm), meter: meterText(seg) };
  }

  function applyEdit() {
    const ed = editing;
    if (!ed) return;
    editing = null;
    const bpm = Number(ed.bpm.replace(",", "."));
    const meter = parseMeter(ed.meter);
    if (!Number.isFinite(bpm) || !meter) return;
    const b = Math.min(BPM_MAX, Math.max(BPM_MIN, Math.round(bpm * 100) / 100));
    if (b === ed.seg.bpm && meter.beats_per_bar === ed.seg.beats_per_bar && meter.beat_unit === ed.seg.beat_unit) return;
    onChange(editSegment(tempo, ed.seg, { bpm: b, ...meter }), "Tempo Change");
  }

  function add(clientX: number) {
    const bar = barNear(clientX, null);
    const r = addChange(tempo, bar, crypto.randomUUID());
    if (r.tempo !== tempo) onChange(r.tempo, "Add Tempo Change");
    const seg = segments(r.tempo).find((s) => s.id === r.id);
    if (seg && strip) openEdit(seg, xOfSeconds(seg.seconds));
  }

  function flagMenu(e: MouseEvent, seg: TempoSegment) {
    e.preventDefault();
    e.stopPropagation();
    const id = seg.id;
    if (!id) return;
    onMenu(e, [
      { kind: "item", label: "Edit Tempo and Time Signature...", run: () => openEdit(seg, xOfSeconds(seg.seconds)) },
      { kind: "item", label: "Delete Tempo Change", run: () => onChange(removeChange(tempo, id), "Delete Tempo Change") },
    ]);
  }

  function stripMenu(e: MouseEvent) {
    e.preventDefault();
    const x = e.clientX;
    onMenu(e, [
      { kind: "item", label: "Add Tempo Change Here", run: () => add(x) },
      ...(flags.length
        ? ([{ kind: "item", label: "Delete All Tempo Changes", run: () => onChange({ ...tempo, changes: [] }, "Delete Tempo Changes") }] as ContextItem[])
        : []),
    ]);
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="strip"
  bind:this={strip}
  style:width="{width}px"
  ondblclick={(e) => add(e.clientX)}
  oncontextmenu={stripMenu}
  title="Tempo and time signature changes. Double-click to add one at the nearest bar."
>
  {#each flags as seg (seg.id)}
    {@const x = drag?.id === seg.id ? xOfBar(drag.bar, seg.id!) : xOfSeconds(seg.seconds)}
    {#if x > -120 && x < width + 4}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="flag"
        class:dragging={drag?.id === seg.id}
        style:left="{x}px"
        title="Bar {(drag?.id === seg.id ? drag.bar : seg.bar) + 1}: {bpmText(seg.bpm)} BPM, {meterText(seg)}. Drag to move it to another bar, double-click to change it."
        onpointerdown={(e) => onFlagDown(e, seg)}
        ondblclick={(e) => {
          e.stopPropagation();
          openEdit(seg, x);
        }}
        oncontextmenu={(e) => flagMenu(e, seg)}
      >
        <span class="bpm">{bpmText(seg.bpm)}</span>
        <span class="meter">{meterText(seg)}</span>
      </div>
    {/if}
  {/each}
  {#if editing}
    {@const ed = editing}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="edit"
      style:left="{ed.x}px"
      ondblclick={(e) => e.stopPropagation()}
      onpointerdown={(e) => e.stopPropagation()}
      onkeydown={(e) => {
        e.stopPropagation();
        if (e.key === "Enter") applyEdit();
        if (e.key === "Escape") editing = null;
      }}
    >
      <label>BPM <input aria-label="Tempo in beats per minute" bind:value={ed.bpm} size="6" /></label>
      <label>Time <input aria-label="Time signature" bind:value={ed.meter} size="4" title="Time signature, such as 4/4, 3/4 or 6/8" /></label>
      <button type="button" onclick={applyEdit}>OK</button>
      <button type="button" onclick={() => (editing = null)}>Cancel</button>
    </div>
  {/if}
</div>

<style>
  .strip {
    position: relative;
    flex: none;
    height: 16px;
    border-bottom: 1px solid var(--border);
    background: var(--surface-1);
    overflow: visible;
  }
  .flag {
    position: absolute;
    top: 1px;
    bottom: 1px;
    display: flex;
    gap: 4px;
    align-items: center;
    padding: 0 5px 0 4px;
    border-left: 2px solid var(--accent);
    border-radius: 0 3px 3px 0;
    background: color-mix(in srgb, var(--accent) 20%, var(--surface-2));
    color: var(--text);
    font-family: var(--font-mono);
    font-size: 10px;
    line-height: 1;
    white-space: nowrap;
    cursor: ew-resize;
    user-select: none;
    touch-action: none;
  }
  .flag.dragging {
    opacity: 0.75;
    outline: 1px dashed var(--accent);
  }
  .flag .meter {
    color: var(--text-dim);
  }
  .edit {
    position: absolute;
    top: 17px;
    z-index: 20;
    display: flex;
    gap: 6px;
    align-items: center;
    padding: 6px 8px;
    border: 1px solid var(--border-strong);
    border-radius: 4px;
    background: var(--surface-2);
    box-shadow: 0 4px 14px rgba(0, 0, 0, 0.45);
    font-size: 11px;
    white-space: nowrap;
  }
  .edit label {
    display: flex;
    gap: 4px;
    align-items: center;
    color: var(--text-dim);
  }
  .edit input {
    width: 52px;
    font-family: var(--font-mono);
  }
</style>
