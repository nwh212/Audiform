<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The Audiform contributors.
  This file is part of Audiform, based on Altered Audio Editor.

  The row of tabs across the top of an effect window that picks what
  kind of effect it is (Room, Hall, Plate...; Digital, Tape...).
-->
<script lang="ts">
  interface Props {
    kinds: readonly string[];
    current: number;
    /** What each tab says when hovered. */
    tips?: readonly string[];
    label: string;
    pick: (i: number) => void;
  }

  const { kinds, current, tips = [], label, pick }: Props = $props();
</script>

<div class="tabs" role="tablist" aria-label={label}>
  {#each kinds as k, i (k)}
    <button type="button" role="tab" class:on={i === current} aria-selected={i === current} title={tips[i] ?? k} onclick={() => i !== current && pick(i)}>{k}</button>
  {/each}
</div>

<style>
  .tabs {
    display: flex;
    gap: 2px;
    padding: 0 10px;
    border-bottom: 1px solid var(--border);
  }
  button {
    flex: 1;
    padding: 6px 8px 5px;
    border: 1px solid transparent;
    border-bottom: 0;
    border-radius: 5px 5px 0 0;
    background: transparent;
    color: var(--text-dim);
    font: inherit;
    font-size: 12px;
    white-space: nowrap;
    cursor: pointer;
  }
  button:hover {
    color: var(--text);
    background: color-mix(in srgb, var(--accent) 8%, transparent);
  }
  button.on {
    border-color: var(--border);
    background: color-mix(in srgb, var(--accent) 16%, var(--panel, transparent));
    color: var(--text);
    font-weight: 600;
    box-shadow: inset 0 2px 0 var(--accent);
  }
</style>
