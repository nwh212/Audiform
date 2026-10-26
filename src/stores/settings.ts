// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * App-wide settings from Edit > Settings that are not editor state:
 * zoom step, fades, colors, the output device, autosave and so on.
 * Saved in the app's preferences under one key; read synchronously
 * wherever they apply, so a change takes effect on the next action.
 */

/** A fade curve, as the session stores it. */
export type FadeShape = "linear" | "logarithmic" | "cosine" | "exponential" | "equal_power";
export type WheelAnchor = "mouse" | "between" | "playhead";

export interface AppColors {
  /** Waveforms in the Waveform editor. Empty keeps the theme color. */
  wave: string;
  playhead: string;
  accent: string;
  /** The selected track's lane. */
  laneSelected: string;
  /** Spectrogram: quiet and loud. Both empty keeps the built-in palette. */
  spectrumLow: string;
  spectrumHigh: string;
  /** Phase Wheel: lowest and highest frequencies. Empty keeps red to magenta. */
  phaseLow: string;
  phaseHigh: string;
}

export interface AppSettings {
  /** How much one Ctrl+wheel step zooms, in percent. */
  zoomStep: number;
  wheelAnchor: WheelAnchor;
  /** The plain mouse wheel over the timeline zooms. Off: it scrolls. Ctrl + wheel always zooms. */
  wheelZooms: boolean;
  /** Shape a new fade starts with. */
  fadeShape: FadeShape;
  /** Several files dropped on the Editor: one after another on one track, or each on its own track from the same time. */
  dropMany: "sequence" | "tracks";
  /** What the space bar does to a playing session. */
  colors: AppColors;
  /** System name of the output; null is the system default. */
  output: string | null;
  /** Device outputs (from 0) the stereo mix plays on: left, right. */
  outputPair: [number, number];
  /** 3D Space sessions heard on speakers: each speaker's device output (from 0) by name. Missing speakers play on the output at their place in the layout. */
  speakerOutputs: Record<string, number>;
  /** Look of the app: the built-in dark theme, or an Aero theme (clear glass over a scene): Meadow, Aqua or Moonlight. */
  theme: "dark" | "meadow" | "aqua" | "moonlight" | "prism";
  /** How lively the level meters are. */
  meterSpeed: "fast" | "medium" | "slow";
  /** The custom loudness target: integrated LUFS and true peak ceiling. */
  customLufs: number;
  customPeak: number;
  /** Audio driver: the system's own, or ASIO (Windows release builds). */
  driver: "system" | "asio";
  /** Input the Waveform editor records from, as a track input ("Mono: ..." or "Stereo: ..."); null is the system default. */
  waveInput: string | null;
  autosave: boolean;
  autosaveMinutes: number;
  /** Mix down sessions that use VST3 plug-ins at playback speed, for plug-ins that only work in real time. */
  realtimePlugins: boolean;
  /** Show the Audio FX chain when an effect is added from the Effects menu. */
  openChainOnAdd: boolean;
  /** Soloing a track unsolos the others. */
  soloExclusive: boolean;
  /** Meter color changes, dBFS. */
  meterYellowDb: number;
  meterRedDb: number;
  /** Keep older copies of the session file in a Backups folder when saving. */
  backups: boolean;
  backupCount: number;
  /** Copy audio imported into a saved session into its folder: always, ask each time, or never. */
  copyMedia: "always" | "ask" | "never";
  /** The folder in the session folder that copies go into. */
  mediaFolder: string;
  /** When saving, copy in any audio the session uses from outside its folder. */
  collectOnSave: boolean;
  /** Leave files from libraries where they are: libraries keep their own copies. */
  leaveLibraryFiles: boolean;
  /** Seconds counted down before recording starts; 0 starts at once. */
  countdownSec: number;
  /** Name new markers start with, numbered after it. */
  markerName: string;
  /** Frames per second for timecode. */
  timecodeFps: number;
  /** The Phase Wheel's choices, kept for every file and session. */
  phaseWheel: PhaseWheelPrefs;
  /** The metronome, kept between runs. */
  metronome: MetronomePrefs;
  /** 3D Space: dots snap to speakers, the center lines and a grid; turns snap to 15 degrees. */
  spaceSnap: boolean;
}

export type MetronomeSound = "tick" | "wood" | "beep" | "shaker";
export type MetronomeSubdivide = "none" | "halves" | "triplets" | "quarters";
export interface MetronomePrefs {
  /** The metronome is on: its track shows and it clicks (unless muted). */
  on: boolean;
  /** Its track's mute: silent, but the track stays. */
  muted: boolean;
  /** Click during playback and recording, or while recording only. */
  when: "always" | "recording";
  levelDb: number;
  sound: MetronomeSound;
  /** The first beat of each bar clicks higher. */
  accent: boolean;
  subdivide: MetronomeSubdivide;
}
export const METRONOME_SOUNDS: { value: MetronomeSound; label: string }[] = [
  { value: "tick", label: "Tick" },
  { value: "wood", label: "Wood" },
  { value: "beep", label: "Beep" },
  { value: "shaker", label: "Shaker" },
];
export const METRONOME_SUBDIVIDE: { value: MetronomeSubdivide; label: string }[] = [
  { value: "none", label: "Beats only" },
  { value: "halves", label: "Halves" },
  { value: "triplets", label: "Triplets" },
  { value: "quarters", label: "Quarters" },
];

export type PhaseSignal = "left" | "right" | "mid" | "side";
export interface PhaseWheelPrefs {
  /** Frames analysed each time. */
  size: number;
  /** Pictures a second, at most. */
  fps: number;
  a: PhaseSignal;
  b: PhaseSignal;
}
export const PHASE_SIZES = [1024, 2048, 4096, 8192, 16384] as const;
export const PHASE_RATES = [15, 30, 60, 120] as const;
const PHASE_SIGNALS: readonly PhaseSignal[] = ["left", "right", "mid", "side"];

export const TIMECODE_RATES = [24, 25, 30, 50, 60] as const;

export const NO_COLORS: AppColors = {
  wave: "",
  playhead: "",
  accent: "",
  laneSelected: "",
  spectrumLow: "",
  spectrumHigh: "",
  phaseLow: "",
  phaseHigh: "",
};

export const DEFAULT_SETTINGS: AppSettings = {
  zoomStep: 25,
  wheelAnchor: "between",
  wheelZooms: true,
  fadeShape: "cosine",
  dropMany: "sequence",
  colors: { ...NO_COLORS },
  output: null,
  outputPair: [0, 1],
  theme: "dark",
  speakerOutputs: {},
  waveInput: null,
  driver: "system",
  meterSpeed: "fast",
  customLufs: -18,
  customPeak: -1,
  autosave: true,
  autosaveMinutes: 2,
  openChainOnAdd: true,
  realtimePlugins: false,
  soloExclusive: false,
  meterYellowDb: -12,
  meterRedDb: -3,
  backups: false,
  backupCount: 5,
  copyMedia: "always",
  mediaFolder: "Audio",
  collectOnSave: true,
  leaveLibraryFiles: true,
  countdownSec: 0,
  markerName: "Marker",
  timecodeFps: 30,
  phaseWheel: { size: 4096, fps: 60, a: "left", b: "right" },
  spaceSnap: true,
  metronome: { on: false, muted: false, when: "always", levelDb: -6, sound: "tick", accent: true, subdivide: "none" },
};

const HEX = /^#[0-9a-f]{6}$/i;

/** Saved settings, with defaults for anything missing, out of range or the wrong type. */
export function sanitizeSettings(raw: unknown): AppSettings {
  const o = (raw && typeof raw === "object" ? raw : {}) as Record<
    string,
    unknown
  >;
  const d = DEFAULT_SETTINGS;
  const num = (v: unknown, lo: number, hi: number, def: number) =>
    typeof v === "number" && Number.isFinite(v)
      ? Math.min(hi, Math.max(lo, v))
      : def;
  const bool = (v: unknown, def: boolean) => (typeof v === "boolean" ? v : def);
  const c = (
    o.colors && typeof o.colors === "object" ? o.colors : {}
  ) as Record<string, unknown>;
  const color = (v: unknown) =>
    typeof v === "string" && HEX.test(v) ? v.toLowerCase() : "";
  const yellow = num(o.meterYellowDb, -40, -1, d.meterYellowDb);
  return {
    zoomStep: Math.round(num(o.zoomStep, 5, 100, d.zoomStep)),
    wheelAnchor: o.wheelAnchor === "playhead" || o.wheelAnchor === "between" || o.wheelAnchor === "mouse" ? o.wheelAnchor : d.wheelAnchor,
    wheelZooms: bool(o.wheelZooms, d.wheelZooms),
    dropMany: o.dropMany === "tracks" ? "tracks" : "sequence",
    fadeShape:
      o.fadeShape === "linear" || o.fadeShape === "logarithmic" || o.fadeShape === "exponential" || o.fadeShape === "equal_power"
        ? o.fadeShape
        : "cosine",
    colors: {
      wave: color(c.wave),
      playhead: color(c.playhead),
      accent: color(c.accent),
      laneSelected: color(c.laneSelected),
      spectrumLow: color(c.spectrumLow),
      spectrumHigh: color(c.spectrumHigh),
      phaseLow: color(c.phaseLow),
      phaseHigh: color(c.phaseHigh),
    },
    output: typeof o.output === "string" && o.output.trim() ? o.output : null,
    outputPair:
      Array.isArray(o.outputPair) && o.outputPair.length === 2 && o.outputPair.every((v) => Number.isInteger(v) && v >= 0 && v < 64)
        ? [o.outputPair[0] as number, o.outputPair[1] as number]
        : [0, 1],
    speakerOutputs: Object.fromEntries(
      Object.entries(typeof o.speakerOutputs === "object" && o.speakerOutputs ? (o.speakerOutputs as Record<string, unknown>) : {}).filter(
        (e): e is [string, number] => /^[A-Za-z]{1,4}$/.test(e[0]) && Number.isInteger(e[1]) && (e[1] as number) >= 0 && (e[1] as number) < 64,
      ),
    ),
    driver: o.driver === "asio" ? "asio" : "system",
    // Sky Glass and Night Glass (tried out, then dropped) become Meadow and Moonlight Aero.
    theme: o.theme === "meadow" || o.theme === "aqua" || o.theme === "moonlight" || o.theme === "prism" ? o.theme : o.theme === "sky" ? "meadow" : o.theme === "night" ? "moonlight" : "dark",
    meterSpeed:
      o.meterSpeed === "medium" || o.meterSpeed === "slow"
        ? o.meterSpeed
        : "fast",
    customLufs: Math.round(num(o.customLufs, -40, -5, d.customLufs) * 10) / 10,
    customPeak: Math.round(num(o.customPeak, -10, 0, d.customPeak) * 10) / 10,
    waveInput:
      typeof o.waveInput === "string" && /^(Mono|Stereo): /.test(o.waveInput)
        ? o.waveInput
        : null,
    autosave: bool(o.autosave, d.autosave),
    autosaveMinutes: Math.round(
      num(o.autosaveMinutes, 1, 60, d.autosaveMinutes),
    ),
    openChainOnAdd: bool(o.openChainOnAdd, d.openChainOnAdd),
    realtimePlugins: bool(o.realtimePlugins, d.realtimePlugins),
    soloExclusive: bool(o.soloExclusive, d.soloExclusive),
    meterYellowDb: yellow,
    // Red sits above yellow.
    meterRedDb: num(
      o.meterRedDb,
      yellow + 1,
      0,
      Math.max(d.meterRedDb, yellow + 1),
    ),
    backups: bool(o.backups, d.backups),
    backupCount: Math.round(num(o.backupCount, 1, 50, d.backupCount)),
    copyMedia: o.copyMedia === "ask" || o.copyMedia === "never" ? o.copyMedia : d.copyMedia,
    mediaFolder: typeof o.mediaFolder === "string" && o.mediaFolder.replace(/[\\/:*?"<>|.]/g, "").trim() ? o.mediaFolder.replace(/[\\/:*?"<>|]/g, "").trim().slice(0, 60) : d.mediaFolder,
    collectOnSave: bool(o.collectOnSave, d.collectOnSave),
    leaveLibraryFiles: bool(o.leaveLibraryFiles, d.leaveLibraryFiles),
    countdownSec: Math.round(num(o.countdownSec, 0, 10, d.countdownSec)),
    markerName:
      typeof o.markerName === "string" && o.markerName.trim()
        ? o.markerName.trim().slice(0, 40)
        : d.markerName,
    timecodeFps: TIMECODE_RATES.includes(o.timecodeFps as never)
      ? (o.timecodeFps as number)
      : d.timecodeFps,
    phaseWheel: phasePrefs(o.phaseWheel),
    metronome: metronomePrefs(o.metronome),
    spaceSnap: bool(o.spaceSnap, d.spaceSnap),
  };
}

function metronomePrefs(raw: unknown): MetronomePrefs {
  const m = (raw && typeof raw === "object" ? raw : {}) as Record<string, unknown>;
  const d = DEFAULT_SETTINGS.metronome;
  const lvl = typeof m.levelDb === "number" && Number.isFinite(m.levelDb) ? Math.min(6, Math.max(-60, m.levelDb)) : d.levelDb;
  return {
    on: typeof m.on === "boolean" ? m.on : d.on,
    muted: typeof m.muted === "boolean" ? m.muted : d.muted,
    when: m.when === "recording" ? "recording" : "always",
    levelDb: Math.round(lvl * 10) / 10,
    sound: METRONOME_SOUNDS.some((x) => x.value === m.sound) ? (m.sound as MetronomeSound) : d.sound,
    accent: typeof m.accent === "boolean" ? m.accent : d.accent,
    subdivide: METRONOME_SUBDIVIDE.some((x) => x.value === m.subdivide) ? (m.subdivide as MetronomeSubdivide) : d.subdivide,
  };
}

function phasePrefs(raw: unknown): PhaseWheelPrefs {
  const p = (raw && typeof raw === "object" ? raw : {}) as Record<string, unknown>;
  const d = DEFAULT_SETTINGS.phaseWheel;
  const sig = (v: unknown, def: PhaseSignal) => (PHASE_SIGNALS.includes(v as PhaseSignal) ? (v as PhaseSignal) : def);
  return {
    size: PHASE_SIZES.includes(p.size as never) ? (p.size as number) : d.size,
    fps: PHASE_RATES.includes(p.fps as never) ? (p.fps as number) : d.fps,
    a: sig(p.a, d.a),
    b: sig(p.b, d.b),
  };
}

type Listener = (s: AppSettings) => void;

class SettingsStore {
  private value: AppSettings = {
    ...DEFAULT_SETTINGS,
    colors: { ...NO_COLORS },
  };
  private readonly listeners = new Set<Listener>();

  get current(): AppSettings {
    return this.value;
  }

  set(next: AppSettings): void {
    this.value = sanitizeSettings(next);
    for (const fn of this.listeners) fn(this.value);
  }

  subscribe(fn: Listener): () => void {
    this.listeners.add(fn);
    fn(this.value);
    return () => {
      this.listeners.delete(fn);
    };
  }
}

export const settings = new SettingsStore();

/** The Ctrl+wheel zoom ratios for zooming in and out. */
/**
 * Where a wheel zoom centers, in pixels across a view `width` wide: the
 * pointer, the playhead (when in view), or between the pointer and the
 * middle of the view (pulled 40% toward the middle, so zooming drifts
 * less). Over a ruler, `exact` keeps it on the pointer.
 */
export function wheelAnchorX(mouse: number, width: number, playheadX: number, exact = false): number {
  const a = settings.current.wheelAnchor;
  if (exact || a === "mouse") return mouse;
  if (a === "playhead") return Math.max(0, Math.min(width, playheadX));
  return mouse + (width / 2 - mouse) * 0.4;
}

export function zoomRatios(step = settings.current.zoomStep): {
  zoomIn: number;
  zoomOut: number;
} {
  const out = 1 + step / 100;
  return { zoomIn: 1 / out, zoomOut: out };
}

/** The theme color overrides as CSS variables; empty ones fall back to the theme. */
export function colorVars(c: AppColors): Record<string, string> {
  const v: Record<string, string> = {};
  if (c.wave) v["--wave"] = c.wave;
  if (c.playhead) v["--playhead"] = c.playhead;
  if (c.accent) v["--accent"] = c.accent;
  if (c.laneSelected) v["--lane-selected"] = c.laneSelected;
  return v;
}

/** Where a level in dBFS sits on a -60 to 0 dB meter, as a percentage. */
export function meterPercent(db: number, floorDb = -60): number {
  return Math.max(0, Math.min(100, ((db - floorDb) / -floorDb) * 100));
}
