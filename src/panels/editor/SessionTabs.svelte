<!-- SPDX-License-Identifier: GPL-3.0-or-later
     Copyright (c) The Audiform contributors.
     This file is part of Audiform, based on Altered Audio Editor. -->
<!--
  The open sessions as tabs above the Multitrack editor, like the documents
  in an image editor: click one to bring it forward, the small x closes it
  (asking to save first when it has unsaved changes).
-->
<script lang="ts">
  import type { Readable } from "svelte/store";
  import type { SessionRow } from "../files/session-row";

  interface Props {
    sessions: Readable<SessionRow[]>;
    onOpen: (id: string) => void;
    onClose: (id: string) => void;
  }
  const { sessions, onOpen, onClose }: Props = $props();

  let rows = $state<SessionRow[]>([]);
  $effect(() => sessions.subscribe((v) => (rows = v)));
</script>

{#if rows.length > 0}
  <div class="tabs" role="tablist" aria-label="Open sessions">
    {#each rows as r (r.id)}
      <div class="tab" class:front={r.front} role="presentation">
        <button type="button" class="name" role="tab" aria-selected={r.front} title={r.name} onclick={() => onOpen(r.id)}>
          {r.name}{r.dirty ? " *" : ""}
        </button>
        <button type="button" class="x" title={`Close ${r.name}${r.dirty ? " (asks to save first)" : ""}`} aria-label={`Close ${r.name}`} onclick={() => onClose(r.id)}>×</button>
      </div>
    {/each}
  </div>
{/if}

<style>
  .tabs {
    display: flex;
    align-items: stretch;
    gap: 1px;
    min-width: 0;
    overflow-x: auto;
    flex: none;
    padding: 2px 4px 0;
    background: var(--surface-2);
    border-bottom: 1px solid var(--border);
    scrollbar-width: none;
  }
  .tab {
    display: flex;
    align-items: center;
    max-width: 220px;
    min-width: 70px;
    border: 1px solid var(--border);
    border-bottom: 0;
    border-radius: 4px 4px 0 0;
    background: color-mix(in srgb, var(--surface-1) 70%, transparent);
    color: var(--text-dim);
  }
  .tab.front {
    background: var(--surface-0);
    color: var(--text);
    border-color: var(--border-strong);
    box-shadow: inset 0 2px 0 var(--accent);
  }
  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-align: left;
    padding: 3px 4px 3px 10px;
    border: 0 !important;
    background: transparent !important;
    box-shadow: none !important;
    color: inherit;
    font: inherit;
    font-size: 11px;
    height: auto !important;
  }
  .x {
    width: 16px;
    height: 16px !important;
    margin-right: 4px;
    padding: 0 !important;
    border: 0 !important;
    border-radius: 3px;
    background: transparent !important;
    box-shadow: none !important;
    color: var(--text-faint);
    font-size: 13px;
    line-height: 14px;
  }
  .x:hover {
    background: color-mix(in srgb, var(--danger) 30%, transparent) !important;
    color: var(--text);
  }
</style>
