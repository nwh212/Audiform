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
  import { fromView, LAYOUT_NAMES, rotationToward, sides, speakerSpots, tidyPos, toView } from "./space";

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
    // Alt and a side of a pair: turn the pair around its spot.
    const turning = e.altKey && side !== 0;
    // Grabbing one side of a pair moves the pair, keeping the grab point under the pointer.
    const [sl, sr] = sides(start);
    const grab: [number, number] = side === -1 ? sl : side === 1 ? sr : [start.x, start.y];
    const off: [number, number] = [grab[0] - start.x, grab[1] - start.y];
    const move = (ev: PointerEvent) => {
      const [x, y] = fromView(ev.clientX - r.left, ev.clientY - r.top, size);
      let pos: SpacePosDoc;
      if (turning) {
        const toward = rotationToward(start, x, y);
        pos = tidyPos({ ...start, rotation: side === 1 ? toward : ((toward + 360) % 360) - 180 });
      } else {
        pos = tidyPos({ ...start, x: x - off[0], y: y - off[1] });
      }
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

  /** The wheel over a dot raises or lowers it; with Shift it turns a pair. */
  function onDotWheel(e: WheelEvent, s: Source) {
    e.preventDefault();
    editor.selectTrack(s.id);
    const up = e.deltaY < 0;
    if (e.shiftKey && s.stereo) {
      let rot = (s.pos.rotation ?? 0) + (up ? 5 : -5);
      if (rot > 180) rot -= 360;
      if (rot < -180) rot += 360;
      setPos(s, { rotation: rot });
    } else setPos(s, { z: s.pos.z + (up ? 0.05 : -0.05) });
  }

  const corners = $derived([...toView(-1, 1, size), ...toView(1, -1, size)]);
  const x0 = $derived(corners[0]!);
  const y0 = $derived(corners[1]!);
  const x1 = $derived(corners[2]!);
  const y1 = $derived(corners[3]!);
  // Dots grow with the room, a little with height, and with size.
  const unit = $derived(Math.max(6, size * 0.014));
  const dotR = (p: SpacePosDoc) => unit * (1 + Math.max(0, p.z) * 0.5 + p.size * 1.2);
  const deg = (v: number) => `${Math.round(v)}°`;
  /** A speaker's name, placed outside the room so dots and names never cover it. */
  function spkLabel(sp: { x: number; y: number }, sx: number, sy: number): { x: number; y: number; anchor: string } {
    if (sp.y >= 0.99) return { x: sx, y: sy - 12, anchor: "middle" };
    if (sp.y <= -0.99) return { x: sx, y: sy + 20, anchor: "middle" };
    return { x: sx + (sp.x < 0 ? -12 : 12), y: sy + 4, anchor: sp.x < 0 ? "end" : "start" };
  }
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
          <defs>
            <radialGradient id="floor" cx="50%" cy="50%" r="60%">
              <stop offset="0%" stop-color="var(--surface-2)" />
              <stop offset="100%" stop-color="var(--surface-0)" />
            </radialGradient>
          </defs>
          <rect x={x0} y={y0} width={x1 - x0} height={y1 - y0} rx="6" class="walls" />
          {#each [0.33, 0.66, 1] as k (k)}
            <circle cx={(x0 + x1) / 2} cy={(y0 + y1) / 2} r={((x1 - x0) / 2) * k} class="ring" />
          {/each}
          <line x1={(x0 + x1) / 2} x2={(x0 + x1) / 2} y1={y0} y2={y1} class="axis" />
          <line x1={x0} x2={x1} y1={(y0 + y1) / 2} y2={(y0 + y1) / 2} class="axis" />
          <text x={(x0 + x1) / 2} y={y0 - 26} class="front">FRONT</text>
          <!-- The listener, facing front. -->
          <g class="head" transform="translate({(x0 + x1) / 2} {(y0 + y1) / 2})">
            <path d="M -5 -{unit * 1.2} L 0 -{unit * 1.9} L 5 -{unit * 1.2} Z" />
            <circle r={unit * 1.3} />
            <ellipse cx={-unit * 1.35} rx={unit * 0.3} ry={unit * 0.55} />
            <ellipse cx={unit * 1.35} rx={unit * 0.3} ry={unit * 0.55} />
          </g>
          {#each spots as sp (sp.name)}
            {@const [sx, sy] = toView(sp.x, sp.y, size)}
            {#if sp.z > 0}
              {@const tx = sx + (sp.x < 0 ? unit * 2.4 : -unit * 2.4)}
              {@const ty = sy + (sp.y > 0 ? unit * 2.4 : sp.y < 0 ? -unit * 2.4 : 0)}
              <g class="spk top" transform="translate({tx} {ty})">
                <rect x={-unit * 0.8} y={-unit * 0.8} width={unit * 1.6} height={unit * 1.6} transform="rotate(45)" />
              </g>
              <text x={tx} y={ty + (sp.y > 0 ? unit * 2.6 : -unit * 1.8)} class="spk-name top">{sp.name}</text>
            {:else}
              {@const lab = spkLabel(sp, sx, sy)}
              <g class="spk" transform="translate({sx} {sy})">
                <rect x={-unit} y={-unit} width={unit * 2} height={unit * 2} rx="3" />
                <circle r={unit * 0.45} class="cone" />
              </g>
              <text x={lab.x} y={lab.y} text-anchor={lab.anchor} class="spk-name">{sp.name}</text>
            {/if}
          {/each}
          {#each sources as s (s.id)}
            {@const r = dotR(s.pos)}
            {@const on = ed.selectedTrack === s.id}
            {@const [nx, ny] = toView(s.pos.x, s.pos.y, size)}
            {@const together = s.stereo && s.pos.width < 0.02}
            {#if s.stereo && !together}
              {@const [l, rr] = sides(s.pos)}
              {@const [ax, ay] = toView(l[0], l[1], size)}
              {@const [bx, by] = toView(rr[0], rr[1], size)}
              <line x1={ax} y1={ay} x2={bx} y2={by} class="pair" class:on style:stroke={s.color} />
              {#each [[ax, ay, -1, "L"], [bx, by, 1, "R"]] as [px, py, side, lab] (lab)}
                <!-- svelte-ignore a11y_no_static_element_interactions -->
                <g class="dot" class:on transform="translate({px} {py})" onpointerdown={(e) => onDotDown(e, s, side as -1 | 1)} onwheel={(e) => onDotWheel(e, s)}>
                  <title>{s.name} ({lab}): drag to move, Alt and drag to turn the pair, the wheel to raise or lower (Shift and the wheel to turn)</title>
                  {#if s.pos.z > 0.02}<circle r={r * 0.8 + 4} class="lift" style:stroke={s.color} />{/if}
                  <circle r={r * 0.8} class="body" style:fill={s.color} />
                  <text dy={unit * 0.35} style:font-size="{Math.max(8, unit * 0.95)}px">{lab}</text>
                </g>
              {/each}
            {:else}
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <g class="dot" class:on transform="translate({nx} {ny})" onpointerdown={(e) => onDotDown(e, s, 0)} onwheel={(e) => onDotWheel(e, s)}>
                <title>{s.name}: drag to move, the wheel to raise or lower</title>
                {#if s.pos.z > 0.02}<circle r={r + 4} class="lift" style:stroke={s.color} />{/if}
                <circle r={r} class="body" style:fill={s.color} />
                {#if together}<text dy={unit * 0.35} style:font-size="{Math.max(7, unit * 0.8)}px">LR</text>{/if}
              </g>
            {/if}
            {@const tag = `${s.name}${Math.abs(s.pos.z) > 0.02 ? (s.pos.z > 0 ? ` ↑${Math.round(s.pos.z * 100)}%` : ` ↓${Math.round(-s.pos.z * 100)}%`) : ""}${(s.pos.lfe ?? 0) > 0 ? " · LFE" : ""}`}
            <g class="tag" class:on transform="translate({nx} {ny + r + unit * 1.6})">
              <rect x={-(tag.length * 3.1 + 6)} y="-9" width={tag.length * 6.2 + 12} height="14" rx="7" />
              <text dy="2">{tag}</text>
            </g>
          {/each}
        </svg>
      </div>
      <div class="side">
        {#if picked}
          <div class="name" style:border-color={picked.color}>{picked.name}</div>
          <label class="ctl" title={heights ? "How high it sits: ceiling (100%) to ear level (0) and below. Speaker layouts have no speakers below ear level, so below 0 plays from the ear-level ones." : "This layout has no height speakers; height is kept for layouts that do"}>
            <span>Height</span>
            <input type="range" min="-1" max="1" step="0.01" value={picked.pos.z} oninput={(e) => setPos(picked, { z: Number(e.currentTarget.value) })} />
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
            <label class="ctl" title="Turns the pair around its spot, clockwise seen from above. Also Alt and drag a side, or Shift and the wheel over it.">
              <span>Rotation</span>
              <input type="range" min="-180" max="180" step="1" value={picked.pos.rotation ?? 0} oninput={(e) => setPos(picked, { rotation: Number(e.currentTarget.value) })} ondblclick={() => setPos(picked, { rotation: 0 })} />
              <span class="val">{deg(picked.pos.rotation ?? 0)}</span>
            </label>
          {/if}
          <label class="ctl" title="Also sends it to the low-frequency (subwoofer) channel, low-passed at 120 Hz. For kick, bass, booms. Layouts without that channel (2.0) leave it out.">
            <span>LFE send</span>
            <input type="range" min="0" max="1" step="0.01" value={picked.pos.lfe ?? 0} oninput={(e) => setPos(picked, { lfe: Number(e.currentTarget.value) })} />
            <span class="val">{(picked.pos.lfe ?? 0) > 0 ? `${(20 * Math.log10(picked.pos.lfe ?? 0)).toFixed(1)} dB` : "Off"}</span>
          </label>
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
          <button type="button" class="quiet" onclick={() => setPos(picked, { x: 0, y: 1, z: 0, size: 0, width: 1, rotation: 0 })}>Back to Front Center</button>
        {:else}
          <p class="hint">Click a dot (or a track) to set its height, size, width, rotation and LFE send. Drag dots to move them; the wheel raises or lowers one; Alt and drag (or Shift and the wheel) turns a stereo pair. Tracks routed to a bus play where the bus sits.</p>
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
    fill: url(#floor);
    stroke: var(--border-strong);
    stroke-width: 1.5;
  }
  .ring {
    fill: none;
    stroke: color-mix(in srgb, var(--text) 7%, transparent);
  }
  .head circle,
  .head ellipse,
  .head path {
    fill: color-mix(in srgb, var(--text) 18%, var(--surface-2));
    stroke: color-mix(in srgb, var(--text) 35%, transparent);
  }
  .spk .cone {
    fill: color-mix(in srgb, var(--text) 25%, var(--surface-2));
    stroke: none;
  }
  .spk-name {
    fill: var(--text-faint);
    font-size: 10px;
    text-anchor: middle;
  }
  .spk-name.top {
    font-style: italic;
  }
  .tag rect {
    fill: color-mix(in srgb, var(--surface-0) 80%, transparent);
    stroke: color-mix(in srgb, var(--text) 12%, transparent);
  }
  .tag text {
    fill: var(--text-dim);
    font-size: 10px;
    text-anchor: middle;
    pointer-events: none;
  }
  .tag.on text {
    fill: var(--text);
    font-weight: 600;
  }
  .tag {
    pointer-events: none;
  }
  .dot.on .body {
    filter: drop-shadow(0 0 4px currentColor);
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
