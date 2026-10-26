<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  The Shaped Noise Reducer on a graph: the sound (filled), the learned
  noise (grey line) and the threshold (orange line). Frequencies under
  the threshold are turned down. Each shaping band is a node on the
  threshold: drag it sideways for frequency and up or down to raise or
  lower the threshold there; the mouse wheel over a node makes it
  narrower or wider; double-click switches it on or off. Every drag is
  one undoable step.
-->
<script lang="ts">
  import { scopeSpectrum, shapedNoiseCurves, type EffectInfo, type EffectParamInfo } from "../../app/core";
  import type { EffectSlotDoc } from "../../stores/session";
  import { paramValue } from "./chain";
  import { formatHz } from "./eq-graph";
  import { freqAt, xOf } from "./graph";
  import VSlider from "./VSlider.svelte";
  import ValueField from "./ValueField.svelte";

  interface Props {
    slot: EffectSlotDoc;
    info: EffectInfo;
    playing: boolean;
    playhead: number;
    wave: boolean;
    onParams: (changes: [EffectParamInfo, number][], label: string, done: boolean) => void;
  }

  const { slot, info, playing, playhead, wave, onParams }: Props = $props();

  /** Spectrum size: the core lines the noise print up with this. */
  const SCOPE = 4096;
  let w = $state(460);
  const H = 240;
  const PAD = { l: 6, r: 34, t: 8, b: 18 };
  const plotW = $derived(Math.max(60, w - PAD.l - PAD.r));
  const plotH = H - PAD.t - PAD.b;
  const TOP = 0;
  const BOTTOM = -120;

  const X = (hz: number) => PAD.l + xOf(hz) * plotW;
  const Y = (db: number) => PAD.t + ((TOP - Math.max(BOTTOM, Math.min(TOP, db))) / (TOP - BOTTOM)) * plotH;
  const hzAt = (x: number) => freqAt((x - PAD.l) / plotW);
  const dbAt = (y: number) => TOP - ((y - PAD.t) / plotH) * (TOP - BOTTOM);

  const byKey = $derived(new Map(info.params.map((p) => [p.key, p])));
  const clampP = (p: EffectParamInfo, v: number) => Math.max(p.min, Math.min(p.max, v));

  // The learned noise and the threshold, worked out by the effect itself.
  let print = $state<[number, number][]>([]);
  let thr = $state<[number, number][]>([]);
  const key = $derived(JSON.stringify(slot.params));
  $effect(() => {
    void key;
    const s = $state.snapshot(slot) as EffectSlotDoc;
    const t = setTimeout(() => {
      shapedNoiseCurves(s, 200, SCOPE)
        .then((c) => {
          print = c.print;
          thr = c.threshold;
        })
        .catch(() => {});
    }, 30);
    return () => clearTimeout(t);
  });
  const line = (pts: [number, number][]) => pts.map(([f, db], i) => `${i ? "L" : "M"}${X(f).toFixed(1)},${Y(db).toFixed(1)}`).join(" ");
  const thrAt = (hz: number) => {
    if (!thr.length) return -60;
    let best = thr[0]!;
    for (const p of thr) if (Math.abs(Math.log(p[0] / hz)) < Math.abs(Math.log(best[0] / hz))) best = p;
    return best[1];
  };

  // The sound behind it, live while playing, at the playhead while stopped.
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
      scopeSpectrum(SCOPE, POINTS, null, wave)
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
    void key;
    const at = playhead;
    const t = setTimeout(() => {
      scopeSpectrum(SCOPE, POINTS, at, wave)
        .then((l) => (levels = l))
        .catch(() => (levels = null));
    }, 120);
    return () => clearTimeout(t);
  });
  const spectrumPath = $derived.by(() => {
    if (!levels || levels.length < 2) return "";
    const n = levels.length;
    let d = `M${X(20).toFixed(1)},${(PAD.t + plotH).toFixed(1)}`;
    for (let i = 0; i < n; i++) d += ` L${X(20 * Math.pow(1000, i / (n - 1))).toFixed(1)},${Y(levels[i]!).toFixed(1)}`;
    return `${d} L${X(20_000).toFixed(1)},${(PAD.t + plotH).toFixed(1)} Z`;
  });

  interface Band {
    i: number;
    on: EffectParamInfo;
    kind: EffectParamInfo;
    freq: EffectParamInfo;
    gain: EffectParamInfo;
    q: EffectParamInfo;
  }
  const bands = $derived(
    [1, 2, 3, 4, 5].flatMap((i): Band[] => {
      const g = (k: string) => byKey.get(`s${i}_${k}`);
      const [on, kind, freq, gain, q] = [g("on"), g("kind"), g("freq"), g("gain"), g("q")];
      return on && kind && freq && gain && q ? [{ i, on, kind, freq, gain, q }] : [];
    }),
  );
  const placed = $derived(
    bands.map((b) => {
      const f = paramValue(slot, b.freq);
      const kind = Math.round(paramValue(slot, b.kind));
      return { b, f, g: paramValue(slot, b.gain), q: paramValue(slot, b.q), on: paramValue(slot, b.on) >= 0.5, kind, x: X(f), y: Y(thrAt(f)), label: kind === 1 ? "L" : kind === 2 ? "H" : String(b.i) };
    }),
  );

  let svg = $state<SVGSVGElement | null>(null);
  let active = $state<number | null>(null);
  let hover = $state<number | null>(null);
  const local = (e: PointerEvent) => {
    const r = svg!.getBoundingClientRect();
    return { x: e.clientX - r.left, y: e.clientY - r.top };
  };

  function dragNode(e: PointerEvent, b: Band, g0: number) {
    if (e.button !== 0 || !svg) return;
    e.preventDefault();
    e.stopPropagation();
    active = b.i;
    const el = e.currentTarget as SVGElement;
    el.setPointerCapture(e.pointerId);
    const y0 = local(e).y;
    const at = (ev: PointerEvent, done: boolean) => {
      const p = local(ev);
      const g = Math.round((g0 + dbAt(p.y) - dbAt(y0)) * 10) / 10;
      onParams(
        [
          [b.freq, clampP(b.freq, Math.round(hzAt(p.x)))],
          [b.gain, clampP(b.gain, g)],
        ],
        `Noise Shape ${b.i}`,
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

  let wheelTimer: ReturnType<typeof setTimeout> | undefined;
  function wheelWidth(e: WheelEvent, b: Band, q: number) {
    e.preventDefault();
    e.stopPropagation();
    const next = clampP(b.q, Math.round(q * (e.deltaY < 0 ? 1.15 : 1 / 1.15) * 100) / 100);
    onParams([[b.q, next]], `Noise Shape ${b.i} Width`, false);
    clearTimeout(wheelTimer);
    wheelTimer = setTimeout(() => onParams([[b.q, next]], `Noise Shape ${b.i} Width`, true), 350);
  }

  const gridHz = [20, 50, 100, 200, 500, 1000, 2000, 5000, 10_000, 20_000];
  const gridDb = [0, -20, -40, -60, -80, -100, -120];
  const tip = $derived.by(() => {
    const p = placed.find((x) => x.b.i === (active ?? hover));
    if (!p) return null;
    const parts = [["Bell", "Low shelf", "High shelf"][p.kind] ?? "Band", formatHz(p.f), `${p.g > 0 ? "+" : ""}${p.g.toFixed(1)} dB`];
    if (p.kind === 0) parts.push(`Q ${p.q.toFixed(2)}`);
    if (!p.on) parts.push("off");
    return { x: Math.min(w - 170, Math.max(4, p.x - 60)), y: Math.max(2, p.y - 30), text: parts.join(" · ") };
  });
  const learned = $derived((slot.params.printlen ?? 0) > 0);

  // The controls around the graph.
  let pick = $state(3);
  const cur = $derived(placed.find((p) => p.b.i === pick) ?? placed[0] ?? null);
  const P = (k: string) => byKey.get(k);
  const val = (p: EffectParamInfo) => paramValue(slot, p);
  const set = (p: EffectParamInfo, v: number, done: boolean) => onParams([[p, clampP(p, v)]], `${info.name} ${p.name}`, done);
</script>

<div class="ns">
  <div class="top">
    <div class="sliders">
      {#each ["threshold", "reduction"] as k (k)}
        {@const p = P(k)}
        {#if p}<VSlider param={p} value={val(p)} height={178} oninput={(v) => set(p, v, false)} oncommit={(v) => set(p, v, true)} />{/if}
      {/each}
    </div>
    <div class="plot" bind:clientWidth={w}>
  <svg bind:this={svg} width={w} height={H} role="img" aria-label="{info.name}: the sound, the learned noise and the threshold">
    <rect class="bg" x={PAD.l} y={PAD.t} width={plotW} height={plotH} />
    {#if spectrumPath}<path class="spectrum" d={spectrumPath} />{/if}
    {#each gridHz as f (f)}
      <line class="grid" class:major={f === 100 || f === 1000 || f === 10_000} x1={X(f)} x2={X(f)} y1={PAD.t} y2={PAD.t + plotH} />
      <text class="lab" x={X(f)} y={H - 5} text-anchor="middle">{f >= 1000 ? `${f / 1000}k` : f}</text>
    {/each}
    {#each gridDb as d (d)}
      <line class="grid" x1={PAD.l} x2={PAD.l + plotW} y1={Y(d)} y2={Y(d)} />
      <text class="lab" x={PAD.l + plotW + 4} y={Y(d) + 3}>{d}</text>
    {/each}
    <text class="lab" x={PAD.l + plotW + 4} y={H - 5}>dB</text>
    {#if print.length}<path class="print" d={line(print)} />{/if}
    {#if thr.length}<path class="thr" d={line(thr)} />{/if}
    {#each placed as p (p.b.i)}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <g
        class="node k{p.kind}"
        class:off={!p.on}
        class:on={active === p.b.i}
        transform="translate({p.x} {p.y})"
        onpointerdown={(e) => {
          pick = p.b.i;
          dragNode(e, p.b, p.g);
        }}
        onwheel={(e) => wheelWidth(e, p.b, p.q)}
        ondblclick={() => onParams([[p.b.on, p.on ? 0 : 1]], `Noise Shape ${p.b.i} ${p.on ? "Off" : "On"}`, true)}
        onpointerenter={() => (hover = p.b.i)}
        onpointerleave={() => (hover = null)}
      >
        <circle class="hit" r="10" />
        <circle class="dot" r="6" />
        <text y="3.5" text-anchor="middle">{p.label}</text>
      </g>
    {/each}
    <g class="legend" transform="translate({PAD.l + 8} {PAD.t + plotH - 34})">
      <rect class="sw spec" width="8" height="8" y="-7" /><text x="12">Sound</text>
      <rect class="sw pr" width="8" height="3" y="-4" x="0" transform="translate(0 12)" /><text x="12" y="12">Learned noise</text>
      <rect class="sw th" width="8" height="3" y="-4" transform="translate(0 24)" /><text x="12" y="24">Threshold</text>
    </g>
  </svg>
  {#if tip}<div class="tip" style:left="{tip.x}px" style:top="{tip.y}px">{tip.text}</div>{/if}
    </div>
  </div>

  {#if cur}
    {@const b = cur.b}
    <div class="band">
      <div class="picks" role="group" aria-label="Shaping band">
        {#each placed as p (p.b.i)}
          <button type="button" class:sel={p.b.i === pick} class:off={!p.on} title="Edit shaping band {p.b.i}" onclick={() => (pick = p.b.i)}>{p.label}</button>
        {/each}
      </div>
      <button type="button" class="pw" class:on={cur.on} aria-pressed={cur.on} title={cur.on ? "Switch this band off" : "Switch this band on"} onclick={() => set(b.on, cur.on ? 0 : 1, true)}>{cur.on ? "On" : "Off"}</button>
      <label class="f"><span>Shape</span>
        <select value={String(cur.kind)} onchange={(e) => set(b.kind, Number(e.currentTarget.value), true)}>
          {#each b.kind.choices ?? [] as c, ci (c)}<option value={String(ci)}>{c}</option>{/each}
        </select>
      </label>
      <div class="f"><span>Gain</span><ValueField param={b.gain} value={cur.g} oninput={(v) => set(b.gain, v, false)} oncommit={(v) => set(b.gain, v, true)} /></div>
      <div class="f"><span>Frequency</span><ValueField param={b.freq} value={cur.f} oninput={(v) => set(b.freq, v, false)} oncommit={(v) => set(b.freq, v, true)} /></div>
      <div class="f"><span>Q</span><ValueField param={b.q} value={cur.q} oninput={(v) => set(b.q, v, false)} oncommit={(v) => set(b.q, v, true)} /></div>
    </div>
  {/if}

  <div class="row">
    {#each ["attack", "release", "knee"] as k (k)}
      {@const p = P(k)}
      {#if p}<div class="f"><span>{p.name}</span><ValueField param={p} value={val(p)} oninput={(v) => set(p, v, false)} oncommit={(v) => set(p, v, true)} /></div>{/if}
    {/each}
    {#if P("adaptive")}
      {@const a = P("adaptive")!}
      <div class="f"><span>Print</span><button type="button" class="pw" class:on={val(a) >= 0.5} aria-pressed={val(a) >= 0.5} title="Adaptive: the learned noise follows a noise that drifts" onclick={() => set(a, val(a) >= 0.5 ? 0 : 1, true)}>Adaptive</button></div>
    {/if}
    {#if P("monitor")}
      {@const m = P("monitor")!}
      <div class="f"><span>Listen to</span>
        <div class="seg" role="group" aria-label="Output">
          <button type="button" class:sel={Math.round(val(m)) === 0} onclick={() => set(m, 0, true)}>Audio</button>
          <button type="button" class:sel={Math.round(val(m)) === 1} title="Hear only what is being removed" onclick={() => set(m, 1, true)}>Removed</button>
        </div>
      </div>
    {/if}
  </div>
  {#if learned}<p class="hint">Frequencies under the orange threshold are turned down. Drag a node to raise or lower the threshold around it, or pick it below to type its settings. The mouse wheel over a node changes its width; double-click switches it on or off.</p>{/if}
</div>

<style>
  .ns {
    display: grid;
    gap: 6px;
    width: 100%;
    margin-bottom: 6px;
    user-select: none;
  }
  .top {
    display: flex;
    gap: 4px;
  }
  .sliders {
    display: flex;
    gap: 2px;
    padding: 4px 2px;
    border: 1px solid var(--border);
    border-radius: 3px;
    background: rgba(0, 0, 0, 0.18);
  }
  .plot {
    position: relative;
    flex: 1;
    min-width: 0;
  }
  .band,
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 10px;
    align-items: flex-end;
    padding: 5px 6px;
    border: 1px solid var(--border);
    border-radius: 3px;
    background: rgba(0, 0, 0, 0.18);
  }
  .f {
    display: grid;
    gap: 2px;
    min-width: 64px;
    color: var(--text-dim);
    font-size: 11px;
  }
  .f > span {
    text-align: center;
  }
  .f select {
    height: 20px;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    background: var(--surface-3, var(--surface-2));
    color: var(--text);
    font: inherit;
    font-size: 11px;
  }
  .picks,
  .seg {
    display: flex;
  }
  .picks button,
  .seg button,
  .pw {
    height: 20px;
    min-width: 24px;
    padding: 0 6px;
    border: 1px solid var(--border-strong);
    background: var(--surface-3, var(--surface-2));
    color: var(--text-dim);
    font: inherit;
    font-size: 11px;
    cursor: pointer;
  }
  .picks button + button,
  .seg button + button {
    border-left: none;
  }
  .picks button.sel,
  .seg button.sel {
    background: #f0a040;
    border-color: #f0a040;
    color: #101012;
    font-weight: 600;
  }
  .picks button.off:not(.sel) {
    opacity: 0.5;
  }
  .pw {
    border-radius: 2px;
  }
  .pw.on {
    border-color: var(--accent);
    color: var(--accent);
  }
  svg {
    display: block;
  }
  .bg {
    fill: #0b0b0e;
    stroke: var(--border);
  }
  .spectrum {
    fill: color-mix(in srgb, var(--accent) 30%, transparent);
    stroke: color-mix(in srgb, var(--accent) 60%, transparent);
    stroke-width: 0.8;
  }
  .grid {
    stroke: rgba(255, 255, 255, 0.06);
  }
  .grid.major {
    stroke: rgba(255, 255, 255, 0.12);
  }
  .lab {
    fill: var(--text-faint);
    font-size: 9px;
  }
  .print {
    fill: none;
    stroke: rgba(235, 235, 240, 0.85);
    stroke-width: 1.4;
  }
  .thr {
    fill: none;
    stroke: #f0a040;
    stroke-width: 2;
  }
  .node {
    cursor: move;
  }
  .node .hit {
    fill: transparent;
  }
  .node .dot {
    fill: #f0a040;
    stroke: #0e0e10;
  }
  .node.on .dot {
    fill: #ffd28a;
  }
  .node.off .dot {
    fill: transparent;
    stroke: rgba(240, 160, 64, 0.8);
  }
  .node text {
    fill: #101012;
    font-size: 9px;
    font-weight: 700;
    pointer-events: none;
  }
  .node.off text {
    fill: #f0a040;
  }
  .legend text {
    fill: var(--text-dim);
    font-size: 9px;
  }
  .sw.spec {
    fill: color-mix(in srgb, var(--accent) 45%, transparent);
  }
  .sw.pr {
    fill: rgba(235, 235, 240, 0.85);
  }
  .sw.th {
    fill: #f0a040;
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
  .hint {
    margin: 2px 0 0;
    color: var(--text-faint);
    font-size: var(--font-size-small);
  }
</style>
