<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The Audiform contributors.
  This file is part of Audiform, based on Altered Audio Editor.

  The Reverb's window, as a piece of hardware. Across the top, the kind
  of space (Room, Hall, Plate, Spring, Tape: each has its own faceplate
  color), and Impulse Response, which turns the slot into the
  recorded-space reverb. In the middle, the two big dials (Size and
  Decay) either side of a lit display that draws the reverb: the
  pre-delay gap, the early reflections, and the tail dying away. Under
  them, small dials for the space, its character, the filters and the
  levels.
-->
<script lang="ts">
  import type { EffectInfo, EffectParamInfo } from "../../app/core";
  import type { EffectSlotDoc } from "../../stores/session";
  import BigKnob from "./BigKnob.svelte";
  import { formatParam, paramValue } from "./chain";
  import "./faceplate.css";
  import { REVERB_KINDS, REVERB_TIPS } from "./reverb-kinds";

  interface Props {
    slot: EffectSlotDoc;
    info: EffectInfo;
    onParams: (changes: [EffectParamInfo, number][], label: string, done: boolean) => void;
    /** Impulse Response: the slot becomes the recorded-space reverb. */
    onImpulse: () => void;
  }

  const { slot, info, onParams, onImpulse }: Props = $props();
  const P = (k: string) => info.params.find((p) => p.key === k)!;
  const val = (k: string) => paramValue(slot, P(k));
  const set = (k: string, v: number, done: boolean) => {
    const p = P(k);
    onParams([[p, Math.max(p.min, Math.min(p.max, v))]], `${info.name} ${p.name}`, done);
  };
  const kind = $derived(Math.round(val("kind")));
  function pick(i: number) {
    if (i === REVERB_KINDS.length - 1) onImpulse();
    else if (i !== kind) set("kind", i, true);
  }

  /** Each kind's faceplate: its face, ink and glow. */
  const LOOKS = [
    { face: "#32261c", face2: "#1c140e", ink: "#f2e2d0", glow: "#e0a46a" },
    { face: "#1c2236", face2: "#10141f", ink: "#dbe4fb", glow: "#7fa8ff" },
    { face: "#2a2d33", face2: "#16181c", ink: "#eef0f4", glow: "#d8dde6" },
    { face: "#1d2b1c", face2: "#101910", ink: "#dcefd8", glow: "#8fd18a" },
    { face: "#332a14", face2: "#1d170a", ink: "#f4e8c8", glow: "#f0c050" },
  ];
  const look = $derived(LOOKS[kind] ?? LOOKS[0]!);
  // Plates and springs have no walls to bounce off.
  const noEarly = $derived(kind === 2 || kind === 3);

  // The display: the reverb over time, as a level in dB from the top down to -60.
  const W = 240;
  const H = 64;
  const shape = $derived.by(() => {
    const pre = val("predelay");
    const decay = val("decay");
    const span = Math.max(300, pre + decay * 1.15);
    const x = (ms: number) => (ms / span) * W;
    const early = noEarly ? 0 : val("early") / 100;
    const size = val("size") / 100;
    // Early reflections: a burst of taps over the first stretch, wider in a bigger space.
    const taps: { x: number; h: number }[] = [];
    if (early > 0) {
      const len = 15 + size * 70;
      for (let i = 0; i < 9; i++) {
        const at = pre + len * ((i * 0.618) % 1);
        taps.push({ x: x(at), h: early * (0.55 + 0.45 * ((i * 0.37) % 1)) });
      }
    }
    // The tail: bars under a straight fall in dB, roughened by the diffusion.
    const rough = 1 - val("diffusion") / 100;
    const bars: { x: number; h: number }[] = [];
    const n = 70;
    for (let i = 0; i < n; i++) {
      const t = (i / n) * decay;
      const level = Math.max(0, 1 - t / decay);
      const jitter = 1 - rough * 0.5 * ((Math.sin(i * 12.9898) * 43758.5453) % 1 + 1) / 2;
      bars.push({ x: x(pre + t), h: level * jitter * 0.85 });
    }
    return { taps, bars, pre: x(pre) };
  });
  const sec = (ms: number) => (ms >= 1000 ? `${(ms / 1000).toFixed(2)} s` : `${Math.round(ms)} ms`);
</script>

<div class="fp-shell">
  <div class="fp-unit" style:--face={look.face} style:--face2={look.face2} style:--panel-ink={look.ink} style:--glow={look.glow}>
    <div class="fp-top">
      <div class="fp-kinds" role="tablist" aria-label="Kind of reverb">
        {#each REVERB_KINDS as k, i (k)}
          <button type="button" role="tab" class:lit={i === kind} aria-selected={i === kind} title={REVERB_TIPS[i]} onclick={() => pick(i)}>{k}</button>
        {/each}
      </div>
    </div>

    <div class="fp-mid">
      <BigKnob param={P("size")} value={val("size")} size={104} label="Size" marks={[0, 50, 100]} oninput={(v) => set("size", v, false)} oncommit={(v) => set("size", v, true)} />
      <div class="fp-screen" aria-live="polite">
        <div class="fp-readout">
          <span class="fp-big">{sec(val("decay"))}</span>
          <span class="fp-small">{(REVERB_KINDS[kind] ?? "").toLowerCase()} · pre-delay {formatParam(P("predelay"), val("predelay"))}</span>
        </div>
        <svg viewBox="0 0 {W} {H}" preserveAspectRatio="none" class="fp-echoes" role="img" aria-label="The reverb dying away">
          <line class="fp-axis" x1="0" x2={W} y1={H - 1} y2={H - 1} />
          <rect class="fp-dry" x="0" y="4" width="3" height={H - 5} />
          {#each shape.bars as b, i (i)}
            <rect class="tail" x={b.x} y={H - 1 - b.h * (H - 6)} width="2" height={b.h * (H - 6)} />
          {/each}
          {#each shape.taps as t, i (i)}
            <rect class="fp-rep" x={t.x - 1} y={H - 1 - t.h * (H - 6)} width="2.5" height={t.h * (H - 6)} />
          {/each}
        </svg>
      </div>
      <BigKnob param={P("decay")} value={val("decay")} size={104} label="Decay" marks={[100, 1000, 20000]} oninput={(v) => set("decay", v, false)} oncommit={(v) => set("decay", v, true)} />
    </div>

    <div class="fp-row fp-pairs">
      <div class="fp-group">
        <span class="fp-gname">Space</span>
        <div class="fp-knobs">
          <BigKnob param={P("predelay")} value={val("predelay")} size={56} label="Pre-delay" oninput={(v) => set("predelay", v, false)} oncommit={(v) => set("predelay", v, true)} />
          <span class:off={noEarly} title={noEarly ? "Plates and springs have no walls to bounce off" : ""}>
            <BigKnob param={P("early")} value={val("early")} size={56} label="Early" oninput={(v) => set("early", v, false)} oncommit={(v) => set("early", v, true)} />
          </span>
          <BigKnob param={P("width")} value={val("width")} size={56} label="Width" oninput={(v) => set("width", v, false)} oncommit={(v) => set("width", v, true)} />
        </div>
      </div>
      <div class="fp-group">
        <span class="fp-gname">Character</span>
        <div class="fp-knobs">
          <BigKnob param={P("damping")} value={val("damping")} size={56} label="Damping" oninput={(v) => set("damping", v, false)} oncommit={(v) => set("damping", v, true)} />
          <BigKnob param={P("diffusion")} value={val("diffusion")} size={56} label="Diffusion" oninput={(v) => set("diffusion", v, false)} oncommit={(v) => set("diffusion", v, true)} />
          <BigKnob param={P("modulation")} value={val("modulation")} size={56} label="Motion" oninput={(v) => set("modulation", v, false)} oncommit={(v) => set("modulation", v, true)} />
        </div>
      </div>
      <div class="fp-group">
        <span class="fp-gname">Filters</span>
        <div class="fp-knobs">
          <BigKnob param={P("low_cut")} value={val("low_cut")} size={56} label="Lo cut" oninput={(v) => set("low_cut", v, false)} oncommit={(v) => set("low_cut", v, true)} />
          <BigKnob param={P("high_cut")} value={val("high_cut")} size={56} label="Hi cut" oninput={(v) => set("high_cut", v, false)} oncommit={(v) => set("high_cut", v, true)} />
        </div>
      </div>
      <div class="fp-group">
        <span class="fp-gname">Output</span>
        <div class="fp-knobs">
          <BigKnob param={P("dry")} value={val("dry")} size={56} label="Dry" oninput={(v) => set("dry", v, false)} oncommit={(v) => set("dry", v, true)} />
          <BigKnob param={P("wet")} value={val("wet")} size={56} label="Wet" oninput={(v) => set("wet", v, false)} oncommit={(v) => set("wet", v, true)} />
        </div>
      </div>
    </div>
  </div>
</div>

<style>
  .tail {
    fill: color-mix(in srgb, var(--glow) 55%, transparent);
  }
  .off {
    opacity: 0.4;
    pointer-events: none;
  }
</style>
