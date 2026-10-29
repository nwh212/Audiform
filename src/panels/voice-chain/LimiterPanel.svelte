<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The Audiform contributors.
  This file is part of Audiform, based on Altered Audio Editor.

  The Limiter's window: Peak or True peak at the top, then long sliders
  for the maximum level, input boost, look-ahead and release, and Link
  channels.
-->
<script lang="ts">
  import type { EffectInfo, EffectParamInfo } from "../../app/core";
  import type { EffectSlotDoc } from "../../stores/session";
  import { paramValue } from "./chain";
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
  const ROWS: [string, number[]][] = [
    ["ceiling", [-60, -48, -36, -24, -12, -6, 0]],
    ["input_boost", [-24, -12, 0, 12, 24]],
    ["lookahead", [0, 5, 10, 15, 20]],
    ["release", [10, 30, 100, 300, 1000]],
  ];
</script>

<div class="lim">
  <div class="detect" role="radiogroup" aria-label="Detect">
    {#each ["Peak", "True peak"] as d, i (d)}
      <label title={i === 0 ? "Holds every sample under the maximum level" : "Also holds the level a player draws between samples, so the file never goes over once it is played or converted"}>
        <input type="radio" name="detect-{slot.id}" checked={Math.round(val("detect")) === i} onchange={() => set("detect")(i, true)} />
        {d}
      </label>
    {/each}
  </div>
  {#each ROWS as [k, marks] (k)}
    <SliderRow param={P(k)} value={val(k)} {marks} set={set(k)} />
  {/each}
  <label class="link" title="Turn both sides down together, so the stereo picture never shifts">
    <input type="checkbox" checked={val("linked") >= 0.5} onchange={(e) => set("linked")(e.currentTarget.checked ? 1 : 0, true)} />
    Link channels
  </label>
</div>

<style>
  .lim {
    display: grid;
    gap: 14px;
    padding: 12px 14px 14px;
  }
  .detect {
    display: flex;
    justify-content: center;
    gap: 22px;
  }
  .detect label,
  .link {
    display: flex;
    gap: 6px;
    align-items: center;
    color: var(--text);
    cursor: pointer;
  }
  .link {
    justify-self: center;
  }
  input[type="radio"],
  input[type="checkbox"] {
    accent-color: var(--accent);
  }
</style>
