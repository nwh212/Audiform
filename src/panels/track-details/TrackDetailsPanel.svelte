<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The Audiform contributors.
  This file is part of Audiform, based on Altered Audio Editor.

  Track Details panel: the tags (title, artist, album, year, genre, track
  number, comment) and cover picture of the file picked in the Files
  panel, or else of the selected clip's file. They are read from the
  file and saved back into it the way its format keeps them. A cover is
  shrunk to fit 600 pixels and saved as JPEG. Save writes the file;
  Revert reads it again.
-->
<script lang="ts">
  import { trackDetailsPickCover, trackDetailsRead, trackDetailsWrite, type FileTags } from "../../app/core";
  import { editor, type EditorState } from "../../stores/editor";
  import type { MediaPool, MediaPoolState } from "../../stores/media-pool";
  import type { SessionDoc, SessionStore } from "../../stores/session";

  interface Props {
    pool: MediaPool;
    session: SessionStore;
    onNotice: (text: string) => void;
  }

  const { pool, session, onNotice }: Props = $props();

  let poolState = $state<MediaPoolState | null>(null);
  $effect(() => pool.subscribe((s) => (poolState = s)));
  let doc = $state<SessionDoc | null>(null);
  $effect(() => session.subscribe((d) => (doc = d)));
  let ed = $state<EditorState>(editor.current());
  $effect(() => editor.subscribe((s) => (ed = s)));

  /** The file shown: one picked in Files, else the selected clip's. */
  const path = $derived.by(() => {
    void poolState;
    const picked = pool.selectedItems();
    if (picked.length === 1) return picked[0]!.path;
    if (picked.length > 1) return null;
    for (const t of doc?.tracks ?? [])
      for (const c of t.clips)
        if (ed.selectedClips.has(c.id)) return doc?.media.find((m) => m.id === c.media_id)?.path ?? null;
    return null;
  });

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
  let loading = $state(false);
  let saving = $state(false);
  let dirty = $state(false);
  let error = $state("");
  let shown = $state<string | null>(null);
  const name = $derived(shown?.split(/[\\/]/).pop() ?? "");

  function read(p: string) {
    loading = true;
    error = "";
    dirty = false;
    void trackDetailsRead(p).then(
      (d) => {
        if (shown !== p) return;
        tags = { ...d.tags };
        cover = d.cover;
        writable = d.writable;
        loading = false;
      },
      (e) => {
        if (shown !== p) return;
        error = String(e);
        loading = false;
      },
    );
  }

  $effect(() => {
    const p = path;
    if (p === shown) return;
    shown = p;
    tags = {};
    cover = null;
    if (p) read(p);
  });

  async function pick() {
    try {
      const c = await trackDetailsPickCover();
      if (c) {
        cover = c;
        dirty = true;
      }
    } catch (e) {
      error = String(e);
    }
  }

  async function save() {
    if (!shown) return;
    saving = true;
    error = "";
    try {
      const clean: FileTags = {};
      for (const f of FIELDS) {
        const v = (tags[f.key] ?? "").trim();
        if (v) clean[f.key] = v;
      }
      await trackDetailsWrite(shown, clean, cover);
      dirty = false;
      onNotice(`Saved the details of ${name}.`);
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
    }
  }

  function key(e: KeyboardEvent) {
    if (e.key === "Enter" && e.target instanceof HTMLInputElement && writable && !saving) {
      e.preventDefault();
      void save();
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="panel" onkeydown={key}>
  {#if !shown}
    <p class="note">Pick one file in the Files panel, or select a clip, to see and edit its title, artist, album and cover.</p>
  {:else}
    <h1><span class="file" title={shown}>{name}</span></h1>
    {#if loading}
      <p class="note">Reading…</p>
    {:else}
      <div class="body">
        <div class="art">
          {#if cover}<img src={cover} alt="Cover" />{:else}<div class="empty">No cover</div>{/if}
          <div class="artbuttons">
            <button type="button" disabled={!writable} onclick={() => void pick()}>Choose…</button>
            <button type="button" disabled={!writable || !cover} onclick={() => ((cover = null), (dirty = true))}>Remove</button>
          </div>
        </div>
        <div class="fields">
          {#each FIELDS as f (f.key)}
            <label class:wide={f.wide}>
              <span>{f.label}</span>
              <input type="text" spellcheck="false" disabled={!writable} value={tags[f.key] ?? ""} oninput={(e) => ((tags = { ...tags, [f.key]: e.currentTarget.value }), (dirty = true))} />
            </label>
          {/each}
        </div>
      </div>
      {#if !writable}<p class="note">Details are saved into WAV, FLAC, MP3 and M4A files. This file's are shown only.</p>{/if}
      <p class="note">Saving writes into the file itself; it is not undone with Undo. Covers are shrunk to fit 600 pixels and stored as JPEG.</p>
    {/if}
    {#if error}<p class="error">{error}</p>{/if}
    <footer>
      <button type="button" disabled={!dirty || saving} onclick={() => shown && read(shown)}>Revert</button>
      <button type="button" class="primary" disabled={loading || !writable || saving || !dirty} onclick={() => void save()}>{saving ? "Saving…" : "Save"}</button>
    </footer>
  {/if}
</div>

<style>
  .panel {
    display: grid;
    gap: 10px;
    align-content: start;
    height: 100%;
    padding: 10px;
    overflow: auto;
    box-sizing: border-box;
    container-type: inline-size;
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
  @container (max-width: 420px) {
    .body {
      grid-template-columns: 1fr;
    }
  }
</style>
