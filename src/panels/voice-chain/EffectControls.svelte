<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  An effect's controls in its settings window: a dial and a value field
  for each setting, one column per band or group, so a whole EQ fits on
  one screen. A group's On switch sits in its heading. Double-click a
  dial for the default; drag or click the value under it to change it.
-->
<script lang="ts">
  import type { EffectInfo, EffectParamInfo } from "../../app/core";
  import Knob from "../../app/Knob.svelte";
  import { formatParam, fromSlider, toSlider } from "./chain";
  import { controlColumns } from "./effect-controls";
  import ValueField from "./ValueField.svelte";

  interface Props {
    info: EffectInfo;
    value: (p: EffectParamInfo) => number;
    /** Whether a group shows (unused Phase Curve points stay hidden). */
    shown?: (group: string) => boolean;
    oninput: (p: EffectParamInfo, v: number) => void;
    oncommit: (p: EffectParamInfo, v: number) => void;
  }

  const { info, value, shown = () => true, oninput, oncommit }: Props = $props();

  const columns = $derived(controlColumns(info.params).filter((c) => shown(c.name)));
  const loose = $derived(columns.filter((c) => !c.name));
  const banded = $derived(columns.filter((c) => c.name));
</script>

{#snippet cell(p: EffectParamInfo)}
  {@const v = value(p)}
  {#if p.toggle}
    <div class="cell">
      <span class="name">{p.name}</span>
      <button type="button" class="switch" class:on={v >= 0.5} aria-pressed={v >= 0.5} aria-label={p.name} onclick={() => oncommit(p, v >= 0.5 ? 0 : 1)}>{v >= 0.5 ? "On" : "Off"}</button>
    </div>
  {:else if p.choices?.length}
    <div class="cell wide">
      <span class="name">{p.name}</span>
      <select aria-label={p.name} value={String(Math.round(v))} onchange={(e) => oncommit(p, Number(e.currentTarget.value))}>
        {#each p.choices as c, ci (c)}<option value={String(p.min + ci)}>{c}</option>{/each}
      </select>
    </div>
  {:else}
    <div class="cell">
      <span class="name" title={p.name}>{p.name}</span>
      <Knob
        label={p.name}
        value={toSlider(p, v)}
        home={toSlider(p, p.default)}
        text={formatParam(p, v)}
        title="Drag up or down to turn (Ctrl for fine, Shift for coarse). Double-click for the default."
        oninput={(t) => oninput(p, fromSlider(p, t))}
        oncommit={(t) => oncommit(p, fromSlider(p, t))}
      />
      <ValueField param={p} value={v} oninput={(x) => oninput(p, x)} oncommit={(x) => oncommit(p, x)} />
    </div>
  {/if}
{/snippet}

<div class="controls">
  {#each loose as c, i (i)}
    <div class="row">
      {#each c.params as p (p.key)}{@render cell(p)}{/each}
    </div>
  {/each}
  {#if banded.length}
    <div class="bands">
      {#each banded as c (c.name)}
        {@const off = c.power ? value(c.power) < 0.5 : false}
        <section class="band" class:off>
          <header>
            <span class="bname" title={c.name}>{c.name}</span>
            {#if c.power}
              {@const pw = c.power}
              <button
                type="button"
                class="power"
                class:on={!off}
                aria-pressed={!off}
                aria-label="{c.name} on"
                title={off ? `Turn ${c.name} on` : `Turn ${c.name} off`}
                onclick={() => oncommit(pw, off ? 1 : 0)}
              ></button>
            {/if}
          </header>
          {#each c.params as p (p.key)}{@render cell(p)}{/each}
        </section>
      {/each}
    </div>
  {/if}
</div>

<style>
  .controls {
    display: grid;
    gap: 6px;
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 6px;
    align-items: flex-start;
  }
  .bands {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(70px, 1fr));
    gap: 4px;
  }
  .band {
    min-width: 0;
    display: grid;
    gap: 4px;
    align-content: start;
    justify-items: center;
    padding: 4px 3px 6px;
    border: 1px solid var(--border);
    border-radius: 3px;
    background: rgba(0, 0, 0, 0.18);
  }
  .band.off .cell {
    opacity: 0.45;
  }
  .band header {
    display: flex;
    gap: 4px;
    align-items: center;
    justify-content: center;
    width: 100%;
    min-width: 0;
  }
  .bname {
    overflow: hidden;
    color: var(--text);
    font-size: 11px;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .power {
    flex: none;
    width: 10px;
    height: 10px;
    padding: 0;
    border: 1px solid var(--border-strong);
    border-radius: 50%;
    background: var(--surface-3, #333);
    cursor: pointer;
  }
  .power.on {
    border-color: var(--accent);
    background: var(--accent);
  }
  .cell {
    display: grid;
    justify-items: center;
    gap: 2px;
    width: 66px;
  }
  .cell.wide {
    width: 120px;
  }
  .band .cell,
  .band .cell.wide {
    width: 100%;
    max-width: 66px;
  }
  .band .cell.wide {
    max-width: none;
  }
  .name {
    max-width: 100%;
    overflow: hidden;
    color: var(--text-dim);
    font-size: 11px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  select {
    width: 100%;
    min-width: 0;
    height: 22px;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    background: var(--surface-3, var(--surface-2));
    color: var(--text);
    font: inherit;
    font-size: 11px;
  }
  .switch {
    height: 20px;
    min-width: 40px;
    margin-top: 4px;
    padding: 0 8px;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    background: var(--surface-3, var(--surface-2));
    color: var(--text-dim);
    font: inherit;
    font-size: 11px;
    cursor: pointer;
  }
  .switch.on {
    border-color: var(--accent);
    color: var(--accent);
  }
</style>
