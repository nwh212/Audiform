<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The Audiform contributors.
  This file is part of Audiform, based on Altered Audio Editor.

  Track Details: a file's tags (title, artist, album, year, genre, track
  number, comment) and its cover picture, read from the file and saved
  back into it the way its format keeps them. A cover is shrunk to fit
  600 pixels and saved as JPEG. Saving writes the file; Cancel leaves it.
-->
<script lang="ts">
  import { trackDetailsPickCover, trackDetailsRead, trackDetailsWrite, type FileTags } from "./core";

  interface Props {
    path: string;
    onNotice: (text: string) => void;
    onDone: () => void;
  }

  const { path, onNotice, onDone }: Props = $props();

  const FIELDS: { key: keyof FileTags; label: string; wide?: boolean }[] = [
    { key: "title", label: "Title", wide: true },
    { key: "artist", label: "Artist", wide: true },
    { key: "album", label: "Album", wide: true },
    { key: "year", label: "Year" },
    { key: "track", label: "Track" },
    { key: "genre", label: "Genre", wide: true },
    { key: "comment", label: "Comment", wide: true },
  ];

  let tags = $state<FileTags>({});
  let cover = $state<string | null>(null);
  let writable = $state(true);
  let loading = $state(true);
  let saving = $state(false);
  let error = $state("");
  const name = $derived(path.split(/[\\/]/).pop() ?? path);

  $effect(() => {
    void trackDetailsRead(path).then(
      (d) => {
        tags = { ...d.tags };
        cover = d.cover;
        writable = d.writable;
        loading = false;
      },
      (e) => {
        error = String(e);
        loading = false;
      },
    );
  });

  async function pick() {
    try {
      const c = await trackDetailsPickCover();
      if (c) cover = c;
    } catch (e) {
      error = String(e);
    }
  }

  async function save() {
    saving = true;
    error = "";
    try {
      const clean: FileTags = {};
      for (const f of FIELDS) {
        const v = (tags[f.key] ?? "").trim();
        if (v) clean[f.key] = v;
      }
      await trackDetailsWrite(path, clean, cover);
      onNotice(`Saved the details of ${name}.`);
      onDone();
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
    }
  }

  function key(e: KeyboardEvent) {
    e.stopPropagation();
    if (e.key === "Escape") {
      e.preventDefault();
      onDone();
    } else if (e.key === "Enter" && e.target instanceof HTMLInputElement && writable && !saving) {
      e.preventDefault();
      void save();
    }
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={onDone}>
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="details-title" tabindex="-1" onclick={(e) => e.stopPropagation()} onkeydown={key}>
    <h1 id="details-title">Track Details <span class="file" title={path}>{name}</span></h1>
    {#if loading}
      <p class="note">Reading…</p>
    {:else}
      <div class="body">
        <div class="art">
          {#if cover}<img src={cover} alt="Cover" />{:else}<div class="empty">No cover</div>{/if}
          <div class="artbuttons">
            <button type="button" disabled={!writable} onclick={() => void pick()}>Choose…</button>
            <button type="button" disabled={!writable || !cover} onclick={() => (cover = null)}>Remove</button>
          </div>
        </div>
        <div class="fields">
          {#each FIELDS as f (f.key)}
            <label class:wide={f.wide}>
              <span>{f.label}</span>
              <input type="text" spellcheck="false" disabled={!writable} value={tags[f.key] ?? ""} oninput={(e) => (tags = { ...tags, [f.key]: e.currentTarget.value })} />
            </label>
          {/each}
        </div>
      </div>
      {#if !writable}<p class="note">Details are saved into WAV, FLAC, MP3 and M4A files. This file's are shown only.</p>{/if}
      <p class="note">Saving writes into the file itself; it is not undone with Undo. Covers are shrunk to fit 600 pixels and stored as JPEG.</p>
    {/if}
    {#if error}<p class="error">{error}</p>{/if}
    <footer>
      <button type="button" onclick={onDone}>Cancel</button>
      <button type="button" class="primary" disabled={loading || !writable || saving} onclick={() => void save()}>{saving ? "Saving…" : "Save"}</button>
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
    width: min(560px, 94vw);
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
  .file {
    margin-left: 6px;
    color: var(--text-dim);
    font-weight: 400;
  }
  .body {
    display: grid;
    grid-template-columns: 170px 1fr;
    gap: 14px;
  }
  .art {
    display: grid;
    gap: 6px;
    align-content: start;
  }
  .art img,
  .empty {
    width: 170px;
    height: 170px;
    object-fit: cover;
    border: 1px solid var(--border-strong);
    border-radius: 4px;
    background: var(--surface-1);
  }
  .empty {
    display: grid;
    place-items: center;
    color: var(--text-faint);
  }
  .artbuttons {
    display: flex;
    gap: 6px;
  }
  .fields {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 6px 10px;
    align-content: start;
  }
  label {
    display: grid;
    gap: 2px;
    color: var(--text-dim);
    font-size: 11px;
  }
  label.wide {
    grid-column: span 2;
  }
  input {
    height: 24px;
    padding: 0 8px;
    border: 1px solid var(--border-strong);
    background: var(--surface-1);
    color: var(--text);
    font: inherit;
    font-size: 12px;
  }
  input:focus {
    border-color: var(--accent);
    outline: none;
  }
  .note {
    margin: 0;
    color: var(--text-faint);
    font-size: 11px;
  }
  .error {
    margin: 0;
    color: var(--danger);
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
  @media (max-width: 520px) {
    .body {
      grid-template-columns: 1fr;
    }
  }
</style>
