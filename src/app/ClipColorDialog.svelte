<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  Picks a clip or track color from the palette or any custom color, or
  the default (the track's color for a clip). Escape cancels.
-->
<script lang="ts">
  interface Props {
    /** Current color: a token like "--clip-3", "#rrggbb", or null for the track color. */
    current: string | null;
    /** The picked color ("" for the track color), or null when cancelled. */
    onDone: (color: string | null) => void;
    title?: string;
    noneLabel?: string;
  }

  const { current, onDone, title = "Clip Color", noneLabel = "Use Track Color" }: Props = $props();
  const SWATCHES = ["--clip-1", "--clip-2", "--clip-3", "--clip-4", "--clip-5", "--clip-6"];
  const custom = $derived(current?.startsWith("#") ? current : "#7fa7d9");
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={() => onDone(null)} onkeydown={(e) => e.key === "Escape" && onDone(null)}>
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="color-title" tabindex="-1" onclick={(e) => e.stopPropagation()}>
    <h1 id="color-title">{title}</h1>
    <div class="swatches">
      {#each SWATCHES as c (c)}
        <button
          type="button"
          class:on={current === c}
          style:background="var({c})"
          aria-label="Color {c.slice(-1)}"
          onclick={() => onDone(c)}
        ></button>
      {/each}
    </div>
    <label class="custom">
      <span>Other color</span>
      <input type="color" value={custom} onchange={(e) => onDone(e.currentTarget.value)} />
    </label>
    <footer>
      <button type="button" class="text" class:on={current === null} onclick={() => onDone("")}>{noneLabel}</button>
      <button type="button" class="text" onclick={() => onDone(null)}>Cancel</button>
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

  .swatches {
    display: grid;
    grid-template-columns: repeat(6, 32px);
    gap: 8px;
  }

  .swatches button {
    width: 32px;
    height: 24px;
    border: 2px solid transparent;
    border-radius: 3px;
  }

  .swatches button:hover,
  .swatches button.on {
    border-color: #fff;
  }

  .custom {
    display: flex;
    gap: 8px;
    align-items: center;
    font-size: var(--font-size-small);
  }

  .custom input {
    width: 48px;
    height: 24px;
    padding: 0;
    border: 1px solid var(--border-strong);
    background: none;
  }

  footer {
    display: flex;
    gap: 6px;
    justify-content: space-between;
  }

  .text {
    height: 22px;
    padding: 0 10px;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    background: var(--surface-3);
    color: var(--text);
    font: inherit;
  }

  .text:hover,
  .text.on {
    border-color: var(--accent);
  }
</style>
