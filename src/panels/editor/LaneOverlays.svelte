<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  Copyright (c) The Audiform contributors.
  This file is part of Audiform, based on Altered Audio Editor.

  What the Multitrack lanes show over the clips while something happens:
  the stretch readout, where dragged-in files would land, the box being
  dragged to select clips, and where the Razor tool would cut.
-->
<script lang="ts">
  import type { DropGhost } from "./drop-ghost";

  interface Props {
    /** While Alt-dragging a clip edge: the stretch it would get. */
    stretchHint: string | null;
    /** Where hovering files would land, shown while `dropping`. */
    dropGhost: DropGhost | null;
    dropping: boolean;
    /** The box being dragged with the Move tool, in lane pixels. */
    selBox: { x: number; y: number; w: number; h: number } | null;
    /** Where a Razor click would cut: across one track, or all of them when `trackId` is null. */
    razor: { x: number; trackId: string | null } | null;
    /** The lanes, to find the razor's track row in. */
    lanesEl: HTMLElement | null;
  }

  const { stretchHint, dropGhost, dropping, selBox, razor, lanesEl }: Props = $props();
</script>

{#if stretchHint}<div class="stretch-hint" aria-live="polite">{stretchHint}</div>{/if}
{#if dropGhost && dropping}
  {#each dropGhost.items as g, k (k)}
    <div class="drop-ghost" style:left="{g.x}px" style:width="{g.w}px" style:top="{dropGhost.top + 1}px" style:height="{dropGhost.height - 2}px">
      <span>{g.name}{dropGhost.newTrack && k === 0 ? " (new track)" : ""}</span>
    </div>
  {/each}
{/if}
{#if selBox}
  <div class="selBox" style:left="{selBox.x}px" style:top="{selBox.y}px" style:width="{selBox.w}px" style:height="{selBox.h}px"></div>
{/if}
{#if razor}
  {@const row = razor.trackId ? lanesEl?.querySelector<HTMLElement>(`[data-track="${razor.trackId}"]`) : null}
  <div
    class="razor"
    style:left="{razor.x}px"
    style:top="{row ? row.offsetTop : 0}px"
    style:height={row ? `${row.offsetHeight}px` : "100%"}
  ></div>
{/if}

<style>
  .stretch-hint {
    position: absolute;
    top: 4px;
    right: 8px;
    z-index: 6;
    padding: 2px 6px;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    background: var(--surface-2);
    color: var(--text);
    font-size: 11px;
    pointer-events: none;
  }

  .selBox {
    position: absolute;
    z-index: 4;
    border: 1px dashed rgba(255, 255, 255, 0.85);
    background: rgba(255, 255, 255, 0.05);
    pointer-events: none;
  }

  .razor {
    position: absolute;
    z-index: 3;
    width: 0;
    border-left: 1px dashed var(--danger);
    pointer-events: none;
  }

  /* Where dragged-in files would land. */
  .drop-ghost {
    position: absolute;
    z-index: 5;
    box-sizing: border-box;
    overflow: hidden;
    border: 1px dashed var(--accent);
    border-radius: 2px;
    background: color-mix(in srgb, var(--accent) 22%, transparent);
    pointer-events: none;
  }

  .drop-ghost span {
    display: block;
    padding: 1px 4px;
    overflow: hidden;
    background: color-mix(in srgb, var(--accent) 55%, transparent);
    color: #fff;
    font-size: 11px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
