<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Altered Audio Editor.

  Multitrack > Add Track > Add Tracks: several mono or stereo tracks at
  once, optionally named ("Guest" gives Guest, Guest 2, Guest 3), placed
  after the selected track, MIDI tracks (notes played by a VST3
  instrument), or bus tracks (added below the audio tracks). Surround
  tracks show why they are not offered.
-->
<script lang="ts">
  import { newTrackNames } from "./track-names";

  export interface AddTracksValues {
    count: number;
    stereo: boolean;
    /** Bus tracks instead of audio tracks. */
    bus: boolean;
    /** MIDI tracks instead of audio tracks. */
    midi: boolean;
    base: string;
  }

  type Kind = "mono" | "stereo" | "midi" | "bus";

  interface Props {
    existing: readonly string[];
    afterName: string | null;
    onApply: (v: AddTracksValues) => void;
    onClose: () => void;
  }

  const { existing, afterName, onApply, onClose }: Props = $props();
  let count = $state(2);
  let kind = $state<Kind>("mono");
  let base = $state("");
  const n = $derived(Math.max(1, Math.min(32, Math.round(Number(count) || 1))));
  const preview = $derived(newTrackNames(existing, Math.min(n, 4), base || (kind === "bus" ? "Bus" : kind === "midi" ? "MIDI" : "")));
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onClose()} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={onClose}>
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="at-title" tabindex="-1" onclick={(e) => e.stopPropagation()}>
    <h1 id="at-title">Add Tracks</h1>
    <label class="row"><span>How many</span><input type="number" min="1" max="32" bind:value={count} /></label>
    <label class="row"><span>Type</span>
      <select bind:value={kind}>
        <option value="mono">Mono audio (one voice or mic)</option>
        <option value="stereo">Stereo audio (music, beds, mixes)</option>
        <option value="midi">MIDI (notes played by an instrument plug-in)</option>
        <option value="bus">Stereo bus (groups tracks, takes sends)</option>
        <option disabled>5.1 surround (not planned for podcast work)</option>
      </select>
    </label>
    <label class="row"><span>Name</span><input type="text" placeholder="Track" bind:value={base} /></label>
    <p class="note">Adds {preview.join(", ")}{n > 4 ? `, and ${n - 4} more` : ""}{kind === "bus" ? " below the audio tracks" : afterName ? ` after ${afterName}` : " at the bottom"}.</p>
    <footer>
      <button type="button" class="primary" onclick={() => onApply({ count: n, stereo: kind !== "mono", bus: kind === "bus", midi: kind === "midi", base: base || (kind === "bus" ? "Bus" : kind === "midi" ? "MIDI" : "") })}>Add</button>
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
    width: min(400px, 94vw);
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
  .row {
    display: grid;
    grid-template-columns: 76px minmax(0, 1fr);
    gap: 8px;
    align-items: center;
    color: var(--text-dim);
  }
  input,
  select {
    height: 24px;
    padding: 0 6px;
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
