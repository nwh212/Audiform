<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  Clip > Fit Music to Length: makes a music clip an exact length by
  cutting out or repeating whole sections at musical joins, so it keeps
  its speed and pitch. Find Joins offers up to three plans that join in
  different places; Apply puts the chosen one on the track with short
  crossfades, grouped, as one undoable step.
-->
<script lang="ts">
  import { describeJoins, describePlan, type FitPlan } from "../panels/editor/musicfit";
  import { parseSeconds } from "../panels/editor/stretch";

  interface Props {
    clipName: string;
    /** Seconds into its file where the clip starts. */
    origin: number;
    /** Seconds the clip plays now. */
    current: number;
    /** Seconds of the time selection, when there is one. */
    selection: number | null;
    plan: (seconds: number) => Promise<FitPlan[]>;
    onApply: (p: FitPlan) => void;
    onClose: () => void;
  }

  const { clipName, origin, current, selection, plan, onApply, onClose }: Props = $props();

  const fmt = (s: number) => {
    const m = Math.floor(s / 60);
    const rest = (s - m * 60).toFixed(3).padStart(6, "0");
    return m > 0 ? `${m}:${rest}` : s.toFixed(3);
  };
  // svelte-ignore state_referenced_locally
  let text = $state(fmt(selection ?? current));
  const secs = $derived(parseSeconds(text));
  let busy = $state(false);
  let plans = $state<FitPlan[]>([]);
  let pick = $state(0);
  const result = $derived(plans[pick] ?? null);
  let error = $state<string | null>(null);
  // A new length needs a new plan.
  let planned = $state<number | null>(null);
  const stale = $derived(result !== null && planned !== secs);

  async function find() {
    if (!secs) return;
    busy = true;
    error = null;
    plans = [];
    pick = 0;
    try {
      plans = await plan(secs);
      planned = secs;
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onClose()} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={onClose}>
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="mf-title" tabindex="-1" onclick={(e) => e.stopPropagation()}>
    <h1 id="mf-title">Fit Music to Length</h1>
    <p class="note">{clipName}: plays {fmt(current)} s now. Sections are cut out or repeated where the music lines up, so the speed and pitch stay the same.</p>

    <label class="row">
      <span>New length</span>
      <input type="text" spellcheck="false" bind:value={text} onkeydown={(e) => e.key === "Enter" && void find()} />
    </label>
    <div class="quick">
      {#each [15, 30, 60] as q (q)}<button type="button" onclick={() => (text = fmt(q))}>{q} s</button>{/each}
      {#if selection}<button type="button" onclick={() => (text = fmt(selection))}>Time selection ({fmt(selection)} s)</button>{/if}
    </div>
    {#if !secs}<p class="warn">Type a length like 30 or 1:15.5.</p>{/if}

    {#if busy}
      <p class="note">Listening for the beat and the sections…</p>
    {:else if error}
      <p class="warn">{error}</p>
    {:else if result}
      {#if stale}<p class="note">The length changed: find joins again.</p>{/if}
      <div class="plans" role="radiogroup" aria-label="Plans" class:dim={stale}>
        {#each plans as p, k (k)}
          <label class="plan">
            <input type="radio" name="mf-plan" value={k} bind:group={pick} disabled={stale} />
            <span>
              <strong>{k === 0 ? "Best" : `Option ${k + 1}`}</strong>: {describePlan(p)}
              <span class="joins">{describeJoins(p, origin)}</span>
            </span>
          </label>
        {/each}
      </div>
      {#if result.quality < 0.8 && !stale}<p class="warn">The join is not a close match and may be heard. A length a few seconds either way, or one that fits whole bars, often joins cleaner.</p>{/if}
    {/if}

    <div class="buttons">
      <button type="button" onclick={onClose}>Cancel</button>
      <button type="button" disabled={!secs || busy} onclick={() => void find()}>Find Joins</button>
      <button type="button" class="primary" disabled={!result || stale || busy} onclick={() => result && onApply(result)}>Apply</button>
    </div>
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
    font-size: 12px;
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
  .plans {
    display: grid;
    gap: 4px;
  }
  .plan {
    display: flex;
    gap: 6px;
    align-items: flex-start;
  }
  .plan input {
    margin: 2px 0 0;
  }
  .joins {
    display: block;
    color: var(--text-dim);
  }
  .dim {
    color: var(--text-faint);
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
  button {
    height: 24px;
    padding: 0 6px;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    background: var(--surface-1);
    color: var(--text);
    font: inherit;
  }
  .quick {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    padding-left: 92px;
  }
  .buttons {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
  }
  .buttons button {
    min-width: 72px;
  }
  .primary {
    border-color: var(--accent);
    background: var(--accent);
    color: var(--on-accent, #fff);
  }
</style>
