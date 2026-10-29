<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  One time in Time Info. Drag left or right on it to move it (one pixel
  is one pixel of the timeline; Ctrl for fine steps, Shift for coarse),
  turn the mouse wheel over it, or click it to type a time (Enter sets
  it, Escape leaves it).
-->
<script lang="ts">
  interface Props {
    /** Samples, or null for "-". */
    value: number | null;
    text: string;
    label: string;
    /** Samples per pixel of drag: the editor's zoom. */
    step: number;
    /** Reads typed text; null when it is not a time. */
    parse: (text: string) => number | null;
    /** Null leaves it read-only. */
    onset: ((v: number) => void) | null;
    big?: boolean;
    /** Given the typed text when Enter is pressed, before it is read as a time. */
    onenter?: (text: string) => void;
  }

  const { value, text, label, step, parse, onset, big = false, onenter }: Props = $props();

  let editing = $state(false);
  let draft = $state("");
  let input = $state<HTMLInputElement | null>(null);
  let drag: { x: number; start: number; moved: boolean } | null = null;

  function startEdit() {
    if (!onset && !onenter) return;
    draft = value === null ? "" : text;
    editing = true;
    queueMicrotask(() => input?.select());
  }
  function down(e: PointerEvent) {
    if (!onset && onenter && e.button === 0 && !editing) {
      e.preventDefault();
      startEdit();
      return;
    }
    if (!onset || value === null || e.button !== 0 || editing) return;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    drag = { x: e.clientX, start: value, moved: false };
    e.preventDefault();
  }
  function move(e: PointerEvent) {
    if (!drag || !onset) return;
    const dx = e.clientX - drag.x;
    if (!drag.moved && Math.abs(dx) < 3) return;
    drag.moved = true;
    const scale = e.ctrlKey ? 0.1 : e.shiftKey ? 10 : 1;
    onset(Math.max(0, Math.round(drag.start + dx * step * scale)));
  }
  function up() {
    const d = drag;
    drag = null;
    if (d && !d.moved) startEdit();
  }
  function wheel(e: WheelEvent) {
    if (!onset || value === null || editing) return;
    e.preventDefault();
    const scale = e.ctrlKey ? 1 : e.shiftKey ? 100 : 10;
    onset(Math.max(0, Math.round(value + (e.deltaY < 0 ? 1 : -1) * step * scale)));
  }
  function finish(keep: boolean, enter = false) {
    if (!editing) return;
    editing = false;
    if (enter) onenter?.(draft);
    if (!keep || !onset) return;
    const v = parse(draft);
    if (v !== null) onset(v);
  }
</script>

{#if editing}
  <input
    bind:this={input}
    class="tf edit"
    class:big
    aria-label={label}
    bind:value={draft}
    onkeydown={(e) => {
      if (e.key === "Enter") finish(true, true);
      else if (e.key === "Escape") finish(false);
      e.stopPropagation();
    }}
    onblur={() => finish(true)}
  />
{:else}
  <span
    class="tf"
    class:big
    class:live={onset && value !== null}
    role="spinbutton"
    tabindex={onset || onenter ? 0 : -1}
    aria-readonly={!onset}
    aria-label={label}
    aria-valuenow={value ?? undefined}
    aria-valuetext={text}
    title={onset ? `${label}: drag left or right, or use the mouse wheel, to move it (Ctrl for fine steps, Shift for coarse). Click to type a time.` : label}
    onpointerdown={down}
    onpointermove={move}
    onpointerup={up}
    onpointercancel={() => (drag = null)}
    onwheel={wheel}
    onkeydown={(e) => e.key === "Enter" && startEdit()}>{text}</span
  >
{/if}

<style>
  .tf {
    display: block;
    box-sizing: border-box;
    min-width: 0;
    height: 17px;
    padding: 0 3px;
    border: 1px solid transparent;
    border-radius: 2px;
    color: var(--value);
    font-family: var(--font-mono);
    font-size: var(--font-size-small);
    font-variant-numeric: tabular-nums;
    line-height: 15px;
    text-align: right;
    white-space: nowrap;
    user-select: none;
    touch-action: none;
  }
  .tf.big {
    height: 22px;
    font-size: 14px;
    line-height: 20px;
  }
  .tf.live {
    cursor: ew-resize;
  }
  .tf.live:hover {
    border-color: var(--border-strong);
  }
  .tf:not(.live) {
    color: var(--text-dim);
  }
  .tf:focus-visible {
    outline: 1px solid var(--accent);
  }
  .tf.edit {
    width: 100%;
    border-color: var(--accent);
    background: var(--surface-0, #000);
    color: var(--text);
    user-select: text;
  }
</style>
