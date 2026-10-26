<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  Files panel. Lists the session's audio files with their format
  details. Files arrive from the Import commands or by dragging them in
  from the system file manager.
-->
<script lang="ts">
  import type { ActionRegistry } from "../../app/actions";
  import ContextMenu, { type ContextItem } from "../../app/ContextMenu.svelte";
  import type { FileDropRouter } from "../../app/filedrop";
  import type { MediaItem, MediaPool, MediaPoolState } from "../../stores/media-pool";
  import {
    formatChannels,
    formatDuration,
    formatRate,
    formatSize,
    matches,
    sortItems,
    statusText,
    type SortKey,
  } from "./format";
  import { clickSelection, stepSelection } from "./selection";
  import { beginPathDrag } from "../../app/pathdrag";
  import { fileManagerName, revealLabel } from "../../lib/platform";
  import { previewPlay, previewStatus, previewStop } from "../../app/core";
  import { prefs } from "../../stores/prefs";
  import { onDestroy, untrack } from "svelte";
  import type { Readable } from "svelte/store";
  import { sessionDuration, type SessionRow } from "./session-row";

  interface Props {
    pool: MediaPool;
    actions: ActionRegistry;
    drop: FileDropRouter;
    /** Imports dropped paths. */
    onDropPaths: (paths: string[]) => void;
    /** Double-click: open the file in the Waveform editor. */
    onOpen?: (path: string) => void;
    shortcutFor?: (id: string) => string | undefined;
    /** The open multitrack sessions, listed first: the one in front, then the ones set aside. */
    sessions?: Readable<SessionRow[]>;
    /** Double-click on a session row: bring it to the front in the Multitrack view. */
    onOpenSession?: (id: string) => void;
    /** Close a session (asks to save first). */
    onCloseSession?: (id: string) => void;
  }

  const { pool, actions, drop, onDropPaths, onOpen = () => {}, shortcutFor = () => undefined, sessions, onOpenSession = () => {}, onCloseSession = () => {} }: Props = $props();

  let sessionRows = $state<SessionRow[]>([]);
  $effect(() =>
    sessions?.subscribe((r) => {
      sessionRows = r;
    }),
  );
  /** A session row picked with a click, for Delete or the remove button. */
  let pickedSession = $state<string | null>(null);
  $effect(() => {
    if (pickedSession && !sessionRows.some((r) => r.id === pickedSession)) pickedSession = null;
  });
  $effect(() => {
    // Picking files drops the session pick.
    if (snap?.selected.size) pickedSession = null;
  });

  let snap = $state<MediaPoolState | null>(null);
  $effect(() =>
    pool.subscribe((s) => {
      snap = s;
    }),
  );

  // Re-read action enabled state for the toolbar.
  let tick = $state(0);
  $effect(() =>
    actions.changed.subscribe((n) => {
      tick = n;
    }),
  );
  const can = (id: string) => {
    void tick;
    return actions.isEnabled(id);
  };
  const run = (id: string) => void actions.run(id);
  const tip = (label: string, id: string) => {
    const key = shortcutFor(id);
    return key ? `${label} (${key})` : label;
  };

  let root = $state<HTMLElement | null>(null);
  let list = $state<HTMLElement | null>(null);
  let dropping = $state(false);

  $effect(() => {
    if (!root) return;
    return drop.register({
      element: root,
      onDrop: onDropPaths,
      onHover: (over) => (dropping = over),
    });
  });

  let query = $state("");
  let sortKey = $state<SortKey | null>(null);
  let descending = $state(false);
  let anchor = $state<string | null>(null);
  let menu = $state<{ x: number; y: number } | null>(null);
  /** Right-click on a session row: its own menu. */
  let sessionMenu = $state<{ x: number; y: number; id: string } | null>(null);

  const visible = $derived.by(() => {
    if (!snap) return [];
    return sortItems(
      snap.items.filter((it) => matches(it, query)),
      sortKey,
      descending,
    );
  });
  const visibleIds = $derived(visible.map((it) => it.id));

  const counts = $derived.by(() => {
    const items = snap?.items ?? [];
    return {
      total: items.length,
      reading: items.filter((it) => it.status === "probing").length,
      errors: items.filter((it) => it.status === "error").length,
      selected: snap?.selected.size ?? 0,
    };
  });

  function sortBy(key: SortKey) {
    if (sortKey === key) {
      if (descending) {
        sortKey = null;
        descending = false;
      } else descending = true;
    } else {
      sortKey = key;
      descending = false;
    }
  }

  function ariaSort(key: SortKey): "ascending" | "descending" | "none" {
    if (sortKey !== key) return "none";
    return descending ? "descending" : "ascending";
  }

  function onRowPointer(e: PointerEvent, item: MediaItem) {
    if (!snap) return;
    // Right-click on a selected row keeps the selection.
    if (e.button === 2 && snap.selected.has(item.id)) return;
    if (e.button !== 0 && e.button !== 2) return;
    // Pressing a selected row keeps the selection so the whole group can be dragged.
    if (e.button === 0 && snap.selected.has(item.id) && !e.ctrlKey && !e.metaKey && !e.shiftKey) {
      list?.focus();
      beginPathDrag(e, drop, () => pool.selectedItems().map((it) => it.path), undefined, root ?? undefined);
      return;
    }
    const next = clickSelection(snap.selected, anchor, item.id, visibleIds, {
      ctrl: e.ctrlKey || e.metaKey,
      shift: e.shiftKey,
    });
    anchor = next.anchor;
    pool.setSelection(next.selected);
    list?.focus();
    if (e.button === 0) {
      beginPathDrag(
        e,
        drop,
        () => pool.selectedItems().map((it) => it.path),
        undefined,
        root ?? undefined,
      );
    }
  }

  /** The session row's right-click menu. The id is fixed here: the menu closes before an item runs. */
  function sessionItems(id: string): ContextItem[] {
    const row = sessionRows.find((r) => r.id === id);
    return [
      { kind: "item", label: row?.front ? "Show in Multitrack" : "Switch to This Session", run: () => onOpenSession(id) },
      { kind: "item", label: "Save", disabled: !row?.front, reason: "Switch to the session first.", run: () => run("file.save"), shortcut: shortcutFor("file.save") },
      { kind: "sep" },
      { kind: "item", label: row?.dirty ? "Close Session..." : "Close Session", shortcut: "Delete", run: () => onCloseSession(id) },
      { kind: "item", label: "Close All Sessions...", run: () => run("file.close-all") },
    ];
  }

  function onListKey(e: KeyboardEvent) {
    // Plain Delete or Backspace only: chords such as Ctrl+Alt+Backspace (Delete Track) pass on.
    if (pickedSession && (e.key === "Delete" || e.key === "Backspace") && !e.ctrlKey && !e.altKey && !e.shiftKey && !e.metaKey) {
      e.preventDefault();
      e.stopPropagation();
      onCloseSession(pickedSession);
      return;
    }
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp" && e.key !== "Home" && e.key !== "End") return;
    e.preventDefault();
    let id: string | null;
    if (e.key === "Home") id = visibleIds[0] ?? null;
    else if (e.key === "End") id = visibleIds[visibleIds.length - 1] ?? null;
    else id = stepSelection(visibleIds, lastSelected(), e.key === "ArrowDown" ? 1 : -1);
    if (!id) return;
    if (e.shiftKey && anchor) {
      const next = clickSelection(snap?.selected ?? new Set(), anchor, id, visibleIds, { ctrl: false, shift: true });
      pool.setSelection(next.selected);
    } else {
      anchor = id;
      pool.setSelection([id]);
    }
    list?.querySelector<HTMLElement>(`[data-id="${CSS.escape(id)}"]`)?.scrollIntoView({ block: "nearest" });
  }

  function lastSelected(): string | null {
    if (!snap) return null;
    const sel = visibleIds.filter((id) => snap!.selected.has(id));
    return anchor && snap.selected.has(anchor) ? anchor : (sel[sel.length - 1] ?? null);
  }

  /** Drag from empty space in the list: a box that picks every file it touches (Ctrl or Shift adds to what is picked). */
  let pickBox = $state<{ x: number; y: number; w: number; h: number } | null>(null);
  function onBlankPointer(e: PointerEvent) {
    if (e.target !== e.currentTarget || e.button !== 0 || !list) return;
    const el = list;
    const keep = e.ctrlKey || e.metaKey || e.shiftKey ? new Set(snap?.selected ?? []) : new Set<string>();
    if (!keep.size) {
      pool.setSelection([]);
      anchor = null;
    }
    el.focus();
    const r0 = el.getBoundingClientRect();
    const sx = e.clientX - r0.left;
    const sy = e.clientY - r0.top + el.scrollTop;
    let moved = false;
    const move = (ev: PointerEvent) => {
      const r = el.getBoundingClientRect();
      const x = ev.clientX - r.left;
      const y = ev.clientY - r.top + el.scrollTop;
      if (!moved && Math.abs(x - sx) < 4 && Math.abs(y - sy) < 4) return;
      moved = true;
      pickBox = { x: Math.min(sx, x), y: Math.min(sy, y), w: Math.abs(x - sx), h: Math.abs(y - sy) };
      const top = Math.min(sy, y);
      const bottom = Math.max(sy, y);
      const picked = new Set(keep);
      for (const row of el.querySelectorAll<HTMLElement>(".row[data-id]")) {
        const rt = row.offsetTop;
        if (rt + row.offsetHeight >= top && rt <= bottom) picked.add(row.dataset.id!);
      }
      pool.setSelection([...picked]);
    };
    const up = () => {
      window.removeEventListener("pointermove", move, true);
      window.removeEventListener("pointerup", up, true);
      pickBox = null;
    };
    window.addEventListener("pointermove", move, true);
    window.addEventListener("pointerup", up, true);
  }

  function openMenu(e: MouseEvent) {
    e.preventDefault();
    menu = { x: e.clientX, y: e.clientY };
  }

  const menuItems = $derived.by((): ContextItem[] => {
    void tick;
    const selected = snap ? pool.selectedItems() : [];
    const failed = selected.filter((it) => it.status === "error");
    const item = (id: string, label: string): ContextItem => ({
      kind: "item",
      label,
      run: () => run(id),
      disabled: !actions.isEnabled(id),
      shortcut: shortcutFor(id),
    });
    const out: ContextItem[] = [
      item("files.edit", "Edit File"),
      {
        kind: "sub",
        label: "Insert into Multitrack",
        items: [item("files.insert-at-playhead", "At the Playhead on the Selected Track"), item("multitrack.insert-selected-tracks", "As New Tracks")],
      },
      { kind: "sep" },
      item("file.import-files", "Import Files..."),
      item("file.import-folder", "Import Folder..."),
      { kind: "sep" },
      item("edit.delete", selected.length > 1 ? `Remove ${selected.length} Files` : "Remove from Session"),
      item("file.close-unused", "Remove Unused Files"),
      item("file.reveal", revealLabel()),
    ];
    if (failed.length > 0) {
      out.push({
        kind: "item",
        label: "Try Reading Again",
        run: () => failed.forEach((it) => pool.retry(it.id)),
      });
    }
    out.push({ kind: "sep" }, item("edit.select-all", "Select All"));
    return out;
  });

  // Preview: hear a file without adding it to a track. Loop restarts it
  // when it ends; Auto-Play previews whichever single file is selected.
  let playingPath = $state<string | null>(null);
  let loop = $state(prefs.boolean("files.preview.loop", false));
  let autoPlay = $state(prefs.boolean("files.preview.autoplay", false));
  let poll: ReturnType<typeof setInterval> | undefined;
  const single = $derived.by(() => {
    if (!snap || snap.selected.size !== 1) return null;
    const it = snap.items.find((i) => snap!.selected.has(i.id));
    return it && it.status === "ready" ? it : null;
  });

  async function startPreview(path: string) {
    clearInterval(poll);
    try {
      await previewPlay(path);
    } catch {
      playingPath = null;
      return;
    }
    playingPath = path;
    poll = setInterval(async () => {
      const st = await previewStatus().catch(() => null);
      if (st?.playing && st.path === playingPath) return;
      if (loop && playingPath && !st?.playing) {
        const again = playingPath;
        clearInterval(poll);
        await startPreview(again);
        return;
      }
      clearInterval(poll);
      playingPath = null;
    }, 120);
  }

  async function stopPreview() {
    clearInterval(poll);
    playingPath = null;
    await previewStop().catch(() => {});
  }

  function togglePreview() {
    if (playingPath) void stopPreview();
    else if (single) void startPreview(single.path);
  }

  function toggleLoop() {
    loop = !loop;
    prefs.set("files.preview.loop", loop);
  }

  function toggleAutoPlay() {
    autoPlay = !autoPlay;
    prefs.set("files.preview.autoplay", autoPlay);
    if (!autoPlay && playingPath) void stopPreview();
  }

  // Auto-Play follows the selection.
  let lastAuto: string | null = null;
  $effect(() => {
    const path = single?.path ?? null;
    if (!autoPlay) {
      lastAuto = null;
      return;
    }
    if (path === lastAuto) return;
    lastAuto = path;
    untrack(() => {
      if (path) void startPreview(path);
      else if (playingPath) void stopPreview();
    });
  });

  onDestroy(() => {
    clearInterval(poll);
    if (playingPath) void previewStop();
  });

  function rowTitle(it: MediaItem): string {
    if (it.status === "error") return `${it.path}\n${it.error ?? "Could not read this file."}`;
    if (!it.info) return it.path;
    const i = it.info;
    const depth = i.bitDepth ? `, ${i.bitDepth}-bit` : "";
    return `${it.path}\n${i.format}, ${i.codec}${depth}, ${formatSize(i.fileSize)}`;
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<section class="panel" class:dropping aria-label="Files" data-area="files" bind:this={root} oncontextmenu={openMenu}>
  <div class="toolbar">
    <button type="button" class="tool" title={tip("New Session", "file.new-session")} aria-label="New Session" disabled={!can("file.new-session")} onclick={() => run("file.new-session")}>
      <svg viewBox="0 0 16 16" aria-hidden="true">
        <rect x="1.5" y="2.5" width="13" height="11" rx="1" fill="none" stroke="currentColor" />
        <path d="M3.5 6h6M3.5 8.5h6M3.5 11h6" stroke="currentColor" stroke-width="1.1" />
        <path d="M12.5 5.5v4M10.5 7.5h4" stroke="currentColor" stroke-width="1.4" />
      </svg>
    </button>
    <button type="button" class="tool" title={tip("New Audio File", "file.new-audio")} aria-label="New Audio File" disabled={!can("file.new-audio")} onclick={() => run("file.new-audio")}>
      <svg viewBox="0 0 16 16" aria-hidden="true">
        <path d="M1.5 8h1.5l1-3 1.5 6 1.5-6 1 3h1" fill="none" stroke="currentColor" stroke-width="1.2" />
        <path d="M12.5 4.5v6M9.5 7.5h6" stroke="currentColor" stroke-width="1.4" />
      </svg>
    </button>
    <button
      type="button"
      class="tool"
      title={tip("Import Files", "file.import-files")}
      aria-label="Import Files"
      disabled={!can("file.import-files")}
      onclick={() => run("file.import-files")}
    >
      <svg viewBox="0 0 16 16" aria-hidden="true">
        <path d="M3 1.5h6l4 4v9H3z" fill="none" stroke="currentColor" />
        <path d="M8 7v5M5.5 9.5h5" stroke="currentColor" stroke-width="1.4" />
      </svg>
    </button>
    <button
      type="button"
      class="tool"
      title={tip("Import Folder", "file.import-folder")}
      aria-label="Import Folder"
      disabled={!can("file.import-folder")}
      onclick={() => run("file.import-folder")}
    >
      <svg viewBox="0 0 16 16" aria-hidden="true">
        <path d="M1.5 3.5h5l1.5 1.5h6.5v8h-13z" fill="none" stroke="currentColor" />
        <path d="M8 7v4.5M5.75 9.25h4.5" stroke="currentColor" stroke-width="1.4" />
      </svg>
    </button>
    <button
      type="button"
      class="tool"
      title={pickedSession ? "Close the picked session (asks to save first)" : tip("Remove from Session", "edit.delete")}
      aria-label={pickedSession ? "Close Session" : "Remove from Session"}
      disabled={!counts.selected && !pickedSession}
      onclick={() => (pickedSession ? onCloseSession(pickedSession) : run("edit.delete"))}
    >
      <svg viewBox="0 0 16 16" aria-hidden="true">
        <path d="M3.5 8h9" stroke="currentColor" stroke-width="1.6" />
      </svg>
    </button>
    <input
      type="search"
      class="search"
      placeholder="Search"
      aria-label="Search files"
      bind:value={query}
      onkeydown={(e) => {
        if (e.key === "Escape") query = "";
      }}
    />
  </div>

  <div class="head" role="row">
    <button type="button" class="col name" data-sort={ariaSort("name")} onclick={() => sortBy("name")}>Name</button>
    <button type="button" class="col status" data-sort={ariaSort("status")} onclick={() => sortBy("status")}>Status</button>
    <button type="button" class="col num duration" data-sort={ariaSort("duration")} onclick={() => sortBy("duration")}
      >Duration</button
    >
    <button type="button" class="col num rate" data-sort={ariaSort("rate")} onclick={() => sortBy("rate")}>Sample Rate</button>
    <button type="button" class="col channels" data-sort={ariaSort("channels")} onclick={() => sortBy("channels")}
      >Channels</button
    >
  </div>

  <div
    class="rows"
    role="listbox"
    aria-label="Session files"
    aria-multiselectable="true"
    tabindex="0"
    bind:this={list}
    onkeydown={onListKey}
    onpointerdown={onBlankPointer}
  >
    {#if pickBox}<div class="pick-box" style:left="{pickBox.x}px" style:top="{pickBox.y}px" style:width="{pickBox.w}px" style:height="{pickBox.h}px"></div>{/if}
    {#each sessionRows as sr (sr.id)}
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <div
        class="row session"
        class:active={sr.active}
        class:behind={!sr.front}
        role="option"
        tabindex="-1"
        aria-selected={pickedSession === sr.id}
        title={sr.front ? "The multitrack session in front. Double-click to show it in the Multitrack view; Delete closes it." : "An open session set aside. Double-click to bring it to the front; Delete closes it."}
        onclick={() => {
          pickedSession = sr.id;
          pool.setSelection([]);
          list?.focus();
        }}
        ondblclick={() => onOpenSession(sr.id)}
        oncontextmenu={(e) => {
          e.preventDefault();
          e.stopPropagation();
          pickedSession = sr.id;
          pool.setSelection([]);
          sessionMenu = { x: e.clientX, y: e.clientY, id: sr.id };
        }}
      >
        <span class="col name"><span class="sicon" aria-hidden="true">▤</span>{sr.name}{sr.dirty ? " *" : ""}</span>
        <span class="col status ready"><span class="dot" aria-hidden="true"></span>{sr.front ? "Session" : "Open"}</span>
        <span class="col num duration">{sessionDuration(sr.seconds)}</span>
        <span class="col num rate">{formatRate(sr.sampleRate)}</span>
        <span class="col channels">
          <button
            type="button"
            class="sclose"
            title="Close {sr.name} (asks to save first)"
            aria-label="Close {sr.name}"
            onclick={(e) => {
              e.stopPropagation();
              onCloseSession(sr.id);
            }}>×</button
          >
        </span>
      </div>
    {/each}
    {#each visible as it (it.id)}
      <div
        class="row"
        role="option"
        tabindex="-1"
        aria-selected={snap?.selected.has(it.id) ?? false}
        data-id={it.id}
        title={menu ? undefined : rowTitle(it)}
        onpointerdown={(e) => onRowPointer(e, it)}
        ondblclick={() => it.status === "ready" && onOpen(it.path)}
      >
        <span class="col name">{it.name}</span>
        <span class="col status {it.status}">
          <span class="dot" aria-hidden="true"></span>{statusText(it)}
        </span>
        <span class="col num duration">{it.info ? formatDuration(it.info) : ""}</span>
        <span class="col num rate">{it.info ? formatRate(it.info.sampleRate) : ""}</span>
        <span class="col channels">{it.info ? formatChannels(it.info.channels) : ""}</span>
      </div>
    {/each}

    {#if counts.total === 0}
      <p class="empty">Drag audio files or folders here from {fileManagerName()}, or use Import.</p>
    {:else if visible.length === 0}
      <p class="empty">No files match.</p>
    {/if}
  </div>

  <div class="preview" role="toolbar" aria-label="Preview">
    <button
      type="button"
      class="tool"
      class:on={!!playingPath}
      title={playingPath ? "Stop Preview" : "Play Preview of the selected file"}
      aria-label={playingPath ? "Stop preview" : "Play preview"}
      disabled={!playingPath && !single}
      onclick={togglePreview}
    >
      {#if playingPath}
        <svg viewBox="0 0 16 16" aria-hidden="true"><rect x="4" y="4" width="8" height="8" rx="1" fill="currentColor" /></svg>
      {:else}
        <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M5 3v10l8-5z" fill="currentColor" /></svg>
      {/if}
    </button>
    <button
      type="button"
      class="tool"
      class:on={loop}
      title="Loop Preview"
      aria-label="Loop preview"
      aria-pressed={loop}
      onclick={toggleLoop}
    >
      <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4 6.5h7.5l-2-2M12 9.5H4.5l2 2" fill="none" stroke="currentColor" stroke-width="1.4" /></svg>
    </button>
    <button
      type="button"
      class="tool"
      class:on={autoPlay}
      title="Auto-Play: preview a file when you select it"
      aria-label="Auto-play preview"
      aria-pressed={autoPlay}
      onclick={toggleAutoPlay}
    >
      <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M2 6h2.5L8 3v10L4.5 10H2z" fill="currentColor" /><path d="M10.5 5.5a3.5 3.5 0 0 1 0 5M12.5 3.5a6 6 0 0 1 0 9" fill="none" stroke="currentColor" stroke-width="1.2" /></svg>
    </button>
  </div>

  <footer class="foot">
    <span>{counts.total} {counts.total === 1 ? "file" : "files"}</span>
    {#if counts.selected > 0}<span>{counts.selected} selected</span>{/if}
    {#if counts.reading > 0}<span class="busy">Reading {counts.reading}</span>{/if}
    {#if counts.errors > 0}<span class="bad">{counts.errors} with errors</span>{/if}
  </footer>

  {#if dropping}
    <div class="dropzone" aria-hidden="true">Drop to import</div>
  {/if}
</section>

{#if menu}
  <ContextMenu x={menu.x} y={menu.y} items={menuItems} onClose={() => (menu = null)} />
{/if}
{#if sessionMenu}
  <ContextMenu x={sessionMenu.x} y={sessionMenu.y} items={sessionItems(sessionMenu.id)} onClose={() => (sessionMenu = null)} />
{/if}

<style>
  .row.session .name {
    font-weight: 600;
  }
  .row.session.active .name {
    color: var(--accent);
  }
  .row.session.behind .name {
    font-weight: normal;
    color: var(--text-dim);
  }
  .row.session[aria-selected="true"] {
    background: var(--selection, rgba(63, 169, 245, 0.18));
  }
  .sclose {
    visibility: hidden;
    width: 16px;
    height: 16px;
    padding: 0;
    border: none;
    border-radius: 2px;
    background: transparent;
    color: var(--text-dim);
    font: inherit;
    line-height: 14px;
    cursor: pointer;
  }
  .row.session:hover .sclose,
  .row.session[aria-selected="true"] .sclose {
    visibility: visible;
  }
  .sclose:hover {
    background: var(--surface-3, #333);
    color: var(--text);
  }
  .sicon {
    margin-right: 5px;
    color: var(--text-dim);
  }
  .panel {
    position: relative;
    display: grid;
    grid-template-rows: auto auto 1fr auto auto;
    grid-template-columns: minmax(0, 1fr);
    height: 100%;
    overflow: hidden;
    min-height: 0;
    container-type: inline-size;
    background: var(--surface-1);
  }

  .toolbar {
    display: flex;
    gap: 2px;
    align-items: center;
    padding: 3px 4px;
    border-bottom: 1px solid var(--border);
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

  .tool.on {
    color: var(--accent);
  }

  .preview {
    display: flex;
    gap: 4px;
    justify-content: center;
    padding: 2px 4px;
    border-top: 1px solid var(--border);
  }

  .search {
    flex: 1;
    min-width: 60px;
    height: 20px;
    margin-left: 4px;
    padding: 0 6px;
    border: 1px solid var(--border);
    border-radius: 2px;
    background: var(--surface-0);
    color: var(--text);
    font: inherit;
  }

  .search:focus {
    border-color: var(--accent-dim);
    outline: none;
  }

  .head,
  .row {
    display: grid;
    grid-template-columns: minmax(80px, 1fr) 64px 78px 86px 62px;
    align-items: center;
  }

  .head {
    border-bottom: 1px solid var(--border);
    background: var(--surface-2);
  }

  .head .col {
    height: 20px;
    padding: 0 6px;
    border: 0;
    border-right: 1px solid var(--border);
    background: none;
    color: var(--text-dim);
    font: inherit;
    font-size: var(--font-size-small);
    text-align: left;
    white-space: nowrap;
  }

  .head .col:hover {
    color: var(--text);
  }

  .head .col[data-sort="ascending"]::after {
    content: " \25B4";
  }

  .head .col[data-sort="descending"]::after {
    content: " \25BE";
  }

  .rows {
    position: relative;
    overflow: auto;
    min-height: 0;
    outline: none;
  }

  .row {
    height: 20px;
    border-bottom: 1px solid var(--surface-0);
    cursor: default;
    user-select: none;
  }

  .row .col {
    overflow: hidden;
    padding: 0 6px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .row:hover {
    background: var(--surface-2);
  }

  .row[aria-selected="true"] {
    background: var(--accent-dim);
    color: #fff;
  }

  .rows:focus-visible .row[aria-selected="true"] {
    outline: 1px solid var(--accent);
    outline-offset: -1px;
  }

  .num {
    font-family: var(--font-mono);
    font-size: var(--font-size-small);
    font-variant-numeric: tabular-nums;
    text-align: right;
  }

  .head .num {
    text-align: right;
  }

  .status {
    display: flex;
    gap: 5px;
    align-items: center;
    font-size: var(--font-size-small);
  }

  .dot {
    flex: none;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--text-faint);
  }

  .status.ready .dot {
    background: var(--clip-1);
  }

  .status.probing .dot {
    background: var(--playhead);
    animation: pulse 0.9s ease-in-out infinite alternate;
  }

  .status.error {
    color: var(--danger);
  }

  .status.error .dot {
    background: var(--danger);
  }

  .row[aria-selected="true"] .status.error {
    color: #ffd0cc;
  }

  @keyframes pulse {
    to {
      opacity: 0.25;
    }
  }

  .empty {
    margin: 0;
    padding: 16px 12px;
    color: var(--text-faint);
    text-align: center;
  }

  .foot {
    display: flex;
    gap: 10px;
    align-items: center;
    height: 20px;
    padding: 0 6px;
    border-top: 1px solid var(--border);
    color: var(--text-faint);
    font-size: var(--font-size-small);
    white-space: nowrap;
    overflow: hidden;
  }

  .busy {
    color: var(--playhead);
  }

  .bad {
    color: var(--danger);
  }

  .dropzone {
    position: absolute;
    inset: 2px;
    display: grid;
    place-items: center;
    border: 2px dashed var(--accent);
    background: rgba(63, 169, 245, 0.12);
    color: var(--text);
    font-weight: 600;
    pointer-events: none;
  }

  /* Narrow left column: Name, Status, Duration only. */
  @container (max-width: 380px) {
    .head,
    .row {
      grid-template-columns: minmax(60px, 1fr) 64px 78px;
    }

    .rate,
    .channels {
      display: none;
    }
  }

  @container (max-width: 200px) {
    .head,
    .row {
      grid-template-columns: minmax(60px, 1fr) 64px;
    }

    .duration {
      display: none;
    }
  }
  .pick-box {
    position: absolute;
    z-index: 2;
    border: 1px dashed var(--accent);
    background: color-mix(in srgb, var(--accent) 15%, transparent);
    pointer-events: none;
  }
</style>
