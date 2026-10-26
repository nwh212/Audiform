<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  Drive Manager. Browse drives and folders, preview audio, and bring
  files into the session by double-click, Import, or dragging rows onto
  the Files panel.
-->
<script lang="ts">
  import { onDestroy } from "svelte";
  import ContextMenu, { type ContextItem } from "../../app/ContextMenu.svelte";
  import {
    browseList,
    browseLocations,
    previewPlay,
    previewStatus,
    previewStop,
    probeMedia,
    revealMedia,
    type AudioInfo,
    type BrowseEntry,
    type BrowseLocation,
  } from "../../app/core";
  import type { FileDropRouter } from "../../app/filedrop";
  import { beginPathDrag } from "../../app/pathdrag";
  import { revealLabel } from "../../lib/platform";
  import type { Preferences } from "../../stores/preferences";
  import { formatChannels, formatDuration, formatSize } from "../files/format";
  import { clickSelection, stepSelection } from "../files/selection";
  import { parentFolder, shortDate } from "./paths";

  interface Props {
    router: FileDropRouter;
    prefs: Preferences;
    /** Adds files or folders to the session's Files panel. */
    onImport: (paths: string[]) => void;
    onError?: (e: unknown) => void;
  }

  const { router, prefs, onImport, onError = console.error }: Props = $props();

  const KEY_FOLDER = "drive-manager.folder";
  const KEY_AUDIO_ONLY = "drive-manager.audio-only";
  const KEY_AUTOPLAY = "drive-manager.autoplay";

  let locations = $state<BrowseLocation[]>([]);
  let folder = $state("");
  let typedPath = $state("");
  let entries = $state<BrowseEntry[]>([]);
  let loadError = $state<string | null>(null);
  let loading = $state(false);
  let audioOnly = $state(true);
  let autoPlay = $state(false);
  let query = $state("");

  let selected = $state<Set<string>>(new Set());
  let anchor = $state<string | null>(null);
  let info = $state<{ path: string; info: AudioInfo | null; error: string | null } | null>(null);

  let playingPath = $state<string | null>(null);
  let position = $state(0);
  let poll: ReturnType<typeof setInterval> | undefined;

  let back: string[] = [];
  let forward: string[] = [];
  let canBack = $state(false);
  let canForward = $state(false);

  let list = $state<HTMLElement | null>(null);
  let menu = $state<{ x: number; y: number } | null>(null);

  const visible = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return q ? entries.filter((e) => e.name.toLowerCase().includes(q)) : entries;
  });
  const visiblePaths = $derived(visible.map((e) => e.path));
  const selectedEntries = $derived(visible.filter((e) => selected.has(e.path)));
  const single = $derived(selectedEntries.length === 1 ? selectedEntries[0]! : null);
  const currentLocation = $derived(
    locations.find((l) => l.path === folder) ??
      [...locations]
        .filter((l) => folder.toLowerCase().startsWith(l.path.toLowerCase()))
        .sort((a, b) => b.path.length - a.path.length)[0] ??
      null,
  );

  // Startup: restore preferences, then list locations and open the last folder.
  void (async () => {
    await prefs.load();
    audioOnly = prefs.boolean(KEY_AUDIO_ONLY, true);
    autoPlay = prefs.boolean(KEY_AUTOPLAY, false);
    try {
      locations = await browseLocations();
    } catch (e) {
      onError(e);
    }
    const saved = prefs.string(KEY_FOLDER, "");
    const start = saved || locations.find((l) => l.label === "Music")?.path || locations[0]?.path || "";
    if (start) await open(start, false);
  })();

  async function open(path: string, remember = true) {
    if (!path) return;
    if (remember && folder && folder !== path) {
      back.push(folder);
      forward = [];
    }
    canBack = back.length > 0;
    canForward = forward.length > 0;
    loading = true;
    try {
      const result = await browseList(path, audioOnly);
      folder = path;
      typedPath = path;
      entries = result;
      loadError = null;
      selected = new Set();
      anchor = null;
      info = null;
      prefs.set(KEY_FOLDER, path);
    } catch (e) {
      loadError = e instanceof Error ? e.message : String(e);
      typedPath = folder;
      // Keep showing the folder we were in.
      if (!folder) {
        folder = path;
        entries = [];
      }
    } finally {
      loading = false;
    }
  }

  function refresh() {
    const keep = new Set(selected);
    void open(folder, false).then(() => (selected = new Set([...keep].filter((p) => entries.some((e) => e.path === p)))));
  }

  function goBack() {
    const prev = back.pop();
    if (!prev) return;
    forward.push(folder);
    void open(prev, false);
  }

  function goForward() {
    const next = forward.pop();
    if (!next) return;
    back.push(folder);
    void open(next, false);
  }

  function goUp() {
    const up = parentFolder(folder);
    if (up) void open(up);
  }

  function setAudioOnly(v: boolean) {
    audioOnly = v;
    prefs.set(KEY_AUDIO_ONLY, v);
    refresh();
  }

  function setAutoPlay(v: boolean) {
    autoPlay = v;
    prefs.set(KEY_AUTOPLAY, v);
    if (!v) void stopPreview();
  }

  function selectOnly(path: string) {
    selected = new Set([path]);
    anchor = path;
    afterSelect();
  }

  function afterSelect() {
    const one = single;
    if (!one || one.isDir || !one.audio) {
      info = null;
      return;
    }
    const path = one.path;
    if (info?.path !== path) {
      info = { path, info: null, error: null };
      probeMedia(path)
        .then((i) => {
          if (info?.path === path) info = { path, info: i, error: null };
        })
        .catch((e: unknown) => {
          if (info?.path === path) info = { path, info: null, error: e instanceof Error ? e.message : String(e) };
        });
    }
    if (autoPlay) void play(path);
  }

  function onRowPointer(e: PointerEvent, entry: BrowseEntry) {
    if (e.button === 2 && selected.has(entry.path)) return;
    if (e.button !== 0 && e.button !== 2) return;
    if (e.button === 0 && selected.has(entry.path) && !e.ctrlKey && !e.shiftKey) {
      // Pressing an already selected row may start a drag of the whole selection.
      beginPathDrag(e, router, () => selectedEntries.map((s) => s.path));
      list?.focus();
      return;
    }
    const next = clickSelection(selected, anchor, entry.path, visiblePaths, {
      ctrl: e.ctrlKey || e.metaKey,
      shift: e.shiftKey,
    });
    selected = new Set(next.selected);
    anchor = next.anchor;
    afterSelect();
    if (e.button === 0) beginPathDrag(e, router, () => selectedEntries.map((s) => s.path));
    list?.focus();
  }

  function activate(entry: BrowseEntry) {
    if (entry.isDir) void open(entry.path);
    else if (entry.audio) onImport([entry.path]);
  }

  function importSelected() {
    const paths = selectedEntries.filter((e) => e.isDir || e.audio).map((e) => e.path);
    if (paths.length) onImport(paths);
  }

  async function play(path: string, start = 0) {
    try {
      await previewPlay(path, start);
      playingPath = path;
      position = start;
      startPolling();
    } catch (e) {
      playingPath = null;
      onError(e);
    }
  }

  async function stopPreview() {
    clearInterval(poll);
    poll = undefined;
    if (playingPath) {
      playingPath = null;
      await previewStop().catch(onError);
    }
  }

  function togglePreview() {
    if (playingPath) void stopPreview();
    else if (single && single.audio) void play(single.path);
  }

  function startPolling() {
    clearInterval(poll);
    poll = setInterval(async () => {
      try {
        const s = await previewStatus();
        if (!s.playing) {
          playingPath = null;
          clearInterval(poll);
          poll = undefined;
        } else {
          position = s.position;
        }
      } catch {
        clearInterval(poll);
      }
    }, 100);
  }

  function seek(e: MouseEvent) {
    const dur = info?.info ? info.info.length / info.info.sampleRate : 0;
    if (!info || !dur) return;
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const t = Math.max(0, Math.min(1, (e.clientX - r.left) / r.width)) * dur;
    void play(info.path, t);
  }

  function onListKey(e: KeyboardEvent) {
    switch (e.key) {
      case "ArrowDown":
      case "ArrowUp": {
        e.preventDefault();
        const id = stepSelection(visiblePaths, anchor, e.key === "ArrowDown" ? 1 : -1);
        if (id) {
          selectOnly(id);
          list?.querySelector<HTMLElement>(`[data-path="${CSS.escape(id)}"]`)?.scrollIntoView({ block: "nearest" });
        }
        break;
      }
      case "Enter":
        e.preventDefault();
        if (single) activate(single);
        break;
      case "Backspace":
        e.preventDefault();
        goUp();
        break;
      case " ":
        e.preventDefault();
        togglePreview();
        break;
    }
  }

  function onPathKey(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      void open(typedPath.trim());
    } else if (e.key === "Escape") {
      typedPath = folder;
    }
  }

  const menuItems = $derived.by((): ContextItem[] => {
    const importable = selectedEntries.some((e) => e.isDir || e.audio);
    return [
      { kind: "item", label: "Import to Files", run: importSelected, disabled: !importable },
      {
        kind: "item",
        label: playingPath ? "Stop Preview" : "Preview",
        run: togglePreview,
        disabled: !playingPath && !(single && single.audio),
        shortcut: "Space",
      },
      { kind: "sep" },
      { kind: "item", label: "Open Folder", run: () => single && void open(single.path), disabled: !single?.isDir },
      {
        kind: "item",
        label: revealLabel(),
        run: () => single && void revealMedia(single.path).catch(onError),
        disabled: !single,
      },
      { kind: "sep" },
      { kind: "item", label: "Refresh", run: refresh },
    ];
  });

  onDestroy(() => {
    clearInterval(poll);
    if (playingPath) void previewStop();
  });

  const duration = $derived(info?.info ? info.info.length / info.info.sampleRate : 0);
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<section class="panel" aria-label="Drive Manager" oncontextmenu={(e) => {
  e.preventDefault();
  menu = { x: e.clientX, y: e.clientY };
}}>
  <div class="bar">
    <button type="button" class="tool" title="Back" aria-label="Back" disabled={!canBack} onclick={goBack}>
      <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M10 3L5 8l5 5" fill="none" stroke="currentColor" stroke-width="1.5" /></svg>
    </button>
    <button type="button" class="tool" title="Forward" aria-label="Forward" disabled={!canForward} onclick={goForward}>
      <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M6 3l5 5-5 5" fill="none" stroke="currentColor" stroke-width="1.5" /></svg>
    </button>
    <button type="button" class="tool" title="Up one folder (Backspace)" aria-label="Up one folder" disabled={!parentFolder(folder)} onclick={goUp}>
      <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3 9l5-5 5 5M8 4v9" fill="none" stroke="currentColor" stroke-width="1.5" /></svg>
    </button>
    <select
      class="where"
      aria-label="Drive or place"
      value={currentLocation?.path ?? ""}
      onchange={(e) => void open((e.currentTarget as HTMLSelectElement).value)}
    >
      {#if !currentLocation}<option value="" disabled>Choose</option>{/if}
      {#each locations.filter((l) => l.kind === "drive") as l (l.path)}
        <option value={l.path}>{l.label}</option>
      {/each}
      {#if locations.some((l) => l.kind === "place")}
        <option disabled>──────</option>
      {/if}
      {#each locations.filter((l) => l.kind === "place") as l (l.path)}
        <option value={l.path}>{l.label}</option>
      {/each}
    </select>
  </div>

  <div class="bar">
    <input
      class="path"
      type="text"
      spellcheck="false"
      aria-label="Folder path"
      bind:value={typedPath}
      onkeydown={onPathKey}
    />
  </div>

  <div class="bar">
    <button
      type="button"
      class="tool"
      class:on={!!playingPath}
      title={playingPath ? "Stop preview (Space)" : "Preview (Space)"}
      aria-label={playingPath ? "Stop preview" : "Preview"}
      disabled={!playingPath && !(single && single.audio)}
      onclick={togglePreview}
    >
      {#if playingPath}
        <svg viewBox="0 0 16 16" aria-hidden="true"><rect x="4" y="4" width="8" height="8" fill="currentColor" /></svg>
      {:else}
        <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M5 3.5v9l7-4.5z" fill="currentColor" /></svg>
      {/if}
    </button>
    <label class="check" title="Play each audio file when selected">
      <input type="checkbox" checked={autoPlay} onchange={(e) => setAutoPlay(e.currentTarget.checked)} />
      Auto-play
    </label>
    <label class="check" title="Hide files that are not audio">
      <input type="checkbox" checked={audioOnly} onchange={(e) => setAudioOnly(e.currentTarget.checked)} />
      Audio only
    </label>
    <input class="filter" type="search" placeholder="Filter" aria-label="Filter this folder" bind:value={query} />
  </div>

  <div
    class="rows"
    role="listbox"
    aria-label="Folder contents"
    aria-multiselectable="true"
    tabindex="0"
    bind:this={list}
    onkeydown={onListKey}
  >
    {#each visible as entry (entry.path)}
      <div
        class="row"
        class:dim={!entry.isDir && !entry.audio}
        role="option"
        tabindex="-1"
        aria-selected={selected.has(entry.path)}
        data-path={entry.path}
        title={entry.path}
        onpointerdown={(e) => onRowPointer(e, entry)}
        ondblclick={() => activate(entry)}
      >
        <span class="icon" aria-hidden="true">
          {#if entry.isDir}
            <svg viewBox="0 0 16 16"><path d="M1.5 3.5h5l1.5 1.5h6.5v8h-13z" fill="currentColor" opacity="0.75" /></svg>
          {:else if entry.audio}
            <svg viewBox="0 0 16 16"><path d="M2 8h1.5M4.5 5v6M7 3v10M9.5 6v4M12 4.5v7M14 8h0.5" stroke="currentColor" stroke-width="1.4" /></svg>
          {:else}
            <svg viewBox="0 0 16 16"><path d="M4 1.5h5.5l3 3v10H4z" fill="none" stroke="currentColor" /></svg>
          {/if}
        </span>
        <span class="name" class:playing={playingPath === entry.path}>{entry.name}</span>
        <span class="size">{entry.isDir ? "" : formatSize(entry.size)}</span>
        <span class="date">{entry.modified ? shortDate(entry.modified) : ""}</span>
      </div>
    {/each}
    {#if loadError}
      <p class="empty bad">{loadError}</p>
    {:else if !loading && visible.length === 0}
      <p class="empty">{query ? "Nothing matches." : audioOnly ? "No audio or folders here." : "This folder is empty."}</p>
    {/if}
  </div>

  <footer class="foot">
    {#if info}
      {#if info.info}
        <div class="meta">
          {info.info.format}, {info.info.codec}, {info.info.sampleRate.toLocaleString("en-US")} Hz, {formatChannels(
            info.info.channels,
          )}, {formatDuration(info.info)}
        </div>
      {:else if info.error}
        <div class="meta bad">{info.error}</div>
      {:else}
        <div class="meta">Reading...</div>
      {/if}
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <div
        class="progress"
        role="slider"
        tabindex="-1"
        aria-label="Preview position"
        aria-valuemin="0"
        aria-valuemax={Math.round(duration)}
        aria-valuenow={Math.round(playingPath === info.path ? position : 0)}
        title="Click to preview from here"
        onclick={seek}
      >
        <div
          class="fill"
          style:width="{duration && playingPath === info.path ? Math.min(100, (position / duration) * 100) : 0}%"
        ></div>
      </div>
    {:else}
      <div class="meta">{selected.size > 1 ? `${selected.size} selected` : `${visible.length} items`}</div>
    {/if}
  </footer>
</section>

{#if menu}
  <ContextMenu x={menu.x} y={menu.y} items={menuItems} onClose={() => (menu = null)} />
{/if}

<style>
  .panel {
    display: grid;
    grid-template-rows: auto auto auto 1fr auto;
    grid-template-columns: minmax(0, 1fr);
    height: 100%;
    overflow: hidden;
    container-type: inline-size;
    background: var(--surface-1);
  }

  .bar {
    display: flex;
    gap: 3px;
    align-items: center;
    min-width: 0;
    padding: 3px 4px;
    border-bottom: 1px solid var(--border);
  }

  .tool {
    display: grid;
    flex: none;
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
    width: 13px;
    height: 13px;
  }

  .tool:hover:not(:disabled) {
    border-color: var(--border-strong);
    color: var(--text);
  }

  .tool:disabled {
    opacity: 0.4;
  }

  .tool.on {
    color: var(--playhead);
  }

  select,
  input[type="text"],
  input[type="search"] {
    min-width: 0;
    height: 20px;
    padding: 0 5px;
    border: 1px solid var(--border);
    border-radius: 2px;
    background: var(--surface-0);
    color: var(--text);
    font: inherit;
  }

  select:focus,
  input:focus {
    border-color: var(--accent-dim);
    outline: none;
  }

  .where {
    flex: 1;
  }

  .path {
    flex: 1;
    font-family: var(--font-mono);
    font-size: var(--font-size-small);
  }

  .filter {
    flex: 1;
    min-width: 40px;
  }

  .check {
    display: flex;
    flex: none;
    gap: 3px;
    align-items: center;
    color: var(--text-dim);
    font-size: var(--font-size-small);
    white-space: nowrap;
  }

  .check input {
    margin: 0;
  }

  .rows {
    overflow: auto;
    min-height: 0;
    outline: none;
  }

  .row {
    display: grid;
    grid-template-columns: 18px minmax(60px, 1fr) 62px 74px;
    align-items: center;
    height: 20px;
    padding: 0 4px;
    border-bottom: 1px solid var(--surface-0);
    cursor: default;
    user-select: none;
  }

  .row:hover {
    background: var(--surface-2);
  }

  .row[aria-selected="true"] {
    background: var(--accent-dim);
    color: #fff;
  }

  .row.dim {
    color: var(--text-faint);
  }

  .icon {
    display: grid;
    place-items: center;
    color: var(--text-dim);
  }

  .icon svg {
    width: 13px;
    height: 13px;
  }

  .name,
  .size,
  .date {
    overflow: hidden;
    padding: 0 4px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .name.playing {
    color: var(--playhead);
  }

  .size,
  .date {
    color: var(--text-dim);
    font-size: var(--font-size-small);
    text-align: right;
  }

  .row[aria-selected="true"] .size,
  .row[aria-selected="true"] .date,
  .row[aria-selected="true"] .icon {
    color: #e6f2fc;
  }

  .empty {
    margin: 0;
    padding: 14px 10px;
    color: var(--text-faint);
    text-align: center;
  }

  .bad {
    color: var(--danger);
  }

  .foot {
    display: grid;
    gap: 3px;
    padding: 4px 6px;
    border-top: 1px solid var(--border);
    font-size: var(--font-size-small);
  }

  .meta {
    overflow: hidden;
    color: var(--text-dim);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .progress {
    height: 5px;
    border-radius: 2px;
    background: var(--surface-0);
    cursor: pointer;
  }

  .fill {
    height: 100%;
    border-radius: 2px;
    background: var(--playhead);
  }

  @container (max-width: 330px) {
    .row {
      grid-template-columns: 18px minmax(60px, 1fr) 62px;
    }

    .date {
      display: none;
    }
  }

  @container (max-width: 230px) {
    .row {
      grid-template-columns: 18px minmax(60px, 1fr);
    }

    .size {
      display: none;
    }
  }
</style>
