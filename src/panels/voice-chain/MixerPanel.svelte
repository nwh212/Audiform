<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The Audiform contributors.
  This file is part of Audiform, based on Altered Audio Editor.

  The Channel Mixer's window: a tab for each output side; on each, how
  much of the left and right inputs goes into it, and whether that part
  is flipped. A drawing under it shows where each input goes.
-->
<script lang="ts">
  import type { EffectInfo, EffectParamInfo } from "../../app/core";
  import type { EffectSlotDoc } from "../../stores/session";
  import { paramValue } from "./chain";
  import KindTabs from "./KindTabs.svelte";
  import SliderRow from "./SliderRow.svelte";

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
  let side = $state(0);
  const out = $derived(side === 0 ? "l" : "r");
  const MARKS = [0, 25, 50, 75, 100];
  /** The four parts, for the drawing: [from, to, amount, flipped]. */
  const parts = $derived(
    (["l", "r"] as const).flatMap((o) =>
      (["l", "r"] as const).map((i) => ({ from: i, to: o, amt: val(`${o}${i}`) / 100, inv: val(`${o}${i}_inv`) >= 0.5 })),
    ),
  );
</script>

<div class="mix">
  <KindTabs kinds={["Left output", "Right output"]} current={side} label="Output side" pick={(i) => (side = i)} />
  <div class="body">
    {#each [["l", "Left input"], ["r", "Right input"]] as const as [i, name] (i)}
      <div class="line">
        <SliderRow param={P(`${out}${i}`)} value={val(`${out}${i}`)} marks={MARKS} label={name} set={set(`${out}${i}`)} />
        <label class="inv" title="Flip the polarity of this part">
          <input type="checkbox" checked={val(`${out}${i}_inv`) >= 0.5} onchange={(e) => set(`${out}${i}_inv`)(e.currentTarget.checked ? 1 : 0, true)} />
          Invert
        </label>
      </div>
    {/each}
    <svg viewBox="0 0 200 64" class="map" role="img" aria-label="Where each input goes">
      <text x="8" y="20">L in</text>
      <text x="8" y="52">R in</text>
      <text x="160" y="20">L out</text>
      <text x="160" y="52">R out</text>
      {#each parts as p (p.from + p.to)}
        {#if p.amt > 0}
          <line
            x1="44"
            y1={p.from === "l" ? 16 : 48}
            x2="152"
            y2={p.to === "l" ? 16 : 48}
            stroke-width={0.6 + p.amt * 3}
            stroke-opacity={0.25 + p.amt * 0.75}
            stroke-dasharray={p.inv ? "4 3" : "none"}
          />
        {/if}
      {/each}
    </svg>
  </div>
</div>

<style>
  .mix {
    display: grid;
    gap: 0;
  }
  .body {
    display: grid;
    gap: 12px;
    padding: 14px 14px 12px;
  }
  .line {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 12px;
    align-items: center;
  }
  .inv {
    display: flex;
    gap: 5px;
    align-items: center;
    color: var(--text-dim);
    cursor: pointer;
  }
  input[type="checkbox"] {
    accent-color: var(--accent);
  }
  .map {
    width: 100%;
    max-width: 320px;
    height: 64px;
    justify-self: center;
  }
  .map text {
    fill: var(--text-dim);
    font-size: 10px;
  }
  .map line {
    stroke: var(--accent);
  }
</style>
