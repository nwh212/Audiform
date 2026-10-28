<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  Copyright (c) The Audiform contributors.
  This file is part of Audiform, based on Altered Audio Editor.

  File > Publish Package: the episode's title, description and numbers,
  where to put the folder, the loudness target and the audio format.
  Makes one folder holding the audio (chapters inside, checked after
  saving) and the files to copy into a podcast host. Nothing is sent.
-->
<script lang="ts" module>
  import type { OutputFormat } from "./output-format";
  import type { EpisodeInfo } from "./publish";

  export interface PublishSettings {
    episode: EpisodeInfo;
    folder: string;
    format: OutputFormat;
    /** A loudness target id, or "" to leave the level as mixed. */
    normalize: string;
  }
</script>

<script lang="ts">
  import OutputFormatFields from "./OutputFormatFields.svelte";
  import { describeFormat } from "./output-format";
  import { targetsFor } from "./loudness";
  import { DESCRIPTION_MAX, TITLE_MAX, episodeProblems, slugOf } from "./publish";

  interface Props {
    initial: PublishSettings;
    source: { sampleRate: number; channels: number; bitDepth: number };
    chapterCount: number;
    adCount: number;
    onBrowse: (from: string) => Promise<string | null>;
    onDone: (value: PublishSettings | null) => void;
  }

  const { initial, source, chapterCount, adCount, onBrowse, onDone }: Props = $props();

  // svelte-ignore state_referenced_locally
  let s = $state<PublishSettings>({ ...initial, episode: { ...initial.episode }, format: { ...initial.format } });
  let input = $state<HTMLInputElement | null>(null);
  $effect(() => {
    input?.focus();
  });

  const targets = targetsFor();
  const problems = $derived(episodeProblems(s.episode));
  const ok = $derived(problems.length === 0 && s.folder !== "");
  const where = $derived(`${s.folder.replace(/[\\/]+$/, "")}${s.folder.includes("\\") ? "\\" : "/"}${slugOf(s.episode.title)}`);
  const summary = $derived(describeFormat(s.format, source));

  /** An empty box means "not given"; anything else is read as a number. */
  function num(text: string): number | null {
    const t = text.trim();
    return t === "" ? null : Number(t);
  }

  async function browse() {
    const f = await onBrowse(s.folder);
    if (f) s.folder = f;
  }

  function done() {
    if (ok) onDone({ ...s, episode: { ...s.episode, title: s.episode.title.trim() } });
  }

  function key(e: KeyboardEvent) {
    e.stopPropagation();
    if (e.key === "Escape") {
      e.preventDefault();
      onDone(null);
    } else if (e.key === "Enter" && e.target instanceof HTMLInputElement && e.target.type === "text") {
      e.preventDefault();
      done();
    }
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={() => onDone(null)}>
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="pub-title" tabindex="-1" onclick={(e) => e.stopPropagation()} onkeydown={key}>
    <h1 id="pub-title">Publish Package</h1>

    <section>
      <h2>Episode</h2>
      <label class="line">
        <span>Title</span>
        <input type="text" spellcheck="true" maxlength={TITLE_MAX + 50} bind:value={s.episode.title} bind:this={input} />
      </label>
      <label class="top">
        <span>Description</span>
        <textarea rows="4" spellcheck="true" bind:value={s.episode.description}></textarea>
        <span class="count" class:over={s.episode.description.length > DESCRIPTION_MAX}>{s.episode.description.length} of {DESCRIPTION_MAX}</span>
      </label>
      <div class="nums">
        <label>Season <input type="text" inputmode="numeric" size="3" value={s.episode.season ?? ""} oninput={(e) => (s.episode.season = num(e.currentTarget.value))} /></label>
        <label>Episode <input type="text" inputmode="numeric" size="3" value={s.episode.episode ?? ""} oninput={(e) => (s.episode.episode = num(e.currentTarget.value))} /></label>
        <label>
          Type
          <select bind:value={s.episode.type}>
            <option value="full">Full episode</option>
            <option value="trailer">Trailer</option>
            <option value="bonus">Bonus</option>
          </select>
        </label>
        <label class="check"><input type="checkbox" bind:checked={s.episode.explicit} /> Explicit</label>
      </div>
      {#each problems as p (p)}<p class="problem">{p}</p>{/each}
    </section>

    <section>
      <h2>Save to</h2>
      <div class="line">
        <span>Folder</span>
        <span class="folder">
          <input type="text" readonly value={s.folder} title={s.folder} placeholder="Choose a folder" />
          <button type="button" onclick={browse}>Browse…</button>
        </span>
      </div>
      {#if s.folder}<p class="summary">Everything goes into <strong>{where}</strong></p>{/if}
    </section>

    <section>
      <h2>Audio</h2>
      <OutputFormatFields bind:format={s.format} sourceLabel="Session" />
      <p class="summary"><strong>{summary}</strong></p>
      <label class="line" title="Measures the mix, then raises or lowers the whole file by one fixed amount to reach the target. Peaks stay under the target's limit.">
        <span>Loudness</span>
        <select bind:value={s.normalize}>
          <option value="">Leave as mixed</option>
          {#each targets as t (t.id)}<option value={t.id}>Bring to {t.name}</option>{/each}
        </select>
      </label>
      <p class="summary">
        {chapterCount === 0 ? "No chapter markers." : `${chapterCount} ${chapterCount === 1 ? "chapter goes" : "chapters go"} inside the file and into chapters.json.`}
        {adCount === 0 ? "" : `${adCount} ad ${adCount === 1 ? "position is" : "positions are"} listed in episode.json.`}
        The finished file is read back and checked.
      </p>
    </section>

    <footer>
      <button type="button" onclick={() => onDone(null)}>Cancel</button>
      <button type="button" class="primary" disabled={!ok} onclick={done}>Make Package</button>
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
    width: min(560px, 92vw);
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

  .line {
    display: grid;
    grid-template-columns: 72px 1fr;
    gap: 8px;
    align-items: center;
    color: var(--text-dim);
  }

  .top {
    display: grid;
    gap: 4px;
    color: var(--text-dim);
  }

  .count {
    justify-self: end;
    color: var(--text-faint);
    font-size: var(--font-size-small);
  }

  .count.over,
  .problem {
    color: var(--danger);
  }

  .problem {
    margin: 0;
  }

  .nums {
    display: flex;
    flex-wrap: wrap;
    gap: 8px 16px;
    align-items: center;
    color: var(--text-dim);
  }

  .nums label {
    display: flex;
    gap: 6px;
    align-items: center;
  }

  .check {
    color: var(--text);
  }

  .folder {
    display: flex;
    gap: 6px;
    align-items: center;
    min-width: 0;
  }

  input[type="text"],
  textarea,
  select {
    min-width: 0;
    padding: 0 8px;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    background: var(--surface-0);
    color: var(--text);
    font: inherit;
  }

  input[type="text"] {
    flex: 1;
    height: 24px;
  }

  textarea {
    padding: 6px 8px;
    resize: vertical;
  }

  input[readonly] {
    color: var(--text-dim);
  }

  .summary {
    margin: 0;
    color: var(--text-dim);
  }

  .summary strong {
    color: var(--value);
    font-weight: 600;
    word-break: break-all;
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
