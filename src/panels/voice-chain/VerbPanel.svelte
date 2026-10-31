<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The Audiform contributors.
  This file is part of Audiform, based on Altered Audio Editor.

  The Reverb's window: a tab for each kind of space across the top
  (Room, Hall, Plate, Spring, Tape, and Impulse Response, which turns
  the slot into the recorded-space reverb), then the space, its
  character and the output as long sliders.
-->
<script lang="ts">
  import type { EffectInfo, EffectParamInfo } from "../../app/core";
  import type { EffectSlotDoc } from "../../stores/session";
  import { paramValue } from "./chain";
  import KindTabs from "./KindTabs.svelte";
  import SliderRow from "./SliderRow.svelte";
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
  const set = (k: string) => (v: number, done: boolean) => {
    const p = P(k);
    onParams([[p, Math.max(p.min, Math.min(p.max, v))]], `${info.name} ${p.name}`, done);
  };
  const kind = $derived(Math.round(val("kind")));
  const PCT = [0, 20, 40, 60, 80, 100];
  const SECTIONS: { title: string; rows: [string, number[]][] }[] = [
    {
      title: "Space",
      rows: [
        ["size", PCT],
        ["decay", [100, 200, 500, 1000, 2000, 5000, 10000, 20000]],
        ["predelay", [0, 50, 100, 150, 200, 250]],
        ["early", PCT],
        ["width", PCT],
      ],
    },
    {
      title: "Character",
      rows: [
        ["damping", PCT],
        ["diffusion", PCT],
        ["modulation", PCT],
        ["high_cut", [1000, 2000, 4000, 8000, 16000]],
        ["low_cut", [20, 50, 100, 200, 500, 1000]],
      ],
    },
    { title: "Output", rows: [["dry", PCT], ["wet", PCT]] },
  ];
  /** Each kind's own look: its color, glow and the depth of its panel. */
  const LOOKS: { hue: string; glow: string; deep: string }[] = [
    { hue: "#e0a46a", glow: "rgba(224, 164, 106, 0.35)", deep: "#1f1610" },
    { hue: "#7fa8ff", glow: "rgba(127, 168, 255, 0.35)", deep: "#0f1424" },
    { hue: "#d8dde6", glow: "rgba(216, 221, 230, 0.3)", deep: "#15171b" },
    { hue: "#8fd18a", glow: "rgba(143, 209, 138, 0.3)", deep: "#0f1a10" },
    { hue: "#f0c050", glow: "rgba(240, 192, 80, 0.32)", deep: "#1e170a" },
  ];
  // Plates and springs have no walls to bounce off.
  const noEarly = $derived(kind === 2 || kind === 3);
</script>

<div class="verb" style:--hue={LOOKS[kind]?.hue} style:--glow={LOOKS[kind]?.glow} style:--deep={LOOKS[kind]?.deep}>
  <KindTabs kinds={REVERB_KINDS} current={kind} tips={REVERB_TIPS} label="Kind of reverb" pick={(i) => (i === REVERB_KINDS.length - 1 ? onImpulse() : set("kind")(i, true))} />
  <div class="body">
    <div class="plate"><span class="title">{REVERB_KINDS[kind] ?? "Reverb"}</span><span class="sub">{REVERB_TIPS[kind] ?? ""}</span></div>
    {#each SECTIONS as s (s.title)}
      <fieldset>
        <legend>{s.title}</legend>
        {#each s.rows as [k, marks] (k)}
          <SliderRow param={P(k)} value={val(k)} {marks} disabled={k === "early" && noEarly} set={set(k)} />
        {/each}
      </fieldset>
    {/each}
  </div>
</div>

<style>
  .verb {
    display: grid;
  }
  .body {
    display: grid;
    gap: 8px;
    margin-bottom: 6px;
    padding: 10px 14px 12px;
    border-radius: 6px;
    background: radial-gradient(120% 90% at 0% 0%, var(--glow, transparent), transparent 60%), var(--deep, var(--surface-1));
    box-shadow: inset 0 0 0 1px var(--glow, var(--border));
  }
  .plate {
    display: flex;
    flex-wrap: wrap;
    gap: 2px 10px;
    align-items: baseline;
    padding-bottom: 6px;
    border-bottom: 1px solid var(--glow, var(--border));
  }
  .title {
    color: var(--hue, var(--accent));
    font-size: 14px;
    font-weight: 700;
    letter-spacing: 0.1em;
    text-transform: uppercase;
  }
  .sub {
    min-width: 0;
    color: var(--text-dim);
    font-size: 11px;
  }
  fieldset {
    display: grid;
    gap: 2px;
    margin: 0;
    padding: 4px 12px 6px;
    border: 1px solid var(--glow, var(--border));
    border-radius: 6px;
  }
  legend {
    padding: 0 6px;
    color: var(--hue, var(--text-dim));
    font-size: 11px;
    letter-spacing: 0.04em;
  }
</style>
