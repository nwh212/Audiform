<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  A question with a few buttons, such as Save / Don't Save / Cancel.
  Enter picks the primary button, Escape picks the last (cancel) one.
-->
<script lang="ts" module>
  export interface ChoiceButton {
    label: string;
    value: string;
    primary?: boolean;
  }
</script>

<script lang="ts">
  interface Props {
    title: string;
    message: string;
    buttons: readonly ChoiceButton[];
    onChoose: (value: string) => void;
  }

  const { title, message, buttons, onChoose }: Props = $props();

  let primary = $state<HTMLButtonElement | null>(null);
  $effect(() => primary?.focus());

  function key(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      const last = buttons[buttons.length - 1];
      if (last) onChoose(last.value);
    }
  }
</script>

<svelte:window onkeydowncapture={key} />

<div class="backdrop">
  <div class="dialog" role="alertdialog" aria-modal="true" aria-labelledby="choice-title" aria-describedby="choice-msg">
    <h1 id="choice-title">{title}</h1>
    <p id="choice-msg">{message}</p>
    <footer>
      {#each buttons as b (b.value)}
        {#if b.primary}
          <button type="button" class="primary" bind:this={primary} onclick={() => onChoose(b.value)}>{b.label}</button>
        {:else}
          <button type="button" onclick={() => onChoose(b.value)}>{b.label}</button>
        {/if}
      {/each}
    </footer>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 5500;
    display: grid;
    place-items: center;
    background: rgba(0, 0, 0, 0.5);
  }

  .dialog {
    display: grid;
    gap: 10px;
    width: min(440px, 90vw);
    padding: 16px;
    border: 1px solid var(--border-strong);
    background: var(--surface-2);
    box-shadow: 0 16px 40px rgba(0, 0, 0, 0.6);
  }

  h1 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
  }

  p {
    margin: 0;
    color: var(--text-dim);
    line-height: 1.45;
    /* Blank lines in a message separate its paragraphs. */
    white-space: pre-line;
  }

  footer {
    display: flex;
    gap: 6px;
    justify-content: flex-end;
    margin-top: 4px;
  }

  button {
    min-width: 84px;
    height: 24px;
    padding: 0 12px;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    background: var(--surface-3);
    color: var(--text);
    font: inherit;
  }

  button:hover,
  button:focus-visible {
    border-color: var(--accent);
    outline: none;
  }

  button.primary {
    border-color: var(--accent-dim);
    background: var(--accent-dim);
    color: #fff;
  }
</style>
