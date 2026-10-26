<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  File > Check Loudness: measures the mix (or the time selection) and
  shows pass or fail against a publishing target, with one-click master
  gain when that alone would reach the target safely.
-->
<script lang="ts">
  import { BROADCAST_REGIONS, evaluate, regionById, targetById, targetsFor, type Grade, type LoudnessReport } from "./loudness";

  interface Props {
    report: LoudnessReport | null;
    /** 0 to 1 while measuring, null when idle. */
    progress: number | null;
    targetId: string;
    /** Broadcast region id. */
    region: string;
    onRegion: (id: string) => void;
    /** "Whole session" or "Time selection". */
    scope: string;
    onTarget: (id: string) => void;
    onMeasure: () => void;
    onCancel: () => void;
    onApplyGain: (db: number) => void;
    onClose: () => void;
  }

  const { report, progress, targetId, region, onRegion, scope, onTarget, onMeasure, onCancel, onApplyGain, onClose }: Props = $props();

  const target = $derived(targetById(targetId, region));
  const targets = $derived(targetsFor(region));
  const verdict = $derived(report && !report.cancelled ? evaluate(report, target) : null);
  const n = (v: number | null | undefined, unit: string) => (v === null || v === undefined || !Number.isFinite(v) ? "-" : `${v.toFixed(1)} ${unit}`);
  const label: Record<Grade, string> = { pass: "Pass", close: "Close", fail: "Fix" };
  const time = (s: number) => `${Math.floor(s / 60)}:${String(Math.floor(s % 60)).padStart(2, "0")}`;
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && progress === null && onClose()} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={() => progress === null && onClose()}>
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="loud-title" tabindex="-1" onclick={(e) => e.stopPropagation()}>
    <h1 id="loud-title">Check Loudness</h1>
    <label class="target">
      <span>Target</span>
      <select value={targetId} onchange={(e) => onTarget(e.currentTarget.value)} disabled={progress !== null}>
        {#each targets as t (t.id)}
          <option value={t.id}>{t.name}</option>
        {/each}
      </select>
    </label>
    {#if target.id === "broadcast"}
      <label class="target">
        <span>Region</span>
        <select value={region} onchange={(e) => onRegion(e.currentTarget.value)} disabled={progress !== null}>
          {#each BROADCAST_REGIONS as r (r.id)}
            <option value={r.id}>{r.name}: {r.standard}, {r.lufs} LUFS, peaks {r.peak} dBTP</option>
          {/each}
        </select>
      </label>
      <p class="scope">Broadcasters can set their own rules. Check the delivery spec you were given; {regionById(region).standard} is the usual starting point.</p>
    {/if}
    <p class="scope">Measuring: {scope}. True peaks should stay at or below {target.peak} dBTP.</p>

    {#if progress !== null}
      <div class="bar"><div style:width="{Math.round(progress * 100)}%"></div></div>
      <p class="scope">Measuring the mix... {Math.round(progress * 100)}%</p>
    {:else if verdict && report}
      <div class="cards">
        <div class="card {verdict.loudness}">
          <span class="k">Integrated{target.mono ? " (mono)" : ""}</span>
          <span class="v">{n(verdict.measured, "LUFS")}</span>
          <span class="s">{verdict.offset > 0 ? "+" : ""}{verdict.offset.toFixed(1)} LU from {target.lufs}</span>
          <span class="badge">{label[verdict.loudness]}</span>
        </div>
        <div class="card {verdict.peak}">
          <span class="k">True peak</span>
          <span class="v">{n(report.truePeak, "dBTP")}</span>
          <span class="s">limit {target.peak} dBTP</span>
          <span class="badge">{label[verdict.peak]}</span>
        </div>
      </div>
      <dl>
        <dt>Loudness range</dt>
        <dd>{n(report.range, "LU")}</dd>
        <dt>Sample peak</dt>
        <dd>{n(report.samplePeak, "dBFS")}</dd>
        <dt>{target.mono ? "Stereo integrated" : "Mono integrated"}</dt>
        <dd>{n(target.mono ? report.integrated : report.integratedMono, "LUFS")}</dd>
        <dt>Length</dt>
        <dd>{time(report.seconds)}</dd>
      </dl>
      <p class="advice">{verdict.advice}</p>
    {:else}
      <p class="scope">Measure the mix to see how it compares with the target.</p>
    {/if}

    <footer>
      {#if progress !== null}
        <button type="button" onclick={onCancel}>Cancel</button>
      {:else}
        {#if verdict && verdict.loudness !== "pass" && verdict.gainIsSafe}
          <button type="button" onclick={() => onApplyGain(verdict.gainToTarget)}>
            {verdict.gainToTarget > 0 ? "Raise" : "Lower"} Master {Math.abs(verdict.gainToTarget).toFixed(1)} dB
          </button>
        {/if}
        <button type="button" onclick={onMeasure}>{report ? "Measure Again" : "Measure"}</button>
        <button type="button" class="primary" onclick={onClose}>Close</button>
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
    width: min(440px, 92vw);
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

  .target {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
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
    font: inherit;
  }

  .scope {
    margin: 0;
    color: var(--text-faint);
    font-size: var(--font-size-small);
  }

  .bar {
    height: 6px;
    border-radius: 3px;
    background: var(--surface-0);
    overflow: hidden;
  }

  .bar div {
    height: 100%;
    background: var(--accent);
  }

  .cards {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }

  .card {
    position: relative;
    display: grid;
    gap: 2px;
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-left: 3px solid var(--text-faint);
    border-radius: 3px;
    background: var(--surface-1);
  }

  .card.pass {
    border-left-color: var(--clip-1);
  }

  .card.close {
    border-left-color: var(--clip-5);
  }

  .card.fail {
    border-left-color: var(--danger);
  }

  .k {
    color: var(--text-dim);
    font-size: var(--font-size-small);
  }

  .v {
    color: var(--value);
    font-family: var(--font-mono);
    font-size: 18px;
  }

  .s {
    color: var(--text-faint);
    font-size: 10px;
  }

  .badge {
    position: absolute;
    top: 6px;
    right: 8px;
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .pass .badge {
    color: var(--clip-1);
  }

  .close .badge {
    color: var(--clip-5);
  }

  .fail .badge {
    color: var(--danger);
  }

  dl {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    gap: 2px 12px;
    margin: 0;
    font-size: var(--font-size-small);
  }

  dt {
    color: var(--text-dim);
  }

  dd {
    margin: 0;
    color: var(--value);
    font-family: var(--font-mono);
    text-align: right;
  }

  .advice {
    margin: 0;
    padding: 6px 8px;
    border-left: 2px solid var(--accent);
    background: var(--surface-1);
    color: var(--text);
    font-size: var(--font-size-small);
  }

  footer {
    display: flex;
    gap: 6px;
    justify-content: flex-end;
  }

  button {
    min-width: 72px;
    height: 22px;
    padding: 0 10px;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    background: var(--surface-3);
    color: var(--text);
    font: inherit;
  }

  button:hover {
    border-color: var(--accent);
  }

  button.primary {
    border-color: var(--accent);
  }
</style>
