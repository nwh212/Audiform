<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The Audiform contributors.
  This file is part of Audiform, based on Altered Audio Editor.

  The Center Channel Extractor's window. At the top, the stereo stage
  seen from above: the listener at the bottom, left to right across, and
  the place being looked for lit up (its spread follows Amplitude
  discrimination). Drag the light to pick a pan of your own. Beside it,
  the Center and Side level faders: Center all the way down takes the
  vocal out, Side all the way down keeps only the vocal. Below, two
  tabs. Extraction: what to look for (Center, Left, Right, Surround or
  Custom, with its phase angle, pan and delay) and the frequency range.
  Discrimination: how strict the match is. Advanced holds the FFT size.
  Every change is one undoable step.
-->
<script lang="ts">
  import type { EffectInfo, EffectParamInfo } from "../../app/core";
  import type { EffectSlotDoc } from "../../stores/session";
  import BigKnob from "./BigKnob.svelte";
  import { paramValue } from "./chain";
  import ValueField from "./ValueField.svelte";
  import VSlider from "./VSlider.svelte";

  interface Props {
    slot: EffectSlotDoc;
    info: EffectInfo;
    onParams: (changes: [EffectParamInfo, number][], label: string, done: boolean) => void;
  }

  const { slot, info, onParams }: Props = $props();

  const byKey = $derived(new Map(info.params.map((p) => [p.key, p])));
  const P = (k: string) => byKey.get(k)!;
  const val = (k: string) => paramValue(slot, P(k));
  const set = (k: string, v: number, done: boolean) => {
    const p = P(k);
    onParams([[p, Math.max(p.min, Math.min(p.max, v))]], `${info.name} ${p.name}`, done);
  };

  const EXTRACT = ["Center", "Left", "Right", "Surround", "Custom"];
  const RANGES = ["Full spectrum", "Bass", "Male voice", "Female voice", "Custom"];
  const extract = $derived(Math.round(val("extract")));
  const custom = $derived(extract === 4);
  const rangeIx = $derived(Math.round(val("range")));
  /** The pan and phase being looked for. */
  const aim = $derived(
    [
      { pan: 0, phase: 0 },
      { pan: -100, phase: 0 },
      { pan: 100, phase: 0 },
      { pan: 0, phase: 180 },
    ][extract] ?? { pan: val("pan"), phase: val("phase") },
  );

  let tab = $state<"extraction" | "discrimination">("extraction");

  // The stage: an arc from left to right in front of the listener.
  const W = 300;
  const SH = 150;
  const CX = W / 2;
  const CY = SH - 18;
  const RAD = 110;
  /** Pan -100..100 to an angle on the arc (degrees from straight ahead). */
  const angOf = (pan: number) => (pan / 100) * 70;
  const pt = (deg: number, r: number) => {
    const a = ((deg - 90) * Math.PI) / 180;
    return [CX + r * Math.cos(a), CY + r * Math.sin(a)] as const;
  };
  // Behind the listener for sound out of phase.
  const behind = $derived(Math.abs(aim.phase) > 90);
  const spot = $derived(behind ? ([CX + (aim.pan / 100) * 60, CY + 6] as const) : pt(angOf(aim.pan), RAD));
  /** Spread of the lit wedge, from Amplitude discrimination (0.5 to 20 dB). */
  const spread = $derived(6 + (val("amp_disc") / 20) * 40);
  const wedge = $derived.by(() => {
    if (behind) return "";
    const a = angOf(aim.pan);
    const [x1, y1] = pt(a - spread / 2, RAD + 16);
    const [x2, y2] = pt(a + spread / 2, RAD + 16);
    return `M${CX},${CY} L${x1.toFixed(1)},${y1.toFixed(1)} A${RAD + 16},${RAD + 16} 0 0 1 ${x2.toFixed(1)},${y2.toFixed(1)} Z`;
  });
  const arc = (() => {
    const [x1, y1] = pt(-70, RAD);
    const [x2, y2] = pt(70, RAD);
    return `M${x1.toFixed(1)},${y1.toFixed(1)} A${RAD},${RAD} 0 0 1 ${x2.toFixed(1)},${y2.toFixed(1)}`;
  })();

  let stage = $state<SVGSVGElement | null>(null);
  function dragSpot(e: PointerEvent) {
    if (e.button !== 0 || !stage) return;
    e.preventDefault();
    const el = e.currentTarget as SVGElement;
    el.setPointerCapture(e.pointerId);
    const panAt = (ev: PointerEvent) => {
      const b = stage!.getBoundingClientRect();
      const x = ((ev.clientX - b.left) / b.width) * W - CX;
      const y = ((ev.clientY - b.top) / b.height) * SH - CY;
      const deg = (Math.atan2(x, -y) * 180) / Math.PI;
      return Math.round(Math.max(-100, Math.min(100, (deg / 70) * 100)));
    };
    const at = (ev: PointerEvent, done: boolean) => {
      const changes: [EffectParamInfo, number][] = [[P("pan"), panAt(ev)]];
      if (!custom) changes.push([P("extract"), 4], [P("phase"), aim.phase]);
      onParams(changes, `${info.name} Pan`, done);
    };
    at(e, false);
    const move = (ev: PointerEvent) => at(ev, false);
    const up = (ev: PointerEvent) => {
      el.removeEventListener("pointermove", move);
      el.removeEventListener("pointerup", up);
      at(ev, true);
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up);
  }

  const range = $derived.by(() => {
    const hz: [number, number][] = [
      [20, 20_000],
      [20, 250],
      [90, 10_000],
      [160, 12_000],
    ];
    const r = hz[rangeIx];
    if (r) return r;
    const half = Math.pow(2, val("width") / 2);
    return [val("center_freq") / half, val("center_freq") * half] as [number, number];
  });
  const hz = (f: number) => (f >= 1000 ? `${(f / 1000).toFixed(f >= 10_000 ? 0 : 1)} kHz` : `${Math.round(f)} Hz`);
  const levelText = (k: string) => (val(k) <= P(k).min + 1e-9 ? "off" : `${val(k) > 0 ? "+" : ""}${val(k).toFixed(1)} dB`);
</script>

{#snippet field(k: string, label: string, disabled = false)}
  {@const p = P(k)}
  <label class="field" class:disabled>
    <span>{label}</span>
    <ValueField param={p} value={val(k)} oninput={(v) => set(k, v, false)} oncommit={(v) => set(k, v, true)} />
  </label>
{/snippet}
{#snippet knob(k: string, label: string)}
  {@const p = P(k)}
  <div class="knob">
    <BigKnob param={p} value={val(k)} size={64} {label} oninput={(v) => set(k, v, false)} oncommit={(v) => set(k, v, true)} />
  </div>
{/snippet}
{#snippet seg(k: string, names: string[], current: number)}
  <div class="seg" role="radiogroup" aria-label={P(k).name}>
    {#each names as n, i (n)}
      <button type="button" role="radio" aria-checked={current === i} class:on={current === i} onclick={() => set(k, i, true)}>{n}</button>
    {/each}
  </div>
{/snippet}

<div class="cce">
  <div class="plate">
    <span class="title">Center Channel Extractor</span>
    <span class="sub">{extract === 3 ? "out-of-phase sound" : custom ? `pan ${aim.pan > 0 ? "+" : ""}${Math.round(aim.pan)}, ${Math.round(aim.phase)}°` : `${EXTRACT[extract]?.toLowerCase()} of the mix`} · {hz(range[0])} to {hz(range[1])}</span>
  </div>
  <div class="top">
    <svg bind:this={stage} class="stage" viewBox="0 0 {W} {SH}" role="img" aria-label="Where in the stereo field it looks">
      <defs>
        <radialGradient id="cce-glow-{slot.id}" cx="50%" cy="50%" r="50%">
          <stop offset="0%" stop-color="#7ff7ea" stop-opacity="0.95" />
          <stop offset="60%" stop-color="#2fc3d6" stop-opacity="0.35" />
          <stop offset="100%" stop-color="#2fc3d6" stop-opacity="0" />
        </radialGradient>
        <linearGradient id="cce-wedge-{slot.id}" x1="0" y1="1" x2="0" y2="0">
          <stop offset="0%" stop-color="#2fc3d6" stop-opacity="0.02" />
          <stop offset="100%" stop-color="#2fc3d6" stop-opacity="0.32" />
        </linearGradient>
      </defs>
      {#each [0.35, 0.6, 0.85] as k (k)}
        <path class="ring" d="M{pt(-70, RAD * k)[0]},{pt(-70, RAD * k)[1]} A{RAD * k},{RAD * k} 0 0 1 {pt(70, RAD * k)[0]},{pt(70, RAD * k)[1]}" />
      {/each}
      {#each [-70, -35, 0, 35, 70] as a (a)}
        <line class="ray" x1={CX} y1={CY} x2={pt(a, RAD)[0]} y2={pt(a, RAD)[1]} />
      {/each}
      {#if wedge}<path d={wedge} fill="url(#cce-wedge-{slot.id})" />{/if}
      <path class="arc" d={arc} />
      <text class="lab" x={pt(-70, RAD + 10)[0] - 4} y={pt(-70, RAD + 10)[1]} text-anchor="end">L</text>
      <text class="lab" x={pt(70, RAD + 10)[0] + 4} y={pt(70, RAD + 10)[1]} text-anchor="start">R</text>
      <text class="lab" x={CX + 30} y={CY - RAD + 4} text-anchor="middle">C</text>
      <circle class="head" cx={CX} cy={CY} r="7" />
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <g class="spot" transform="translate({spot[0]} {spot[1]})" onpointerdown={dragSpot}>
        <circle r="22" fill="url(#cce-glow-{slot.id})" />
        <circle class="core" r="5" />
        <title>Drag to look at another pan</title>
      </g>
    </svg>
    <div class="faders">
      {#each [["center", "Center"], ["side", "Side"]] as const as [k, label] (k)}
        <div class="fader">
          <VSlider param={P(k)} value={val(k)} height={112} oninput={(v) => set(k, v, false)} oncommit={(v) => set(k, v, true)} />
          <span class="lvl" class:off={levelText(k) === "off"}>{label}: {levelText(k)}</span>
        </div>
      {/each}
    </div>
  </div>

  <div class="tabs" role="tablist">
    <button type="button" role="tab" aria-selected={tab === "extraction"} class:on={tab === "extraction"} onclick={() => (tab = "extraction")}>Extraction</button>
    <button type="button" role="tab" aria-selected={tab === "discrimination"} class:on={tab === "discrimination"} onclick={() => (tab = "discrimination")}>Discrimination</button>
  </div>
  <div class="page" role="tabpanel">
    {#if tab === "extraction"}
      <div class="line"><span class="lname">Extract</span>{@render seg("extract", EXTRACT, extract)}</div>
      <div class="fields">
        {@render field("phase", "Phase angle", !custom)}
        {@render field("pan", "Pan", !custom)}
        {@render field("delay", "Delay", !custom)}
      </div>
      <div class="line"><span class="lname">Frequency range</span>{@render seg("range", RANGES, rangeIx)}</div>
      <div class="fields">
        {@render field("center_freq", "Center frequency", rangeIx !== 4)}
        {@render field("width", "Width", rangeIx !== 4)}
      </div>
    {:else}
      <div class="knobs">
        {@render knob("bleed", "Crossover bleed")}
        {@render knob("phase_disc", "Phase")}
        {@render knob("amp_disc", "Amplitude")}
        {@render knob("amp_band", "Amp. bandwidth")}
        {@render knob("decay", "Spectral decay")}
      </div>
      <p class="hint">Lower Phase and Amplitude find only sound right at the spot; higher catch more around it. Crossover bleed softens the edge. Spectral decay smooths a fluttery, watery sound.</p>
    {/if}
  </div>
  <details class="adv">
    <summary>Advanced</summary>
    <label class="field">
      <span>FFT size</span>
      <select value={String(Math.round(val("fft")))} onchange={(e) => set("fft", Number(e.currentTarget.value), true)}>
        {#each P("fft").choices ?? [] as c, i (c)}<option value={String(i)}>{c}</option>{/each}
      </select>
    </label>
    <p class="hint">Larger sizes separate low frequencies better but smear quick sounds and add delay.</p>
  </details>
</div>

<style>
  .cce {
    --ink: #d7f4f1;
    --cyan: #3ed6d0;
    display: grid;
    gap: 10px;
    margin-bottom: 6px;
    padding: 10px 12px 12px;
    border: 1px solid #000;
    border-radius: 6px;
    background:
      radial-gradient(120% 80% at 30% 0%, rgba(62, 214, 208, 0.12), transparent 60%),
      linear-gradient(180deg, #172028, #0e1318);
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
    border-bottom: 1px solid rgba(62, 214, 208, 0.3);
  }
  .title {
    color: var(--cyan);
    font-size: 14px;
    font-weight: 700;
    letter-spacing: 0.1em;
    text-transform: uppercase;
  }
  .sub {
    color: rgba(215, 244, 241, 0.6);
    font-size: 11px;
  }
  .top {
    display: flex;
    gap: 10px;
    align-items: stretch;
  }
  .stage {
    flex: 1;
    min-width: 0;
    border: 1px solid rgba(62, 214, 208, 0.18);
    border-radius: 4px;
    background: radial-gradient(90% 120% at 50% 100%, #10262b, #080c10);
    user-select: none;
  }
  .ring {
    fill: none;
    stroke: rgba(62, 214, 208, 0.12);
  }
  .ray {
    stroke: rgba(62, 214, 208, 0.1);
  }
  .arc {
    fill: none;
    stroke: rgba(62, 214, 208, 0.55);
    stroke-width: 1.5;
  }
  .lab {
    fill: rgba(215, 244, 241, 0.7);
    font-size: 10px;
    font-weight: 600;
  }
  .head {
    fill: #1d3036;
    stroke: rgba(215, 244, 241, 0.6);
  }
  .spot {
    cursor: grab;
  }
  .spot .core {
    fill: #eafffd;
    stroke: #0b1a1d;
  }
  .faders {
    display: flex;
    gap: 4px;
  }
  .fader {
    display: grid;
    justify-items: center;
    gap: 2px;
  }
  .lvl {
    color: var(--cyan);
    font-size: 10px;
    white-space: nowrap;
  }
  .lvl.off {
    color: #ff8a7a;
  }
  .tabs {
    display: flex;
    gap: 2px;
    border-bottom: 1px solid rgba(62, 214, 208, 0.25);
  }
  .tabs button {
    padding: 4px 14px;
    border: 1px solid transparent;
    border-bottom: 0;
    border-radius: 4px 4px 0 0;
    background: none;
    color: rgba(215, 244, 241, 0.6);
    font: inherit;
  }
  .tabs button.on {
    border-color: rgba(62, 214, 208, 0.3);
    background: rgba(62, 214, 208, 0.1);
    color: var(--ink);
  }
  .page {
    display: grid;
    gap: 8px;
    min-height: 150px;
  }
  .line {
    display: grid;
    gap: 4px;
  }
  .lname {
    color: rgba(215, 244, 241, 0.7);
    font-size: 11px;
  }
  .seg {
    display: flex;
    flex-wrap: wrap;
    border: 1px solid rgba(62, 214, 208, 0.3);
    border-radius: 4px;
    overflow: hidden;
  }
  .seg button {
    flex: 1;
    padding: 3px 6px;
    border: 0;
    border-right: 1px solid rgba(62, 214, 208, 0.2);
    background: rgba(0, 0, 0, 0.25);
    color: rgba(215, 244, 241, 0.75);
    font: inherit;
    font-size: 11px;
    white-space: nowrap;
  }
  .seg button:last-child {
    border-right: 0;
  }
  .seg button.on {
    background: linear-gradient(180deg, #43e0d8, #1e9fa6);
    color: #04191b;
    font-weight: 600;
  }
  .fields {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 14px;
  }
  .field {
    display: flex;
    gap: 6px;
    align-items: center;
    font-size: 11px;
    color: rgba(215, 244, 241, 0.75);
  }
  .field :global(.vf) {
    width: 86px;
  }
  .field.disabled {
    opacity: 0.4;
    pointer-events: none;
  }
  .knobs {
    display: flex;
    justify-content: space-between;
    gap: 4px;
  }
  .knob {
    display: grid;
    justify-items: center;
  }
  .hint {
    margin: 0;
    color: rgba(215, 244, 241, 0.55);
    font-size: 11px;
  }
  .adv summary {
    color: rgba(215, 244, 241, 0.7);
    font-size: 11px;
    cursor: pointer;
  }
  .adv {
    display: grid;
    gap: 6px;
  }
  .adv[open] {
    padding-bottom: 2px;
  }
  .adv .field {
    margin: 6px 0 4px;
  }
  select {
    background: #0b1216;
    border: 1px solid rgba(62, 214, 208, 0.35);
    border-radius: 2px;
    color: var(--ink);
    font: inherit;
    padding: 1px 4px;
  }
</style>
