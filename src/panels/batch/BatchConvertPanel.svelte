<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  Batch Convert: a list of files, an optional FX Chain preset run on
  each one, and one set of output settings (in the Output Settings
  dialog), turned into new files one after another. Add files with the
  buttons, from the Files panel's selection, or by dropping them on the
  panel. Rows show each file's status and stage; a file that fails says
  why and the rest carry on. Settings are remembered between runs.
-->
<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { onDestroy } from "svelte";
  import { batchCancel, batchConvert, effectsCatalog, expandMediaPaths, exportPickFolder, pickMediaFiles, pickMediaFolder, unzipMedia, type BatchProgress } from "../../app/core";
  import type { FileDropRouter } from "../../app/filedrop";
  import type { MediaPool } from "../../stores/media-pool";
  import type { Preferences } from "../../stores/preferences";
  import { batchPresets, BUILTIN_PRESETS, presetChain, sanitizePresets, type ChainPreset } from "../voice-chain/chain";
  import { addRows, batchRows, conflicts, destFor, effectiveTemplate, fileName, sanitizeBatch, sanitizeLevel, shapingFor, tagsFor, type BatchRow, type BatchSettings } from "./batch";
  import BatchOutputDialog from "./BatchOutputDialog.svelte";

  interface Props {
    pool: MediaPool;
    drop: FileDropRouter;
    prefs: Preferences;
    onNotice: (text: string) => void;
    onError: (e: unknown) => void;
  }

  const { pool, drop, prefs, onNotice, onError }: Props = $props();

  const KEY = "batch.settings";
  let s = $state<BatchSettings>(sanitizeBatch(null));
  let loaded = $state(false);
  // Settings save once the saved ones are in, so defaults never overwrite them.
  // svelte-ignore state_referenced_locally
  void prefs.load().then(() => {
    s = sanitizeBatch(prefs.raw(KEY));
    loaded = true;
  });
  $effect(() => {
    const snap = $state.snapshot(s);
    if (loaded) prefs.set(KEY, snap);
  });

  let rows = $state<BatchRow[]>([]);
  const unsub = batchRows.subscribe((v) => (rows = v));
  onDestroy(unsub);
  const setRows = (next: BatchRow[]) => batchRows.set(next);

  let selected = $state<Set<string>>(new Set());
  let running = $state(false);
  let root = $state<HTMLElement | null>(null);
  let dropping = $state(false);
  let poolCount = $state(0);
  $effect(() =>
    pool.subscribe((p) => {
      poolCount = p.selected.size;
    }),
  );

  $effect(() => {
    if (!root) return;
    return drop.register({ element: root, onDrop: (paths) => void add(paths), onHover: (over) => (dropping = over) });
  });

  const today = new Date();
  const template = $derived(effectiveTemplate(s));
  const dests = $derived(
    new Map(rows.map((r, i) => [r.id, destFor(r.source, { template, folder: s.folder, kind: s.format.kind }, { number: i + 1, count: rows.length, date: today })])),
  );

  // Processing presets: the user's FX Chain presets and the built-in ones.
  let userPresets = $state<ChainPreset[]>([]);
  // svelte-ignore state_referenced_locally
  void prefs.load().then(() => (userPresets = sanitizePresets(prefs.raw("voiceChain.presets"))));
  let known = $state<Set<string> | null>(null);
  void effectsCatalog()
    .then((c) => (known = new Set(c.map((e) => e.id))))
    .catch(() => (known = null));
  const allPresets = $derived([...userPresets, ...BUILTIN_PRESETS]);
  const GROUPS = ["Voice chains", "Processing", "Breath gates", "Repair", "EQ"];
  const BATCH_PRESETS = batchPresets(BUILTIN_PRESETS);
  let outputOpen = $state(false);
  const problems = $derived(conflicts(rows, dests, s.overwrite));
  const waiting = $derived(rows.filter((r) => r.status !== "done" && !problems.has(r.id)));
  const outputSummary = $derived.by(() => {
    const f = s.format;
    const kind = f.kind === "mp3" ? (f.mp3Mode === "cbr" ? `MP3 ${f.mp3Bitrate} kbps` : `MP3 VBR q${f.mp3Quality}`) : f.kind === "m4a" ? `AAC ${f.aacBitrate} kbps` : f.kind.toUpperCase();
    const where = s.folder ? `into ${fileName(s.folder) || s.folder}` : "next to each original";
    return `${kind}, ${where}`;
  });
  const exampleStem = $derived.by(() => {
    const n = rows[0] ? fileName(rows[0].source) : "Episode 12.wav";
    const dot = n.lastIndexOf(".");
    return dot > 0 ? n.slice(0, dot) : n;
  });

  async function add(paths: string[]) {
    if (paths.length === 0) return;
    try {
      const plain: string[] = [];
      for (const p of paths) {
        if (/\.zip$/i.test(p)) plain.push(...(await unzipMedia(p)));
        else plain.push(p);
      }
      const files = await expandMediaPaths(plain);
      const r = addRows(rows, files, () => crypto.randomUUID());
      setRows(r.rows);
      if (r.added < files.length) onNotice(`${files.length - r.added} already in the list.`);
    } catch (e) {
      onError(e);
    }
  }

  const addFiles = async () => add(await pickMediaFiles());
  async function addFolder() {
    const f = await pickMediaFolder();
    if (f) await add([f]);
  }
  const addFromFiles = () => add(pool.selectedItems().map((it) => it.path));

  function remove(ids: Set<string>) {
    setRows(rows.filter((r) => !ids.has(r.id)));
    selected = new Set();
  }

  function pick(e: MouseEvent, id: string) {
    const next = new Set(e.ctrlKey || e.metaKey ? selected : []);
    if (next.has(id) && (e.ctrlKey || e.metaKey)) next.delete(id);
    else next.add(id);
    selected = next;
  }

  function patch(id: string, p: Partial<BatchRow>) {
    setRows(rows.map((r) => (r.id === id ? { ...r, ...p } : r)));
  }

  async function run() {
    if (running || waiting.length === 0) return;
    running = true;
    const jobs = waiting.map((r) => ({
      id: r.id,
      source: r.source,
      dest: dests.get(r.id)!,
      tags: tagsFor(s.tags, r.source, s.format.kind, { number: rows.indexOf(r) + 1, count: rows.length, date: today }),
    }));
    setRows(rows.map((r) => (jobs.some((j) => j.id === r.id) ? { ...r, status: "waiting", progress: 0, error: null, stage: "Waiting" } : r)));
    const preset = s.process ? allPresets.find((p) => p.name === s.process) : undefined;
    if (s.process && !preset) {
      running = false;
      onNotice(`The preset "${s.process}" is gone. Pick another, or None.`);
      return;
    }
    const process = preset ? presetChain(preset, known ?? new Set(preset.chain.slots.map((x) => x.effect)), () => crypto.randomUUID()) : null;
    const off = await listen<BatchProgress>("batch-progress", (e) => {
      const p = e.payload;
      patch(p.id, { status: p.status, progress: p.progress, error: p.error, stage: p.stage });
    });
    try {
      const r = await batchConvert(jobs, $state.snapshot(s.format), s.overwrite, s.copyTags, process, shapingFor(s.level));
      if (s.removeDone) setRows(rows.filter((x) => x.status !== "done"));
      const parts = [`Converted ${r.done} ${r.done === 1 ? "file" : "files"}`];
      if (r.failed) parts.push(`${r.failed} failed`);
      if (r.stopped) parts.push("stopped early");
      onNotice(`${parts.join(", ")}.`);
    } catch (e) {
      onError(e);
    } finally {
      off();
      running = false;
    }
  }

  function statusText(r: BatchRow): string {
    const problem = problems.get(r.id);
    if (problem && r.status !== "done") return problem;
    switch (r.status) {
      case "running":
        return `${Math.round(r.progress * 100)}%`;
      case "done":
        return r.error ? `Done. ${r.error}` : "Done";
      case "failed":
        return r.error ?? "Failed";
      case "stopped":
        return "Stopped";
      default:
        return running ? "Waiting" : "Ready";
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<section class="panel" class:dropping aria-label="Batch Convert" bind:this={root}>
  <div class="toolbar">
    <button type="button" class="tool" title="Add Files" aria-label="Add Files" disabled={running} onclick={addFiles}>
      <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3 1.5h6l4 4v9H3z" fill="none" stroke="currentColor" /><path d="M8 7v5M5.5 9.5h5" stroke="currentColor" stroke-width="1.4" /></svg>
    </button>
    <button type="button" class="tool" title="Add Folder" aria-label="Add Folder" disabled={running} onclick={addFolder}>
      <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M1.5 3.5h5l1.5 1.5h6.5v8h-13z" fill="none" stroke="currentColor" /><path d="M8 7v4.5M5.75 9.25h4.5" stroke="currentColor" stroke-width="1.4" /></svg>
    </button>
    <button
      type="button"
      class="tool"
      title={poolCount ? "Add the files selected in the Files panel" : "Select files in the Files panel first"}
      aria-label="Add from Files panel"
      disabled={running || poolCount === 0}
      onclick={addFromFiles}
    >
      <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M2 3.5h12M2 7h12M2 10.5h7" stroke="currentColor" /><path d="M12.5 9v5M10 11.5h5" stroke="currentColor" stroke-width="1.4" /></svg>
    </button>
    <span class="gap"></span>
    <button type="button" class="tool" title="Remove from list" aria-label="Remove from list" disabled={running || selected.size === 0} onclick={() => remove(selected)}>
      <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" stroke-width="1.4" /></svg>
    </button>
    <button type="button" class="tool" title="Clear finished" aria-label="Clear finished" disabled={running || !rows.some((r) => r.status === "done")} onclick={() => remove(new Set(rows.filter((r) => r.status === "done").map((r) => r.id)))}>
      <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3 8.5l3 3 7-7" fill="none" stroke="currentColor" stroke-width="1.4" /></svg>
    </button>
  </div>

  <label class="process" title="An FX Chain preset run on every file before it is saved">
    <span>Process with</span>
    <select value={s.process ?? ""} disabled={running} onchange={(e) => (s.process = e.currentTarget.value || null)}>
      <option value="">Nothing (convert only)</option>
      {#if userPresets.length}
        <optgroup label="Yours">
          {#each userPresets as p (p.name)}<option value={p.name}>{p.name}</option>{/each}
        </optgroup>
      {/if}
      {#each GROUPS as g (g)}
        <optgroup label={g}>
          {#each BATCH_PRESETS.filter((p) => p.group === g) as p (p.name)}<option value={p.name}>{p.name}</option>{/each}
        </optgroup>
      {/each}
    </select>
  </label>
  <div class="process level" title="After the preset: bring every file to the same peak, and fade the ends">
    <span class="grp">
      <label class="check"><input type="checkbox" checked={s.level.normalize} disabled={running} onchange={(e) => (s.level.normalize = e.currentTarget.checked)} /> Normalize to</label>
      <input type="number" class="num" min="-30" max="0" step="0.1" value={s.level.normalizeDb} disabled={running || !s.level.normalize} aria-label="Normalize peak, dB" onchange={(e) => (s.level.normalizeDb = sanitizeLevel({ ...s.level, normalizeDb: Number(e.currentTarget.value) }).normalizeDb)} />
      <span>dB</span>
    </span>
    <span class="grp">
      <span>Fade in</span>
      <input type="number" class="num" min="0" max="60000" step="10" value={s.level.fadeInMs} disabled={running} aria-label="Fade in, ms" onchange={(e) => (s.level.fadeInMs = sanitizeLevel({ ...s.level, fadeInMs: Number(e.currentTarget.value) }).fadeInMs)} />
      <span>out</span>
      <input type="number" class="num" min="0" max="60000" step="10" value={s.level.fadeOutMs} disabled={running} aria-label="Fade out, ms" onchange={(e) => (s.level.fadeOutMs = sanitizeLevel({ ...s.level, fadeOutMs: Number(e.currentTarget.value) }).fadeOutMs)} />
      <span>ms</span>
    </span>
  </div>

  <div class="list" role="listbox" aria-label="Files to convert" aria-multiselectable="true">
    {#if rows.length === 0}
      <p class="empty">Drop audio files, folders or zips here, or use the buttons above. A call recorder's per-speaker files convert in one go.</p>
    {:else}
      <div class="head" aria-hidden="true"><span>Name</span><span>Status</span><span>Stage</span></div>
      {#each rows as r (r.id)}
        {@const bad = problems.has(r.id) && r.status !== "done"}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div
          class="row"
          class:sel={selected.has(r.id)}
          class:bad={bad || r.status === "failed"}
          class:done={r.status === "done"}
          role="option"
          aria-selected={selected.has(r.id)}
          tabindex="-1"
          title={`${r.source}\nBecomes ${dests.get(r.id) ?? ""}`}
          onclick={(e) => pick(e, r.id)}
        >
          <span class="name">{fileName(r.source)}</span>
          <span class="status" title={statusText(r)}>
            {#if r.status === "running"}<span class="bar"><span style:width="{r.progress * 100}%"></span></span>{/if}
            {statusText(r)}
          </span>
          <span class="dest">{r.stage ?? (problems.has(r.id) ? "Needs a fix" : "")}</span>
        </div>
      {/each}
    {/if}
  </div>

  <footer>
    <button type="button" disabled={running} title="Naming, where files go, format and tags" onclick={() => (outputOpen = true)}>Output Settings…</button>
    <span class="count" title={outputSummary}>{rows.length} {rows.length === 1 ? "file" : "files"}{problems.size ? `, ${problems.size} need attention` : ""} · {outputSummary}</span>
    {#if running}
      <button type="button" onclick={() => void batchCancel()}>Stop</button>
    {:else}
      <button type="button" class="primary" disabled={waiting.length === 0} onclick={() => void run()}>Convert {waiting.length || ""}</button>
    {/if}
  </footer>
</section>

{#if outputOpen}
  <BatchOutputDialog
    settings={s}
    {exampleStem}
    pickFolder={(cur) => exportPickFolder(cur, "Convert Into")}
    onApply={(next) => {
      s = next;
      outputOpen = false;
    }}
    onClose={() => (outputOpen = false)}
  />
{/if}

<style>
  .panel {
    display: grid;
    grid-template-rows: auto auto auto minmax(70px, 1fr) auto;
    grid-template-columns: minmax(0, 1fr);
    height: 100%;
    min-height: 0;
    overflow: hidden;
    background: var(--surface-1);
  }

  .panel.dropping {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }

  .toolbar {
    display: flex;
    gap: 2px;
    align-items: center;
    padding: 3px 4px;
    border-bottom: 1px solid var(--border);
  }

  .gap {
    flex: 1;
  }

  .process {
    display: flex;
    gap: 6px;
    align-items: center;
    padding: 4px 6px;
    border-bottom: 1px solid var(--border);
    color: var(--text-dim);
  }

  .process select {
    flex: 1;
    min-width: 0;
  }
  .level {
    flex-wrap: wrap;
    gap: 4px;
  }
  .level .check {
    display: flex;
    gap: 4px;
    align-items: center;
  }
  .level .num {
    width: 56px;
    height: 20px;
    padding: 0 4px;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    background: var(--surface-0);
    color: var(--text);
    font: inherit;
    font-size: 11px;
  }
  .level {
    row-gap: 4px;
    column-gap: 12px;
  }
  .level .grp {
    display: flex;
    gap: 4px;
    align-items: center;
    white-space: nowrap;
  }

  .tool {
    display: grid;
    place-items: center;
    width: 22px;
    height: 20px;
    padding: 0;
    border: 1px solid transparent;
    border-radius: 2px;
    background: none;
    color: var(--text-dim);
  }

  .tool svg {
    width: 14px;
    height: 14px;
  }

  .tool:hover:not(:disabled) {
    border-color: var(--border-strong);
    color: var(--text);
  }

  .tool:disabled {
    color: var(--text-faint);
    opacity: 0.6;
  }

  .list {
    min-height: 0;
    min-width: 0;
    overflow-x: hidden;
    overflow-y: auto;
    background: var(--surface-0);
  }

  .empty {
    margin: 0;
    padding: 14px 12px;
    color: var(--text-faint);
    line-height: 1.5;
  }

  .head,
  .row {
    display: grid;
    grid-template-columns: minmax(0, 1.2fr) minmax(0, 1fr) minmax(64px, 0.9fr);
    gap: 8px;
    align-items: center;
    padding: 0 8px;
  }

  .head {
    position: sticky;
    top: 0;
    height: 20px;
    border-bottom: 1px solid var(--border);
    background: var(--surface-2);
    color: var(--text-faint);
    font-size: var(--font-size-small);
  }

  .row {
    height: 22px;
    border-bottom: 1px solid color-mix(in srgb, var(--border) 50%, transparent);
    cursor: default;
  }

  .row.sel {
    background: color-mix(in srgb, var(--accent) 22%, transparent);
  }

  .row span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .dest {
    color: var(--text-dim);
  }

  .status {
    display: flex;
    gap: 6px;
    align-items: center;
    color: var(--text-dim);
    font-size: var(--font-size-small);
  }

  .row.done .status {
    color: var(--ok, #5fbf8f);
  }

  .row.bad .status {
    color: var(--danger);
  }

  .bar {
    flex: 0 0 36px;
    height: 5px;
    border-radius: 2px;
    background: var(--surface-3);
  }

  .bar span {
    display: block;
    height: 100%;
    border-radius: 2px;
    background: var(--accent);
  }

  footer {
    display: flex;
    gap: 8px;
    align-items: center;
    justify-content: space-between;
    padding: 3px 6px;
    border-top: 1px solid var(--border);
    background: var(--surface-2);
  }

  .count {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    color: var(--text-dim);
    font-size: var(--font-size-small);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  footer button,
  .process select {
    height: 20px;
    padding: 0 6px;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    background: var(--surface-0);
    color: var(--text);
    font: inherit;
    font-size: 11px;
    white-space: nowrap;
  }

  footer button:disabled {
    color: var(--text-faint);
    opacity: 0.6;
  }

  footer button.primary {
    border-color: var(--accent);
    background: var(--accent);
    color: var(--on-accent, #fff);
  }

  button:not(.tool) {
    height: 24px;
    padding: 0 12px;
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
