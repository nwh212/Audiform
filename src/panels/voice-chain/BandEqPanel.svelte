<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The Audiform contributors.
  This file is part of Audiform, based on Altered Audio Editor.

  The Band EQ window: one tall slider per band in a row, lowest band on
  the left, with the band's level above it and its frequency below. Drag
  a slider (Ctrl for fine steps, Shift for coarse) or turn the mouse
  wheel over it; double-click sets it back to 0 dB; click the level to
  type one. Drag across several sliders with Alt held to draw a shape.
  Output is the slider on the right. Flat puts every band back to 0 dB
  in one step. Every change is one undoable step.
-->
<script lang="ts">
  import type { EffectInfo, EffectParamInfo } from "../../app/core";
  import type { EffectSlotDoc } from "../../stores/session";
  import { paramValue } from "./chain";
  import VSlider from "./VSlider.svelte";

  interface Props {
    slot: EffectSlotDoc;
    info: EffectInfo;
    onParams: (changes: [EffectParamInfo, number][], label: string, done: boolean) => void;
  }

  const { slot, info, onParams }: Props = $props();

  const bands = $derived(info.params.filter((p) => p.group === "Bands"));
  const output = $derived(info.params.find((p) => p.key === "output") ?? null);
  const H = 180;
  const colW = $derived(bands.length <= 10 ? 46 : bands.length <= 20 ? 30 : 24);
  const SCALE = [15, 10, 5, 0, -5, -10, -15];

  const short = (name: string) => name.replace(/ kHz$/, "k").replace(/ Hz$/, "");
  const sign = (v: number) => (v > 0.04 ? "+" : "") + v.toFixed(1);
  const clamp = (p: EffectParamInfo, v: number) => Math.max(p.min, Math.min(p.max, v));
  const tOf = (p: EffectParamInfo, v: number) => (clamp(p, v) - p.min) / (p.max - p.min);
  const step = (p: EffectParamInfo, v: number) => Math.round(v / p.step) * p.step;

  // One drag: plain moves one band; with Alt held it draws across bands.
  let drag: { p: EffectParamInfo; y: number; start: number; draw: boolean; changes: Map<string, [EffectParamInfo, number]> } | null = null;
  let rows = $state<HTMLDivElement | null>(null);

  function valueAt(p: EffectParamInfo, el: Element, clientY: number) {
    const r = el.getBoundingClientRect();
    return step(p, clamp(p, p.min + (1 - (clientY - r.top) / r.height) * (p.max - p.min)));
  }
  function send(done: boolean) {
    if (!drag) return;
    const list = [...drag.changes.values()];
    onParams(list, list.length > 1 ? `${info.name} Bands` : `${info.name} ${list[0]?.[0].name ?? ""}`, done);
  }
  function down(e: PointerEvent, p: EffectParamInfo) {
    if (e.button !== 0) return;
    e.preventDefault();
    const track = e.currentTarget as HTMLElement;
    track.setPointerCapture(e.pointerId);
    const onThumb = (e.target as HTMLElement).classList.contains("thumb");
    const v = onThumb ? paramValue(slot, p) : valueAt(p, track, e.clientY);
    drag = { p, y: e.clientY, start: v, draw: e.altKey, changes: new Map([[p.key, [p, v]]]) };
    if (!onThumb) send(false);
  }
  function move(e: PointerEvent) {
    if (!drag) return;
    if (drag.draw && rows) {
      // Drawing: whichever band is under the pointer takes the pointer's level.
      const el = document.elementsFromPoint(e.clientX, Math.min(Math.max(e.clientY, rows.getBoundingClientRect().top + 1), rows.getBoundingClientRect().bottom - 1)).find((x) => x instanceof HTMLElement && x.dataset.band);
      const p = el && bands.find((b) => b.key === (el as HTMLElement).dataset.band);
      if (p && el) drag.changes.set(p.key, [p, valueAt(p, el, e.clientY)]);
    } else {
      const scale = e.ctrlKey ? 0.2 : e.shiftKey ? 3 : 1;
      const p = drag.p;
      drag.changes.set(p.key, [p, step(p, clamp(p, drag.start + ((drag.y - e.clientY) / H) * (p.max - p.min) * scale))]);
    }
    send(false);
  }
  function up() {
    if (!drag) return;
    send(true);
    drag = null;
  }

  let wheelTimer: ReturnType<typeof setTimeout> | undefined;
  let wheelAt: [EffectParamInfo, number] | null = null;
  function wheel(e: WheelEvent, p: EffectParamInfo) {
    e.preventDefault();
    const base = wheelAt && wheelAt[0] === p ? wheelAt[1] : paramValue(slot, p);
    const v = step(p, clamp(p, base + (e.ctrlKey ? 0.1 : e.shiftKey ? 3 : 0.5) * (e.deltaY < 0 ? 1 : -1)));
    if (wheelAt && wheelAt[0] !== p) onParams([wheelAt], `${info.name} ${wheelAt[0].name}`, true);
    wheelAt = [p, v];
    onParams([[p, v]], `${info.name} ${p.name}`, false);
    clearTimeout(wheelTimer);
    wheelTimer = setTimeout(() => {
      if (wheelAt) onParams([wheelAt], `${info.name} ${wheelAt[0].name}`, true);
      wheelAt = null;
    }, 350);
  }
  $effect(() => () => clearTimeout(wheelTimer));

  let typing = $state<string | null>(null);
  function typed(p: EffectParamInfo, text: string) {
    typing = null;
    const v = Number.parseFloat(text.replace(",", "."));
    if (Number.isFinite(v)) onParams([[p, step(p, clamp(p, v))]], `${info.name} ${p.name}`, true);
  }
  const flat = $derived(bands.every((p) => Math.abs(paramValue(slot, p)) < 1e-6));
</script>

<div class="beq" style:--col="{colW}px">
  <div class="body">
    <div class="scale" style:height="{H}px">
      {#each SCALE as d (d)}<span style:top="{(1 - (d + 15) / 30) * 100}%">{d > 0 ? `+${d}` : d}</span>{/each}
    </div>
    <div class="rows" bind:this={rows}>
      {#each SCALE as d (d)}<div class="line" class:zero={d === 0} style:top="calc(18px + {(1 - (d + 15) / 30) * H}px)"></div>{/each}
      {#each bands as p (p.key)}
        {@const v = paramValue(slot, p)}
        {@const t = tOf(p, v)}
        {@const z = tOf(p, 0)}
        <div class="col">
          {#if typing === p.key}
            <!-- svelte-ignore a11y_autofocus -->
            <input
              class="val"
              value={v.toFixed(1)}
              autofocus
              onkeydown={(e) => {
                if (e.key === "Enter") typed(p, e.currentTarget.value);
                else if (e.key === "Escape") typing = null;
              }}
              onblur={(e) => typed(p, e.currentTarget.value)}
            />
          {:else}
            <button type="button" class="val" class:off={Math.abs(v) < 0.05} title="Click to type a level" onclick={() => (typing = p.key)}>{sign(v)}</button>
          {/if}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            class="track"
            data-band={p.key}
            style:height="{H}px"
            role="slider"
            tabindex="0"
            aria-label="{p.name} band"
            aria-valuemin={p.min}
            aria-valuemax={p.max}
            aria-valuenow={v}
            aria-orientation="vertical"
            title="{p.name}: {sign(v)} dB. Drag up or down (Ctrl fine, Shift coarse, Alt to draw across bands), wheel, or double-click for 0 dB."
            onpointerdown={(e) => down(e, p)}
            onpointermove={move}
            onpointerup={up}
            onpointercancel={() => (drag = null)}
            onwheel={(e) => wheel(e, p)}
            ondblclick={() => onParams([[p, 0]], `${info.name} ${p.name}`, true)}
            onkeydown={(e) => {
              const d = e.key === "ArrowUp" ? 0.5 : e.key === "ArrowDown" ? -0.5 : e.key === "PageUp" ? 3 : e.key === "PageDown" ? -3 : 0;
              if (!d) return;
              e.preventDefault();
              onParams([[p, step(p, clamp(p, v + d))]], `${info.name} ${p.name}`, true);
            }}
          >
            <div class="fill" style:bottom="{Math.min(t, z) * 100}%" style:height="{Math.abs(t - z) * 100}%"></div>
            <div class="thumb" style:bottom="calc({t * 100}% - 5px)"></div>
          </div>
          <span class="hz">{short(p.name)}</span>
        </div>
      {/each}
    </div>
    {#if output}
      {@const o = output}
      <div class="out">
        <VSlider param={o} value={paramValue(slot, o)} height={H} oninput={(v) => onParams([[o, v]], `${info.name} Output`, false)} oncommit={(v) => onParams([[o, v]], `${info.name} Output`, true)} />
      </div>
    {/if}
  </div>
  <div class="bar">
    <span class="hint how">Alt+drag across the sliders to draw a shape. Double-click a slider for 0 dB.</span>
    <button type="button" disabled={flat} title="Every band back to 0 dB" onclick={() => onParams(bands.map((p) => [p, 0]), `${info.name} Flat`, true)}>Flat</button>
  </div>
</div>

<style>
  .beq {
    display: grid;
    gap: 8px;
    margin-bottom: 8px;
  }
  .body {
    display: flex;
    gap: 6px;
    align-items: flex-start;
    padding: 8px 8px 6px;
    border: 1px solid var(--border);
    border-radius: 3px;
    background: rgba(0, 0, 0, 0.3);
  }
  .scale {
    position: relative;
    width: 24px;
    margin-top: 18px;
    flex: none;
  }
  .scale span {
    position: absolute;
    right: 0;
    transform: translateY(-50%);
    color: var(--text-dim);
    font-size: 9px;
    font-variant-numeric: tabular-nums;
  }
  .rows {
    position: relative;
    display: flex;
    flex: none;
  }
  .line {
    position: absolute;
    left: 0;
    right: 0;
    height: 1px;
    background: rgba(255, 255, 255, 0.06);
    pointer-events: none;
  }
  .line.zero {
    background: rgba(255, 255, 255, 0.2);
  }
  .col {
    display: grid;
    justify-items: center;
    width: var(--col);
    gap: 2px;
  }
  .val {
    width: calc(var(--col) - 2px);
    height: 16px;
    padding: 0;
    border: 0;
    background: none;
    color: var(--text);
    font: inherit;
    font-size: 9px;
    font-variant-numeric: tabular-nums;
    text-align: center;
  }
  .val.off {
    color: var(--text-dim);
  }
  input.val {
    border: 1px solid var(--accent);
    background: var(--surface-0);
  }
  .track {
    position: relative;
    width: 6px;
    border: 1px solid var(--border-strong);
    border-radius: 3px;
    background: rgba(0, 0, 0, 0.5);
    cursor: ns-resize;
    touch-action: none;
  }
  .track:focus-visible {
    outline: 1px solid var(--accent);
    outline-offset: 2px;
  }
  .fill {
    position: absolute;
    left: 0;
    right: 0;
    border-radius: 2px;
    background: color-mix(in srgb, var(--accent) 60%, transparent);
    pointer-events: none;
  }
  .thumb {
    position: absolute;
    left: calc(50% - min(9px, var(--col) / 2 - 2px));
    width: min(18px, calc(var(--col) - 4px));
    height: 10px;
    border: 1px solid #0e0e10;
    border-radius: 2px;
    background: linear-gradient(#e8e8ec, #a8a8b0);
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.5);
  }
  .thumb::after {
    content: "";
    position: absolute;
    left: 2px;
    right: 2px;
    top: 4px;
    height: 1px;
    background: #2a2a30;
  }
  .hz {
    color: var(--text-dim);
    font-size: 9px;
    white-space: nowrap;
  }
  .out {
    margin-left: auto;
    padding-left: 8px;
    border-left: 1px solid var(--border);
  }
  .bar {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .hint {
    flex: 1;
    margin: 0;
    color: var(--text-dim);
    font-size: var(--font-size-small);
  }
  .bar button {
    background: var(--surface-0);
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    color: var(--text);
    font: inherit;
    padding: 1px 10px;
  }
  .bar button:disabled {
    opacity: 0.45;
  }
</style>
