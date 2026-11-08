<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The Audiform contributors.
  This file is part of Audiform.

  Measure Scores: runs the downloaded separation models over the
  MUSDB18-HQ test songs (which the user downloads and points at) and
  writes what each scored: scores.json with every number, scores.txt as
  a table and run.log with each step. Models whose part the test set
  does not hold are listed with the reason. Nothing is downloaded.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { libraryPickFolder, onSeparationMeasure, revealMedia, separationCancel, separationMeasure, separationMeasurePlan, type MeasurePlan, type MeasureResult } from "../../app/core";
  import { prefs } from "../../stores/prefs";

  interface Props {
    gpu: boolean;
    onClose: () => void;
    showError: (e: unknown) => void;
  }

  const { gpu, onClose, showError }: Props = $props();

  const FOLDER = "separation.measure.folder";
  let folder = $state((prefs.raw(FOLDER) as string | null) ?? "");
  let plans = $state<MeasurePlan[]>([]);
  let picked = $state<Set<string>>(new Set());
  let songs = $state(50);
  let seconds = $state(0);
  // svelte-ignore state_referenced_locally
  let onCard = $state(gpu);
  let running = $state(false);
  let progress = $state({ done: 0, total: 0 });
  let lines = $state<string[]>([]);
  let result = $state<MeasureResult | null>(null);
  let startedAt = 0;

  const measurable = $derived(plans.filter((p) => p.installed && p.test));
  const notDownloaded = $derived(plans.filter((p) => !p.installed && p.test));
  const notMeasurable = $derived(plans.filter((p) => !p.test));

  onMount(() => {
    let off: (() => void) | undefined;
    void separationMeasurePlan()
      .then((p) => {
        plans = p;
        picked = new Set(p.filter((x) => x.installed && x.test).map((x) => x.id));
      })
      .catch(showError);
    void onSeparationMeasure((e) => {
      progress = { done: e.done, total: e.total };
      lines = [...lines.slice(-200), e.line];
    }).then((f) => (off = f));
    return () => off?.();
  });

  async function choose() {
    const f = await libraryPickFolder("Choose the MUSDB18-HQ folder (or its test folder)");
    if (f) {
      folder = f;
      prefs.set(FOLDER, f);
    }
  }

  function toggle(id: string) {
    const next = new Set(picked);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    picked = next;
  }

  async function start() {
    running = true;
    result = null;
    lines = [];
    progress = { done: 0, total: 0 };
    startedAt = Date.now();
    try {
      result = await separationMeasure(folder, [...picked], songs, seconds > 0 ? seconds : null, onCard);
    } catch (e) {
      showError(e);
    } finally {
      running = false;
    }
  }

  const eta = $derived.by(() => {
    if (!running || progress.done < 1 || progress.total < 1) return "";
    const per = (Date.now() - startedAt) / progress.done;
    const left = Math.round((per * (progress.total - progress.done)) / 60000);
    return left >= 60 ? `about ${Math.floor(left / 60)} h ${left % 60} min left` : `about ${left} min left`;
  });

  function close() {
    if (running) void separationCancel();
    onClose();
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && !running && onClose()} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop">
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="ms-title" tabindex="-1" onclick={(e) => e.stopPropagation()}>
    <h1 id="ms-title">Measure Scores</h1>
    <p class="note">
      Runs the downloaded models over the MUSDB18-HQ test songs on this computer and scores each result against the song's own recorded part (SDR, higher is better). Denoise, reverb and bleed models are scored on the songs' vocals with noise, a room or leakage added, so the right answer is known. Download MUSDB18-HQ yourself (about 22 GB) and pick its folder; Audiform downloads nothing for this.
    </p>

    <label class="row">
      <span>Test songs</span>
      <input type="text" readonly value={folder} placeholder="The MUSDB18-HQ folder" />
      <button type="button" disabled={running} onclick={() => void choose()}>Choose…</button>
    </label>
    <div class="row opts">
      <label>Songs <input type="number" min="1" max="50" bind:value={songs} disabled={running} /></label>
      <label
        >Length
        <select bind:value={seconds} disabled={running}>
          <option value={0}>Whole songs</option>
          <option value={60}>First 60 s</option>
          <option value={30}>First 30 s</option>
        </select>
      </label>
      <label><input type="checkbox" bind:checked={onCard} disabled={running} /> On the graphics card</label>
    </div>

    <fieldset disabled={running}>
      <legend>Models to measure ({picked.size} of {measurable.length} downloaded)</legend>
      <div class="models">
        {#each measurable as p (p.id)}
          <label title={p.test ?? ""}><input type="checkbox" checked={picked.has(p.id)} onchange={() => toggle(p.id)} /> {p.name}</label>
        {:else}
          <p class="note">No downloaded model can be measured yet.</p>
        {/each}
      </div>
      {#if notDownloaded.length || notMeasurable.length}
        <details>
          <summary>Not measured here ({notDownloaded.length + notMeasurable.length})</summary>
          <ul class="why">
            {#each notDownloaded as p (p.id)}<li><b>{p.name}</b>: not downloaded.</li>{/each}
            {#each notMeasurable as p (p.id)}<li><b>{p.name}</b>: {p.reason}.</li>{/each}
          </ul>
        </details>
      {/if}
    </fieldset>

    {#if running || lines.length}
      <div class="bar"><div style:width={`${progress.total ? Math.round((progress.done / progress.total) * 100) : 0}%`}></div></div>
      <div class="status">{progress.done} of {progress.total} runs{eta ? `, ${eta}` : ""}</div>
      <pre class="log">{lines.slice(-8).join("\n")}</pre>
    {/if}
    {#if result}
      <p class="note">
        {result.status === "done" ? "Done." : result.status === "cancelled" ? "Cancelled; what was measured is saved." : "Stopped; what was measured is saved."} The folder holds scores.json, scores.txt and run.log: send those to have the scores added to the model list.
      </p>
    {/if}

    <div class="buttons">
      {#if result}<button type="button" onclick={() => result && void revealMedia(result.json).catch(showError)}>Show Results</button>{/if}
      <span class="grow"></span>
      {#if running}
        <button type="button" onclick={() => void separationCancel()}>Cancel</button>
      {:else}
        <button type="button" onclick={close}>Close</button>
        <button type="button" class="primary" disabled={!folder || picked.size === 0} onclick={() => void start()}>Start</button>
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
    width: min(620px, 94vw);
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
  .note {
    margin: 0;
    color: var(--text-dim);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .row input[type="text"] {
    flex: 1;
    min-width: 0;
  }
  .opts {
    flex-wrap: wrap;
    gap: 14px;
  }
  .opts input[type="number"] {
    width: 4em;
  }
  fieldset {
    margin: 0;
    border: 1px solid var(--border);
  }
  .models {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
    gap: 2px 12px;
    max-height: 180px;
    overflow: auto;
  }
  .why {
    margin: 4px 0 0;
    padding-left: 18px;
    max-height: 140px;
    overflow: auto;
    color: var(--text-dim);
  }
  .bar {
    height: 6px;
    background: var(--surface-3, var(--surface-1));
  }
  .bar div {
    height: 100%;
    background: var(--accent);
  }
  .status {
    color: var(--text-dim);
  }
  .log {
    margin: 0;
    max-height: 9em;
    overflow: auto;
    padding: 6px;
    background: var(--surface-1);
    font-size: 11px;
    white-space: pre-wrap;
  }
  .buttons {
    display: flex;
    gap: 8px;
  }
  .grow {
    flex: 1;
  }
</style>
