<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  Copyright (c) The Audiform contributors.
  This file is part of Audiform, based on Altered Audio Editor.

  Track scrollbar: each track's color, the visible part as the thumb.
  Drag the thumb to scroll; drag its ends to make every track taller or
  shorter. The editor handles the presses; this draws the bar.
-->
<script lang="ts">
  import type { TrackDoc } from "../../stores/session";
  import { trackColor } from "./editor-layout";

  interface Props {
    /** The listed tracks, top to bottom. */
    tracks: TrackDoc[];
    /** A track's full height with its automation lanes, in pixels. */
    segHeight: (t: TrackDoc) => number;
    /** Height of all the rows. */
    rowsH: number;
    /** How far the rows are scrolled. */
    scrollTop: number;
    /** Height of the visible part. */
    scrollerH: number;
    onPointer: (e: PointerEvent) => void;
    onDouble: () => void;
    onWheel: (e: WheelEvent) => void;
  }

  const { tracks, segHeight, rowsH, scrollTop, scrollerH, onPointer, onDouble, onWheel }: Props = $props();

  /** Over the track scrollbar: a resize cursor near the thumb's ends. */
  function onHover(e: PointerEvent) {
    const el = e.currentTarget as HTMLElement;
    const bar = el.getBoundingClientRect();
    const scale = rowsH / Math.max(1, bar.height);
    const top = bar.top + scrollTop / scale;
    const bottom = top + scrollerH / scale;
    const near = Math.abs(e.clientY - top) < 7 || Math.abs(e.clientY - bottom) < 7;
    el.style.cursor = near ? "ns-resize" : "pointer";
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="vbar" onpointerdown={onPointer} onpointermove={onHover} ondblclick={onDouble} onwheel={onWheel} title="Drag to scroll the tracks. Drag the thumb's ends, or use the mouse wheel here, to zoom: every track the same height. Double-click to fit them all.">
  {#each tracks as track, i (track.id)}
    <div class="vseg" style:height="{(segHeight(track) / Math.max(1, rowsH)) * 100}%" style:background={trackColor(track.color, i)}></div>
  {/each}
  {#if rowsH > 0}
    <div class="vthumb" style:top="{(scrollTop / rowsH) * 100}%" style:height="{(scrollerH / rowsH) * 100}%">
      <span class="vend top"></span>
      <span class="vend bottom"></span>
    </div>
  {/if}
</div>

<style>
  .vbar {
    position: relative;
    display: flex;
    flex: none;
    flex-direction: column;
    width: 14px;
    border-left: 1px solid var(--border);
    background: var(--surface-0);
    cursor: pointer;
    user-select: none;
  }

  .vseg {
    flex: none;
    margin: 0 3px;
    opacity: 0.55;
    border-bottom: 1px solid var(--surface-0);
  }

  .vthumb {
    position: absolute;
    right: 0;
    left: 0;
    box-sizing: border-box;
    border: 1px solid #b4b4bc;
    border-radius: 4px;
    background: rgba(255, 255, 255, 0.1);
    pointer-events: none;
  }

  /* Grips on the thumb's ends: drag them to zoom vertically. */
  .vend {
    position: absolute;
    left: 1px;
    right: 1px;
    height: 5px;
    border-radius: 2px;
    background: repeating-linear-gradient(180deg, #d0d0d6 0 1px, transparent 1px 2px);
    opacity: 0.85;
  }

  .vend.top {
    top: 1px;
  }

  .vend.bottom {
    bottom: 1px;
  }
</style>
