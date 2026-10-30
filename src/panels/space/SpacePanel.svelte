<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  The 3D Space panel: the room seen from above, front at the top, with
  the layout's speakers and a dot for each track and bus that feeds the
  master (a pair of dots for stereo ones). Drag a dot to move it; the
  wheel over a dot raises or lowers it. A stereo source has a center node
  that moves the pair, side dots that set its width as they are dragged
  away from or toward the center, and turn arrows (when picked) that
  rotate it. Snapping (on the bar) pulls spots onto speakers, the center
  lines and a quarter-room grid, and turns onto 15 degree steps; Alt
  places freely. The picked one's settings are beside the room. The bar on top picks the speaker layout and
  how the session is heard (stereo speakers or headphones). In a session
  that is not a 3D Space one, it offers to make it one.
-->
<script lang="ts">
  import type { CommandRunner } from "../../stores/media-pool";
  import { editor, type EditorState } from "../../stores/editor";
  import { SPACE_LAYOUTS, type EnvelopeDoc, type SessionDoc, type SessionStore, type SpaceDoc, type SpaceLayout, type SpacePosDoc, type SpatializeMode } from "../../stores/session";
  import { defaultTrackColor } from "../editor/timeline";
  import { transport } from "../../stores/open-session";
  import type { TransportState } from "../../stores/transport";
  import { angleOf, automatedPos, fromView, lastLayout, LAYOUT_NAMES, sides, spaceToggle, snapAngle, snapSpot, SPATIALIZE_MODES, speakerSpots, tidyPos, toView, wrapDeg } from "./space";
  import { settings } from "../../stores/settings";
  import SpaceFader from "./SpaceFader.svelte";

  interface Props {
    session: SessionStore;
    runner: CommandRunner;
    onError?: (e: unknown) => void;
    /** Opens Export Mixdown, where the speaker layout for the file is picked. */
    onExport?: () => void;
  }

  const { session, runner, onError = console.error, onExport }: Props = $props();

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

  // While playing, spots follow their automation at the playhead.
  let tp = $state<TransportState | null>(null);
  $effect(() =>
    transport.subscribe((s) => {
      tp = s;
    }),
  );
  const at = $derived(tp?.playing ? tp.position : (doc?.view.playhead ?? 0));

  const space = $derived(doc?.space ?? null);
  let snapOn = $state(settings.current.spaceSnap);
  $effect(() =>
    settings.subscribe((v) => {
      snapOn = v.spaceSnap;
    }),
  );
  function toggleSnap() {
    settings.set({ ...settings.current, spaceSnap: !snapOn });
  }
  let newLayout = $state<SpaceLayout>("7.1.4");
  // Coming back to a session that had 3D Space: offer the layout it had.
  $effect(() => {
    const id = doc?.id;
    const was = id ? lastLayout(id) : null;
    if (was && !doc?.space) newLayout = was;
  });

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
      // A track with nothing on it has nothing to place.
      if (t.clips.length === 0 && !(t.midi_clips?.length ?? 0)) return;
      out.push({
        id: t.id,
        kind: "track",
        name: t.name,
        color: cssColor(t.color ?? defaultTrackColor(i)),
        stereo: t.channels !== "mono" && !t.mono,
        pos: live?.id === t.id ? live.pos : automatedPos(tidyPos(t.space), t.envelopes, at),
      });
    });
    for (const b of doc.buses ?? []) {
      if (b.output && b.output.type !== "master") continue;
      out.push({ id: b.id, kind: "bus", name: b.name, color: b.color ? cssColor(b.color) : "var(--clip-3)", stereo: b.channels !== "mono" && !b.mono, pos: live?.id === b.id ? live.pos : automatedPos(tidyPos(b.space), b.envelopes as EnvelopeDoc[] | undefined, at) });
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
  /**
   * Dragging in the room. "move": the spot (a mono dot, or a stereo pair's
   * center node), keeping the grab point under the pointer. "left" and
   * "right": a pair's side, whose distance from the center sets the width
   * (Alt also turns the pair toward the pointer). "turn": a turn arrow,
   * rotating the pair around its center as the pointer goes around it.
   */
  function onDotDown(e: PointerEvent, s: Source, mode: "move" | "left" | "right" | "turn") {
    if (e.button !== 0 || !room) return;
    e.preventDefault();
    e.stopPropagation();
    editor.selectTrack(s.id);
    const el = e.currentTarget as SVGGElement;
    el.setPointerCapture(e.pointerId);
    const r = room.getBoundingClientRect();
    const start = s.pos;
    const [gx, gy] = fromView(e.clientX - r.left, e.clientY - r.top, size);
    const off: [number, number] = [gx - start.x, gy - start.y];
    const a0 = angleOf(start.x, start.y, gx, gy);
    const move = (ev: PointerEvent) => {
      const free = ev.altKey || !snapOn;
      const [x, y] = fromView(ev.clientX - r.left, ev.clientY - r.top, size);
      let pos: SpacePosDoc;
      if (mode === "move") {
        let [nx, ny] = [x - off[0], y - off[1]];
        if (!free) [nx, ny] = snapSpot(nx, ny, spots);
        pos = tidyPos({ ...start, x: nx, y: ny });
      } else if (mode === "turn") {
        let rot = wrapDeg((start.rotation ?? 0) + (angleOf(start.x, start.y, x, y) - a0));
        if (!free) rot = snapAngle(rot);
        pos = tidyPos({ ...start, rotation: rot });
      } else {
        let w = Math.min(1, Math.hypot(x - start.x, y - start.y));
        if (!free) {
          const g = Math.round(w * 4) / 4;
          if (Math.abs(g - w) <= 0.04) w = g;
        }
        const change: Partial<SpacePosDoc> = { width: w };
        if (ev.altKey) {
          const toward = angleOf(start.x, start.y, x, y);
          change.rotation = Math.round(mode === "right" ? toward : wrapDeg(toward + 180));
        }
        pos = tidyPos({ ...start, ...change });
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

  /**
   * Side dots in view terms, kept at least a dot's reach from the center
   * so a narrow (or zero width) pair can still be grabbed and widened.
   */
  function apart(cx: number, cy: number, ax: number, ay: number, bx: number, by: number, rot: number): [number, number, number, number] {
    const min = unit * 2.2;
    if (Math.hypot(bx - cx, by - cy) >= min) return [ax, ay, bx, by];
    const a = (rot * Math.PI) / 180;
    const dx = Math.cos(a) * min;
    const dy = Math.sin(a) * min;
    return [cx - dx, cy - dy, cx + dx, cy + dy];
  }

  /** A turn arrow: an arc around a pair's center, across the pair's axis, with heads at both ends. */
  function turnArrow(cx: number, cy: number, rot: number, flip: boolean, rr: number): { arc: string; heads: string } {
    const a = (rot * Math.PI) / 180;
    // The axis across the pair, in view terms (the room's front is up).
    const t0 = Math.atan2(-Math.cos(a), Math.sin(a)) + (flip ? Math.PI : 0);
    const span = 0.55;
    const p = (t: number) => [cx + rr * Math.cos(t), cy + rr * Math.sin(t)] as const;
    const [x1, y1] = p(t0 - span);
    const [x2, y2] = p(t0 + span);
    const head = (t: number, dir: 1 | -1) => {
      const [hx, hy] = p(t);
      const tx = -Math.sin(t) * dir;
      const ty = Math.cos(t) * dir;
      const nx = Math.cos(t);
      const ny = Math.sin(t);
      const k = 5;
      return `M${hx + tx * k} ${hy + ty * k} L${hx + nx * k * 0.8} ${hy + ny * k * 0.8} L${hx - nx * k * 0.8} ${hy - ny * k * 0.8} Z`;
    };
    return { arc: `M${x1} ${y1} A${rr} ${rr} 0 0 1 ${x2} ${y2}`, heads: `${head(t0 + span, 1)} ${head(t0 - span, -1)}` };
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
  {#if ed.mode === "waveform"}
    <p class="hint">3D Space is where you set up surround and height sound for a session. Switch to the Multitrack editor to use it.</p>
  {:else if !doc}
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
      <button type="button" class="primary" onclick={() => { if (doc) { const t = spaceToggle(doc, newLayout); setSpace(t.space, t.label); } }}>Turn On 3D Space</button>
      <p class="hint">Turning it off again later makes the session stereo, with every track's pan as you left it; the spots in the room are kept too. So one session can hold a stereo mix and a surround mix, and you switch between them here or with Multitrack &gt; 3D Space Session.</p>
    </div>
  {:else}
    <div class="bar">
      <button type="button" class="toggle on" aria-pressed="true" title="3D Space is on. Click to make this a stereo session again: tracks use their pan, and their spots in the room are kept for when you turn it back on. Undo also turns it back." onclick={() => { if (doc) { const t = spaceToggle(doc); setSpace(t.space, t.label); } }}>
        <svg viewBox="0 0 16 16" aria-hidden="true"><circle cx="8" cy="8" r="5.5" fill="none" stroke="currentColor" stroke-width="1.5" /><path d="M8 1.5v4" stroke="currentColor" stroke-width="1.6" /></svg>
        3D Space
      </button>
      <label title="The speakers the session plays and exports in. Spots are kept in the room, so changing this never moves anything; any export can pick another layout too.">Layout
        <select value={space.layout} onchange={(e) => setSpace({ ...space, layout: e.currentTarget.value as SpaceLayout }, "Speaker Layout")}>
          {#each SPACE_LAYOUTS as l (l)}<option value={l}>{LAYOUT_NAMES[l]}</option>{/each}
        </select>
      </label>
      <label title="How you hear it while working. Stereo folds each sound to your speakers by where it sits left to right. Headphones plays the layout's speakers as if they stood around your head (binaural). Speakers plays each speaker on its own output of your audio device, for a room with these speakers (Settings > Channel Routing picks the outputs).">Listen
        <select value={space.listen ?? "stereo"} onchange={(e) => { const v = e.currentTarget.value; setSpace({ ...space, listen: v === "binaural" || v === "speakers" ? v : "stereo" }, "Listen To 3D Space"); }}>
          <option value="stereo">Stereo speakers</option>
          <option value="binaural">Headphones (binaural)</option>
          <option value="speakers">Speakers (each on its own output)</option>
        </select>
      </label>
      <button type="button" class="toggle" class:on={snapOn} aria-pressed={snapOn} title="Snapping: dots snap to speakers, the center lines and a quarter-room grid, widths to quarter steps and turns to 15 degrees. Hold Alt while dragging to place freely." onclick={toggleSnap}>
        <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4 2v6a4 4 0 0 0 8 0V2" fill="none" stroke="currentColor" stroke-width="1.6" /><path d="M2.5 2h3M10.5 2h3" stroke="currentColor" stroke-width="1.6" /></svg>
        Snap
      </button>
      <span class="spacer"></span>
      {#if onExport}
        <button type="button" title="Export the mix in any speaker layout (5.1, 7.1.2, 7.1.4 and so on, one channel per speaker), stereo, or headphones. Pick the layout under Speakers in the export window." onclick={onExport}>Export Mixdown...</button>
      {/if}
    </div>
    <div class="body">
      <!-- Every placed track and bus, in its color: click to pick it. -->
      <div class="picks" role="listbox" aria-label="Tracks and buses in the room">
        {#each sources as s (s.id)}
          <button
            type="button"
            role="option"
            class="pick"
            class:on={ed.selectedTrack === s.id}
            aria-selected={ed.selectedTrack === s.id}
            style:--c={s.color}
            title={s.name}
            onclick={() => editor.selectTrack(s.id)}
          >
            <span class="swatch"></span><span class="pname">{s.name}</span>
          </button>
        {/each}
      </div>
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
            {#if s.stereo}
              {@const [l, rr] = sides(s.pos)}
              {@const [ax0, ay0] = toView(l[0], l[1], size)}
              {@const [bx0, by0] = toView(rr[0], rr[1], size)}
              {@const [ax, ay, bx, by] = apart(nx, ny, ax0, ay0, bx0, by0, s.pos.rotation ?? 0)}
              <line x1={ax} y1={ay} x2={bx} y2={by} class="pair" class:on style:stroke={s.color} />
              {#if on}
                {#each [false, true] as flip (flip)}
                  {@const t = turnArrow(nx, ny, s.pos.rotation ?? 0, flip, unit * 3.4)}
                  <!-- svelte-ignore a11y_no_static_element_interactions -->
                  <g class="turn" onpointerdown={(e) => onDotDown(e, s, "turn")}>
                    <title>Turn {s.name}: drag around the center ({deg(s.pos.rotation ?? 0)})</title>
                    <path d={t.arc} class="turn-hit" />
                    <path d={t.arc} class="turn-arc" style:stroke={s.color} />
                    <path d={t.heads} class="turn-head" style:fill={s.color} />
                  </g>
                {/each}
              {/if}
              {#each [[ax, ay, "left", "L"], [bx, by, "right", "R"]] as [px, py, mode, lab] (lab)}
                <!-- svelte-ignore a11y_no_static_element_interactions -->
                <g class="dot side" class:on transform="translate({px} {py})" onpointerdown={(e) => onDotDown(e, s, mode as "left" | "right")} onwheel={(e) => onDotWheel(e, s)}>
                  <title>{s.name} ({lab}): drag away from or toward the center to set the width (Alt also turns the pair); the wheel raises or lowers it</title>
                  {#if s.pos.z > 0.02}<circle r={r * 0.75 + 4} class="lift" style:stroke={s.color} />{/if}
                  <circle r={r * 0.75} class="body" style:fill={s.color} />
                  <text dy={unit * 0.35} style:font-size="{Math.max(8, unit * 0.9)}px">{lab}</text>
                </g>
              {/each}
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <g class="dot center" class:on transform="translate({nx} {ny})" onpointerdown={(e) => onDotDown(e, s, "move")} onwheel={(e) => onDotWheel(e, s)}>
                <title>{s.name}: drag to move the pair, the wheel to raise or lower it (Shift and the wheel to turn it)</title>
                <circle r={unit * 1.6} class="grab" />
                <rect x={-unit * 0.7} y={-unit * 0.7} width={unit * 1.4} height={unit * 1.4} transform="rotate(45)" style:fill={s.color} />
              </g>
            {:else}
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <g class="dot" class:on transform="translate({nx} {ny})" onpointerdown={(e) => onDotDown(e, s, "move")} onwheel={(e) => onDotWheel(e, s)}>
                <title>{s.name}: drag to move, the wheel to raise or lower</title>
                {#if s.pos.z > 0.02}<circle r={r + 4} class="lift" style:stroke={s.color} />{/if}
                <circle r={r} class="body" style:fill={s.color} />
              </g>
            {/if}
            {@const tag = `${s.name}${Math.abs(s.pos.z) > 0.02 ? (s.pos.z > 0 ? ` ↑${Math.round(s.pos.z * 100)}%` : ` ↓${Math.round(-s.pos.z * 100)}%`) : ""}${(s.pos.lfe ?? 0) > 0 ? " · LFE" : ""}`}
            <g class="tag" class:on transform="translate({nx} {ny + Math.max(r, unit * 1.6) + unit * 1.6})">
              <rect x={-(tag.length * 3.1 + 6)} y="-9" width={tag.length * 6.2 + 12} height="14" rx="7" />
              <text dy="2">{tag}</text>
            </g>
          {/each}
        </svg>
      </div>
      <div class="side">
        {#if picked}
          <div class="name" style:border-color={picked.color}>{picked.name}</div>
          {@const p = picked.pos}
          <SpaceFader label="Height" value={p.z} min={-1} max={1} step={0.01} origin={0} format={pct} title={heights ? "How high it sits: ceiling (100%) to ear level (0) and below. Speaker layouts have no speakers below ear level, so below 0 plays from the ear-level ones." : "This layout has no height speakers; height is kept for layouts that do"} onChange={(v) => setPos(picked, { z: v })} />
          <SpaceFader label="Size" value={p.size} min={0} max={1} step={0.01} format={pct} title="0 is a point; turn it up to blend it toward more of the speakers around it" onChange={(v) => setPos(picked, { size: v })} />
          <SpaceFader label="Spread" value={p.spread ?? 0} min={0} max={1} step={0.01} format={pct} title="Spreads the sound through the room as a diffuse field: decorrelated copies on every speaker in use, so it surrounds you instead of sitting at a point. 100% is all diffuse." onChange={(v) => setPos(picked, { spread: v })} />
          {#if picked.stereo}
            <SpaceFader label="Width" value={p.width} min={0} max={1} step={0.01} format={pct} title="How far its left and right sides sit from its center. Also drag a side dot away from or toward the center." onChange={(v) => setPos(picked, { width: v })} />
            <SpaceFader label="Rotation" value={p.rotation ?? 0} min={-180} max={180} step={1} origin={0} format={deg} title="Turns the pair around its center, clockwise seen from above. Also drag the turn arrows, or Shift and the wheel over it." onChange={(v) => setPos(picked, { rotation: v })} />
            <SpaceFader label="Spatialize" value={p.spatialize ?? 0} min={0} max={1} step={0.01} format={pct} title="Opens a stereo sound out into the room, the way the Method below picks, with what the sides share fed to the center." onChange={(v) => setPos(picked, { spatialize: v })} />
            <label class="row method" title={SPATIALIZE_MODES.find((m) => m.id === (p.spatialize_mode ?? "side_spread"))?.tip}>
              Method
              <select value={p.spatialize_mode ?? "side_spread"} onchange={(e) => setPos(picked, { spatialize_mode: e.currentTarget.value as SpatializeMode })}>
                {#each SPATIALIZE_MODES as m (m.id)}<option value={m.id} title={m.tip}>{m.label}</option>{/each}
              </select>
            </label>
          {/if}
          <SpaceFader label="LFE send" value={p.lfe ?? 0} min={0} max={1} step={0.01} format={(v) => (v > 0 ? `${(20 * Math.log10(v)).toFixed(1)} dB` : "Off")} title="Also sends it to the low-frequency (subwoofer) channel, filtered to 20 to 120 Hz. For kick, bass, booms. Layouts without that channel (2.0) leave it out." onChange={(v) => setPos(picked, { lfe: v })} />
          <SpaceFader label="Left / right" value={p.x} min={-1} max={1} step={0.01} origin={0} format={(v) => v.toFixed(2)} onChange={(v) => setPos(picked, { x: v })} />
          <SpaceFader label="Back / front" value={p.y} min={-1} max={1} step={0.01} origin={0} format={(v) => v.toFixed(2)} onChange={(v) => setPos(picked, { y: v })} />
          <button type="button" class="quiet" onclick={() => setPos(picked, { x: 0, y: 1, z: 0, size: 0, width: 1, rotation: 0, spread: 0, spatialize: 0, spatialize_mode: "side_spread" })}>Back to Front Center</button>
        {:else}
          <p class="hint">Click a dot (or a track) to set its height, size, spread, width, rotation, spatialize and LFE send. Drag a dot, or a stereo pair's center diamond, to move it; drag a pair's L or R dot to set its width; drag the arrows around a picked pair to turn it. The wheel raises or lowers one. Tracks routed to a bus play where the bus sits.</p>
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
  /* Controls in the app's style: flat, square-cornered, dense. */
  .bar select,
  .bar button,
  .intro select,
  .intro button,
  .side > button {
    height: 22px;
    padding: 1px 6px;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    background: var(--surface-0);
    color: var(--text);
    font: inherit;
  }
  .bar button:hover,
  .side > button:hover {
    background: var(--surface-2);
  }
  .bar .toggle {
    display: inline-flex;
    gap: 4px;
    align-items: center;
    color: var(--text-dim);
  }
  .bar .toggle svg {
    width: 12px;
    height: 12px;
  }
  .bar .toggle.on {
    background: var(--accent-dim);
    border-color: var(--accent);
    color: #fff;
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
  .picks {
    display: flex;
    flex-direction: column;
    gap: 3px;
    width: 96px;
    flex: none;
    padding: 6px 4px;
    overflow-y: auto;
    border-right: 1px solid var(--border);
  }
  .pick {
    display: flex;
    gap: 5px;
    align-items: center;
    min-width: 0;
    padding: 3px 5px;
    border: 1px solid transparent;
    border-radius: 4px;
    background: color-mix(in srgb, var(--c) 10%, transparent);
    color: var(--text-dim);
    font: inherit;
    font-size: 11px;
    text-align: left;
    cursor: pointer;
  }
  .pick:hover {
    color: var(--text);
    background: color-mix(in srgb, var(--c) 22%, transparent);
  }
  .pick.on {
    border-color: var(--c);
    background: color-mix(in srgb, var(--c) 32%, transparent);
    color: var(--text);
  }
  .swatch {
    flex: none;
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--c);
  }
  .pick .pname {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
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
  .dot.center .grab {
    fill: transparent;
    stroke: none;
  }
  .dot.center rect {
    stroke: #0e0e10;
    stroke-width: 1;
  }
  .dot.center.on rect {
    stroke: var(--text);
    stroke-width: 2;
  }
  .turn {
    cursor: grab;
  }
  .turn-hit {
    fill: none;
    stroke: transparent;
    stroke-width: 12;
  }
  .turn-arc {
    fill: none;
    stroke-width: 2;
    opacity: 0.85;
  }
  .turn:hover .turn-arc {
    opacity: 1;
    stroke-width: 2.5;
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
</style>
