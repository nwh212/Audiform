<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  Copyright (c) The Audiform contributors.
  This file is part of Audiform, based on Altered Audio Editor.

  Nothing open yet: the same quiet screen as the Waveform editor's.
  The lanes stay underneath, so files dropped here still land.
-->
<script lang="ts">
  interface Props {
    /** Where the screen starts: just under the toolbar. */
    top: number;
    onMenu: (e: MouseEvent) => void;
  }

  const { top, onMenu }: Props = $props();
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="start"
  style:top="{top}px"
  oncontextmenu={(e) => {
    e.preventDefault();
    onMenu(e);
  }}
>
  <p>No session is open in the Multitrack editor.</p>
  <p class="start-hint">Double-click a session or file in the Files panel, drag files here, add a track, or use File &gt; New Session or File &gt; Open.</p>
</div>

<style>
  /* Over everything below the toolbar, as the Waveform editor's empty screen. */
  .start {
    position: absolute;
    right: 0;
    bottom: 0;
    left: 0;
    z-index: 30;
    display: grid;
    align-content: center;
    justify-items: center;
    gap: 4px;
    padding: 0 16px;
    background: var(--surface-0);
    color: var(--text-dim);
    text-align: center;
  }

  .start p {
    margin: 0;
  }

  .start-hint {
    color: var(--text-faint);
    font-size: var(--font-size-small);
  }
</style>
