// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Multitrack view state that is not part of the session document:
 * which track and clips are selected, and how times are shown.
 * Selection is not an edit, so it is not undoable.
 */

import { writable, type Readable } from "svelte/store";

/** How times show: minutes and seconds, plain seconds, bars and beats (at the session tempo), samples or timecode. */
export type TimeFormat = "time" | "seconds" | "bars" | "samples" | "timecode";

/** What clip drags and time selections snap to. */
export interface SnapSettings {
  on: boolean;
  clips: boolean;
  playhead: boolean;
  markers: boolean;
  ruler: boolean;
}

export interface EditorState {
  selectedTrack: string | null;
  selectedClips: ReadonlySet<string>;
  timeFormat: TimeFormat;
  /** Width of the lanes in pixels, for zoom commands. */
  width: number;
  snap: SnapSettings;
  /** Overlapping clips on a track crossfade when moved or trimmed into each other. */
  autoCrossfade: boolean;
  /** Clip volume envelopes are shown and editable. */
  showVolume: boolean;
  /** Clip pan envelopes are shown and editable. */
  showPan: boolean;
  /** Clip effect-setting envelopes (Audio FX automation) are shown and editable. */
  showFx: boolean;
  /** Which effect setting each clip shows, by clip id. Not saved. */
  fxLanes: Record<string, { slot: string; param: string }>;
  /** Which editor the Editor panel shows. */
  mode: EditorMode;
  /** What a drag or click in the multitrack does. */
  tool: EditorTool;
  /** How playback treats the time selection: loop it, skip it, or neither. */
  playMode: PlayMode;
  /** The lanes scroll to keep the playhead in view during playback. */
  followPlayhead: boolean;
  /** The Waveform editor shows the spectrum under the waveform. */
  showSpectrum: boolean;
  /** The Waveform editor draws the pitch line over the spectrum. */
  showPitch: boolean;
  /** What a drag does in the Waveform editor's spectrum. */
  spectralTool: SpectralTool;
  /** Brush Select and Spot Repair size, in pixels. */
  brushSize: number;
  /** Clips show stretch handles in their top corners; dragging one stretches the clip. */
  stretchHandles: boolean;
}

/**
 * time: select time, as on the waveform.
 * box, lasso, brush: select an area of time and frequency.
 * spot: paint over a short sound to fill it from the audio around it.
 */
export type SpectralTool = "time" | "box" | "lasso" | "brush" | "spot";

export type PlayMode = "normal" | "loop" | "skip";

export type EditorMode = "multitrack" | "waveform";
/**
 * move: select and drag clips, drag empty space to select time.
 * razor: click a clip to split it there (Shift: every track).
 * slip: drag inside a clip to move its audio without moving the clip.
 * time: drag anywhere, even over clips, to select time.
 */
export type EditorTool = "move" | "razor" | "slip" | "time";

const initial: EditorState = {
  selectedTrack: null,
  selectedClips: new Set(),
  timeFormat: "time",
  width: 800,
  snap: { on: true, clips: true, playhead: true, markers: true, ruler: true },
  autoCrossfade: true,
  showVolume: false,
  showPan: false,
  showFx: false,
  fxLanes: {},
  mode: "multitrack",
  tool: "move",
  playMode: "normal",
  followPlayhead: true,
  showSpectrum: false,
  showPitch: false,
  spectralTool: "time",
  brushSize: 16,
  stretchHandles: false,
};
const state = writable<EditorState>(initial);
let value: EditorState = initial;
state.subscribe((v) => (value = v));

export const editor: Readable<EditorState> & {
  current(): EditorState;
  selectTrack(id: string | null): void;
  selectClips(ids: Iterable<string>, track?: string | null): void;
  setTimeFormat(f: TimeFormat): void;
  setWidth(w: number): void;
  setSnap(patch: Partial<SnapSettings>): void;
  setAutoCrossfade(on: boolean): void;
  setShowVolume(on: boolean): void;
  setShowPan(on: boolean): void;
  setShowFx(on: boolean): void;
  pickFxLane(clipId: string, lane: { slot: string; param: string }): void;
  setMode(mode: EditorMode): void;
  setTool(tool: EditorTool): void;
  /** Turns a play mode on, or back to normal if it was already on. */
  togglePlayMode(mode: Exclude<PlayMode, "normal">): void;
  setFollowPlayhead(on: boolean): void;
  setShowSpectrum(on: boolean): void;
  setShowPitch(on: boolean): void;
  setSpectralTool(tool: SpectralTool): void;
  setBrushSize(px: number): void;
  setStretchHandles(on: boolean): void;
} = {
  subscribe: state.subscribe,
  current: () => value,
  selectTrack: (id) => state.update((s) => ({ ...s, selectedTrack: id, selectedClips: new Set() })),
  selectClips: (ids, track) =>
    state.update((s) => ({ ...s, selectedClips: new Set(ids), selectedTrack: track === undefined ? s.selectedTrack : track })),
  setTimeFormat: (f) => state.update((s) => ({ ...s, timeFormat: f })),
  setAutoCrossfade: (on) => state.update((s) => ({ ...s, autoCrossfade: on })),
  setShowVolume: (on) => state.update((s) => ({ ...s, showVolume: on })),
  setShowPan: (on) => state.update((s) => ({ ...s, showPan: on })),
  setShowFx: (on) => state.update((s) => ({ ...s, showFx: on })),
  pickFxLane: (clipId, lane) => state.update((s) => ({ ...s, showFx: true, fxLanes: { ...s.fxLanes, [clipId]: lane } })),
  setMode: (mode) => state.update((s) => (s.mode === mode ? s : { ...s, mode })),
  setTool: (tool) => state.update((s) => (s.tool === tool ? s : { ...s, tool })),
  togglePlayMode: (mode) => state.update((s) => ({ ...s, playMode: s.playMode === mode ? "normal" : mode })),
  setShowPitch: (on) => state.update((s) => ({ ...s, showPitch: on })),
  setSpectralTool: (tool) => state.update((s) => ({ ...s, spectralTool: tool })),
  setBrushSize: (px) => state.update((s) => ({ ...s, brushSize: Math.max(2, Math.min(80, Math.round(px))) })),
  setShowSpectrum: (on) => state.update((s) => ({ ...s, showSpectrum: on })),
  setFollowPlayhead: (on) => state.update((s) => ({ ...s, followPlayhead: on })),
  setStretchHandles: (on) => state.update((s) => ({ ...s, stretchHandles: on })),
  setSnap: (patch) => state.update((s) => ({ ...s, snap: { ...s.snap, ...patch } })),
  setWidth: (w) => {
    if (w > 0 && w !== value.width) state.update((s) => ({ ...s, width: w }));
  },
};

/** The editor settings that are saved between runs. */
export interface EditorSettings {
  snap: SnapSettings;
  autoCrossfade: boolean;
  timeFormat: TimeFormat;
  showVolume: boolean;
  showPan: boolean;
  playMode: PlayMode;
  followPlayhead: boolean;
  showSpectrum: boolean;
  showPitch: boolean;
  brushSize: number;
  stretchHandles: boolean;
}

export function settingsOf(s: EditorState): EditorSettings {
  return { snap: s.snap, autoCrossfade: s.autoCrossfade, timeFormat: s.timeFormat, showVolume: s.showVolume, showPan: s.showPan, playMode: s.playMode, followPlayhead: s.followPlayhead, showSpectrum: s.showSpectrum, showPitch: s.showPitch, brushSize: s.brushSize, stretchHandles: s.stretchHandles };
}

/** Reads saved settings, keeping defaults for anything missing or the wrong type. */
export function parseSettings(raw: unknown, base: EditorSettings = settingsOf(initial)): EditorSettings {
  const o = (raw && typeof raw === "object" ? raw : {}) as Record<string, unknown>;
  const snapRaw = (o.snap && typeof o.snap === "object" ? o.snap : {}) as Record<string, unknown>;
  const bool = (v: unknown, d: boolean) => (typeof v === "boolean" ? v : d);
  return {
    snap: {
      on: bool(snapRaw.on, base.snap.on),
      clips: bool(snapRaw.clips, base.snap.clips),
      playhead: bool(snapRaw.playhead, base.snap.playhead),
      markers: bool(snapRaw.markers, base.snap.markers),
      ruler: bool(snapRaw.ruler, base.snap.ruler),
    },
    autoCrossfade: bool(o.autoCrossfade, base.autoCrossfade),
    showVolume: bool(o.showVolume, base.showVolume),
    showPan: bool(o.showPan, base.showPan),
    followPlayhead: bool(o.followPlayhead, base.followPlayhead),
    showSpectrum: bool(o.showSpectrum, base.showSpectrum),
    showPitch: bool(o.showPitch, base.showPitch),
    stretchHandles: bool(o.stretchHandles, base.stretchHandles),
    brushSize: typeof o.brushSize === "number" && o.brushSize >= 2 && o.brushSize <= 80 ? Math.round(o.brushSize) : base.brushSize,
    playMode: o.playMode === "loop" || o.playMode === "skip" || o.playMode === "normal" ? o.playMode : base.playMode,
    timeFormat: ["samples", "time", "timecode", "seconds", "bars"].includes(o.timeFormat as string) ? (o.timeFormat as TimeFormat) : base.timeFormat,
  };
}

/** Applies saved settings at startup. */
export function restoreSettings(raw: unknown): void {
  const s = parseSettings(raw);
  state.update((v) => ({ ...v, ...s }));
}
