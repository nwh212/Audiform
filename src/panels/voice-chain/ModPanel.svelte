<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The Audiform contributors.
  This file is part of Audiform, based on Altered Audio Editor.

  The Modulation window, as a piece of hardware. Across the top, the
  kind (Chorus, Flanger, Phaser, Double Track: each has its own
  faceplate color). In the middle, the two big dials (Rate and Depth)
  either side of a lit display that draws the movement: each chorus
  voice's swing, the flanger's and phaser's sweep, or the double track's
  wander. Under them, small dials for the kind's own settings, the
  feedback, width and room spread, and the levels. Picking a kind starts
  it from settings that suit it, in one undoable step.
-->
<script lang="ts">
  import type { EffectInfo, EffectParamInfo } from "../../app/core";
  import type { EffectSlotDoc } from "../../stores/session";
  import BigKnob from "./BigKnob.svelte";
  import { formatParam, paramValue } from "./chain";
  import "./faceplate.css";
  import { MOD_KINDS, MOD_OWN, MOD_START, MOD_TIPS } from "./modulation-kinds";

  interface Props {
    slot: EffectSlotDoc;
    info: EffectInfo;
    onParams: (changes: [EffectParamInfo, number][], label: string, done: boolean) => void;
  }

  const { slot, info, onParams }: Props = $props();
  const P = (k: string) => info.params.find((p) => p.key === k)!;
  const val = (k: string) => paramValue(slot, P(k));
  const set = (k: string, v: number, done: boolean) => {
    const p = P(k);
    onParams([[p, Math.max(p.min, Math.min(p.max, v))]], `${info.name} ${p.name}`, done);
  };
  const kind = $derived(Math.round(val("kind")));
  function pick(i: number) {
    if (i === kind) return;
    const start = MOD_START[i] ?? {};
    onParams([[P("kind"), i], ...Object.entries(start).map(([k, v]) => [P(k), v] as [EffectParamInfo, number])], `${info.name}: ${MOD_KINDS[i]}`, true);
  }

  /** Each kind's faceplate: its face, ink and glow. */
  const LOOKS = [
    { face: "#1b2a2c", face2: "#0f191a", ink: "#d3eeee", glow: "#4fe0d2" },
    { face: "#2a2033", face2: "#17111d", ink: "#e8dcf3", glow: "#c38bff" },
    { face: "#33261a", face2: "#1c140d", ink: "#f3e3cf", glow: "#ff9d4a" },
    { face: "#202b1d", face2: "#121910", ink: "#dcefd6", glow: "#9be06a" },
  ];
  const look = $derived(LOOKS[kind] ?? LOOKS[0]!);
  /** Short names for the kind's own dials. */
  const SHORT: Record<string, string> = {
    voices: "Voices",
    chorus_delay: "Delay",
    flange_delay: "Delay",
    stages: "Stages",
    center: "Center",
    dt_delay: "Delay",
    wander: "Wander",
  };

  // The display: two turns of the movement, one line per voice.
  const W = 240;
  const H = 64;
  const depth = $derived(val("depth") / 100);
  const width = $derived(val("width") / 100);
  const lines = $derived.by(() => {
    const steps = 96;
    const n = kind === 0 ? Math.round(val("voices")) : kind === 3 ? 1 : 2;
    const out: string[] = [];
    for (let v = 0; v < n; v++) {
      // Chorus voices spread round the turn; the right side runs behind by the width.
      const off = kind === 0 ? v / n : v * width * 0.25;
      let d = "";
      for (let i = 0; i <= steps; i++) {
        const t = (i / steps) * 2;
        const y =
          kind === 3
            ? Math.sin(t * 5.1) * 0.5 + Math.sin(t * 2.3 + 1) * 0.35 + Math.sin(t * 9.7 + 2) * 0.15 * val("wander") / 100
            : Math.sin((t + off) * Math.PI * 2);
        const py = H / 2 - y * depth * (H / 2 - 6);
        d += `${i ? "L" : "M"}${((i / steps) * W).toFixed(1)} ${py.toFixed(1)}`;
      }
      out.push(d);
    }
    return out;
  });
  const detail = $derived(
    kind === 0
      ? `${Math.round(val("voices"))} voices · ${formatParam(P("chorus_delay"), val("chorus_delay"))}`
      : kind === 1
        ? `${formatParam(P("flange_delay"), val("flange_delay"))} · feedback ${Math.round(val("feedback"))}%`
        : kind === 2
          ? `${formatParam(P("stages"), val("stages"))} stages · ${formatParam(P("center"), val("center"))}`
          : `${formatParam(P("dt_delay"), val("dt_delay"))} late`,
  );
</script>

<div class="fp-shell">
  <div class="fp-unit" style:--face={look.face} style:--face2={look.face2} style:--panel-ink={look.ink} style:--glow={look.glow}>
    <div class="fp-top">
      <div class="fp-kinds" role="tablist" aria-label="Kind of modulation">
        {#each MOD_KINDS as k, i (k)}
          <button type="button" role="tab" class:lit={i === kind} aria-selected={i === kind} title={MOD_TIPS[i]} onclick={() => pick(i)}>{k}</button>
        {/each}
      </div>
    </div>

    <div class="fp-mid">
      <BigKnob param={P("rate")} value={val("rate")} size={104} label="Rate" marks={[0.02, 0.1, 1, 10]} oninput={(v) => set("rate", v, false)} oncommit={(v) => set("rate", v, true)} />
      <div class="fp-screen" aria-live="polite">
        <div class="fp-readout">
          <span class="fp-big">{formatParam(P("rate"), val("rate"))}</span>
          <span class="fp-small">{detail}</span>
        </div>
        <svg viewBox="0 0 {W} {H}" preserveAspectRatio="none" class="fp-echoes" role="img" aria-label="The movement over two turns">
          <line class="fp-axis" x1="0" x2={W} y1={H / 2} y2={H / 2} />
          {#each lines as d, i (i)}
            <path class="sweep" {d} style:opacity={1 - i * 0.12} />
          {/each}
        </svg>
      </div>
      <BigKnob param={P("depth")} value={val("depth")} size={104} label="Depth" marks={[0, 50, 100]} oninput={(v) => set("depth", v, false)} oncommit={(v) => set("depth", v, true)} />
    </div>

    <div class="fp-row">
      <div class="fp-group">
        <span class="fp-gname">{MOD_KINDS[kind] ?? "Kind"}</span>
        <div class="fp-knobs">
          {#each MOD_OWN[kind] ?? [] as k (k)}
            <BigKnob param={P(k)} value={val(k)} size={56} label={SHORT[k] ?? P(k).name} oninput={(v) => set(k, v, false)} oncommit={(v) => set(k, v, true)} />
          {/each}
        </div>
      </div>
      <div class="fp-group">
        <span class="fp-gname">Movement</span>
        <div class="fp-knobs">
          <BigKnob param={P("feedback")} value={val("feedback")} size={56} label="Feedback" oninput={(v) => set("feedback", v, false)} oncommit={(v) => set("feedback", v, true)} />
          <BigKnob param={P("width")} value={val("width")} size={56} label="Width" oninput={(v) => set("width", v, false)} oncommit={(v) => set("width", v, true)} />
          <span title="On a surround bed or a 3D Space master: how much of each speaker's neighbours its moving copy takes in, so the movement spreads round the room. Stereo ignores it.">
            <BigKnob param={P("spread")} value={val("spread")} size={56} label="Spread" oninput={(v) => set("spread", v, false)} oncommit={(v) => set("spread", v, true)} />
          </span>
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
  .sweep {
    fill: none;
    stroke: var(--glow);
    stroke-width: 1.6;
    filter: drop-shadow(0 0 2px var(--glow));
    vector-effect: non-scaling-stroke;
  }
</style>
