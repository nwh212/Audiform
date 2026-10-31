<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The Audiform contributors.
  This file is part of Audiform, based on Altered Audio Editor.

  The Convolution Reverb's window, on the same faceplate as the Reverb's
  other kinds. Across the top, the kinds (Impulse Response lit) and the
  Load and Clear switches. In the middle, a lit display of the impulse
  response: any audio file of a space's echo, drawn as its level over
  time (on a dB scale, so the tail shows), with the part Length keeps
  lit and the Pre-delay drawn as a gap before it. Below, small dials for
  the response, the filters and the levels. Loading or clearing a file
  is one undoable step, like every control.
-->
<script lang="ts">
  import { reverbLoad, reverbPick, type EffectInfo, type EffectParamInfo, type ReverbIr } from "../../app/core";
  import type { EffectSlotDoc } from "../../stores/session";
  import BigKnob from "./BigKnob.svelte";
  import { REVERB_KINDS, REVERB_TIPS } from "./reverb-kinds";
  import { paramValue } from "./chain";
  import "./faceplate.css";

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
  <BigKnob param={P(k)} value={val(k)} size={56} {label} oninput={(v) => set(k, v, false)} oncommit={(v) => set(k, v, true)} />
{/snippet}

<div class="fp-shell">
  <div class="fp-unit ir-unit">
    <div class="fp-top">
      <div class="fp-kinds" role="tablist" aria-label="Kind of reverb">
        {#each REVERB_KINDS as k, i (k)}
          {@const lit = i === REVERB_KINDS.length - 1}
          <button type="button" role="tab" class:lit aria-selected={lit} title={REVERB_TIPS[i]} disabled={!onKind && !lit} onclick={() => !lit && onKind?.(i)}>{k}</button>
        {/each}
      </div>
      <div class="fp-switches">
        <button type="button" class:lit={!ir && !saved} disabled={busy} title="Pick an audio file of a space's echo" onclick={() => void load()}>{busy ? "Loading…" : ir || saved ? "Load another" : "Load"}</button>
        <button type="button" disabled={!slot.data} title="Remove the impulse response" onclick={() => onData(null, "Clear Impulse Response")}>Clear</button>
      </div>
    </div>

    <div class="fp-screen">
      <div class="fp-readout">
        <span class="fp-big">{ir ? `${(ir.seconds * keep).toFixed(2)} s` : "--"}</span>
        <span class="fp-small" title={ir?.path ?? ""}>{ir ? `${ir.name} · ${chans(ir.channels)} · ${(ir.rate / 1000).toFixed(1)} kHz` : saved?.name ? saved.name : "no impulse response loaded"}</span>
      </div>
      <div class="ir">
        <svg viewBox="0 0 {W} {H}" preserveAspectRatio="none" role="img" aria-label="The impulse response over time">
          <defs>
            <linearGradient id="verb-fill-{slot.id}" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stop-color="var(--glow)" stop-opacity="0.85" />
              <stop offset="100%" stop-color="var(--glow)" stop-opacity="0.12" />
            </linearGradient>
            <clipPath id="verb-keep-{slot.id}"><rect x="0" y="0" width={keepX} height={H} /></clipPath>
          </defs>
          {#each [0.25, 0.5, 0.75] as g (g)}<line class="fp-axis" x1="0" x2={W} y1={H * g} y2={H * g} />{/each}
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
    </div>

    <div class="fp-row">
      <div class="fp-group">
        <span class="fp-gname">Response</span>
        <div class="fp-knobs">
          {@render dial("predelay", "Pre-delay")}
          {@render dial("length", "Length")}
          {@render dial("width", "Width")}
        </div>
      </div>
      <div class="fp-group">
        <span class="fp-gname">Filters</span>
        <div class="fp-knobs">
          {@render dial("low_cut", "Lo cut")}
          {@render dial("high_cut", "Hi cut")}
        </div>
      </div>
      <div class="fp-group">
        <span class="fp-gname">Output</span>
        <div class="fp-knobs">
          {@render dial("dry", "Dry")}
          {@render dial("wet", "Wet")}
        </div>
      </div>
    </div>
  </div>
</div>

<style>
  .ir-unit {
    --face: #241d33;
    --face2: #120f1b;
    --panel-ink: #ece4ff;
    --glow: #b48cff;
  }
  .ir {
    position: relative;
    height: 96px;
  }
  .ir svg {
    display: block;
    width: 100%;
    height: 100%;
  }
  .cut {
    fill: color-mix(in srgb, var(--glow) 12%, transparent);
  }
  .edge {
    stroke: color-mix(in srgb, var(--panel-ink) 70%, transparent);
    stroke-dasharray: 3 3;
    vector-effect: non-scaling-stroke;
  }
  .edge.pre {
    stroke: color-mix(in srgb, var(--glow) 60%, transparent);
  }
  .empty {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    padding: 0 20px;
    color: color-mix(in srgb, var(--panel-ink) 60%, transparent);
    font-size: 11px;
    text-align: center;
  }
  .fp-small {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
