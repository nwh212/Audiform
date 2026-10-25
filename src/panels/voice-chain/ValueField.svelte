<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Altered Audio Editor.

  A control's value in a number box. Click it to type a value (Enter
  sets it, Escape leaves it), or drag up or down on it to change it (Ctrl for
  fine steps, Shift for coarse) or turn the mouse wheel over it.
-->
<script lang="ts">
  import type { EffectParamInfo } from "../../app/core";
  import { dragValue } from "../../app/knob";
  import { formatParam, fromSlider, toSlider } from "./chain";
  import { parseParam } from "./effect-controls";

  interface Props {
    param: EffectParamInfo;
    value: number;
    /** While dragging or turning the wheel. */
    oninput: (v: number) => void;
    /** Once, when the change is done. */
    oncommit: (v: number) => void;
  }

  const { param, value, oninput, oncommit }: Props = $props();

  let editing = $state(false);
  let text = $state("");
  let input = $state<HTMLInputElement | null>(null);
  let drag: { y: number; start: number; last: number; moved: boolean } | null = null;

  function down(e: PointerEvent) {
    if (e.button !== 0 || editing) return;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    drag = { y: e.clientY, start: toSlider(param, value), last: value, moved: false };
    e.preventDefault();
  }
  function move(e: PointerEvent) {
    if (!drag) return;
    const dy = drag.y - e.clientY;
    if (!drag.moved && Math.abs(dy) < 3) return;
    drag.moved = true;
    const v = fromSlider(param, dragValue(drag.start, dy, { ctrl: e.ctrlKey, shift: e.shiftKey }));
    if (v === drag.last) return;
    drag.last = v;
    oninput(v);
  }
  function up() {
    if (!drag) return;
    const d = drag;
    drag = null;
    if (d.moved) {
      oncommit(d.last);
      return;
    }
    text = formatParam(param, value);
    editing = true;
    queueMicrotask(() => input?.select());
  }

  let wheelTimer: ReturnType<typeof setTimeout> | undefined;
  let wheelValue: number | null = null;
  function wheel(e: WheelEvent) {
    if (editing) return;
    e.preventDefault();
    const base = wheelValue ?? value;
    const step = (e.ctrlKey ? 0.002 : e.shiftKey ? 0.05 : 0.01) * (e.deltaY < 0 ? 1 : -1);
    const v = fromSlider(param, Math.min(1, Math.max(0, toSlider(param, base) + step)));
    if (v === base) return;
    wheelValue = v;
    oninput(v);
    clearTimeout(wheelTimer);
    wheelTimer = setTimeout(() => {
      const w = wheelValue;
      wheelValue = null;
      if (w !== null) oncommit(w);
    }, 350);
  }
  $effect(() => () => clearTimeout(wheelTimer));

  function finish(keep: boolean) {
    if (!editing) return;
    editing = false;
    if (!keep) return;
    const v = parseParam(param, text);
    if (v !== null && v !== value) oncommit(v);
  }
</script>

{#if editing}
  <input
    bind:this={input}
    class="vf edit"
    aria-label={param.name}
    bind:value={text}
    onkeydown={(e) => {
      if (e.key === "Enter") finish(true);
      else if (e.key === "Escape") finish(false);
      e.stopPropagation();
    }}
    onblur={() => finish(true)}
  />
{:else}
  <span
    class="vf"
    role="spinbutton"
    tabindex="0"
    aria-label={param.name}
    aria-valuemin={param.min}
    aria-valuemax={param.max}
    aria-valuenow={value}
    aria-valuetext={formatParam(param, value)}
    title="Click to type a value. Drag up or down, or use the mouse wheel, to change it (Ctrl for fine steps, Shift for coarse)."
    onpointerdown={down}
    onpointermove={move}
    onpointerup={up}
    onpointercancel={() => (drag = null)}
    onwheel={wheel}
    onkeydown={(e) => {
      if (e.key === "Enter") {
        text = formatParam(param, value);
        editing = true;
        queueMicrotask(() => input?.select());
      }
    }}>{formatParam(param, value)}</span
  >
{/if}

<style>
  .vf {
    display: block;
    box-sizing: border-box;
    width: 100%;
    height: 20px;
    padding: 0 3px;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    background: rgba(0, 0, 0, 0.35);
    color: var(--accent);
    font: inherit;
    font-size: 11px;
    font-variant-numeric: tabular-nums;
    line-height: 18px;
    text-align: center;
    white-space: nowrap;
    cursor: text;
    touch-action: none;
    user-select: none;
  }
  .vf:hover {
    border-color: var(--accent);
  }
  .vf:focus-visible {
    outline: 1px solid var(--accent);
  }
  .vf.edit {
    border-color: var(--accent);
    color: var(--text);
    user-select: text;
  }
</style>
