<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  Generate tools. Tone and Noise make a new audio file and place it as
  a clip at the playhead on the selected track. Insert Silence makes
  room at the playhead: everything after it moves later on every track
  (or only the selected one), and a clip under the playhead is split.
-->
<script lang="ts">
  import { parseSeconds } from "../panels/editor/stretch";

  export type GenerateMode = "tone" | "noise" | "silence";
  export interface GenerateValues {
    seconds: number;
    frequency: number;
    levelDb: number;
    noise: "white" | "pink";
    channels: 1 | 2;
    allTracks: boolean;
    /** Insert Silence with a time selection: silence that range in place instead of pushing audio later. */
    overwrite: boolean;
  }

  interface Props {
    mode: GenerateMode;
    initial: GenerateValues;
    trackName: string | null;
    /** Seconds of the time selection, when there is one. */
    selection?: number | null;
    onApply: (v: GenerateValues) => void;
    onClose: () => void;
  }

  const { mode, initial, trackName, selection = null, onApply, onClose }: Props = $props();
  // svelte-ignore state_referenced_locally
  let v = $state<GenerateValues>({ ...initial });
  // svelte-ignore state_referenced_locally
  let lengthText = $state(String(initial.seconds));
  // svelte-ignore state_referenced_locally
  if (mode === "silence" && selection !== null) v.overwrite = true;
  const replacing = $derived(mode === "silence" && selection !== null && v.overwrite);
  const secs = $derived(parseSeconds(lengthText));
  const title = $derived(mode === "tone" ? "Generate Tone" : mode === "noise" ? "Generate Noise" : "Insert Silence");
  const ok = $derived(replacing || secs !== null && secs <= 3600 && (mode !== "tone" || (v.frequency >= 1 && v.frequency <= 20_000)));
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onClose()} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={onClose}>
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="gen-title" tabindex="-1" onclick={(e) => e.stopPropagation()}>
    <h1 id="gen-title">{title}</h1>
    <p class="note">
      {#if mode === "silence" && replacing}
        Replaces the time selection with silence. Nothing moves; audio in the selection is removed.
      {:else if mode === "silence"}
        Makes room at the playhead. Clips after it move later; a clip under the playhead is split.
      {:else}
        Creates a new audio file in the session's Generated folder and places it at the playhead{trackName ? ` on ${trackName}` : ""}.
      {/if}
    </p>
    {#if mode === "silence" && selection !== null}
      <fieldset>
        <label><input type="radio" name="how" checked={v.overwrite} onchange={() => (v.overwrite = true)} /> Silence the time selection ({selection.toFixed(2)} s), moving nothing</label>
        <label><input type="radio" name="how" checked={!v.overwrite} onchange={() => (v.overwrite = false)} /> Insert new silence at the playhead, pushing audio later</label>
      </fieldset>
    {/if}
    {#if !replacing}
      <label class="row"><span>Length</span><input type="text" bind:value={lengthText} aria-label="Length in seconds or minutes:seconds" /> <span class="unit">seconds, or m:ss</span></label>
    {/if}
    {#if mode === "tone"}
      <label class="row"><span>Frequency</span><input type="number" min="1" max="20000" step="1" bind:value={v.frequency} /> <span class="unit">Hz</span></label>
    {/if}
    {#if mode === "noise"}
      <label class="row"><span>Type</span>
        <select bind:value={v.noise}>
          <option value="pink">Pink (softer, like room tone)</option>
          <option value="white">White (even, bright)</option>
        </select>
      </label>
    {/if}
    {#if mode !== "silence"}
      <label class="row"><span>Level</span><input type="number" min="-96" max="0" step="1" bind:value={v.levelDb} /> <span class="unit">dBFS {mode === "tone" ? "peak" : "average"}</span></label>
      <label class="row"><span>Channels</span>
        <select bind:value={v.channels}>
          <option value={1}>Mono</option>
          <option value={2}>Stereo</option>
        </select>
      </label>
    {:else}
      <fieldset>
        <label><input type="radio" name="scope" checked={v.allTracks} onchange={() => (v.allTracks = true)} /> On every track{replacing ? "" : " (keeps them in sync; markers move too)"}</label>
        <label><input type="radio" name="scope" checked={!v.allTracks} disabled={!trackName} onchange={() => (v.allTracks = false)} /> Only on {trackName ?? "the selected track (select one first)"}</label>
      </fieldset>
    {/if}
    {#if secs === null && !replacing}<p class="warn">Type a length like 5, 2.5 or 1:30.</p>{/if}
    <footer>
      <button type="button" class="primary" disabled={!ok} onclick={() => (replacing ? onApply({ ...v, seconds: selection ?? 0 }) : secs !== null && onApply({ ...v, seconds: secs }))}>{replacing ? "Silence" : mode === "silence" ? "Insert" : "Generate"}</button>
      <button type="button" onclick={onClose}>Cancel</button>
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
    gap: 10px;
    width: min(420px, 94vw);
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
  .note,
  .unit {
    margin: 0;
    color: var(--text-dim);
  }
  .warn {
    margin: 0;
    color: var(--danger);
  }
  .row {
    display: grid;
    grid-template-columns: 76px 110px auto;
    gap: 8px;
    align-items: center;
    color: var(--text-dim);
  }
  input,
  select {
    height: 24px;
    padding: 0 6px;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    background: var(--surface-1);
    color: var(--text);
    font: inherit;
  }
  select {
    grid-column: span 2;
  }
  fieldset {
    display: grid;
    gap: 4px;
    margin: 0;
    padding: 6px 8px;
    border: 1px solid var(--border);
  }
  footer {
    display: flex;
    gap: 6px;
    justify-content: flex-end;
  }
  .primary {
    border-color: var(--accent);
  }
</style>
