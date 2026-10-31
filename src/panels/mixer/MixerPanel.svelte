<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  Mixer: one strip per track (pan, mute, solo, arm, fader, level meter),
  one per bus, and the master strip. Each track and bus strip picks its
  output (the master or a bus) and holds its aux sends. Every change is
  undoable; a fader or send drag is one undo step.
-->
<script lang="ts">
  import MonoIcon from "../../app/MonoIcon.svelte";
  import { trackOp } from "./solo";
  import { busOp, isRouted, MASTER_ID } from "./routing";
  import SendList from "./SendList.svelte";
  import { monoCheck } from "../../stores/mono-check";
  import ContextMenu, { type ContextItem } from "../../app/ContextMenu.svelte";
  import type { CommandRunner } from "../../stores/media-pool";
  import type { OutputDoc, SendDoc, SessionDoc, SessionStore } from "../../stores/session";
  import { channelNames } from "../space/space";
  import type { MeterLevel, TransportState, TransportStore } from "../../stores/transport";
  import { editor } from "../../stores/editor";
  import { dbToFader, faderToDb, formatDb, formatPan } from "./fader";

  interface Props {
    session: SessionStore;
    transport: TransportStore;
    runner: CommandRunner;
    onError?: (e: unknown) => void;
  }

  const { session, transport, runner, onError = console.error }: Props = $props();

  let doc = $state<SessionDoc | null>(null);
  /** 3D Space sessions: the master's speaker channel names, in order. */
  const bedNames = $derived(doc?.space ? channelNames(doc.space.layout) : []);
  $effect(() =>
    session.subscribe((d) => {
      doc = d;
    }),
  );
  let tp = $state<TransportState | null>(null);
  $effect(() =>
    transport.subscribe((t) => {
      tp = t;
    }),
  );
  let selected = $state<string | null>(null);
  $effect(() =>
    editor.subscribe((e) => {
      selected = e.selectedTrack;
    }),
  );

  const masterDb = $derived(doc?.master?.volume_db ?? 0);
  const routed = $derived(isRouted(doc));

  /** A track or bus strip. */
  interface Strip {
    kind: "track" | "bus";
    id: string;
    name: string;
    volume_db: number;
    pan: number;
    muted: boolean;
    solo: boolean;
    record_armed: boolean;
    mono: boolean;
    output: OutputDoc;
    sends: SendDoc[];
  }
  const strips = $derived<Strip[]>([
    ...(doc?.tracks ?? [])
      .filter((x) => !x.hidden)
      .map((t): Strip => ({ kind: "track", id: t.id, name: t.name, volume_db: t.volume_db, pan: t.pan, muted: t.muted, solo: t.solo, record_armed: t.record_armed, mono: !!t.mono, output: t.output ?? { type: "master" }, sends: t.sends ?? [] })),
    ...(doc?.buses ?? []).map((b): Strip => ({ kind: "bus", id: b.id, name: b.name, volume_db: b.volume_db, pan: b.pan, muted: b.muted, solo: b.solo, record_armed: false, mono: !!b.mono, output: b.output, sends: b.sends })),
  ]);

  type StripChanges = Partial<Pick<Strip, "volume_db" | "pan" | "muted" | "solo" | "record_armed" | "name" | "output" | "mono">>;
  function setStrip(st: Strip, changes: StripChanges, label: string) {
    const what = st.kind === "bus" ? label.replace("Track", "Bus") : label;
    const op = st.kind === "bus" ? busOp(doc, st.id, changes) : trackOp(doc, st.id, changes);
    void session.edit(what, op, runner).catch(onError);
  }

  /** Ctrl+click on Solo: this track alone, whatever else was soloed. */
  function soloOnly(id: string) {
    void session.edit("Solo Track Only", trackOp(doc, id, { solo: true }, true), runner).catch(onError);
  }

  let menu = $state<{ x: number; y: number; items: ContextItem[] } | null>(null);
  let renaming = $state<string | null>(null);
  function busMenu(e: MouseEvent, st: Strip) {
    if (st.kind !== "bus") return;
    e.preventDefault();
    menu = {
      x: e.clientX,
      y: e.clientY,
      items: [
        { kind: "item", label: "Rename Bus", run: () => (renaming = st.id) },
        { kind: "item", label: "Delete Bus", run: () => void session.edit("Delete Bus", { op: "remove_bus", id: st.id }, runner).catch(onError) },
      ],
    };
  }
  function finishRename(st: Strip, value: string) {
    renaming = null;
    const name = value.trim();
    if (name && name !== st.name) setStrip(st, { name }, "Rename Bus");
  }

  function setMaster(db: number) {
    void session.edit("Master Volume", { op: "set_master_volume", volume_db: db }, runner).catch(onError);
  }

  /** Vertical drag on a fader track. Shift moves in fine steps. */
  function dragFader(e: PointerEvent, current: number, apply: (db: number) => void) {
    if (e.button !== 0) return;
    e.preventDefault();
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    const rect = el.getBoundingClientRect();
    const startY = e.clientY;
    const startPos = dbToFader(current);
    let last = current;
    // Clicking the track (not the cap) jumps there; the cap drags from where it is.
    const onCap = (e.target as HTMLElement).classList.contains("cap");
    const posAt = (ev: PointerEvent) =>
      onCap || ev.shiftKey
        ? startPos - ((ev.clientY - startY) / rect.height) * (ev.shiftKey ? 0.2 : 1)
        : 1 - (ev.clientY - rect.top) / rect.height;
    const set = (ev: PointerEvent) => {
      const db = faderToDb(posAt(ev));
      if (db !== last) {
        last = db;
        apply(db);
      }
    };
    if (!onCap) set(e);
    const move = (ev: PointerEvent) => set(ev);
    const up = () => {
      el.removeEventListener("pointermove", move);
      el.removeEventListener("pointerup", up);
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up);
  }

  function wheelFader(e: WheelEvent, current: number, apply: (db: number) => void) {
    e.preventDefault();
    const step = e.shiftKey ? 0.1 : 0.5;
    const next = Math.min(12, Math.max(-60, Math.round((current + (e.deltaY < 0 ? step : -step)) * 10) / 10));
    apply(next);
  }

  function dragPan(e: PointerEvent, t: Strip) {
    if (e.button !== 0) return;
    e.preventDefault();
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    const startX = e.clientX;
    const start = t.pan;
    let last = start;
    const move = (ev: PointerEvent) => {
      const v = Math.min(100, Math.max(-100, Math.round(start + (ev.clientX - startX) * (ev.shiftKey ? 0.25 : 1))));
      if (v !== last) {
        last = v;
        setStrip(t, { pan: v }, "Track Pan");
      }
    };
    const up = () => {
      el.removeEventListener("pointermove", move);
      el.removeEventListener("pointerup", up);
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up);
  }

  /** Meters use the fader's scale, so the labels beside the master fader read true. */
  const onScale = (v: number) => (v > 0 ? dbToFader(20 * Math.log10(v)) * 100 : 0);
  const levelOf = (m: MeterLevel | undefined) => onScale(m?.level ?? 0);
  const peakOf = (m: MeterLevel | undefined) => onScale(m?.peak ?? 0);
  const SCALE = [12, 6, 0, -6, -12, -24, -48];
</script>

<section class="mixer" aria-label="Mixer">
  <div class="strips">
    {#each strips as t (t.id)}
      {@const m = tp?.tracks[t.id]}
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <div class="strip" class:bus={t.kind === "bus"} class:selected={selected === t.id} onclick={() => editor.selectTrack(t.id)} oncontextmenu={(e) => busMenu(e, t)}>
        {#if renaming === t.id}
          <!-- svelte-ignore a11y_autofocus -->
          <input
            class="rename"
            aria-label="Bus name"
            value={t.name}
            autofocus
            onkeydown={(e) => {
              if (e.key === "Enter") finishRename(t, e.currentTarget.value);
              if (e.key === "Escape") renaming = null;
            }}
            onblur={(e) => finishRename(t, e.currentTarget.value)}
          />
        {:else}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div class="name" title={t.kind === "bus" ? `${t.name} (bus). Double-click to rename, right-click for more.` : t.name} ondblclick={() => t.kind === "bus" && (renaming = t.id)}>
            {#if t.kind === "bus"}<span class="tag">BUS</span>{/if}{t.name}
          </div>
        {/if}
        {#if doc}
          <SendList {doc} {session} {runner} {onError} kind={t.kind} id={t.id} name={t.name} output={t.output} sends={t.sends} />
        {/if}
        {#if doc?.space}
          <div class="pan space3d" title="3D Space session: this plays at a spot in the room instead of a pan. Place it in the 3D Space panel (Multitrack > 3D Space).">3D</div>
        {:else}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="pan"
          title="Pan. Drag left or right, double-click to center."
          onpointerdown={(e) => dragPan(e, t)}
          ondblclick={() => setStrip(t, { pan: 0 }, "Track Pan")}
        >
          <div class="pan-line"></div>
          <div class="pan-dot" style:left="{(t.pan + 100) / 2}%"></div>
          <span>{formatPan(t.pan)}</span>
        </div>
        {/if}
        <div class="buttons">
          <button type="button" class:on={t.muted} class="m mute" title="Mute" aria-pressed={t.muted} onclick={() => setStrip(t, { muted: !t.muted }, t.muted ? "Unmute Track" : "Mute Track")}>M</button>
          <button type="button" class:on={t.solo} class="s" title={t.kind === "bus" ? "Solo: plays this bus and everything feeding it" : "Solo. Ctrl+click to hear only this track."} aria-pressed={t.solo} onclick={(e) => (t.kind === "track" && (e.ctrlKey || e.metaKey) ? soloOnly(t.id) : setStrip(t, { solo: !t.solo }, t.solo ? "Unsolo Track" : "Solo Track"))}>S</button>
          <button type="button" class:on={t.mono} class="mo" title="Mono: folds this strip's left and right into one centered signal, after its effects and before its fader and pan. Part of the mix, so exports hear it too." aria-pressed={t.mono} onclick={() => setStrip(t, { mono: !t.mono }, t.mono ? "Track Stereo" : "Track Mono")}><MonoIcon /></button>
          {#if t.kind === "track"}
            <button type="button" class:on={t.record_armed} class="r" title="Arm for recording" aria-pressed={t.record_armed} onclick={() => setStrip(t, { record_armed: !t.record_armed }, "Arm Track")}>R</button>
          {/if}
        </div>
        <div class="body">
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            class="fader"
            role="slider"
            tabindex="-1"
            aria-label="{t.name} volume"
            aria-valuemin={-60}
            aria-valuemax={12}
            aria-valuenow={t.volume_db}
            title="Volume. Drag, or use the mouse wheel. Shift for fine steps. Double-click for 0 dB."
            onpointerdown={(e) => dragFader(e, t.volume_db, (db) => setStrip(t, { volume_db: db }, "Track Volume"))}
            onwheel={(e) => wheelFader(e, t.volume_db, (db) => setStrip(t, { volume_db: db }, "Track Volume"))}
            ondblclick={() => setStrip(t, { volume_db: 0 }, "Track Volume")}
          >
            <div class="slot"></div>
            <div class="unity" style:bottom="{dbToFader(0) * 100}%"></div>
            <div class="cap" style:bottom="{dbToFader(t.volume_db) * 100}%"></div>
          </div>
          <div class="meter" aria-hidden="true">
            {#each [0, 1] as const as ch (ch)}
              <div class="mbar">
                <div class="mcover" style:height="{100 - levelOf(m?.[ch])}%"></div>
                {#if (m?.[ch].peak ?? 0) > 0}<div class="mpeak" style:bottom="{peakOf(m?.[ch])}%"></div>{/if}
              </div>
            {/each}
          </div>
        </div>
        <div class="db">{formatDb(t.volume_db)}</div>
      </div>
    {/each}
    {#if (doc?.tracks.length ?? 0) === 0}
      <p class="empty">Tracks appear here once the session has some.</p>
    {/if}
  </div>

  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="strip master" class:selected={selected === MASTER_ID} onclick={() => editor.selectTrack(MASTER_ID)} title="Click to show the master's Audio FX chain">
    <div class="name">Master</div>
    <button type="button" class="check" class:on={$monoCheck} title="Mono check: hear the whole mix folded to mono, to check how it holds up. Listening only: exports are not changed (to write a mono file, pick Mono in Export)." aria-pressed={$monoCheck} onclick={(e) => { e.stopPropagation(); void monoCheck.toggle().catch(onError); }}><MonoIcon /></button>
    <div class="body">
      <div class="scale" aria-hidden="true">
        {#each SCALE as s (s)}<span style:bottom="{dbToFader(s) * 100}%">{s > 0 ? `+${s}` : s}</span>{/each}
      </div>
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="fader"
        role="slider"
        tabindex="-1"
        aria-label="Master volume"
        aria-valuemin={-60}
        aria-valuemax={12}
        aria-valuenow={masterDb}
        title="Master volume. Drag, or use the mouse wheel. Double-click for 0 dB."
        onpointerdown={(e) => dragFader(e, masterDb, setMaster)}
        onwheel={(e) => wheelFader(e, masterDb, setMaster)}
        ondblclick={() => setMaster(0)}
      >
        <div class="slot"></div>
        <div class="unity" style:bottom="{dbToFader(0) * 100}%"></div>
        <div class="cap" style:bottom="{dbToFader(masterDb) * 100}%"></div>
      </div>
      {#if bedNames.length > 2}
        <!-- 3D Space sessions: one bar per speaker channel, named on hover. -->
        <div class="meter bed" aria-hidden="true">
          {#each bedNames as name, k (k)}
            {@const m = tp?.bed?.length === bedNames.length ? tp.bed[k] : undefined}
            <div class="mbar" title={name}>
              <div class="mcover" style:height="{100 - levelOf(m)}%"></div>
              {#if (m?.peak ?? 0) > 0}<div class="mpeak" style:bottom="{peakOf(m)}%"></div>{/if}
            </div>
          {/each}
        </div>
      {:else}
        <div class="meter wide" aria-hidden="true">
          {#each [0, 1] as const as ch (ch)}
            <div class="mbar">
              <div class="mcover" style:height="{100 - levelOf(tp?.master[ch])}%"></div>
              {#if (tp?.master[ch].peak ?? 0) > 0}<div class="mpeak" style:bottom="{peakOf(tp?.master[ch])}%"></div>{/if}
            </div>
          {/each}
        </div>
      {/if}
    </div>
    <div class="db">{formatDb(masterDb)}</div>
    {#if routed}
      <p class="lag" title="The mix uses buses, sends, a master chain or a sidechain, so it is mixed ahead of the speakers. Track and bus faders are heard about half a second after you move them; the master fader is instant.">Faders lag ½ s</p>
    {/if}
  </div>
</section>

{#if menu}
  <ContextMenu x={menu.x} y={menu.y} items={menu.items} onClose={() => (menu = null)} />
{/if}

<style>
  .pan.space3d {
    display: grid;
    place-items: center;
    color: var(--accent);
    font-size: 10px;
    font-weight: 700;
  }

  .mixer {
    display: flex;
    height: 100%;
    min-height: 0;
    overflow: hidden;
    background: var(--surface-0);
  }

  .strips {
    display: flex;
    flex: 1;
    gap: 2px;
    min-width: 0;
    padding: 4px;
    overflow-x: auto;
  }

  .strip {
    display: grid;
    flex: none;
    grid-template-rows: auto auto auto auto 1fr auto;
    gap: 6px;
    width: 92px;
    padding: 6px 5px;
    border: 1px solid var(--border);
    background: var(--surface-1);
    box-sizing: border-box;
  }

  .strip.bus {
    background: color-mix(in srgb, var(--surface-1) 82%, var(--clip-3) 18%);
  }

  .tag {
    margin-right: 3px;
    padding: 0 2px;
    border-radius: 2px;
    background: var(--surface-3);
    color: var(--text-dim);
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.04em;
  }

  .rename {
    width: 100%;
    min-width: 0;
    font: inherit;
    font-size: var(--font-size-small);
    box-sizing: border-box;
  }

  .lag {
    margin: 0;
    color: var(--text-faint);
    font-size: 9px;
    text-align: center;
  }

  .strip.selected {
    border-color: var(--accent-dim);
    background: var(--surface-2);
  }

  .strip.master {
    width: 96px;
    margin: 4px;
    grid-template-rows: auto auto 1fr auto auto;
  }

  .name {
    overflow: hidden;
    font-size: var(--font-size-small);
    text-align: center;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .pan {
    position: relative;
    height: 22px;
    cursor: ew-resize;
  }

  .pan-line {
    position: absolute;
    top: 6px;
    left: 4px;
    right: 4px;
    height: 2px;
    background: var(--surface-3);
  }

  .pan-dot {
    position: absolute;
    top: 3px;
    width: 8px;
    height: 8px;
    margin-left: -4px;
    border-radius: 50%;
    background: var(--value);
  }

  .pan span {
    position: absolute;
    bottom: 0;
    width: 100%;
    color: var(--text-dim);
    font-family: var(--font-mono);
    font-size: 10px;
    text-align: center;
  }

  .buttons {
    display: flex;
    gap: 2px;
    justify-content: center;
  }

  .buttons button {
    width: 20px;
    height: 18px;
    padding: 0;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    background: var(--surface-3);
    color: var(--text-dim);
    font: inherit;
    font-size: 10px;
    font-weight: 600;
  }

  .buttons .m.on {
    background: var(--accent-dim);
    color: #fff;
  }

  .buttons .s.on {
    background: var(--clip-5);
    color: #111;
  }

  .buttons .mo {
    width: 22px;
    font-size: 8px;
  }

  .buttons .mo.on {
    background: var(--accent-dim);
    color: #fff;
  }

  .check {
    height: 18px;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    background: var(--surface-3);
    color: var(--text-dim);
    font: inherit;
    font-size: 9px;
    font-weight: 700;
  }

  .check.on {
    background: var(--clip-5);
    color: #111;
  }

  .buttons .r.on {
    background: var(--danger);
    color: #fff;
  }

  .body {
    display: flex;
    gap: 6px;
    justify-content: center;
    min-height: 80px;
  }

  .fader {
    position: relative;
    width: 22px;
    cursor: ns-resize;
  }

  .slot {
    position: absolute;
    top: 0;
    bottom: 0;
    left: 10px;
    width: 2px;
    background: var(--surface-0);
  }

  .unity {
    position: absolute;
    left: 2px;
    right: 2px;
    height: 1px;
    background: var(--border-strong);
  }

  .cap {
    position: absolute;
    left: 0;
    width: 22px;
    height: 10px;
    margin-bottom: -5px;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    background: linear-gradient(var(--surface-3), var(--surface-2));
    box-sizing: border-box;
  }

  .cap::after {
    content: "";
    position: absolute;
    top: 4px;
    left: 3px;
    right: 3px;
    height: 1px;
    background: var(--text-dim);
  }

  .meter {
    display: flex;
    gap: 1px;
  }

  .mbar {
    position: relative;
    width: 5px;
    overflow: hidden;
    /* Green below -12 dB, yellow to 0 dB, red above (on the fader scale). */
    background: linear-gradient(to top, #3d8f6a 0 53%, #c9b33f 53% 77%, #e5534b 77% 100%);
  }

  .meter.wide .mbar {
    width: 8px;
  }

  .meter.bed .mbar {
    width: 3px;
  }

  .mcover {
    position: absolute;
    top: 0;
    width: 100%;
    background: var(--surface-0);
  }

  .mpeak {
    position: absolute;
    width: 100%;
    height: 1px;
    background: var(--text);
  }

  .db {
    color: var(--value);
    font-family: var(--font-mono);
    font-size: 10px;
    text-align: center;
  }

  .scale {
    position: relative;
    width: 20px;
  }

  .scale span {
    position: absolute;
    right: 0;
    transform: translateY(50%);
    color: var(--text-faint);
    font-size: 9px;
  }

  .empty {
    margin: auto;
    color: var(--text-faint);
  }
</style>
