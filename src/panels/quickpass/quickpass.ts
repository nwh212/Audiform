// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * QuickPass logic with no screen in it: turning measured levels into
 * findings (silences, short noises such as coughs and bumps, sound to
 * mark), turning the core's click and clipping spots into findings,
 * suggesting levels from a clip, and building the edits (remove, turn
 * down, split, mark) that act on findings. Repair is done by the app,
 * which writes a repaired copy of the clip's audio.
 */

import {
  clearRange,
  clipEnd,
  mediaOf,
  previewTracks,
  rippleDeleteRange,
  splitClips,
} from "../editor/clip-edit";
import { levelAt, volumePoints } from "../editor/envelope";
import { newMarker } from "../markers/markers";
import type { ClipLevels, ClipSpots } from "../../app/core";
import type {
  ClipChange,
  EnvelopePointDoc,
  MarkerDoc,
  Op,
  SessionDoc,
} from "../../stores/session";

export type ScanType = "clicks" | "clipping" | "silence" | "noise" | "audio";

export const SCAN_TYPES: {
  id: ScanType;
  label: string;
  noun: [string, string];
  actions: FixAction[];
}[] = [
  {
    id: "clicks",
    label: "Click Repair (clicks, pops, mouth noise)",
    noun: ["click", "clicks"],
    actions: ["repair", "mark"],
  },
  {
    id: "clipping",
    label: "Clip Repair (flattened peaks)",
    noun: ["clipped spot", "clipped spots"],
    actions: ["repair", "mark", "split"],
  },
  {
    id: "silence",
    label: "Remove Silence",
    noun: ["silence", "silences"],
    actions: ["remove", "duck", "split", "mark"],
  },
  {
    id: "noise",
    label: "Short noises (coughs, bumps)",
    noun: ["noise", "noises"],
    actions: ["duck", "remove", "split", "mark"],
  },
  {
    id: "audio",
    label: "Mark Audio (the parts with sound)",
    noun: ["sound", "sounds"],
    actions: ["mark", "split"],
  },
];

export function scanInfo(id: ScanType) {
  return SCAN_TYPES.find((t) => t.id === id) ?? SCAN_TYPES[0]!;
}

export interface QuickPassSettings {
  /** What to look for. */
  scan: ScanType;
  /** Short noises: sound shorter than this (seconds) counts as a noise. */
  maxBurstSec: number;
  /** Short noises: quiet needed either side (seconds), so words are never caught. */
  guardSec: number;
  /** Clipping: samples at or above this level (dBFS). */
  clipDb: number;
  /** Clipping: the longest flat run (ms) that is repaired. */
  clipLongestMs: number;
  /** Clicks: 0 (only obvious ones) to 100 (catch faint ones). */
  clickSensitivity: number;
  /** Clicks: the longest click (ms). */
  clickWidthMs: number;
  /** Audio below this level counts as quiet. */
  quietDb: number;
  /** Quiet must last this long (seconds) to be a silence. */
  minQuietSec: number;
  /** Audio at or above this level counts as speech. */
  contentDb: number;
  /** Speech shorter than this (seconds) does not break a silence. */
  minContentSec: number;
  /** What fixing a silence does. */
  action: FixAction;
  /** Remove leaves this much of each silence (milliseconds) so speech does not run together. */
  keepMs: number;
  /** Where Remove closes the gap. */
  /** What happens to the gap after a removal: stays open, closes on that track, or on all tracks. */
  ripple: "none" | "track" | "all";
  /** How far Turn Down lowers each silence, in dB (negative). */
  duckDb: number;
}

export type FixAction = "repair" | "remove" | "duck" | "split" | "mark";

export const DEFAULTS: QuickPassSettings = {
  scan: "silence",
  maxBurstSec: 0.35,
  guardSec: 0.3,
  clipDb: -0.3,
  clipLongestMs: 3,
  clickSensitivity: 50,
  clickWidthMs: 1,
  quietDb: -60,
  minQuietSec: 0.5,
  contentDb: -50,
  minContentSec: 0.05,
  action: "remove",
  keepMs: 100,
  ripple: "none",
  duckDb: -15,
};

export interface Preset {
  name: string;
  settings: QuickPassSettings;
}

/** Starting points. Each is a full set of settings the user can then adjust. */
export const BUILTIN_PRESETS: readonly Preset[] = [
  { name: "Default", settings: DEFAULTS },
  // Silence and the junk between sentences in one pass: a cough, bump or click
  // shorter than the speech time sits inside the silence and goes with it.
  {
    name: "Clean Up Podcast Interview",
    settings: {
      ...DEFAULTS,
      quietDb: -50,
      minQuietSec: 0.6,
      contentDb: -40,
      minContentSec: 0.3,
      action: "remove",
      keepMs: 150,
    },
  },
  {
    name: "Trim Long Silent Pauses",
    settings: {
      ...DEFAULTS,
      quietDb: -74,
      minQuietSec: 1,
      contentDb: -63,
      minContentSec: 0.05,
      action: "remove",
      keepMs: 100,
    },
  },
  {
    name: "Tighten Pauses Between Speakers",
    settings: {
      ...DEFAULTS,
      quietDb: -55,
      minQuietSec: 0.8,
      contentDb: -45,
      minContentSec: 0.1,
      action: "remove",
      keepMs: 300,
    },
  },
  {
    name: "Trim Short Digital Dropouts",
    settings: {
      ...DEFAULTS,
      quietDb: -100,
      minQuietSec: 0.01,
      contentDb: -80,
      minContentSec: 0.02,
      action: "remove",
      keepMs: 0,
    },
  },
  {
    name: "Turn Down Breaths and Pauses",
    settings: {
      ...DEFAULTS,
      quietDb: -40,
      minQuietSec: 0.15,
      contentDb: -30,
      minContentSec: 0.08,
      action: "duck",
      keepMs: 0,
      duckDb: -12,
    },
  },
  {
    name: "Mark Long Pauses",
    settings: {
      ...DEFAULTS,
      quietDb: -60,
      minQuietSec: 2,
      contentDb: -50,
      minContentSec: 0.1,
      action: "mark",
      keepMs: 0,
    },
  },
  {
    name: "Split at Pauses",
    settings: {
      ...DEFAULTS,
      quietDb: -60,
      minQuietSec: 1,
      contentDb: -50,
      minContentSec: 0.1,
      action: "split",
      keepMs: 0,
    },
  },
  {
    name: "Turn Down Coughs and Bumps",
    settings: {
      ...DEFAULTS,
      scan: "noise",
      quietDb: -45,
      contentDb: -35,
      maxBurstSec: 0.35,
      guardSec: 0.3,
      action: "duck",
      duckDb: -30,
      keepMs: 0,
    },
  },
  {
    name: "Cut Coughs and Bumps",
    settings: {
      ...DEFAULTS,
      scan: "noise",
      quietDb: -45,
      contentDb: -35,
      maxBurstSec: 0.35,
      guardSec: 0.3,
      action: "remove",
      keepMs: 0,
    },
  },
  {
    name: "Find Clipping",
    settings: {
      ...DEFAULTS,
      scan: "clipping",
      clipDb: -0.3,
      action: "mark",
      keepMs: 0,
    },
  },
  {
    name: "Repair Clipping",
    settings: {
      ...DEFAULTS,
      scan: "clipping",
      clipDb: -0.5,
      clipLongestMs: 3,
      action: "repair",
      keepMs: 0,
    },
  },
  {
    name: "Remove Clicks",
    settings: {
      ...DEFAULTS,
      scan: "clicks",
      clickSensitivity: 50,
      clickWidthMs: 1,
      action: "repair",
      keepMs: 0,
    },
  },
  {
    name: "Remove Mouth Clicks (sensitive)",
    settings: {
      ...DEFAULTS,
      scan: "clicks",
      clickSensitivity: 75,
      clickWidthMs: 0.6,
      action: "repair",
      keepMs: 0,
    },
  },
  {
    name: "Mark Non-Silent Areas",
    settings: {
      ...DEFAULTS,
      scan: "audio",
      quietDb: -50,
      minQuietSec: 0.5,
      contentDb: -40,
      minContentSec: 0.1,
      action: "mark",
      keepMs: 0,
    },
  },
];

const num = (v: unknown, lo: number, hi: number, fallback: number) =>
  typeof v === "number" && Number.isFinite(v)
    ? Math.min(hi, Math.max(lo, v))
    : fallback;

/** Saved settings with anything missing or out of range put right. */
export function sanitizeSettings(raw: unknown): QuickPassSettings {
  const r = (raw && typeof raw === "object" ? raw : {}) as Record<
    string,
    unknown
  >;
  const quietDb = num(r.quietDb, -100, -1, DEFAULTS.quietDb);
  const scan: ScanType =
    r.scan === "noise" ||
    r.scan === "clipping" ||
    r.scan === "clicks" ||
    r.scan === "audio"
      ? r.scan
      : "silence";
  const actions = scanInfo(scan).actions;
  const action = actions.includes(r.action as FixAction)
    ? (r.action as FixAction)
    : actions[0]!;
  return {
    scan,
    maxBurstSec: num(r.maxBurstSec, 0.02, 2, DEFAULTS.maxBurstSec),
    guardSec: num(r.guardSec, 0.05, 5, DEFAULTS.guardSec),
    clipDb: num(r.clipDb, -12, 0, DEFAULTS.clipDb),
    clipLongestMs: num(r.clipLongestMs, 0.1, 10, DEFAULTS.clipLongestMs),
    clickSensitivity: num(
      r.clickSensitivity,
      0,
      100,
      DEFAULTS.clickSensitivity,
    ),
    clickWidthMs: num(r.clickWidthMs, 0.1, 3, DEFAULTS.clickWidthMs),
    quietDb,
    minQuietSec: num(r.minQuietSec, 0.005, 60, DEFAULTS.minQuietSec),
    contentDb: Math.max(quietDb, num(r.contentDb, -100, 0, DEFAULTS.contentDb)),
    minContentSec: num(r.minContentSec, 0.01, 10, DEFAULTS.minContentSec),
    action,
    keepMs: num(r.keepMs, 0, 5000, DEFAULTS.keepMs),
    ripple:
      r.ripple === "all" ? "all" : r.ripple === "track" ? "track" : "none",
    duckDb: num(r.duckDb, -60, -1, DEFAULTS.duckDb),
  };
}

export interface Finding {
  id: string;
  kind: ScanType;
  trackId: string;
  /** The clip it was found in. */
  clipId?: string;
  clipName: string;
  /** Session samples. */
  start: number;
  end: number;
}

/** Runs of windows [from, to) that are silence under the settings. */
export function silentRuns(
  db: readonly number[],
  windowSec: number,
  s: QuickPassSettings,
): [number, number][] {
  const minQuiet = Math.max(1, Math.ceil(s.minQuietSec / windowSec));
  const minContent = Math.max(1, Math.ceil(s.minContentSec / windowSec));
  // Quiet runs, then join two that only a short burst of speech separates.
  const quiet: [number, number][] = [];
  let from = -1;
  db.forEach((v, i) => {
    if (v < s.quietDb) {
      if (from < 0) from = i;
    } else if (from >= 0) {
      quiet.push([from, i]);
      from = -1;
    }
  });
  if (from >= 0) quiet.push([from, db.length]);
  const joined: [number, number][] = [];
  for (const run of quiet) {
    const last = joined[joined.length - 1];
    if (last) {
      const between = db.slice(last[1], run[0]);
      const loud = between.filter((v) => v >= s.contentDb).length;
      if (between.length < minContent || loud < minContent) {
        last[1] = run[1];
        continue;
      }
    }
    joined.push([...run]);
  }
  return joined.filter(([a, b]) => b - a >= minQuiet);
}

/** Findings for the measured clips, in timeline order per track. */
export function findSilences(
  levels: readonly ClipLevels[],
  tracks: ReadonlyMap<string, { trackId: string; name: string }>,
  s: QuickPassSettings,
  rate: number,
): Finding[] {
  const out: Finding[] = [];
  for (const l of levels) {
    const info = tracks.get(l.clipId);
    if (!info) continue;
    const windowSec = l.window / rate;
    const clipStart = l.start;
    const clipEndAt = l.start + l.db.length * l.window;
    for (const [a, b] of silentRuns(l.db, windowSec, s)) {
      const start = clipStart + a * l.window;
      const end = Math.min(clipEndAt, clipStart + b * l.window);
      out.push({
        id: `${l.clipId}:${start}`,
        kind: "silence",
        trackId: info.trackId,
        clipId: l.clipId,
        clipName: info.name,
        start,
        end,
      });
    }
  }
  return out.sort(
    (x, y) => x.start - y.start || x.trackId.localeCompare(y.trackId),
  );
}

/**
 * Short noises: sound at or above the quiet level that lasts no longer
 * than `maxBurstSec`, with at least `guardSec` of quiet either side (the
 * clip's ends count as quiet). The guard keeps short words from being
 * caught. Each run is padded by one window so its edges go too.
 */
export function noiseRuns(
  db: readonly number[],
  windowSec: number,
  s: QuickPassSettings,
): [number, number][] {
  const maxLen = Math.max(1, Math.floor(s.maxBurstSec / windowSec));
  const guard = Math.max(1, Math.ceil(s.guardSec / windowSec));
  const loud: [number, number][] = [];
  let from = -1;
  db.forEach((v, i) => {
    if (v >= s.quietDb) {
      if (from < 0) from = i;
    } else if (from >= 0) {
      loud.push([from, i]);
      from = -1;
    }
  });
  if (from >= 0) loud.push([from, db.length]);
  const out: [number, number][] = [];
  loud.forEach(([a, b], i) => {
    if (b - a > maxLen) return;
    const before = i === 0 ? a : a - loud[i - 1]![1];
    const after = i === loud.length - 1 ? db.length - b : loud[i + 1]![0] - b;
    if ((i > 0 && before < guard) || (i < loud.length - 1 && after < guard))
      return;
    out.push([Math.max(0, a - 1), Math.min(db.length, b + 1)]);
  });
  return out;
}

/** The parts with sound: everything between silences, in windows [from, to). */
export function soundRuns(
  db: readonly number[],
  windowSec: number,
  s: QuickPassSettings,
): [number, number][] {
  const out: [number, number][] = [];
  let at = 0;
  for (const [a, b] of silentRuns(db, windowSec, s)) {
    if (a > at) out.push([at, a]);
    at = b;
  }
  if (at < db.length) out.push([at, db.length]);
  // A lone window of noise is not a part with sound.
  const minContent = Math.max(1, Math.ceil(s.minContentSec / windowSec));
  return out.filter(([a, b]) => b - a >= minContent);
}

/** What the core looks for, from the settings. */
export function spotsRequest(
  s: QuickPassSettings,
):
  | { kind: "clicks"; sensitivity: number; widthMs: number }
  | { kind: "clipping"; levelDb: number; longestMs: number } {
  return s.scan === "clicks"
    ? {
        kind: "clicks",
        sensitivity: s.clickSensitivity,
        widthMs: s.clickWidthMs,
      }
    : { kind: "clipping", levelDb: s.clipDb, longestMs: s.clipLongestMs };
}

/** Findings from the core's click or clipping spots, in timeline order. */
export function spotFindings(
  spots: readonly ClipSpots[],
  tracks: ReadonlyMap<string, { trackId: string; name: string }>,
  kind: ScanType,
): Finding[] {
  const out: Finding[] = [];
  for (const c of spots) {
    const info = tracks.get(c.clipId);
    if (!info) continue;
    for (const [a, b] of c.spans) {
      out.push({
        id: `${c.clipId}:${kind}:${a}`,
        kind,
        trackId: info.trackId,
        clipId: c.clipId,
        clipName: info.name,
        start: a,
        end: b + 1,
      });
    }
  }
  return out.sort(
    (x, y) => x.start - y.start || x.trackId.localeCompare(y.trackId),
  );
}

/** Clipping: windows whose peak reaches `clipDb`, joined when two or fewer windows apart. */
export function clipRuns(
  peak: readonly number[],
  s: QuickPassSettings,
): [number, number][] {
  const out: [number, number][] = [];
  peak.forEach((v, i) => {
    if (v < s.clipDb) return;
    const last = out[out.length - 1];
    if (last && i - last[1] <= 2) last[1] = i + 1;
    else out.push([i, i + 1]);
  });
  return out;
}

/** Findings of the chosen scan type for the measured clips, in timeline order. */
export function findAll(
  levels: readonly ClipLevels[],
  tracks: ReadonlyMap<string, { trackId: string; name: string }>,
  s: QuickPassSettings,
  rate: number,
): Finding[] {
  if (s.scan === "silence") return findSilences(levels, tracks, s, rate);
  const out: Finding[] = [];
  for (const l of levels) {
    const info = tracks.get(l.clipId);
    if (!info) continue;
    const runs =
      s.scan === "noise"
        ? noiseRuns(l.db, l.window / rate, s)
        : s.scan === "audio"
          ? soundRuns(l.db, l.window / rate, s)
          : clipRuns(l.peak ?? [], s);
    const clipEndAt = l.start + l.db.length * l.window;
    for (const [a, b] of runs) {
      const start = l.start + a * l.window;
      const end = Math.min(clipEndAt, l.start + b * l.window);
      out.push({
        id: `${l.clipId}:${s.scan}:${start}`,
        kind: s.scan,
        trackId: info.trackId,
        clipId: l.clipId,
        clipName: info.name,
        start,
        end,
      });
    }
  }
  return out.sort(
    (x, y) => x.start - y.start || x.trackId.localeCompare(y.trackId),
  );
}

/**
 * Starting levels from what the clip sounds like: the quiet level sits
 * a little above its noise floor, the speech level halfway to its
 * normal speaking level. Null when there is too little to judge.
 */
export function suggestLevels(
  db: readonly number[],
): { quietDb: number; contentDb: number } | null {
  const v = db.filter((x) => x > -120).sort((a, b) => a - b);
  if (v.length < 20) return null;
  const at = (p: number) =>
    v[Math.min(v.length - 1, Math.floor(p * v.length))]!;
  const floor = at(0.1);
  const speech = at(0.9);
  if (speech - floor < 6) return null;
  const quietDb = Math.round(floor + (speech - floor) * 0.2);
  const contentDb = Math.max(
    quietDb,
    Math.round(floor + (speech - floor) * 0.4),
  );
  return { quietDb, contentDb };
}

/** What an edit did to the clips, so several can be built one after another. */
export function applyLocal(doc: SessionDoc, op: Op): SessionDoc {
  switch (op.op) {
    case "batch":
      return op.ops.reduce(applyLocal, doc);
    case "remove_clips": {
      const gone = new Set(op.ids);
      return {
        ...doc,
        tracks: doc.tracks.map((t) => ({
          ...t,
          clips: t.clips.filter((c) => !gone.has(c.id)),
        })),
      };
    }
    case "set_clips":
      return { ...doc, tracks: previewTracks(doc, op.changes) };
    case "restore_clips": {
      const by = new Map<string, typeof op.clips>();
      for (const e of op.clips) by.set(e[0], [...(by.get(e[0]) ?? []), e]);
      return {
        ...doc,
        tracks: doc.tracks.map((t) => ({
          ...t,
          clips: [...t.clips, ...(by.get(t.id) ?? []).map((e) => e[1])],
        })),
      };
    }
    default:
      return doc;
  }
}

/** Removes the silences, leaving the gaps open unless `ripple` closes them. Last one first so earlier ones keep their place. */
export function cutRange(
  f: Pick<Finding, "start" | "end">,
  keepMs: number,
  rate: number,
): [number, number] | null {
  const half = Math.round((keepMs / 1000) * rate * 0.5);
  const a = f.start + half;
  const b = f.end - half;
  return b > a ? [a, b] : null;
}

export function removeOp(
  doc: SessionDoc,
  found: readonly Finding[],
  s: Pick<QuickPassSettings, "ripple" | "keepMs">,
  newId: () => string,
): Op | null {
  const ops: Op[] = [];
  let cur = doc;
  for (const f of [...found].sort((a, b) => b.start - a.start)) {
    const cut = cutRange(f, s.keepMs, doc.sample_rate);
    if (!cut) continue;
    const tracks =
      s.ripple === "all" ? cur.tracks.map((t) => t.id) : [f.trackId];
    // "none" strips the audio and leaves the gap open, so every track stays in step.
    const op =
      s.ripple === "none"
        ? clearRange(cur, tracks, cut[0], cut[1], newId)
        : rippleDeleteRange(cur, tracks, cut[0], cut[1], newId);
    if (!op) continue;
    ops.push(op);
    cur = applyLocal(cur, op);
  }
  return ops.length ? { op: "batch", ops } : null;
}

/** Splits each track at both edges of every silence. Locked clips are left alone. */
export function splitOp(
  doc: SessionDoc,
  found: readonly Finding[],
  newId: () => string,
): Op | null {
  const ops: Op[] = [];
  let cur = doc;
  for (const f of found) {
    const t = cur.tracks.find((x) => x.id === f.trackId);
    if (!t) continue;
    for (const at of [f.end, f.start]) {
      const op = splitClips(
        cur,
        t.clips.map((c) => c.id),
        at,
        newId,
      );
      if (!op) continue;
      ops.push(op);
      cur = applyLocal(cur, op);
    }
  }
  return ops.length ? { op: "batch", ops } : null;
}

/** The clips that now sit exactly on the findings, to leave selected after a split. */
export function clipsOn(doc: SessionDoc, found: readonly Finding[]): string[] {
  const media = mediaOf(doc);
  const ids: string[] = [];
  for (const f of found) {
    const t = doc.tracks.find((x) => x.id === f.trackId);
    for (const c of t?.clips ?? [])
      if (c.start === f.start && clipEnd(c, media, doc.sample_rate) === f.end)
        ids.push(c.id);
  }
  return ids;
}

/** One range marker per silence. */
export function markersFor(
  doc: SessionDoc,
  found: readonly Finding[],
  newId: () => string,
  base = "Silence",
): MarkerDoc[] {
  const re = new RegExp(
    `^${base.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")} \\d+$`,
  );
  const taken = doc.markers.filter((m) => re.test(m.name)).length;
  return [...found]
    .sort((a, b) => a.start - b.start)
    .map((f, i) =>
      newMarker(newId(), `${base} ${taken + i + 1}`, f.start, f.end - f.start),
    );
}

/** After `gone` is removed, where the other findings sit. Those inside it are dropped. */
export function shiftFindings(
  rest: readonly Finding[],
  gone: Finding,
  ripple: "none" | "track" | "all",
  keepMs = 0,
  rate = 48000,
): Finding[] {
  const cut = cutRange(gone, keepMs, rate) ?? [gone.start, gone.start];
  const len = cut[1] - cut[0];
  gone = { ...gone, start: cut[0], end: cut[1] };
  // Left open: nothing moves, only findings inside the stripped part are gone.
  if (ripple === "none")
    return rest.filter(
      (f) =>
        f.id !== gone.id &&
        !(
          f.trackId === gone.trackId &&
          f.start >= gone.start &&
          f.end <= gone.end
        ),
    );
  return rest
    .filter(
      (f) =>
        f.id !== gone.id &&
        !(
          (ripple === "all" || f.trackId === gone.trackId) &&
          f.start < gone.end &&
          f.end > gone.start
        ),
    )
    .map((f) =>
      f.start >= gone.end && (ripple === "all" || f.trackId === gone.trackId)
        ? { ...f, start: f.start - len, end: f.end - len }
        : f,
    );
}

/** Longest fade into and out of a turned-down stretch, in seconds. */
const DUCK_RAMP_SEC = 0.01;

/**
 * Turns each silence down with volume envelope points instead of cutting
 * it, so clips stay whole and every dip can be adjusted afterward. Points
 * already inside a dip are replaced; the level either side is kept.
 */
export function duckOp(
  doc: SessionDoc,
  found: readonly Finding[],
  duckDb: number,
): Op | null {
  const media = mediaOf(doc);
  const rate = doc.sample_rate;
  const maxRamp = Math.round(DUCK_RAMP_SEC * rate);
  const changes: ClipChange[] = [];
  for (const t of doc.tracks) {
    const mine = found.filter((f) => f.trackId === t.id);
    if (mine.length === 0) continue;
    for (const c of t.clips) {
      if (c.locked) continue;
      const len = clipEnd(c, media, rate) - c.start;
      let points: EnvelopePointDoc[] = [...volumePoints(c)];
      let touched = false;
      for (const f of [...mine].sort((a, b) => a.start - b.start)) {
        const a = Math.max(0, f.start - c.start);
        const b = Math.min(len, f.end - c.start);
        if (b - a < 2) continue;
        const ramp = Math.max(1, Math.min(maxRamp, Math.floor((b - a) / 4)));
        const lead = Math.max(0, a - ramp);
        const tail = Math.min(len, b + ramp);
        const clamp = (v: number) => Math.max(-60, Math.min(12, v));
        const before = levelAt(points, lead);
        const after = levelAt(points, tail);
        const atA = levelAt(points, a);
        const atB = levelAt(points, b);
        const add: EnvelopePointDoc[] = [];
        if (a > 0) add.push({ position: lead, value: before, curve: "linear" });
        add.push({ position: a, value: clamp(atA + duckDb), curve: "linear" });
        add.push({ position: b, value: clamp(atB + duckDb), curve: "linear" });
        if (b < len)
          add.push({ position: tail, value: after, curve: "linear" });
        const lo = a > 0 ? lead : a;
        const hi = b < len ? tail : b;
        points = points
          .filter((p) => p.position < lo || p.position > hi)
          .concat(add);
        points.sort((x, y) => x.position - y.position);
        // Two points on one position: keep the later one.
        points = points.filter(
          (p, i) =>
            i === points.length - 1 || points[i + 1]!.position !== p.position,
        );
        touched = true;
      }
      if (touched) {
        // A clip with no envelope before needs its ends pinned at 0 dB.
        if (volumePoints(c).length === 0) {
          if (points[0]!.position > 0)
            points.unshift({ position: 0, value: 0, curve: "linear" });
          if (points[points.length - 1]!.position < len)
            points.push({ position: len, value: 0, curve: "linear" });
        }
        changes.push({ id: c.id, volume: points });
      }
    }
  }
  return changes.length ? { op: "set_clips", changes } : null;
}
