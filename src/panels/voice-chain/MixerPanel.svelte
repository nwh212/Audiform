<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The Audiform contributors.
  This file is part of Audiform, based on Altered Audio Editor.

  The Channel Mixer's window: a small tab for each output side (L, R);
  on each, one row per input channel with how much of it goes in and
  whether that part is flipped (Invert).
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
  let side = $state<"l" | "r">("l");
</script>

<div class="mixer">
  <fieldset>
    <div class="tabs" role="tablist" aria-label="Output channel">
      {#each [["l", "L"], ["r", "R"]] as const as [k, name] (k)}
        <button type="button" role="tab" class:on={side === k} aria-selected={side === k} title="{k === 'l' ? 'Left' : 'Right'} output: what goes into it" onclick={() => (side = k)}>{name}</button>
      {/each}
    </div>
    <div class="head">
      <span>Input Channel</span>
      <span>Invert</span>
    </div>
    {#each [["l", "L"], ["r", "R"]] as const as [i, name] (i)}
      <div class="line">
        <SliderRow param={P(`${side}${i}`)} value={val(`${side}${i}`)} marks={[0, 50, 100]} label={name} labelWidth={14} set={set(`${side}${i}`)} />
        <input
          type="checkbox"
          aria-label="Invert {name} into {side === 'l' ? 'left' : 'right'}"
          title="Flip the polarity of this part"
          checked={val(`${side}${i}_inv`) >= 0.5}
          onchange={(e) => set(`${side}${i}_inv`)(e.currentTarget.checked ? 1 : 0, true)}
        />
      </div>
    {/each}
  </fieldset>
</div>

<style>
  .mixer {
    padding: 12px 14px 14px;
  }
  fieldset {
    display: grid;
    gap: 6px;
    margin: 0;
    padding: 8px 14px 12px;
    border: 1px solid var(--border);
    border-radius: 6px;
  }
  .tabs {
    display: flex;
    gap: 14px;
    padding-left: 2px;
  }
  .tabs button {
    padding: 0 2px 2px;
    border: 0;
    border-bottom: 2px solid transparent;
    background: transparent;
    color: var(--text-dim);
    font: inherit;
    font-weight: 600;
    cursor: pointer;
  }
  .tabs button.on {
    border-bottom-color: var(--accent);
    color: var(--text);
  }
  .head {
    display: grid;
    grid-template-columns: 1fr 44px;
    padding: 4px 0 0 24px;
    color: var(--text-dim);
    font-size: 12px;
  }
  .head span:last-child {
    text-align: center;
  }
  .line {
    display: grid;
    grid-template-columns: 1fr 44px;
    align-items: center;
  }
  .line input {
    justify-self: center;
    width: 15px;
    height: 15px;
    accent-color: var(--accent);
  }
</style>
