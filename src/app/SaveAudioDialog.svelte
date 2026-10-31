<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The Audiform contributors.
  This file is part of Audiform, based on Altered Audio Editor.

  File > Save Audio As (Waveform editor): the file name, where it goes,
  the format and its settings, and whether the markers go in, with the
  sample type and roughly how big the file will be. Enter saves, Escape
  cancels.
-->
<script lang="ts" module>
  import type { OutputFormat } from "./output-format";

  export interface SaveAudioChoice {
    /** The full path, its extension matching the format. */
    path: string;
    format: OutputFormat;
    /** Markers into the file: cue points in WAV, chapters in FLAC, MP3 and M4A. */
    markers: boolean;
  }

  export interface SaveAudioInit {
    name: string;
    folder: string;
    format: OutputFormat;
    markers: boolean;
    seconds: number;
    source: { sampleRate: number; channels: number; bitDepth: number };
    markerCount: number;
  }
</script>

<script lang="ts">
  import OutputFormatFields from "./OutputFormatFields.svelte";
  import { describeFormat, estimateBytes, extensionOf } from "./output-format";

  interface Props {
    init: SaveAudioInit;
    onBrowse: (from: string) => Promise<string | null>;
    onDone: (value: SaveAudioChoice | null) => void;
  }

  const { init, onBrowse, onDone }: Props = $props();

  // svelte-ignore state_referenced_locally
  let name = $state(init.name);
  // svelte-ignore state_referenced_locally
  let folder = $state(init.folder);
  // svelte-ignore state_referenced_locally
  let format = $state<OutputFormat>({ ...init.format });
  // svelte-ignore state_referenced_locally
  let markers = $state(init.markers);
  let input = $state<HTMLInputElement | null>(null);

  /** More than two channels: only WAV keeps them all. */
  const multi = $derived(init.source.channels > 2);
  const ext = $derived(extensionOf(format.kind));
  const summary = $derived(describeFormat(format, init.source));
  const size = $derived.by(() => {
    const b = estimateBytes(format, init.seconds, init.source);
    return b >= 1e9 ? `${(b / 1e9).toFixed(2)} GB` : b >= 1e6 ? `${(b / 1e6).toFixed(1)} MB` : `${Math.max(1, Math.round(b / 1e3))} KB`;
  });
  const stem = $derived(name.trim().replace(/\.(wav|wave|flac|mp3|m4a)$/i, ""));
  const ok = $derived(stem !== "" && folder !== "" && !/[\\/:*?"<>|]/.test(stem));
  const sep = $derived(folder.includes("\\") ? "\\" : "/");

  $effect(() => {
    input?.focus();
    input?.select();
  });

  function done() {
    if (!ok) return;
    const dir = folder.replace(/[\\/]+$/, "");
    onDone({ path: `${dir}${sep}${stem}.${ext}`, format: { ...format }, markers });
  }

  async function browse() {
    const f = await onBrowse(folder);
    if (f) folder = f;
  }

  function key(e: KeyboardEvent) {
    e.stopPropagation();
    if (e.key === "Enter" && !(e.target instanceof HTMLSelectElement) && !(e.target instanceof HTMLButtonElement)) {
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
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="save-audio-title" tabindex="-1" onclick={(e) => e.stopPropagation()} onkeydown={key}>
    <h1 id="save-audio-title">Save Audio As</h1>

    <section>
      <label class="line">
        <span>File name</span>
        <span class="row">
          <input type="text" spellcheck="false" bind:value={name} bind:this={input} />
          <span class="ext">.{ext}</span>
        </span>
      </label>
      <div class="line">
        <span>Location</span>
        <span class="row">
          <input type="text" readonly value={folder} title={folder} placeholder="Choose a folder" />
          <button type="button" onclick={browse}>Browse…</button>
        </span>
      </div>
    </section>

    <section>
      <OutputFormatFields bind:format sourceLabel="Same as the file" only={multi ? ["wav"] : undefined} fixedChannels={multi} />
      <div class="line">
        <span>Sample type</span>
        <strong class="value">{summary}</strong>
      </div>
      {#if multi}<p class="note">This file has {init.source.channels} channels, so it saves as WAV to keep them all.</p>{/if}
    </section>

    <label class="check" class:off={init.markerCount === 0} title="WAV keeps markers as cue points. FLAC, MP3 and M4A take chapter markers as chapters; other markers stay only while the file is open.">
      <input type="checkbox" bind:checked={markers} disabled={init.markerCount === 0} />
      Include markers{init.markerCount > 0 ? ` (${init.markerCount})` : " (none yet)"}
    </label>

    <footer>
      <span class="size">Estimated file size: {size}</span>
      <button type="button" onclick={() => onDone(null)}>Cancel</button>
      <button type="button" class="primary" disabled={!ok} onclick={done}>Save</button>
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
    width: min(460px, 94vw);
    max-height: 92vh;
    overflow: auto;
    padding: 14px 16px;
    border: 1px solid var(--border-strong);
    border-radius: 4px;
    background: var(--surface-2);
    box-shadow: 0 16px 40px rgba(0, 0, 0, 0.6);
  }
  h1 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
  }
  section {
    display: grid;
    gap: 8px;
    margin: 0;
    padding: 10px;
    border: 1px solid var(--border);
    border-radius: 3px;
    background: var(--surface-1);
  }
  .line {
    display: grid;
    grid-template-columns: 76px 1fr;
    gap: 8px;
    align-items: center;
    color: var(--text-dim);
  }
  .row {
    display: flex;
    gap: 6px;
    align-items: center;
    min-width: 0;
  }
  .ext {
    color: var(--text-faint);
    font-family: var(--font-mono);
  }
  input[type="text"] {
    flex: 1;
    min-width: 0;
    height: 24px;
    padding: 0 8px;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    background: var(--surface-0);
    color: var(--text);
    font: inherit;
  }
  input[readonly] {
    color: var(--text-dim);
  }
  .value {
    color: var(--value);
    font-weight: 600;
  }
  .note {
    margin: 0;
    color: var(--text-dim);
    font-size: 11px;
  }
  .check {
    display: flex;
    gap: 6px;
    align-items: center;
    color: var(--text);
  }
  .off {
    color: var(--text-faint);
  }
  footer {
    display: flex;
    gap: 8px;
    align-items: center;
    justify-content: flex-end;
  }
  .size {
    margin-right: auto;
    color: var(--text-dim);
  }
  button {
    height: 26px;
    padding: 0 14px;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    background: var(--surface-3);
    color: var(--text);
    font: inherit;
  }
  button.primary {
    border-color: var(--accent);
    background: var(--accent);
    color: var(--accent-text, #fff);
  }
  button:disabled {
    opacity: 0.5;
  }
</style>
