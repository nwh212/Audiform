<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  Effects > Loudness Match: measures each selected clip, or each track
  when no clips are selected, and sets clip gain so each one reaches the
  chosen loudness target, or a level by its peak, true peak or average
  (RMS). Rows that would push peaks over the limit are flagged and left
  unticked, and so are rows already within the tolerance. Applying is
  one undoable step.
-->
<script lang="ts">
  import { BROADCAST_REGIONS, targetById, targetsFor } from "./loudness";
  import type { MatchBasis, MatchRow } from "./loudness-match";

  interface Props {
    mode: "clips" | "tracks";
    rows: MatchRow[] | null;
    /** 0 to 1 while measuring, null when idle. */
    progress: number | null;
    targetId: string;
    region: string;
    basis: MatchBasis;
    /** The dB level for a Peak, True Peak or RMS match. */
    level: number;
    /** Rows within this many dB of the target are left alone. */
    tolerance: number;
    onBasis: (b: MatchBasis) => void;
    onLevel: (db: number) => void;
    onTolerance: (db: number) => void;
    onTarget: (id: string) => void;
    onRegion: (id: string) => void;
    onMeasure: () => void;
    onCancel: () => void;
    onApply: (rows: MatchRow[]) => void;
    onClose: () => void;
  }

  const { mode, rows, progress, targetId, region, basis, level, tolerance, onBasis, onLevel, onTolerance, onTarget, onRegion, onMeasure, onCancel, onApply, onClose }: Props = $props();

  const target = $derived(targetById(targetId, region));
  const targets = $derived(targetsFor(region));
  let skip = $state<ReadonlySet<string>>(new Set());
  $effect(() => {
    // New measurements: untick rows that would clip or cannot change.
    skip = new Set((rows ?? []).filter((r) => !r.measurable || r.peakWarning || Math.abs(r.change) <= tolerance).map((r) => r.key));
  });
  const chosen = $derived((rows ?? []).filter((r) => r.measurable && !skip.has(r.key) && r.change !== 0));
  const db = (v: number) => (Number.isFinite(v) ? `${v > 0 ? "+" : ""}${v.toFixed(1)}` : "-");
  function toggle(key: string) {
    const next = new Set(skip);
    if (next.has(key)) next.delete(key);
    else next.add(key);
    skip = next;
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && progress === null && onClose()} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={() => progress === null && onClose()}>
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="lm-title" tabindex="-1" onclick={(e) => e.stopPropagation()}>
    <h1 id="lm-title">Loudness Match</h1>
    <p class="note">
      {mode === "clips" ? "Each selected clip is measured on its own and its gain set to reach the target." : "Each track is measured as a whole and the same gain change goes to every clip on it. Select clips first to match them one by one."}
    </p>
    <label class="row">
      <span>Match by</span>
      <select value={basis} disabled={progress !== null} onchange={(e) => onBasis(e.currentTarget.value as MatchBasis)}>
        <option value="loudness">Loudness (LUFS)</option>
        <option value="peak">Peak (loudest sample)</option>
        <option value="truePeak">True peak (between samples too)</option>
        <option value="rms">RMS (average level)</option>
      </select>
    </label>
    {#if basis !== "loudness"}
      <label class="row">
        <span>Level</span>
        <input type="number" min="-60" max="0" step="0.5" value={level} disabled={progress !== null} onchange={(e) => onLevel(Math.max(-60, Math.min(0, Number(e.currentTarget.value) || 0)))} />
        <span class="unit">dB</span>
      </label>
    {/if}
    <label class="row" title="Clips or tracks already this close to the target are left unticked">
      <span>Tolerance</span>
      <input type="number" min="0" max="6" step="0.1" value={tolerance} disabled={progress !== null} onchange={(e) => onTolerance(Math.max(0, Math.min(6, Number(e.currentTarget.value) || 0)))} />
      <span class="unit">± dB</span>
    </label>
    <label class="row" hidden={basis !== "loudness"}>
      <span>Target</span>
      <select value={targetId} disabled={progress !== null} onchange={(e) => onTarget(e.currentTarget.value)}>
        {#each targets as t (t.id)}<option value={t.id}>{t.name}</option>{/each}
      </select>
    </label>
    {#if basis === "loudness" && target.id === "broadcast"}
      <label class="row">
        <span>Region</span>
        <select value={region} disabled={progress !== null} onchange={(e) => onRegion(e.currentTarget.value)}>
          {#each BROADCAST_REGIONS as r (r.id)}<option value={r.id}>{r.name}: {r.standard}, {r.lufs} LUFS</option>{/each}
        </select>
      </label>
    {/if}

    {#if progress !== null}
      <div class="bar"><div style:width="{Math.round(progress * 100)}%"></div></div>
      <p class="note">Measuring... {Math.round(progress * 100)}%</p>
    {:else if rows}
      <div class="table" aria-label="Measurements">
        <div class="head"><span></span><span>{mode === "clips" ? "Clip" : "Track"}</span><span>Now</span><span>Change</span><span>Peak after</span></div>
        {#each rows as r (r.key)}
          <label class="line" class:warn={r.peakWarning} class:off={!r.measurable}>
            <input type="checkbox" checked={r.measurable && !skip.has(r.key)} disabled={!r.measurable} onchange={() => toggle(r.key)} />
            <span class="nm">{r.label}</span>
            <span>{r.measurable ? `${r.measured.toFixed(1)} ${basis === "loudness" ? "LUFS" : "dB"}` : "Silent"}</span>
            <span>{r.measurable ? `${db(r.change)} dB` : "-"}</span>
            <span>{r.measurable ? `${db(r.peakAfter)} dBTP` : "-"}</span>
          </label>
        {/each}
      </div>
      {#if rows.some((r) => r.peakWarning)}
        <p class="note warn">Rows marked in red would peak above {basis === "loudness" ? `${target.peak} dBTP` : "full scale (0 dBTP)"}, so they start unticked. Lower their loudest moments or limit them (limiting arrives with effects), or tick them to apply anyway.</p>
      {/if}
    {:else}
      <p class="note">Measure to see how far each {mode === "clips" ? "clip" : "track"} is from {basis === "loudness" ? `${target.lufs} LUFS` : `${level} dB ${basis === "peak" ? "peak" : basis === "truePeak" ? "true peak" : "RMS"}`}.</p>
    {/if}

    <footer>
      {#if progress !== null}
        <button type="button" onclick={onCancel}>Cancel</button>
      {:else}
        <button type="button" onclick={onMeasure}>{rows ? "Measure Again" : "Measure"}</button>
        <button type="button" class="primary" disabled={chosen.length === 0} onclick={() => onApply(chosen)}>Apply to {chosen.length} {chosen.length === 1 ? "item" : "items"}</button>
        <button type="button" onclick={onClose}>Close</button>
      {/if}
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
    width: min(560px, 94vw);
    max-height: 86vh;
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
  .note {
    margin: 0;
    color: var(--text-dim);
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
  .bar {
    height: 6px;
    background: var(--surface-0);
  }
  .bar div {
    height: 100%;
    background: var(--accent);
  }
  .table {
    max-height: 40vh;
    overflow: auto;
    border: 1px solid var(--border);
    background: var(--surface-0);
  }
  .head,
  .line {
    display: grid;
    grid-template-columns: 22px minmax(0, 1fr) 86px 74px 86px;
    gap: 6px;
    align-items: center;
    padding: 3px 8px;
    font-variant-numeric: tabular-nums;
  }
  .head {
    position: sticky;
    top: 0;
    background: var(--surface-1);
    color: var(--text-dim);
  }
  .nm {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .warn {
    color: var(--danger);
  }
  .off {
    color: var(--text-faint);
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
