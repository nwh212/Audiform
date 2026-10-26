<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  Clip > Time Stretch: makes a clip play for a set length without
  changing its audio range. Keep pitch for speech and ad reads; change
  pitch with speed for a tape-style effect. Quick buttons fit common ad
  slot lengths, and Fit to Time Selection uses the selected range.
-->
<script lang="ts">
  import { NATURAL_LIMIT, parseSeconds, percent } from "../panels/editor/stretch";

  interface Props {
    clipName: string;
    /** Seconds at no stretch. */
    natural: number;
    /** Seconds now. */
    current: number;
    keepPitch: boolean;
    /** Seconds of the time selection, when there is one. */
    selection: number | null;
    stretched: boolean;
    onApply: (seconds: number, keepPitch: boolean, toSelection: boolean) => void;
    onReset: () => void;
    onClose: () => void;
  }

  const { clipName, natural, current, keepPitch, selection, stretched, onApply, onReset, onClose }: Props = $props();

  const fmt = (s: number) => {
    const m = Math.floor(s / 60);
    const rest = (s - m * 60).toFixed(3).padStart(6, "0");
    return m > 0 ? `${m}:${rest}` : (s).toFixed(3);
  };
  // svelte-ignore state_referenced_locally
  let text = $state(fmt(current));
  // svelte-ignore state_referenced_locally
  let pitch = $state(keepPitch);
  let pct = $state("");
  const secs = $derived(parseSeconds(text));
  const ratio = $derived(secs ? secs / natural : null);
  const far = $derived(ratio !== null && pitch && Math.abs(ratio - 1) > NATURAL_LIMIT);
  const outOfRange = $derived(ratio !== null && (ratio < 0.25 || ratio > 4));

  function fromPercent() {
    const p = Number(pct);
    if (Number.isFinite(p) && p > 0) text = fmt((natural * p) / 100);
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onClose()} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={onClose}>
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="st-title" tabindex="-1" onclick={(e) => e.stopPropagation()}>
    <h1 id="st-title">Time Stretch</h1>
    <p class="note">{clipName}: {fmt(natural)} s at natural speed{stretched ? `, now ${fmt(current)} s` : ""}.</p>

    <label class="row">
      <span>New length</span>
      <input type="text" bind:value={text} aria-label="New length in seconds, or minutes:seconds" />
    </label>
    <div class="quick" role="group" aria-label="Common lengths">
      <button type="button" onclick={() => (text = fmt(15))}>15 s</button>
      <button type="button" onclick={() => (text = fmt(30))}>30 s</button>
      <button type="button" onclick={() => (text = fmt(60))}>60 s</button>
      <button type="button" disabled={selection === null} title={selection === null ? "Make a time selection first." : "Fit the clip into the time selection and move it there."} onclick={() => selection !== null && onApply(selection, pitch, true)}>Fit to Time Selection</button>
    </div>
    <label class="row">
      <span>Or percent</span>
      <span class="pct"><input type="number" min="25" max="400" step="0.5" bind:value={pct} onchange={fromPercent} aria-label="Percent of natural length" /> % of natural length</span>
    </label>

    <fieldset>
      <label><input type="radio" name="mode" checked={pitch} onchange={() => (pitch = true)} /> Keep pitch (speech, ad reads)</label>
      <label><input type="radio" name="mode" checked={!pitch} onchange={() => (pitch = false)} /> Change pitch with speed (tape style)</label>
    </fieldset>

    {#if secs === null}
      <p class="warn">Type a length in seconds (30) or minutes and seconds (1:05.5).</p>
    {:else if outOfRange}
      <p class="warn">A clip can play from 25% to 400% of its natural length.</p>
    {:else if ratio !== null}
      <p class="note">That plays at {percent(ratio)} of natural length.{far ? " Beyond about 25% either way, speech starts to sound processed." : ""}</p>
    {/if}

    <footer>
      {#if stretched}<button type="button" onclick={onReset}>Remove Stretch</button>{/if}
      <span class="grow"></span>
      <button type="button" class="primary" disabled={secs === null || outOfRange} onclick={() => secs !== null && onApply(secs, pitch, false)}>Apply</button>
      <button type="button" onclick={onClose}>Cancel</button>
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
    width: min(440px, 94vw);
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
  .warn {
    margin: 0;
    color: var(--danger);
  }
  .row {
    display: grid;
    grid-template-columns: 84px minmax(0, 1fr);
    gap: 8px;
    align-items: center;
    color: var(--text-dim);
  }
  input[type="text"],
  input[type="number"] {
    height: 24px;
    padding: 0 6px;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    background: var(--surface-1);
    color: var(--text);
    font: inherit;
  }
  input[type="number"] {
    width: 70px;
  }
  .pct {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .quick {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    padding-left: 92px;
  }
  fieldset {
    display: grid;
    gap: 4px;
    margin: 0;
    padding: 6px 8px;
    border: 1px solid var(--border);
  }
  footer {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .grow {
    flex: 1;
  }
  .primary {
    border-color: var(--accent);
  }
</style>
