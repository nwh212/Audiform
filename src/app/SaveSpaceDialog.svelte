<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The Audiform contributors.
  This file is part of Audiform.

  File > Save Space: makes smaller copies of the session's audio that
  sound exactly the same (WAV and AIFF stored as FLAC at its smallest
  setting, stereo files with two identical sides made mono), checks each
  copy sample for sample, points the session at the copies as one undo
  step, saves it, and moves the originals to the Recycle Bin or keeps them.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { mediaSaveSpace, mediaSaveSpaceCancel, onSaveSpaceProgress } from "./core";
  import { bytesText, type SpaceItem, type SpaceOptions, type SpaceOutcome } from "./save-space";
  import { prefs } from "../stores/prefs";

  interface Props {
    items: SpaceItem[];
    /** Points the session at the copies and saves it; false when saving failed. */
    onApply: (outcomes: SpaceOutcome[]) => Promise<boolean>;
    /** Moves files to the Recycle Bin; returns those that could not be moved. */
    onTrash: (paths: string[]) => Promise<string[]>;
    onClose: () => void;
    showError: (e: unknown) => void;
  }

  const { items, onApply, onTrash, onClose, showError }: Props = $props();

  const KEY = "saveSpace.options";
  const saved = (prefs.raw(KEY) as (SpaceOptions & { trash: boolean }) | null) ?? null;
  let opts = $state<SpaceOptions>({ flac: saved?.flac ?? true, to24: saved?.to24 ?? false, mono: saved?.mono ?? true, near: saved?.near ?? false });
  let trash = $state(saved?.trash ?? true);
  let running = $state(false);
  let progress = $state({ done: 0, total: 0 });
  let outcomes = $state<SpaceOutcome[] | null>(null);
  let summary = $state("");

  const made = $derived((outcomes ?? []).filter((o) => o.to));
  const skipped = $derived((outcomes ?? []).filter((o) => !o.to));
  const savedBytes = $derived(made.reduce((s, o) => s + (o.bytesBefore - o.bytesAfter), 0));
  const monoCount = $derived(made.filter((o) => o.mono).length);
  const stereoOnly = $derived(items.filter((i) => !i.monoOk).length);

  onMount(() => {
    let off: (() => void) | undefined;
    void onSaveSpaceProgress((p) => (progress = p)).then((f) => (off = f));
    return () => off?.();
  });

  async function start() {
    prefs.set(KEY, { ...opts, trash });
    running = true;
    progress = { done: 0, total: items.length };
    try {
      const out = await mediaSaveSpace(items, opts);
      outcomes = out;
      const done = out.filter((o) => o.to);
      if (!done.length) {
        summary = "Nothing could be made smaller.";
        return;
      }
      if (!(await onApply(out))) {
        summary = "The session could not be saved, so the originals were kept. The session uses the smaller copies; Undo goes back.";
        return;
      }
      let note = "";
      if (trash) {
        const failed = await onTrash(done.map((o) => o.from));
        note = failed.length ? ` ${failed.length} originals could not be moved to the Recycle Bin and are still there.` : " The originals are in the Recycle Bin.";
      } else {
        note = " The originals are still where they were.";
      }
      summary = `Saved ${bytesText(done.reduce((s, o) => s + o.bytesBefore - o.bytesAfter, 0))} across ${done.length} ${done.length === 1 ? "file" : "files"}. The session is saved and uses the smaller copies.${note}`;
    } catch (e) {
      showError(e);
    } finally {
      running = false;
    }
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && !running && onClose()} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop">
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="sp-title" tabindex="-1" onclick={(e) => e.stopPropagation()}>
    <h1 id="sp-title">Save Space</h1>
    <p class="note">
      Makes smaller copies of the {items.length} {items.length === 1 ? "file" : "files"} this session uses from its folder, which sound exactly the same. Each copy is read back and compared with the original, sample for sample; if anything differs, the original is kept. Then the session uses the copies (one step to undo) and is saved.
    </p>

    <fieldset disabled={running || !!outcomes}>
      <label><input type="checkbox" bind:checked={opts.flac} /> Store WAV and AIFF files as FLAC (smallest setting, 8). Lossless.</label>
      <label class="sub"><input type="checkbox" bind:checked={opts.to24} disabled={!opts.flac} /> Also 32-bit files, stored as 24-bit (FLAC holds at most 24-bit; the change is far below hearing, but not bit-exact)</label>
      <label><input type="checkbox" bind:checked={opts.mono} /> Make stereo files whose two sides are the same into mono files. A stereo track plays them on both sides as before.</label>
      <label class="sub"><input type="checkbox" bind:checked={opts.near} disabled={!opts.mono} /> Also when the sides differ by less than -90 dB (the mono file is their average)</label>
      {#if opts.mono && stereoOnly}
        <p class="note sub">{stereoOnly} {stereoOnly === 1 ? "file stays" : "files stay"} stereo: a clip uses one side of it, or the session is in 3D Space.</p>
      {/if}
      <div class="row">
        <span>Originals:</span>
        <label><input type="radio" name="sp-orig" checked={trash} onchange={() => (trash = true)} /> Move to the Recycle Bin</label>
        <label><input type="radio" name="sp-orig" checked={!trash} onchange={() => (trash = false)} /> Keep them</label>
      </div>
      <p class="note sub">Other sessions that use the same files will ask for them when opened (File panel right-click, Link Media), unless the originals are kept.</p>
    </fieldset>

    {#if running}
      <div class="bar"><div style:width={`${progress.total ? Math.round((progress.done / progress.total) * 100) : 0}%`}></div></div>
      <div class="note">{progress.done} of {progress.total} files checked</div>
    {/if}
    {#if outcomes}
      <p>
        {made.length} made smaller{monoCount ? ` (${monoCount} now mono)` : ""}, {bytesText(savedBytes)} saved. {skipped.length ? `${skipped.length} left as they are.` : ""}
      </p>
      {#if summary}<p class="note">{summary}</p>{/if}
      {#if skipped.length}
        <details>
          <summary>Left as they are ({skipped.length})</summary>
          <ul>
            {#each skipped as o (o.from)}
              <li><b>{o.from.split(/[\\/]/).pop()}</b>: {o.skipped}</li>
            {/each}
          </ul>
        </details>
      {/if}
    {/if}

    <div class="buttons">
      <span class="grow"></span>
      {#if running}
        <button type="button" onclick={() => void mediaSaveSpaceCancel()}>Stop</button>
      {:else if outcomes}
        <button type="button" class="primary" onclick={onClose}>Close</button>
      {:else}
        <button type="button" onclick={onClose}>Cancel</button>
        <button type="button" class="primary" disabled={!items.length || (!opts.flac && !opts.mono)} onclick={() => void start()}>Save Space</button>
      {/if}
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 5100;
    display: grid;
    place-items: center;
    background: rgba(0, 0, 0, 0.5);
  }
  .dialog {
    display: grid;
    gap: 10px;
    width: min(560px, 94vw);
    max-height: 92vh;
    overflow: auto;
    padding: 14px;
    border: 1px solid var(--border-strong);
    background: var(--surface-2);
    box-shadow: 0 16px 40px rgba(0, 0, 0, 0.6);
    font-size: 12px;
  }
  h1 {
    margin: 0;
    font-size: 14px;
  }
  p {
    margin: 0;
  }
  .note {
    color: var(--text-dim);
  }
  fieldset {
    display: grid;
    gap: 6px;
    margin: 0;
    border: 1px solid var(--border);
  }
  .sub {
    margin-left: 20px;
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
    align-items: center;
  }
  ul {
    margin: 4px 0 0;
    padding-left: 18px;
    max-height: 160px;
    overflow: auto;
    color: var(--text-dim);
  }
  .bar {
    height: 6px;
    background: var(--surface-1);
  }
  .bar div {
    height: 100%;
    background: var(--accent);
  }
  .buttons {
    display: flex;
    gap: 8px;
  }
  .grow {
    flex: 1;
  }
</style>
