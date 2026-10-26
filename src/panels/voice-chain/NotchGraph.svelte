<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The Audiform contributors.
  This file is part of Audiform, based on Altered Audio Editor.

  The Notch Filter on a graph: its response curve, with a numbered
  marker per notch. Drag a marker left and right for its frequency, up
  and down for its depth; double-click it to switch that notch on or
  off. Below, a column per notch (1 to 6): frequency, depth and its on
  switch, then the width shared by all of them. Every drag is one
  undoable step.
-->
<script lang="ts">
  import { effectResponse, type EffectInfo, type EffectParamInfo } from "../../app/core";
  import type { EffectSlotDoc } from "../../stores/session";
  import { paramValue } from "./chain";
  import { formatHz } from "./eq-graph";
  import { freqAt, freqTicks, xOf } from "./graph";
  import ValueField from "./ValueField.svelte";

  interface Props {
    slot: EffectSlotDoc;
    info: EffectInfo;
    onParams: (changes: [EffectParamInfo, number][], label: string, done: boolean) => void;
  }

  const { slot, info, onParams }: Props = $props();

  let w = $state(460);
  const H = 220;
  const PAD = { l: 6, r: 34, t: 10, b: 18 };
  const TOP = 6;
  const BOTTOM = -66;
  const plotW = $derived(Math.max(60, w - PAD.l - PAD.r));
  const plotH = H - PAD.t - PAD.b;
  const X = (hz: number) => PAD.l + xOf(hz) * plotW;
  const Y = (db: number) => PAD.t + ((TOP - Math.max(BOTTOM, Math.min(TOP, db))) / (TOP - BOTTOM)) * plotH;
  const hzAt = (x: number) => freqAt((x - PAD.l) / plotW);
  const dbAt = (y: number) => TOP - ((y - PAD.t) / plotH) * (TOP - BOTTOM);
  const clampP = (p: EffectParamInfo, v: number) => Math.max(p.min, Math.min(p.max, v));

  /** The notches, by their groups ("Notch 1" and so on): switch, frequency and depth. */
  const notches = $derived.by(() => {
    const groups = [...new Set(info.params.map((p) => p.group).filter((g): g is string => !!g && g.startsWith("Notch")))];
    return groups.flatMap((g, k) => {
      const of = info.params.filter((p) => p.group === g);
      const on = of.find((p) => p.toggle);
      const freq = of.find((p) => p.unit === "Hz");
      const depth = of.find((p) => p.unit === "dB");
      return on && freq && depth ? [{ name: g, label: String(k + 1), on, freq, depth }] : [];
    });
  });
  const width = $derived(info.params.find((p) => p.key === "width") ?? null);
  const placed = $derived(
    notches.map((n) => {
      const f = paramValue(slot, n.freq);
      const d = paramValue(slot, n.depth);
      return { n, f, d, on: paramValue(slot, n.on) >= 0.5, x: X(f), y: Y(d) };
    }),
  );

  let curve = $state<[number, number][]>([]);
  const key = $derived(JSON.stringify(slot.params));
  let timer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    void key;
    const s = $state.snapshot(slot) as EffectSlotDoc;
    clearTimeout(timer);
    timer = setTimeout(() => {
      effectResponse(s, 400)
        .then((p) => (curve = p))
        .catch(() => (curve = []));
    }, 30);
    return () => clearTimeout(timer);
  });
  const curvePath = $derived(curve.map(([f, db], i) => `${i ? "L" : "M"}${X(f).toFixed(1)},${Y(db).toFixed(1)}`).join(" "));

  let svg = $state<SVGSVGElement | null>(null);
  let active = $state<string | null>(null);
  let hover = $state<string | null>(null);
  type Notch = (typeof notches)[number];

  function dragNotch(e: PointerEvent, n: Notch) {
    if (e.button !== 0 || !svg) return;
    e.preventDefault();
    active = n.name;
    const el = e.currentTarget as SVGElement;
    el.setPointerCapture(e.pointerId);
    const at = (ev: PointerEvent, done: boolean) => {
      const b = svg!.getBoundingClientRect();
      onParams(
        [
          [n.freq, clampP(n.freq, Math.round(hzAt(ev.clientX - b.left)))],
          [n.depth, clampP(n.depth, Math.round(dbAt(ev.clientY - b.top) * 2) / 2)],
        ],
        `${n.name} Point`,
        done,
      );
    };
    const move = (ev: PointerEvent) => at(ev, false);
    const up = (ev: PointerEvent) => {
      el.removeEventListener("pointermove", move);
      el.removeEventListener("pointerup", up);
      at(ev, true);
      active = null;
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up);
  }
  const toggle = (n: Notch, on: boolean) => onParams([[n.on, on ? 0 : 1]], `${n.name} ${on ? "Off" : "On"}`, true);

  const ticks = $derived(freqTicks(plotW));
  const gridDb = [0, -12, -24, -36, -48, -60];
  const tip = $derived.by(() => {
    const p = placed.find((x) => x.n.name === (active ?? hover));
    if (!p) return null;
    const parts = [p.n.name, formatHz(p.f), `${p.d.toFixed(1)} dB`];
    if (!p.on) parts.push("off");
    return { x: Math.min(w - 150, Math.max(4, p.x - 60)), y: Math.max(2, p.y - 30), text: parts.join(" · ") };
  });
</script>

<div class="notchwin">
  <div class="graph" bind:clientWidth={w}>
    <svg bind:this={svg} width={w} height={H} role="img" aria-label="{info.name} response">
      <rect class="bg" x={PAD.l} y={PAD.t} width={plotW} height={plotH} />
      {#each ticks as t (t.f)}
        <line class="grid" class:major={t.major} x1={X(t.f)} x2={X(t.f)} y1={PAD.t} y2={PAD.t + plotH} />
        {#if t.label}<text class="lab" class:strong={t.major} x={X(t.f)} y={H - 5} text-anchor={t.f === 20 ? "start" : t.f === 20_000 ? "end" : "middle"}>{t.label}</text>{/if}
      {/each}
      {#each gridDb as d (d)}
        <line class="grid" class:zero={d === 0} x1={PAD.l} x2={PAD.l + plotW} y1={Y(d)} y2={Y(d)} />
        <text class="lab" x={PAD.l + plotW + 4} y={Y(d) + 3}>{d}</text>
      {/each}
      <text class="lab" x={PAD.l + plotW + 4} y={H - 5}>dB</text>
      {#if curvePath}<path class="curve" d={curvePath} />{/if}
      {#each placed as p (p.n.name)}
        {#if p.on}<line class="stem" x1={p.x} x2={p.x} y1={Y(0)} y2={p.y} />{/if}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <g
          class="node"
          class:off={!p.on}
          class:on={active === p.n.name}
          transform="translate({p.x} {p.on ? p.y : Y(0)})"
          onpointerdown={(e) => dragNotch(e, p.n)}
          ondblclick={() => toggle(p.n, p.on)}
          onpointerenter={() => (hover = p.n.name)}
          onpointerleave={() => (hover = null)}
        >
          <circle class="hit" r="10" />
          <rect x="-5" y="-5" width="10" height="10" />
          <text y="16" text-anchor="middle">{p.n.label}</text>
        </g>
      {/each}
    </svg>
    {#if tip}<div class="tip" style:left="{tip.x}px" style:top="{tip.y}px">{tip.text}</div>{/if}
  </div>

  <div class="table" style:--cols={placed.length}>
    <span class="rowhead">Frequency</span>
    {#each placed as c (c.n.name)}
      <div class="cell" class:dim={!c.on}><ValueField param={c.n.freq} value={c.f} oninput={(v) => onParams([[c.n.freq, v]], `${c.n.name} Frequency`, false)} oncommit={(v) => onParams([[c.n.freq, v]], `${c.n.name} Frequency`, true)} /></div>
    {/each}
    <span class="rowhead">Depth</span>
    {#each placed as c (c.n.name)}
      <div class="cell" class:dim={!c.on}><ValueField param={c.n.depth} value={c.d} oninput={(v) => onParams([[c.n.depth, v]], `${c.n.name} Depth`, false)} oncommit={(v) => onParams([[c.n.depth, v]], `${c.n.name} Depth`, true)} /></div>
    {/each}
    <span class="rowhead">Notch</span>
    {#each placed as c (c.n.name)}
      <div class="cell">
        <button type="button" class="band" class:on={c.on} aria-pressed={c.on} title="{c.n.name}: {c.on ? 'on' : 'off'}" onclick={() => toggle(c.n, c.on)}>{c.n.label}</button>
      </div>
    {/each}
  </div>
  {#if width}
    {@const wp = width}
    <div class="opts">
      <span class="olabel">Width (Q), all notches:</span>
      <ValueField param={wp} value={paramValue(slot, wp)} oninput={(v) => onParams([[wp, v]], `${info.name} Width`, false)} oncommit={(v) => onParams([[wp, v]], `${info.name} Width`, true)} />
      <span class="olabel">Higher is narrower.</span>
    </div>
  {/if}
</div>

<style>
  .notchwin {
    display: grid;
    gap: 8px;
    margin-bottom: 8px;
  }
  .graph {
    position: relative;
    min-width: 0;
    user-select: none;
  }
  svg {
    display: block;
  }
  .bg {
    fill: #0b0b0e;
    stroke: var(--border);
  }
  .grid {
    stroke: rgba(255, 255, 255, 0.06);
  }
  .grid.major {
    stroke: rgba(255, 255, 255, 0.12);
  }
  .grid.zero {
    stroke: rgba(255, 255, 255, 0.25);
  }
  .lab {
    fill: var(--text-faint);
    font-size: 9px;
  }
  .lab.strong {
    fill: var(--text-dim);
    font-weight: 600;
  }
  .curve {
    fill: none;
    stroke: #7d8cff;
    stroke-width: 2;
  }
  .stem {
    stroke: rgba(255, 255, 255, 0.2);
    stroke-dasharray: 2 2;
    pointer-events: none;
  }
  .node {
    cursor: move;
  }
  .node .hit {
    fill: transparent;
  }
  .node rect {
    fill: #ffffff;
    stroke: #0e0e10;
  }
  .node.on rect {
    fill: #7d8cff;
  }
  .node.off rect {
    fill: transparent;
    stroke: rgba(255, 255, 255, 0.6);
  }
  .node text {
    fill: #ffffff;
    font-size: 10px;
    font-weight: 600;
    pointer-events: none;
  }
  .tip {
    position: absolute;
    padding: 2px 6px;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    background: var(--surface-2);
    color: var(--text);
    font-size: var(--font-size-small);
    white-space: nowrap;
    pointer-events: none;
  }
  .table {
    display: grid;
    grid-template-columns: 74px repeat(var(--cols), minmax(0, 1fr));
    gap: 4px 2px;
    align-items: center;
  }
  .rowhead,
  .olabel {
    color: var(--text-dim);
    font-size: var(--font-size-small);
  }
  .rowhead {
    text-align: right;
    padding-right: 6px;
  }
  .cell {
    display: flex;
    justify-content: center;
    min-width: 0;
  }
  .cell.dim {
    opacity: 0.45;
  }
  .cell :global(.vf) {
    width: 100%;
    min-width: 0;
  }
  .band {
    min-width: 30px;
    height: 22px;
    padding: 0 6px;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    background: var(--surface-0);
    color: var(--text-dim);
    font: inherit;
    font-weight: 600;
  }
  .band.on {
    border-color: var(--accent);
    color: var(--accent);
  }
  .opts {
    display: flex;
    gap: 8px;
    align-items: center;
    justify-content: center;
    padding-top: 6px;
    border-top: 1px solid var(--border);
  }
  .opts :global(.vf) {
    width: 80px;
  }
</style>
