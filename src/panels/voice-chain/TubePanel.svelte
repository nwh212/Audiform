<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Altered Audio Editor.

  The hardware-style windows for the Opto Leveler and the Tube Preamp:
  big numbered dials for the main controls, a needle meter fed by the
  effect itself, lever switches, and small dials for the rest. Every
  control also has a number box to type into. Our own look: a charcoal
  faceplate with brass trim.
-->
<script lang="ts">
  import type { EffectInfo, EffectParamInfo } from "../../app/core";
  import type { EffectSlotDoc } from "../../stores/session";
  import { paramValue } from "./chain";
  import BigKnob from "./BigKnob.svelte";
  import NeedleMeter from "./NeedleMeter.svelte";

  interface Props {
    slot: EffectSlotDoc;
    info: EffectInfo;
    onParams: (changes: [EffectParamInfo, number][], label: string, done: boolean) => void;
  }

  const { slot, info, onParams }: Props = $props();

  const byKey = $derived(new Map(info.params.map((p) => [p.key, p])));
  const P = (k: string) => byKey.get(k)!;
  const val = (k: string) => paramValue(slot, P(k));
  const set = (p: EffectParamInfo, v: number, done: boolean) => onParams([[p, Math.max(p.min, Math.min(p.max, v))]], `${info.name} ${p.name}`, done);
  const opto = $derived(info.id === "altaudio.opto_leveler");
  const NUMS = [0, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100];
</script>

{#snippet big(k: string, marks: number[], label?: string)}
  {@const p = P(k)}
  <BigKnob param={p} value={val(k)} size={112} {marks} {label} oninput={(v) => set(p, v, false)} oncommit={(v) => set(p, v, true)} />
{/snippet}
{#snippet small(k: string)}
  {@const p = P(k)}
  <BigKnob param={p} value={val(k)} size={58} oninput={(v) => set(p, v, false)} oncommit={(v) => set(p, v, true)} />
{/snippet}
{#snippet lever(k: string, up: string, down: string)}
  {@const p = P(k)}
  {@const on = Math.round(val(k)) === 1}
  <div class="lever">
    <span class:lit={on}>{down}</span>
    <button type="button" class:on aria-pressed={on} aria-label="{p.name}: {on ? down : up}" title="{p.name}: click to switch" onclick={() => set(p, on ? 0 : 1, true)}><span class="bat"></span></button>
    <span class:lit={!on}>{up}</span>
  </div>
{/snippet}

<div class="tube" class:opto>
  <div class="plate">
    <span class="title">{info.name}</span>
    <span class="sub">{opto ? "optical leveling amplifier" : "vintage tube preamplifier"}</span>
  </div>
  {#if opto}
    <div class="main">
      {@render big("gain", NUMS)}
      <div class="mid">
        <NeedleMeter slotId={slot.id} mode={Math.round(val("meter")) === 1 ? "out" : "gr"} />
        <div class="switches">
          {@render lever("mode", "Compress", "Limit")}
          {@render lever("meter", "Reduction", "Output")}
        </div>
      </div>
      {@render big("peak", NUMS)}
    </div>
    <div class="row">
      {@render small("emphasis")}
      {@render small("tube")}
      {@render small("mix")}
    </div>
    <p class="hint">Turn up Peak Reduction until the needle shows a few dB of leveling, then set Gain for the level you want. Emphasis makes it listen harder to "s" sounds.</p>
  {:else}
    <div class="main">
      {@render big("input", [-12, 0, 12, 24])}
      {@render big("drive", NUMS)}
      <div class="mid">
        <NeedleMeter slotId={slot.id} mode="out" width={200} />
        <div class="switches">
          {@render lever("transformer", "Clean", "Transformer")}
        </div>
      </div>
      {@render big("output", [-24, -12, 0, 12])}
    </div>
    <div class="row">
      {@render small("low")}
      {@render small("high")}
      {@render small("mix")}
    </div>
    <p class="hint">Drive adds warmth; past halfway it starts to sound overdriven. The needle shows the output against a 0 mark at -18 dBFS.</p>
  {/if}
</div>

<style>
  .tube {
    --panel-ink: #eadfc4;
    --panel-accent: #d9a441;
    display: grid;
    gap: 10px;
    margin-bottom: 6px;
    padding: 10px 12px 12px;
    border: 1px solid #000;
    border-radius: 6px;
    background:
      linear-gradient(180deg, rgba(255, 255, 255, 0.05), rgba(0, 0, 0, 0.2)),
      #26272a;
    box-shadow:
      inset 0 0 0 2px #8a6a2d,
      inset 0 0 0 3px #1a1a1c;
    user-select: none;
  }
  .plate {
    display: flex;
    gap: 10px;
    align-items: baseline;
    padding-bottom: 6px;
    border-bottom: 1px solid rgba(217, 164, 65, 0.35);
  }
  .title {
    color: var(--panel-accent);
    font-size: 15px;
    font-weight: 700;
    letter-spacing: 0.12em;
    text-transform: uppercase;
  }
  .sub {
    color: var(--panel-ink);
    opacity: 0.6;
    font-size: 10px;
    font-style: italic;
    letter-spacing: 0.08em;
  }
  .main {
    display: flex;
    gap: 14px;
    align-items: center;
    justify-content: space-between;
  }
  .mid {
    display: grid;
    gap: 10px;
    justify-items: center;
  }
  .switches {
    display: flex;
    gap: 18px;
  }
  .row {
    display: flex;
    gap: 22px;
    justify-content: center;
    padding-top: 6px;
    border-top: 1px solid rgba(217, 164, 65, 0.2);
  }
  .lever {
    display: grid;
    justify-items: center;
    gap: 2px;
    color: var(--panel-ink);
    font-size: 9px;
    letter-spacing: 0.1em;
    text-transform: uppercase;
  }
  .lever span {
    opacity: 0.45;
  }
  .lever span.lit {
    opacity: 1;
    color: var(--panel-accent);
  }
  .lever button {
    position: relative;
    width: 22px;
    height: 30px;
    padding: 0;
    border: 1px solid #000;
    border-radius: 11px;
    background: radial-gradient(circle at 50% 50%, #5b5d62, #1b1c1e 70%);
    cursor: pointer;
  }
  .lever .bat {
    position: absolute;
    left: 7px;
    top: 15px;
    width: 6px;
    height: 13px;
    border-radius: 3px;
    background: linear-gradient(90deg, #8d8f94, #e8e9ec, #8d8f94);
    transform-origin: 50% 0;
    transform: rotate(180deg);
  }
  .lever button.on .bat {
    transform: rotate(0deg);
  }
  .hint {
    margin: 0;
    color: var(--panel-ink);
    opacity: 0.55;
    font-size: 10.5px;
  }
</style>
