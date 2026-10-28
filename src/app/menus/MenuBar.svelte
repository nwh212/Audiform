<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  Menu bar. Mouse: click a menu to open, hover to move between menus
  while one is open. Keyboard: Alt plus the underlined letter opens a
  menu; arrows move; Enter runs; Escape closes. Typing letters jumps to
  the first item in the open list that starts with them. A popup entry
  (the scanned plug-ins) opens a list the app builds at that moment, as
  a menu beside it that nests as deep as it needs. A menu with a search
  box (Effects) lists what matches the text typed into it in place of
  its items.
-->
<script lang="ts" module>
  import type { ContextItem } from "../ContextMenu.svelte";
  /** What a popup entry shows: its rows, or why it is unavailable. */
  export type PopupList = { items: readonly ContextItem[]; reason?: string | undefined };
</script>

<script lang="ts">
  import { tick as nextTick } from "svelte";
  import type { ActionRegistry } from "../actions";
  import ContextMenu from "../ContextMenu.svelte";
  import { isTypingKey, typeahead } from "../typeahead";
  import type { Menu, MenuItem } from "./model";

  interface Props {
    menus: readonly Menu[];
    actions: ActionRegistry;
    /** Shortcut text shown beside an item, from the keymap. */
    shortcutFor: (id: string) => string | undefined;
    onError: (e: unknown) => void;
    /** Builds each popup entry's list by id, when it opens. */
    popups?: Record<string, () => PopupList>;
    /** Menus with a search box at the top, by menu label: what each finds for the text typed. */
    searches?: Record<string, { placeholder: string; find: (query: string) => readonly ContextItem[] }>;
  }

  const { menus, actions, shortcutFor, onError, popups = {}, searches = {} }: Props = $props();
  /** Text in the open menu's search box, and the highlighted result. */
  let menuQuery = $state("");
  let resultActive = $state(-1);
  const results = $derived.by(() => {
    const q = menuQuery.trim();
    const box = openMenu === null ? undefined : searches[menus[openMenu]?.label ?? ""];
    if (!q || !box) return [];
    return box.find(q).filter((r): r is Extract<ContextItem, { kind: "item" }> => r.kind === "item");
  });
  /** Puts the cursor in the search box once the menu is on screen, so typing searches straight away. */
  function focusSoon(node: HTMLInputElement) {
    requestAnimationFrame(() => node.focus());
  }

  function runResult(i: number) {
    const r = results[i];
    if (!r || r.disabled) return;
    close();
    r.run();
  }
  /** The popup list open beside its entry, if any. */
  let popupOpen = $state<{ id: string; x: number; y: number; items: readonly ContextItem[] } | null>(null);
  const jump = typeahead();

  let openMenu = $state<number | null>(null);
  let active = $state(-1);
  let openSub = $state<number | null>(null);
  let subActive = $state(-1);
  let altHeld = $state(false);
  let root: HTMLElement;

  // Re-read action state (enabled, checked, labels) when it changes.
  let tick = $state(0);
  $effect(() =>
    actions.changed.subscribe((n) => {
      tick = n;
    }),
  );

  type Row = Exclude<MenuItem, { kind: "sep" }>;

  function enabled(it: MenuItem): boolean {
    void tick;
    if (it.kind === "item") return actions.isEnabled(it.id);
    if (it.kind === "submenu") return it.items.some(enabled);
    if (it.kind === "popup") {
      const p = popups[it.id]?.();
      return !!p && p.items.length > 0 && !p.reason;
    }
    return false;
  }

  function checked(it: Row): boolean | undefined {
    void tick;
    return it.kind === "item" ? actions.isChecked(it.id) : undefined;
  }

  function label(it: Row): string {
    void tick;
    return it.kind === "item" ? (actions.label(it.id) ?? it.label) : it.label;
  }

  function shortcut(id: string): string | undefined {
    void tick;
    return shortcutFor(id);
  }

  function tooltip(it: Row): string | undefined {
    if (it.kind === "item" && !enabled(it)) return actions.reason(it.id) ?? it.pending ?? "Not built yet.";
    if (it.kind === "popup" && !enabled(it)) return popups[it.id]?.().reason ?? "Nothing to show yet.";
    return undefined;
  }

  /** Items with hidden ones (such as unused Open Recent slots) left out. */
  function shown(items: readonly MenuItem[]): readonly MenuItem[] {
    void tick;
    return items.filter((it) => it.kind !== "item" || actions.isVisible(it.id));
  }

  function currentItems(): readonly MenuItem[] {
    return openMenu === null ? [] : shown(menus[openMenu]?.items ?? []);
  }

  function subItems(): readonly MenuItem[] {
    const it = openSub === null ? undefined : currentItems()[openSub];
    return it?.kind === "submenu" ? shown(it.items) : [];
  }

  function step(items: readonly MenuItem[], from: number, dir: 1 | -1): number {
    const n = items.length;
    // Nothing active yet: Down starts at the top, Up at the bottom.
    if (from < 0) from = dir === 1 ? -1 : n;
    for (let i = 1; i <= n; i++) {
      const idx = (from + dir * i + n * 2) % n;
      if (items[idx]?.kind !== "sep") return idx;
    }
    return from;
  }

  function open(i: number, focusFirst = false) {
    menuQuery = "";
    resultActive = -1;
    openMenu = i;
    openSub = null;
    subActive = -1;
    active = focusFirst ? step(shown(menus[i]?.items ?? []), -1, 1) : -1;
  }

  /** Opens a popup entry's list beside its row. */
  function openPopup(it: Extract<MenuItem, { kind: "popup" }>, rowEl: Element | null) {
    if (!enabled(it) || !rowEl) return;
    const r = rowEl.getBoundingClientRect();
    popupOpen = { id: it.id, x: r.right, y: r.top - 4, items: popups[it.id]?.().items ?? [] };
  }

  function close() {
    popupOpen = null;
    openMenu = null;
    openSub = null;
    active = -1;
    subActive = -1;
  }

  function activate(it: MenuItem | undefined, index: number, inSub: boolean) {
    if (!it || it.kind === "sep") return;
    if (it.kind === "popup") {
      openPopup(it, root.querySelector(`[data-popup="${it.id}"]`));
      return;
    }
    if (it.kind === "submenu") {
      if (!inSub) {
        openSub = index;
        subActive = step(shown(it.items), -1, 1);
      }
      return;
    }
    if (!actions.isEnabled(it.id)) return;
    close();
    actions.run(it.id).catch(onError);
  }

  function onKey(e: KeyboardEvent) {
    // A dialog owns the keyboard while it is open.
    if (document.querySelector('[aria-modal="true"]')) return;
    if (e.target instanceof HTMLInputElement && e.target.dataset.menuSearch !== undefined) {
      // In the search box: typing goes into it; the arrows, Enter and Escape work the results.
      if (e.key === "Escape") {
        e.preventDefault();
        if (menuQuery) menuQuery = "";
        else close();
        return;
      }
      if (menuQuery.trim()) {
        if (e.key === "ArrowDown" || e.key === "ArrowUp") {
          e.preventDefault();
          const n = results.length;
          if (n) resultActive = (resultActive + (e.key === "ArrowDown" ? 1 : n - 1) + (resultActive < 0 && e.key === "ArrowUp" ? 1 : 0)) % n;
        } else if (e.key === "Enter") {
          e.preventDefault();
          runResult(Math.max(0, resultActive));
        }
        return;
      }
      if (!["ArrowDown", "ArrowUp", "ArrowLeft", "ArrowRight", "Enter"].includes(e.key)) return;
    }
    if (e.key === "Alt") {
      altHeld = true;
      return;
    }

    if (e.altKey && !e.ctrlKey && !e.metaKey && e.key.length === 1) {
      const i = menus.findIndex((m) => m.mnemonic.toLowerCase() === e.key.toLowerCase());
      if (i >= 0) {
        e.preventDefault();
        open(i, true);
        return;
      }
    }

    if (openMenu === null) return;
    const items = currentItems();
    const sub = subItems();
    if (isTypingKey(e)) {
      const list = openSub !== null ? sub : items;
      const labels = list.map((it) => (it.kind === "sep" || !enabled(it) ? null : label(it)));
      const to = jump.next(e.key, labels, openSub !== null ? subActive : active);
      if (to !== null) {
        e.preventDefault();
        if (openSub !== null) subActive = to;
        else active = to;
      }
      return;
    }

    switch (e.key) {
      case "Escape":
        e.preventDefault();
        if (openSub !== null) openSub = null;
        else close();
        break;
      case "ArrowDown":
      case "ArrowUp": {
        e.preventDefault();
        const dir = e.key === "ArrowDown" ? 1 : -1;
        if (openSub !== null) subActive = step(sub, subActive, dir);
        else active = step(items, active, dir);
        showActive();
        break;
      }
      case "ArrowRight": {
        e.preventDefault();
        const it = items[active];
        if (openSub === null && (it?.kind === "submenu" || it?.kind === "popup")) activate(it, active, false);
        else if (openSub !== null && sub[subActive]?.kind === "popup") activate(sub[subActive], subActive, true);
        else open((openMenu + 1) % menus.length, true);
        break;
      }
      case "ArrowLeft":
        e.preventDefault();
        if (openSub !== null) openSub = null;
        else open((openMenu - 1 + menus.length) % menus.length, true);
        break;
      case "Enter":
      case " ":
        e.preventDefault();
        if (openSub !== null) activate(sub[subActive], subActive, true);
        else activate(items[active], active, false);
        break;
    }
  }

  function onKeyUp(e: KeyboardEvent) {
    if (e.key === "Alt") altHeld = false;
  }

  function onPointerDown(e: PointerEvent) {
    // A popup list lives at the end of the page (see ContextMenu), so it counts as inside too.
    const t = e.target as Element;
    if (openMenu !== null && !root.contains(t) && !t.closest?.(".context")) close();
  }

  /**
   * Keeps a submenu on screen: capped to the window's height and scrolling
   * inside when it is longer (the plug-in list can be), and moved up when it
   * would run off the bottom.
   */
  function fit(node: HTMLElement) {
    const margin = 8;
    const room = window.innerHeight - margin * 2;
    node.style.maxHeight = `${room}px`;
    const rect = node.getBoundingClientRect();
    const over = rect.top + rect.height - (window.innerHeight - margin);
    if (over > 0) node.style.top = `${-5 - over}px`;
  }

  /** Brings the row the arrow keys are on into view in a scrolling submenu. */
  function showActive() {
    void nextTick().then(() => {
      const row = root.querySelector(".dropdown.sub .row.active") ?? root.querySelector(".row.active");
      row?.scrollIntoView({ block: "nearest" });
    });
  }
</script>

<svelte:window onkeydown={onKey} onkeyup={onKeyUp} onpointerdown={onPointerDown} onblur={() => { close(); altHeld = false; }} />

{#snippet labelWithMnemonic(text: string, letter: string)}
  {@const i = text.toLowerCase().indexOf(letter.toLowerCase())}
  {#if i >= 0}{text.slice(0, i)}<span class="mn" class:show={altHeld || openMenu !== null}>{text[i]}</span>{text.slice(i + 1)}{:else}{text}{/if}
{/snippet}

{#snippet row(it: MenuItem, index: number, inSub: boolean)}
  {#if it.kind === "sep"}
    <div class="sep" role="separator"></div>
  {:else}
    {@const on = enabled(it)}
    {@const isActive = inSub ? subActive === index : active === index}
    {@const isChecked = checked(it)}
    <button
      type="button"
      class="row"
      class:active={isActive}
      class:disabled={!on}
      role={isChecked === undefined ? "menuitem" : "menuitemcheckbox"}
      aria-checked={isChecked}
      aria-disabled={!on}
      aria-haspopup={it.kind === "submenu" || it.kind === "popup" ? "menu" : undefined}
      data-popup={it.kind === "popup" ? it.id : undefined}
      title={tooltip(it)}
      onpointerenter={(e) => {
        if (popupOpen && !(it.kind === "popup" && popupOpen.id === it.id)) popupOpen = null;
        if (inSub) subActive = index;
        else {
          active = index;
          if (it.kind === "submenu") {
            openSub = index;
            subActive = -1;
          } else openSub = null;
        }
        if (it.kind === "popup") openPopup(it, e.currentTarget);
      }}
      onclick={(e) => (it.kind === "popup" ? openPopup(it, e.currentTarget) : activate(it, index, inSub))}
    >
      <span class="check">{isChecked ? "✓" : ""}</span>
      <span class="label">{label(it)}</span>
      <span class="shortcut">{it.kind === "item" ? (shortcut(it.id) ?? "") : ""}</span>
      <span class="arrow">
        {#if it.kind === "submenu" || it.kind === "popup"}
          <svg viewBox="0 0 8 8" aria-hidden="true"><path d="M2 1l4 3-4 3" /></svg>
        {/if}
      </span>
    </button>
    {#if !inSub && it.kind === "submenu" && openSub === index}
      <div class="dropdown sub" role="menu" use:fit>
        {#each shown(it.items) as child, ci (ci)}
          {@render row(child, ci, true)}
        {/each}
      </div>
    {/if}
  {/if}
{/snippet}

<nav class="menubar" bind:this={root} aria-label="Main menu">
  {#each menus as menu, mi (menu.label)}
    <div class="slot">
      <button
        type="button"
        class="top"
        class:open={openMenu === mi}
        aria-haspopup="menu"
        aria-expanded={openMenu === mi}
        onclick={() => (openMenu === mi ? close() : open(mi))}
        onpointerenter={() => {
          if (openMenu !== null && openMenu !== mi) open(mi);
        }}
      >
        {@render labelWithMnemonic(menu.label, menu.mnemonic)}
      </button>
      {#if openMenu === mi}
        {@const box = searches[menu.label]}
        <div class="dropdown" role="menu">
          {#if box}
            <div class="search">
              <input
                type="search"
                spellcheck="false"
                data-menu-search
                use:focusSoon
                placeholder={box.placeholder}
                aria-label={box.placeholder}
                value={menuQuery}
                oninput={(e) => {
                  menuQuery = e.currentTarget.value;
                  resultActive = -1;
                  openSub = null;
                  popupOpen = null;
                }}
              />
            </div>
          {/if}
          {#if box && menuQuery.trim()}
            {#each results as r, ri (ri)}
              <button
                type="button"
                class="row"
                class:active={resultActive === ri}
                class:disabled={r.disabled}
                role="menuitem"
                onpointerenter={() => (resultActive = ri)}
                onclick={() => runResult(ri)}
              >
                <span class="check"></span>
                <span class="label">{r.label}</span>
                <span class="shortcut">{r.shortcut ?? ""}</span>
                <span class="arrow"></span>
              </button>
            {:else}
              <div class="none">No matches</div>
            {/each}
          {:else}
            {#each shown(menu.items) as it, ii (ii)}
              <div class="cell">{@render row(it, ii, false)}</div>
            {/each}
          {/if}
        </div>
      {/if}
    </div>
  {/each}
  {#if popupOpen}
    <!-- Inside the bar, so a click in it does not count as a click outside. -->
    <ContextMenu x={popupOpen.x} y={popupOpen.y} items={popupOpen.items} onClose={close} />
  {/if}
</nav>

<style>
  .menubar {
    display: flex;
    align-items: stretch;
    height: 100%;
  }

  .slot {
    position: relative;
  }

  .search {
    padding: 3px 6px 4px;
  }

  .search input {
    width: 100%;
    box-sizing: border-box;
    height: 22px;
    padding: 0 6px;
    border: 1px solid var(--border-strong);
    border-radius: 3px;
    background: var(--surface-0);
    color: var(--text);
    font: inherit;
  }

  .none {
    padding: 4px 12px 6px 28px;
    color: var(--text-faint);
  }

  .top {
    height: 100%;
    padding: 0 9px;
    border: 0;
    background: transparent;
    color: var(--text);
    font: inherit;
  }

  .top:hover,
  .top.open {
    background: var(--surface-3);
  }

  .top:focus,
  .row:focus {
    outline: none;
  }

  .mn.show {
    text-decoration: underline;
    text-underline-offset: 2px;
  }

  .dropdown {
    position: absolute;
    top: 100%;
    left: 0;
    z-index: 3000;
    min-width: 260px;
    padding: 4px 0;
    border: 1px solid var(--border-strong);
    background: var(--surface-2);
    box-shadow: 0 10px 28px rgba(0, 0, 0, 0.55);
  }

  .cell {
    position: relative;
  }

  .dropdown.sub {
    top: -5px;
    left: 100%;
    min-width: 220px;
    box-sizing: border-box;
    overflow-x: hidden;
    overflow-y: auto;
    overscroll-behavior: contain;
    scrollbar-width: thin;
  }

  .row {
    display: grid;
    grid-template-columns: 24px 1fr auto 18px;
    align-items: center;
    width: 100%;
    height: var(--row-height);
    padding: 0 6px 0 0;
    border: 0;
    background: transparent;
    color: var(--text);
    font: inherit;
    text-align: left;
    white-space: nowrap;
  }

  .row.active {
    background: var(--accent-dim);
    color: #fff;
  }

  .row.disabled {
    color: var(--text-faint);
  }

  .row.disabled.active {
    background: var(--surface-3);
    color: var(--text-dim);
  }

  .check {
    text-align: center;
  }

  .shortcut {
    padding-left: 24px;
    color: var(--text-dim);
    font-size: var(--font-size-small);
  }

  .row.active .shortcut {
    color: inherit;
  }

  .arrow svg {
    width: 8px;
    height: 8px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.4;
  }

  .sep {
    height: 1px;
    margin: 4px 0;
    background: var(--border);
  }
</style>
