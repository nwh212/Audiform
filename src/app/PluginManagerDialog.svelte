<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  Effects > Plug-in Manager: where VST3 plug-ins are looked for (the
  standard folders, which can be turned off, and the user's own), a scan,
  every plug-in found, and every bundle that could not be read and why.
  Each bundle is read in a process of its own, so one that crashes is
  only listed as failed.
-->
<script lang="ts">
  import { onPluginScanProgress, pluginsRegistry, pluginsScan, pluginsScanStop, pluginsSetFolder, pluginsSetShared, type PluginRegistry, type PluginScanProgress } from "./core";
  import { pluginsVersion } from "../stores/plugins";

  interface Props {
    /** Asks the user for a folder. */
    pickFolder: () => Promise<string | null>;
    onNotice: (text: string) => void;
    onError: (e: unknown) => void;
    onClose: () => void;
    /** Fetches running plug-ins' settings into the session (before some of them restart). */
    beforeRestart?: () => Promise<void>;
  }

  const { pickFolder, onNotice, onError, onClose, beforeRestart = async () => {} }: Props = $props();

  let reg = $state<PluginRegistry | null>(null);
  let scanning = $state(false);
  let progress = $state<PluginScanProgress | null>(null);
  let filter = $state("");

  void pluginsRegistry()
    .then((r) => (reg = r))
    .catch((e: unknown) => onError(e));

  const shown = $derived(
    (reg?.plugins ?? []).filter((p) => {
      const q = filter.trim().toLowerCase();
      return !q || p.name.toLowerCase().includes(q) || p.vendor.toLowerCase().includes(q) || p.kind.toLowerCase().includes(q);
    }),
  );

  /** Each maker with plug-ins installed, and whether its plug-ins run together. */
  const makers = $derived.by(() => {
    const seen = new Map<string, string>();
    for (const p of reg?.plugins ?? []) if (p.vendor.trim()) seen.set(p.vendor.trim().toLowerCase(), p.vendor.trim());
    return [...seen.entries()].sort((a, b) => a[1].localeCompare(b[1])).map(([key, name]) => ({ key, name, on: reg?.shared.includes(key) ?? false }));
  });
  async function setTogether(key: string, on: boolean) {
    const now = new Set(reg?.shared ?? []);
    if (on) now.add(key);
    else now.delete(key);
    try {
      // Its running plug-ins restart the new way: keep what they are set to first.
      await beforeRestart();
      reg = await pluginsSetShared([...now]);
      onNotice(on ? "Plug-ins from that maker will run together from the next playback." : "Plug-ins from that maker will each run on their own from the next playback.");
    } catch (e) {
      onError(e);
    }
  }

  async function scan(all: boolean) {
    if (scanning) return;
    scanning = true;
    progress = null;
    const off = await onPluginScanProgress((p) => (progress = p)).catch(() => null);
    try {
      const [report, r] = await pluginsScan(all);
      reg = r;
      pluginsVersion.update((n) => n + 1);
      const failed = report.failed ? `, ${report.failed} could not be read` : "";
      const stopped = report.stopped ? " Stopped early: Scan again to read the rest." : "";
      onNotice(`Plug-in scan: ${report.found} plug-in${report.found === 1 ? "" : "s"} found${failed}.${stopped}`);
    } catch (e) {
      onError(e);
    } finally {
      off?.();
      scanning = false;
      progress = null;
    }
  }

  async function addFolder() {
    const path = await pickFolder();
    if (!path) return;
    try {
      reg = await pluginsSetFolder(path, true);
      await scan(false);
    } catch (e) {
      onError(e);
    }
  }

  async function setFolder(path: string, on: boolean) {
    try {
      reg = await pluginsSetFolder(path, on);
    } catch (e) {
      onError(e);
    }
  }

  /** Closing during a scan stops it; what it read so far is kept. */
  function close() {
    if (scanning) void pluginsScanStop().catch(() => {});
    onClose();
  }

  let stopping = $state(false);
  function stopScan() {
    stopping = true;
    void pluginsScanStop().catch(onError);
  }
  $effect(() => {
    if (!scanning) stopping = false;
  });

  const name = (path: string) => path.split(/[\\/]/).filter(Boolean).pop() ?? path;
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && close()} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={close}>
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="pm-title" tabindex="-1" onclick={(e) => e.stopPropagation()}>
    <h1 id="pm-title">Plug-in Manager</h1>

    <section>
      <h2>Folders searched</h2>
      <ul class="folders">
        {#each reg?.folders ?? [] as f, k (k)}
          <li class:off={!f.on}>
            {#if f.standard}
              <label title={f.on ? "Searched. Untick to leave this standard folder out." : "Left out. Tick to search it again."}>
                <input type="checkbox" checked={f.on} onchange={(e) => void setFolder(f.path, e.currentTarget.checked)} />
                <span class="path">{f.path}</span>
              </label>
              <span class="tag">standard</span>
            {:else}
              <span class="path" title={f.path}>{f.path}</span>
              <button type="button" class="small" title="Stop searching this folder" onclick={() => void setFolder(f.path, false)}>Remove</button>
            {/if}
          </li>
        {/each}
      </ul>
      <div class="row">
        <button type="button" onclick={() => void addFolder()} disabled={scanning}>Add Folder...</button>
        <span class="grow progress" aria-live="polite">
          {#if scanning && progress && progress.total > 0}
            Read {progress.done} of {progress.total}{progress.name ? `. Now: ${progress.name}` : ""}
          {:else if scanning}
            Looking for plug-ins...
          {/if}
        </span>
        {#if scanning}
          <button type="button" class="primary" onclick={stopScan} disabled={stopping} title="Stop scanning. What was read so far is kept; Scan reads the rest later.">{stopping ? "Stopping..." : "Stop Scan"}</button>
        {:else}
          <button type="button" class="primary" onclick={() => void scan(false)} title="Read new and changed plug-ins, and try failed ones again">Scan</button>
          <button type="button" onclick={() => void scan(true)} title="Read every plug-in again">Rescan All</button>
        {/if}
      </div>
    </section>

    <section class="list">
      <div class="row">
        <h2>Plug-ins ({reg?.plugins.length ?? 0})</h2>
        <span class="grow"></span>
        <input class="search" type="search" placeholder="Find" aria-label="Find a plug-in" bind:value={filter} />
      </div>
      {#if !reg}
        <p class="note">Loading...</p>
      {:else if reg.plugins.length === 0}
        <p class="note">No plug-ins yet. Install VST3 plug-ins (or add the folder they are in) and press Scan.</p>
      {:else}
        <div class="table" role="table" aria-label="Plug-ins">
          <div class="tr th" role="row">
            <span role="columnheader">Name</span><span role="columnheader">Maker</span><span role="columnheader">Kind</span><span role="columnheader" title="Look-ahead the plug-in adds, compensated during playback and export">Delay</span><span role="columnheader">Window</span>
          </div>
          {#each shown as p, k (k)}
            <div class="tr" role="row" title={p.note ? `${p.bundle}\n${p.note}` : p.bundle}>
              <span>{#if p.note}<span class="warn" title={p.note}>⚠</span> {/if}{p.name}</span><span>{p.vendor}</span><span>{p.kind.replaceAll("|", ", ")}</span><span>{p.latency ? `${p.latency} smp` : "none"}</span><span>{p.hasWindow ? "yes" : "controls only"}</span>
            </div>
          {/each}
        </div>
      {/if}
      {#if makers.length}
        <h2>Run together</h2>
        <p class="note">
          Some plug-ins talk to each other while they play, for example an immersive panner on each track and the renderer on the mix. Those need to run in one shared
          process. Tick a maker to run all its plug-ins together. If one of them crashes, the others from that maker stop too. Takes effect from the next playback or export
          (their current settings are kept).
        </p>
        <div class="makers">
          {#each makers as m (m.key)}
            <label><input type="checkbox" checked={m.on} onchange={(e) => void setTogether(m.key, e.currentTarget.checked)} /> {m.name}</label>
          {/each}
        </div>
      {/if}
      {#if reg?.failed.length}
        <h2>Could not be read ({reg.failed.length})</h2>
        <ul class="failed">
          {#each reg.failed as f, k (k)}
            <li title={f.bundle}><b>{name(f.bundle)}</b>: {f.error}</li>
          {/each}
        </ul>
      {/if}
    </section>

    <p class="note">
      Effects appear in the Audio FX chain's Add effect list under Plug-ins (VST3); instruments in MIDI track headers. ⚠ marks a plug-in that crashed while being read: it is
      listed anyway, and hover it to see where it stopped. Each runs in a process of its own (or its maker's, when set to run together), so one that crashes is
      bypassed and the editor carries on. VST2 plug-ins are not supported.
    </p>
    <footer>
      <button type="button" onclick={close} title={scanning ? "Stops the scan and closes. What was read so far is kept." : undefined}>Close</button>
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
    width: min(760px, 94vw);
    max-height: 88vh;
    padding: 14px;
    overflow: auto;
    border: 1px solid var(--border-strong);
    background: var(--surface-2);
    box-shadow: 0 16px 40px rgba(0, 0, 0, 0.6);
  }
  h1 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
  }
  .makers {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 14px;
    margin: 4px 0 8px;
  }
  .makers label {
    display: flex;
    gap: 5px;
    align-items: center;
  }
  h2 {
    margin: 0 0 6px;
    color: var(--text-dim);
    font-size: var(--font-size-small);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  section {
    display: grid;
    gap: 6px;
  }
  .row {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .grow {
    flex: 1;
  }
  .progress {
    overflow: hidden;
    color: var(--text-dim);
    font-size: var(--font-size-small);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .folders,
  .failed {
    display: grid;
    gap: 2px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .folders li {
    display: flex;
    gap: 8px;
    align-items: center;
    padding: 3px 6px;
    background: var(--surface-1);
  }
  .folders li.off .path {
    color: var(--text-faint);
    text-decoration: line-through;
  }
  .folders label {
    display: flex;
    flex: 1;
    gap: 6px;
    align-items: center;
    min-width: 0;
  }
  .path {
    flex: 1;
    overflow: hidden;
    font-family: var(--font-mono);
    font-size: 11px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .tag {
    color: var(--text-faint);
    font-size: 10px;
  }
  .table {
    display: grid;
    max-height: 260px;
    overflow: auto;
    border: 1px solid var(--border);
  }
  .tr {
    display: grid;
    grid-template-columns: 2fr 1.4fr 1.6fr 0.8fr 1fr;
    gap: 8px;
    padding: 3px 8px;
    font-size: var(--font-size-small);
  }
  .tr:nth-child(even) {
    background: var(--surface-1);
  }
  .tr span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .th {
    position: sticky;
    top: 0;
    background: var(--surface-3);
    color: var(--text-dim);
    font-weight: 600;
  }
  .warn {
    color: var(--warning, #e0a030);
    cursor: help;
  }
  .failed li {
    color: var(--text-dim);
    font-size: var(--font-size-small);
  }
  .note {
    margin: 0;
    color: var(--text-dim);
    font-size: var(--font-size-small);
  }
  button,
  input.search {
    height: 24px;
    padding: 0 10px;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    background: var(--surface-3);
    color: var(--text);
    font: inherit;
  }
  input.search {
    width: 180px;
    background: var(--surface-1);
  }
  button.small {
    height: 20px;
    padding: 0 6px;
    font-size: 11px;
  }
  .primary {
    border-color: var(--accent);
  }
  footer {
    display: flex;
    justify-content: flex-end;
  }
</style>
