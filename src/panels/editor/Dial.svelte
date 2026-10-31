<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  A small round dial with its value beside it. Drag up or right to raise
  it, down or left to lower it; Shift for fine steps; double-click the
  dial to reset it, or double-click the number to type a value.
-->
<script lang="ts">
  import { tick as nextTick } from "svelte";
  import { parseDialValue } from "../mixer/fader";
  import { toTurn } from "../../app/knob";

  interface Props {
    value: number;
    min: number;
    max: number;
    /** Value change per pixel of drag. Shift divides it by 5. */
    step: number;
    /** Value the dial resets to on double-click, and where its arc starts. */
    origin: number;
    label: string;
    /** Small symbol before the dial so volume and pan read at a glance. */
    icon?: "volume" | "pan";
    format: (v: number) => string;
    onChange: (v: number) => void;
  }

  const { value, min, max, step, origin, label, icon, format, onChange }: Props = $props();

  // The reset value (0 dB, center) points straight up; each half of the
  // turn covers its own side of the range.
  const angle = (v: number) =>
    -135 + toTurn((Math.min(max, Math.max(min, v)) - min) / (max - min), (origin - min) / (max - min)) * 270;
  const a = $derived(angle(value));
  const a0 = $derived(angle(origin));

  function point(deg: number, r: number): [number, number] {
    const rad = ((deg - 90) * Math.PI) / 180;
    return [8 + r * Math.cos(rad), 8 + r * Math.sin(rad)];
  }

  const arc = $derived.by(() => {
    const from = Math.min(a, a0);
    const to = Math.max(a, a0);
    if (to - from < 0.5) return "";
    const [x0, y0] = point(from, 6.5);
    const [x1, y1] = point(to, 6.5);
    return `M${x0} ${y0} A6.5 6.5 0 ${to - from > 180 ? 1 : 0} 1 ${x1} ${y1}`;
  });
  const tick = $derived(point(a, 4.5));

  // Typing a value: the number turns into a box while editing.
  let editing = $state(false);
  let text = $state("");
  let box = $state<HTMLInputElement | null>(null);
  async function startEdit(e: MouseEvent) {
    e.stopPropagation();
    text = format(value).replace(/\s*dB$/i, "");
    editing = true;
    await nextTick();
    box?.focus();
    box?.select();
  }
  function finishEdit(keep: boolean) {
    if (!editing) return;
    editing = false;
    if (!keep) return;
    const v = parseDialValue(text, min);
    if (v === null) return;
    const clamped = Math.min(max, Math.max(min, v));
    if (clamped !== value) onChange(clamped);
  }

  function drag(e: PointerEvent) {
    if (e.button !== 0 || editing) return;
    // The number is for typing: a press there waits for the double-click.
    if ((e.target as Element).closest(".val")) return;
    e.preventDefault();
    e.stopPropagation();
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    const sx = e.clientX;
    const sy = e.clientY;
    const start = value;
    let last = start;
    const move = (ev: PointerEvent) => {
      const s = ev.shiftKey ? step / 5 : step;
      const d = ev.clientX - sx - (ev.clientY - sy);
      const decimals = step < 1 ? 1 : 0;
      const k = 10 ** decimals;
      const v = Math.min(max, Math.max(min, Math.round((start + d * s) * k) / k));
      if (v !== last) {
        last = v;
        onChange(v);
      }
    };
    const up = () => {
      el.removeEventListener("pointermove", move);
      el.removeEventListener("pointerup", up);
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up);
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<span
  class="dial"
  title="{label}. Drag the dial up or right to raise, Shift for fine steps; double-click the dial to reset, or the number to type a value."
  onpointerdown={drag}
  ondblclick={(e) => {
    if (!(e.target as Element).closest(".val")) onChange(origin);
  }}
>
  {#if icon === "volume"}
    <svg class="sym" viewBox="0 0 12 12" aria-hidden="true"><path d="M1 9.5h1.5M3.5 8h1.5M6 6h1.5M8.5 4h1.5" stroke="currentColor" stroke-width="1" /><path d="M1 10v1.5M3.5 9v2.5M6 7.5v4M8.5 5.5v6M11 3v8.5" stroke="currentColor" stroke-width="1.2" /></svg>
  {:else if icon === "pan"}
    <svg class="sym" viewBox="0 0 12 12" aria-hidden="true"><path d="M1 6h10M1 6l2-2M1 6l2 2M11 6l-2-2M11 6l-2 2" fill="none" stroke="currentColor" stroke-width="1.1" /></svg>
  {/if}
  <svg class="knob" viewBox="0 0 16 16" aria-hidden="true">
    <circle cx="8" cy="8" r="7.2" fill="var(--surface-0)" />
    <circle cx="8" cy="8" r="6.5" fill="none" stroke="currentColor" stroke-width="1.4" opacity="0.35" />
    {#if arc}<path d={arc} fill="none" stroke="var(--value)" stroke-width="1.6" />{/if}
    <line x1="8" y1="8" x2={tick[0]} y2={tick[1]} stroke="var(--text)" stroke-width="1.5" stroke-linecap="round" />
  </svg>
  {#if editing}
    <input
      class="val edit"
      bind:this={box}
      bind:value={text}
      aria-label={label}
      onkeydown={(e) => {
        e.stopPropagation();
        if (e.key === "Enter") finishEdit(true);
        else if (e.key === "Escape") finishEdit(false);
      }}
      onblur={() => finishEdit(true)}
      onpointerdown={(e) => e.stopPropagation()}
    />
  {:else}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <span class="val" ondblclick={startEdit}>{format(value)}</span>
  {/if}
</span>

<style>
  .dial {
    display: inline-flex;
    gap: 3px;
    align-items: center;
    padding: 1px 3px 1px 1px;
    border-radius: 3px;
    color: var(--text-dim);
    cursor: ns-resize;
  }

  .dial:hover {
    background: var(--surface-3);
  }

  .knob {
    width: 19px;
    height: 19px;
  }

  .sym {
    width: 10px;
    height: 10px;
    color: var(--text-faint);
  }

  .val {
    min-width: 34px;
    cursor: text;
    color: var(--value);
    font-family: var(--font-mono);
    font-size: 11px;
    white-space: nowrap;
  }
  .val.edit {
    width: 42px;
    padding: 0 2px;
    border: 1px solid var(--accent);
    border-radius: 2px;
    background: var(--surface-0);
    outline: none;
  }
</style>
