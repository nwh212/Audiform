<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The Audiform contributors.
  This file is part of Audiform, based on Altered Audio Editor.

  The Delay and Echo window, as a piece of hardware. Across the top, the
  kind (Digital, Tape, Analog: each has its own faceplate color) and the
  Follow Tempo and Ping-pong switches. In the middle, the two big dials
  (Time, or the note value when following the tempo, and Feedback)
  either side of a lit display with the time and a picture of the
  repeats fading away (alternating sides when they ping-pong). Under
  them, a row of small dials for the modulation, filters, drive and the
  levels.
-->
<script lang="ts">
  import type { EffectInfo, EffectParamInfo } from "../../app/core";
  import type { EffectSlotDoc } from "../../stores/session";
  import BigKnob from "./BigKnob.svelte";
  import { formatParam, paramValue } from "./chain";

  interface Props {
    slot: EffectSlotDoc;
    info: EffectInfo;
    /** The session tempo, to follow. */
    tempo?: number | undefined;
    onParams: (changes: [EffectParamInfo, number][], label: string, done: boolean) => void;
  }

  const { slot, info, tempo, onParams }: Props = $props();
  const P = (k: string) => info.params.find((p) => p.key === k)!;
  const val = (k: string) => paramValue(slot, P(k));
  const set = (k: string, v: number, done: boolean) => {
    const p = P(k);
    onParams([[p, Math.max(p.min, Math.min(p.max, v))]], `${info.name} ${p.name}`, done);
  };

  const KINDS = [
    { name: "Digital", tip: "Clean repeats, each a copy of the last" },
    { name: "Tape", tip: "A tape loop: wow, flutter and saturation, each pass a little darker" },
    { name: "Analog", tip: "A bucket-brigade delay: darker repeats that clip softly" },
  ];
  const BEATS = [1 / 16, 1 / 12, 1 / 8, 1 / 6, 0.25, 0.375, 1 / 3, 0.5, 0.75, 2 / 3, 1, 1.5, 2, 3, 4, 8];

  const kind = $derived(Math.round(val("kind")));
  const sync = $derived(val("sync") >= 0.5);
  const pingpong = $derived(val("pingpong") >= 0.5);
  const bpm = $derived(val("tempo"));
  const ms = $derived(sync ? ((BEATS[Math.round(val("note"))] ?? 0.5) * 60_000) / Math.max(30, bpm) : val("time"));
  const fb = $derived(val("feedback") / 100);

  function follow(on: boolean) {
    const changes: [EffectParamInfo, number][] = [[P("sync"), on ? 1 : 0]];
    if (on && tempo) changes.push([P("tempo"), Math.max(30, Math.min(300, tempo))]);
    onParams(changes, on ? "Delay Follows Tempo" : "Delay Time in ms", true);
  }

  // The repeats over the next few seconds: when each comes, how loud, which side.
  const W = 240;
  const H = 64;
  const repeats = $derived.by(() => {
    const span = Math.max(1500, Math.min(6000, ms * 8));
    const out: { x: number; h: number; up: boolean }[] = [];
    let level = 1;
    for (let k = 1; k * ms <= span && out.length < 48; k++) {
      out.push({ x: (k * ms * W) / span, h: Math.min(1, level), up: !pingpong || k % 2 === 1 });
      level *= fb;
      if (level < 0.01) break;
    }
    return { list: out, span };
  });
  const msText = (v: number) => (v >= 1000 ? `${(v / 1000).toFixed(2)} s` : `${Math.round(v)} ms`);
</script>

<div class="shell"><div class="unit kind{kind}">
  <div class="top">
    <div class="kinds" role="tablist" aria-label="Kind of delay">
      {#each KINDS as k, i (k.name)}
        <button type="button" role="tab" class:lit={i === kind} aria-selected={i === kind} title={k.tip} onclick={() => i !== kind && set("kind", i, true)}>{k.name}</button>
      {/each}
    </div>
    <div class="switches">
      <button type="button" class:lit={sync} aria-pressed={sync} title="The time follows the tempo as a note value" onclick={() => follow(!sync)}>Tempo</button>
      <button type="button" class:lit={pingpong} aria-pressed={pingpong} title="Each repeat crosses to the other side" onclick={() => set("pingpong", pingpong ? 0 : 1, true)}>Ping-pong</button>
    </div>
  </div>

  <div class="mid">
    {#if sync}
      <BigKnob param={P("note")} value={val("note")} size={104} label="Time" oninput={(v) => set("note", Math.round(v), false)} oncommit={(v) => set("note", Math.round(v), true)} />
    {:else}
      <BigKnob param={P("time")} value={val("time")} size={104} label="Time" marks={[1, 100, 1000, 4000]} oninput={(v) => set("time", v, false)} oncommit={(v) => set("time", v, true)} />
    {/if}
    <div class="screen" aria-live="polite">
      <div class="readout">
        {#if sync}
          <span class="big">{formatParam(P("note"), val("note"))}</span>
          <span class="small">{Math.round(bpm * 10) / 10} BPM · {msText(ms)}</span>
        {:else}
          <span class="big">{msText(ms)}</span>
          <span class="small">{kind === 1 ? "tape" : kind === 2 ? "analog" : "digital"}</span>
        {/if}
      </div>
      <svg viewBox="0 0 {W} {H}" preserveAspectRatio="none" class="echoes" role="img" aria-label="The repeats fading away">
        <line class="axis" x1="0" x2={W} y1={H / 2} y2={H / 2} />
        <rect class="dry" x="0" y={H / 2 - (H / 2 - 4)} width="3" height={H - 8} />
        {#each repeats.list as r, i (i)}
          {@const len = r.h * (H / 2 - 4)}
          <rect class="rep" x={r.x - 1.5} y={r.up ? H / 2 - len : H / 2} width="3" height={Math.max(1, len)} />
        {/each}
      </svg>
      {#if sync && tempo && Math.abs(tempo - bpm) > 0.05}
        <button type="button" class="use" title="Use the session tempo" onclick={() => set("tempo", tempo, true)}>Use session {tempo} BPM</button>
      {/if}
    </div>
    <BigKnob param={P("feedback")} value={val("feedback")} size={104} label="Feedback" marks={[0, 60, 120]} oninput={(v) => set("feedback", v, false)} oncommit={(v) => set("feedback", v, true)} />
  </div>

  <div class="row">
    <div class="group">
      <span class="gname">Modulation</span>
      <div class="knobs">
        <BigKnob param={P("depth")} value={val("depth")} size={56} label={kind === 1 ? "Wow" : "Depth"} oninput={(v) => set("depth", v, false)} oncommit={(v) => set("depth", v, true)} />
        <BigKnob param={P("rate")} value={val("rate")} size={56} label="Rate" oninput={(v) => set("rate", v, false)} oncommit={(v) => set("rate", v, true)} />
      </div>
    </div>
    <div class="group">
      <span class="gname">Filters</span>
      <div class="knobs">
        <BigKnob param={P("low_cut")} value={val("low_cut")} size={56} label="Lo cut" oninput={(v) => set("low_cut", v, false)} oncommit={(v) => set("low_cut", v, true)} />
        <BigKnob param={P("tone")} value={val("tone")} size={56} label="Hi cut" oninput={(v) => set("tone", v, false)} oncommit={(v) => set("tone", v, true)} />
      </div>
    </div>
    <div class="group">
      <span class="gname">Output</span>
      <div class="knobs">
        <BigKnob param={P("drive")} value={val("drive")} size={56} label="Drive" oninput={(v) => set("drive", v, false)} oncommit={(v) => set("drive", v, true)} />
        <BigKnob param={P("dry")} value={val("dry")} size={56} label="Dry" oninput={(v) => set("dry", v, false)} oncommit={(v) => set("dry", v, true)} />
        <BigKnob param={P("wet")} value={val("wet")} size={56} label="Echo" oninput={(v) => set("wet", v, false)} oncommit={(v) => set("wet", v, true)} />
      </div>
    </div>
  </div>
</div></div>

<style>
  .shell {
    container-type: inline-size;
  }
  .unit {
    --face: #1f262c;
    --face2: #141a1f;
    --panel-ink: #d6e6ee;
    --panel-accent: #5fd3f3;
    --glow: #5fd3f3;
    display: grid;
    gap: 12px;
    margin: 4px 6px 6px;
    padding: 12px 14px 14px;
    border: 1px solid #000;
    border-radius: 8px;
    background:
      radial-gradient(130% 90% at 50% 0%, color-mix(in srgb, var(--glow) 10%, transparent), transparent 60%),
      linear-gradient(180deg, var(--face), var(--face2));
    box-shadow:
      inset 0 1px 0 rgba(255, 255, 255, 0.08),
      0 2px 10px rgba(0, 0, 0, 0.5);
    color: var(--panel-ink);
  }
  .unit.kind1 {
    --face: #3a2c1e;
    --face2: #221911;
    --panel-ink: #f1e2c6;
    --panel-accent: #f0b35a;
    --glow: #f0b35a;
  }
  .unit.kind2 {
    --face: #331d1f;
    --face2: #1c1011;
    --panel-ink: #f0d6d2;
    --panel-accent: #ff6b5e;
    --glow: #ff6b5e;
  }
  .top {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    justify-content: space-between;
  }
  .kinds,
  .switches {
    display: flex;
    gap: 4px;
  }
  /* A faceplate's own switches: the theme's button look stays out. */
  .unit .top button {
    min-width: 0;
    height: auto;
    padding: 4px 10px !important;
    border: 1px solid #000 !important;
    border-radius: 4px !important;
    background: linear-gradient(180deg, #3b3b3b, #232323) !important;
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.12) !important;
    color: color-mix(in srgb, var(--panel-ink) 70%, transparent) !important;
    text-shadow: none !important;
    white-space: nowrap;
    font: inherit;
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    cursor: pointer;
  }
  .unit .top button.lit {
    color: #111 !important;
    background: linear-gradient(180deg, color-mix(in srgb, var(--glow) 90%, #fff), var(--glow)) !important;
    box-shadow:
      0 0 8px color-mix(in srgb, var(--glow) 60%, transparent),
      inset 0 1px 0 rgba(255, 255, 255, 0.4) !important;
  }
  .mid {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto;
    gap: 14px;
    align-items: center;
  }
  .screen {
    display: grid;
    gap: 6px;
    padding: 8px 10px;
    border: 2px solid #000;
    border-radius: 6px;
    background: linear-gradient(180deg, #070908, #0f1311);
    box-shadow:
      inset 0 0 14px rgba(0, 0, 0, 0.9),
      0 1px 0 rgba(255, 255, 255, 0.06);
  }
  .readout {
    display: flex;
    gap: 10px;
    align-items: baseline;
    justify-content: space-between;
  }
  .readout {
    flex-wrap: wrap;
  }
  .big {
    white-space: nowrap;
    color: var(--glow);
    font-family: ui-monospace, "Cascadia Mono", Consolas, monospace;
    font-size: 26px;
    font-weight: 700;
    text-shadow: 0 0 8px color-mix(in srgb, var(--glow) 70%, transparent);
  }
  .small {
    color: color-mix(in srgb, var(--glow) 70%, #888);
    font-family: ui-monospace, "Cascadia Mono", Consolas, monospace;
    font-size: 11px;
  }
  .echoes {
    width: 100%;
    height: 64px;
  }
  .axis {
    stroke: color-mix(in srgb, var(--glow) 25%, transparent);
    stroke-width: 1;
  }
  .dry {
    fill: color-mix(in srgb, var(--panel-ink) 60%, transparent);
  }
  .rep {
    fill: var(--glow);
    filter: drop-shadow(0 0 2px var(--glow));
  }
  .use {
    justify-self: start;
    padding: 2px 8px;
    border: 1px solid color-mix(in srgb, var(--glow) 50%, transparent);
    border-radius: 3px;
    background: transparent;
    color: var(--glow);
    font: inherit;
    font-size: 11px;
    cursor: pointer;
  }
  .row {
    display: flex;
    flex-wrap: nowrap;
    gap: 8px;
    justify-content: center;
    padding-top: 8px;
    border-top: 1px solid rgba(255, 255, 255, 0.08);
  }
  .group {
    display: grid;
    gap: 2px;
    justify-items: center;
    padding: 4px 6px;
    border: 1px solid rgba(255, 255, 255, 0.07);
    border-radius: 6px;
    background: rgba(0, 0, 0, 0.18);
  }
  .gname {
    color: color-mix(in srgb, var(--panel-ink) 60%, transparent);
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.12em;
    text-transform: uppercase;
  }
  .knobs {
    display: flex;
    gap: 2px;
  }
  .knobs :global(.box) {
    width: 60px;
  }
  .unit :global(.bk .name) {
    white-space: nowrap;
  }
  /* Narrow windows: the display gets its own row above the two big dials,
     and the small dials go two groups to a row. */
  @container (max-width: 600px) {
    .mid {
      grid-template-columns: 1fr 1fr;
      justify-items: center;
    }
    .screen {
      grid-column: 1 / -1;
      grid-row: 1;
      justify-self: stretch;
    }
    .row {
      flex-wrap: wrap;
    }
  }
</style>