<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  A horizontal fader for the 3D Space panel, in the Mixer's fader style:
  a recessed slot with tick marks and a block cap with a center line.
  Drag the cap (or anywhere on the slot) to set it; Ctrl while dragging
  for fine steps; the wheel steps it; double-click goes back to its
  default; arrow keys step it when it has focus.
-->
<script lang="ts">
  interface Props {
    label: string;
    value: number;
    min: number;
    max: number;
    step: number;
    /** Where double-click puts it, and where the slot's mark sits. */
    origin?: number;
    format: (v: number) => string;
    title?: string;
    disabled?: boolean;
    onChange: (v: number) => void;
  }

  const { label, value, min, max, step, origin = min, format, title, disabled = false, onChange }: Props = $props();

  const frac = (v: number) => (Math.min(max, Math.max(min, v)) - min) / (max - min || 1);
  const ticks = $derived(Array.from({ length: 11 }, (_, i) => i / 10));

  let track = $state<HTMLDivElement | null>(null);
  const snapTo = (v: number) => Math.min(max, Math.max(min, Math.round(v / step) * step));

  function onDown(e: PointerEvent) {
    if (disabled || e.button !== 0 || !track) return;
    e.preventDefault();
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    el.focus();
    const r = track.getBoundingClientRect();
    const onCap = (e.target as HTMLElement).closest(".cap") !== null;
    const x0 = e.clientX;
    const v0 = value;
    const at = (x: number) => min + ((x - r.left) / Math.max(1, r.width)) * (max - min);
    // Pressing the slot jumps there; pressing the cap keeps where it was grabbed.
    if (!onCap) onChange(snapTo(at(e.clientX)));
    const base = onCap ? v0 : snapTo(at(e.clientX));
    const move = (ev: PointerEvent) => {
      const fine = ev.ctrlKey ? 0.1 : 1;
      const dv = ((ev.clientX - x0) / Math.max(1, r.width)) * (max - min) * fine;
      onChange(snapTo(base + dv));
    };
    const up = () => {
      el.removeEventListener("pointermove", move);
      el.removeEventListener("pointerup", up);
      el.removeEventListener("pointercancel", up);
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up);
    el.addEventListener("pointercancel", up);
  }

  function onWheel(e: WheelEvent) {
    if (disabled) return;
    e.preventDefault();
    const n = e.shiftKey ? 10 : 1;
    onChange(snapTo(value + (e.deltaY < 0 ? step : -step) * n));
  }

  function onKey(e: KeyboardEvent) {
    if (disabled) return;
    const n = e.shiftKey ? 10 : 1;
    if (e.key === "ArrowRight" || e.key === "ArrowUp") onChange(snapTo(value + step * n));
    else if (e.key === "ArrowLeft" || e.key === "ArrowDown") onChange(snapTo(value - step * n));
    else if (e.key === "Home") onChange(min);
    else if (e.key === "End") onChange(max);
    else return;
    e.preventDefault();
    e.stopPropagation();
  }
</script>

<div class="fader" class:disabled {title}>
  <div class="head">
    <span class="lab">{label}</span>
    <span class="val">{format(value)}</span>
  </div>
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div
    class="body"
    role="slider"
    tabindex={disabled ? -1 : 0}
    aria-label={label}
    aria-valuemin={min}
    aria-valuemax={max}
    aria-valuenow={value}
    aria-valuetext={format(value)}
    aria-disabled={disabled}
    onpointerdown={onDown}
    onwheel={onWheel}
    onkeydown={onKey}
    ondblclick={() => !disabled && onChange(origin)}
  >
    <div class="track" bind:this={track}>
      {#each ticks as t (t)}<span class="tick" class:major={t === 0 || t === 0.5 || t === 1} style:left="{t * 100}%"></span>{/each}
      <span class="slot"></span>
      <span class="fill" style:left="{Math.min(frac(origin), frac(value)) * 100}%" style:width="{Math.abs(frac(value) - frac(origin)) * 100}%"></span>
      <span class="mark" style:left="{frac(origin) * 100}%"></span>
      <span class="cap" style:left="{frac(value) * 100}%"></span>
    </div>
  </div>
</div>

<style>
  .fader {
    display: grid;
    gap: 2px;
  }
  .fader.disabled {
    opacity: 0.45;
  }
  .head {
    display: flex;
    justify-content: space-between;
    color: var(--text-dim);
  }
  .val {
    color: var(--value);
    font-family: var(--font-mono);
  }
  .body {
    padding: 0 8px;
    cursor: ew-resize;
    outline: none;
  }
  .body:focus-visible .cap {
    border-color: var(--accent);
  }
  .track {
    position: relative;
    height: 22px;
  }
  .tick {
    position: absolute;
    bottom: 0;
    width: 1px;
    height: 4px;
    background: var(--border-strong);
  }
  .tick.major {
    height: 7px;
  }
  .slot {
    position: absolute;
    top: 8px;
    left: 0;
    right: 0;
    height: 3px;
    border-radius: 1px;
    background: var(--surface-0);
    box-shadow: inset 0 1px 1px rgba(0, 0, 0, 0.6);
  }
  .fill {
    position: absolute;
    top: 8px;
    height: 3px;
    background: color-mix(in srgb, var(--accent) 55%, transparent);
  }
  .mark {
    position: absolute;
    top: 3px;
    width: 1px;
    height: 13px;
    background: var(--border-strong);
  }
  .cap {
    position: absolute;
    top: 2px;
    width: 12px;
    height: 16px;
    margin-left: -6px;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    background: linear-gradient(var(--surface-3), var(--surface-2));
    box-sizing: border-box;
    cursor: grab;
  }
  .cap::after {
    content: "";
    position: absolute;
    top: 2px;
    bottom: 2px;
    left: 4px;
    width: 2px;
    background: var(--text);
    opacity: 0.8;
  }
</style>
