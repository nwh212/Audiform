<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The Audiform contributors.
  This file is part of Audiform, based on Altered Audio Editor.

  The Modulation window: a tab for each kind (Chorus, Flanger, Phaser,
  Double Track), the kind's own settings, then the movement and the
  output as long sliders. Picking a tab starts that kind from settings
  that suit it, in one undoable step.
-->
<script lang="ts">
  import type { EffectInfo, EffectParamInfo } from "../../app/core";
  import type { EffectSlotDoc } from "../../stores/session";
  import { paramValue } from "./chain";
  import KindTabs from "./KindTabs.svelte";
  import SliderRow from "./SliderRow.svelte";
  import { MOD_KINDS, MOD_OWN, MOD_START, MOD_TIPS } from "./modulation-kinds";

  interface Props {
    slot: EffectSlotDoc;
    info: EffectInfo;
    onParams: (changes: [EffectParamInfo, number][], label: string, done: boolean) => void;
  }

  const { slot, info, onParams }: Props = $props();
  const P = (k: string) => info.params.find((p) => p.key === k)!;
  const val = (k: string) => paramValue(slot, P(k));
  const set = (k: string) => (v: number, done: boolean) => {
    const p = P(k);
    onParams([[p, Math.max(p.min, Math.min(p.max, v))]], `${info.name} ${p.name}`, done);
  };
  const kind = $derived(Math.round(val("kind")));
  function pick(i: number) {
    if (i === kind) return;
    const start = MOD_START[i] ?? {};
    onParams([[P("kind"), i], ...Object.entries(start).map(([k, v]) => [P(k), v] as [EffectParamInfo, number])], `${info.name}: ${MOD_KINDS[i]}`, true);
  }
  const PCT = [0, 20, 40, 60, 80, 100];
  const MARKS: Record<string, number[]> = {
    voices: [2, 3, 4, 5, 6],
    chorus_delay: [5, 10, 20, 30, 40],
    flange_delay: [0.1, 0.5, 1, 2, 5, 10],
    stages: [0, 1, 2, 3, 4, 5],
    center: [100, 200, 500, 1000, 2000, 5000],
    dt_delay: [10, 20, 30, 40, 50, 60],
    wander: PCT,
    rate: [0.02, 0.1, 0.5, 1, 2, 5, 10],
    depth: PCT,
    feedback: [-95, -50, 0, 50, 95],
    width: PCT,
    dry: PCT,
    wet: PCT,
  };
  const sections = $derived([
    { title: MOD_KINDS[kind] ?? "Kind", rows: MOD_OWN[kind] ?? [] },
    { title: "Movement", rows: ["rate", "depth", "feedback", "width"] },
    { title: "Output", rows: ["dry", "wet"] },
  ]);
</script>

<div class="mod">
  <KindTabs kinds={MOD_KINDS} current={kind} tips={MOD_TIPS} label="Kind of modulation" {pick} />
  <div class="body">
    {#each sections as s (s.title)}
      <fieldset>
        <legend>{s.title}</legend>
        {#each s.rows as k (k)}
          <SliderRow param={P(k)} value={val(k)} marks={MARKS[k] ?? []} set={set(k)} />
        {/each}
      </fieldset>
    {/each}
  </div>
</div>

<style>
  .mod {
    display: grid;
  }
  .body {
    display: grid;
    gap: 8px;
    padding: 10px 14px 10px;
  }
  fieldset {
    display: grid;
    gap: 2px;
    margin: 0;
    padding: 4px 12px 6px;
    border: 1px solid var(--border);
    border-radius: 6px;
  }
  legend {
    padding: 0 6px;
    color: var(--text-dim);
    font-size: 11px;
    letter-spacing: 0.04em;
  }
</style>
