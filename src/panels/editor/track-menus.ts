// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Right-click and picker menus for Multitrack rows: tracks (with their
 * output and sends), buses, automation lanes, MIDI clips, a track's input
 * and a MIDI track's instrument. Each builder only makes the items.
 */

import type { ContextItem } from "../../app/ContextMenu.svelte";
import type { AudioInput, EffectInfo } from "../../app/core";
import { editor } from "../../stores/editor";
import type { BusDoc, EnvelopeDoc, MidiClipDoc, Op, TempoDoc, TrackChanges, TrackDoc } from "../../stores/session";
import { busChoices, newSend } from "../mixer/routing";
import { pluginMenuGroups, type MenuPlugin } from "../voice-chain/chain";
import { autoAt, autoName, autoRest, dropLane, nextTarget, putLane } from "./automation";
import { withGroups } from "./clip-edit";
import { ROW_SENDS } from "./editor-layout";
import type { MenuCtx } from "./lane-menus";
import { duplicateClips, newClip, replaceClip } from "./midi";

/** A row whose color is being picked. */
export type ColorTarget = { kind: "track" | "bus"; id: string; current: string | null };

export interface TrackMenuExtra {
  rename: (id: string) => void;
  pickColor: (target: ColorTarget) => void;
  toggleLanes: (track: TrackDoc) => void;
  setTrack: (track: TrackDoc, changes: TrackChanges, label: string) => void;
  /** Switches the headers to the mix view, where sends show. */
  showMix: () => void;
}

export function trackItems(ctx: MenuCtx, track: TrackDoc, x: TrackMenuExtra): ContextItem[] {
  const { run } = ctx;
  return [
    { kind: "item", label: "Select All Clips on Track", disabled: track.clips.length === 0, run: () => editor.selectClips(withGroups(ctx.doc()!, track.clips.map((c) => c.id)), track.id) },
    { kind: "sep" },
    { kind: "item", label: "Rename Track", run: () => x.rename(track.id) },
    { kind: "item", label: track.show_automation ? "Hide Automation Lanes" : "Show Automation Lanes", run: () => x.toggleLanes(track) },
    { kind: "item", label: "Track Color...", run: () => x.pickColor({ kind: "track", id: track.id, current: track.color ?? null }) },
    { kind: "item", label: "Duplicate Track", run: () => { editor.selectTrack(track.id); run("multitrack.duplicate-track"); } },
    { kind: "item", label: "Bounce Track to New Track", disabled: track.clips.length === 0 && (track.midi_clips ?? []).length === 0, run: () => { editor.selectTrack(track.id); run("multitrack.bounce.track"); } },
    { kind: "item", label: "Delete Track", run: () => { editor.selectTrack(track.id); run("multitrack.delete-track"); } },
    { kind: "sep" },
    { kind: "item", label: "Add Mono Track", run: () => { editor.selectTrack(track.id); run("multitrack.add.mono"); } },
    { kind: "item", label: "Add Stereo Track", run: () => { editor.selectTrack(track.id); run("multitrack.add.stereo"); } },
    { kind: "item", label: "Add MIDI Track", run: () => { editor.selectTrack(track.id); run("multitrack.add.midi"); } },
    { kind: "item", label: "Add Bus Track", run: () => run("multitrack.add.bus") },
    { kind: "sep" },
    ...routingItems(ctx, track, x),
  ];
}

/** Output and send choices for a track's right-click menu. */
function routingItems(ctx: MenuCtx, track: TrackDoc, x: TrackMenuExtra): ContextItem[] {
  const doc = ctx.doc();
  if (!doc) return [];
  const choices = busChoices(doc, track.id);
  const sends = track.sends ?? [];
  const out = track.output ?? { type: "master" as const };
  const none = (doc.buses?.length ?? 0) === 0 ? "Add a bus first: Multitrack > Add > Bus Track." : "No bus can take this track without feeding back into it.";
  return [
    {
      kind: "sub",
      label: "Output To",
      items: [
        { kind: "item", label: "Master", checked: out.type === "master", run: () => x.setTrack(track, { output: { type: "master" } }, "Route Track Output") },
        ...choices.map((b): ContextItem => ({ kind: "item", label: b.name, checked: out.type === "bus" && out.bus_id === b.id, run: () => x.setTrack(track, { output: { type: "bus", bus_id: b.id } }, "Route Track Output") })),
      ],
    },
    {
      kind: "sub",
      label: "Add Send To",
      disabled: choices.length === 0,
      reason: choices.length === 0 ? none : null,
      items: choices.map((b): ContextItem => ({
        kind: "item",
        label: b.name,
        run: () => {
          // Tall enough to show the send in the header.
          const ops: Op[] = [{ op: "set_sends", owner: track.id, sends: [...sends, newSend(b.id, crypto.randomUUID())] }];
          if (!track.collapsed && track.height < ROW_SENDS + 24) ops.push({ op: "set_track", id: track.id, changes: { height: ROW_SENDS + 24 } } as Op);
          void ctx.edit("Add Send", ops.length === 1 ? ops[0]! : { op: "batch", ops }).catch(ctx.onError);
          x.showMix();
        },
      })),
    },
  ];
}

export function busItems(ctx: MenuCtx, b: BusDoc, x: Pick<TrackMenuExtra, "rename" | "pickColor">): ContextItem[] {
  return [
    { kind: "item", label: "Rename Bus", run: () => x.rename(b.id) },
    { kind: "item", label: "Bus Color...", run: () => x.pickColor({ kind: "bus", id: b.id, current: b.color }) },
    { kind: "item", label: "Delete Bus", run: () => void ctx.edit("Delete Bus", { op: "remove_bus", id: b.id }).catch(ctx.onError) },
    { kind: "sep" },
    { kind: "item", label: "Add Bus Track", run: () => ctx.run("multitrack.add.bus") },
  ];
}

export interface LaneMenuExtra {
  fxCatalog: () => EffectInfo[];
  playhead: () => number;
  setLanes: (track: TrackDoc, envelopes: EnvelopeDoc[], label: string) => void;
  addLane: (track: TrackDoc) => void;
  toggleLanes: (track: TrackDoc) => void;
}

/** The right-click menu of an automation lane; `index` is the point under the pointer, or -1. */
export function autoLaneItems(ctx: MenuCtx, track: TrackDoc, lane: EnvelopeDoc, index: number, x: LaneMenuExtra): ContextItem[] {
  const { setLanes } = x;
  const name = autoName(track, x.fxCatalog(), lane.target);
  const at = (v: number) => putLane(track, { ...lane, points: lane.points.filter((_, k) => k !== v) });
  const doc = ctx.doc;
  return [
    { kind: "item", label: "Delete Point", disabled: index < 0, run: () => setLanes(track, at(index), "Delete Automation Point") },
    {
      kind: "item",
      label: "Add Point at Playhead",
      run: () => {
        const playhead = x.playhead();
        const fxCatalog = x.fxCatalog();
        const v = lane.points.length ? autoAt(lane.points, playhead, autoRest(track, fxCatalog, lane.target)) : autoRest(track, fxCatalog, lane.target);
        const pts = lane.points.filter((p) => p.position !== Math.round(playhead)).concat({ position: Math.max(0, Math.round(playhead)), value: v, curve: "linear" as const });
        pts.sort((a, b) => a.position - b.position);
        setLanes(track, putLane(track, { ...lane, points: pts }), "Add Automation Point");
      },
    },
    {
      kind: "item",
      label: "Delete Points in Time Selection",
      disabled: !doc()?.view.selection || !lane.points.some((p) => p.position >= doc()!.view.selection!.start && p.position <= doc()!.view.selection!.end),
      run: () => {
        const sel = doc()?.view.selection;
        if (!sel) return;
        setLanes(track, putLane(track, { ...lane, points: lane.points.filter((p) => p.position < sel.start || p.position > sel.end) }), "Delete Automation Points");
      },
    },
    { kind: "item", label: `Clear ${name} Lane`, disabled: lane.points.length === 0, run: () => setLanes(track, putLane(track, { ...lane, points: [] }), "Clear Automation Lane") },
    { kind: "sep" },
    { kind: "item", label: lane.enabled ? "Turn Lane Off" : "Turn Lane On", disabled: !lane.id, run: () => setLanes(track, putLane(track, { ...lane, enabled: !lane.enabled }), lane.enabled ? "Automation Lane Off" : "Automation Lane On") },
    { kind: "item", label: "Add Lane", disabled: !nextTarget(track, x.fxCatalog(), !!doc()?.space), run: () => x.addLane(track) },
    { kind: "item", label: "Remove Lane", disabled: !lane.id, run: () => setLanes(track, dropLane(track, lane.id), "Remove Automation Lane") },
    { kind: "item", label: "Hide Automation Lanes", run: () => x.toggleLanes(track) },
  ];
}

export interface MidiMenuExtra {
  tempo: () => TempoDoc;
  setMidiClips: (track: TrackDoc, clips: MidiClipDoc[], label: string) => void;
  openPianoRoll: (trackId: string, clipId: string) => void;
  prompt: (title: string, label: string, value: string) => Promise<string | null>;
}

/** The right-click menu on a MIDI lane: a clip's entries, or a new clip on empty space. */
export function midiClipItems(ctx: MenuCtx, track: TrackDoc, clip: MidiClipDoc | null, beat: number, x: MidiMenuExtra): ContextItem[] {
  const { run, shortcutFor } = ctx;
  const { setMidiClips } = x;
  const clips = track.midi_clips ?? [];
  const bounceTrack: ContextItem = {
    kind: "item",
    label: "Bounce Track to New Track",
    disabled: clips.length === 0,
    run: () => {
      editor.selectTrack(track.id);
      run("multitrack.bounce.track");
    },
  };
  if (!clip) {
    return [
      {
        kind: "item",
        label: "New MIDI Clip Here",
        run: () => {
          const id = crypto.randomUUID();
          setMidiClips(track, [...clips, newClip(id, beat, x.tempo())].sort((a, b) => a.start - b.start), "New MIDI Clip");
          editor.selectClips([id], track.id);
        },
      },
      { kind: "sep" },
      bounceTrack,
    ];
  }
  const ed = ctx.ed();
  const ids = new Set(clips.filter((c) => ed.selectedClips.has(c.id)).map((c) => c.id));
  if (!ids.size) ids.add(clip.id);
  const many = ids.size > 1;
  return [
    { kind: "item", label: "Edit Notes...", run: () => x.openPianoRoll(track.id, clip.id) },
    { kind: "sep" },
    {
      kind: "item",
      label: many ? `Duplicate ${ids.size} Clips` : "Duplicate Clip",
      run: () => {
        const d = duplicateClips(clips, ids, () => crypto.randomUUID());
        setMidiClips(track, d.clips, many ? "Duplicate Clips" : "Duplicate Clip");
        editor.selectClips(d.added, track.id);
      },
    },
    {
      kind: "item",
      label: clip.muted ? "Unmute Clip" : "Mute Clip",
      run: () => setMidiClips(track, clips.map((c) => (ids.has(c.id) ? { ...c, muted: !clip.muted } : c)), clip.muted ? "Unmute Clip" : "Mute Clip"),
    },
    {
      kind: "item",
      label: "Rename Clip...",
      run: () =>
        void x.prompt("Rename Clip", "Name", clip.name).then((name) => {
          if (name && name.trim() && name.trim() !== clip.name) setMidiClips(track, replaceClip(clips, { ...clip, name: name.trim() }), "Rename Clip");
        }),
    },
    { kind: "sep" },
    {
      kind: "item",
      label: many ? `Delete ${ids.size} Clips` : "Delete Clip",
      shortcut: shortcutFor("edit.delete"),
      run: () => {
        setMidiClips(track, clips.filter((c) => !ids.has(c.id)), many ? "Delete Clips" : "Delete Clip");
        editor.selectClips([], track.id);
      },
    },
    { kind: "sep" },
    {
      kind: "item",
      label: many ? `Bounce ${ids.size} Clips to New Track` : "Bounce Clip to New Track",
      run: () => {
        editor.selectClips([...ids], track.id);
        run("multitrack.bounce.clips");
      },
    },
    bounceTrack,
  ];
}

/** Input picker: default, None, then Mono and Stereo submenus, each grouped by device. */
export function inputMenuItems(track: TrackDoc, inputs: AudioInput[], setInput: (value: string | null) => void, openHardware: () => void): ContextItem[] {
  const pick = (value: string | null, label: string): ContextItem => ({
    kind: "item",
    label,
    checked: (track.input ?? null) === value,
    run: () => setInput(value),
  });
  const items: ContextItem[] = [pick(null, track.channels === "mono" ? "Default Mono Input" : "Default Stereo Input"), pick("none", "None"), { kind: "sep" }];
  if (inputs.length === 0) items.push({ kind: "item", label: "No audio inputs found", run: () => {}, disabled: true });
  // Grouped as Mono and Stereo, then by device, so a system with many
  // devices stays readable. The stored value is unchanged.
  const mono: ContextItem[] = [];
  const stereo: ContextItem[] = [];
  for (const d of inputs) {
    const m: ContextItem[] = [];
    for (let c = 1; c <= d.channels; c++) m.push({ ...pick(`Mono: ${d.name} ${c}`, `Channel ${c}`) });
    if (m.length) mono.push({ kind: "sub", label: d.name, items: m });
    const st: ContextItem[] = [];
    for (let c = 1; c + 1 <= d.channels; c += 2) st.push(pick(`Stereo: ${d.name} ${c}-${c + 1}`, `Channels ${c}-${c + 1}`));
    if (st.length) stereo.push({ kind: "sub", label: d.name, items: st });
  }
  if (mono.length) items.push({ kind: "sub", label: "Mono", items: mono });
  if (stereo.length) items.push({ kind: "sub", label: "Stereo", items: stereo });
  items.push({ kind: "sep" }, { kind: "item", label: "Audio Hardware...", run: openHardware });
  return items;
}

/** The instrument picker: by type, then by maker, as in the Add effect menu. */
export function instrumentMenuItems(track: TrackDoc, instruments: MenuPlugin[], setInstrument: (effect: string) => void): ContextItem[] {
  const current = track.instrument?.effect ?? "";
  const pick = (effect: string) => () => setInstrument(effect);
  const list = (ps: MenuPlugin[]): ContextItem[] => ps.map((p) => ({ kind: "item", label: p.name, checked: p.effect === current, run: pick(p.effect) }));
  const { types, makers } = pluginMenuGroups(instruments);
  return instruments.length
    ? [
        ...types.map(([t, ps]): ContextItem => ({ kind: "sub", label: t, items: list(ps) })),
        { kind: "sep" },
        ...makers.map(([m, ps]): ContextItem => ({ kind: "sub", label: m, items: list(ps) })),
        { kind: "sep" },
        { kind: "item", label: "No Instrument", disabled: !track.instrument, run: pick("") },
      ]
    : [{ kind: "item", label: "No instruments found. Scan in Effects > Plug-in Manager.", disabled: true, run: () => {} }];
}
