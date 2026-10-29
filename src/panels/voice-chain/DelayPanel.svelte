<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The Audiform contributors.
  This file is part of Audiform, based on Altered Audio Editor.

  The Delay and Echo window: a tab for each kind (Digital, Tape,
  Analog), then the time (in milliseconds, or a note value at the
  tempo), the repeats, their color and the output.
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
    /** The session tempo, to follow. */
    tempo?: number | undefined;
    onParams: (changes: [EffectParamInfo, number][], label: string, done: boolean) => void;
  }

  const { slot, info, tempo, onParams }: Props = $props();
  const P = (k: string) => info.params.find((p) => p.key === k)!;
  const val = (k: string) => paramValue(slot, P(k));
  const set = (k: string) => (v: number, done: boolean) => {
    const p = P(k);
    onParams([[p, Math.max(p.min, Math.min(p.max, v))]], `${info.name} ${p.name}`, done);
  };
  const KINDS = ["Digital", "Tape", "Analog"];
  const TIPS = [
    "Clean repeats, each a copy of the last",
    "A tape loop: wow, flutter and saturation, each pass a little darker",
    "A bucket-brigade delay: darker repeats that clip softly",
  ];
  const kind = $derived(Math.round(val("kind")));
  const sync = $derived(val("sync") >= 0.5);
  const notes = $derived(P("note").choices ?? []);
  const PCT = [0, 25, 50, 75, 100];

  function follow(on: boolean) {
    const changes: [EffectParamInfo, number][] = [[P("sync"), on ? 1 : 0]];
    if (on && tempo) changes.push([P("tempo"), Math.max(30, Math.min(300, tempo))]);
    onParams(changes, on ? "Delay Follows Tempo" : "Delay Time in ms", true);
  }
</script>

<div class="delay">
  <KindTabs kinds={KINDS} current={kind} tips={TIPS} label="Kind of delay" pick={(i) => set("kind")(i, true)} />
  <div class="body">
    <fieldset>
      <legend>Time</legend>
      <div class="sync">
        <label title="The time follows the tempo as a note value">
          <input type="checkbox" checked={sync} onchange={(e) => follow(e.currentTarget.checked)} />
          Follow tempo
        </label>
        {#if sync}
          <select aria-label="Note value" value={Math.round(val("note"))} onchange={(e) => set("note")(Number(e.currentTarget.value), true)}>
            {#each notes as n, i (n)}<option value={i}>{n}</option>{/each}
          </select>
          <span class="bpm">at {Math.round(val("tempo") * 10) / 10} BPM</span>
          {#if tempo && Math.abs(tempo - val("tempo")) > 0.05}
            <button type="button" title="Use the session tempo" onclick={() => set("tempo")(tempo, true)}>Use {tempo} BPM</button>
          {/if}
        {/if}
      </div>
      {#if !sync}
        <SliderRow param={P("time")} value={val("time")} marks={[1, 10, 50, 100, 250, 500, 1000, 2000, 4000]} set={set("time")} />
      {/if}
    </fieldset>
    <fieldset>
      <legend>Repeats</legend>
      <SliderRow param={P("feedback")} value={val("feedback")} marks={[0, 25, 50, 75, 100, 120]} set={set("feedback")} />
      <SliderRow param={P("low_cut")} value={val("low_cut")} marks={[20, 50, 100, 200, 500, 1000, 2000]} set={set("low_cut")} />
      <SliderRow param={P("tone")} value={val("tone")} marks={[500, 1000, 2000, 4000, 8000, 16000]} label="High cut" set={set("tone")} />
      <label class="pp" title="Each repeat crosses to the other side">
        <input type="checkbox" checked={val("pingpong") >= 0.5} onchange={(e) => set("pingpong")(e.currentTarget.checked ? 1 : 0, true)} />
        Ping-pong
      </label>
    </fieldset>
    <fieldset>
      <legend>Color</legend>
      <SliderRow param={P("depth")} value={val("depth")} marks={PCT} label={kind === 1 ? "Wow and flutter" : "Modulation"} set={set("depth")} />
      <SliderRow param={P("rate")} value={val("rate")} marks={[0.05, 0.1, 0.5, 1, 2, 5, 10]} set={set("rate")} />
      <SliderRow param={P("drive")} value={val("drive")} marks={PCT} label={kind === 1 ? "Tape drive" : "Drive"} set={set("drive")} />
    </fieldset>
    <fieldset>
      <legend>Output</legend>
      <SliderRow param={P("dry")} value={val("dry")} marks={PCT} set={set("dry")} />
      <SliderRow param={P("wet")} value={val("wet")} marks={PCT} set={set("wet")} />
    </fieldset>
  </div>
</div>

<style>
  .delay {
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
  .sync {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    align-items: center;
    padding-left: 124px;
  }
  .sync label,
  .pp {
    display: flex;
    gap: 6px;
    align-items: center;
    cursor: pointer;
  }
  .pp {
    padding-left: 124px;
  }
  .bpm {
    color: var(--text-dim);
  }
  input[type="checkbox"] {
    accent-color: var(--accent);
  }
</style>
