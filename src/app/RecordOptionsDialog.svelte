<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  Multitrack > Recording Options: the latency offset that lines takes up
  with what they were played against, and how punch-in recording works.
-->
<script lang="ts">
  import type { RecordOptions } from "./recording";

  interface Props {
    initial: RecordOptions;
    onApply: (o: RecordOptions) => void;
    onClose: () => void;
  }

  const { initial, onApply, onClose }: Props = $props();
  // svelte-ignore state_referenced_locally
  let o = $state<RecordOptions>({ ...initial });
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onClose()} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={onClose}>
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="ro-title" tabindex="-1" onclick={(e) => e.stopPropagation()}>
    <h1 id="ro-title">Recording Options</h1>
    <fieldset>
      <legend>Latency offset</legend>
      <label class="row">Trim from each take <input type="number" min="0" max="1000" step="1" bind:value={o.offsetMs} /> ms</label>
      <p class="note">Takes can land slightly late against the music or other tracks. To find the number: record a clap or a click track through your mic, zoom in, and measure how far the recorded clap sits after the original. Most USB interfaces land between 5 and 30 ms.</p>
    </fieldset>
    <fieldset>
      <legend>Punch-in</legend>
      <label><input type="checkbox" bind:checked={o.punch} /> With a time selection, record only into the selection</label>
      <label class="row">Play before it <input type="number" min="0" max="10" step="0.5" bind:value={o.prerollSec} disabled={!o.punch} /> seconds</label>
      <label><input type="checkbox" bind:checked={o.replace} disabled={!o.punch} /> Replace what was in the selection on the recorded tracks</label>
      <p class="note">Recording stops by itself at the end of the selection. Without a time selection, recording works as usual from the playhead.</p>
    </fieldset>
    <footer>
      <button type="button" class="primary" onclick={() => onApply({ ...o, offsetMs: Number(o.offsetMs) || 0, prerollSec: Number(o.prerollSec) || 0 })}>OK</button>
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
    width: min(460px, 94vw);
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
  fieldset {
    display: grid;
    gap: 6px;
    margin: 0;
    padding: 6px 8px 8px;
    border: 1px solid var(--border);
  }
  legend {
    padding: 0 4px;
    color: var(--text-dim);
  }
  .row {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  input[type="number"] {
    width: 64px;
    height: 22px;
    padding: 0 4px;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    background: var(--surface-1);
    color: var(--text);
    font: inherit;
  }
  .note {
    margin: 0;
    color: var(--text-dim);
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
