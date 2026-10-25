<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<!-- Copyright (c) The GeekCast Radio Network, LLC. -->
<!-- This file is part of Altered Audio Editor. -->

<script lang="ts">
  import { dragValue, keyValue, pointerAngle } from "./knob";

  /**
   * A small dial. `value` is 0 to 1. Drag up or down to turn it (Ctrl for
   * fine steps, Shift for coarse), double-click for `home`, arrow keys and
   * Page Up/Down to step. `oninput` fires while turning, `oncommit` once on
   * release (or per key press).
   */
  let {
    value,
    home = 0.5,
    label,
    text,
    disabled = false,
    title,
    oninput,
    oncommit,
  }: {
    value: number;
    home?: number;
    label: string;
    text: string;
    disabled?: boolean;
    title?: string;
    oninput: (t: number) => void;
    oncommit: (t: number) => void;
  } = $props();

  let drag: { y: number; start: number; last: number } | null = null;

  function down(e: PointerEvent) {
    if (disabled || e.button !== 0) return;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    drag = { y: e.clientY, start: value, last: value };
    e.preventDefault();
  }
  function move(e: PointerEvent) {
    if (!drag) return;
    const t = dragValue(drag.start, drag.y - e.clientY, { ctrl: e.ctrlKey, shift: e.shiftKey });
    if (t === drag.last) return;
    drag.last = t;
    oninput(t);
  }
  function up() {
    if (!drag) return;
    const t = drag.last;
    const moved = t !== drag.start;
    drag = null;
    if (moved) oncommit(t);
  }
  function key(e: KeyboardEvent) {
    if (disabled) return;
    const t = keyValue(value, e.key, home);
    if (t === null) return;
    e.preventDefault();
    if (t !== value) oncommit(t);
  }
</script>

<div
  class="knob"
  class:disabled
  role="slider"
  tabindex={disabled ? -1 : 0}
  aria-label={label}
  aria-valuemin={0}
  aria-valuemax={1}
  aria-valuenow={value}
  aria-valuetext={text}
  {title}
  onpointerdown={down}
  onpointermove={move}
  onpointerup={up}
  onpointercancel={up}
  ondblclick={() => !disabled && oncommit(home)}
  onkeydown={key}
>
  <svg viewBox="0 0 28 28" aria-hidden="true">
    <circle class="body" cx="14" cy="14" r="11" />
    <line class="tick" x1="14" y1="14" x2="14" y2="5" transform="rotate({pointerAngle(value)} 14 14)" />
  </svg>
</div>

<style>
  .knob {
    width: 28px;
    height: 28px;
    cursor: ns-resize;
    touch-action: none;
    border-radius: 50%;
  }
  .knob:focus-visible {
    outline: 1px solid var(--accent);
    outline-offset: 1px;
  }
  .knob.disabled {
    opacity: 0.4;
    cursor: default;
  }
  .body {
    fill: var(--surface-3);
    stroke: var(--border-strong);
    stroke-width: 1.5;
  }
  .knob:hover .body {
    stroke: var(--accent);
  }
  .tick {
    stroke: var(--text);
    stroke-width: 2;
    stroke-linecap: round;
  }
</style>
