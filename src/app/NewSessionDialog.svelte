<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  File > New Session: name, where it lives, an optional template, and
  the sample rate, bit depth and mix for an empty session. A template
  sets its own rate, depth and mix. Enter confirms, Escape cancels.
-->
<script lang="ts" module>
  export interface NewSessionSettings {
    name: string;
    /** Parent folder; the session gets its own folder inside it. Empty means save later. */
    folder: string;
    template: string | null;
    sampleRate: number;
    bitDepth: number;
    mono: boolean;
  }
</script>

<script lang="ts">
  import type { TemplateEntry } from "./core";
  import { formatRate } from "../panels/files/format";
  import { BIT_DEPTHS, SAMPLE_RATES } from "./session-properties";

  interface Props {
    initial: NewSessionSettings;
    templates: TemplateEntry[];
    onBrowse: (current: string) => Promise<string | null>;
    onDeleteTemplate: (t: TemplateEntry) => Promise<boolean>;
    /** Why the last attempt failed, shown above the buttons. */
    error?: string | null;
    onDone: (value: NewSessionSettings | null) => void;
  }

  const { initial, templates, onBrowse, onDeleteTemplate, error = null, onDone }: Props = $props();

  // svelte-ignore state_referenced_locally
  let name = $state(initial.name);
  // svelte-ignore state_referenced_locally
  let folder = $state(initial.folder);
  // svelte-ignore state_referenced_locally
  let template = $state<string | null>(initial.template && templates.some((t) => t.path === initial.template) ? initial.template : null);
  // svelte-ignore state_referenced_locally
  let sampleRate = $state(initial.sampleRate);
  // svelte-ignore state_referenced_locally
  let bitDepth = $state(initial.bitDepth);
  // svelte-ignore state_referenced_locally
  let mono = $state(initial.mono);
  let input = $state<HTMLInputElement | null>(null);

  const chosen = $derived(templates.find((t) => t.path === template) ?? null);
  const where = $derived(folder.trim() ? `${folder.trim().replace(/[\\/]+$/, "")}${folder.includes("\\") ? "\\" : "/"}${name.trim() || "..."}` : "");

  $effect(() => {
    input?.focus();
    input?.select();
  });

  function done() {
    if (name.trim()) onDone({ name: name.trim(), folder: folder.trim(), template, sampleRate, bitDepth, mono });
  }

  function key(e: KeyboardEvent) {
    e.stopPropagation();
    if (e.key === "Enter" && !(e.target instanceof HTMLSelectElement)) {
      e.preventDefault();
      done();
    } else if (e.key === "Escape") {
      e.preventDefault();
      onDone(null);
    }
  }

  async function browse() {
    const f = await onBrowse(folder);
    if (f) folder = f;
  }

  async function removeTemplate() {
    if (chosen && (await onDeleteTemplate(chosen))) template = null;
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={() => onDone(null)}>
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="new-session-title" tabindex="-1" onclick={(e) => e.stopPropagation()} onkeydown={key}>
    <h1 id="new-session-title">New Session</h1>

    <div class="form">
      <label for="ns-name">Name</label>
      <input id="ns-name" type="text" spellcheck="false" bind:value={name} bind:this={input} />

      <label for="ns-folder">Folder</label>
      <div class="pick">
        <input id="ns-folder" type="text" spellcheck="false" placeholder="Choose later, when you first save" bind:value={folder} />
        <button type="button" onclick={() => void browse()}>Browse...</button>
      </div>

      <label for="ns-template">Template</label>
      <div class="pick">
        <select id="ns-template" bind:value={template}>
          <option value={null}>None (six empty tracks)</option>
          {#each templates as t (t.path)}
            <option value={t.path}>{t.name}</option>
          {/each}
        </select>
        <button type="button" class="icon" title="Delete this template" aria-label="Delete template" disabled={!chosen} onclick={() => void removeTemplate()}>
          <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3 4.5h10M6.5 4.5V3h3v1.5M4.5 4.5l.7 8.5h5.6l.7-8.5" fill="none" stroke="currentColor" stroke-width="1.3" /></svg>
        </button>
      </div>

      <label for="ns-rate">Sample rate</label>
      <select id="ns-rate" bind:value={sampleRate} disabled={!!chosen}>
        {#each SAMPLE_RATES as r (r)}
          <option value={r}>{formatRate(r)}</option>
        {/each}
      </select>

      <label for="ns-depth">Bit depth</label>
      <select id="ns-depth" bind:value={bitDepth} disabled={!!chosen}>
        {#each BIT_DEPTHS as d (d.value)}
          <option value={d.value}>{d.label}</option>
        {/each}
      </select>

      <label for="ns-mix">Mix</label>
      <select id="ns-mix" bind:value={mono} disabled={!!chosen}>
        <option value={false}>Stereo</option>
        <option value={true}>Mono</option>
      </select>
    </div>

    {#if chosen}
      <p class="note">
        {chosen.name}: {chosen.tracks} {chosen.tracks === 1 ? "track" : "tracks"}{chosen.clips ? `, ${chosen.clips} ${chosen.clips === 1 ? "clip" : "clips"}` : ""},
        {formatRate(chosen.sample_rate)}.{chosen.description ? ` ${chosen.description}` : ""} The template sets the rate, depth and mix.
      </p>
    {/if}
    {#if where}
      <p class="note">Saved as {where}</p>
    {:else}
      <p class="note">No folder: the session stays unsaved until you save it.</p>
    {/if}

    {#if error}
      <p class="error" role="alert">{error}</p>
    {/if}
    <footer>
      <button type="button" onclick={() => onDone(null)}>Cancel</button>
      <button type="button" class="primary" disabled={!name.trim()} onclick={done}>OK</button>
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
    gap: 12px;
    width: min(500px, 92vw);
    padding: 14px;
    border: 1px solid var(--border-strong);
    background: var(--surface-2);
    box-shadow: 0 16px 40px rgba(0, 0, 0, 0.6);
  }

  h1 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
  }

  .form {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    gap: 8px 12px;
    align-items: center;
  }

  label {
    color: var(--text-dim);
    text-align: right;
  }

  .pick {
    display: flex;
    gap: 6px;
    min-width: 0;
  }

  .pick > input,
  .pick > select {
    flex: 1;
    min-width: 0;
  }

  input,
  select {
    height: 24px;
    padding: 0 8px;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    background: var(--surface-1);
    color: var(--text);
    font: inherit;
  }

  select:disabled {
    opacity: 0.5;
  }

  input:focus,
  select:focus {
    border-color: var(--accent);
    outline: none;
  }

  .note {
    margin: 0;
    overflow-wrap: anywhere;
    color: var(--text-dim);
    font-size: var(--font-size-small);
  }

  .error {
    margin: 0;
    color: var(--danger);
    font-size: var(--font-size-small);
    overflow-wrap: anywhere;
  }

  footer {
    display: flex;
    gap: 6px;
    justify-content: flex-end;
  }

  button {
    min-width: 72px;
    height: 24px;
    padding: 0 10px;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    background: var(--surface-3);
    color: var(--text);
    font: inherit;
  }

  button.icon {
    display: grid;
    place-items: center;
    min-width: 28px;
    padding: 0;
  }

  button.icon svg {
    width: 14px;
    height: 14px;
  }

  button:hover:not(:disabled) {
    border-color: var(--accent);
  }

  button.primary {
    border-color: var(--accent);
  }

  button:disabled {
    color: var(--text-faint);
  }
</style>
