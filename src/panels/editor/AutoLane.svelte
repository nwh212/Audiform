<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  One automation lane under a track or bus: a line for one setting
  (volume, pan or an effect control) over the session's time. Click to
  add a point and drag to place it; drag a point to move it; double-click
  a point to remove it. Volume snaps to 0 dB and pan to center when close
  (hold Alt to place freely). Each finished change is one undoable step.
-->
<script lang="ts">
  import type { EnvelopeDoc, EnvelopePointDoc } from "../../stores/session";
  import { autoAdd, autoAt, autoFormat, autoMove, autoSnap, autoValue, autoY, type AutoRange } from "./automation";
  import { sampleAt, xOf, TRACK_GAP, type Viewport } from "./timeline";

  interface Props {
    lane: EnvelopeDoc;
    range: AutoRange;
    /** Where the line sits with no points. */
    rest: number;
    view: Viewport;
    width: number;
    height: number;
    /** The owner's color, as CSS. */
    color: string;
    playhead: number;
    /** Replaces the lane's points as one undoable edit. */
    onPoints: (points: EnvelopePointDoc[], label: string) => void;
    /** Right-click, with the point under the pointer (or -1) and the time there. */
    onMenu: (e: MouseEvent, index: number, pos: number) => void;
  }

  const { lane, range, rest, view, width, height, color, playhead, onPoints, onMenu }: Props = $props();

  /** Points while a drag is under way; the lane's own otherwise. */
  let draft = $state<EnvelopePointDoc[] | null>(null);
  let tip = $state<{ x: number; y: number; text: string } | null>(null);
  let hover = $state(-1);
  const pts = $derived(draft ?? lane.points);

  const top = 5;
  const h = $derived(Math.max(8, height - TRACK_GAP - 10));
  const y = (v: number) => autoY(v, range, top, h);
  const restY = $derived(y(rest));

  const path = $derived.by(() => {
    if (pts.length === 0) return `M0 ${restY} L${width} ${restY}`;
    const xs = pts.map((p) => xOf(p.position, view));
    let d = `M0 ${y(pts[0]!.value)}`;
    for (let i = 0; i < pts.length; i++) d += ` L${xs[i]} ${y(pts[i]!.value)}`;
    return `${d} L${width} ${y(pts[pts.length - 1]!.value)}`;
  });
  const visible = $derived(
    pts.map((p, i) => ({ i, x: xOf(p.position, view), y: y(p.value) })).filter((p) => p.x >= -6 && p.x <= width + 6),
  );
  const guide = $derived(lane.target.type === "volume" ? y(0) : lane.target.type === "pan" ? y(0) : null);
  const playX = $derived(xOf(playhead, view));

  function hit(px: number, py: number): number {
    let best = -1;
    let dist = 7;
    for (const p of visible) {
      const d = Math.hypot(p.x - px, p.y - py);
      if (d <= dist) {
        dist = d;
        best = p.i;
      }
    }
    return best;
  }

  function local(e: MouseEvent): [number, number] {
    const r = (e.currentTarget as Element).getBoundingClientRect();
    return [e.clientX - r.left, e.clientY - r.top];
  }

  function valueAt(py: number, alt: boolean): number {
    const v = autoValue(py, range, top, h);
    return alt ? v : autoSnap(lane.target, v);
  }

  function showTip(px: number, py: number, v: number) {
    tip = { x: px, y: py, text: autoFormat(lane.target, v, range) };
  }

  function onDown(e: PointerEvent) {
    if (e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    const el = e.currentTarget as Element;
    const [px, py] = local(e);
    let i = hit(px, py);
    let list = lane.points.map((p) => ({ ...p }));
    let label = "Move Automation Point";
    if (i < 0) {
      const added = autoAdd(list, Math.max(0, sampleAt(px, view)), valueAt(py, e.altKey));
      list = added.points;
      i = added.index;
      label = "Add Automation Point";
    }
    draft = list;
    showTip(px, py, list[i]!.value);
    const rect = el.getBoundingClientRect();
    const x0 = e.clientX;
    const y0 = e.clientY;
    const p0 = list[i]!;
    let moved = label === "Add Automation Point";
    const move = (ev: PointerEvent) => {
      if (!moved && Math.hypot(ev.clientX - x0, ev.clientY - y0) < 3) return;
      moved = true;
      const qx = ev.clientX - rect.left;
      const qy = ev.clientY - rect.top;
      // Shift keeps the time and moves only the value.
      const pos = ev.shiftKey ? p0.position : Math.max(0, sampleAt(qx, view));
      const v = valueAt(qy, ev.altKey);
      draft = autoMove(list, i, pos, v);
      showTip(qx, qy, v);
    };
    const up = () => {
      window.removeEventListener("pointermove", move, true);
      window.removeEventListener("pointerup", up, true);
      const out = draft;
      draft = null;
      tip = null;
      if (out && moved) onPoints(out, label);
    };
    window.addEventListener("pointermove", move, true);
    window.addEventListener("pointerup", up, true);
  }

  function onMove(e: PointerEvent) {
    if (draft) return;
    const [px, py] = local(e);
    hover = hit(px, py);
    const p = hover >= 0 ? pts[hover] : undefined;
    showTip(px, p ? y(p.value) : py, p ? p.value : autoAt(pts, sampleAt(px, view), rest));
  }

  function onDouble(e: MouseEvent) {
    const [px, py] = local(e);
    const i = hit(px, py);
    if (i < 0) return;
    e.stopPropagation();
    onPoints(
      lane.points.filter((_, k) => k !== i),
      "Delete Automation Point",
    );
  }

  function onContext(e: MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
    const [px, py] = local(e);
    onMenu(e, hit(px, py), Math.max(0, sampleAt(px, view)));
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="auto-lane"
  class:off={!lane.enabled}
  style:height="{height}px"
  style:--c={color}
  onpointerdown={onDown}
  onpointermove={onMove}
  onpointerleave={() => {
    if (!draft) {
      tip = null;
      hover = -1;
    }
  }}
  ondblclick={onDouble}
  oncontextmenu={onContext}
>
  <svg {width} height={height - TRACK_GAP} aria-hidden="true">
    {#if guide !== null}<line class="guide" x1="0" x2={width} y1={guide} y2={guide} />{/if}
    <path class="fill" d="{path} L{width} {top + h} L0 {top + h} Z" />
    <path class="line" d={path} />
    {#each visible as p (p.i)}
      <circle class="pt" class:hot={p.i === hover} cx={p.x} cy={p.y} r={p.i === hover ? 4.5 : 3.5} />
    {/each}
    {#if playX >= 0 && playX <= width}<line class="play" x1={playX} x2={playX} y1="0" y2={height} />{/if}
  </svg>
  {#if tip}
    <span class="tip" style:left="{Math.min(width - 70, tip.x + 10)}px" style:top="{Math.max(0, tip.y - 18)}px">{tip.text}</span>
  {/if}
</div>

<style>
  .auto-lane {
    position: relative;
    box-sizing: border-box;
    border-bottom: 3px solid var(--track-gap);
    background: color-mix(in srgb, var(--surface-1) 88%, var(--c) 12%);
    cursor: crosshair;
    overflow: hidden;
  }

  .auto-lane.off {
    opacity: 0.55;
  }

  svg {
    display: block;
  }

  .guide {
    stroke: var(--border);
    stroke-dasharray: 3 4;
  }

  .fill {
    fill: color-mix(in srgb, var(--c) 14%, transparent);
  }

  .line {
    fill: none;
    stroke: var(--c);
    stroke-width: 1.6;
  }

  .pt {
    fill: var(--surface-0);
    stroke: var(--c);
    stroke-width: 1.6;
  }

  .pt.hot {
    fill: var(--c);
  }

  .play {
    stroke: var(--playhead, var(--accent));
    stroke-width: 1;
  }

  .tip {
    position: absolute;
    padding: 1px 5px;
    border-radius: 3px;
    background: var(--surface-3);
    color: var(--text);
    font-size: var(--font-size-small);
    white-space: nowrap;
    pointer-events: none;
  }
</style>
