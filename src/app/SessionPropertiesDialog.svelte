<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  Session name, sample rate and bit depth, plus a read-only summary.
  Enter confirms, Escape cancels.
-->
<script lang="ts">
  import { formatClock } from "../panels/editor/timeline";
  import { formatRate, formatSize } from "../panels/files/format";
  import { sessionEnd, type SessionDoc } from "../stores/session";
  import {
    BIT_DEPTHS,
    currentProperties,
    mixdownBytes,
    SAMPLE_RATES,
    type SessionProperties,
  } from "./session-properties";

  interface Props {
    doc: SessionDoc;
    path: string | null;
    onDone: (value: SessionProperties | null) => void;
  }

  const { doc, path, onDone }: Props = $props();

  // svelte-ignore state_referenced_locally
  const start = currentProperties(doc);
  let name = $state(start.name);
  let sampleRate = $state(start.sampleRate);
  let bitDepth = $state(start.bitDepth);
  let input = $state<HTMLInputElement | null>(null);

  const rates = $derived(
    (SAMPLE_RATES as readonly number[]).includes(start.sampleRate)
      ? [...SAMPLE_RATES]
      : [...SAMPLE_RATES, start.sampleRate].sort((a, b) => a - b),
  );
  const clips = $derived(doc.tracks.reduce((n, t) => n + t.clips.length, 0));
  const length = $derived(sessionEnd(doc));
  const lengthAtNewRate = $derived(Math.round((length * sampleRate) / (doc.sample_rate || sampleRate)));
  const rateChanged = $derived(sampleRate !== doc.sample_rate);

  $effect(() => {
    input?.focus();
    input?.select();
  });

  function done() {
    if (name.trim()) onDone({ name: name.trim(), sampleRate, bitDepth });
  }

  function key(e: KeyboardEvent) {
    e.stopPropagation();
    if (e.key === "Enter") {
      e.preventDefault();
      done();
    } else if (e.key === "Escape") {
      e.preventDefault();
      onDone(null);
    }
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={() => onDone(null)}>
  <div
    class="dialog"
    role="dialog"
    aria-modal="true"
    aria-labelledby="props-title"
    tabindex="-1"
    onclick={(e) => e.stopPropagation()}
    onkeydown={key}
  >
    <h1 id="props-title">Session Properties</h1>
    <label>
      <span>Name</span>
      <input type="text" spellcheck="false" bind:value={name} bind:this={input} />
    </label>
    <div class="row">
      <label>
        <span>Sample rate</span>
        <select bind:value={sampleRate}>
          {#each rates as r (r)}
            <option value={r}>{formatRate(r)}</option>
          {/each}
        </select>
      </label>
      <label>
        <span>Bit depth</span>
        <select bind:value={bitDepth}>
          {#each BIT_DEPTHS as d (d.value)}
            <option value={d.value}>{d.label}</option>
          {/each}
        </select>
      </label>
    </div>
    {#if rateChanged}
      <p class="note">
        Clips keep their place in time. Files at a different rate are converted as they play, so nothing on disk
        changes.
      </p>
    {/if}
    <dl>
      <dt>Tracks</dt>
      <dd>{doc.tracks.length}</dd>
      <dt>Clips</dt>
      <dd>{clips}</dd>
      <dt>Files used</dt>
      <dd>{doc.media.length}</dd>
      <dt>Length</dt>
      <dd>{formatClock(length, doc.sample_rate)}</dd>
      <dt>Stereo mixdown</dt>
      <dd>{formatSize(mixdownBytes(lengthAtNewRate, bitDepth))}</dd>
      <dt>Saved at</dt>
      <dd class="path" title={path ?? ""}>{path ?? "Not saved yet"}</dd>
    </dl>
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
    width: min(420px, 90vw);
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

  .row {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }

  label {
    display: grid;
    gap: 4px;
    color: var(--text-dim);
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

  input:focus,
  select:focus {
    border-color: var(--accent);
    outline: none;
  }

  .note {
    margin: 0;
    color: var(--text-dim);
    font-size: var(--font-size-small);
  }

  dl {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    gap: 2px 12px;
    margin: 0;
    padding-top: 8px;
    border-top: 1px solid var(--border);
    font-size: var(--font-size-small);
  }

  dt {
    color: var(--text-dim);
  }

  dd {
    margin: 0;
    color: var(--value);
    font-family: var(--font-mono);
    text-align: right;
  }

  dd.path {
    overflow: hidden;
    font-family: inherit;
    text-overflow: ellipsis;
    white-space: nowrap;
    direction: rtl;
  }

  footer {
    display: flex;
    gap: 6px;
    justify-content: flex-end;
  }

  button {
    min-width: 72px;
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
</style>
