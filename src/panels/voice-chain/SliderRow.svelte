<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The Audiform contributors.
  This file is part of Audiform, based on Altered Audio Editor.

  One effect setting as a long slider over a numbered scale, with its
  value in a box to type into. Logarithmic settings (times, frequencies)
  spread their scale to match. Double-click the slider to reset it.
-->
<script lang="ts">
  import type { EffectParamInfo } from "../../app/core";
  import { fromSlider, toSlider } from "./chain";
  import ValueField from "./ValueField.svelte";

  interface Props {
    param: EffectParamInfo;
    value: number;
    /** Values to number along the scale. */
    marks: number[];
    /** Words under the scale, at values. */
    words?: { at: number; text: string }[];
    /** Shown instead of the setting's own name. */
    label?: string;
    disabled?: boolean;
    /** Width of the name column, in pixels. */
    labelWidth?: number;
    set: (v: number, done: boolean) => void;
  }

  const { param, value, marks, words = [], label, disabled = false, labelWidth = 112, set }: Props = $props();
  const STEPS = 1000;
  const at = (v: number) => Math.min(100, Math.max(0, toSlider(param, v) * 100));
  const short = (m: number) => (Math.abs(m) >= 1000 ? `${m / 1000}k` : String(m));
</script>

<div class="row" class:disabled style:--label="{labelWidth}px">
  <span class="name">{label ?? param.name}</span>
  <div class="scale">
    <div class="nums" aria-hidden="true">
      {#each marks as m (m)}<span style:left="{at(m)}%">{short(m)}</span>{/each}
    </div>
    <input
      type="range"
      min="0"
      max={STEPS}
      step="1"
      {disabled}
      value={Math.round(toSlider(param, value) * STEPS)}
      aria-label={label ?? param.name}
      title="{label ?? param.name}. Double-click to reset."
      oninput={(e) => set(fromSlider(param, Number(e.currentTarget.value) / STEPS), false)}
      onchange={(e) => set(fromSlider(param, Number(e.currentTarget.value) / STEPS), true)}
      ondblclick={() => set(param.default, true)}
    />
    <div class="ticks" aria-hidden="true">
      {#each marks as m (m)}<i style:left="{at(m)}%"></i>{/each}
    </div>
    {#if words.length}
      <div class="words" aria-hidden="true">
        {#each words as w (w.text)}<span style:left="{at(w.at)}%">{w.text}</span>{/each}
      </div>
    {/if}
  </div>
  <ValueField {param} {value} oninput={(x) => set(x, false)} oncommit={(x) => set(x, true)} />
</div>

<style>
  .row {
    display: grid;
    grid-template-columns: var(--label, 112px) minmax(140px, 1fr) 72px;
    gap: 10px;
    align-items: center;
  }
  .row.disabled {
    opacity: 0.45;
  }
  .name {
    color: var(--text-dim);
    text-align: right;
    font-size: 12px;
  }
  .scale {
    position: relative;
    padding: 13px 0 8px;
    container-type: inline-size;
  }
  /* A short scale keeps only its end numbers, so they never pile up. */
  @container (max-width: 240px) {
    .nums span:not(:first-child):not(:last-child) {
      display: none;
    }
  }
  .row:has(.words) .scale {
    padding-bottom: 16px;
  }
  .nums,
  .ticks,
  .words {
    position: absolute;
    left: 7px;
    right: 7px;
    pointer-events: none;
  }
  .nums {
    top: 0;
    height: 11px;
  }
  .ticks {
    top: 29px;
    height: 4px;
  }
  .words {
    bottom: 0;
    height: 12px;
  }
  .nums span,
  .words span {
    position: absolute;
    transform: translateX(-50%);
    color: var(--text-faint);
    font-size: 10px;
    white-space: nowrap;
  }
  .words span {
    color: var(--text-dim);
  }
  .ticks i {
    position: absolute;
    width: 1px;
    height: 4px;
    background: var(--text-faint);
  }
  input[type="range"] {
    width: 100%;
    margin: 0;
    accent-color: var(--accent);
  }
</style>
