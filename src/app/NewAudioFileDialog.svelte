<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  File > New Audio File: name, sample rate, channels and bit depth for
  an empty file in the Waveform editor. Enter confirms, Escape cancels.
-->
<script lang="ts" module>
  export interface NewAudioSettings {
    name: string;
    sampleRate: number;
    channels: 1 | 2;
    bitDepth: number;
  }
</script>

<script lang="ts">
  import { formatRate } from "../panels/files/format";
  import { BIT_DEPTHS, SAMPLE_RATES } from "./session-properties";

  interface Props {
    initial: NewAudioSettings;
    onDone: (value: NewAudioSettings | null) => void;
  }

  const { initial, onDone }: Props = $props();

  // svelte-ignore state_referenced_locally
  let name = $state(initial.name);
  // svelte-ignore state_referenced_locally
  let sampleRate = $state(initial.sampleRate);
  // svelte-ignore state_referenced_locally
  let channels = $state<1 | 2>(initial.channels);
  // svelte-ignore state_referenced_locally
  let bitDepth = $state(initial.bitDepth);
  let input = $state<HTMLInputElement | null>(null);

  $effect(() => {
    input?.focus();
    input?.select();
  });

  function done() {
    if (name.trim()) onDone({ name: name.trim(), sampleRate, channels, bitDepth });
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
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="new-audio-title" tabindex="-1" onclick={(e) => e.stopPropagation()} onkeydown={key}>
    <h1 id="new-audio-title">New Audio File</h1>
    <label>
      <span>Name</span>
      <input type="text" spellcheck="false" bind:value={name} bind:this={input} />
    </label>
    <div class="row">
      <label>
        <span>Sample rate</span>
        <select bind:value={sampleRate}>
          {#each SAMPLE_RATES as r (r)}
            <option value={r}>{formatRate(r)}</option>
          {/each}
        </select>
      </label>
      <label>
        <span>Channels</span>
        <select bind:value={channels}>
          <option value={1}>Mono</option>
          <option value={2}>Stereo</option>
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
    <p class="note">The file starts empty. Paste audio into it, then use Save Audio As to choose where it goes.</p>
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
    width: min(440px, 90vw);
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
    grid-template-columns: 1fr 1fr 1fr;
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

  button.primary {
    border-color: var(--accent);
  }

  button:disabled {
    color: var(--text-faint);
  }
</style>
