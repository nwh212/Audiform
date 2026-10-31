<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The Audiform contributors.
  This file is part of Audiform, based on Altered Audio Editor.

  The Vox Strip window: four modules side by side in signal order
  (Dynamics, Tone, Effects, Space), each with a power light, a style
  stepper and its one or two main controls, over an Input and Output
  bar. Our own look: a graphite faceplate with numbered modules in one
  accent color (the theme's), so the flow reads left to right.
-->
<script lang="ts">
  import { effectMeter, voxToneCurve, type EffectInfo, type EffectParamInfo } from "../../app/core";
  import type { EffectSlotDoc } from "../../stores/session";
  import { formatParam, paramValue } from "./chain";
  import BigKnob from "./BigKnob.svelte";
  import ValueField from "./ValueField.svelte";

  interface Props {
    slot: EffectSlotDoc;
    info: EffectInfo;
    /** The session tempo, for the Space module's delay. */
    tempo?: number | undefined;
    onParams: (changes: [EffectParamInfo, number][], label: string, done: boolean) => void;
  }

  const { slot, info, tempo, onParams }: Props = $props();

  const byKey = $derived(new Map(info.params.map((p) => [p.key, p])));
  const P = (k: string) => byKey.get(k)!;
  const val = (k: string) => paramValue(slot, P(k));
  const set = (k: string, v: number, done: boolean) => {
    const p = P(k);
    onParams([[p, Math.max(p.min, Math.min(p.max, v))]], `${info.name} ${p.name}`, done);
  };
  const on = (k: string) => val(k) >= 0.5;
  const choicesOf = (k: string): string[] => P(k).choices ?? [];

  interface Module {
    key: string;
    name: string;
    power: string;
    style: string;
    trim: string;
    color: string;
  }
  const MODULES: Module[] = [
    { key: "dyn", name: "Dynamics", power: "dyn_on", style: "dyn_style", trim: "dyn_trim", color: "var(--accent)" },
    { key: "tone", name: "Tone", power: "tone_on", style: "tone_style", trim: "tone_trim", color: "var(--accent)" },
    { key: "fx", name: "Effects", power: "fx_on", style: "fx_style", trim: "fx_trim", color: "var(--accent)" },
    { key: "space", name: "Space", power: "space_on", style: "space_style", trim: "space_trim", color: "var(--accent)" },
  ];

  function step(m: Module, d: number) {
    const n = choicesOf(m.style).length;
    set(m.style, (((Math.round(val(m.style)) + d) % n) + n) % n, true);
  }

  // Meters: gain reduction, output and de-essing, read about 30 times a second.
  let gr = $state(0);
  let ds = $state(0);
  let out = $state(-120);
  $effect(() => {
    const id = slot.id;
    let alive = true;
    let busy = false;
    const timer = setInterval(() => {
      if (busy || !alive) return;
      busy = true;
      effectMeter(id)
        .then((m) => {
          if (!m || !alive) return;
          gr = m[0];
          out = m[1];
          ds = m[2] ?? 0;
        })
        .catch(() => {})
        .finally(() => (busy = false));
    }, 33);
    return () => {
      alive = false;
      clearInterval(timer);
    };
  });

  // The Tone curve, from the core so it matches what is heard.
  let curve = $state<[number, number][]>([]);
  $effect(() => {
    const s = $state.snapshot(slot) as EffectSlotDoc;
    let alive = true;
    voxToneCurve(s, 96)
      .then((c) => alive && (curve = c))
      .catch(() => {});
    return () => {
      alive = false;
    };
  });
  const CW = 200;
  const CH = 64;
  const curvePath = $derived.by(() => {
    if (curve.length < 2) return "";
    const x = (f: number) => (Math.log10(f / 20) / 3) * CW;
    const y = (db: number) => CH / 2 - (Math.max(-15, Math.min(15, db)) / 15) * (CH / 2 - 4);
    return curve.map(([f, db], i) => `${i ? "L" : "M"}${x(f).toFixed(1)},${y(db).toFixed(1)}`).join(" ");
  });

  /** A bar meter's fill, 0 to 1, for 0 to 24 dB of reduction. */
  const fill = (db: number) => Math.max(0, Math.min(1, db / 24));
  const outFill = (db: number) => Math.max(0, Math.min(1, (db + 60) / 60));

  const FX_ICONS: Record<string, string> = {
    Chorus: "M3 14c3-6 5-6 8 0s5 6 8 0 5-6 8 0",
    Doubler: "M6 8h8v12H6zM16 8h8v12h-8z",
    Saturate: "M3 20c4 0 5-12 12-12s8 12 12 12",
    Radio: "M5 11h20v11H5zM8 7l9 4M19 16a2.5 2.5 0 1 0 0.01 0",
    Widen: "M15 6v16M4 14h7M8 11l-4 3 4 3M26 14h-7M22 11l4 3-4 3",
  };
  const SPACE_ICONS: Record<string, string> = {
    Plate: "M5 7h16l4 4v10H9l-4-4z",
    Room: "M5 22V10l10-5 10 5v12z",
    Hall: "M3 22V12l12-7 12 7v10M8 22v-6M15 22v-6M22 22v-6",
    Chamber: "M5 22V8h20v14M10 22v-8h10v8",
    Slap: "M6 14h4M12 10v8M18 12v4M22 13v2",
    "Tape Echo": "M9 14a4 4 0 1 0 0.01 0M21 14a4 4 0 1 0 0.01 0M9 18h12",
  };
  const DYN_ICONS: Record<string, string> = {
    "Smooth + Fast": "M4 20c6 0 6-12 11-12M15 8l3 6 3-10 3 16",
    Clean: "M4 20L15 9h11",
    Warm: "M15 4c-4 6-7 9-7 13a7 7 0 0 0 14 0c0-4-3-7-7-13z",
  };
  const TONE_ICONS: Record<string, string> = {
    Clean: "M3 16c6 0 6-6 12-6s6 6 12 6",
    Vintage: "M15 5a9 9 0 1 0 0.01 0M15 9v5l3 2",
    Modern: "M3 18l6-8 6 6 6-10 6 4",
  };
  const iconFor = (m: Module, name: string) =>
    (m.key === "fx" ? FX_ICONS : m.key === "space" ? SPACE_ICONS : m.key === "dyn" ? DYN_ICONS : TONE_ICONS)[name] ?? "";

  const TONE_SLIDERS = ["low", "mid", "high"];
</script>

{#snippet knob(k: string, size: number, label?: string)}
  {@const p = P(k)}
  <BigKnob param={p} value={val(k)} {size} {label} oninput={(v) => set(k, v, false)} oncommit={(v) => set(k, v, true)} />
{/snippet}

{#snippet slider(k: string, air = false)}
  {@const p = P(k)}
  <div class="vs" class:air title="{p.name}: {formatParam(p, val(k))}. Double-click the slider for the default, or the number to type one.">
    <input
      type="range"
      min={p.min}
      max={p.max}
      step={p.step}
      value={val(k)}
      aria-label={p.name}
      oninput={(e) => set(k, Number(e.currentTarget.value), false)}
      onchange={(e) => set(k, Number(e.currentTarget.value), true)}
      ondblclick={() => set(k, p.default, true)}
    />
    <span class="vname">{p.name}</span>
    <span class="vval"><ValueField param={p} value={val(k)} oninput={(v) => set(k, v, false)} oncommit={(v) => set(k, v, true)} /></span>
  </div>
{/snippet}

<div class="vox">
  <div class="brand">
    <svg viewBox="0 0 40 24" class="mark" aria-hidden="true">
      <path d="M2 12h4l3-8 4 16 4-12 3 6 2-2h4" fill="none" stroke="var(--accent)" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" />
    </svg>
    <span class="word">VOX STRIP</span>
    <span class="tag">vocal chain</span>
    <button type="button" class="lowcut" class:lit={on("low_cut")} aria-pressed={on("low_cut")} title="Low Cut: removes rumble below 80 Hz before everything else" onclick={() => set("low_cut", on("low_cut") ? 0 : 1, true)}>
      <svg viewBox="0 0 24 12" aria-hidden="true"><path d="M2 11c4 0 5-8 9-8h11" fill="none" stroke="currentColor" stroke-width="1.6" /></svg>
      Low Cut
    </button>
  </div>

  <div class="modules">
    {#each MODULES as m (m.key)}
      {@const style = choicesOf(m.style)[Math.round(val(m.style))] ?? ""}
      {@const live = on(m.power)}
      <section class="mod" class:off={!live} style:--mod={m.color}>
        <header>
          <button type="button" class="led" class:lit={live} aria-pressed={live} title="{m.name}: {live ? 'on (click to bypass)' : 'bypassed (click to turn on)'}" onclick={() => set(m.power, live ? 0 : 1, true)}>
            <span></span>
          </button>
          <span class="mnum">{MODULES.indexOf(m) + 1}</span>
          <span class="mname">{m.name}</span>
        </header>
        <div class="stepper">
          <button type="button" aria-label="Previous {m.name} style" onclick={() => step(m, -1)}>‹</button>
          <div class="stylebox">
            <svg viewBox="0 0 30 28" aria-hidden="true"><path d={iconFor(m, style)} fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round" stroke-linecap="round" /></svg>
            <span>{style}</span>
            <span class="dots">
              {#each choicesOf(m.style) as c, i (c)}<i class:here={i === Math.round(val(m.style))}></i>{/each}
            </span>
          </div>
          <button type="button" aria-label="Next {m.name} style" onclick={() => step(m, 1)}>›</button>
        </div>

        <div class="body">
          {#if m.key === "dyn"}
            <div class="dyn">
              <div class="bars">
                <div class="bar" title="Gain reduction: {gr.toFixed(1)} dB"><i style:height="{fill(live ? gr : 0) * 100}%"></i><span>GR</span></div>
                <div class="bar ds" title="De-essing: {ds.toFixed(1)} dB"><i style:height="{fill(live ? ds : 0) * 100}%"></i><span>DS</span></div>
              </div>
              <div class="knobs">
                {@render knob("comp", 74)}
                {@render knob("deess", 74)}
              </div>
            </div>
          {:else if m.key === "tone"}
            <svg class="curve" viewBox="0 0 {CW} {CH}" preserveAspectRatio="none" aria-hidden="true">
              <line x1="0" x2={CW} y1={CH / 2} y2={CH / 2} />
              {#each [100, 1000, 10000] as f (f)}<line x1={(Math.log10(f / 20) / 3) * CW} x2={(Math.log10(f / 20) / 3) * CW} y1="0" y2={CH} />{/each}
              <path d={curvePath} />
            </svg>
            <div class="sliders">
              {#each TONE_SLIDERS as k (k)}{@render slider(k)}{/each}
              {@render slider("air", true)}
            </div>
          {:else if m.key === "fx"}
            <div class="one">{@render knob("fx", 112)}</div>
          {:else}
            <div class="knobs">
              {@render knob("reverb", 74)}
              {@render knob("delay", 74)}
            </div>
            <div class="sync">
              <select aria-label="Delay time" value={Math.round(val("note"))} onchange={(e) => set("note", Number(e.currentTarget.value), true)} disabled={style === "Slap"} title={style === "Slap" ? "Slap uses its own short echo" : "Delay time, as a note value at the tempo"}>
                {#each choicesOf("note") as c, i (c)}<option value={i}>{c}</option>{/each}
              </select>
              <span class="bpm">{Math.round(val("tempo"))} BPM</span>
              {#if tempo && Math.abs(tempo - val("tempo")) > 0.05}
                <button type="button" title="Use the session tempo ({tempo} BPM)" onclick={() => set("tempo", tempo, true)}>Sync</button>
              {/if}
            </div>
          {/if}
        </div>
        <footer>
          {@render knob(m.trim, 40, "Trim")}
        </footer>
      </section>
    {/each}
  </div>

  <div class="io">
    <div class="k">{@render knob("input", 44)}</div>
    <div class="ladder" title="Output level: {out.toFixed(1)} dBFS">
      {#each Array.from({ length: 30 }, (_, i) => i) as i (i)}<i class:on={outFill(out) * 30 > i} class:hot={i >= 27}></i>{/each}
    </div>
    <div class="k">{@render knob("output", 44)}</div>
  </div>
</div>

<style>
  .vox {
    --plate: #1d2024;
    --plate-2: #262a30;
    --ink: #d9dde3;
    --ink-dim: #8d949e;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px;
    background: linear-gradient(180deg, #2a2e34, var(--plate) 60%);
    color: var(--ink);
    font-size: 11px;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 2px 4px 6px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.06);
  }
  .mark {
    width: 36px;
    height: 22px;
  }
  .word {
    font-weight: 700;
    letter-spacing: 0.22em;
    font-size: 14px;
  }
  .tag {
    flex: 1;
    color: var(--ink-dim);
    letter-spacing: 0.08em;
    text-transform: uppercase;
    font-size: 9px;
  }
  .lowcut {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 3px 8px;
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 4px;
    background: var(--plate-2);
    color: var(--ink-dim);
  }
  .lowcut svg {
    width: 20px;
    height: 10px;
  }
  .lowcut.lit {
    color: #fff;
    border-color: var(--accent);
  }
  .modules {
    display: grid;
    grid-template-columns: repeat(4, minmax(150px, 1fr));
    gap: 8px;
  }
  .mod {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
    padding: 8px;
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 6px;
    background: linear-gradient(180deg, var(--plate-2), var(--plate));
    transition: opacity 0.15s;
  }
  .mnum {
    display: grid;
    width: 15px;
    height: 15px;
    place-items: center;
    border: 1px solid rgba(255, 255, 255, 0.25);
    border-radius: 3px;
    color: var(--ink-dim);
    font-size: 9px;
    font-weight: 700;
  }
  .mod.off .stepper,
  .mod.off .body {
    opacity: 0.45;
  }
  .mod header {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .mname {
    font-weight: 700;
    letter-spacing: 0.14em;
    text-transform: uppercase;
    font-size: 10px;
    color: var(--ink);
  }
  .led {
    display: grid;
    width: 16px;
    height: 16px;
    padding: 0;
    place-items: center;
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 50%;
    background: #111;
  }
  .led span {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: #333;
  }
  .led.lit span {
    background: var(--mod);
    box-shadow: 0 0 6px var(--mod);
  }
  .stepper {
    display: flex;
    align-items: stretch;
    gap: 3px;
  }
  .stepper > button {
    width: 18px;
    padding: 0;
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 4px;
    background: #15171a;
    color: var(--ink-dim);
    font-size: 14px;
  }
  .stepper > button:hover {
    color: var(--mod);
  }
  .stylebox {
    display: flex;
    flex: 1;
    flex-direction: column;
    align-items: center;
    gap: 2px;
    padding: 5px 2px 4px;
    border-radius: 4px;
    background: #121417;
    box-shadow: inset 0 1px 4px rgba(0, 0, 0, 0.6);
    color: var(--mod);
    min-width: 0;
  }
  .stylebox svg {
    width: 30px;
    height: 26px;
    filter: drop-shadow(0 0 4px color-mix(in srgb, var(--mod) 60%, transparent));
  }
  .stylebox > span:first-of-type {
    color: var(--ink);
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 100%;
  }
  .dots {
    display: flex;
    gap: 3px;
  }
  .dots i {
    width: 4px;
    height: 4px;
    border-radius: 50%;
    background: #3a3f46;
  }
  .dots i.here {
    background: var(--mod);
  }
  .body {
    display: flex;
    flex: 1;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    min-height: 210px;
  }
  .dyn {
    display: flex;
    gap: 8px;
    align-items: stretch;
  }
  .bars {
    display: flex;
    gap: 5px;
  }
  .bar {
    position: relative;
    display: flex;
    flex-direction: column;
    justify-content: flex-start;
    width: 12px;
    height: 190px;
    border-radius: 3px;
    background: #0e1013;
    box-shadow: inset 0 1px 3px rgba(0, 0, 0, 0.7);
    overflow: hidden;
  }
  .bar i {
    display: block;
    width: 100%;
    background: linear-gradient(180deg, var(--mod), color-mix(in srgb, var(--mod) 50%, #000));
  }
  .bar.ds i {
    background: linear-gradient(180deg, #f5e18a, #8a7a2a);
  }
  .bar span {
    position: absolute;
    bottom: 2px;
    left: 0;
    right: 0;
    text-align: center;
    font-size: 8px;
    color: var(--ink-dim);
  }
  .knobs {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
  }
  .body > .knobs {
    flex-direction: row;
    flex-wrap: wrap;
    justify-content: center;
  }
  .one {
    display: grid;
    flex: 1;
    place-items: center;
  }
  .curve {
    width: 100%;
    height: 64px;
    border-radius: 4px;
    background: #0e1013;
    box-shadow: inset 0 1px 3px rgba(0, 0, 0, 0.7);
  }
  .curve line {
    stroke: rgba(255, 255, 255, 0.07);
    stroke-width: 1;
  }
  .curve path {
    fill: none;
    stroke: var(--mod);
    stroke-width: 2;
    vector-effect: non-scaling-stroke;
    filter: drop-shadow(0 0 3px var(--mod));
  }
  .sliders {
    display: flex;
    justify-content: space-around;
    width: 100%;
  }
  .vs {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
  }
  .vs input {
    writing-mode: vertical-lr;
    direction: rtl;
    width: 22px;
    height: 104px;
    accent-color: var(--mod);
  }
  .vs.air .vname {
    color: var(--accent);
  }
  .vname {
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    font-size: 9px;
  }
  .vval {
    width: 52px;
    color: var(--ink-dim);
    font-variant-numeric: tabular-nums;
    font-size: 9px;
  }
  .sync {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .sync select,
  .sync button {
    padding: 1px 4px;
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 3px;
    background: #15171a;
    color: var(--ink);
    font-size: 10px;
  }
  .bpm {
    color: var(--ink-dim);
    font-variant-numeric: tabular-nums;
  }
  .mod footer {
    display: flex;
    justify-content: center;
    min-width: 0;
  }
  .mod footer :global(*),
  .io .k :global(*) {
    max-width: 100%;
    box-sizing: border-box;
  }
  .io .k {
    display: flex;
    justify-content: center;
    width: 84px;
    flex: none;
  }
  .io {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 8px;
    border-radius: 6px;
    background: #15171a;
  }
  .ladder {
    display: flex;
    flex: 1;
    gap: 2px;
    height: 10px;
  }
  .ladder i {
    flex: 1;
    border-radius: 1px;
    background: #2a2e34;
  }
  .ladder i.on {
    background: var(--accent);
  }
  .ladder i.on.hot {
    background: var(--danger);
  }
</style>
