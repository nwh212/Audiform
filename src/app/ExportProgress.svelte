<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  Progress for a running mixdown, with Cancel.
-->
<script lang="ts">
  interface Props {
    title: string;
    /** 0 to 1. */
    progress: number;
    onCancel: () => void;
    /** Small card in the corner; the app stays usable while it runs. */
    floating?: boolean;
  }

  const { title, progress, onCancel, floating = false }: Props = $props();
</script>

<div class="backdrop" class:floating>
  <div class="dialog" role="dialog" aria-modal={!floating} aria-labelledby="export-title">
    <h1 id="export-title">{title}</h1>
    <div class="bar" role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(progress * 100)}>
      <div style:width="{Math.round(progress * 100)}%"></div>
    </div>
    <footer>
      <span>{Math.round(progress * 100)}%</span>
      <button type="button" onclick={onCancel}>Cancel</button>
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

  .backdrop.floating {
    place-items: end end;
    padding: 36px 16px;
    background: none;
    pointer-events: none;
  }

  .floating .dialog {
    pointer-events: auto;
  }

  .dialog {
    display: grid;
    gap: 12px;
    width: min(360px, 90vw);
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

  footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    color: var(--text-dim);
    font-family: var(--font-mono);
    font-size: var(--font-size-small);
  }

  button {
    min-width: 72px;
    height: 22px;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    background: var(--surface-3);
    color: var(--text);
    font: inherit;
  }

  button:hover {
    border-color: var(--accent);
  }
</style>
