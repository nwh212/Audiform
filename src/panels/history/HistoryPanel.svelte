<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  History panel. Lists every edit since the session opened. Click a row
  to return to that point; rows after it dim until a new edit replaces
  them.
-->
<script lang="ts">
  import { editor } from "../../stores/editor";
  import type { HistorySnapshot, UndoStack } from "../../commands";

  interface Props {
    stack: UndoStack;
    /** The Waveform editor's history, shown instead while that editor shows. */
    waveStack?: UndoStack;
    /** Shortcut text for an action id, for tooltips. */
    shortcutFor?: (id: string) => string | undefined;
    onError?: (error: unknown) => void;
  }

  const { stack, waveStack, shortcutFor = () => undefined, onError = console.error }: Props = $props();

  let mode = $state(editor.current().mode);
  $effect(() =>
    editor.subscribe((s) => {
      mode = s.mode;
    }),
  );
  /** The history of whichever editor shows. */
  const active = $derived(mode === "waveform" && waveStack ? waveStack : stack);

  const withKey = (label: string, id: string) => {
    const key = shortcutFor(id);
    return key ? `${label} (${key})` : label;
  };

  // Filled by the subscription below, which calls back immediately.
  let snap = $state<HistorySnapshot | null>(null);
  let list = $state<HTMLOListElement | null>(null);

  $effect(() =>
    active.subscribe((s) => {
      snap = s;
    }),
  );

  // Keep the current row in view as history grows.
  $effect(() => {
    // Re-run whenever the position or entry count changes.
    void snap?.position;
    void snap?.entries.length;
    const current = list?.querySelector<HTMLElement>("[aria-current='step']");
    current?.scrollIntoView({ block: "nearest" });
  });

  function jump(position: number) {
    active.jumpTo(position).catch(onError);
  }

  function formatTime(ms: number): string {
    return new Date(ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" });
  }
</script>

{#if snap}
<section class="panel" aria-label="History">
  <ol class="rows" bind:this={list}>
    <li>
      <button
        type="button"
        class="row base"
        class:current={snap.position === 0}
        aria-current={snap.position === 0 ? "step" : undefined}
        onclick={() => jump(0)}
      >
        <span class="label">{snap.baseLabel}</span>
      </button>
    </li>
    {#each snap.entries as entry, i (entry.id)}
      {@const position = i + 1}
      <li>
        <button
          type="button"
          class="row"
          class:current={snap.position === position}
          class:undone={position > snap.position}
          aria-current={snap.position === position ? "step" : undefined}
          title={formatTime(entry.time)}
          onclick={() => jump(position)}
        >
          <span class="label">{entry.label}</span>
        </button>
      </li>
    {/each}
  </ol>

  <footer class="bar">
    <span class="count">
      {snap.entries.length}
      {snap.entries.length === 1 ? "Undo" : "Undos"}
    </span>
    <span class="spacer"></span>
    <button
      type="button"
      class="icon"
      disabled={!snap.canUndo}
      title={withKey(snap.undoLabel ? `Undo ${snap.undoLabel}` : "Undo", "edit.undo")}
      aria-label="Undo"
      onclick={() => active.undo().catch(onError)}
    >
      <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M6 3 2 7l4 4M2.5 7H10a4 4 0 0 1 0 8H8" /></svg>
    </button>
    <button
      type="button"
      class="icon"
      disabled={!snap.canRedo}
      title={withKey(snap.redoLabel ? `Redo ${snap.redoLabel}` : "Redo", "edit.redo")}
      aria-label="Redo"
      onclick={() => active.redo().catch(onError)}
    >
      <svg viewBox="0 0 16 16" aria-hidden="true"><path d="m10 3 4 4-4 4M13.5 7H6a4 4 0 0 0 0 8h2" /></svg>
    </button>
  </footer>
</section>
{/if}

<style>
  .panel {
    display: grid;
    grid-template-rows: 1fr auto;
    height: 100%;
    min-height: 0;
    background: var(--surface-1);
  }

  .rows {
    margin: 0;
    padding: var(--space-1) 0;
    list-style: none;
    overflow-y: auto;
    min-height: 0;
  }

  .row {
    display: flex;
    align-items: center;
    width: 100%;
    height: var(--row-height);
    padding: 0 var(--space-3);
    border: 0;
    background: transparent;
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: default;
  }

  .row:hover {
    background: var(--surface-2);
  }

  .row.current {
    background: var(--accent-dim);
    color: #fff;
  }

  .row.undone {
    color: var(--text-faint);
    font-style: italic;
  }

  .row:focus-visible {
    outline: 1px solid var(--accent);
    outline-offset: -1px;
  }

  .label {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .bar {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: var(--row-height);
    padding: 0 var(--space-2) 0 var(--space-3);
    border-top: 1px solid var(--border);
    background: var(--surface-2);
    color: var(--text-dim);
    font-size: var(--font-size-small);
  }

  .spacer {
    flex: 1;
  }

  .icon {
    display: grid;
    place-items: center;
    width: 20px;
    height: 18px;
    padding: 0;
    border: 0;
    border-radius: 2px;
    background: transparent;
    color: var(--text-dim);
  }

  .icon:hover:not(:disabled) {
    background: var(--surface-3);
    color: var(--text);
  }

  .icon:disabled {
    color: var(--text-faint);
    opacity: 0.5;
  }

  .icon svg {
    width: 14px;
    height: 14px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.5;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
</style>
