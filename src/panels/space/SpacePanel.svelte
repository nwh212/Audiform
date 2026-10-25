<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Altered Audio Editor.

  The 3D Space panel: the room seen from above, front at the top, with
  the layout's speakers and a dot for each track and bus that feeds the
  master (a pair of dots for stereo ones). Drag a dot to move it; the
  wheel over a dot raises or lowers it. The picked one's height, size and
  width are beside the room. The bar on top picks the speaker layout and
  how the session is heard (stereo speakers or headphones). In a session
  that is not a 3D Space one, it offers to make it one.
-->
<script lang="ts">
  import type { CommandRunner } from "../../stores/media-pool";
  import { editor, type EditorState } from "../../stores/editor";
  import { SPACE_LAYOUTS, type SessionDoc, type SessionStore, type SpaceDoc, type SpaceLayout, type SpacePosDoc } from "../../stores/session";
  import { defaultTrackColor } from "../editor/timeline";
  import { fromView, LAYOUT_NAMES, sides, speakerSpots, tidyPos, toView } from "./space";

  interface Props {
    session: SessionStore;
    runner: CommandRunner;
    onError?: (e: unknown) => void;
  }

  const { session, runner, onError = console.error }: Props = $props();

  let doc = $state<SessionDoc | null>(null);
  $effect(() =>
    session.subscribe((d) => {
      doc = d;
    }),
  );
  let ed = $state<EditorState>(editor.current());
  $effect(() =>
    editor.subscribe((s) => {
      ed = s;
    }),
  );

  const space = $derived(doc?.space ?? null);
  let newLayout = $state<SpaceLayout>("7.1.4");

  interface Source {
    id: string;
    kind: "track" | "bus";
    name: string;
    color: string;
    stereo: boolean;
    pos: SpacePosDoc;
  }
  const cssColor = (c: string) => (c.startsWith("--") ? `var(${c})` : c);
  /** While dragging, the spot being dragged, shown at once (the edit follows). */
  let live = $state<{ id: string; pos: SpacePosDoc } | null>(null);
  const sources = $derived.by((): Source[] => {
    if (!doc) return [];
    const out: Source[] = [];
    doc.tracks.forEach((t, i) => {
      if (t.output && t.output.type !== "master") return;
      out.push({
        id: t.id,
        kind: "track",
        name: t.name,
        color: cssColor(t.color ?? defaultTrackColor(i)),
        stereo: t.channels !== "mono" && !t.mono,
        pos: live?.id === t.id ? live.pos : tidyPos(t.space),
      });
    });
    for (const b of doc.buses ?? []) {
      if (b.output && b.output.type !== "master") continue;
      out.push({ id: b.id, kind: "bus", name: b.name, color: b.color ? cssColor(b.color) : "var(--clip-3)", stereo: b.channels !== "mono" && !b.mono, pos: live?.id === b.id ? live.pos : tidyPos(b.space) });
    }
    return out;
  });
  const picked = $derived(sources.find((s) => s.id === ed.selectedTrack) ?? null);
  const spots = $derived(space ? speakerSpots(space.layout) : []);
  const heights = $derived(spots.some((s) => s.z > 0));

  let panelW = $state(600);
  let boxW = $state(300);
  let boxH = $state(300);
  // Narrow: controls go under the room, which takes the panel's width.
  const size = $derived(Math.max(160, panelW < 460 ? boxW : Math.min(boxW, boxH)));

  function setSpace(next: SpaceDoc | null, label: string) {
    void session.edit(label, { op: "set_space", space: next }, runner).catch(onError);
  }

  // Edits while dragging go one at a time; the latest waits for the one in flight.
  let inFlight = false;
  let queued: { s: Source; pos: SpacePosDoc } | null = null;
  function sendPos(s: Source, pos: SpacePosDoc) {
    if (inFlight) {
      queued = { s, pos };
      return;
    }
    inFlight = true;
    const op = s.kind === "track" ? ({ op: "set_track", id: s.id, changes: { space: pos } } as const) : ({ op: "set_bus", id: s.id, changes: { space: pos } } as const);
    void session
      .edit(s.kind === "track" ? "Move Track in Space" : "Move Bus in Space", op, runner)
      .catch(onError)
      .finally(() => {
        inFlight = false;
        const q = queued;
        queued = null;
        if (q) sendPos(q.s, q.pos);
      });
  }

  function setPos(s: Source, change: Partial<SpacePosDoc>) {
    sendPos(s, tidyPos({ ...s.pos, ...change }));
  }

  let room = $state<SVGSVGElement | null>(null);
  function onDotDown(e: PointerEvent, s: Source, side: -1 | 0 | 1) {
    if (e.button !== 0 || !room) return;
    e.preventDefault();
    e.stopPropagation();
    editor.selectTrack(s.id);
    const el = e.currentTarget as SVGGElement;
    el.setPointerCapture(e.pointerId);
    const r = room.getBoundingClientRect();
    const start = s.pos;
    // Grabbing one side of a stereo pair moves the pair, keeping the grab point under the pointer.
    const offset = side === 0 ? 0 : side * start.width;
    const move = (ev: PointerEvent) => {
      const [x, y] = fromView(ev.clientX - r.left, ev.clientY - r.top, size);
      const pos = tidyPos({ ...start, x: x - offset, y });
      live = { id: s.id, pos };
      sendPos(s, pos);
    };
    const up = () => {
      el.removeEventListener("pointermove", move);
      el.removeEventListener("pointerup", up);
      el.removeEventListener("pointercancel", up);
      live = null;
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up);
    el.addEventListener("pointercancel", up);
  }

  function onDotWheel(e: WheelEvent, s: Source) {
    e.preventDefault();
    editor.selectTrack(s.id);
    setPos(s, { z: s.pos.z + (e.deltaY < 0 ? 0.05 : -0.05) });
  }

  const corners = $derived([...toView(-1, 1, size), ...toView(1, -1, size)]);
  const x0 = $derived(corners[0]!);
  const y0 = $derived(corners[1]!);
  const x1 = $derived(corners[2]!);
  const y1 = $derived(corners[3]!);
  const dotR = (p: SpacePosDoc) => 7 + p.z * 5 + p.size * 10;
  const pct = (v: number) => `${Math.round(v * 100)}%`;
</script>

<section class="space-panel" bind:clientWidth={panelW} class:narrow={panelW < 460}>
  {#if !doc}
    <p class="hint">Open a session to place its tracks in a room.</p>
  {:else if !space}
    <div class="intro">
      <p class="lead">This is a stereo session.</p>
      <p class="hint">A 3D Space session places each track at a spot in a room instead of panning it. You hear it on stereo speakers or headphones while you work, and export it to any speaker layout: 5.1, 7.1, 7.1.4 and more, or stereo and headphone mixes, from the same session.</p>
      <label class="row">Speaker layout
        <select bind:value={newLayout}>
          {#each SPACE_LAYOUTS as l (l)}<option value={l}>{LAYOUT_NAMES[l]}</option>{/each}
        </select>
      </label>
      <button type="button" class="primary" onclick={() => setSpace({ layout: newLayout, listen: "stereo" }, "Make 3D Space Session")}>Make This a 3D Space Session</button>
    </div>
  {:else}
    <div class="bar">
      <label title="The speakers the session plays and exports in. Spots are kept in the room, so changing this never moves anything; any export can pick another layout too.">Layout
        <select value={space.layout} onchange={(e) => setSpace({ ...space, layout: e.currentTarget.value as SpaceLayout }, "Speaker Layout")}>
          {#each SPACE_LAYOUTS as l (l)}<option value={l}>{LAYOUT_NAMES[l]}</option>{/each}
        </select>
      </label>
      <label title="How you hear it while working. Stereo folds each sound to your speakers by where it sits left to right. Headphones plays the layout's speakers as if they stood around your head (binaural).">Listen
        <select value={space.listen ?? "stereo"} onchange={(e) => setSpace({ ...space, listen: e.currentTarget.value === "binaural" ? "binaural" : "stereo" }, "Listen To 3D Space")}>
          <option value="stereo">Stereo speakers</option>
          <option value="binaural">Headphones (binaural)</option>
        </select>
      </label>
      <span class="spacer"></span>
      <button type="button" class="quiet" title="Make it an ordinary stereo session again. Spots are kept, so turning it back on puts everything where it was." onclick={() => setSpace(null, "Make Stereo Session")}>Stereo Session</button>
    </div>
    <div class="body">
      <div class="roombox" bind:clientWidth={boxW} bind:clientHeight={boxH}>
        <svg bind:this={room} width={size} height={size} class="room" role="application" aria-label="Room, seen from above. Front is at the top.">
          <rect x={x0} y={y0} width={x1 - x0} height={y1 - y0} class="walls" />
          <line x1={(x0 + x1) / 2} x2={(x0 + x1) / 2} y1={y0} y2={y1} class="axis" />
          <line x1={x0} x2={x1} y1={(y0 + y1) / 2} y2={(y0 + y1) / 2} class="axis" />
          <text x={(x0 + x1) / 2} y={y0 - 20} class="front">FRONT</text>
          {#each spots as sp (sp.name)}
            {@const [sx, sy] = toView(sp.x, sp.y, size)}
            {#if sp.z > 0}
              <g class="spk top" transform="translate({sx + (sp.x < 0 ? 14 : -14)} {sy + (sp.y > 0 ? 14 : sp.y < 0 ? -14 : 0)})">
                <rect x="-6" y="-6" width="12" height="12" transform="rotate(45)" /><text y="20">{sp.name}</text>
              </g>
            {:else}
              <g class="spk" transform="translate({sx} {sy})">
                <rect x="-7" y="-7" width="14" height="14" rx="2" /><text y={sp.y > 0 ? -11 : 20}>{sp.name}</text>
              </g>
            {/if}
          {/each}
          {#each sources as s (s.id)}
            {@const r = dotR(s.pos)}
            {@const on = ed.selectedTrack === s.id}
            {@const [nx, ny] = toView(s.pos.x, s.pos.y, size)}
            {#if s.stereo}
              {@const [lx, rx] = sides(s.pos)}
              {@const [ax, ay] = toView(lx, s.pos.y, size)}
              {@const [bx, by] = toView(rx, s.pos.y, size)}
              <line x1={ax} y1={ay} x2={bx} y2={by} class="pair" class:on style:stroke={s.color} />
              {#each [[ax, ay, -1, "L"], [bx, by, 1, "R"]] as [px, py, side, lab] (lab)}
                <!-- svelte-ignore a11y_no_static_element_interactions -->
                <g class="dot" class:on transform="translate({px} {py})" onpointerdown={(e) => onDotDown(e, s, side as -1 | 1)} onwheel={(e) => onDotWheel(e, s)}>
                  <title>{s.name} ({lab}): drag to move, wheel to raise or lower</title>
                  {#if s.pos.z > 0.02}<circle r={r + 4} class="lift" style:stroke={s.color} />{/if}
                  <circle r={r * 0.8} style:fill={s.color} />
                  <text dy="3.5">{lab}</text>
                </g>
              {/each}
            {:else}
              {@const [px, py] = toView(s.pos.x, s.pos.y, size)}
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <g class="dot" class:on transform="translate({px} {py})" onpointerdown={(e) => onDotDown(e, s, 0)} onwheel={(e) => onDotWheel(e, s)}>
                <title>{s.name}: drag to move, wheel to raise or lower</title>
                {#if s.pos.z > 0.02}<circle r={r + 4} class="lift" style:stroke={s.color} />{/if}
                <circle r={r} style:fill={s.color} />
              </g>
            {/if}
            <text x={nx} y={ny + r + 13} class="label" class:on>{s.name}</text>
          {/each}
        </svg>
      </div>
      <div class="side">
        {#if picked}
          <div class="name" style:border-color={picked.color}>{picked.name}</div>
          <label class="ctl" title={heights ? "How high it sits: floor to ceiling" : "This layout has no height speakers; height is kept for layouts that do"}>
            <span>Height</span>
            <input type="range" min="0" max="1" step="0.01" value={picked.pos.z} oninput={(e) => setPos(picked, { z: Number(e.currentTarget.value) })} />
            <span class="val">{pct(picked.pos.z)}</span>
          </label>
          <label class="ctl" title="0 is a point; turn it up to spread the sound over more of the room">
            <span>Size</span>
            <input type="range" min="0" max="1" step="0.01" value={picked.pos.size} oninput={(e) => setPos(picked, { size: Number(e.currentTarget.value) })} />
            <span class="val">{pct(picked.pos.size)}</span>
          </label>
          {#if picked.stereo}
            <label class="ctl" title="How far its left and right sides sit from its spot">
              <span>Width</span>
              <input type="range" min="0" max="1" step="0.01" value={picked.pos.width} oninput={(e) => setPos(picked, { width: Number(e.currentTarget.value) })} />
              <span class="val">{pct(picked.pos.width)}</span>
            </label>
          {/if}
          <label class="ctl">
            <span>Left / right</span>
            <input type="range" min="-1" max="1" step="0.01" value={picked.pos.x} oninput={(e) => setPos(picked, { x: Number(e.currentTarget.value) })} />
            <span class="val">{picked.pos.x.toFixed(2)}</span>
          </label>
          <label class="ctl">
            <span>Back / front</span>
            <input type="range" min="-1" max="1" step="0.01" value={picked.pos.y} oninput={(e) => setPos(picked, { y: Number(e.currentTarget.value) })} />
            <span class="val">{picked.pos.y.toFixed(2)}</span>
          </label>
          <button type="button" class="quiet" onclick={() => setPos(picked, { x: 0, y: 1, z: 0, size: 0, width: 1 })}>Back to Front Center</button>
        {:else}
          <p class="hint">Click a dot (or a track) to set its height, size and width. Tracks routed to a bus play where the bus sits.</p>
        {/if}
      </div>
    </div>
  {/if}
</section>

<style>
  .space-panel {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    background: var(--surface-1);
    color: var(--text);
    font-size: var(--font-size-small);
  }
  .intro {
    display: grid;
    gap: 10px;
    max-width: 420px;
    padding: 16px;
  }
  .lead {
    margin: 0;
    font-weight: 600;
  }
  .hint {
    margin: 0;
    padding: 8px;
    color: var(--text-dim);
  }
  .row,
  .bar label {
    display: flex;
    gap: 6px;
    align-items: center;
    color: var(--text-dim);
  }
  .bar {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    align-items: center;
    padding: 4px 8px;
    border-bottom: 1px solid var(--border);
  }
  .spacer {
    flex: 1;
  }
  .primary {
    justify-self: start;
    border-color: var(--accent);
  }
  .quiet {
    opacity: 0.85;
  }
  .body {
    display: flex;
    flex: 1;
    min-height: 0;
  }
  .roombox {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: grid;
    place-items: center;
    overflow: hidden;
  }
  .room {
    touch-action: none;
    user-select: none;
  }
  .walls {
    fill: color-mix(in srgb, var(--text) 3%, var(--surface-0));
    stroke: var(--border-strong);
    stroke-width: 1.5;
  }
  .axis {
    stroke: color-mix(in srgb, var(--text) 8%, transparent);
    stroke-dasharray: 3 4;
  }
  .front {
    fill: var(--text-faint);
    font-size: 9px;
    letter-spacing: 0.15em;
    text-anchor: middle;
  }
  .spk rect {
    fill: var(--surface-3);
    stroke: color-mix(in srgb, var(--text) 45%, transparent);
  }
  .spk.top rect {
    fill: none;
    stroke-dasharray: 2 2;
  }
  .spk text {
    fill: var(--text-faint);
    font-size: 9px;
    text-anchor: middle;
  }
  .dot {
    cursor: grab;
  }
  .dot circle:not(.lift) {
    stroke: #0e0e10;
    stroke-width: 1;
  }
  .dot.on circle:not(.lift) {
    stroke: var(--text);
    stroke-width: 2;
  }
  .dot .lift {
    fill: none;
    stroke-width: 1;
    stroke-dasharray: 2 2;
  }
  .dot text {
    fill: #0e0e10;
    font-size: 9px;
    font-weight: 700;
    text-anchor: middle;
    pointer-events: none;
  }
  .pair {
    stroke-width: 1.5;
    opacity: 0.5;
  }
  .pair.on {
    opacity: 0.9;
  }
  .label {
    fill: var(--text-dim);
    font-size: 10px;
    text-anchor: middle;
    pointer-events: none;
  }
  .label.on {
    fill: var(--text);
  }
  .narrow .body {
    flex-direction: column;
    overflow-y: auto;
  }
  .narrow .roombox {
    flex: none;
  }
  .narrow .side {
    width: auto;
    border-left: 0;
    border-top: 1px solid var(--border);
    overflow: visible;
  }
  .side {
    display: flex;
    flex-direction: column;
    gap: 8px;
    width: 190px;
    padding: 8px;
    border-left: 1px solid var(--border);
    overflow-y: auto;
  }
  .name {
    padding-left: 6px;
    border-left: 3px solid;
    font-weight: 600;
  }
  .ctl {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 2px 6px;
    color: var(--text-dim);
  }
  .ctl input {
    grid-column: 1 / -1;
    grid-row: 2;
  }
  .val {
    color: var(--value);
    font-family: var(--font-mono);
  }
</style>
