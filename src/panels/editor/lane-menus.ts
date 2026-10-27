// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Right-click menus of the Multitrack lanes and ruler: empty lane space,
 * clips (with their envelopes and fade handles), a clip's envelope chip,
 * and the ruler. Each builder only makes the items; the editor shows them.
 */

import type { ContextItem } from "../../app/ContextMenu.svelte";
import { clipReversePrepare } from "../../app/core";
import { editor, type EditorState } from "../../stores/editor";
import type { ClipDoc, MarkerDoc, Op, SessionDoc, TempoDoc } from "../../stores/session";
import { addChange, nearestBar, secondsToBeats } from "../../stores/tempo";
import { mixOf, toSpeakers } from "../properties/channel-mix";
import { envChange, envName, fxChoices, fxLaneOf, pointsOf } from "./envelope";
import type { Zone } from "./lane-hit";
import { timeDisplayItems } from "./time-menu";

/** What the menu builders read and run. Getters, so a menu entry sees the session as it is when picked. */
export interface MenuCtx {
  doc: () => SessionDoc | null;
  ed: () => EditorState;
  /** One undoable edit. */
  edit: (label: string, op: Op) => Promise<void>;
  onError: (e: unknown) => void;
  run: (id: string) => void;
  can: (id: string) => boolean;
  has: (id: string) => boolean;
  reason: (id: string) => string | null;
  isChecked: (id: string) => boolean | undefined;
  shortcutFor: (id: string) => string | undefined;
}

/** The right-click menu for empty space in a lane. Every entry runs an app action, so it stays in step with the menu bar. */
export function emptyLaneItems(ctx: MenuCtx): ContextItem[] {
  const { can, run, shortcutFor } = ctx;
  const act = (label: string, id: string): ContextItem => {
    const ok = can(id);
    return {
      kind: "item",
      label,
      shortcut: shortcutFor(id),
      disabled: !ok,
      reason: ok ? null : ctx.has(id) ? ctx.reason(id) : "Not built yet.",
      run: () => run(id),
    };
  };
  const off = (label: string, reason: string): ContextItem => ({ kind: "item", label, disabled: true, reason, run: () => {} });
  const sub = (label: string, items: ContextItem[], reason?: string): ContextItem => ({ kind: "sub", label, items, ...(reason ? { disabled: true, reason } : {}) });
  const sep: ContextItem = { kind: "sep" };
  return [
    act("Paste", "edit.paste"),
    sep,
    sub("Ripple Delete", [
      act("Selected Clips", "edit.ripple.clips"),
      act("Time Selection in Selected Clips", "edit.ripple.time-clips"),
      act("Time Selection in Selected Track", "edit.ripple.time-track"),
      act("Time Selection in All Tracks", "edit.ripple.time-all"),
      act("Gap Between Clips", "edit.ripple.gap"),
    ]),
    sub("Select", [
      act("Select All", "edit.select-all"),
      act("Deselect All", "edit.deselect"),
      sep,
      act("All Clips in Track", "edit.select-track-clips"),
      act("Next Clip in Selected Track", "edit.select-next-clip"),
      act("Clips to End of Track", "edit.select-clips-track-end"),
      sep,
      act("Clips to Start of Session", "edit.select-clips-session-start"),
      act("Clips to End of Session", "edit.select-clips-session-end"),
      sub("Clips of Audio Type", [
        act("Dialogue", "edit.select-type.dialogue"),
        act("Music", "edit.select-type.music"),
        act("Sound Effects", "edit.select-type.sfx"),
        act("Ambience", "edit.select-type.ambience"),
      ]),
      sep,
      act("Select Current View Time", "edit.select-view-time"),
      act("Select All Time", "edit.select-all-time"),
      act("Select Time Around Selected Clips", "edit.select-time-around-clips"),
      act("Clear Time Selection", "edit.clear-time-selection"),
    ]),
    sub("Groups", [
      act("Group Clips", "clip.group"),
      act("Suspend Groups", "clip.group-suspend"),
      act("Remove Focus Clip from Group", "clip.group-remove-focus"),
      act("Ungroup Selected Clips", "clip.ungroup"),
    ]),
    sep,
    sub("Track", [
      act("Add Mono Audio Track", "multitrack.add.mono"),
      act("Add Stereo Audio Track", "multitrack.add.stereo"),
      act("Add MIDI Track", "multitrack.add.midi"),
      act("Add Tracks...", "multitrack.add-tracks"),
      off("Add 5.1 Audio Track", "Surround tracks are not planned."),
      sep,
      act("Add Mono Bus Track", "multitrack.add.bus-mono"),
      act("Add Stereo Bus Track", "multitrack.add.bus"),
      off("Add 5.1 Bus Track", "Surround tracks are not planned."),
      sep,
      act("Delete Empty Tracks", "multitrack.delete-empty"),
      sep,
      act("Delete Selected Track", "multitrack.delete-track"),
      act("Duplicate Selected Track", "multitrack.duplicate-track"),
    ]),
    sub("Track Visibility", [
      act("Hide Selected Track", "multitrack.vis.hide-selected"),
      act("Show All Tracks", "multitrack.vis.show-all"),
      sep,
      act("Show Only Tracks with Clips", "multitrack.vis.only-clips"),
      act("Show Only Tracks with Clips within Time Selection", "multitrack.vis.only-selection"),
      act("Show Only Tracks with Clips at Playhead", "multitrack.vis.only-playhead"),
      sep,
      off("Show Only Bus Tracks", "Not built yet: bus tracks always show below the audio tracks."),
      off("Load and Save Visibility Presets", "Not built yet."),
    ]),
    sub("Insert", [
      act("Files at Playhead...", "multitrack.insert-files"),
      act("Files as Tracks...", "multitrack.insert-tracks"),
      act("Selected Files as Tracks", "multitrack.insert-selected-tracks"),
      sep,
      act("Silence...", "multitrack.insert-silence"),
    ]),
    sub("Marker", [
      act("Add Cue Marker", "edit.marker.cue"),
      act("Add Subclip Marker", "edit.marker.subclip"),
      act("Add CD Track Marker", "edit.marker.cd-track"),
      act("Add Cart Timer Marker", "edit.marker.cart"),
      act("Add Range Marker", "edit.marker.range"),
      act("Add Chapter Marker", "edit.marker.chapter"),
      act("Add Ad Slot", "edit.marker.ad-slot"),
      act("Add Social Clip", "edit.marker.social"),
    ]),
    sep,
    sub("Mixdown Session to New File", [
      act("Entire Session", "multitrack.mixdown.all"),
      act("Time Selection", "multitrack.mixdown.selection"),
      act("Selected Clips", "multitrack.mixdown.clips"),
    ]),
    sub("Export Mixdown", [act("Entire Session...", "file.export.mixdown"), act("Time Selection...", "file.export.selection")]),
    sub("Bounce to New Track", [
      act("Selected Track", "multitrack.bounce.track"),
      act("Time Selection", "multitrack.bounce.selection"),
      act("Selected Clips in Time Selection", "multitrack.bounce.clips-selection"),
      act("Selected Clips Only", "multitrack.bounce.clips"),
    ]),
    sep,
    act("Session Properties...", "multitrack.properties"),
  ];
}

/** Fade shapes for a fade handle's menu: straight or curved, each bent by dragging the handle up or down. */
const FADE_SHAPES = [
  ["linear", "Straight"],
  ["cosine", "Curve"],
] as const;

/**
 * The right-click menu for a clip: its envelope point or line, its fade
 * handle (that menu stands alone), then the selected clips' commands.
 * The track's own entries go after these, except under a fade handle.
 */
export function clipZoneItems(ctx: MenuCtx, z: Zone): ContextItem[] {
  const { can, run, shortcutFor, onError } = ctx;
  const ed = ctx.ed();
  const items: ContextItem[] = [];
  if (z.kind === "env-point" || z.kind === "env-line") {
    const clip = z.clip;
    const env = z.env ?? "volume";
    const fxLane = env === "fx" ? fxLaneOf(clip, ed.fxLanes) : null;
    const name = envName(env, clip, fxLane);
    const pts = pointsOf(clip, env, fxLane);
    const setPts = (label: string, next: typeof pts) => void ctx.edit(label, { op: "set_clips", changes: [envChange(clip, env, next, fxLane)] }).catch(onError);
    if (z.kind === "env-point" && z.index !== undefined) {
      const idx = z.index;
      items.push({ kind: "item", label: `Delete ${name} Point`, run: () => setPts(`Delete ${name} Point`, pts.filter((_, k) => k !== idx)) });
    }
    items.push({ kind: "item", label: `Reset ${name} Envelope`, disabled: pts.length === 0, run: () => setPts(`Reset ${name} Envelope`, []) }, { kind: "sep" });
  }
  if (z.kind === "fade-in" || z.kind === "fade-out") {
    const which = z.kind === "fade-in" ? "fade_in" : "fade_out";
    const word = z.kind === "fade-in" ? "Fade In" : "Fade Out";
    // The clip under the pointer, and the other selected clips with it.
    const ids = ed.selectedClips.has(z.clip.id) ? [...ed.selectedClips] : [z.clip.id];
    const clips = (ctx.doc()?.tracks ?? []).flatMap((t) => t.clips).filter((c) => ids.includes(c.id));
    const current = z.clip[which].shape;
    const setShape = (shape: string, label: string) =>
      void ctx.edit(`${word} Shape: ${label}`, { op: "set_clips", changes: clips.map((c) => ({ id: c.id, [which]: { ...c[which], shape } })) }).catch(onError);
    for (const [shape, label] of FADE_SHAPES) items.push({ kind: "item", label, checked: current === shape || (shape === "linear" && current !== "cosine"), run: () => setShape(shape, label) });
    const bent = clips.some((c) => (c[which].curve ?? 0) !== 0);
    items.push({ kind: "item", label: `Skew: ${Math.round((z.clip[which].curve ?? 0) * 100)} (drag the handle up or down)`, disabled: true, run: () => {} });
    items.push({ kind: "item", label: "Reset Skew", disabled: !bent, run: () => void ctx.edit(`${word} Skew`, { op: "set_clips", changes: clips.map((c) => ({ id: c.id, [which]: { ...c[which], curve: 0 } })) }).catch(onError) });
    items.push(
      { kind: "sep" },
      { kind: "item", label: `Remove ${word}`, disabled: clips.every((c) => c[which].length === 0), run: () => void ctx.edit(`Remove ${word}`, { op: "set_clips", changes: clips.map((c) => ({ id: c.id, [which]: { ...c[which], length: 0 } })) }).catch(onError) },
    );
    return items;
  }
  if (ed.selectedClips.size > 0) {
    const act = (label: string, id: string): ContextItem => ({ kind: "item", label, shortcut: shortcutFor(id), disabled: !can(id), run: () => run(id) });
    items.push(
      act("Split at Playhead", "clip.split"),
      act("Trim Start to Playhead", "clip.trim.start"),
      act("Trim End to Playhead", "clip.trim.end"),
      { kind: "sep" },
      act(ctx.isChecked("clip.mute") ? "Unmute Clip" : "Mute Clip", "clip.mute"),
      act(ctx.isChecked("clip.lock") ? "Unlock Position" : "Lock Position", "clip.lock"),
      act("Clip Gain...", "clip.gain"),
      act("Clip Color...", "clip.color"),
      ...clipFlipItems(ctx),
      act("Group Clips", "clip.group"),
      act("Ungroup Clips", "clip.ungroup"),
      {
        kind: "sub",
        label: "Audio Type",
        items: [
          { ...(act("Dialogue", "clip.type.dialogue") as Extract<ContextItem, { kind: "item" }>), checked: ctx.isChecked("clip.type.dialogue") === true },
          { ...(act("Music", "clip.type.music") as Extract<ContextItem, { kind: "item" }>), checked: ctx.isChecked("clip.type.music") === true },
          { ...(act("Sound Effects", "clip.type.sfx") as Extract<ContextItem, { kind: "item" }>), checked: ctx.isChecked("clip.type.sfx") === true },
          { ...(act("Ambience", "clip.type.ambience") as Extract<ContextItem, { kind: "item" }>), checked: ctx.isChecked("clip.type.ambience") === true },
          { kind: "sep" },
          act("None", "clip.type.none"),
        ],
      },
      act("Time Stretch...", "clip.stretch"),
      act("Stretch and Pitch...", "clip.properties"),
      act(ctx.isChecked("clip.fade-in") ? "Remove Fade In" : "Fade In", "clip.fade-in"),
      act(ctx.isChecked("clip.fade-out") ? "Remove Fade Out" : "Fade Out", "clip.fade-out"),
      { kind: "sep" },
      { kind: "item", label: ed.selectedClips.size > 1 ? `Delete ${ed.selectedClips.size} Clips` : "Delete Clip", shortcut: shortcutFor("edit.delete"), run: () => run("edit.delete") },
      { kind: "item", label: "Show in Files Panel", disabled: !can("clip.reveal"), run: () => run("clip.reveal") },
      { kind: "sep" },
    );
  }
  return items;
}

/** Reverse and Invert for the selected clips: toggles that leave the files alone. */
function clipFlipItems(ctx: MenuCtx): ContextItem[] {
  const ed = ctx.ed();
  const found = (ctx.doc()?.tracks ?? []).flatMap((t) => t.clips).filter((c) => ed.selectedClips.has(c.id));
  if (!found.length) return [];
  const allRev = found.every((c) => c.reversed);
  const allInv = found.every((c) => c.inverted);
  const reverse = async () => {
    const on = !allRev;
    if (on) {
      const paths = [...new Set(found.map((c) => ctx.doc()?.media.find((m) => m.id === c.media_id)?.path).filter((p): p is string => !!p))];
      await Promise.all(paths.map((p) => clipReversePrepare(p)));
    }
    // The region flips with it: positions count from the file's end while reversed.
    const flip = (c: ClipDoc) => {
      const len = ctx.doc()?.media.find((m) => m.id === c.media_id)?.length ?? c.source_out;
      return { source_in: Math.max(0, len - c.source_out), source_out: Math.max(0, len - c.source_in) };
    };
    await ctx.edit(on ? "Reverse Clip" : "Play Clip Forwards", { op: "set_clips", changes: found.filter((c) => !!c.reversed !== on).map((c) => ({ id: c.id, reversed: on, ...flip(c) })) });
  };
  return [
    { kind: "sep" },
    { kind: "item", label: "Reverse", checked: allRev, run: () => void reverse().catch(ctx.onError) },
    ...invertItems(ctx, found, allInv),
    ...passThroughItem(ctx, found),
    { kind: "sep" },
  ];
}

/**
 * Invert Polarity. Stereo clips choose both channels, the left only or
 * the right only (folded to mono, one flipped side cancels what the two
 * share, leaving the difference); mono and multichannel clips flip all.
 */
function invertItems(ctx: MenuCtx, found: ClipDoc[], allInv: boolean): ContextItem[] {
  const chans = (c: ClipDoc) => ctx.doc()?.media.find((m) => m.id === c.media_id)?.channels ?? 2;
  const edit = (label: string, change: (c: ClipDoc) => { inverted: boolean; invert_channel?: "left" | "right" | "both" }) =>
    void ctx.edit(label, { op: "set_clips", changes: found.map((c) => ({ id: c.id, ...change(c) })) }).catch(ctx.onError);
  if (!found.every((c) => chans(c) === 2)) {
    return [{ kind: "item", label: "Invert Polarity", checked: allInv, run: () => edit(allInv ? "Polarity Back" : "Invert Polarity", () => ({ inverted: !allInv, invert_channel: "both" })) }];
  }
  const sideOf = (c: ClipDoc) => (c.inverted ? (c.invert_channel ?? "both") : null);
  const all = (side: "both" | "left" | "right") => found.every((c) => sideOf(c) === side);
  const pick = (side: "both" | "left" | "right", label: string): ContextItem => ({
    kind: "item",
    label,
    checked: all(side),
    run: () => (all(side) ? edit("Polarity Back", () => ({ inverted: false, invert_channel: "both" })) : edit(`Invert Polarity (${label})`, () => ({ inverted: true, invert_channel: side }))),
  });
  return [
    {
      kind: "sub",
      label: "Invert Polarity",
      items: [pick("both", "Both Channels"), pick("left", "Left Only"), pick("right", "Right Only")],
    },
  ];
}

/**
 * 3D Space sessions: Pass Through to Speakers for multichannel clips.
 * Each channel then plays from the speaker of its own name (L to L, C to
 * C...); a channel the layout lacks folds into the speakers nearest
 * where it would stand. Off, the clip is placed in the room like any
 * other, from its left and right.
 */
function passThroughItem(ctx: MenuCtx, found: ClipDoc[]): ContextItem[] {
  if (!ctx.doc()?.space) return [];
  const chans = (c: ClipDoc) => ctx.doc()?.media.find((m) => m.id === c.media_id)?.channels ?? 2;
  const multi = found.filter((c) => chans(c) > 1);
  if (!multi.length) return [];
  const allOn = multi.every((c) => toSpeakers(c.channel_mix, chans(c)));
  const changes = multi.map((c) => ({ id: c.id, channel_mix: { ...mixOf(c.channel_mix, chans(c)), to_speakers: !allOn } }));
  return [{ kind: "item", label: "Pass Through to Speakers", checked: allOn, run: () => void ctx.edit(allOn ? "Place Clip in the Room" : "Pass Through to Speakers", { op: "set_clips", changes }).catch(ctx.onError) }];
}

/** The menu on a clip's envelope chip: which envelopes show, and which effect setting. */
export function chipMenuItems(ctx: MenuCtx, clip: ClipDoc): ContextItem[] {
  const { run } = ctx;
  const ed = ctx.ed();
  const current = fxLaneOf(clip, ed.fxLanes);
  const choices = fxChoices(clip);
  const fx: ContextItem[] = choices.length
    ? [
        { kind: "item", label: "Hide Effect Envelopes", checked: !ed.showFx, run: () => editor.setShowFx(false) },
        { kind: "sep" },
        ...choices.map(
          (c): ContextItem => ({
            kind: "item",
            label: c.label,
            checked: ed.showFx && !!current && current.slot === c.lane.slot && current.param === c.lane.param,
            run: () => editor.pickFxLane(clip.id, c.lane),
          }),
        ),
      ]
    : [{ kind: "item", label: "Add an effect to this clip's Audio FX chain first", disabled: true, run: () => {} }];
  return [
    { kind: "item", label: "Volume", checked: ed.showVolume, run: () => run("view.env.volume") },
    { kind: "item", label: "Pan", checked: ed.showPan, run: () => run("view.env.pan") },
    { kind: "sub", label: "Effect Setting", items: fx },
    { kind: "sep" },
    { kind: "item", label: "Add/Remove Effects", run: () => run("window.panel.voice-chain") },
    { kind: "sep" },
    { kind: "item", label: "Chain Input Gain", disabled: true, reason: "Chain-level envelopes are not built yet", run: () => {} },
    { kind: "item", label: "Chain Output Gain", disabled: true, reason: "Chain-level envelopes are not built yet", run: () => {} },
    { kind: "item", label: "Chain Mix", disabled: true, reason: "Chain-level envelopes are not built yet", run: () => {} },
    { kind: "item", label: "Chain Power", disabled: true, reason: "Chain-level envelopes are not built yet", run: () => {} },
  ];
}

export interface RulerMenuExtra {
  seek: (sample: number) => Promise<void>;
  prompt: (title: string, label: string, value: string) => Promise<string | null>;
  tempo: () => TempoDoc;
  rate: () => number;
  setTempoMap: (t: TempoDoc, label: string) => void;
}

/** The ruler's right-click menu: a marker's entries when on one, otherwise adding markers and how times show. */
export function rulerMenuItems(ctx: MenuCtx, hit: MarkerDoc | null, at: number, x: RulerMenuExtra): ContextItem[] {
  const { run, can, onError } = ctx;
  return hit
    ? [
        { kind: "item", label: "Go to Marker", run: () => void x.seek(hit.position) },
        {
          kind: "item",
          label: "Rename Marker...",
          run: async () => {
            const name = await x.prompt("Rename Marker", "Name", hit.name);
            if (name && name !== hit.name) await ctx.edit("Rename Marker", { op: "set_markers", changes: [{ id: hit.id, name }] }).catch(onError);
          },
        },
        { kind: "item", label: "Delete Marker", run: () => void ctx.edit("Delete Marker", { op: "remove_markers", ids: [hit.id] }).catch(onError) },
      ]
    : [
        {
          kind: "item",
          label: "Add Cue Marker Here",
          run: () => {
            void x.seek(at).then(() => run("edit.marker.cue"));
          },
        },
        { kind: "item", label: "Add Range Marker", disabled: !can("edit.marker.range"), run: () => run("edit.marker.range") },
        {
          kind: "item",
          label: "Add Tempo Change at Nearest Bar",
          run: () => {
            const tempo = x.tempo();
            const bar = Math.max(1, nearestBar(tempo, secondsToBeats(tempo, at / x.rate())));
            const r = addChange(tempo, bar, crypto.randomUUID());
            if (r.tempo !== tempo) x.setTempoMap(r.tempo, "Add Tempo Change");
          },
        },
        { kind: "sep" },
        ...timeDisplayItems(),
      ];
}
