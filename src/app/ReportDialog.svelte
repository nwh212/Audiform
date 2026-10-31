<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The Audiform contributors.
  This file is part of Audiform, based on Altered Audio Editor.

  Help > Report a Problem: what happened, how to see it, and what was
  expected, with the version, system and theme added (they can be left
  out). Nothing is sent from Audiform: Open Report puts the filled-in
  report on the project's issue page in the browser, to send from there,
  and Copy Report copies it to paste anywhere.
-->
<script lang="ts">
  import { PLATFORM } from "../lib/platform";
  import { settings } from "../stores/settings";
  import { openLink } from "./core";
  import { reportText, reportUrl } from "./report";

  interface Props {
    version: string | null;
    onNotice: (text: string) => void;
    onClose: () => void;
  }

  const { version, onNotice, onClose }: Props = $props();

  let what = $state("");
  let steps = $state("");
  let expected = $state("");
  let withDetails = $state(true);
  const SYSTEM = { windows: "Windows", macos: "macOS", linux: "Linux" } as const;
  const details = $derived(`Audiform ${version ?? "(version unknown)"} on ${SYSTEM[PLATFORM]}, theme ${settings.current.theme}`);
  const input = $derived({ what, steps, expected, details: withDetails ? details : null });

  async function open() {
    try {
      await openLink(reportUrl(input));
      onNotice("The report opened in your browser. Check it over and send it from there.");
      onClose();
    } catch (e) {
      onNotice(`The browser did not open (${e instanceof Error ? e.message : String(e)}). Use Copy Report instead.`);
    }
  }

  async function copy() {
    const { title, body } = reportText(input);
    try {
      await navigator.clipboard.writeText(`${title}\n\n${body}`);
      onNotice("Report copied.");
    } catch {
      onNotice("Copying was refused. Select the text and copy it instead.");
    }
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onClose()} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={onClose}>
  <div class="dialog" role="dialog" tabindex="-1" aria-modal="true" aria-labelledby="report-title" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()}>
    <h1 id="report-title">Report a Problem</h1>
    <p class="note">Tell us what went wrong. Nothing is sent from Audiform: Open Report fills in a report on the project's issue page in your browser, for you to check and send.</p>
    <label>
      <span>What happened</span>
      <textarea rows="4" bind:value={what} placeholder="Playback stopped after a minute in a long session"></textarea>
    </label>
    <label>
      <span>Steps to see it <em>(if you know them)</em></span>
      <textarea rows="3" bind:value={steps} placeholder="1. Open a session with 12 tracks&#10;2. Press Space"></textarea>
    </label>
    <label>
      <span>What you expected <em>(optional)</em></span>
      <textarea rows="2" bind:value={expected}></textarea>
    </label>
    <label class="check"><input type="checkbox" bind:checked={withDetails} /> Include: {details}</label>
    <footer>
      <button type="button" onclick={onClose}>Cancel</button>
      <button type="button" disabled={!what.trim()} onclick={() => void copy()}>Copy Report</button>
      <button type="button" class="primary" disabled={!what.trim()} onclick={() => void open()}>Open Report</button>
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
    width: min(520px, 94vw);
    padding: 16px 18px 14px;
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
    font-size: 11px;
    line-height: 1.4;
  }
  label {
    display: grid;
    gap: 3px;
    color: var(--text-dim);
    font-size: 11px;
  }
  em {
    color: var(--text-faint);
    font-style: normal;
  }
  textarea {
    resize: vertical;
    padding: 6px 8px;
    border: 1px solid var(--border-strong);
    background: var(--surface-1);
    color: var(--text);
    font: inherit;
    font-size: 12px;
  }
  textarea:focus {
    border-color: var(--accent);
    outline: none;
  }
  .check {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  footer {
    display: flex;
    gap: 6px;
    justify-content: flex-end;
  }
  button {
    min-width: 84px;
    height: 24px;
    padding: 0 10px;
    border: 1px solid var(--border-strong);
    background: var(--surface-3);
    color: var(--text);
    font: inherit;
  }
  button.primary {
    border-color: var(--accent);
  }
  button:disabled {
    color: var(--text-faint);
  }
</style>
