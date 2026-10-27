<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The Audiform contributors.
  This file is part of Audiform, based on Altered Audio Editor.

  The Stereo Width window: two long sliders over numbered scales, Center
  Pan (left to right) and Width (mono, as is, wide), each with its value
  in a box to type into. Double-click a slider to reset it.
-->
<script lang="ts">
  import type { EffectInfo, EffectParamInfo } from "../../app/core";
  import type { EffectSlotDoc } from "../../stores/session";
  import { paramValue } from "./chain";
  import ValueField from "./ValueField.svelte";

  interface Props {
    slot: EffectSlotDoc;
    info: EffectInfo;
    onParams: (changes: [EffectParamInfo, number][], label: string, done: boolean) => void;
  }

  const { slot, info, onParams }: Props = $props();
  const P = (k: string) => info.params.find((p) => p.key === k)!;
  const set = (p: EffectParamInfo, v: number, done: boolean) => onParams([[p, Math.max(p.min, Math.min(p.max, v))]], `${info.name} ${p.name}`, done);

  interface Row {
    key: string;
    marks: number[];
    words: { at: number; text: string }[];
  }
  const ROWS: Row[] = [
    {
      key: "pan",
      marks: [-100, -75, -50, -25, 0, 25, 50, 75, 100],
      words: [
        { at: -100, text: "Left" },
        { at: 0, text: "Center" },
        { at: 100, text: "Right" },
      ],
    },
    {
      key: "width",
      marks: [0, 50, 100, 150, 200, 250, 300],
      words: [
        { at: 0, text: "Mono" },
        { at: 100, text: "As is" },
        { at: 300, text: "Wide" },
      ],
    },
  ];
  const pos = (p: EffectParamInfo, v: number) => ((v - p.min) / (p.max - p.min)) * 100;
</script>

<div class="width">
  {#each ROWS as row (row.key)}
    {@const p = P(row.key)}
    {@const v = paramValue(slot, p)}
    <div class="row">
      <span class="name">{p.name}</span>
      <div class="scale">
        <div class="nums" aria-hidden="true">
          {#each row.marks as m (m)}<span style:left="{pos(p, m)}%">{m}</span>{/each}
        </div>
        <input
          type="range"
          min={p.min}
          max={p.max}
          step={p.step}
          value={v}
          aria-label={p.name}
          title="{p.name}. Double-click to reset."
          oninput={(e) => set(p, Number(e.currentTarget.value), false)}
          onchange={(e) => set(p, Number(e.currentTarget.value), true)}
          ondblclick={() => set(p, p.default, true)}
        />
        <div class="ticks" aria-hidden="true">
          {#each row.marks as m (m)}<i style:left="{pos(p, m)}%"></i>{/each}
        </div>
        <div class="words" aria-hidden="true">
          {#each row.words as w (w.text)}<span style:left="{pos(p, w.at)}%">{w.text}</span>{/each}
        </div>
      </div>
      <ValueField param={p} value={v} oninput={(x) => set(p, x, false)} oncommit={(x) => set(p, x, true)} />
    </div>
  {/each}
</div>

<style>
  .width {
    display: grid;
    gap: 18px;
    padding: 16px 14px 18px;
  }
  .row {
    display: grid;
    grid-template-columns: 82px 1fr 64px;
    gap: 12px;
    align-items: center;
  }
  .name {
    color: var(--text-dim);
    text-align: right;
  }
  .scale {
    position: relative;
    padding: 14px 0 16px;
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
    height: 12px;
  }
  .ticks {
    top: 30px;
    height: 5px;
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
    height: 5px;
    background: var(--text-faint);
  }
  input[type="range"] {
    width: 100%;
    margin: 0;
    accent-color: var(--accent);
  }
</style>
