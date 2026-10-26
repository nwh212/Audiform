<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  File > Export > Markers and Chapters: writes the session's markers as
  a timestamp list, a CSV spreadsheet or podcast chapters JSON, with a
  preview and warnings about lists a site or app would reject.
-->
<script lang="ts">
  import { CHAPTER_FORMATS, chaptersOf, render, warnings, type ChapterFormat } from "../panels/markers/chapters";
  import type { MarkerDoc } from "../stores/session";

  interface Props {
    markers: readonly MarkerDoc[];
    rate: number;
    format: ChapterFormat;
    onFormat: (f: ChapterFormat) => void;
    onSave: (content: string, format: ChapterFormat) => void;
    onClose: () => void;
  }

  const { markers, rate, format, onFormat, onSave, onClose }: Props = $props();

  const hasChapters = $derived(markers.some((m) => m.kind === "chapter"));
  // svelte-ignore state_referenced_locally
  let which = $state<"chapters" | "all">(markers.some((m) => m.kind === "chapter") ? "chapters" : "all");
  const list = $derived(chaptersOf(markers, rate, which));
  const text = $derived(render(list, format));
  const issues = $derived(warnings(list, format));
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onClose()} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={onClose}>
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="em-title" tabindex="-1" onclick={(e) => e.stopPropagation()}>
    <h1 id="em-title">Export Markers and Chapters</h1>
    <label class="row">
      <span>Format</span>
      <select value={format} onchange={(e) => onFormat(e.currentTarget.value as ChapterFormat)}>
        {#each CHAPTER_FORMATS as f (f.id)}<option value={f.id}>{f.label}</option>{/each}
      </select>
    </label>
    <fieldset>
      <label><input type="radio" name="which" value="chapters" checked={which === "chapters"} disabled={!hasChapters} onchange={() => (which = "chapters")} /> Chapter markers only{hasChapters ? "" : " (none yet: set a marker's type to Chapter in the Markers panel)"}</label>
      <label><input type="radio" name="which" value="all" checked={which === "all"} onchange={() => (which = "all")} /> Every marker</label>
    </fieldset>
    <textarea readonly rows="10" aria-label="Preview">{text}</textarea>
    {#each issues as w (w)}<p class="warn">{w}</p>{/each}
    <footer>
      <button type="button" class="primary" disabled={list.length === 0} onclick={() => onSave(text, format)}>Save...</button>
      <button type="button" onclick={onClose}>Close</button>
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
    width: min(520px, 94vw);
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
    grid-template-columns: 60px minmax(0, 1fr);
    gap: 8px;
    align-items: center;
    color: var(--text-dim);
  }
  select {
    height: 24px;
    padding: 0 6px;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    background: var(--surface-1);
    color: var(--text);
  }
  fieldset {
    display: grid;
    gap: 4px;
    margin: 0;
    padding: 6px 8px;
    border: 1px solid var(--border);
  }
  textarea {
    resize: vertical;
    padding: 6px;
    border: 1px solid var(--border);
    background: var(--surface-0);
    color: var(--text);
    font-family: var(--font-mono, monospace);
    font-size: 11px;
  }
  .warn {
    margin: 0;
    color: var(--danger);
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
