<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  Dynamics Curve on a graph: the level going in across, the level coming out up. The diagonal is "no
  change"; a curve below it turns that level down, above it turns it up. Drag a point to move it;
  click empty space to add one (up to eight); double-click or right-click a point to remove it (two
  always stay). Reset puts back the straight line. Every change is one undoable step.
-->
<script lang="ts">
  import type { EffectInfo, EffectParamInfo } from "../../app/core";
  import type { EffectSlotDoc } from "../../stores/session";
  import { addPoint, curveAt, curvePoints, IN_MAX, IN_MIN, inKey, OUT_MAX, OUT_MIN, outKey, removePoint, RESET } from "./dynamics-curve";

  interface Props {
    slot: EffectSlotDoc;
    info: EffectInfo;
    onParams: (changes: [EffectParamInfo, number][], label: string, done: boolean) => void;
  }

  const { slot, info, onParams }: Props = $props();

  let w = $state(360);
  const H = 300;
  const PAD = { l: 30, r: 10, t: 8, b: 20 };
  const plotW = $derived(Math.max(80, w - PAD.l - PAD.r));
  const plotH = H - PAD.t - PAD.b;
  const knee = $derived(slot.params.knee ?? 0);

  const X = (db: number) => PAD.l + ((Math.max(IN_MIN, Math.min(IN_MAX, db)) - IN_MIN) / (IN_MAX - IN_MIN)) * plotW;
  const Y = (db: number) => PAD.t + ((OUT_MAX - Math.max(OUT_MIN, Math.min(OUT_MAX, db))) / (OUT_MAX - OUT_MIN)) * plotH;
  const inAt = (x: number) => IN_MIN + Math.min(1, Math.max(0, (x - PAD.l) / plotW)) * (IN_MAX - IN_MIN);
  const outAt = (y: number) => OUT_MAX - Math.min(1, Math.max(0, (y - PAD.t) / plotH)) * (OUT_MAX - OUT_MIN);

  const pts = $derived(curvePoints(slot));
  const path = $derived.by(() => {
    let d = "";
    for (let i = 0; i <= 200; i++) {
      const x = IN_MIN + ((IN_MAX - IN_MIN) * i) / 200;
      d += `${i ? "L" : "M"}${X(x).toFixed(1)},${Y(curveAt(pts, knee, x)).toFixed(1)}`;
    }
    return d;
  });

  const byKey = $derived(new Map(info.params.map((p) => [p.key, p])));
  /** Control changes by key, as the editor takes them. */
  function send(changes: Record<string, number>, label: string, done: boolean) {
    const list: [EffectParamInfo, number][] = [];
    for (const [k, v] of Object.entries(changes)) {
      const p = byKey.get(k);
      if (p) list.push([p, Math.max(p.min, Math.min(p.max, v))]);
    }
    if (list.length) onParams(list, label, done);
  }

  let svg = $state<SVGSVGElement | null>(null);
  let active = $state<number | null>(null);
  let hover = $state<number | null>(null);
  const local = (e: MouseEvent) => {
    const b = svg!.getBoundingClientRect();
    return { x: e.clientX - b.left, y: e.clientY - b.top };
  };
  const round = (a: number, b: number) => [Math.round(a * 10) / 10, Math.round(b * 10) / 10] as const;

  function dragPoint(e: PointerEvent, index: number) {
    if (e.button !== 0 || !svg) return;
    e.preventDefault();
    e.stopPropagation();
    active = index;
    const el = e.currentTarget as SVGElement;
    el.setPointerCapture(e.pointerId);
    const at = (ev: PointerEvent, done: boolean) => {
      const p = local(ev);
      const [i, o] = round(inAt(p.x), outAt(p.y));
      send({ [inKey(index)]: i, [outKey(index)]: o }, `Curve Point ${index + 1}`, done);
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

  function onBackground(e: MouseEvent) {
    if (!svg) return;
    const p = local(e);
    if (p.x < PAD.l || p.x > PAD.l + plotW || p.y < PAD.t || p.y > PAD.t + plotH) return;
    const [i, o] = round(inAt(p.x), outAt(p.y));
    const ch = addPoint(slot, i, o);
    if (ch) send(ch, "Add Curve Point", true);
  }

  function remove(e: MouseEvent, index: number) {
    e.preventDefault();
    e.stopPropagation();
    const ch = removePoint(slot, index);
    if (ch) send(ch, "Remove Curve Point", true);
  }

  const grid = [-100, -80, -60, -40, -20, 0];
  const gridOut = [-100, -80, -60, -40, -20, 0];
  const tip = $derived.by(() => {
    const p = pts.find((x) => x.index === (active ?? hover));
    if (!p) return null;
    return { x: Math.min(w - 140, Math.max(4, X(p.inDb) + 8)), y: Math.max(2, Y(p.outDb) - 28), text: `in ${p.inDb.toFixed(1)} dB · out ${p.outDb.toFixed(1)} dB` };
  });
</script>

<div class="dc" bind:clientWidth={w}>
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions, a11y_no_noninteractive_element_interactions -->
  <svg bind:this={svg} width={w} height={H} role="img" aria-label="Dynamics Curve: level out by level in" onclick={onBackground}>
    <rect class="bg" x={PAD.l} y={PAD.t} width={plotW} height={plotH} />
    {#each grid as g (g)}
      <line class="grid" x1={X(g)} x2={X(g)} y1={PAD.t} y2={PAD.t + plotH} />
      <text class="lab" x={X(g)} y={H - 6} text-anchor="middle">{g}</text>
    {/each}
    {#each gridOut as g (g)}
      <line class="grid" x1={PAD.l} x2={PAD.l + plotW} y1={Y(g)} y2={Y(g)} />
      <text class="lab" x={PAD.l - 4} y={Y(g) + 3} text-anchor="end">{g}</text>
    {/each}
    <line class="diag" x1={X(IN_MIN)} y1={Y(IN_MIN)} x2={X(IN_MAX)} y2={Y(IN_MAX)} />
    <path class="curve" d={path} />
    {#each pts as p (p.index)}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <g
        class="pt"
        class:on={active === p.index}
        transform="translate({X(p.inDb)} {Y(p.outDb)})"
        onpointerdown={(e) => dragPoint(e, p.index)}
        onclick={(e) => e.stopPropagation()}
        ondblclick={(e) => remove(e, p.index)}
        oncontextmenu={(e) => remove(e, p.index)}
        onpointerenter={() => (hover = p.index)}
        onpointerleave={() => (hover = null)}
      >
        <circle class="hit" r="9" />
        <rect x="-4" y="-4" width="8" height="8" />
      </g>
    {/each}
  </svg>
  {#if tip}<div class="tip" style:left="{tip.x}px" style:top="{tip.y}px">{tip.text}</div>{/if}
  <div class="row">
    <button type="button" title="A straight line: the sound is not changed" onclick={() => send(RESET, "Reset Dynamics Curve", true)}>Reset</button>
    <span class="hint">Drag a point. Click to add. Double-click a point to remove it.</span>
  </div>
</div>

<style>
  .dc {
    position: relative;
    width: 100%;
    margin-bottom: 6px;
    user-select: none;
  }
  svg {
    display: block;
    cursor: crosshair;
  }
  .bg {
    fill: #0b0b0e;
    stroke: var(--border);
  }
  .grid {
    stroke: rgba(255, 255, 255, 0.08);
  }
  .diag {
    stroke: rgba(255, 255, 255, 0.28);
    stroke-dasharray: 4 4;
  }
  .lab {
    fill: var(--text-faint);
    font-size: 9px;
  }
  .curve {
    fill: none;
    stroke: #d8964f;
    stroke-width: 2;
    pointer-events: none;
  }
  .pt {
    cursor: move;
  }
  .pt .hit {
    fill: transparent;
  }
  .pt rect {
    fill: #ffffff;
    stroke: #0e0e10;
  }
  .pt.on rect {
    fill: #d8964f;
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
  .row {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .row button {
    height: 20px;
    padding: 0 8px;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    background: var(--surface-3, var(--surface-2));
    color: var(--text);
    font: inherit;
    font-size: var(--font-size-small);
    cursor: pointer;
  }
  .hint {
    color: var(--text-faint);
    font-size: var(--font-size-small);
  }
</style>
