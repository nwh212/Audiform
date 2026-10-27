<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  Copyright (c) The Audiform contributors.
  This file is part of Audiform, based on Altered Audio Editor.

  The Multitrack editor's top strip: add a track or bus, and the session
  sample rate.
-->
<script lang="ts">
  interface Props {
    /** The session sample rate, shown when a session is open. */
    rate: number;
    hasDoc: boolean;
    run: (id: string) => void;
    /** A button's tooltip with its shortcut. */
    tip: (label: string, id: string) => string;
    /** The strip's height, for the start screen to sit under it. */
    height?: number;
  }

  let { rate, hasDoc, run, tip, height = $bindable(0) }: Props = $props();
</script>

<div class="toolbar" bind:offsetHeight={height}>
  <button type="button" class="tool text" title={tip("Add Mono Track", "multitrack.add.mono")} onclick={() => run("multitrack.add.mono")}>+ Mono</button>
  <button type="button" class="tool text" title={tip("Add Stereo Track", "multitrack.add.stereo")} onclick={() => run("multitrack.add.stereo")}>+ Stereo</button>
  <button type="button" class="tool text" title={tip("Add MIDI Track", "multitrack.add.midi")} onclick={() => run("multitrack.add.midi")}>+ MIDI</button>
  <button type="button" class="tool text" title={tip("Add Bus Track", "multitrack.add.bus")} onclick={() => run("multitrack.add.bus")}>+ Bus</button>
  <span class="spacer"></span>
  {#if hasDoc}<span class="rate" title="Session sample rate">{(rate / 1000).toLocaleString("en-US")} kHz</span>{/if}
</div>

<style>
  .toolbar {
    display: flex;
    gap: 2px;
    align-items: center;
    padding: 3px 6px;
    border-bottom: 1px solid var(--border);
    background: var(--surface-1);
  }

  .tool {
    display: grid;
    place-items: center;
    min-width: 22px;
    height: 20px;
    padding: 0 4px;
    border: 1px solid transparent;
    border-radius: 2px;
    background: none;
    color: var(--text-dim);
    font: inherit;
    font-size: var(--font-size-small);
  }

  .tool:hover:not(:disabled) {
    border-color: var(--border-strong);
    color: var(--text);
  }

  .tool:disabled {
    opacity: 0.4;
  }

  .spacer {
    flex: 1;
  }

  .toolbar .rate {
    padding: 0 4px;
    color: var(--text-faint);
    font-size: var(--font-size-small);
    white-space: nowrap;
  }
</style>
