<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The Audiform contributors.
  This file is part of Audiform, based on Altered Audio Editor.

  The Delay and Echo window, as a piece of hardware. Across the top, the
  kind (Digital, Tape, Analog: each has its own faceplate color) and the
  Follow Tempo and Ping-pong switches. In the middle, the two big dials
  (Time, or the note value when following the tempo, and Feedback)
  either side of a lit display with the time and a picture of the
  repeats fading away (alternating sides when they ping-pong). Under
  them, a row of small dials for the modulation, filters, drive and the
  levels, and the room spread used on surround. The Simple kind shows a
  time, a mix and an Invert switch for each side instead, with the
  times in milliseconds or samples.
-->
<script lang="ts">
  import type { EffectInfo, EffectParamInfo } from "../../app/core";
  import type { EffectSlotDoc } from "../../stores/session";
  import BigKnob from "./BigKnob.svelte";
  import { formatParam, paramValue } from "./chain";
  import "./faceplate.css";

  interface Props {
    slot: EffectSlotDoc;
    info: EffectInfo;
    /** The session tempo, to follow. */
    tempo?: number | undefined;
    /** The sample rate, for times shown in samples. */
    rate?: number | undefined;
    onParams: (changes: [EffectParamInfo, number][], label: string, done: boolean) => void;
  }

  const { slot, info, tempo, rate = 48_000, onParams }: Props = $props();
  const P = (k: string) => info.params.find((p) => p.key === k)!;
  const val = (k: string) => paramValue(slot, P(k));
  const set = (k: string, v: number, done: boolean) => {
    const p = P(k);
    onParams([[p, Math.max(p.min, Math.min(p.max, v))]], `${info.name} ${p.name}`, done);
  };

  const KINDS = [
    { name: "Digital", tip: "Clean repeats, each a copy of the last" },
    { name: "Tape", tip: "A tape loop: wow, flutter and saturation, each pass a little darker" },
    { name: "Analog", tip: "A bucket-brigade delay: darker repeats that clip softly" },
    { name: "Simple", tip: "Each side moved by its own time, early or late, with its own mix and polarity: for lining up or widening" },
  ];
  const BEATS = [1 / 16, 1 / 12, 1 / 8, 1 / 6, 0.25, 0.375, 1 / 3, 0.5, 0.75, 2 / 3, 1, 1.5, 2, 3, 4, 8];

  const kind = $derived(Math.round(val("kind")));
  const simple = $derived(kind === 3);
  const samples = $derived(val("units") >= 0.5);
  /** A side's time as shown: ms, or samples at the session rate. */
  const timeText = (ms: number) => (samples ? `${Math.round((ms / 1000) * rate)} smp` : `${ms > 0 ? "+" : ""}${ms.toFixed(2)} ms`);
  const sync = $derived(val("sync") >= 0.5);
  const pingpong = $derived(val("pingpong") >= 0.5);
  const bpm = $derived(val("tempo"));
  const ms = $derived(sync ? ((BEATS[Math.round(val("note"))] ?? 0.5) * 60_000) / Math.max(30, bpm) : val("time"));
  const fb = $derived(val("feedback") / 100);

  function follow(on: boolean) {
    const changes: [EffectParamInfo, number][] = [[P("sync"), on ? 1 : 0]];
    if (on && tempo) changes.push([P("tempo"), Math.max(30, Math.min(300, tempo))]);
    onParams(changes, on ? "Delay Follows Tempo" : "Delay Time in ms", true);
  }

  // The repeats over the next few seconds: when each comes, how loud, which side.
  const W = 240;
  const H = 64;
  const repeats = $derived.by(() => {
    const span = Math.max(1500, Math.min(6000, ms * 8));
    const out: { x: number; h: number; up: boolean }[] = [];
    let level = 1;
    for (let k = 1; k * ms <= span && out.length < 48; k++) {
      out.push({ x: (k * ms * W) / span, h: Math.min(1, level), up: !pingpong || k % 2 === 1 });
      level *= fb;
      if (level < 0.01) break;
    }
    return { list: out, span };
  });
  const msText = (v: number) => (v >= 1000 ? `${(v / 1000).toFixed(2)} s` : `${Math.round(v)} ms`);
</script>

<div class="fp-shell"><div class="fp-unit" class:fp-tape={kind === 1} class:fp-analog={kind === 2}>
  <div class="fp-top">
    <div class="fp-kinds" role="tablist" aria-label="Kind of delay">
      {#each KINDS as k, i (k.name)}
        <button type="button" role="tab" class:lit={i === kind} aria-selected={i === kind} title={k.tip} onclick={() => i !== kind && set("kind", i, true)}>{k.name}</button>
      {/each}
    </div>
    {#if simple}
    <div class="fp-switches" role="radiogroup" aria-label="Show times in">
      <button type="button" role="radio" class:lit={!samples} aria-checked={!samples} title="Show times in milliseconds" onclick={() => set("units", 0, true)}>ms</button>
      <button type="button" role="radio" class:lit={samples} aria-checked={samples} title="Show times in samples" onclick={() => set("units", 1, true)}>Samples</button>
    </div>
    {:else}
    <div class="fp-switches">
      <button type="button" class:lit={sync} aria-pressed={sync} title="The time follows the tempo as a note value" onclick={() => follow(!sync)}>Tempo</button>
      <button type="button" class:lit={pingpong} aria-pressed={pingpong} title="Each repeat crosses to the other side. On surround, each repeat moves on to the next speaker round the room." onclick={() => set("pingpong", pingpong ? 0 : 1, true)}>Ping-pong</button>
    </div>
    {/if}
  </div>

  {#if simple}
  <div class="fp-screen simple-screen" aria-live="polite">
    <div class="fp-readout">
      <span class="fp-big">L {timeText(val("l_time"))}</span>
      <span class="fp-big">R {timeText(val("r_time"))}</span>
    </div>
  </div>
  <div class="fp-row fp-pairs">
    {#each [["l", "Left"], ["r", "Right"]] as const as [k, side] (k)}
      {@const inv = val(`${k}_invert`) >= 0.5}
      <div class="fp-group">
        <span class="fp-gname">{side}</span>
        <div class="fp-knobs">
          <BigKnob param={P(`${k}_time`)} value={val(`${k}_time`)} size={72} label="Time" marks={[-500, 0, 500]} oninput={(v) => set(`${k}_time`, v, false)} oncommit={(v) => set(`${k}_time`, v, true)} />
          <BigKnob param={P(`${k}_mix`)} value={val(`${k}_mix`)} size={72} label="Mix" marks={[0, 50, 100]} oninput={(v) => set(`${k}_mix`, v, false)} oncommit={(v) => set(`${k}_mix`, v, true)} />
        </div>
        <button type="button" class="fp-toggle" class:lit={inv} aria-pressed={inv} title="Flip this side's polarity" onclick={() => set(`${k}_invert`, inv ? 0 : 1, true)}>Invert</button>
      </div>
    {/each}
  </div>
  {:else}
  <div class="fp-mid">
    {#if sync}
      <BigKnob param={P("note")} value={val("note")} size={104} label="Time" oninput={(v) => set("note", Math.round(v), false)} oncommit={(v) => set("note", Math.round(v), true)} />
    {:else}
      <BigKnob param={P("time")} value={val("time")} size={104} label="Time" marks={[1, 100, 1000, 4000]} oninput={(v) => set("time", v, false)} oncommit={(v) => set("time", v, true)} />
    {/if}
    <div class="fp-screen" aria-live="polite">
      <div class="fp-readout">
        {#if sync}
          <span class="fp-big">{formatParam(P("note"), val("note"))}</span>
          <span class="fp-small">{Math.round(bpm * 10) / 10} BPM · {msText(ms)}</span>
        {:else}
          <span class="fp-big">{msText(ms)}</span>
          <span class="fp-small">{kind === 1 ? "tape" : kind === 2 ? "analog" : "digital"}</span>
        {/if}
      </div>
      <svg viewBox="0 0 {W} {H}" preserveAspectRatio="none" class="fp-echoes" role="img" aria-label="The repeats fading away">
        <line class="fp-axis" x1="0" x2={W} y1={H / 2} y2={H / 2} />
        <rect class="fp-dry" x="0" y={H / 2 - (H / 2 - 4)} width="3" height={H - 8} />
        {#each repeats.list as r, i (i)}
          {@const len = r.h * (H / 2 - 4)}
          <rect class="fp-rep" x={r.x - 1.5} y={r.up ? H / 2 - len : H / 2} width="3" height={Math.max(1, len)} />
        {/each}
      </svg>
      {#if sync && tempo && Math.abs(tempo - bpm) > 0.05}
        <button type="button" class="fp-use" title="Use the session tempo" onclick={() => set("tempo", tempo, true)}>Use session {tempo} BPM</button>
      {/if}
    </div>
    <BigKnob param={P("feedback")} value={val("feedback")} size={104} label="Feedback" marks={[0, 60, 120]} oninput={(v) => set("feedback", v, false)} oncommit={(v) => set("feedback", v, true)} />
  </div>

  <div class="fp-row">
    <div class="fp-group">
      <span class="fp-gname">Modulation</span>
      <div class="fp-knobs">
        <BigKnob param={P("depth")} value={val("depth")} size={56} label={kind === 1 ? "Wow" : "Depth"} oninput={(v) => set("depth", v, false)} oncommit={(v) => set("depth", v, true)} />
        <BigKnob param={P("rate")} value={val("rate")} size={56} label="Rate" oninput={(v) => set("rate", v, false)} oncommit={(v) => set("rate", v, true)} />
      </div>
    </div>
    <div class="fp-group">
      <span class="fp-gname">Filters</span>
      <div class="fp-knobs">
        <BigKnob param={P("low_cut")} value={val("low_cut")} size={56} label="Lo cut" oninput={(v) => set("low_cut", v, false)} oncommit={(v) => set("low_cut", v, true)} />
        <BigKnob param={P("tone")} value={val("tone")} size={56} label="Hi cut" oninput={(v) => set("tone", v, false)} oncommit={(v) => set("tone", v, true)} />
      </div>
    </div>
    <div class="fp-group" title="On a surround bed or a 3D Space master: how far each repeat spreads to the speakers either side. Stereo ignores it.">
      <span class="fp-gname">Room</span>
      <div class="fp-knobs">
        <BigKnob param={P("spread")} value={val("spread")} size={56} label="Spread" oninput={(v) => set("spread", v, false)} oncommit={(v) => set("spread", v, true)} />
      </div>
    </div>
    <div class="fp-group">
      <span class="fp-gname">Output</span>
      <div class="fp-knobs">
        <BigKnob param={P("drive")} value={val("drive")} size={56} label="Drive" oninput={(v) => set("drive", v, false)} oncommit={(v) => set("drive", v, true)} />
        <BigKnob param={P("dry")} value={val("dry")} size={56} label="Dry" oninput={(v) => set("dry", v, false)} oncommit={(v) => set("dry", v, true)} />
        <BigKnob param={P("wet")} value={val("wet")} size={56} label="Echo" oninput={(v) => set("wet", v, false)} oncommit={(v) => set("wet", v, true)} />
      </div>
    </div>
  </div>
  {/if}
</div></div>
