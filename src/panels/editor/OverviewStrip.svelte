<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  Overview strip above the timeline. Shows the whole session with a
  window over the part the lanes show.
  - Drag the window to scroll.
  - Drag either edge of the window to zoom in or out.
  - Click outside the window to jump there.
  - Wheel zooms around the pointer. Double-click fits the session.
-->
<script lang="ts">
  import { clipLength, type MediaRefDoc, type SessionDoc } from "../../stores/session";
  import { defaultTrackColor, dragOverviewEdge, overviewWindow, zoomAround, type Viewport } from "./timeline";

  interface Props {
    doc: SessionDoc | null;
    media: ReadonlyMap<string, MediaRefDoc>;
    view: Viewport;
    /** Width of the lanes in pixels. */
    lanesWidth: number;
    /** Span the strip covers, in session samples. */
    total: number;
    playhead: number;
    onView: (v: Viewport) => void;
    onFit: () => void;
  }

  const { doc, media, view, lanesWidth, total, playhead, onView, onFit }: Props = $props();

  const H = 22;
  const EDGE = 5;
  let width = $state(0);
  let canvas = $state<HTMLCanvasElement | null>(null);
  let hover = $state<"left" | "right" | "body" | null>(null);

  const win = $derived(overviewWindow(view, lanesWidth, total, width));

  function css(name: string): string {
    return getComputedStyle(document.documentElement).getPropertyValue(name).trim() || "#888";
  }

  $effect(() => {
    const c = canvas;
    if (!c || width <= 0) return;
    const dpr = window.devicePixelRatio || 1;
    c.width = Math.round(width * dpr);
    c.height = Math.round(H * dpr);
    const g = c.getContext("2d");
    if (!g) return;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    g.fillStyle = css("--surface-0");
    g.fillRect(0, 0, width, H);
    const k = width / Math.max(1, total);
    const tracks = doc?.tracks ?? [];
    const rowH = tracks.length ? Math.max(1, Math.min(4, (H - 4) / tracks.length)) : 0;
    tracks.forEach((t, i) => {
      for (const clip of t.clips) {
        const len = clipLength(clip, media.get(clip.media_id), doc!.sample_rate);
        const col = clip.color ?? t.color ?? defaultTrackColor(i);
        g.fillStyle = col.startsWith("--") ? css(col) : col;
        g.fillRect(clip.start * k, 2 + i * rowH, Math.max(1, len * k), Math.max(1, rowH - 1));
      }
    });
    g.fillStyle = css("--playhead");
    g.fillRect(Math.round(playhead * k), 0, 1, H);
  });

  function zoneAt(x: number): "left" | "right" | "body" | null {
    if (Math.abs(x - win.x) <= EDGE) return "left";
    if (Math.abs(x - (win.x + win.w)) <= EDGE) return "right";
    if (x > win.x && x < win.x + win.w) return "body";
    return null;
  }

  function localX(e: PointerEvent | WheelEvent): number {
    return e.clientX - (e.currentTarget as HTMLElement).getBoundingClientRect().left;
  }

  function onPointerDown(e: PointerEvent) {
    if (e.button !== 0) return;
    e.preventDefault();
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    const left = el.getBoundingClientRect().left;
    const startX = e.clientX - left;
    let zone = zoneAt(startX);
    const start = { ...view };
    const perPx = total / Math.max(1, width);
    if (zone === null) {
      // Jump: center the window on the click, then keep dragging it.
      const span = lanesWidth * view.spp;
      onView({ spp: view.spp, scroll: Math.max(0, startX * perPx - span / 2) });
      start.scroll = Math.max(0, startX * perPx - span / 2);
      zone = "body";
    }
    const z = zone;
    const move = (ev: PointerEvent) => {
      const x = ev.clientX - left;
      if (z === "body") onView({ spp: start.spp, scroll: Math.max(0, start.scroll + (x - startX) * perPx) });
      else onView(dragOverviewEdge(start, z, x, lanesWidth, total, width));
    };
    const up = () => {
      el.removeEventListener("pointermove", move);
      el.removeEventListener("pointerup", up);
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up);
  }

  function onWheel(e: WheelEvent) {
    e.preventDefault();
    // Anchor the zoom on the session spot under the pointer.
    const sample = (localX(e) / Math.max(1, width)) * total;
    const anchorX = Math.min(lanesWidth, Math.max(0, (sample - view.scroll) / view.spp));
    onView(zoomAround(view, e.deltaY > 0 ? 1.25 : 0.8, anchorX));
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="strip"
  bind:clientWidth={width}
  class:edge={hover === "left" || hover === "right"}
  class:body={hover === "body"}
  title="Overview. Drag the window to scroll, drag its edges to zoom, double-click to fit the session."
  onpointerdown={onPointerDown}
  onpointermove={(e) => (hover = zoneAt(localX(e)))}
  onpointerleave={() => (hover = null)}
  ondblclick={onFit}
  onwheel={onWheel}
>
  <canvas bind:this={canvas} style:width="{width}px" style:height="{H}px" aria-hidden="true"></canvas>
  <div class="window" style:left="{win.x}px" style:width="{win.w}px"></div>
</div>

<style>
  .strip {
    position: relative;
    flex: 1;
    min-width: 0;
    height: 22px;
    overflow: hidden;
    background: #08080b;
    cursor: pointer;
    user-select: none;
    touch-action: none;
  }

  .strip.body {
    cursor: grab;
  }

  .strip.edge {
    cursor: ew-resize;
  }

  canvas {
    display: block;
  }

  .window {
    position: absolute;
    top: 1px;
    bottom: 1px;
    box-sizing: border-box;
    border: 1px solid #b4b4bc;
    border-radius: 4px;
    background: rgba(255, 255, 255, 0.08);
    box-shadow: 0 0 0 1px rgba(0, 0, 0, 0.6);
    pointer-events: none;
  }

  /* Grips on both ends: drag them to zoom. */
  .window::before,
  .window::after {
    content: "";
    position: absolute;
    top: 3px;
    bottom: 3px;
    width: 3px;
    border-left: 1px solid #b4b4bc;
    border-right: 1px solid #b4b4bc;
  }

  .window::before {
    left: 2px;
  }

  .window::after {
    right: 2px;
  }
</style>
