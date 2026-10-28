<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  File > Export > Mixdown: what to mix, the file name and folder, and
  the format. Shows what the file will be and roughly how big before
  anything is written. Enter exports, Escape cancels.
-->
<script lang="ts" module>
  import type { OutputFormat } from "./output-format";

  export type ExportScope = "all" | "selection" | "clips";

  export interface ExportSettings {
    scope: ExportScope;
    name: string;
    folder: string;
    format: OutputFormat;
    addToFiles: boolean;
    /** Write chapter markers into the file (WAV, FLAC, MP3 and M4A). */
    chapters: boolean;
    /** Read the file back after saving and check it. */
    verify: boolean;
    /** A loudness target to bring the file to (an id from Check Loudness); "" leaves it as mixed. */
    normalize: string;
    /** 3D Space sessions: a speaker layout ("5.1", "7.1.4"...) or "binaural". */
    speakers?: string;
  }

  export interface ExportScopeInfo {
    scope: ExportScope;
    label: string;
    /** Length in seconds, or null when unavailable. */
    seconds: number | null;
    /** Why it's unavailable. */
    reason: string;
  }
</script>

<script lang="ts">
  import OutputFormatFields from "./OutputFormatFields.svelte";
  import { describeFormat, estimateBytes, extensionOf } from "./output-format";
  import { SPACE_LAYOUTS, type SpaceLayout } from "../stores/session";
  import { targetsFor } from "./loudness";
  import { LAYOUT_NAMES } from "../panels/space/space";

  interface Props {
    initial: ExportSettings;
    scopes: ExportScopeInfo[];
    source: { sampleRate: number; channels: number; bitDepth: number };
    onBrowse: (from: string) => Promise<string | null>;
    /** Chapter markers in the session. */
    chapterCount?: number;
    /** 3D Space sessions: the session's speaker layout, to offer every layout and binaural. */
    spaceLayout?: SpaceLayout | null;
    onDone: (value: ExportSettings | null) => void;
  }

  const { initial, scopes, source, onBrowse, chapterCount = 0, spaceLayout = null, onDone }: Props = $props();

  // svelte-ignore state_referenced_locally
  let s = $state<ExportSettings>({ ...initial, format: { ...initial.format } });
  let input = $state<HTMLInputElement | null>(null);

  $effect(() => {
    input?.focus();
    input?.select();
  });

  const current = $derived(scopes.find((x) => x.scope === s.scope));
  /** A layout with more than two speakers is written as a WAV with one channel per speaker. */
  const multi = $derived(!!spaceLayout && s.speakers !== undefined && s.speakers !== "binaural" && s.speakers !== "2.0");
  $effect(() => {
    if (multi && s.format.kind !== "wav") s.format = { ...s.format, kind: "wav" };
  });
  const targets = targetsFor();
  const ext = $derived(extensionOf(s.format.kind));
  const canChapter = $derived(["wav", "flac", "mp3", "m4a"].includes(s.format.kind));
  const summary = $derived(describeFormat(s.format, source));
  const size = $derived.by(() => {
    const secs = current?.seconds;
    if (secs === null || secs === undefined) return "";
    const b = estimateBytes(s.format, secs, source);
    return b >= 1e9 ? `${(b / 1e9).toFixed(2)} GB` : b >= 1e6 ? `${(b / 1e6).toFixed(1)} MB` : `${Math.max(1, Math.round(b / 1e3))} KB`;
  });
  const ok = $derived(s.name.trim() !== "" && s.folder !== "" && (current?.seconds ?? null) !== null);

  function done() {
    if (ok) onDone({ ...s, name: s.name.trim().replace(/\.(wav|wave|flac|mp3)$/i, "") });
  }

  async function browse() {
    const f = await onBrowse(s.folder);
    if (f) s.folder = f;
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
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="export-title" tabindex="-1" onclick={(e) => e.stopPropagation()} onkeydown={key}>
    <h1 id="export-title">Export Mixdown</h1>

    <section aria-labelledby="export-mix">
      <h2 id="export-mix">Mix</h2>
      <div class="scopes" role="radiogroup" aria-labelledby="export-mix">
      {#each scopes as x (x.scope)}
        <label class="radio" class:off={x.seconds === null} title={x.seconds === null ? x.reason : ""}>
          <input type="radio" name="scope" value={x.scope} bind:group={s.scope} disabled={x.seconds === null} />
          {x.label}
        </label>
      {/each}
      </div>
    </section>

    <section>
      <h2>Save as</h2>
      <label class="line">
        <span>Name</span>
        <span class="name">
          <input type="text" spellcheck="false" bind:value={s.name} bind:this={input} />
          <span class="ext">.{ext}</span>
        </span>
      </label>
      <div class="line">
        <span>Folder</span>
        <span class="folder">
          <input type="text" readonly value={s.folder} title={s.folder} placeholder="Choose a folder" />
          <button type="button" onclick={browse}>Browse…</button>
        </span>
      </div>
    </section>

    {#if spaceLayout}
      <section>
        <h2>Speakers</h2>
        <label class="line" title="The same session plays in any layout: pick one for this file. Layouts with more than two speakers are written as WAV with one channel per speaker, labeled for other software. Headphones writes a stereo binaural mix.">
          <span>Layout</span>
          <select bind:value={s.speakers}>
            {#each SPACE_LAYOUTS as l (l)}<option value={l}>{LAYOUT_NAMES[l]}{l === spaceLayout ? " (the session's)" : ""}</option>{/each}
            <option value="binaural">Headphones (binaural stereo)</option>
          </select>
        </label>
        {#if multi}<p class="summary">Written as a WAV with one channel per speaker.</p>{/if}
      </section>
    {/if}

    <section>
      <h2>Audio</h2>
      <OutputFormatFields bind:format={s.format} sourceLabel="Session" />
      <p class="summary">
        <strong>{summary}</strong>{#if size}<span>&nbsp;· about {size}</span>{/if}
      </p>
      <label class="line" title="Measures the mix first, then raises or lowers the whole file by one fixed amount to reach the target. Peaks stay under the target's limit, so a mix with high peaks can end up a little quieter than the target. The session itself is not changed. Mono targets are measured when the session mixes to mono.">
        <span>Loudness</span>
        <select bind:value={s.normalize} disabled={multi}>
          <option value="">Leave as mixed</option>
          {#each targets as t (t.id)}<option value={t.id}>Bring to {t.name}</option>{/each}
        </select>
      </label>
    </section>

    <section class="options">
      <label class="check"><input type="checkbox" bind:checked={s.addToFiles} /> Add the file to the Files panel</label>
      <label class="check" title="Reads the finished file back from start to end: it opens, every sample decodes, it is the right length and the chapters are in it."><input type="checkbox" bind:checked={s.verify} /> Check the file after saving</label>
      {#if chapterCount > 0}
        <label class="check" class:off={!canChapter} title={canChapter ? "" : "Chapters go into WAV, FLAC, MP3 and M4A files. For other formats, use File > Export > Markers and Chapters."}>
          <input type="checkbox" bind:checked={s.chapters} disabled={!canChapter} /> Add the {chapterCount} chapter {chapterCount === 1 ? "marker" : "markers"} to the file
        </label>
      {:else}
        <label class="check off" title="Set a marker's type to Chapter, or use Edit > Markers > Add Chapter Marker.">
          <input type="checkbox" disabled /> Add chapters to the file (no chapter markers yet)
        </label>
      {/if}
    </section>

    <footer>
      <button type="button" onclick={() => onDone(null)}>Cancel</button>
      <button type="button" class="primary" disabled={!ok} onclick={done}>Export</button>
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
    width: min(520px, 92vw);
    max-height: 92vh;
    overflow: auto;
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

  h2 {
    margin: 0;
    padding: 0;
    color: var(--text-faint);
    font-size: var(--font-size-small);
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
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

  .scopes {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 18px;
  }

  .line {
    display: grid;
    grid-template-columns: 56px 1fr;
    gap: 8px;
    align-items: center;
    color: var(--text-dim);
  }

  .name,
  .folder {
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

  .summary {
    margin: 2px 0 0;
    color: var(--text-dim);
  }

  .summary strong {
    color: var(--value);
    font-weight: 600;
  }

  .radio,
  .check {
    display: flex;
    gap: 6px;
    align-items: center;
    color: var(--text);
  }

  .off {
    color: var(--text-faint);
  }

  .options {
    gap: 6px;
  }

  footer {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
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
