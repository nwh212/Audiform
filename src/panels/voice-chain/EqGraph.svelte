<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  The Parametric EQ on a graph: its response curve over the sound
  itself. Each band is a node to drag, left and right for frequency, up
  and down for gain (the high-pass and low-pass move sideways only). A
  band's width (Q) shows as two handles either side of its node; drag
  one, or turn the mouse wheel over the node, to make the band narrower
  or wider. Double-click the high-pass or low-pass node to switch it on
  or off. Behind the curve, the spectrum of what is playing (or, when
  stopped, of the audio at the playhead). The slider on the left is the
  overall gain. Below the graph, a column per band: frequency, gain (or
  slope for the high-pass and low-pass), width as Q or in octaves, and
  the band's on switch. Range shows 30 dB or 96 dB on the graph. Every
  drag is one undoable step.
-->
<script lang="ts">
  import { effectResponse, scopeSpectrum, type EffectInfo, type EffectParamInfo } from "../../app/core";
  import type { EffectSlotDoc } from "../../stores/session";
  import { paramValue } from "./chain";
  import { freqAt, freqTicks, xOf } from "./graph";
  import { eqNodes, formatHz, nodeOn, octavesOf, qFromEdge, qOf, widthEdges, type EqNode } from "./eq-graph";
  import ValueField from "./ValueField.svelte";
  import type { Preferences } from "../../stores/preferences";
  import VSlider from "./VSlider.svelte";

  interface Props {
    slot: EffectSlotDoc;
    info: EffectInfo;
    /** Something is playing: the spectrum follows it live. */
    playing: boolean;
    /** Where the playhead is (session samples), for the spectrum while stopped. */
    playhead: number;
    /** The Waveform editor's file, rather than the session. */
    wave: boolean;
    /** Controls moved: `done` on release, so a drag is one undoable step. */
    onParams: (changes: [EffectParamInfo, number][], label: string, done: boolean) => void;
    /** Where the view choices are kept between runs. */
    prefs?: Preferences;
  }

  const { slot, info, playing, playhead, wave, onParams, prefs }: Props = $props();

  let w = $state(460);
  const H = 250;
  const PAD = { l: 6, r: 34, t: 10, b: 18 };
  const plotW = $derived(Math.max(60, w - PAD.l - PAD.r));
  const plotH = H - PAD.t - PAD.b;
  // Range: 30 dB (plus and minus 15) or 96 dB (plus and minus 48), and Q or width in octaves.
  const KEY_VIEW = "eq.view";
  // svelte-ignore state_referenced_locally
  const saved = ((prefs?.raw(KEY_VIEW) as { range?: number; width?: boolean } | null) ?? {});
  let range = $state(saved.range === 96 ? 96 : 30);
  let byWidth = $state(saved.width === true);
  $effect(() => {
    const v = { range, width: byWidth };
    const old = prefs?.raw(KEY_VIEW) as typeof v | null;
    if (old?.range !== v.range || old?.width !== v.width) prefs?.set(KEY_VIEW, v);
  });
  /** The gain the graph shows, top and bottom (dB). */
  const TOP = $derived(range / 2);
  const BOTTOM = $derived(-range / 2);
  /** The spectrum's range, top and bottom (dBFS). */
  const S_TOP = 0;
  const S_BOTTOM = -96;

  const X = (hz: number) => PAD.l + xOf(hz) * plotW;
  const Y = (db: number) => PAD.t + ((TOP - Math.max(BOTTOM - 6, Math.min(TOP + 6, db))) / (TOP - BOTTOM)) * plotH;
  const hzAt = (x: number) => freqAt((x - PAD.l) / plotW);
  const dbAt = (y: number) => TOP - ((y - PAD.t) / plotH) * (TOP - BOTTOM);

  const nodes = $derived(eqNodes(info));
  const clampP = (p: EffectParamInfo, v: number) => Math.max(p.min, Math.min(p.max, v));

  // The response curve, measured from the effect itself.
  let curve = $state<[number, number][]>([]);
  const key = $derived(JSON.stringify(slot.params));
  let timer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    void key;
    const s = $state.snapshot(slot) as EffectSlotDoc;
    clearTimeout(timer);
    timer = setTimeout(() => {
      effectResponse(s, 240)
        .then((p) => (curve = p))
        .catch(() => (curve = []));
    }, 30);
    return () => clearTimeout(timer);
  });
  const curvePath = $derived(curve.map(([f, db], i) => `${i ? "L" : "M"}${X(f).toFixed(1)},${Y(db).toFixed(1)}`).join(" "));

  // The sound behind it: live while playing, at the playhead while stopped.
  const POINTS = 200;
  let levels = $state<Float32Array | null>(null);
  let frame: number | null = null;
  let busy = false;
  let last = 0;
  function poll(now: number) {
    frame = null;
    if (!busy && now - last > 45) {
      last = now;
      busy = true;
      scopeSpectrum(4096, POINTS, null, wave)
        .then((l) => (levels = l))
        .catch(() => {})
        .finally(() => (busy = false));
    }
    if (playing) frame = requestAnimationFrame(poll);
  }
  $effect(() => {
    if (frame !== null) cancelAnimationFrame(frame);
    frame = null;
    if (playing) {
      frame = requestAnimationFrame(poll);
      return () => {
        if (frame !== null) cancelAnimationFrame(frame);
        frame = null;
      };
    }
    // Stopped: the audio at the playhead (and again whenever the playhead or settings move).
    void key;
    const at = playhead;
    const t = setTimeout(() => {
      scopeSpectrum(8192, POINTS, at, wave)
        .then((l) => (levels = l))
        .catch(() => (levels = null));
    }, 120);
    return () => clearTimeout(t);
  });
  const SY = (db: number) => PAD.t + ((S_TOP - Math.max(S_BOTTOM, Math.min(S_TOP, db))) / (S_TOP - S_BOTTOM)) * plotH;
  const spectrumPath = $derived.by(() => {
    if (!levels || levels.length < 2) return "";
    const n = levels.length;
    let d = `M${X(20).toFixed(1)},${(PAD.t + plotH).toFixed(1)}`;
    for (let i = 0; i < n; i++) {
      const f = 20 * Math.pow(1000, i / (n - 1));
      d += ` L${X(f).toFixed(1)},${SY(levels[i]!).toFixed(1)}`;
    }
    return `${d} L${X(20_000).toFixed(1)},${(PAD.t + plotH).toFixed(1)} Z`;
  });

  // Nodes where their controls put them.
  const placed = $derived(
    nodes.map((n) => {
      const f = paramValue(slot, n.freq);
      const g = n.gain ? paramValue(slot, n.gain) : 0;
      const q = n.q ? paramValue(slot, n.q) : null;
      return { n, f, g, q, on: nodeOn(slot, n), x: X(f), y: Y(g), edges: q !== null ? widthEdges(f, q) : null };
    }),
  );

  let svg = $state<SVGSVGElement | null>(null);
  let active = $state<string | null>(null);
  let hover = $state<string | null>(null);
  const local = (e: PointerEvent | WheelEvent) => {
    const b = svg!.getBoundingClientRect();
    return { x: e.clientX - b.left, y: e.clientY - b.top };
  };

  function dragNode(e: PointerEvent, n: EqNode) {
    if (e.button !== 0 || !svg) return;
    e.preventDefault();
    e.stopPropagation();
    active = n.name;
    const el = e.currentTarget as SVGElement;
    el.setPointerCapture(e.pointerId);
    const at = (ev: PointerEvent, done: boolean) => {
      const p = local(ev);
      const changes: [EffectParamInfo, number][] = [[n.freq, clampP(n.freq, hzAt(p.x))]];
      if (n.gain) changes.push([n.gain, clampP(n.gain, Math.round(dbAt(p.y) * 10) / 10)]);
      onParams(changes, `${n.name} Point`, done);
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

  /** A width handle: the band's Q from how far the handle sits from its center. */
  function dragWidth(e: PointerEvent, n: EqNode, f: number) {
    if (e.button !== 0 || !svg || !n.q) return;
    e.preventDefault();
    e.stopPropagation();
    active = n.name;
    const q = n.q;
    const el = e.currentTarget as SVGElement;
    el.setPointerCapture(e.pointerId);
    const at = (ev: PointerEvent, done: boolean) => onParams([[q, clampP(q, Math.round(qFromEdge(f, hzAt(local(ev).x)) * 100) / 100)]], `${n.name} Width`, done);
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

  /** The wheel over a band's node: narrower (up) or wider (down). One step once it stops. */
  let wheelTimer: ReturnType<typeof setTimeout> | undefined;
  function wheelWidth(e: WheelEvent, n: EqNode, q: number | null) {
    if (!n.q || q === null) return;
    e.preventDefault();
    e.stopPropagation();
    const next = clampP(n.q, Math.round(q * (e.deltaY < 0 ? 1.15 : 1 / 1.15) * 100) / 100);
    const p = n.q;
    onParams([[p, next]], `${n.name} Width`, false);
    clearTimeout(wheelTimer);
    wheelTimer = setTimeout(() => onParams([[p, next]], `${n.name} Width`, true), 350);
  }

  function toggle(n: EqNode, on: boolean) {
    if (n.on) onParams([[n.on, on ? 0 : 1]], `${n.name} ${on ? "Off" : "On"}`, true);
  }

  const ticks = $derived(freqTicks(plotW));
  const gridDb = $derived(range === 30 ? [15, 10, 5, 0, -5, -10, -15] : [48, 36, 24, 12, 0, -12, -24, -36, -48]);
  const master = $derived(info.params.find((p) => p.key === "gain") ?? null);
  /** The table's columns, low to high. */
  const columns = $derived(placed);
  /** Width in octaves as its own control, so it can be dragged and typed like Q. */
  const octaveParam = (q: EffectParamInfo): EffectParamInfo => ({ ...q, key: `${q.key}_oct`, name: "Width", unit: "oct", min: 0.02, max: 5, default: octavesOf(q.default), step: 0.01, log: true });
  const slopeOf = (n: EqNode) => info.params.find((p) => p.group === n.name && p.choices) ?? null;
  const tip = $derived.by(() => {
    const p = placed.find((x) => x.n.name === (active ?? hover));
    if (!p) return null;
    const parts = [p.n.name, formatHz(p.f)];
    if (p.n.gain) parts.push(`${p.g > 0 ? "+" : ""}${p.g.toFixed(1)} dB`);
    if (p.q !== null) parts.push(`Q ${p.q.toFixed(2)}`);
    if (p.n.on && !p.on) parts.push("off");
    return { x: Math.min(w - 150, Math.max(4, p.x - 60)), y: Math.max(2, p.y - 30), text: parts.join(" · ") };
  });
</script>

<div class="eqwin">
<div class="top">
  {#if master}
    <div class="master">
      <VSlider param={master} value={paramValue(slot, master)} height={H - 70} oninput={(v) => onParams([[master, v]], "Gain", false)} oncommit={(v) => onParams([[master, v]], "Gain", true)} />
    </div>
  {/if}
<div class="eq" bind:clientWidth={w}>
  <svg bind:this={svg} width={w} height={H} role="img" aria-label="{info.name}: response over the sound">
    <rect class="bg" x={PAD.l} y={PAD.t} width={plotW} height={plotH} />
    {#if spectrumPath}<path class="spectrum" d={spectrumPath} />{/if}
    {#each ticks as t (t.f)}
      <line class="grid" class:major={t.major} x1={X(t.f)} x2={X(t.f)} y1={PAD.t} y2={PAD.t + plotH} />
      {#if t.label}<text class="lab" class:strong={t.major} x={X(t.f)} y={H - 5} text-anchor={t.f === 20 ? "start" : t.f === 20_000 ? "end" : "middle"}>{t.label}</text>{/if}
    {/each}
    {#each gridDb as d (d)}
      <line class="grid" class:zero={d === 0} x1={PAD.l} x2={PAD.l + plotW} y1={Y(d)} y2={Y(d)} />
      <text class="lab" x={PAD.l + plotW + 4} y={Y(d) + 3}>{d > 0 ? `+${d}` : d}</text>
    {/each}
    <text class="lab" x={PAD.l + plotW + 4} y={H - 5}>dB</text>
    <clipPath id="eq-plot-{slot.id}"><rect x={PAD.l} y={PAD.t} width={plotW} height={plotH} /></clipPath>
    {#if curvePath}<path class="curve" d={curvePath} clip-path="url(#eq-plot-{slot.id})" />{/if}

    {#each placed as p (p.n.name)}
      {#if p.edges && p.on}
        {#each p.edges as ef, k (k)}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <rect
            class="width"
            class:on={active === p.n.name || hover === p.n.name}
            x={X(ef) - 2.5}
            y={p.y - 6}
            width="5"
            height="12"
            onpointerdown={(e) => dragWidth(e, p.n, p.f)}
            onpointerenter={() => (hover = p.n.name)}
            onpointerleave={() => (hover = null)}
          />
        {/each}
        <line class="span" x1={X(p.edges[0])} x2={X(p.edges[1])} y1={p.y} y2={p.y} />
      {/if}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <g
        class="node"
        class:off={!p.on}
        class:on={active === p.n.name}
        transform="translate({p.x} {p.n.gain ? p.y : Y(0)})"
        onpointerdown={(e) => dragNode(e, p.n)}
        onwheel={(e) => wheelWidth(e, p.n, p.q)}
        ondblclick={() => toggle(p.n, p.on)}
        onpointerenter={() => (hover = p.n.name)}
        onpointerleave={() => (hover = null)}
      >
        <circle class="hit" r="10" />
        <rect x="-5" y="-5" width="10" height="10" />
        <text y="-9" text-anchor="middle">{p.n.label}</text>
      </g>
    {/each}
  </svg>
  {#if tip}<div class="tip" style:left="{tip.x}px" style:top="{tip.y}px">{tip.text}</div>{/if}
</div>
</div>

<!-- A column per band: frequency, gain (or slope), width, and its switch. -->
<div class="table" style:--cols={columns.length}>
  <span class="rowhead">Frequency</span>
  {#each columns as c (c.n.name)}
    <div class="cell" class:dim={!c.on}><ValueField param={c.n.freq} value={c.f} oninput={(v) => onParams([[c.n.freq, v]], `${c.n.name} Frequency`, false)} oncommit={(v) => onParams([[c.n.freq, v]], `${c.n.name} Frequency`, true)} /></div>
  {/each}
  <span class="rowhead">Gain</span>
  {#each columns as c (c.n.name)}
    {@const slope = c.n.gain ? null : slopeOf(c.n)}
    <div class="cell" class:dim={!c.on}>
      {#if c.n.gain}
        <ValueField param={c.n.gain} value={c.g} oninput={(v) => onParams([[c.n.gain!, v]], `${c.n.name} Gain`, false)} oncommit={(v) => onParams([[c.n.gain!, v]], `${c.n.name} Gain`, true)} />
      {:else if slope}
        <select aria-label="{c.n.name} slope" title="Slope, per octave" value={String(paramValue(slot, slope))} onchange={(e) => onParams([[slope, Number(e.currentTarget.value)]], `${c.n.name} Slope`, true)}>
          {#each slope.choices ?? [] as label, k (k)}<option value={String(k)}>{label.replace(" dB/oct", " dB")}</option>{/each}
        </select>
      {/if}
    </div>
  {/each}
  <span class="rowhead">{byWidth ? "Width" : "Q"}</span>
  {#each columns as c (c.n.name)}
    <div class="cell" class:dim={!c.on}>
      {#if c.n.q && c.q !== null}
        {#if byWidth}
          {@const op = octaveParam(c.n.q)}
          <ValueField param={op} value={octavesOf(c.q)} oninput={(v) => onParams([[c.n.q!, qOf(v)]], `${c.n.name} Width`, false)} oncommit={(v) => onParams([[c.n.q!, qOf(v)]], `${c.n.name} Width`, true)} />
        {:else}
          <ValueField param={c.n.q} value={c.q} oninput={(v) => onParams([[c.n.q!, v]], `${c.n.name} Width`, false)} oncommit={(v) => onParams([[c.n.q!, v]], `${c.n.name} Width`, true)} />
        {/if}
      {/if}
    </div>
  {/each}
  <span class="rowhead">Band</span>
  {#each columns as c (c.n.name)}
    <div class="cell">
      <button type="button" class="band" class:on={c.on} aria-pressed={c.on} disabled={!c.n.on} title="{c.n.name}: {c.on ? 'on' : 'off'}" onclick={() => toggle(c.n, c.on)}>{c.n.label}</button>
    </div>
  {/each}
</div>
<div class="opts">
  <span class="olabel">Constant:</span>
  <label><input type="radio" name="eq-width-{slot.id}" checked={!byWidth} onchange={() => (byWidth = false)} /> Q</label>
  <label><input type="radio" name="eq-width-{slot.id}" checked={byWidth} onchange={() => (byWidth = true)} /> Width</label>
  <span class="sep"></span>
  <span class="olabel">Range:</span>
  <label><input type="radio" name="eq-range-{slot.id}" checked={range === 30} onchange={() => (range = 30)} /> 30 dB</label>
  <label><input type="radio" name="eq-range-{slot.id}" checked={range === 96} onchange={() => (range = 96)} /> 96 dB</label>
</div>
</div>

<style>
  .eqwin {
    display: grid;
    gap: 8px;
  }
  .top {
    display: flex;
    gap: 6px;
    align-items: stretch;
  }
  .master {
    display: flex;
    flex: none;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    width: 64px;
  }
  .rowhead,
  .olabel {
    color: var(--text-dim);
    font-size: var(--font-size-small);
  }
  .eq {
    position: relative;
    flex: 1;
    min-width: 0;
    user-select: none;
  }
  .table {
    display: grid;
    grid-template-columns: 74px repeat(var(--cols), minmax(0, 1fr));
    gap: 4px 2px;
    align-items: center;
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
  .cell :global(.vf),
  .cell select {
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
    flex-wrap: wrap;
    gap: 6px 10px;
    align-items: center;
    justify-content: center;
    padding-top: 6px;
    border-top: 1px solid var(--border);
    font-size: var(--font-size-small);
  }
  .opts label {
    display: flex;
    gap: 4px;
    align-items: center;
  }
  .sep {
    width: 1px;
    height: 14px;
    background: var(--border-strong);
  }
  select {
    background: var(--surface-0);
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    color: var(--text);
    font: inherit;
    padding: 1px 4px;
  }
  svg {
    display: block;
  }
  .bg {
    fill: #0b0b0e;
    stroke: var(--border);
  }
  .spectrum {
    fill: color-mix(in srgb, var(--accent) 35%, transparent);
    stroke: color-mix(in srgb, var(--accent) 70%, transparent);
    stroke-width: 0.8;
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
  .span {
    stroke: rgba(255, 255, 255, 0.25);
    stroke-dasharray: 2 2;
    pointer-events: none;
  }
  .width {
    fill: rgba(255, 255, 255, 0.55);
    cursor: ew-resize;
  }
  .width.on {
    fill: #ffffff;
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
    stroke-width: 1;
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
</style>
