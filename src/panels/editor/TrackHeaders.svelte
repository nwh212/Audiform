<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  Copyright (c) The Audiform contributors.
  This file is part of Audiform, based on Altered Audio Editor.

  The Multitrack editor's header column: one header per track (with its
  automation lane heads under it), per bus, and the master at the end.
  Mix controls or the effects show, as the editor's header view says.
-->
<script lang="ts">
  import type { ContextItem } from "../../app/ContextMenu.svelte";
  import MonoIcon from "../../app/MonoIcon.svelte";
  import { audioInputs, openSoundSettings, pluginOpenWindow, type AudioInput, type EffectInfo } from "../../app/core";
  import { keepLast } from "../../app/settings/device-cache";
  import { chainTarget } from "../../stores/chain-target";
  import { editor, type EditorState } from "../../stores/editor";
  import type { CommandRunner } from "../../stores/media-pool";
  import { monoCheck } from "../../stores/mono-check";
  import { isMidi, type BusDoc, type EffectSlotDoc, type EnvelopeDoc, type Op, type SessionDoc, type SessionStore, type TrackChanges, type TrackDoc } from "../../stores/session";
  import { quiet, type MeterLevel, type TransportState, type TransportStore } from "../../stores/transport";
  import { busChoices, busOp, MASTER_ID } from "../mixer/routing";
  import SendList from "../mixer/SendList.svelte";
  import { trackOp } from "../mixer/solo";
  import { channelNames } from "../space/space";
  import type { MenuPlugin } from "../voice-chain/chain";
  import { effectWindowRequest } from "../voice-chain/effect-window";
  import { rowTarget } from "../voice-chain/slot-drag";
  import { autoChoices, autoMode, dropLane, isOrphan, nextTarget, putLane, setMode, shownLanes, targetKey } from "./automation";
  import Dial from "./Dial.svelte";
  import { busHeight, formatDb, formatPan, HEADER_W, ROW_AUTO, ROW_INPUT, ROW_OUTPUT, ROW_SENDS, trackColor } from "./editor-layout";
  import HeaderFx from "./HeaderFx.svelte";
  import type { Row } from "./rows";
  import { inputMenuItems, instrumentMenuItems } from "./track-menus";
  import TrackMeter from "./TrackMeter.svelte";

  /** An installed instrument plug-in. */
  type Instrument = MenuPlugin & { hasWindow: boolean };

  interface Props {
    /** Tracks and buses, top to bottom. */
    rows: Row[];
    /** The master header shows after the rows. */
    showMaster: boolean;
    doc: SessionDoc | null;
    session: SessionStore;
    runner: CommandRunner;
    transport: TransportStore;
    tp: TransportState | null;
    ed: EditorState;
    /** What the headers show: mix controls (with input, output and sends as room allows) or the effects. */
    headerView: "mix" | "fx";
    /** The track or bus whose name is being edited. */
    renaming: string | null;
    instruments: Instrument[];
    fxCatalog: EffectInfo[];
    fxPlugins: MenuPlugin[];
    onError: (e: unknown) => void;
    run: (id: string) => void;
    /** Brings the Audio FX panel forward. */
    onShowChain: () => void;
    onMenu: (x: number, y: number, items: ContextItem[]) => void;
    onRename: (id: string | null) => void;
    trackMenu: (track: TrackDoc) => ContextItem[];
    busMenu: (b: BusDoc) => ContextItem[];
    /** A track's height as drawn now (live while it is being resized). */
    shownHeight: (t: TrackDoc) => number;
    /** An automation lane's height as drawn now. */
    liveLaneH: (l: { id: string; height?: number }) => number;
    /** A track's meter: the input while armed, otherwise playback. */
    meterOf: (track: TrackDoc) => readonly [MeterLevel, MeterLevel] | null;
    setTrack: (track: TrackDoc, changes: TrackChanges, label: string) => void;
    setLanes: (track: TrackDoc, envelopes: EnvelopeDoc[], label: string) => void;
    toggleLanes: (track: TrackDoc) => void;
    addLane: (track: TrackDoc) => void;
    startRowDrag: (e: PointerEvent, id: string, label: string) => void;
    startResize: (e: PointerEvent, track: TrackDoc) => void;
    resizeLane: (e: PointerEvent, track: TrackDoc, lane: EnvelopeDoc) => void;
  }

  const {
    rows,
    showMaster,
    doc,
    session,
    runner,
    transport,
    tp,
    ed,
    headerView,
    renaming,
    instruments,
    fxCatalog,
    fxPlugins,
    onError,
    run,
    onShowChain,
    onMenu,
    onRename,
    trackMenu,
    busMenu,
    shownHeight,
    liveLaneH,
    meterOf,
    setTrack,
    setLanes,
    toggleLanes,
    addLane,
    startRowDrag,
    startResize,
    resizeLane,
  }: Props = $props();

  /** Opens an effect from a header: a plug-in in its own window, anything else in the Audio FX chain. */
  function openHeaderEffect(rowId: string, rowName: string, slot: EffectSlotDoc) {
    const info = fxCatalog.find((e) => e.id === slot.effect);
    if (info?.plugin && info.hasWindow) {
      void pluginOpenWindow(slot, `${info.name} - ${rowName}`).catch(onError);
      return;
    }
    editor.selectTrack(rowId);
    onShowChain();
    effectWindowRequest.set(slot.id);
  }
  function headerEdit(label: string, op: Op) {
    void session.edit(label, op, runner).catch(onError);
  }
  /** Where a track's or bus's sound goes: the master or a bus. */
  function setOutput(kind: "track" | "bus", id: string, value: string) {
    const o = value === MASTER_ID ? { type: "master" as const } : { type: "bus" as const, bus_id: value };
    headerEdit(kind === "bus" ? "Route Bus Output" : "Route Track Output", kind === "bus" ? busOp(doc, id, { output: o }) : trackOp(doc, id, { output: o }));
  }
  const outName = (o: { type: string; bus_id?: string } | undefined) => (o?.type === "bus" ? (doc?.buses?.find((b) => b.id === o.bus_id)?.name ?? "Missing bus") : "Master");

  function setInstrument(track: TrackDoc, effect: string) {
    if (!effect) {
      setTrack(track, { clear_instrument: true }, "Remove Instrument");
      return;
    }
    const slot = { id: crypto.randomUUID(), effect, enabled: true, params: {}, preset: null };
    setTrack(track, { instrument: slot }, "Set Instrument");
  }

  /** The instrument picker: by type, then by maker, as in the Add effect menu. */
  function openInstrumentMenu(e: MouseEvent, track: TrackDoc) {
    const el = e.currentTarget as HTMLElement;
    const r = (el.closest(".instrument") ?? el).getBoundingClientRect();
    onMenu(r.left, r.bottom, instrumentMenuItems(track, instruments, (effect) => setInstrument(track, effect)));
  }

  function openInstrument(track: TrackDoc) {
    if (!track.instrument) return;
    const name = instruments.find((i) => i.effect === track.instrument!.effect)?.name ?? "Instrument";
    void pluginOpenWindow(track.instrument, `${name}: ${track.name}`).catch(onError);
  }

  function setBus(b: BusDoc, changes: Record<string, unknown>, label: string) {
    void session.edit(label, busOp(doc, b.id, changes), runner).catch(onError);
  }

  function commitBusRename(b: BusDoc, value: string) {
    onRename(null);
    const v = value.trim();
    if (v && v !== b.name) setBus(b, { name: v }, "Rename Bus");
  }

  /** Ctrl+click on Solo: this track alone, whatever else was soloed. */
  function soloOnly(id: string) {
    void session.edit("Solo Track Only", trackOp(doc, id, { solo: true }, true), runner).catch(onError);
  }

  /** Points a lane at another setting. Its points are kept only when the value range is the same kind. */
  function pickTarget(track: TrackDoc, lane: EnvelopeDoc, key: string) {
    const c = autoChoices(track, fxCatalog, !!doc?.space).find((x) => targetKey(x.target) === key);
    if (!c || targetKey(c.target) === targetKey(lane.target)) return;
    const same = c.target.type === lane.target.type && c.target.type !== "effect_param";
    setLanes(track, putLane(track, { ...lane, target: c.target, points: same ? lane.points : [] }), `Automate ${c.label}`);
  }

  function setValue(track: TrackDoc, key: "volume_db" | "pan", value: number) {
    const label = key === "volume_db" ? "Track Volume" : "Track Pan";
    void session.edit(label, { op: "set_track", id: track.id, changes: { [key]: value } }, runner).catch(onError);
  }

  function commitRename(track: TrackDoc, value: string) {
    onRename(null);
    const v = value.trim();
    if (v && v !== track.name) setTrack(track, { name: v }, "Rename Track");
  }

  // Track reordering by dragging the header.
  function onHeaderPointer(e: PointerEvent, track: TrackDoc) {
    // Controls work as controls; only the rest of the header picks up and drags the track.
    if (e.button !== 0 || (e.target as HTMLElement).closest("button, input, select, textarea, label, a")) return;
    startRowDrag(e, track.id, "Move Track");
  }

  function setMasterVolume(db: number) {
    void session.edit("Master Volume", { op: "set_master_volume", volume_db: db }, runner).catch(onError);
  }

  /** The track's input as shown on its picker. */
  function inputLabel(track: TrackDoc): string {
    if (track.input === "none") return "None";
    if (track.input) return track.input;
    return track.channels === "mono" ? "Default Mono Input" : "Default Stereo Input";
  }

  /** Input picker: default, None, then Mono and Stereo submenus, each grouped by device. */
  async function openInputMenu(e: MouseEvent, track: TrackDoc) {
    e.stopPropagation();
    const x = e.clientX;
    const y = e.clientY;
    let inputs: AudioInput[] = [];
    try {
      inputs = keepLast("in:track", await audioInputs()).list;
    } catch {
      inputs = [];
    }
    onMenu(
      x,
      y,
      inputMenuItems(
        track,
        inputs,
        (value) => setTrack(track, { input: value }, "Track Input"),
        () => void openSoundSettings().catch(onError),
      ),
    );
  }
</script>

<div class="headers" style:width="{HEADER_W}px">
  {#each rows as row (row.id)}
    {#if row.kind === "track"}
    {@const track = row.track}
    {@const i = row.i}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="header"
      class:alt={i % 2 === 1}
      class:selected={ed.selectedTrack === track.id}
      class:fx={$chainTarget === track.id}
      style:--track-color={trackColor(track.color, i)}
      data-header={track.id}
      data-row={track.id}
      style:height="{shownHeight(track)}px"
      onpointerdown={(e) => onHeaderPointer(e, track)}
      oncontextmenu={(e) => {
        e.preventDefault();
        e.stopPropagation();
        onMenu(e.clientX, e.clientY, trackMenu(track));
      }}
    >
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="grab" title="Click to select the track (its effects show in the Audio FX chain). Drag to move it up or down." onpointerdown={(e) => onHeaderPointer(e, track)}></div>
      {#if $chainTarget === track.id}<span class="fx-tag" title="The Audio FX chain is showing this track's effects">FX</span>{/if}
      <div class="line">
        <button
          type="button"
          class="fold"
          class:folded={!!track.collapsed}
          title={track.collapsed ? "Expand track" : "Collapse track to its name"}
          aria-label={track.collapsed ? "Expand track" : "Collapse track"}
          aria-expanded={!track.collapsed}
          onclick={() => setTrack(track, { collapsed: !track.collapsed }, track.collapsed ? "Expand Track" : "Collapse Track")}
        >
          <svg viewBox="0 0 10 10" aria-hidden="true"><path d="M2 3.5l3 3 3-3" fill="none" stroke="currentColor" stroke-width="1.4" /></svg>
        </button>
        {#if renaming === track.id}
          <!-- svelte-ignore a11y_autofocus -->
          <input
            class="rename"
            type="text"
            value={track.name}
            autofocus
            onblur={(e) => commitRename(track, e.currentTarget.value)}
            onkeydown={(e) => {
              e.stopPropagation();
              if (e.key === "Enter") commitRename(track, e.currentTarget.value);
              if (e.key === "Escape") onRename(null);
            }}
          />
        {:else}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <span class="name" title="Double-click to rename" ondblclick={() => onRename(track.id)}>{track.name}</span>
        {/if}
        <button type="button" class="ms" class:on={track.muted} title="Mute" aria-pressed={track.muted} onclick={() => setTrack(track, { muted: !track.muted }, track.muted ? "Unmute Track" : "Mute Track")}>M</button>
        <button type="button" class="ms solo" class:on={track.solo} title="Solo. Ctrl+click to hear only this track." aria-pressed={track.solo} onclick={(e) => (e.ctrlKey || e.metaKey ? soloOnly(track.id) : setTrack(track, { solo: !track.solo }, track.solo ? "Unsolo Track" : "Solo Track"))}>S</button>
        {#if !isMidi(track)}
        <button type="button" class="ms rec" class:on={track.record_armed} title="Arm for recording: Record records this track's input" aria-pressed={track.record_armed} onclick={() => setTrack(track, { record_armed: !track.record_armed }, "Arm Track")}>R</button>
        <button
          type="button"
          class="ms mon"
          class:on={!!track.monitor_input}
          title="Monitor Input: hear this track's input through the speakers while it is armed. Use headphones: speakers feed back into the mic."
          aria-pressed={!!track.monitor_input}
          onclick={() => setTrack(track, { monitor_input: !track.monitor_input }, "Monitor Input")}
        >
          <svg viewBox="0 0 16 16" aria-hidden="true"><circle cx="8" cy="8" r="1.6" fill="currentColor" /><path d="M5 5a4.2 4.2 0 0 0 0 6M11 5a4.2 4.2 0 0 1 0 6" fill="none" stroke="currentColor" stroke-width="1.3" /></svg>
        </button>
        {/if}
        <button type="button" class="ms mono short" class:on={!!track.mono} title="Mono: folds this track's left and right into one centered signal, after its effects and before its fader and pan. Part of the mix, so exports hear it too." aria-pressed={!!track.mono} onclick={() => setTrack(track, { mono: !track.mono }, track.mono ? "Track Stereo" : "Track Mono")}><MonoIcon /></button>
      </div>
      {#if !track.collapsed && headerView === "fx" && doc}
        <HeaderFx
          {doc}
          target={rowTarget(track.id, false)}
          chain={track.effects}
          catalog={fxCatalog}
          plugins={fxPlugins}
          onEdit={headerEdit}
          onOpen={(slot) => openHeaderEffect(track.id, track.name, slot)}
          onMenu={(x, y, items) => onMenu(x, y, items as ContextItem[])}
        />
      {/if}
      {#if !track.collapsed && headerView === "mix"}
      <div class="line">
        <Dial
          value={track.volume_db}
          min={-60}
          max={12}
          step={0.1}
          origin={0}
          label="Volume"
          icon="volume"
          format={(v) => `${formatDb(v)}`}
          onChange={(v) => setValue(track, "volume_db", v)}
        />
        {#if doc?.space}
          <button
            type="button"
            class="ms space3d"
            title="3D Space session: this track plays at a spot in the room instead of a pan. Click to place it in the 3D Space panel."
            onclick={() => {
              editor.selectTrack(track.id);
              run("multitrack.space");
            }}>3D</button
          >
        {:else}
          <Dial value={track.pan} min={-100} max={100} step={1} origin={0} label="Pan" icon="pan" format={formatPan} onChange={(v) => setValue(track, "pan", v)} />
        {/if}
        <button
          type="button"
          class="ms pol"
          class:on={!!track.polarity_inverted}
          title="Invert Polarity: flips the track's waveform, to fix a mic wired backwards or two mics fighting each other"
          aria-pressed={!!track.polarity_inverted}
          onclick={() => setTrack(track, { polarity_inverted: !track.polarity_inverted }, "Invert Polarity")}
        >
          <svg viewBox="0 0 16 16" aria-hidden="true"><circle cx="8" cy="8" r="4.5" fill="none" stroke="currentColor" stroke-width="1.3" /><path d="M3.5 13.5l9-11" stroke="currentColor" stroke-width="1.3" /></svg>
        </button>
      </div>
      {/if}
      {#if !track.collapsed && isMidi(track)}
        {@const inst = track.instrument ? instruments.find((x) => x.effect === track.instrument!.effect) : null}
        <div class="line">
<div class="instrument">
            <button
              type="button"
              class="inst-name"
              title={inst ? `Open ${inst.name}. The arrow changes the instrument.` : track.instrument ? "This instrument is not installed (or not scanned yet). Effects > Plug-in Manager." : "The VST3 instrument that plays this track's notes. Grouped by type and by maker."}
              onclick={(e) => (inst ? openInstrument(track) : openInstrumentMenu(e, track))}
            >{inst ? `${inst.name}${inst.vendor ? ` (${inst.vendor})` : ""}` : track.instrument ? "Missing instrument" : instruments.length ? "No instrument" : "No instruments found"}</button>
            <button type="button" class="chev" aria-haspopup="menu" title="Change the instrument" aria-label="Change the instrument" onclick={(e) => openInstrumentMenu(e, track)}>▾</button>
          </div>
        </div>
      {/if}
      {#if !track.collapsed && headerView === "mix" && shownHeight(track) >= ROW_INPUT && !isMidi(track)}
        <div class="line">
          <button type="button" class="pick" title="The input this track records from" onclick={(e) => void openInputMenu(e, track)}>
            <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M1.5 8h9M7.5 5l3 3-3 3" fill="none" stroke="currentColor" stroke-width="1.4" /></svg>
            <span>{inputLabel(track)}</span>
            <span class="chev" aria-hidden="true">&rsaquo;</span>
          </button>
        </div>
      {/if}
      {#if !track.collapsed && headerView === "mix" && shownHeight(track) >= ROW_OUTPUT && doc}
        {@const out = track.output ?? { type: "master" }}
        <div class="line">
          <label class="pick outpick" title="Where this track's sound goes after its fader: the master or a bus">
            <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M14.5 8h-9M8.5 5l-3 3 3 3" fill="none" stroke="currentColor" stroke-width="1.4" /><path d="M3 3v10" stroke="currentColor" stroke-width="1.4" /></svg>
            <select aria-label="{track.name} output" value={out.type === "bus" ? out.bus_id : MASTER_ID} onchange={(e) => setOutput("track", track.id, e.currentTarget.value)}>
              <option value={MASTER_ID}>Master</option>
              {#each busChoices(doc, track.id) as bc (bc.id)}<option value={bc.id}>{bc.name}</option>{/each}
              {#if out.type === "bus" && !busChoices(doc, track.id).some((bc) => out.type === "bus" && bc.id === out.bus_id)}<option value={out.bus_id}>{outName(out)}</option>{/if}
            </select>
          </label>
          {#if shownHeight(track) < ROW_AUTO}{@render lanesToggle(track)}{/if}
        </div>
      {/if}
      {#if !track.collapsed && headerView === "mix" && shownHeight(track) >= ROW_AUTO}
        <div class="line">
          <select
            class="auto"
            value={autoMode(track)}
            title="Automation mode. Read plays the track's automation lanes; Off plays none of them (the fader, pan and effects hold still). Write, Latch and Touch (recording moves while playing) are not built yet."
            onchange={(e) => setLanes(track, setMode(track, e.currentTarget.value === "off" ? "off" : "read"), "Automation Mode")}
          >
            <option value="off" disabled={(track.envelopes?.length ?? 0) === 0}>Off</option>
            <option value="read">Read</option>
            <option value="write" disabled>Write (not built yet)</option>
            <option value="latch" disabled>Latch (not built yet)</option>
            <option value="touch" disabled>Touch (not built yet)</option>
          </select>
          {@render lanesToggle(track)}
        </div>
      {/if}
      {#if !track.collapsed && headerView === "mix" && shownHeight(track) >= ROW_SENDS && doc}
        <div class="sendview">
          <SendList {doc} {session} {runner} {onError} kind="track" id={track.id} name={track.name} output={track.output ?? { type: "master" }} sends={track.sends ?? []} noOutput />
        </div>
      {/if}
      {#if !track.collapsed}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="resize" title="Drag to change the track height" onpointerdown={(e) => startResize(e, track)}></div>
      {/if}
      <!-- Level meter: the input while armed, otherwise playback. -->
      <TrackMeter levels={meterOf(track)} armed={track.record_armed} onClear={() => transport.clearOver(track.id)} />
    </div>
    {#if track.show_automation && !track.collapsed}
      {@const lanes = shownLanes(track)}
      {#each lanes as lane, k (lane.id || "new")}
        {@const choices = autoChoices(track, fxCatalog, !!doc?.space)}
        {@const taken = (c: (typeof choices)[number]) => targetKey(c.target) !== targetKey(lane.target) && lanes.some((l) => targetKey(l.target) === targetKey(c.target))}
        <div class="header auto-head" class:alt={i % 2 === 1} style:--track-color={trackColor(track.color, i)} style:height="{liveLaneH(lane)}px">
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div class="lane-edge" title="Drag to make this lane taller or shorter" onpointerdown={(e) => resizeLane(e, track, lane)}></div>
          <div class="line">
            <select
              class="auto-pick"
              value={targetKey(lane.target)}
              title="The setting this lane moves"
              onchange={(e) => pickTarget(track, lane, e.currentTarget.value)}
            >
              {#if isOrphan(track, fxCatalog, lane.target)}<option value={targetKey(lane.target)}>Removed effect</option>{/if}
              {#each choices.filter((c) => !c.group) as c (targetKey(c.target))}
                <option value={targetKey(c.target)} disabled={taken(c)}>{c.label}</option>
              {/each}
              <!-- Each effect (plug-ins too), with its controls under it. -->
              {#each [...new Set(choices.flatMap((c) => (c.group ? [c.group] : [])))] as g (g)}
                <optgroup label={g.slice(0, g.lastIndexOf("#"))}>
                  {#each choices.filter((c) => c.group === g) as c (targetKey(c.target))}
                    <option value={targetKey(c.target)} disabled={taken(c)}>{c.name}</option>
                  {/each}
                </optgroup>
              {/each}
            </select>
          </div>
          <div class="line">
            <button
              type="button"
              class="ms wide"
              class:on={lane.enabled}
              title={lane.enabled ? "This lane plays (Read). Click to turn it off and hold the setting still." : "This lane is off. Click to play it."}
              aria-pressed={lane.enabled}
              disabled={!lane.id}
              onclick={() => setLanes(track, putLane(track, { ...lane, enabled: !lane.enabled }), lane.enabled ? "Automation Lane Off" : "Automation Lane On")}
            >{lane.enabled ? "Read" : "Off"}</button>
            <span class="auto-count">{lane.points.length === 0 ? "Click the lane to add points" : `${lane.points.length} point${lane.points.length === 1 ? "" : "s"}`}</span>
            {#if k === lanes.length - 1}
              <button type="button" class="ms" title="Add a lane for another setting" disabled={!nextTarget(track, fxCatalog, !!doc?.space)} onclick={() => addLane(track)}>+</button>
            {/if}
            <button type="button" class="ms" title="Remove this lane and its points" disabled={!lane.id} onclick={() => setLanes(track, dropLane(track, lane.id), "Remove Automation Lane")}>×</button>
          </div>
        </div>
      {/each}
    {/if}
    {:else}
    {@const b = row.bus}
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div
      class="header bus"
      class:selected={ed.selectedTrack === b.id}
      class:fx={$chainTarget === b.id}
      style:--track-color={b.color ? trackColor(b.color, 0) : "var(--clip-3)"}
      style:height="{busHeight(b)}px"
      data-bus-header={b.id}
      data-row={b.id}
      onclick={() => editor.selectTrack(b.id)}
      oncontextmenu={(e) => {
        e.preventDefault();
        e.stopPropagation();
        onMenu(e.clientX, e.clientY, busMenu(b));
      }}
    >
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="grab" title="Drag to move the bus up or down" onpointerdown={(e) => startRowDrag(e, b.id, "Move Bus")}></div>
      <div class="line">
        <span class="bus-tag" title="Bus track: sums the tracks routed or sent to it, then runs its own Audio FX chain and fader">BUS</span>
        {#if renaming === b.id}
          <!-- svelte-ignore a11y_autofocus -->
          <input
            class="rename"
            type="text"
            value={b.name}
            autofocus
            onblur={(e) => commitBusRename(b, e.currentTarget.value)}
            onkeydown={(e) => {
              e.stopPropagation();
              if (e.key === "Enter") commitBusRename(b, e.currentTarget.value);
              if (e.key === "Escape") onRename(null);
            }}
          />
        {:else}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <span class="name" title="Double-click to rename" ondblclick={() => onRename(b.id)}>{b.name}</span>
        {/if}
        <button type="button" class="ms" class:on={b.muted} title="Mute" aria-pressed={b.muted} onclick={() => setBus(b, { muted: !b.muted }, b.muted ? "Unmute Bus" : "Mute Bus")}>M</button>
        <button type="button" class="ms solo" class:on={b.solo} title="Solo: plays this bus and everything feeding it" aria-pressed={b.solo} onclick={() => setBus(b, { solo: !b.solo }, b.solo ? "Unsolo Bus" : "Solo Bus")}>S</button>
      </div>
      {#if headerView === "mix"}
        <div class="line">
          <Dial value={b.volume_db} min={-60} max={12} step={0.1} origin={0} label="Volume" icon="volume" format={(v) => `${formatDb(v)}`} onChange={(v) => setBus(b, { volume_db: v }, "Bus Volume")} />
          <Dial value={b.pan} min={-100} max={100} step={1} origin={0} label="Pan" icon="pan" format={formatPan} onChange={(v) => setBus(b, { pan: v }, "Bus Pan")} />
          <button type="button" class="ms mono" class:on={!!b.mono} title="Mono: folds this bus's left and right into one centered signal, after its effects and before its fader and pan. Part of the mix, so exports hear it too." aria-pressed={!!b.mono} onclick={() => setBus(b, { mono: !b.mono }, b.mono ? "Bus Stereo" : "Bus Mono")}><MonoIcon /></button>
        </div>
        {#if busHeight(b) >= ROW_OUTPUT - 8 && doc}
          <div class="line">
            <label class="pick outpick" title="Where this bus's sound goes after its fader: the master or another bus">
              <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M14.5 8h-9M8.5 5l-3 3 3 3" fill="none" stroke="currentColor" stroke-width="1.4" /><path d="M3 3v10" stroke="currentColor" stroke-width="1.4" /></svg>
              <select aria-label="{b.name} output" value={b.output.type === "bus" ? b.output.bus_id : MASTER_ID} onchange={(e) => setOutput("bus", b.id, e.currentTarget.value)}>
                <option value={MASTER_ID}>Master</option>
                {#each busChoices(doc, b.id) as bc (bc.id)}<option value={bc.id}>{bc.name}</option>{/each}
              </select>
            </label>
          </div>
        {/if}
        {#if busHeight(b) >= ROW_SENDS - 30 && doc}
          <div class="sendview">
            <SendList {doc} {session} {runner} {onError} kind="bus" id={b.id} name={b.name} output={b.output} sends={b.sends} noOutput />
          </div>
        {/if}
      {:else if doc}
        <HeaderFx
          {doc}
          target={rowTarget(b.id, true)}
          chain={b.effects}
          catalog={fxCatalog}
          plugins={fxPlugins}
          onEdit={headerEdit}
          onOpen={(slot) => openHeaderEffect(b.id, b.name, slot)}
          onMenu={(x, y, items) => onMenu(x, y, items as ContextItem[])}
        />
      {/if}
      <TrackMeter levels={meterOf(b as unknown as TrackDoc)} armed={false} onClear={() => transport.clearOver(b.id)} />
    </div>
    {/if}
  {/each}
  {#if doc && showMaster}
    {@const mfx = doc.master?.effects?.slots.filter((x) => x.enabled).length ?? 0}
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div
      class="header master"
      class:selected={ed.selectedTrack === MASTER_ID}
      class:fx={$chainTarget === MASTER_ID}
      style:height="72px"
      title="Master: every track and bus ends here. Click to show its Audio FX chain."
      onclick={() => editor.selectTrack(MASTER_ID)}
      oncontextmenu={(e) => {
        e.preventDefault();
        e.stopPropagation();
      }}
    >
      <div class="line">
        <span class="bus-tag">OUT</span>
        <span class="name">Master</span>
        <button type="button" class="ms fx" class:on={mfx > 0} title={mfx > 0 ? `Master Audio FX: ${mfx} effect${mfx === 1 ? "" : "s"}. Click to edit.` : "Master Audio FX: empty. Click to add effects."} onclick={() => editor.selectTrack(MASTER_ID)}>fx</button>
      </div>
      <div class="line">
        <Dial value={doc.master?.volume_db ?? 0} min={-60} max={12} step={0.1} origin={0} label="Volume" icon="volume" format={(v) => `${formatDb(v)}`} onChange={setMasterVolume} />
        <button type="button" class="ms mono check" class:on={$monoCheck} title="Mono check: hear the whole mix folded to mono, to check how it holds up. Listening only: exports are not changed (to write a mono file, pick Mono in Export)." aria-pressed={$monoCheck} onclick={(e) => { e.stopPropagation(); void monoCheck.toggle().catch(onError); }}><MonoIcon /></button>
      </div>
      {#if doc.space && channelNames(doc.space.layout).length > 2}
        {@const names = channelNames(doc.space.layout)}
        <TrackMeter levels={tp?.bed?.length === names.length ? tp.bed : names.map(() => quiet())} {names} armed={false} onClear={() => transport.clearOver()} />
      {:else}
        <TrackMeter levels={tp?.master ?? null} armed={false} onClear={() => transport.clearOver()} />
      {/if}
    </div>
  {/if}
</div>

{#snippet lanesToggle(track: TrackDoc)}
<button
  type="button"
  class="ms autos"
  class:on={!!track.show_automation}
  class:has={(track.envelopes?.length ?? 0) > 0}
  title={track.show_automation ? "Hide automation lanes" : (track.envelopes?.length ?? 0) > 0 ? `Show automation lanes (${track.envelopes!.length})` : "Show automation lanes: draw volume, pan or effect settings over time"}
  aria-pressed={!!track.show_automation}
  onclick={() => toggleLanes(track)}
>
  <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M1.5 11.5L5 6l4 3.5 5.5-6" fill="none" stroke="currentColor" stroke-width="1.4" /><circle cx="5" cy="6" r="1.4" fill="currentColor" /><circle cx="9" cy="9.5" r="1.4" fill="currentColor" /></svg>
</button>
{/snippet}

<style>
  .instrument {
    display: flex;
    flex: 1;
    min-width: 0;
    height: 20px;
    border: 1px solid var(--border);
    border-radius: 2px;
    background: var(--surface-2);
  }

  .instrument button {
    height: 100%;
    padding: 0 4px;
    border: 0;
    border-radius: 0;
    background: none;
    color: var(--text);
    font: inherit;
    font-size: var(--font-size-small);
  }

  .instrument button:hover {
    background: var(--surface-3, var(--surface-1));
  }

  .instrument .inst-name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-align: left;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .instrument .chev {
    flex: none;
    border-left: 1px solid var(--border);
  }

  .headers {
    flex: none;
    border-right: 3px solid var(--track-gap);
    background: var(--track-gap);
  }

  .header {
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: 3px 24px 3px 14px;
    border-left: 0;
    border-bottom: 3px solid var(--track-gap);
    box-sizing: border-box;
    position: relative;
    overflow: hidden;
    background: var(--header-a);
    cursor: grab;
    user-select: none;
  }

  .resize {
    position: absolute;
    right: 0;
    bottom: 0;
    left: 0;
    z-index: 2;
    height: 6px;
    cursor: ns-resize;
  }

  .resize:hover {
    background: color-mix(in srgb, var(--accent) 45%, transparent);
  }

  .header.alt {
    background: var(--header-b);
  }

  .header.selected {
    background: var(--lane-selected);
  }

  /* The row whose effects the Audio FX chain shows. */
  .header.fx::after {
    content: "";
    position: absolute;
    inset: 0;
    border: 1px solid var(--accent);
    border-left-width: 3px;
    pointer-events: none;
  }

  /* On the grab strip, written sideways, so it never covers a control. */
  .fx-tag {
    position: absolute;
    bottom: 3px;
    left: 0;
    z-index: 2;
    width: 10px;
    padding: 2px 0;
    background: var(--accent);
    color: #fff;
    font-size: 8px;
    font-weight: 700;
    line-height: 10px;
    text-align: center;
    writing-mode: vertical-rl;
    transform: rotate(180deg);
    pointer-events: none;
  }

  .header.selected .name {
    color: var(--selected-text, #fff);
    font-weight: 600;
  }

  .line {
    display: flex;
    gap: 3px;
    align-items: center;
    min-width: 0;
  }

  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .rename {
    flex: 1;
    min-width: 0;
    height: 18px;
    padding: 0 4px;
    border: 1px solid var(--accent);
    background: var(--surface-0);
    color: var(--text);
    font: inherit;
  }


  .ms {
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

  .ms.on {
    background: var(--accent-dim);
    color: #fff;
  }

  .ms.autos {
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }

  .ms.autos svg {
    width: 13px;
    height: 13px;
  }

  .ms.autos.has:not(.on) {
    color: var(--accent);
  }

  .ms.wide {
    width: auto;
    padding: 0 6px;
  }

  .lane-edge {
    position: absolute;
    right: 0;
    bottom: -3px;
    left: 0;
    z-index: 2;
    height: 6px;
    cursor: ns-resize;
  }
  .lane-edge:hover {
    background: color-mix(in srgb, var(--accent) 45%, transparent);
  }
  .header.auto-head {
    position: relative;
    justify-content: center;
    padding-left: 22px;
    background: color-mix(in srgb, var(--header-a) 88%, var(--track-color) 12%);
    cursor: default;
  }

  .auto-pick {
    flex: 1;
    min-width: 0;
    font-size: var(--font-size-small);
  }

  .auto-count {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    color: var(--text-faint);
    font-size: var(--font-size-small);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .ms.space3d {
    width: 30px;
    font-size: 10px;
    font-weight: 700;
    color: var(--accent);
  }

  .ms.solo.on {
    background: var(--clip-5);
    color: #111;
  }

  .ms.rec.on {
    background: var(--danger);
    color: #fff;
  }

  .ms svg {
    width: 12px;
    height: 12px;
    vertical-align: middle;
  }

  .ms.mon.on {
    background: var(--clip-1);
    color: #fff;
  }

  .ms.pol {
    margin-left: auto;
  }

  .ms.pol.on {
    background: var(--clip-4);
    color: #fff;
  }

  .fold {
    display: grid;
    flex: none;
    place-items: center;
    width: 14px;
    height: 16px;
    padding: 0;
    border: 0;
    background: none;
    color: var(--text-dim);
  }

  .fold svg {
    width: 10px;
    height: 10px;
    transition: transform 0.1s;
  }

  .fold.folded svg {
    transform: rotate(-90deg);
  }

  .fold:hover {
    color: var(--text);
  }

  .pick,
  .auto {
    flex: 1;
    min-width: 0;
    height: 18px;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    background: var(--surface-1);
    color: var(--text);
    font: inherit;
    font-size: var(--font-size-small);
  }

  .pick {
    display: flex;
    gap: 4px;
    align-items: center;
    padding: 0 4px;
    text-align: left;
  }

  .pick:hover {
    border-color: var(--accent);
  }

  .pick svg {
    flex: none;
    width: 12px;
    height: 12px;
    color: var(--text-dim);
  }

  .pick span:not(.chev) {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .chev {
    color: var(--text-dim);
  }

  .auto {
    height: 20px;
    padding: 0 2px;
    line-height: 18px;
  }

  .auto:disabled {
    color: var(--text-dim);
    opacity: 0.7;
  }

  /* A slim bar in the track's color. Pointing at it widens it and shows
     grip dots, so it reads as the place to drag the track from. The hit
     area stays 10 px wide either way. */
  .grab {
    position: absolute;
    top: 0;
    bottom: 0;
    left: 0;
    z-index: 1;
    width: 10px;
    cursor: grab;
  }

  .grab::before {
    content: "";
    position: absolute;
    inset: 0 auto 0 0;
    width: 4px;
    background: var(--track-color, var(--border-strong));
    transition: width 0.12s ease;
  }

  .grab::after {
    content: "";
    position: absolute;
    top: 50%;
    left: 3px;
    width: 4px;
    height: 22px;
    transform: translateY(-50%);
    background-image: radial-gradient(circle, rgba(0, 0, 0, 0.5) 1.1px, transparent 1.4px);
    background-size: 4px 5.5px;
    opacity: 0;
    transition: opacity 0.12s ease;
  }

  .grab:hover::before,
  .grab:active::before {
    width: 10px;
  }

  .grab:hover::after,
  .grab:active::after {
    opacity: 1;
  }

  .grab:active {
    cursor: grabbing;
  }

  .header.master {
    border-left: 10px solid var(--accent);
    padding-left: 4px;
    background: color-mix(in srgb, var(--header-a) 80%, var(--accent) 20%);
    cursor: default;
  }

  .ms.mono {
    width: auto;
    padding: 0 3px;
    font-size: 8px;
    letter-spacing: 0.03em;
  }

  .ms.mono.short {
    width: 20px;
    padding: 0;
  }

  .ms.mono.check.on {
    background: var(--clip-5);
    color: #111;
  }

  .ms.fx {
    font-style: italic;
  }

  .header.bus {
    background: color-mix(in srgb, var(--header-a) 82%, var(--clip-3) 18%);
    cursor: default;
  }

  .bus-tag {
    flex: none;
    padding: 0 3px;
    border-radius: 2px;
    background: var(--surface-3);
    color: var(--text-dim);
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.04em;
  }

  .outpick {
    position: relative;
  }
  .outpick select {
    flex: 1;
    min-width: 0;
    height: 16px;
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--text);
    font: inherit;
  }

  .sendview {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }
</style>
