<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  The Amp Model's window: the capture and cabinet in use, with Load and
  Clear for each, then the gate, input, tone and output dials. Loading
  reads the file into the slot itself (saved with the session), as one
  undoable edit. Our own look: a dark faceplate with a woven grille band.
-->
<script lang="ts">
  import type { EffectInfo, EffectParamInfo } from "../../app/core";
  import { ampLoadIr, ampLoadModel, ampPick } from "../../app/core";
  import type { EffectSlotDoc } from "../../stores/session";
  import { paramValue } from "./chain";
  import { ampSummary } from "./amp";
  import BigKnob from "./BigKnob.svelte";

  interface Props {
    slot: EffectSlotDoc;
    info: EffectInfo;
    /** The session's (or file's) sample rate, to warn when a capture was made at another. */
    rate: number;
    onParams: (changes: [EffectParamInfo, number][], label: string, done: boolean) => void;
    /** Saves new slot data (or none) as one undoable edit. */
    onData: (data: string | null, label: string) => void;
    onError: (e: unknown) => void;
  }

  const { slot, info, rate, onParams, onData, onError }: Props = $props();

  const byKey = $derived(new Map(info.params.map((p) => [p.key, p])));
  const P = (k: string) => byKey.get(k)!;
  const val = (k: string) => paramValue(slot, P(k));
  const set = (p: EffectParamInfo, v: number, done: boolean) => onParams([[p, Math.max(p.min, Math.min(p.max, v))]], `${info.name} ${p.name}`, done);
  const sum = $derived(ampSummary(slot.data ?? null));
  let busy = $state(false);
  /** Why the last load failed, shown in the window until the next one. */
  let failed = $state<string | null>(null);

  async function load(kind: "model" | "ir") {
    if (busy) return;
    busy = true;
    try {
      const path = await ampPick(kind);
      if (!path) return;
      failed = null;
      const data = kind === "model" ? await ampLoadModel(path, slot.data ?? null) : await ampLoadIr(path, slot.data ?? null);
      onData(data, kind === "model" ? "Load Amp Capture" : "Load Cabinet");
    } catch (e) {
      failed = `${kind === "model" ? "That capture" : "That cabinet"} did not load: ${e instanceof Error ? e.message : String(e)}`;
      onError(e);
    } finally {
      busy = false;
    }
  }

  function clear(kind: "model" | "ir") {
    const d = slot.data ? (JSON.parse(slot.data) as Record<string, unknown>) : {};
    if (kind === "model") {
      delete d.model;
      delete d.model_name;
    } else delete d.ir;
    onData(Object.keys(d).length ? JSON.stringify(d) : null, kind === "model" ? "Clear Amp Capture" : "Clear Cabinet");
  }
</script>

{#snippet dial(k: string, size = 64)}
  {@const p = P(k)}
  <BigKnob param={p} value={val(k)} {size} oninput={(v) => set(p, v, false)} oncommit={(v) => set(p, v, true)} />
{/snippet}
{#snippet sw(k: string, label: string)}
  {@const p = P(k)}
  {@const on = Math.round(val(k)) === 1}
  <button type="button" class="sw" class:on aria-pressed={on} title="{p.name}: {on ? 'on' : 'off'}" onclick={() => set(p, on ? 0 : 1, true)}>
    <span class="led"></span>{label}
  </button>
{/snippet}

<div class="amp">
  <div class="grille" aria-hidden="true"></div>
  <div class="slots">
    <div class="file">
      <span class="what">Capture</span>
      {#if sum.model}
        <span class="name" title={sum.model.detail}>{sum.model.name}</span>
        <span class="meta">{sum.model.kind}{sum.model.rate ? ` · ${(sum.model.rate / 1000).toLocaleString("en-US")} kHz` : ""}{sum.model.gear ? ` · ${sum.model.gear}` : ""}</span>
      {:else}
        <span class="name empty">None loaded: sound passes through the gate and tone only.</span>
      {/if}
      <span class="btns">
        <button type="button" disabled={busy} onclick={() => void load("model")}>Load...</button>
        {#if sum.model}<button type="button" onclick={() => clear("model")}>Clear</button>{/if}
      </span>
    </div>
    {#if failed}<p class="warn">{failed}</p>{/if}
    {#if sum.problem}<p class="warn">{sum.problem}</p>{/if}
    {#if sum.model?.rate && Math.abs(sum.model.rate - rate) > 1}
      <p class="warn">This capture was made at {(sum.model.rate / 1000).toLocaleString("en-US")} kHz and plays here at {(rate / 1000).toLocaleString("en-US")} kHz, so its tone shifts a little. A session at {(sum.model.rate / 1000).toLocaleString("en-US")} kHz plays it exactly.</p>
    {/if}
    <div class="file">
      <span class="what">Cabinet</span>
      {#if sum.ir}
        <span class="name">{sum.ir.name}</span>
        <span class="meta">{sum.ir.ms} ms · {(sum.ir.rate / 1000).toLocaleString("en-US")} kHz</span>
      {:else}
        <span class="name empty">None: load an impulse response for captures of an amp alone.</span>
      {/if}
      <span class="btns">
        <button type="button" disabled={busy} onclick={() => void load("ir")}>Load...</button>
        {#if sum.ir}<button type="button" onclick={() => clear("ir")}>Clear</button>{/if}
      </span>
    </div>
  </div>
  <div class="row">
    <div class="gate">
      {@render sw("gate", "Gate")}
      {@render dial("gate_db", 54)}
    </div>
    {@render dial("input", 72)}
    {@render dial("bass")}
    {@render dial("middle")}
    {@render dial("treble")}
    {@render dial("output", 72)}
  </div>
  <div class="switches">
    {@render sw("level", "Level Match")}
    {@render sw("cab", "Cabinet")}
  </div>
  <p class="hint">Input sets how hard the capture is driven. Level Match brings every capture to about the same loudness, so switching between them is fair. The capture plays in mono; both channels get the result.</p>
</div>

<style>
  .amp {
    --ink: #e4e6e1;
    --hot: #e2683c;
    display: grid;
    gap: 10px;
    margin-bottom: 6px;
    padding: 0 12px 12px;
    border: 1px solid #000;
    border-radius: 6px;
    background: linear-gradient(180deg, #2b2d2c, #1d1f1e);
    box-shadow: inset 0 0 0 2px #3c403e;
    overflow: hidden;
    user-select: none;
  }
  .grille {
    height: 14px;
    margin: 0 -12px;
    background:
      repeating-linear-gradient(45deg, rgba(255, 255, 255, 0.06) 0 2px, transparent 2px 5px),
      repeating-linear-gradient(-45deg, rgba(0, 0, 0, 0.35) 0 2px, transparent 2px 5px),
      #3a3529;
    border-bottom: 2px solid #6b5f45;
  }
  .slots {
    display: grid;
    gap: 6px;
  }
  .file {
    display: grid;
    grid-template-columns: 64px minmax(0, 1fr) auto;
    grid-template-areas: "what name btns" "what meta btns";
    column-gap: 8px;
    align-items: center;
    padding: 6px 8px;
    border: 1px solid #000;
    border-radius: 4px;
    background: #121413;
  }
  .what {
    grid-area: what;
    color: var(--hot);
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.1em;
    text-transform: uppercase;
  }
  .name {
    grid-area: name;
    overflow: hidden;
    color: var(--ink);
    font-size: 12px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .name.empty {
    opacity: 0.5;
    font-size: 11px;
  }
  .meta {
    grid-area: meta;
    color: var(--ink);
    opacity: 0.55;
    font-size: 10px;
  }
  .btns {
    grid-area: btns;
    display: flex;
    gap: 4px;
  }
  .row {
    display: flex;
    gap: 12px;
    align-items: flex-end;
    justify-content: space-between;
    padding-top: 4px;
  }
  .gate {
    display: grid;
    gap: 4px;
    justify-items: center;
  }
  .switches {
    display: flex;
    gap: 10px;
    justify-content: center;
  }
  .sw {
    display: flex;
    gap: 6px;
    align-items: center;
    height: 22px;
    padding: 0 8px;
    border: 1px solid #000;
    border-radius: 3px;
    background: linear-gradient(180deg, #3b3e3c, #262827);
    color: var(--ink);
    font-size: 10px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    cursor: pointer;
  }
  .led {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: #3b1d12;
    box-shadow: inset 0 0 0 1px #000;
  }
  .sw.on .led {
    background: var(--hot);
    box-shadow: 0 0 5px var(--hot);
  }
  .warn {
    margin: 0;
    color: #f0b46a;
    font-size: 10.5px;
  }
  .hint {
    margin: 0;
    color: var(--ink);
    opacity: 0.5;
    font-size: 10.5px;
  }
</style>
