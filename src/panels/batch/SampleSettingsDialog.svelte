<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  Sample Settings: what each converted file's audio becomes. Sample
  rate, channels (with how much of each side goes into a mono file) and
  bit depth (with dither on or off). "Same as each file" keeps it.
-->
<script lang="ts">
  import { formatRate } from "../files/format";
  import { depthsFor, MP3_MAX_RATE, OUTPUT_RATES, type OutputFormat } from "../../app/output-format";

  interface Props {
    format: OutputFormat;
    onApply: (f: OutputFormat) => void;
    onClose: () => void;
  }

  const { format, onApply, onClose }: Props = $props();
  // svelte-ignore state_referenced_locally
  let f = $state<OutputFormat>({ ...format });
  let moreCh = $state(false);
  let moreDepth = $state(false);
  const depths = $derived(depthsFor(f.kind));
  const pct = (x: number) => Math.round(x * 100);
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onClose()} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={onClose}>
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="ss-title" tabindex="-1" onclick={(e) => e.stopPropagation()}>
    <h1 id="ss-title">Sample Settings</h1>

    <fieldset>
      <legend>Sample rate</legend>
      <label class="row">
        <span>Rate</span>
        <select bind:value={f.sampleRate}>
          <option value={null}>Same as each file</option>
          {#each OUTPUT_RATES as r (r)}
            <option value={r}>{formatRate(r)}{f.kind === "mp3" && r > MP3_MAX_RATE ? " (MP3 uses 48 kHz)" : ""}</option>
          {/each}
        </select>
      </label>
      <p class="note">Rate changes use high-quality resampling with filtering, so nothing folds back as noise.</p>
    </fieldset>

    <fieldset>
      <legend>Channels</legend>
      <label class="row">
        <span>Channels</span>
        <select bind:value={f.channels}>
          <option value={null}>Same as each file</option>
          <option value={1}>Mono</option>
          <option value={2}>Stereo</option>
        </select>
      </label>
      <button type="button" class="more" aria-expanded={moreCh} onclick={() => (moreCh = !moreCh)}>{moreCh ? "▾" : "▸"} Mono mix</button>
      {#if moreCh}
        <label class="row" class:off={f.channels !== 1}>
          <span>Left</span>
          <input type="range" min="0" max="100" step="1" value={pct(f.mixLeft)} disabled={f.channels !== 1} oninput={(e) => (f.mixLeft = Number(e.currentTarget.value) / 100)} />
          <span class="val">{pct(f.mixLeft)}%</span>
        </label>
        <label class="row" class:off={f.channels !== 1}>
          <span>Right</span>
          <input type="range" min="0" max="100" step="1" value={pct(f.mixRight)} disabled={f.channels !== 1} oninput={(e) => (f.mixRight = Number(e.currentTarget.value) / 100)} />
          <span class="val">{pct(f.mixRight)}%</span>
        </label>
        <p class="note">{f.channels === 1 ? "How much of each side goes into the mono file. 50% each keeps the level of centered speech." : "Used when the files become mono."}</p>
      {/if}
    </fieldset>

    <fieldset>
      <legend>Bit depth</legend>
      <label class="row">
        <span>Depth</span>
        <select bind:value={f.bitDepth} disabled={depths.length === 0}>
          <option value={null}>{depths.length === 0 ? "Set by the format" : "Same as each file"}</option>
          {#each depths as d (d.value)}
            <option value={d.value}>{d.label}</option>
          {/each}
        </select>
      </label>
      <button type="button" class="more" aria-expanded={moreDepth} onclick={() => (moreDepth = !moreDepth)}>{moreDepth ? "▾" : "▸"} Dither</button>
      {#if moreDepth}
        <label class="row">
          <span>Dither</span>
          <select value={f.dither ? "on" : "off"} onchange={(e) => (f.dither = e.currentTarget.value === "on")}>
            <option value="on">On (triangular)</option>
            <option value="off">Off</option>
          </select>
        </label>
        <p class="note">Dither adds a whisper of noise when going to fewer bits so quiet fades stay smooth. Leave it on unless the file is going straight back into an editor. 32-bit float never needs it.</p>
      {/if}
    </fieldset>

    <div class="buttons">
      <button type="button" onclick={onClose}>Cancel</button>
      <button type="button" class="primary" onclick={() => onApply($state.snapshot(f))}>OK</button>
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 5100;
    display: grid;
    place-items: center;
    background: rgba(0, 0, 0, 0.5);
  }
  .dialog {
    display: grid;
    gap: 10px;
    width: min(380px, 94vw);
    max-height: 92vh;
    overflow: auto;
    padding: 14px;
    border: 1px solid var(--border-strong);
    background: var(--surface-2);
    box-shadow: 0 16px 40px rgba(0, 0, 0, 0.6);
    font-size: 12px;
  }
  h1 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
  }
  fieldset {
    display: grid;
    gap: 6px;
    margin: 0;
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: 3px;
  }
  legend {
    padding: 0 4px;
    color: var(--text-dim);
  }
  .row {
    display: grid;
    grid-template-columns: 70px minmax(0, 1fr) auto;
    gap: 8px;
    align-items: center;
    color: var(--text-dim);
  }
  .row.off {
    opacity: 0.5;
  }
  .val {
    min-width: 34px;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  select,
  button {
    height: 24px;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    background: var(--surface-1);
    color: var(--text);
    font: inherit;
  }
  .more {
    justify-self: start;
    border: 0;
    background: none;
    color: var(--text-dim);
    padding: 0;
  }
  .note {
    margin: 0;
    color: var(--text-faint);
  }
  .buttons {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
  }
  .buttons button {
    min-width: 72px;
  }
  .primary {
    border-color: var(--accent);
    background: var(--accent);
    color: var(--on-accent, #fff);
  }
</style>
