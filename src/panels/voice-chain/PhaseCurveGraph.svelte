<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  Phase Curve on a graph: frequency across (logarithmic or linear,
  20 Hz to 20 kHz), the phase turn up and down (±180° or ±360°). Drag a
  point to move it; click the curve or empty space to add one (up to
  eight); right-click or double-click a point to remove it (two always
  stay). Below: Spline Curves, Reset, and the scale, range, channel and
  FFT size. Every change is one undoable step.
-->
<script lang="ts">
  import type { EffectInfo, EffectParamInfo } from "../../app/core";
  import type { EffectSlotDoc } from "../../stores/session";
  import { addPoint, curveAt, curvePoints, freqKey, phaseKey, rangeOf, removePoint } from "./phase-curve";
  import { freqTicks, xOf } from "./graph";
  import { paramValue } from "./chain";
  import type { Preferences } from "../../stores/preferences";

  interface Props {
    slot: EffectSlotDoc;
    info: EffectInfo;
    onParams: (changes: [EffectParamInfo, number][], label: string, done: boolean) => void;
    /** Where the view choices are kept between runs. */
    prefs?: Preferences;
  }

  const { slot, info, onParams, prefs }: Props = $props();

  const LOW = 20;
  const HIGH = 20_000;
  let w = $state(460);
  const H = 300;
  const PAD = { l: 6, r: 40, t: 8, b: 18 };
  // The frequency scale is how it is shown, kept between runs.
  const KEY_LINEAR = "phase-curve.linear";
  // svelte-ignore state_referenced_locally
  let linear = $state(prefs?.raw(KEY_LINEAR) === true);
  $effect(() => prefs?.set(KEY_LINEAR, linear));
  const plotW = $derived(Math.max(60, w - PAD.l - PAD.r));
  const plotH = H - PAD.t - PAD.b;
  const range = $derived(rangeOf(slot));
  const spline = $derived((slot.params.spline ?? 1) >= 0.5);

  const X = (hz: number) => PAD.l + (linear ? Math.max(0, Math.min(HIGH, hz)) / HIGH : xOf(hz)) * plotW;
  const Y = (deg: number) => PAD.t + ((range - deg) / (2 * range)) * plotH;
  const hzAt = (x: number) => {
    const t = Math.min(1, Math.max(0, (x - PAD.l) / plotW));
    return linear ? Math.max(LOW, t * HIGH) : LOW * Math.pow(HIGH / LOW, t);
  };
  const degAt = (y: number) => Math.max(-range, Math.min(range, range - ((y - PAD.t) / plotH) * 2 * range));

  const pts = $derived(curvePoints(slot));
  const path = $derived.by(() => {
    let d = "";
    for (let i = 0; i <= 200; i++) {
      const hz = linear ? Math.max(LOW, (i / 200) * HIGH) : LOW * Math.pow(HIGH / LOW, i / 200);
      d += `${i ? "L" : "M"}${X(hz).toFixed(1)},${Y(curveAt(pts, spline, hz)).toFixed(1)}`;
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
  const round = (hz: number, deg: number) => [Math.round(hz * 10) / 10, Math.round(deg * 10) / 10] as const;

  function dragPoint(e: PointerEvent, index: number) {
    if (e.button !== 0 || !svg) return;
    e.preventDefault();
    e.stopPropagation();
    active = index;
    const el = e.currentTarget as SVGElement;
    el.setPointerCapture(e.pointerId);
    const at = (ev: PointerEvent, done: boolean) => {
      const p = local(ev);
      const [hz, deg] = round(hzAt(p.x), degAt(p.y));
      send({ [freqKey(index)]: hz, [phaseKey(index)]: deg }, `Phase Point ${index + 1}`, done);
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
    const [hz, deg] = round(hzAt(p.x), degAt(p.y));
    const ch = addPoint(slot, hz, deg);
    if (ch) send(ch, "Add Phase Point", true);
  }

  function remove(e: MouseEvent, index: number) {
    e.preventDefault();
    e.stopPropagation();
    const ch = removePoint(slot, index);
    if (ch) send(ch, "Remove Phase Point", true);
  }

  function reset() {
    send({ points: 2, p1_freq: 20, p1_phase: 0, p2_freq: 20_000, p2_phase: 0 }, "Reset Phase Curve", true);
  }

  const ticks = $derived(
    linear
      ? [0, 2000, 4000, 6000, 8000, 10_000, 12_000, 14_000, 16_000, 18_000, 20_000].map((f) => ({ f, label: f === 0 ? "Hz" : `${f / 1000}k`, major: f % 10_000 === 0 }))
      : freqTicks(plotW),
  );
  const gridDeg = $derived(Array.from({ length: 19 }, (_, k) => range - (k * range) / 9));
  const ctl = (key: string) => info.params.find((p) => p.key === key) ?? null;
  const setCtl = (key: string, v: number, label: string) => {
    const p = ctl(key);
    if (p) onParams([[p, v]], label, true);
  };
  const tip = $derived.by(() => {
    const p = pts.find((x) => x.index === (active ?? hover));
    if (!p) return null;
    const hz = p.hz >= 1000 ? `${+(p.hz / 1000).toFixed(2)} kHz` : `${Math.round(p.hz)} Hz`;
    return { x: Math.min(w - 130, Math.max(4, X(p.hz) - 50)), y: Math.max(2, Y(p.deg) - 28), text: `${hz} · ${p.deg > 0 ? "+" : ""}${p.deg.toFixed(1)}°` };
  });
</script>

<div class="pc" bind:clientWidth={w}>
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions, a11y_no_noninteractive_element_interactions -->
  <svg bind:this={svg} width={w} height={H} role="img" aria-label="Phase Curve: phase turn by frequency" onclick={onBackground}>
    <rect class="bg" x={PAD.l} y={PAD.t} width={plotW} height={plotH} />
    {#each ticks as t (t.f)}
      <line class="grid" class:major={t.major} x1={X(t.f)} x2={X(t.f)} y1={PAD.t} y2={PAD.t + plotH} />
      {#if t.label}<text class="lab" class:strong={t.major} x={X(t.f)} y={H - 5} text-anchor={t.f <= 20 ? "start" : t.f >= 20_000 ? "end" : "middle"}>{t.label}</text>{/if}
    {/each}
    {#each gridDeg as d (d)}
      <line class="grid" class:zero={Math.abs(d) < 1e-9} x1={PAD.l} x2={PAD.l + plotW} y1={Y(d)} y2={Y(d)} />
      <text class="lab" x={PAD.l + plotW + 4} y={Y(d) + 3}>{Math.round(d)}°</text>
    {/each}
    <path class="curve" d={path} />
    {#each pts as p (p.index)}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <g
        class="pt"
        class:on={active === p.index}
        transform="translate({X(p.hz)} {Y(p.deg)})"
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
    <p class="hint">Click to add a point (up to 8), drag to move it, right-click to remove it.</p>
    <label class="check"><input type="checkbox" checked={spline} onchange={(e) => setCtl("spline", e.currentTarget.checked ? 1 : 0, "Spline Curves")} /> Spline Curves</label>
    <button type="button" class="reset" title="Reset: a flat curve at 0°" aria-label="Reset" onclick={reset}>
      <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true"><path d="M3.5 8a4.5 4.5 0 1 0 1.4-3.3M3 2.5v3h3" fill="none" stroke="currentColor" stroke-width="1.5" /></svg>
    </button>
  </div>
  <div class="form">
    <span class="flabel">Frequency Scale:</span>
    <span class="opts">
      <label><input type="radio" name="pc-scale-{slot.id}" checked={linear} onchange={() => (linear = true)} /> Linear</label>
      <label><input type="radio" name="pc-scale-{slot.id}" checked={!linear} onchange={() => (linear = false)} /> Logarithmic</label>
    </span>
    <span class="flabel">Range:</span>
    <span class="opts">
      <label><input type="radio" name="pc-range-{slot.id}" checked={range !== 360} onchange={() => setCtl("range", 0, "Phase Range")} /> 180°</label>
      <label><input type="radio" name="pc-range-{slot.id}" checked={range === 360} onchange={() => setCtl("range", 1, "Phase Range")} /> 360°</label>
    </span>
    {#each [["channels", "Channel"], ["fft", "FFT Size"]] as const as [key, label] (key)}
      {@const p = ctl(key)}
      {#if p}
        <span class="flabel">{label}:</span>
        <select value={String(paramValue(slot, p))} onchange={(e) => setCtl(key, Number(e.currentTarget.value), label)}>
          {#each p.choices ?? [] as c, k (k)}<option value={String(k)}>{c}</option>{/each}
        </select>
      {/if}
    {/each}
  </div>
</div>

<style>
  .pc {
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
  .grid.major {
    stroke: rgba(255, 255, 255, 0.16);
  }
  .grid.zero {
    stroke: rgba(255, 255, 255, 0.28);
  }
  .lab.strong {
    fill: var(--text-dim);
    font-weight: 600;
  }
  .check {
    display: flex;
    gap: 4px;
    align-items: center;
    font-size: var(--font-size-small);
  }
  .form {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 6px 10px;
    align-items: center;
    margin-top: 8px;
    font-size: var(--font-size-small);
  }
  .flabel {
    color: var(--text-dim);
    text-align: right;
  }
  .opts {
    display: flex;
    gap: 14px;
  }
  .opts label {
    display: flex;
    gap: 4px;
    align-items: center;
  }
  .form select {
    justify-self: start;
    min-width: 140px;
    background: var(--surface-0);
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    color: var(--text);
    font: inherit;
    padding: 1px 4px;
  }
  .lab {
    fill: var(--text-faint);
    font-size: 9px;
  }
  .curve {
    fill: none;
    stroke: #7d8cff;
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
    fill: #7d8cff;
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
  .hint {
    flex: 1;
    margin: 2px 0 0;
    color: var(--text-faint);
    font-size: var(--font-size-small);
  }
  .row .reset {
    display: grid;
    place-items: center;
    width: 24px;
    padding: 0;
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
</style>
