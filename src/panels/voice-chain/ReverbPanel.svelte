<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The Audiform contributors.
  This file is part of Audiform, based on Altered Audio Editor.

  The Convolution Reverb's window. At the top, the impulse response: any
  audio file of a space's echo, drawn as its level over time (on a dB
  scale, so the tail shows), with the part Length keeps lit and the
  Pre-delay drawn as a gap before it. Load picks a file, Clear removes
  it. Below, dials for Pre-delay, Length, Width, Low cut and High cut,
  and the Dry and Wet faders. Loading or clearing a file is one undoable
  step, like every control.
-->
<script lang="ts">
  import { reverbLoad, reverbPick, type EffectInfo, type EffectParamInfo, type ReverbIr } from "../../app/core";
  import type { EffectSlotDoc } from "../../stores/session";
  import BigKnob from "./BigKnob.svelte";
  import KindTabs from "./KindTabs.svelte";
  import { REVERB_KINDS, REVERB_TIPS } from "./reverb-kinds";
  import { paramValue } from "./chain";
  import VSlider from "./VSlider.svelte";

  interface Props {
    slot: EffectSlotDoc;
    info: EffectInfo;
    onParams: (changes: [EffectParamInfo, number][], label: string, done: boolean) => void;
    /** Saves new slot data (or none) as one undoable edit. */
    onData: (data: string | null, label: string) => void;
    /** Another tab: the slot becomes the made Reverb of that kind. */
    onKind?: (kind: number) => void;
    onError?: (e: unknown) => void;
  }

  const { slot, info, onParams, onData, onKind, onError = console.error }: Props = $props();

  const byKey = $derived(new Map(info.params.map((p) => [p.key, p])));
  const P = (k: string) => byKey.get(k)!;
  const val = (k: string) => paramValue(slot, P(k));
  const set = (k: string, v: number, done: boolean) => {
    const p = P(k);
    onParams([[p, Math.max(p.min, Math.min(p.max, v))]], `${info.name} ${p.name}`, done);
  };

  /** The file the slot names, as saved. */
  const saved = $derived.by(() => {
    try {
      return slot.data ? (JSON.parse(slot.data) as { path?: string; name?: string }) : null;
    } catch {
      return null;
    }
  });
  let ir = $state<ReverbIr | null>(null);
  let missing = $state<string | null>(null);
  let busy = $state(false);
  // Describe the named file whenever the slot names another (it may have moved since).
  let asked: string | null = null;
  $effect(() => {
    const path = saved?.path ?? null;
    if (path === asked) return;
    asked = path;
    if (!path) {
      ir = null;
      missing = null;
      return;
    }
    reverbLoad(path)
      .then((r) => {
        ir = r;
        missing = null;
      })
      .catch((e) => {
        ir = null;
        missing = `${saved?.name ?? "The response"} could not be read (${e instanceof Error ? e.message : String(e)}). Only the dry sound plays until it is loaded again.`;
      });
  });

  async function load() {
    if (busy) return;
    busy = true;
    try {
      const path = await reverbPick();
      if (!path) return;
      const r = await reverbLoad(path);
      ir = r;
      missing = null;
      asked = (JSON.parse(r.data) as { path: string }).path;
      onData(r.data, `Load Impulse Response: ${r.name}`);
    } catch (e) {
      missing = `That file did not load: ${e instanceof Error ? e.message : String(e)}`;
      onError(e);
    } finally {
      busy = false;
    }
  }

  const W = 480;
  const H = 96;
  const pre = $derived(val("predelay") / 1000);
  const total = $derived(ir ? ir.seconds + pre : 1);
  const keep = $derived(val("length") / 100);
  const path = $derived.by(() => {
    if (!ir || !ir.shape.length) return "";
    const x0 = (pre / total) * W;
    const span = (ir.seconds / total) * W;
    const n = ir.shape.length;
    let d = `M${x0.toFixed(1)},${H}`;
    ir.shape.forEach((v, k) => {
      d += ` L${(x0 + (k / (n - 1)) * span).toFixed(1)},${(H - v * (H - 6)).toFixed(1)}`;
    });
    return `${d} L${(x0 + span).toFixed(1)},${H} Z`;
  });
  const keepX = $derived(ir ? ((pre + ir.seconds * keep) / total) * W : W);
  const chans = (n: number) => (n === 1 ? "mono" : n === 2 ? "stereo" : n === 4 ? "true stereo (4 channels)" : `${n} channels, used as stereo`);
</script>

{#snippet dial(k: string, label: string)}
  {@const p = P(k)}
  <div class="dial"><BigKnob param={p} value={val(k)} size={64} {label} oninput={(v) => set(k, v, false)} oncommit={(v) => set(k, v, true)} /></div>
{/snippet}

<div class="wrap">
{#if onKind}<KindTabs kinds={REVERB_KINDS} current={REVERB_KINDS.length - 1} tips={REVERB_TIPS} label="Kind of reverb" pick={(i) => i < REVERB_KINDS.length - 1 && onKind(i)} />{/if}
<div class="verb">
  <div class="plate">
    <span class="title">Convolution Reverb</span>
    <span class="sub">{ir ? `${ir.name} · ${ir.seconds.toFixed(2)} s · ${chans(ir.channels)} · ${(ir.rate / 1000).toFixed(1)} kHz` : saved?.name ? saved.name : "no impulse response loaded"}</span>
  </div>
  <div class="ir">
    <svg viewBox="0 0 {W} {H}" preserveAspectRatio="none" role="img" aria-label="The impulse response over time">
      <defs>
        <linearGradient id="verb-fill-{slot.id}" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stop-color="#b48cff" stop-opacity="0.85" />
          <stop offset="100%" stop-color="#5b3fd1" stop-opacity="0.15" />
        </linearGradient>
        <clipPath id="verb-keep-{slot.id}"><rect x="0" y="0" width={keepX} height={H} /></clipPath>
      </defs>
      {#each [0.25, 0.5, 0.75] as g (g)}<line class="grid" x1="0" x2={W} y1={H * g} y2={H * g} />{/each}
      {#if path}
        <path class="cut" d={path} />
        <path d={path} fill="url(#verb-fill-{slot.id})" clip-path="url(#verb-keep-{slot.id})" />
        {#if keep < 0.999}<line class="edge" x1={keepX} x2={keepX} y1="0" y2={H} />{/if}
        {#if pre > 0}<line class="edge pre" x1={(pre / total) * W} x2={(pre / total) * W} y1="0" y2={H} />{/if}
      {/if}
    </svg>
    {#if !ir}
      <div class="empty">{missing ?? "Load any audio file of a space's echo: a room, hall, plate, spring, or something stranger."}</div>
    {/if}
  </div>
  {#if ir && missing === null}<p class="where" title={ir.path}>{ir.path}</p>{/if}
  <div class="bar">
    <button type="button" class="load" disabled={busy} onclick={() => void load()}>{busy ? "Loading…" : ir || saved ? "Load Another…" : "Load Impulse Response…"}</button>
    <button type="button" disabled={!slot.data} onclick={() => onData(null, "Clear Impulse Response")}>Clear</button>
  </div>
  <div class="controls">
    <div class="dials">
      {@render dial("predelay", "Pre-delay")}
      {@render dial("length", "Length")}
      {@render dial("width", "Width")}
      {@render dial("low_cut", "Low cut")}
      {@render dial("high_cut", "High cut")}
    </div>
    <div class="faders">
      {#each [["dry", "Dry"], ["wet", "Wet"]] as const as [k, label] (k)}
        <div class="fader">
          <VSlider param={P(k)} value={val(k)} height={110} oninput={(v) => set(k, v, false)} oncommit={(v) => set(k, v, true)} />
          <span class="lvl">{label}{val(k) <= P(k).min + 1e-9 ? ": off" : ""}</span>
        </div>
      {/each}
    </div>
  </div>
</div>
</div>

<style>
  /* Tabs above, the panel below: one column. */
  .wrap {
    display: grid;
    min-width: 0;
  }
  .verb {
    --ink: #ece4ff;
    --violet: #b48cff;
    display: grid;
    gap: 10px;
    margin-bottom: 6px;
    padding: 10px 12px 12px;
    border: 1px solid #000;
    border-radius: 6px;
    background:
      radial-gradient(120% 90% at 80% 0%, rgba(180, 140, 255, 0.14), transparent 60%),
      linear-gradient(180deg, #1c1828, #100e17);
    box-shadow:
      inset 0 1px 0 rgba(255, 255, 255, 0.07),
      0 2px 8px rgba(0, 0, 0, 0.5);
    color: var(--ink);
  }
  .plate {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 10px;
    align-items: baseline;
    padding-bottom: 6px;
    border-bottom: 1px solid rgba(180, 140, 255, 0.3);
  }
  .title {
    color: var(--violet);
    font-size: 14px;
    font-weight: 700;
    letter-spacing: 0.1em;
    text-transform: uppercase;
  }
  .sub {
    min-width: 0;
    overflow: hidden;
    color: rgba(236, 228, 255, 0.6);
    font-size: 11px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ir {
    position: relative;
    height: 96px;
    border: 1px solid rgba(180, 140, 255, 0.2);
    border-radius: 4px;
    background: radial-gradient(100% 140% at 0% 100%, #1d1533, #09080e);
    overflow: hidden;
  }
  .ir svg {
    display: block;
    width: 100%;
    height: 100%;
  }
  .grid {
    stroke: rgba(180, 140, 255, 0.08);
    vector-effect: non-scaling-stroke;
  }
  .cut {
    fill: rgba(180, 140, 255, 0.12);
  }
  .edge {
    stroke: rgba(236, 228, 255, 0.7);
    stroke-dasharray: 3 3;
    vector-effect: non-scaling-stroke;
  }
  .edge.pre {
    stroke: rgba(180, 140, 255, 0.6);
  }
  .empty {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    padding: 0 20px;
    color: rgba(236, 228, 255, 0.6);
    font-size: 11px;
    text-align: center;
  }
  .where {
    margin: -4px 0 0;
    overflow: hidden;
    color: rgba(236, 228, 255, 0.4);
    font-size: 10px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .bar {
    display: flex;
    gap: 6px;
  }
  .bar button {
    padding: 3px 10px;
    border: 1px solid rgba(180, 140, 255, 0.4);
    border-radius: 3px;
    background: rgba(0, 0, 0, 0.3);
    color: var(--ink);
    font: inherit;
  }
  .bar .load {
    background: linear-gradient(180deg, #c3a2ff, #7c55e0);
    color: #140c24;
    font-weight: 600;
  }
  .bar button:disabled {
    opacity: 0.45;
  }
  .controls {
    display: flex;
    gap: 10px;
    align-items: center;
  }
  .dials {
    display: flex;
    flex: 1;
    flex-wrap: wrap;
    justify-content: space-between;
    gap: 4px;
  }
  .dial {
    display: grid;
    justify-items: center;
  }
  .faders {
    display: flex;
    gap: 4px;
    padding-left: 8px;
    border-left: 1px solid rgba(180, 140, 255, 0.2);
  }
  .fader {
    display: grid;
    justify-items: center;
    gap: 2px;
  }
  .lvl {
    color: var(--violet);
    font-size: 10px;
  }
</style>
