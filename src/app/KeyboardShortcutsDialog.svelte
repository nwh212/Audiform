<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  Edit > Keyboard Shortcuts. Lists every command with its shortcut and
  lets the user change, clear, or reset them. Changes save right away.
-->
<script lang="ts">
  import { chordFromEvent } from "../keymap/chord";
  import type { Keymap } from "../keymap/keymap";
  import { commandList, type CommandEntry } from "./menus/model";

  interface Props {
    keymap: Keymap;
    onChange: () => void;
    onClose: () => void;
  }

  const { keymap, onChange, onClose }: Props = $props();

  const commands: CommandEntry[] = commandList();
  const pathOf = new Map(commands.map((c) => [c.id, c.path]));

  let tick = $state(0);
  $effect(() =>
    keymap.changed.subscribe((n) => {
      tick = n;
    }),
  );

  let query = $state("");
  let search = $state<HTMLInputElement | null>(null);

  // Put the cursor in the search box when the dialog opens.
  $effect(() => {
    search?.focus();
  });
  let capturing = $state<string | null>(null);
  /** A key press that could not be read as a shortcut, shown so it can be reported. */
  let unread = $state<string | null>(null);
  let pending = $state<{ id: string; chord: string; owner: string } | null>(null);
  let notice = $state<string | null>(null);

  const visible = $derived.by(() => {
    void tick;
    const q = query.trim().toLowerCase();
    if (!q) return commands;
    return commands.filter(
      (c) =>
        c.path.toLowerCase().includes(q) ||
        keymap.chordsFor(c.id).some((k) => k.toLowerCase().includes(q)),
    );
  });

  function chords(id: string): readonly string[] {
    void tick;
    return keymap.chordsFor(id);
  }

  function changed(id: string): boolean {
    void tick;
    return keymap.isChanged(id);
  }

  function startCapture(id: string) {
    pending = null;
    notice = null;
    capturing = id;
    unread = null;
  }

  function apply(id: string, chord: string) {
    const lost = keymap.assign(id, chord);
    notice = lost ? `${chord} was removed from ${pathOf.get(lost) ?? lost}.` : null;
    pending = null;
    capturing = null;
    onChange();
  }

  // Capture phase, so the key never reaches menus or the app's shortcuts.
  function onKeyCapture(e: KeyboardEvent) {
    if (capturing === null) {
      if (e.key === "Escape" && pending === null) {
        e.preventDefault();
        onClose();
      }
      return;
    }
    e.preventDefault();
    e.stopPropagation();
    if (e.key === "Escape") {
      capturing = null;
      return;
    }
    const chord = chordFromEvent(e);
    if (!chord) {
      const lone = ["Control", "Alt", "Shift", "Meta", "AltGraph", "OS"].includes(e.key);
      if (!lone) unread = `key ${JSON.stringify(e.key)}, code ${e.code || "none"}${e.ctrlKey ? ", Ctrl" : ""}${e.altKey ? ", Alt" : ""}${e.shiftKey ? ", Shift" : ""}`;
      return;
    }
    unread = null;
    const owner = keymap.actionFor(chord);
    if (owner && owner !== capturing) {
      pending = { id: capturing, chord, owner };
      capturing = null;
      return;
    }
    apply(capturing, chord);
  }

  function clear(id: string) {
    keymap.clear(id);
    notice = null;
    onChange();
  }

  function reset(id: string) {
    keymap.reset(id);
    notice = null;
    onChange();
  }

  function resetAll() {
    keymap.resetAll();
    notice = "All shortcuts are back to their defaults.";
    onChange();
  }
</script>

<svelte:window onkeydowncapture={onKeyCapture} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={onClose}>
  <div
    class="dialog"
    role="dialog"
    tabindex="-1"
    aria-modal="true"
    aria-labelledby="ks-title"
    onclick={(e) => e.stopPropagation()}
  >
    <header>
      <h1 id="ks-title">Keyboard Shortcuts</h1>
      <input
        type="search"
        placeholder="Search commands or keys"
        bind:value={query}
        bind:this={search}
        aria-label="Search commands or keys"
      />
    </header>

    {#if pending}
      <div class="banner warn">
        <span>
          <strong>{pending.chord}</strong> is used by {pathOf.get(pending.owner) ?? pending.owner}. Move it to
          {pathOf.get(pending.id) ?? pending.id}?
        </span>
        <button type="button" onclick={() => pending && apply(pending.id, pending.chord)}>Move</button>
        <button type="button" onclick={() => (pending = null)}>Cancel</button>
      </div>
    {:else if notice}
      <div class="banner">{notice}</div>
    {/if}

    <div class="table" role="table" aria-label="Shortcuts">
      {#each visible as cmd (cmd.id)}
        {@const keys = chords(cmd.id)}
        <div class="row" role="row" class:capturing={capturing === cmd.id}>
          <span class="name" role="cell">
            {cmd.path}
            {#if changed(cmd.id)}<span class="mark" title="Changed from the default">edited</span>{/if}
          </span>
          <span class="keys" role="cell">
            {#if capturing === cmd.id}
              <span class="hint">{unread ? `Could not read that press (${unread}). Try again, or Esc to cancel.` : "Press keys. Esc cancels."}</span>
            {:else if keys.length === 0}
              <span class="none">None</span>
            {:else}
              {#each keys as k (k)}<kbd>{k}</kbd>{/each}
            {/if}
          </span>
          <span class="buttons" role="cell">
            <button type="button" onclick={() => startCapture(cmd.id)}>Change</button>
            <button type="button" disabled={keys.length === 0} onclick={() => clear(cmd.id)}>Clear</button>
            <button type="button" disabled={!changed(cmd.id)} onclick={() => reset(cmd.id)}>Default</button>
          </span>
        </div>
      {/each}
      {#if visible.length === 0}
        <p class="empty">No commands match.</p>
      {/if}
    </div>

    <footer>
      <button type="button" onclick={resetAll}>Reset All to Defaults</button>
      <span class="spacer"></span>
      <button type="button" class="primary" onclick={onClose}>Close</button>
    </footer>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 5000;
    display: grid;
    place-items: center;
    background: rgba(0, 0, 0, 0.5);
  }

  .dialog {
    display: grid;
    grid-template-rows: auto auto 1fr auto;
    width: min(760px, 92vw);
    height: min(600px, 86vh);
    border: 1px solid var(--border-strong);
    background: var(--surface-2);
    box-shadow: 0 16px 40px rgba(0, 0, 0, 0.6);
  }

  header {
    display: flex;
    gap: var(--space-4);
    align-items: center;
    padding: 12px 14px;
    border-bottom: 1px solid var(--border);
  }

  h1 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
  }

  input[type="search"] {
    flex: 1;
    height: 24px;
    padding: 0 8px;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    background: var(--surface-1);
    color: var(--text);
    font: inherit;
  }

  .banner {
    display: flex;
    gap: var(--space-3);
    align-items: center;
    padding: 6px 14px;
    background: var(--surface-3);
    color: var(--text-dim);
  }

  .banner.warn {
    color: var(--text);
  }

  .banner span {
    flex: 1;
  }

  .table {
    overflow-y: auto;
    min-height: 0;
  }

  .row {
    display: grid;
    grid-template-columns: 1fr 190px auto;
    gap: var(--space-3);
    align-items: center;
    min-height: 26px;
    padding: 0 14px;
    border-bottom: 1px solid var(--surface-1);
  }

  .row:hover {
    background: var(--surface-1);
  }

  .row.capturing {
    background: var(--accent-dim);
  }

  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .mark {
    margin-left: 6px;
    color: var(--playhead);
    font-size: var(--font-size-small);
  }

  .keys {
    display: flex;
    gap: 4px;
    flex-wrap: wrap;
  }

  kbd {
    padding: 1px 6px;
    border: 1px solid var(--border-strong);
    border-radius: 3px;
    background: var(--surface-1);
    color: var(--value);
    font-family: var(--font-mono);
    font-size: var(--font-size-small);
  }

  .none,
  .empty {
    color: var(--text-faint);
  }

  .hint {
    color: #fff;
    font-size: var(--font-size-small);
  }

  .empty {
    padding: 14px;
  }

  .buttons {
    display: flex;
    gap: 4px;
  }

  footer {
    display: flex;
    align-items: center;
    padding: 10px 14px;
    border-top: 1px solid var(--border);
  }

  .spacer {
    flex: 1;
  }

  button {
    height: 22px;
    padding: 0 10px;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    background: var(--surface-3);
    color: var(--text);
    font: inherit;
  }

  button:hover:not(:disabled) {
    border-color: var(--accent);
  }

  button:disabled {
    color: var(--text-faint);
  }

  button.primary {
    min-width: 72px;
  }
</style>
