<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  Help > About.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import logo from "../assets/logo-dark.png";
  import { formatMb, memoryReport, type MemoryReport } from "./core";

  interface Props {
    version: string | null;
    onClose: () => void;
  }

  const { version, onClose }: Props = $props();

  // Where the memory goes, refreshed while the window is open.
  let memory = $state<MemoryReport | null>(null);
  onMount(() => {
    const read = () => memoryReport().then((r) => (memory = r)).catch(() => {});
    void read();
    const timer = setInterval(() => void read(), 2000);
    return () => clearInterval(timer);
  });
  const pluginsMb = $derived(memory ? memory.plugins.reduce((n, p) => n + p.memoryMb, 0) : 0);
  /** What the page itself holds: script memory and canvas pixels (the rest of the view is the browser's own). */
  let page = $state<{ scriptMb: number; canvases: number; canvasMb: number } | null>(null);
  function readPage() {
    const mem = (performance as unknown as { memory?: { usedJSHeapSize: number } }).memory;
    let bytes = 0;
    const list = document.querySelectorAll("canvas");
    for (const c of list) bytes += c.width * c.height * 4;
    page = { scriptMb: mem ? mem.usedJSHeapSize / 1048576 : 0, canvases: list.length, canvasMb: bytes / 1048576 };
  }
  onMount(() => {
    readPage();
    const timer = setInterval(readPage, 2000);
    return () => clearInterval(timer);
  });
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onClose()} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={onClose}>
  <div class="dialog" role="dialog" tabindex="-1" aria-modal="true" aria-labelledby="about-title" onclick={(e) => e.stopPropagation()}>
    <img class="logo" src={logo} alt="" width="372" height="134" />
    <h1 id="about-title" class="visually-hidden">Audiform</h1>
    <p class="version">Version {version ?? "unknown"}{__BUILD_COMMIT__ ? ` (build ${__BUILD_COMMIT__})` : ""}</p>
    <p>A free, open-source multitrack audio editor for podcasts, voiceover, music and immersive mixes.</p>
    <div class="credit">
      <p class="credit-head">Built on Altered Audio Editor</p>
      <p>
        Audiform is a fork of Altered Audio Editor, created by Steve Phillips and The GeekCast Radio Network, LLC. Its editor,
        sessions, effects, Voice Chain, podcast tools and the design of the whole app are their work; Audiform builds on that
        foundation. Their website, Steve's book and a way to support them are in Help &gt; Based on Altered Audio Editor.
      </p>
    </div>
    <p class="privacy">
      No account, no sign-in, no telemetry, no tracking. The app never contacts the internet on its own; it only opens a web
      page when you pick a link in the Help menu. Your audio and sessions stay on your computer.
    </p>
    <p>Copyright (c) The GeekCast Radio Network, LLC (Altered Audio Editor).<br />Copyright (c) The Audiform contributors (changes since October 2026).</p>
    <p class="fine">
      Licensed under the GNU General Public License v3.0 or later. This program comes with no warranty.
      You may redistribute it under the terms of that license.
    </p>
    <p class="fine">
      Audiform is an independent project, not made or endorsed by the makers of Altered Audio Editor. Please report problems to Audiform (Help &gt; Report a Problem).
    </p>
    {#if memory?.known}
      <div class="memory">
        <p class="credit-head">Memory in use: {formatMb(memory.totalMb)}</p>
        <p>
          Audiform {formatMb(memory.appMb)}, editor view {formatMb(memory.viewMb)}{memory.viewProcesses > 1 ? ` (${memory.viewProcesses} processes)` : ""}, plug-ins {formatMb(pluginsMb)}{memory.otherMb > 0 ? `, other ${formatMb(memory.otherMb)}` : ""}.
        </p>
        {#if page}
          <p>Of the editor view, the page itself holds {formatMb(page.scriptMb)} of script memory and {formatMb(page.canvasMb)} in {page.canvases} canvases.</p>
        {/if}
        {#if memory.plugins.length > 0}
          <ul>
            {#each memory.plugins as p (p.name + p.memoryMb)}
              <li><span>{p.name}</span><span>{formatMb(p.memoryMb)}</span></li>
            {/each}
          </ul>
        {/if}
      </div>
    {/if}
    <div class="actions">
      <button type="button" onclick={onClose}>OK</button>
    </div>
  </div>
</div>

<style>
  .memory {
    margin-top: 10px;
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: 6px;
    font-size: 12px;
    text-align: left;
  }
  .memory p {
    margin: 2px 0;
  }
  .memory ul {
    margin: 4px 0 0;
    padding: 0;
    list-style: none;
    max-height: 120px;
    overflow: auto;
  }
  .memory li {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    color: var(--text-muted);
  }
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 5000;
    display: grid;
    place-items: center;
    background: rgba(0, 0, 0, 0.5);
  }

  .dialog {
    width: 460px;
    padding: 20px 24px 16px;
    border: 1px solid var(--border-strong);
    background: var(--surface-2);
    box-shadow: 0 16px 40px rgba(0, 0, 0, 0.6);
    user-select: text;
  }

  .logo {
    display: block;
    width: 100%;
    height: auto;
    margin: -8px 0 4px;
    border-radius: 4px;
  }

  .visually-hidden {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
    white-space: nowrap;
  }

  .privacy {
    padding: 6px 8px;
    border-left: 2px solid var(--accent);
    background: var(--surface-1);
    color: var(--text-dim);
    font-size: var(--font-size-small);
  }

  h1 {
    margin: 0 0 2px;
    font-size: 16px;
    font-weight: 600;
  }

  .credit {
    margin: 10px 0;
    padding: 6px 10px;
    border-left: 2px solid var(--accent);
    background: var(--surface-1);
  }

  .credit p {
    margin: 4px 0;
  }

  .credit-head {
    font-weight: 600;
  }

  .version {
    margin-top: 0;
    color: var(--value);
  }

  p {
    margin: 8px 0;
    line-height: 1.45;
  }

  .fine {
    color: var(--text-dim);
    font-size: var(--font-size-small);
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    margin-top: 14px;
  }

  button {
    min-width: 72px;
    height: 24px;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    background: var(--surface-3);
    color: var(--text);
    font: inherit;
  }

  button:hover {
    border-color: var(--accent);
  }
</style>
