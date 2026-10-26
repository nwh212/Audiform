<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  Libraries panel. One library per show holds reusable assets (intros,
  ads, promos, beds, legal reads), sorted into colored bins. Drop files
  here to copy them in; drag items onto a bin to file them, or out to
  the Files panel and onto tracks.
-->
<script lang="ts">
  import { onDestroy } from "svelte";
  import ContextMenu, { type ContextItem } from "../../app/ContextMenu.svelte";
  import PromptDialog from "../../app/PromptDialog.svelte";
  import ClipColorDialog from "../../app/ClipColorDialog.svelte";
  import ChoiceDialog from "../../app/ChoiceDialog.svelte";
  import type { PeakStore } from "../../stores/peaks";
  import { peakRange, pickLevel } from "../editor/timeline";
  import { binsOf, defaultColor, initials, missingStarterBins, STARTER_BINS } from "./bins";
  import {
    libraryCleanUp,
    libraryPickFolder,
    pickMediaFiles,
    previewPlay,
    previewStatus,
    previewStop,
    revealMedia,
    type LibraryItem,
  } from "../../app/core";
  import type { FileDropRouter } from "../../app/filedrop";
  import { beginPathDrag } from "../../app/pathdrag";
  import { revealLabel } from "../../lib/platform";
  import { itemPath, type LibrariesState, type LibraryStore } from "../../stores/libraries";
  import type { CommandRunner } from "../../stores/media-pool";
  import { clickSelection, stepSelection } from "../files/selection";
  import { formatChannels } from "../files/format";

  interface Props {
    store: LibraryStore;
    runner: CommandRunner;
    router: FileDropRouter;
    /** Adds files to the session's Files panel. */
    onAddToSession: (paths: string[]) => void;
    onNotice: (text: string) => void;
    onError?: (e: unknown) => void;
    shortcutFor?: (id: string) => string | undefined;
    /** Waveform peaks for the small previews on each row. */
    peaks?: PeakStore | null;
  }

  const {
    store,
    runner,
    router,
    onAddToSession,
    onNotice,
    onError = console.error,
    shortcutFor = () => undefined,
    peaks = null,
  }: Props = $props();

  const COLORS = ["--clip-1", "--clip-2", "--clip-3", "--clip-4", "--clip-5", "--clip-6"];

  let snap = $state<LibrariesState | null>(null);
  $effect(() =>
    store.subscribe((s) => {
      snap = s;
    }),
  );

  let query = $state("");
  let anchor = $state<string | null>(null);
  let dropping = $state(false);
  let busy = $state(false);
  let menu = $state<{ x: number; y: number } | null>(null);
  let prompt = $state<{ title: string; label: string; value: string; confirm: string; done: (v: string) => void } | null>(
    null,
  );
  let root = $state<HTMLElement | null>(null);
  let list = $state<HTMLElement | null>(null);

  let playingId = $state<string | null>(null);
  /** Bin under the pointer while dragging items, to show where they will go. */
  let hoverBin = $state<string | null>(null);
  let colorFor = $state<{ kind: "bin"; id: string; current: string | null } | { kind: "show"; current: string | null } | null>(null);
  let confirmClean = $state<{ dir: string; count: number } | null>(null);
  let binMenu = $state<{ x: number; y: number; id: string } | null>(null);
  let libMenu = $state<{ x: number; y: number } | null>(null);
  let poll: ReturnType<typeof setInterval> | undefined;

  $effect(() => {
    if (!root) return;
    return router.register({
      element: root,
      onDrop: (paths) => void addFiles(paths),
      onHover: (over) => (dropping = over),
    });
  });

  const active = $derived(snap?.active ?? null);
  const shown = $derived.by(() => {
    void snap;
    const items = store.shownItems();
    const q = query.trim().toLowerCase();
    if (!q) return items;
    return items.filter((i) =>
      q.split(/\s+/).every(
        (w) => i.name.toLowerCase().includes(w) || i.notes.toLowerCase().includes(w) || i.tags.some((t) => t.toLowerCase().includes(w)),
      ),
    );
  });
  const shownIds = $derived(shown.map((i) => i.id));
  const selectedItems = $derived(shown.filter((i) => snap?.selected.has(i.id)));
  const single = $derived(selectedItems.length === 1 ? selectedItems[0]! : null);
  const currentCollection = $derived(active?.lib.collections.find((c) => c.id === snap?.collection) ?? null);

  function pathOf(item: LibraryItem): string {
    return active ? itemPath(active.dir, item) : item.file;
  }

  function fail(e: unknown) {
    onError(e);
  }

  async function addFiles(paths: string[]) {
    if (!active) {
      onNotice("Create or open a library first.");
      return;
    }
    busy = true;
    try {
      const r = await store.importPaths(paths, runner);
      if (r.skipped.length > 0) {
        onNotice(
          r.skipped.length === 1
            ? `Skipped ${r.skipped[0]!.path}: ${r.skipped[0]!.reason}`
            : `Skipped ${r.skipped.length} files that could not be read.`,
        );
      }
    } catch (e) {
      fail(e);
    } finally {
      busy = false;
    }
  }

  function ask(title: string, label: string, value: string, confirm: string, done: (v: string) => void) {
    prompt = { title, label, value, confirm, done };
  }

  function newLibrary() {
    ask("New Show Library", "Show name", "", "Create", (name) => void store.create(name, undefined, STARTER_BINS).catch(fail));
  }

  async function openLibrary() {
    try {
      const dir = await libraryPickFolder("Open Library");
      if (dir) await store.openDir(dir);
    } catch (e) {
      fail(e);
    }
  }

  /** Counts unused copied files first and asks before deleting anything. */
  async function cleanUp() {
    if (!active) return;
    const a = active;
    try {
      const n = await libraryCleanUp(a.dir, a.lib, true);
      if (n === 0) {
        onNotice("Nothing to clean up.");
        return;
      }
      confirmClean = { dir: a.dir, count: n };
    } catch (e) {
      fail(e);
    }
  }

  async function doCleanUp() {
    const c = confirmClean;
    confirmClean = null;
    const lib = snap?.libs.find((l) => l.dir === c?.dir)?.lib;
    if (!c || !lib) return;
    try {
      const n = await libraryCleanUp(c.dir, lib, false);
      onNotice(`Deleted ${n} unused audio ${n === 1 ? "file" : "files"} from the library folder.`);
    } catch (e) {
      fail(e);
    }
  }

  function onRowPointer(e: PointerEvent, item: LibraryItem) {
    if (!snap) return;
    if (e.button === 2 && snap.selected.has(item.id)) return;
    if (e.button !== 0 && e.button !== 2) return;
    if (!(e.button === 0 && snap.selected.has(item.id) && !e.ctrlKey && !e.shiftKey)) {
      const next = clickSelection(snap.selected, anchor, item.id, shownIds, { ctrl: e.ctrlKey || e.metaKey, shift: e.shiftKey });
      anchor = next.anchor;
      store.setSelection(next.selected);
    }
    if (e.button === 0) {
      trackBinDrop(e);
      beginPathDrag(
        e,
        router,
        () => store.selectedItems().map(pathOf),
        (n) => (n === 1 ? "1 item" : `${n} items`),
        root ?? undefined,
      );
    }
    list?.focus();
  }

  /** While items are dragged, a bin under the pointer lights up; dropping there files them. */
  function trackBinDrop(e: PointerEvent) {
    const startX = e.clientX;
    const startY = e.clientY;
    const binAt = (x: number, y: number) =>
      (document.elementFromPoint(x, y)?.closest("[data-bin]") as HTMLElement | null)?.dataset.bin ?? null;
    const move = (ev: PointerEvent) => {
      if (Math.abs(ev.clientX - startX) + Math.abs(ev.clientY - startY) < 4) return;
      hoverBin = binAt(ev.clientX, ev.clientY);
    };
    const up = (ev: PointerEvent) => {
      window.removeEventListener("pointermove", move, true);
      window.removeEventListener("pointerup", up, true);
      const bin = hoverBin ?? binAt(ev.clientX, ev.clientY);
      hoverBin = null;
      if (Math.abs(ev.clientX - startX) + Math.abs(ev.clientY - startY) < 4 || !bin || bin === "all") return;
      const ids = store.selectedItems().map((i) => i.id);
      const name = active?.lib.collections.find((c) => c.id === bin)?.name ?? "bin";
      if (ids.length) void store.addToCollection(bin, ids, runner).then(() => onNotice(`Added ${ids.length === 1 ? "1 item" : `${ids.length} items`} to ${name}.`), fail);
    };
    window.addEventListener("pointermove", move, true);
    window.addEventListener("pointerup", up, true);
  }

  function binItems(id: string | null): ContextItem[] {
    const c = active?.lib.collections.find((x) => x.id === id);
    if (!c || !active) return [];
    const i = active.lib.collections.indexOf(c);
    return [
      { kind: "item", label: "Show Bin", run: () => store.showCollection(c.id) },
      { kind: "sep" },
      { kind: "item", label: "Rename Bin...", run: () => ask("Rename Bin", "Bin name", c.name, "Rename", (n) => void store.renameCollection(c.id, n, runner).catch(fail)) },
      { kind: "item", label: "Bin Color...", run: () => (colorFor = { kind: "bin", id: c.id, current: c.color ?? null }) },
      { kind: "item", label: "Move Left", disabled: i === 0, run: () => void store.moveCollection(c.id, i - 1, runner).catch(fail) },
      { kind: "item", label: "Move Right", disabled: i === active.lib.collections.length - 1, run: () => void store.moveCollection(c.id, i + 1, runner).catch(fail) },
      { kind: "sep" },
      { kind: "item", label: "Delete Bin", run: () => void store.deleteCollection(c.id, runner).catch(fail) },
    ];
  }

  function libraryItems(): ContextItem[] {
    const out: ContextItem[] = (snap?.libs ?? []).map((l) => ({
      kind: "item" as const,
      label: l.dir === active?.dir ? `${l.lib.name}  (shown)` : l.lib.name,
      run: () => store.show(l.dir),
    }));
    out.push(
      { kind: "sep" },
      { kind: "item", label: "New Show Library...", run: newLibrary },
      { kind: "item", label: "Open Library...", run: () => void openLibrary() },
    );
    return out;
  }

  /** Plays one item from its row button; a second press stops it. */
  async function previewItem(item: LibraryItem) {
    if (playingId === item.id) {
      await togglePreview();
      return;
    }
    if (playingId) await togglePreview();
    store.setSelection([item.id]);
    try {
      await previewPlay(pathOf(item));
      playingId = item.id;
      clearInterval(poll);
      poll = setInterval(async () => {
        const st = await previewStatus().catch(() => null);
        if (!st?.playing) {
          playingId = null;
          clearInterval(poll);
        }
      }, 150);
    } catch (e) {
      fail(e);
    }
  }

  /** Draws a small waveform of the whole file into a canvas. */
  function miniWave(canvas: HTMLCanvasElement, path: string) {
    let current = path;
    const draw = () => {
      if (!peaks) return;
      const e = peaks.get(current);
      const g = canvas.getContext("2d");
      if (!g) return;
      const dpr = window.devicePixelRatio || 1;
      const w = canvas.clientWidth || 56;
      const h = canvas.clientHeight || 16;
      canvas.width = Math.round(w * dpr);
      canvas.height = Math.round(h * dpr);
      g.setTransform(dpr, 0, 0, dpr, 0, 0);
      g.clearRect(0, 0, w, h);
      // A failed waveform says why on hover instead of staying blank.
      canvas.title = e.status === "error" ? `No waveform: ${e.error ?? "unknown error"}` : "";
      if (e.status === "error") {
        g.fillStyle = getComputedStyle(canvas).getPropertyValue("--danger") || "#d55";
        g.fillRect(0, h / 2 - 1, w, 2);
        return;
      }
      if (e.status !== "ready" || !e.summary) return;
      const ch = e.summary.channels;
      const total = e.summary.length;
      const level = pickLevel(e.levels, total / w);
      if (!level || total <= 0 || level.data.length === 0) {
        canvas.title = `No waveform data (${e.levels.length} levels, length ${total})`;
        return;
      }
      g.fillStyle = getComputedStyle(canvas).color;
      for (let x = 0; x < w; x++) {
        const s0 = (x * total) / w;
        const [lo, hi] = peakRange(level, ch, 0, s0, s0 + total / w);
        const y0 = h / 2 - hi * (h / 2);
        const y1 = h / 2 - lo * (h / 2);
        g.fillRect(x, y0, 1, Math.max(1, y1 - y0));
      }
    };
    const off = peaks?.subscribe(draw);
    draw();
    return {
      update(p: string) {
        current = p;
        draw();
      },
      destroy() {
        off?.();
      },
    };
  }

  function onListKey(e: KeyboardEvent) {
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const id = stepSelection(shownIds, anchor, e.key === "ArrowDown" ? 1 : -1);
      if (id) {
        anchor = id;
        store.setSelection([id]);
        list?.querySelector<HTMLElement>(`[data-id="${CSS.escape(id)}"]`)?.scrollIntoView({ block: "nearest" });
      }
    } else if (e.key === " ") {
      e.preventDefault();
      togglePreview();
    } else if (e.key === "Enter") {
      e.preventDefault();
      addSelectedToSession();
    }
  }

  function addSelectedToSession() {
    const paths = store.selectedItems().map(pathOf);
    if (paths.length) onAddToSession(paths);
  }

  async function togglePreview() {
    if (playingId) {
      clearInterval(poll);
      playingId = null;
      await previewStop().catch(fail);
      return;
    }
    if (!single) return;
    try {
      await previewPlay(pathOf(single));
      playingId = single.id;
      clearInterval(poll);
      poll = setInterval(async () => {
        const s = await previewStatus().catch(() => null);
        if (!s?.playing) {
          playingId = null;
          clearInterval(poll);
        }
      }, 150);
    } catch (e) {
      fail(e);
    }
  }

  function duration(i: LibraryItem): string {
    const secs = i.sample_rate > 0 ? i.length / i.sample_rate : 0;
    const m = Math.floor(secs / 60);
    const s = Math.floor(secs % 60);
    return `${m}:${String(s).padStart(2, "0")}`;
  }

  function commit(field: "name" | "tags" | "notes", raw: string) {
    if (!single) return;
    const item = single;
    if (field === "name") {
      const v = raw.trim();
      if (v && v !== item.name) void store.updateItem(item.id, { name: v }, runner).catch(fail);
    } else if (field === "tags") {
      const tags = raw
        .split(",")
        .map((t) => t.trim())
        .filter(Boolean);
      if (tags.join(",") !== item.tags.join(",")) void store.updateItem(item.id, { tags }, runner).catch(fail);
    } else if (raw !== item.notes) {
      void store.updateItem(item.id, { notes: raw }, runner).catch(fail);
    }
  }

  function setColor(color: string | null) {
    if (single && single.color !== color) void store.updateItem(single.id, { color }, runner).catch(fail);
  }

  const menuItems = $derived.by((): ContextItem[] => {
    const sel = selectedItems;
    const ids = sel.map((i) => i.id);
    const out: ContextItem[] = [];
    if (active) {
      out.push(
        { kind: "item", label: playingId ? "Stop Preview" : "Preview", run: () => void togglePreview(), disabled: !playingId && !single, shortcut: "Space" },
        { kind: "item", label: "Add to Session", run: addSelectedToSession, disabled: sel.length === 0, shortcut: "Enter" },
        { kind: "sep" },
      );
      for (const c of active.lib.collections) {
        if (c.id === snap?.collection) continue;
        out.push({
          kind: "item",
          label: `Add to Bin: ${c.name}`,
          run: () => void store.addToCollection(c.id, ids, runner).catch(fail),
          disabled: sel.length === 0,
        });
      }
      out.push({
        kind: "item",
        label: "New Bin from Selection...",
        disabled: sel.length === 0,
        run: () =>
          ask("New Bin", "Bin name", "", "Create", (name) =>
            void store.addCollection(name, runner, ids).catch(fail),
          ),
      });
      if (currentCollection) {
        out.push({
          kind: "item",
          label: `Remove from Bin: ${currentCollection.name}`,
          disabled: sel.length === 0,
          run: () => void store.removeFromCollection(currentCollection.id, ids, runner).catch(fail),
        });
      }
      out.push(
        { kind: "item", label: "Remove from Library", disabled: sel.length === 0, shortcut: shortcutFor("edit.delete"), run: () => void store.removeItems(ids, runner).catch(fail) },
        { kind: "item", label: revealLabel(), disabled: !single, run: () => single && void revealMedia(pathOf(single)).catch(fail) },
        { kind: "sep" },
        { kind: "item", label: "Add Files to Library...", run: () => void pickMediaFiles().then((p) => (p.length ? addFiles(p) : undefined)).catch(fail) },
        {
          kind: "item",
          label: "New Bin...",
          run: () => ask("New Bin", "Bin name", "", "Create", (name) => void store.addCollection(name, runner).catch(fail)),
        },
      );
      if (currentCollection) {
        const c = currentCollection;
        out.push(
          { kind: "item", label: "Rename Bin...", run: () => ask("Rename Bin", "Bin name", c.name, "Rename", (n) => void store.renameCollection(c.id, n, runner).catch(fail)) },
          { kind: "item", label: "Delete Bin", run: () => void store.deleteCollection(c.id, runner).catch(fail) },
        );
      }
      out.push({ kind: "sep" });
    }
    out.push(
      { kind: "item", label: "New Show Library...", run: newLibrary },
      { kind: "item", label: "Open Library...", run: () => void openLibrary() },
    );
    if (active) {
      const a = active;
      out.push(
        { kind: "item", label: "Rename Library...", run: () => ask("Rename Library", "Library name", a.lib.name, "Rename", (n) => void store.renameLibrary(n, runner).catch(fail)) },
        { kind: "item", label: "Show Color...", run: () => (colorFor = { kind: "show", current: a.lib.color ?? null }) },
        {
          kind: "item",
          label: "Add Starter Bins",
          disabled: missingStarterBins(a.lib).length === 0,
          run: () => void store.addBins(missingStarterBins(a.lib), runner).catch(fail),
        },
        { kind: "item", label: "Clean Up Library Folder", run: () => void cleanUp() },
        { kind: "item", label: "Close Library", run: () => store.close(a.dir) },
      );
    }
    return out;
  });

  function openMenu(e: MouseEvent) {
    e.preventDefault();
    menu = { x: e.clientX, y: e.clientY };
  }

  onDestroy(() => {
    clearInterval(poll);
    if (playingId) void previewStop();
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<section class="panel" class:dropping aria-label="Libraries" data-area="libraries" bind:this={root} oncontextmenu={openMenu}>
  {#if !snap || snap.libs.length === 0}
    <div class="start">
      <p>Keep intros, beds, promos and legal reads in a library and reuse them in every episode.</p>
      <button type="button" onclick={newLibrary}>New Library...</button>
      <button type="button" onclick={() => void openLibrary()}>Open Library...</button>
    </div>
  {:else}
    {@const lib = active?.lib}
    {@const badge = lib?.color ?? (lib ? defaultColor(lib.name) : "--clip-2")}
    <header class="show">
      <button
        type="button"
        class="badge"
        style:background="var({badge})"
        title="Show color"
        aria-label="Show color"
        onclick={() => lib && (colorFor = { kind: "show", current: lib.color ?? null })}>{initials(lib?.name ?? "")}</button
      >
      <button
        type="button"
        class="title"
        title="Switch library"
        onclick={(e) => {
          const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
          libMenu = { x: r.left, y: r.bottom };
        }}
      >
        <span class="name">{lib?.name ?? "No library"}</span>
        <span class="sub">{lib ? `${lib.items.length} ${lib.items.length === 1 ? "item" : "items"} · ${lib.collections.length} ${lib.collections.length === 1 ? "bin" : "bins"}` : ""}</span>
        <svg class="caret" viewBox="0 0 16 16" aria-hidden="true"><path d="M4 6l4 4 4-4" fill="none" stroke="currentColor" stroke-width="1.5" /></svg>
      </button>
      <button type="button" class="tool" title="Library menu" aria-label="Library menu" onclick={(e) => {
        const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
        menu = { x: r.left, y: r.bottom };
      }}>
        <svg viewBox="0 0 16 16" aria-hidden="true"><circle cx="8" cy="3.5" r="1.3" /><circle cx="8" cy="8" r="1.3" /><circle cx="8" cy="12.5" r="1.3" /></svg>
      </button>
    </header>

    <div class="bins" role="tablist" aria-label="Bins">
      <button
        type="button"
        role="tab"
        class="bin"
        class:on={snap.collection === null}
        data-bin="all"
        aria-selected={snap.collection === null}
        onclick={() => store.showCollection(null)}
      >
        All <span class="count">{lib?.items.length ?? 0}</span>
      </button>
      {#each lib?.collections ?? [] as c (c.id)}
        <button
          type="button"
          role="tab"
          class="bin"
          class:on={snap.collection === c.id}
          class:target={hoverBin === c.id}
          data-bin={c.id}
          aria-selected={snap.collection === c.id}
          onclick={() => store.showCollection(c.id)}
          oncontextmenu={(e) => {
            e.preventDefault();
            e.stopPropagation();
            binMenu = { x: e.clientX, y: e.clientY, id: c.id };
          }}
        >
          <i style:background={c.color ? `var(${c.color})` : "var(--text-faint)"}></i>{c.name} <span class="count">{c.items.length}</span>
        </button>
      {/each}
      <button
        type="button"
        class="bin add"
        title="New bin"
        aria-label="New bin"
        onclick={() => ask("New Bin", "Bin name", "", "Create", (name) => void store.addCollection(name, runner).catch(fail))}>+</button
      >
      {#if lib && lib.collections.length === 0}
        <button type="button" class="bin starter" onclick={() => void store.addBins(missingStarterBins(lib), runner).catch(fail)}>
          Add starter bins
        </button>
      {/if}
    </div>

    <div class="bar">
      <input type="search" placeholder="Search name, tag, note" aria-label="Search library" bind:value={query} />
    </div>

    <div
      class="rows"
      role="listbox"
      aria-label="Library items"
      aria-multiselectable="true"
      tabindex="0"
      bind:this={list}
      onkeydown={onListKey}
    >
      {#each shown as it (it.id)}
        {@const inBins = active ? binsOf(active.lib, it.id) : []}
        {@const stripe = it.color ?? inBins.find((b) => b.color)?.color ?? null}
        <div
          class="row"
          role="option"
          tabindex="-1"
          aria-selected={snap.selected.has(it.id)}
          data-id={it.id}
          title={menu ? undefined : [it.name, inBins.length ? `Bins: ${inBins.map((b) => b.name).join(", ")}` : "", it.tags.length ? `Tags: ${it.tags.join(", ")}` : "", it.notes].filter(Boolean).join("\n")}
          onpointerdown={(e) => onRowPointer(e, it)}
          ondblclick={() => onAddToSession([pathOf(it)])}
        >
          <span class="stripe" style:background={stripe ? `var(${stripe})` : "transparent"}></span>
          <button
            type="button"
            class="play"
            class:on={playingId === it.id}
            aria-label={playingId === it.id ? "Stop preview" : "Preview"}
            onpointerdown={(e) => e.stopPropagation()}
            onclick={(e) => {
              e.stopPropagation();
              void previewItem(it);
            }}
          >
            {#if playingId === it.id}
              <svg viewBox="0 0 16 16" aria-hidden="true"><rect x="4" y="4" width="8" height="8" /></svg>
            {:else}
              <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M5 3.5v9l7-4.5z" /></svg>
            {/if}
          </button>
          <span class="text">
            <span class="name" class:playing={playingId === it.id}>{it.name}</span>
            {#if inBins.length > 0 || it.tags.length > 0}
              <span class="tags">
                {#each inBins as b (b.id)}<span class="dot" style:background={b.color ? `var(${b.color})` : "var(--text-faint)"} title={b.name}></span>{/each}
                {#if it.tags.length}<span class="tag">{it.tags.slice(0, 2).join(" · ")}</span>{/if}
              </span>
            {/if}
          </span>
          {#if peaks}<canvas class="wave" use:miniWave={pathOf(it)} aria-hidden="true"></canvas>{/if}
          <span class="dur">{duration(it)}</span>
        </div>
      {/each}
      {#if shown.length === 0}
        <p class="empty">
          {#if busy}Adding...{:else if query}Nothing matches.{:else if currentCollection}This bin is empty. Drag items from All onto its name above to add them.{:else}Drop audio files here to copy them into this library.{/if}
        </p>
      {/if}
    </div>

    {#if single}
      {#key single.id}
        <div class="details">
          <input class="field" type="text" aria-label="Item name" value={single.name} onchange={(e) => commit("name", e.currentTarget.value)} />
          <input
            class="field"
            type="text"
            aria-label="Tags"
            placeholder="Tags, comma separated"
            value={single.tags.join(", ")}
            onchange={(e) => commit("tags", e.currentTarget.value)}
          />
          <textarea class="field" rows="2" aria-label="Notes" placeholder="Notes" value={single.notes} onchange={(e) => commit("notes", e.currentTarget.value)}></textarea>
          <div class="colors" role="radiogroup" aria-label="Color">
            <button type="button" class="chip none" role="radio" aria-checked={!single.color} aria-label="No color" onclick={() => setColor(null)}></button>
            {#each COLORS as c (c)}
              <button type="button" class="chip" role="radio" aria-checked={single.color === c} aria-label="Color" style:background="var({c})" onclick={() => setColor(c)}></button>
            {/each}
          </div>
          {#if active && active.lib.collections.length > 0}
            {@const it = single}
            <div class="binset" aria-label="Bins for this item">
              {#each active.lib.collections as c (c.id)}
                {@const inBin = c.items.includes(it.id)}
                <button
                  type="button"
                  class="binchip"
                  class:on={inBin}
                  aria-pressed={inBin}
                  onclick={() =>
                    void (inBin ? store.removeFromCollection(c.id, [it.id], runner) : store.addToCollection(c.id, [it.id], runner)).catch(fail)}
                >
                  <i style:background={c.color ? `var(${c.color})` : "var(--text-faint)"}></i>{c.name}
                </button>
              {/each}
            </div>
          {/if}
          <div class="meta">
            {single.sample_rate.toLocaleString("en-US")} Hz, {formatChannels(single.channels)}{single.copied ? "" : ", linked"}
          </div>
        </div>
      {/key}
    {/if}
  {/if}

  {#if dropping}
    <div class="dropzone" aria-hidden="true">{active ? `Add to ${active.lib.name}` : "Create a library first"}</div>
  {/if}
</section>

{#if menu}
  <ContextMenu x={menu.x} y={menu.y} items={menuItems} onClose={() => (menu = null)} />
{/if}

{#if confirmClean}
  <ChoiceDialog
    title="Clean Up Library Folder"
    message={`Delete ${confirmClean.count} audio ${confirmClean.count === 1 ? "file" : "files"} in this library's audio folder that no item uses? This cannot be undone. Other files are never touched.`}
    buttons={[
      { label: "Delete", value: "yes", primary: true },
      { label: "Cancel", value: "no" },
    ]}
    onChoose={(v) => (v === "yes" ? void doCleanUp() : (confirmClean = null))}
  />
{/if}

{#if libMenu}
  <ContextMenu x={libMenu.x} y={libMenu.y} items={libraryItems()} onClose={() => (libMenu = null)} />
{/if}

{#if binMenu}
  <ContextMenu x={binMenu.x} y={binMenu.y} items={binItems(binMenu.id)} onClose={() => (binMenu = null)} />
{/if}

{#if colorFor}
  <ClipColorDialog
    title={colorFor.kind === "bin" ? "Bin Color" : "Show Color"}
    noneLabel={colorFor.kind === "bin" ? "No Color" : "Automatic"}
    current={colorFor.current}
    onDone={(c) => {
      const target = colorFor;
      colorFor = null;
      if (!target || c === null) return;
      const value = c === "" ? null : c;
      if (target.kind === "bin") void store.setCollectionColor(target.id, value, runner).catch(fail);
      else void store.setLibraryColor(value, runner).catch(fail);
    }}
  />
{/if}

{#if prompt}
  <PromptDialog
    title={prompt.title}
    label={prompt.label}
    value={prompt.value}
    confirm={prompt.confirm}
    onDone={(v) => {
      const done = prompt?.done;
      prompt = null;
      if (v && done) done(v);
    }}
  />
{/if}

<style>
  .panel {
    position: relative;
    display: flex;
    flex-direction: column;
    height: 100%;
    min-width: 0;
    overflow: hidden;
    background: var(--surface-1);
  }

  .start {
    display: grid;
    gap: 8px;
    padding: 14px 10px;
    color: var(--text-dim);
    text-align: center;
  }

  .start p {
    margin: 0 0 4px;
    font-size: var(--font-size-small);
  }

  .bar {
    display: flex;
    gap: 3px;
    align-items: center;
    padding: 3px 4px;
    border-bottom: 1px solid var(--border);
  }

  input[type="search"],
  .field {
    flex: 1;
    min-width: 0;
    height: 20px;
    padding: 0 5px;
    border: 1px solid var(--border);
    border-radius: 2px;
    background: var(--surface-0);
    color: var(--text);
    font: inherit;
  }

  textarea.field {
    height: auto;
    padding: 3px 5px;
    resize: none;
  }

  input:focus,
  textarea:focus {
    border-color: var(--accent-dim);
    outline: none;
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
    fill: currentColor;
  }

  .tool:hover {
    border-color: var(--border-strong);
    color: var(--text);
  }

  .start button {
    height: 22px;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    background: var(--surface-3);
    color: var(--text);
    font: inherit;
  }

  .start button:hover {
    border-color: var(--accent);
  }

  .rows {
    flex: 1;
    min-height: 64px;
    overflow: auto;
    outline: none;
  }

  .row {
    display: grid;
    grid-template-columns: 3px 18px minmax(0, 1fr) 56px auto;
    gap: 6px;
    align-items: center;
    min-height: 30px;
    padding-right: 8px;
    border-bottom: 1px solid var(--surface-0);
    color: var(--text-dim);
    cursor: default;
    user-select: none;
  }

  .row:hover {
    background: var(--surface-2);
  }

  .row[aria-selected="true"] {
    background: var(--accent-dim);
    color: #e6f2fc;
  }

  .stripe {
    align-self: stretch;
  }

  .play {
    display: grid;
    place-items: center;
    width: 18px;
    height: 18px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: var(--surface-3);
    color: var(--text-dim);
    opacity: 0;
    cursor: pointer;
  }

  .play svg {
    width: 9px;
    height: 9px;
    fill: currentColor;
  }

  .row:hover .play,
  .row[aria-selected="true"] .play,
  .play.on {
    opacity: 1;
  }

  .play:hover,
  .play.on {
    background: var(--accent);
    color: #fff;
  }

  .text {
    display: grid;
    gap: 1px;
    min-width: 0;
  }

  .name {
    overflow: hidden;
    color: var(--text);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .row[aria-selected="true"] .name {
    color: #fff;
  }

  .name.playing,
  .row[aria-selected="true"] .name.playing {
    color: var(--playhead);
  }

  .tags {
    display: flex;
    gap: 3px;
    align-items: center;
    min-width: 0;
    overflow: hidden;
    font-size: 10px;
    white-space: nowrap;
  }

  .dot {
    flex: none;
    width: 6px;
    height: 6px;
    border-radius: 50%;
  }

  .tag {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .wave {
    width: 56px;
    height: 16px;
    color: var(--clip-2);
    opacity: 0.85;
  }

  .row[aria-selected="true"] .wave {
    color: #cfe6fb;
  }

  .dur {
    font-family: var(--font-mono);
    font-size: var(--font-size-small);
  }

  /* Show header: color badge with initials, name and counts, switcher. */
  .show {
    display: flex;
    gap: 8px;
    align-items: center;
    padding: 8px 6px 6px 8px;
  }

  .badge {
    flex: none;
    width: 30px;
    height: 30px;
    padding: 0;
    border: 0;
    border-radius: 6px;
    color: #101012;
    font: inherit;
    font-size: 12px;
    font-weight: 700;
    letter-spacing: 0.03em;
    cursor: pointer;
  }

  .title {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    flex: 1;
    min-width: 0;
    padding: 2px 4px;
    border: 0;
    border-radius: 3px;
    background: none;
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .title:hover {
    background: var(--surface-2);
  }

  .title .name {
    font-weight: 600;
  }

  .title .sub {
    grid-row: 2;
    color: var(--text-faint);
    font-size: 10px;
  }

  .caret {
    grid-row: 1 / 3;
    grid-column: 2;
    align-self: center;
    width: 12px;
    height: 12px;
    color: var(--text-dim);
  }

  /* Bin strip: chips that wrap, with a color dot and a count. */
  .bins {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    padding: 0 6px 6px;
  }

  .bin {
    display: inline-flex;
    gap: 5px;
    align-items: center;
    height: 22px;
    padding: 0 8px;
    border: 1px solid var(--border);
    border-radius: 11px;
    background: var(--surface-2);
    color: var(--text-dim);
    font: inherit;
    font-size: var(--font-size-small);
    white-space: nowrap;
    cursor: pointer;
  }

  .bin i {
    width: 7px;
    height: 7px;
    border-radius: 50%;
  }

  .bin .count {
    color: var(--text-faint);
    font-family: var(--font-mono);
    font-size: 10px;
  }

  .bin:hover {
    border-color: var(--border-strong);
    color: var(--text);
  }

  .bin.on {
    border-color: var(--accent);
    background: var(--accent-dim);
    color: #fff;
  }

  .bin.on .count {
    color: #cfe6fb;
  }

  .bin.target {
    border-color: var(--accent);
    box-shadow: 0 0 0 2px var(--accent);
    color: var(--text);
  }

  .bin.add {
    width: 22px;
    padding: 0;
    justify-content: center;
    font-size: 14px;
  }

  .bin.starter {
    border-style: dashed;
  }

  /* Bin membership toggles in the details area. */
  .binset {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }

  .binchip {
    display: inline-flex;
    gap: 4px;
    align-items: center;
    height: 20px;
    padding: 0 7px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: none;
    color: var(--text-faint);
    font: inherit;
    font-size: 10px;
    cursor: pointer;
  }

  .binchip i {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    opacity: 0.5;
  }

  .binchip.on {
    border-color: var(--border-strong);
    background: var(--surface-3);
    color: var(--text);
  }

  .binchip.on i {
    opacity: 1;
  }

  .empty {
    margin: 0;
    padding: 14px 10px;
    color: var(--text-faint);
    font-size: var(--font-size-small);
    text-align: center;
  }

  .details {
    display: grid;
    flex: 0 1 auto;
    gap: 3px;
    max-height: 45%;
    padding: 4px;
    overflow: auto;
    border-top: 1px solid var(--border);
  }

  .colors {
    display: flex;
    gap: 4px;
    padding: 2px 0;
  }

  .chip {
    width: 14px;
    height: 14px;
    padding: 0;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
  }

  .chip[aria-checked="true"] {
    outline: 1px solid var(--text);
    outline-offset: 1px;
  }

  .chip.none {
    background: linear-gradient(135deg, transparent 45%, var(--danger) 45%, var(--danger) 55%, transparent 55%);
  }

  .meta {
    color: var(--text-faint);
    font-size: var(--font-size-small);
  }

  .dropzone {
    position: absolute;
    inset: 2px;
    display: grid;
    place-items: center;
    padding: 8px;
    border: 2px dashed var(--accent);
    background: rgba(63, 169, 245, 0.12);
    color: var(--text);
    font-weight: 600;
    text-align: center;
    pointer-events: none;
  }
</style>
