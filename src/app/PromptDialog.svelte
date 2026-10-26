<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  Asks for one line of text, such as a name. Enter confirms, Escape cancels.
-->
<script lang="ts">
  interface Props {
    title: string;
    label: string;
    value?: string;
    confirm?: string;
    onDone: (value: string | null) => void;
  }

  const { title, label, value = "", confirm = "OK", onDone }: Props = $props();

  // svelte-ignore state_referenced_locally
  let text = $state(value);
  let input = $state<HTMLInputElement | null>(null);

  $effect(() => {
    input?.focus();
    input?.select();
  });

  function key(e: KeyboardEvent) {
    e.stopPropagation();
    if (e.key === "Enter") {
      e.preventDefault();
      if (text.trim()) onDone(text.trim());
    } else if (e.key === "Escape") {
      e.preventDefault();
      onDone(null);
    }
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={() => onDone(null)}>
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="prompt-title" tabindex="-1" onclick={(e) => e.stopPropagation()}>
    <h1 id="prompt-title">{title}</h1>
    <label>
      <span>{label}</span>
      <input type="text" spellcheck="false" bind:value={text} bind:this={input} onkeydown={key} />
    </label>
    <footer>
      <button type="button" onclick={() => onDone(null)}>Cancel</button>
      <button type="button" class="primary" disabled={!text.trim()} onclick={() => onDone(text.trim())}>{confirm}</button>
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
    width: min(380px, 90vw);
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

  label {
    display: grid;
    gap: 4px;
    color: var(--text-dim);
  }

  input {
    height: 24px;
    padding: 0 8px;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    background: var(--surface-1);
    color: var(--text);
    font: inherit;
  }

  input:focus {
    border-color: var(--accent);
    outline: none;
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

  button:hover:not(:disabled) {
    border-color: var(--accent);
  }

  button:disabled {
    color: var(--text-faint);
  }
</style>
