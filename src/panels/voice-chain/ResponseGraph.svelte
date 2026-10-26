<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  The response of an EQ or filter in the Audio FX chain: how much each
  frequency is turned up or down, measured from the effect itself. The
  Curve EQ's points can be dragged on the graph (left and right for
  frequency, up and down for gain).
-->
<script lang="ts">
  import { effectResponse, type EffectInfo, type EffectParamInfo } from "../../app/core";
  import type { EffectSlotDoc } from "../../stores/session";
  import { paramValue } from "./chain";
  import { dbAt, freqAt, handles, range, xOf, yOf } from "./graph";

  interface Props {
    slot: EffectSlotDoc;
    info: EffectInfo;
    /** A point moved: both of its controls at once. `done` on release. */
    onPoint?: (freq: EffectParamInfo, f: number, gain: EffectParamInfo, g: number, done: boolean) => void;
  }

  const { slot, info, onPoint }: Props = $props();

  const W = 300;
  const H = 110;
  let points = $state<[number, number][]>([]);
  const key = $derived(JSON.stringify(slot.params));
  let timer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    void key;
    const s = $state.snapshot(slot) as EffectSlotDoc;
    clearTimeout(timer);
    timer = setTimeout(() => {
      effectResponse(s, 160)
        .then((p) => (points = p))
        .catch(() => (points = []));
    }, 40);
    return () => clearTimeout(timer);
  });

  const r = $derived(range(points, info));
  const path = $derived(points.map(([f, db], i) => `${i ? "L" : "M"}${(xOf(f) * W).toFixed(1)},${(yOf(db, r[0], r[1]) * H).toFixed(1)}`).join(" "));
  const grid = [100, 1000, 10_000];
  const dbLines = $derived([r[0], 12, 0, -12, r[1]].filter((v, i, a) => a.indexOf(v) === i && v <= r[0] && v >= r[1]));
  const dots = $derived(handles(info).map((h) => ({ ...h, f: paramValue(slot, h.freq), g: paramValue(slot, h.gain) })));

  let svg = $state<SVGSVGElement | null>(null);
  let dragging = $state<string | null>(null);
  function pointer(e: PointerEvent, h: (typeof dots)[number], done: boolean) {
    if (!svg) return;
    const b = svg.getBoundingClientRect();
    const f = freqAt((e.clientX - b.left) / b.width);
    const g = dbAt((e.clientY - b.top) / b.height, r[0], r[1]);
    onPoint?.(h.freq, f, h.gain, Math.max(h.gain.min, Math.min(h.gain.max, g)), done);
  }
</script>

<svg bind:this={svg} class="graph" viewBox="0 0 {W} {H}" preserveAspectRatio="none" role="img" aria-label="{info.name} response">
  {#each grid as f (f)}<line class="grid" x1={xOf(f) * W} x2={xOf(f) * W} y1="0" y2={H} />{/each}
  {#each dbLines as d (d)}<line class="grid" class:zero={d === 0} x1="0" x2={W} y1={yOf(d, r[0], r[1]) * H} y2={yOf(d, r[0], r[1]) * H} />{/each}
  {#if path}<path class="curve" d={path} />{/if}
  {#each dots as h (h.name)}
    <circle
      class="dot"
      class:on={dragging === h.name}
      cx={xOf(h.f) * W}
      cy={yOf(h.g, r[0], r[1]) * H}
      r="4"
      role="slider"
      tabindex="-1"
      aria-label={h.name}
      aria-valuenow={h.g}
      onpointerdown={(e) => {
        dragging = h.name;
        (e.currentTarget as Element).setPointerCapture(e.pointerId);
      }}
      onpointermove={(e) => dragging === h.name && pointer(e, h, false)}
      onpointerup={(e) => {
        if (dragging === h.name) pointer(e, h, true);
        dragging = null;
      }}
    ><title>{h.name}</title></circle>
  {/each}
</svg>
<div class="scale" aria-hidden="true">
  <span>100</span><span>1k</span><span>10k</span>
</div>

<style>
  .graph {
    display: block;
    width: 100%;
    height: 110px;
    background: var(--surface-0);
    border: 1px solid var(--border);
    touch-action: none;
  }
  .grid {
    stroke: var(--border);
    stroke-width: 1;
    vector-effect: non-scaling-stroke;
  }
  .zero {
    stroke: var(--border-strong);
  }
  .curve {
    fill: none;
    stroke: var(--accent);
    stroke-width: 1.5;
    vector-effect: non-scaling-stroke;
  }
  .dot {
    fill: var(--surface-2);
    stroke: var(--accent);
    stroke-width: 1.5;
    vector-effect: non-scaling-stroke;
    cursor: grab;
  }
  .dot.on {
    fill: var(--accent);
    cursor: grabbing;
  }
  .scale {
    position: relative;
    height: 12px;
    color: var(--text-faint);
    font-size: 10px;
  }
  .scale span {
    position: absolute;
    transform: translateX(-50%);
  }
  .scale span:nth-child(1) {
    left: 33.3%;
  }
  .scale span:nth-child(2) {
    left: 66.7%;
  }
  .scale span:nth-child(3) {
    left: 99%;
    transform: translateX(-100%);
  }
</style>
