<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  A tall slider for an effect's main setting, with its number box under
  it. Drag the handle (or anywhere on the track) up or down (Ctrl for
  fine steps, Shift for coarse), turn the mouse wheel over it, or
  double-click for the default. With `scale`, a ruler of those values
  runs beside the track, and the fill starts from 0 when the range spans
  it (a gain that cuts and boosts fills up or down from 0).
-->
<script lang="ts">
  import type { EffectParamInfo } from "../../app/core";
  import { fromSlider, formatParam, toSlider } from "./chain";
  import ValueField from "./ValueField.svelte";

  interface Props {
    param: EffectParamInfo;
    value: number;
    /** Track height in pixels. */
    height?: number;
    oninput: (v: number) => void;
    oncommit: (v: number) => void;
    /** Values to label on a ruler beside the track. */
    scale?: readonly number[];
  }

  const { param, value, height = 150, oninput, oncommit, scale }: Props = $props();

  const t = $derived(Math.min(1, Math.max(0, toSlider(param, value))));
  /** Where the fill starts: 0 when a ruler is shown and the range spans it, else the bottom. */
  const base = $derived(scale && param.min < 0 && param.max > 0 ? Math.min(1, Math.max(0, toSlider(param, 0))) : 0);
  const ruler = $derived((scale ?? []).map((v) => ({ v, at: Math.min(1, Math.max(0, toSlider(param, v))) })));
  let track = $state<HTMLDivElement | null>(null);
  let drag: { y: number; start: number; last: number } | null = null;

  function down(e: PointerEvent) {
    if (e.button !== 0 || !track) return;
    e.preventDefault();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    // A click on the track jumps there; the handle itself only moves by dragging.
    const onThumb = (e.target as HTMLElement).classList.contains("thumb");
    let start = t;
    if (!onThumb) {
      const r = track.getBoundingClientRect();
      start = 1 - (e.clientY - r.top) / r.height;
      const v = fromSlider(param, start);
      oninput(v);
    }
    drag = { y: e.clientY, start, last: fromSlider(param, start) };
  }
  function move(e: PointerEvent) {
    if (!drag) return;
    const scale = e.ctrlKey ? 0.2 : e.shiftKey ? 3 : 1;
    const v = fromSlider(param, drag.start + ((drag.y - e.clientY) / height) * scale);
    if (v === drag.last) return;
    drag.last = v;
    oninput(v);
  }
  function up() {
    if (!drag) return;
    const v = drag.last;
    drag = null;
    oncommit(v);
  }

  let wheelTimer: ReturnType<typeof setTimeout> | undefined;
  let wheelValue: number | null = null;
  function wheel(e: WheelEvent) {
    e.preventDefault();
    const base = wheelValue ?? value;
    const step = (e.ctrlKey ? 0.002 : e.shiftKey ? 0.05 : 0.01) * (e.deltaY < 0 ? 1 : -1);
    const v = fromSlider(param, toSlider(param, base) + step);
    wheelValue = v;
    oninput(v);
    clearTimeout(wheelTimer);
    wheelTimer = setTimeout(() => {
      const w = wheelValue;
      wheelValue = null;
      if (w !== null) oncommit(w);
    }, 350);
  }
  $effect(() => () => clearTimeout(wheelTimer));
</script>

<div class="vs">
  <span class="name">{param.name}</span>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="track"
    bind:this={track}
    style:height="{height}px"
    role="slider"
    tabindex="0"
    aria-label={param.name}
    aria-valuemin={param.min}
    aria-valuemax={param.max}
    aria-valuenow={value}
    aria-valuetext={formatParam(param, value)}
    aria-orientation="vertical"
    title="Drag up or down (Ctrl for fine steps, Shift for coarse), or use the mouse wheel. Double-click for the default."
    onpointerdown={down}
    onpointermove={move}
    onpointerup={up}
    onpointercancel={() => (drag = null)}
    onwheel={wheel}
    ondblclick={() => oncommit(param.default)}
    onkeydown={(e) => {
      const d = e.key === "ArrowUp" ? 0.01 : e.key === "ArrowDown" ? -0.01 : e.key === "PageUp" ? 0.1 : e.key === "PageDown" ? -0.1 : 0;
      if (!d) return;
      e.preventDefault();
      oncommit(fromSlider(param, t + d));
    }}
  >
    <div class="fill" style:bottom="{Math.min(t, base) * 100}%" style:height="{Math.abs(t - base) * 100}%"></div>
    {#if scale}
      {#each ruler as r (r.v)}
        <div class="tick" style:bottom="{r.at * 100}%"></div>
        <span class="rlab" style:bottom="{r.at * 100}%">{r.v}</span>
      {/each}
    {:else}
      {#each [0.25, 0.5, 0.75] as m (m)}<div class="mark" style:bottom="{m * 100}%"></div>{/each}
    {/if}
    <div class="thumb" style:bottom="calc({t * 100}% - 6px)"></div>
  </div>
  <ValueField {param} {value} {oninput} {oncommit} />
</div>

<style>
  .vs {
    display: grid;
    justify-items: center;
    gap: 4px;
    width: 64px;
  }
  .name {
    color: var(--text-dim);
    font-size: 11px;
  }
  .track {
    position: relative;
    width: 10px;
    border: 1px solid var(--border-strong);
    border-radius: 5px;
    background: rgba(0, 0, 0, 0.45);
    cursor: ns-resize;
    touch-action: none;
  }
  .track:focus-visible {
    outline: 1px solid var(--accent);
    outline-offset: 2px;
  }
  .fill {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    border-radius: 4px;
    background: color-mix(in srgb, var(--accent) 45%, transparent);
    pointer-events: none;
  }
  .mark {
    position: absolute;
    left: -5px;
    width: 4px;
    height: 1px;
    background: var(--border-strong);
    pointer-events: none;
  }
  .tick {
    position: absolute;
    left: -7px;
    width: 5px;
    height: 1px;
    background: var(--text-faint);
    pointer-events: none;
  }
  .rlab {
    position: absolute;
    right: calc(100% + 11px);
    transform: translateY(50%);
    color: var(--text-faint);
    font-size: 9px;
    line-height: 1;
    font-variant-numeric: tabular-nums;
    pointer-events: none;
  }
  .thumb {
    position: absolute;
    left: -9px;
    width: 26px;
    height: 12px;
    border: 1px solid #0e0e10;
    border-radius: 2px;
    background: linear-gradient(#e8e8ec, #a8a8b0);
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.5);
  }
  .thumb::after {
    content: "";
    position: absolute;
    left: 3px;
    right: 3px;
    top: 5px;
    height: 1px;
    background: #2a2a30;
  }
</style>
