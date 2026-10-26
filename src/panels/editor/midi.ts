// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * MIDI clips and notes: beats and samples, snapping, and the edits the
 * lanes and the piano roll make. MIDI clips and notes are placed in
 * beats, so they follow the session tempo; the core turns beats into
 * samples (the same sums as here) when it plays them.
 */

import type { MidiClipDoc, NoteDoc, Op, SessionDoc, TempoDoc } from "../../stores/session";
import { barAt, barBeats, barStart, beatsToSeconds, secondsToBeats, segmentAtBeat } from "../../stores/tempo";

/** Beats to session samples, through the tempo map. */
export function beatsToSamples(beats: number, t: TempoDoc, rate: number): number {
  return Math.round(beatsToSeconds(t, beats) * rate);
}

/** Session samples to beats, through the tempo map. */
export function samplesToBeats(samples: number, t: TempoDoc, rate: number): number {
  return secondsToBeats(t, samples / Math.max(1, rate));
}

/** Rounds to the nearest multiple of `grid` beats (0 leaves it alone). */
export function snap(beats: number, grid: number): number {
  if (grid <= 0) return beats;
  return Math.round(beats / grid) * grid;
}

/** Rounds down to a multiple of `grid` beats. */
export function snapDown(beats: number, grid: number): number {
  if (grid <= 0) return beats;
  return Math.floor(beats / grid + 1e-9) * grid;
}

/** Grid choices for the piano roll, in beats. */
export const GRIDS: { label: string; beats: number }[] = [
  { label: "1 bar", beats: 0 },
  { label: "1/4 (beat)", beats: 1 },
  { label: "1/8", beats: 0.5 },
  { label: "1/16", beats: 0.25 },
  { label: "1/32", beats: 0.125 },
  { label: "1/8 triplet", beats: 1 / 3 },
  { label: "1/16 triplet", beats: 1 / 6 },
  { label: "Off", beats: -1 },
];

/** The grid size in beats: "1 bar" is the bar length (where `at` beats falls), "Off" is none. */
export function gridBeats(grid: number, t: TempoDoc, at = 0): number {
  if (grid === 0) return barBeats(segmentAtBeat(t, at));
  return grid < 0 ? 0 : grid;
}

const NAMES = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

/** Note name with octave: 60 is C4 (middle C). */
export function noteName(pitch: number): string {
  return `${NAMES[((pitch % 12) + 12) % 12]}${Math.floor(pitch / 12) - 1}`;
}

export const isBlackKey = (pitch: number) => [1, 3, 6, 8, 10].includes(((pitch % 12) + 12) % 12);

/** A new empty clip one bar long, starting at the bar that holds `atBeats`. */
export function newClip(id: string, atBeats: number, t: TempoDoc, name = "MIDI"): MidiClipDoc {
  const { bar } = barAt(t, Math.max(0, atBeats));
  const start = barStart(t, bar);
  return { id, name, start, length: barStart(t, bar + 1) - start, notes: [] };
}

/** The clip under a timeline position in beats, latest-starting first. */
export function clipAtBeat(clips: MidiClipDoc[], beat: number): MidiClipDoc | null {
  let best: MidiClipDoc | null = null;
  for (const c of clips) {
    if (beat >= c.start && beat < c.start + c.length && (!best || c.start >= best.start)) best = c;
  }
  return best;
}

const byStart = (a: { start: number }, b: { start: number }) => a.start - b.start;

/** Clips in timeline order. */
export function sorted(clips: MidiClipDoc[]): MidiClipDoc[] {
  return [...clips].sort(byStart);
}

/** Moves the clips `ids` by `delta` beats, none before the start. */
export function moveClips(clips: MidiClipDoc[], ids: ReadonlySet<string>, delta: number): MidiClipDoc[] {
  const moving = clips.filter((c) => ids.has(c.id));
  const first = Math.min(...moving.map((c) => c.start));
  const d = Math.max(-first, delta);
  return sorted(clips.map((c) => (ids.has(c.id) ? { ...c, start: c.start + d } : c)));
}

/**
 * Moves (or with `copy`, copies under new ids) the clips `ids` from one
 * track's clips to another's, `delta` beats later, none before the
 * start. Their notes go along; the other track's instrument plays them.
 */
export function transferClips(
  from: MidiClipDoc[],
  to: MidiClipDoc[],
  ids: ReadonlySet<string>,
  delta: number,
  copy: boolean,
  newId: () => string,
): { from: MidiClipDoc[]; to: MidiClipDoc[] } {
  const moving = from.filter((c) => ids.has(c.id));
  if (moving.length === 0) return { from, to };
  const d = Math.max(-Math.min(...moving.map((c) => c.start)), delta);
  const placed = moving.map((c) => ({ ...(JSON.parse(JSON.stringify(c)) as MidiClipDoc), id: copy ? newId() : c.id, start: c.start + d }));
  return {
    from: copy ? from : from.filter((c) => !ids.has(c.id)),
    to: sorted([...to, ...placed]),
  };
}

/** Sets a clip's length (at least `min` beats). Notes past the end stay, unheard. */
export function resizeClip(clips: MidiClipDoc[], id: string, length: number, min = 0.25): MidiClipDoc[] {
  return clips.map((c) => (c.id === id ? { ...c, length: Math.max(min, length) } : c));
}

export function removeClips(clips: MidiClipDoc[], ids: ReadonlySet<string>): MidiClipDoc[] {
  return clips.filter((c) => !ids.has(c.id));
}

/** Copies of the clips `ids`, right after the last of them, with new ids from `newId`. */
export function duplicateClips(clips: MidiClipDoc[], ids: ReadonlySet<string>, newId: () => string): { clips: MidiClipDoc[]; added: string[] } {
  const picked = clips.filter((c) => ids.has(c.id));
  if (picked.length === 0) return { clips, added: [] };
  const from = Math.min(...picked.map((c) => c.start));
  const to = Math.max(...picked.map((c) => c.start + c.length));
  const copies = picked.map((c) => ({ ...c, id: newId(), start: c.start + (to - from), notes: c.notes.map((n) => ({ ...n })) }));
  return { clips: sorted([...clips, ...copies]), added: copies.map((c) => c.id) };
}

export function replaceClip(clips: MidiClipDoc[], clip: MidiClipDoc): MidiClipDoc[] {
  return clips.map((c) => (c.id === clip.id ? clip : c));
}

/** Shortest note the piano roll makes, in beats. */
export const MIN_NOTE = 1 / 64;

const clampPitch = (p: number) => Math.min(127, Math.max(0, Math.round(p)));

/** Notes in time order, then low to high. */
export function sortNotes(notes: NoteDoc[]): NoteDoc[] {
  return [...notes].sort((a, b) => a.start - b.start || a.pitch - b.pitch);
}

export function addNote(notes: NoteDoc[], n: NoteDoc): NoteDoc[] {
  return sortNotes([...notes, { ...n, pitch: clampPitch(n.pitch), start: Math.max(0, n.start), length: Math.max(MIN_NOTE, n.length) }]);
}

/** Moves the notes at `indexes` by `beats` and `semitones`, kept inside the clip's start and the pitch range. */
export function moveNotes(notes: NoteDoc[], indexes: ReadonlySet<number>, beats: number, semitones: number): NoteDoc[] {
  const picked = notes.filter((_, i) => indexes.has(i));
  if (picked.length === 0) return notes;
  const earliest = Math.min(...picked.map((n) => n.start));
  const low = Math.min(...picked.map((n) => n.pitch));
  const high = Math.max(...picked.map((n) => n.pitch));
  const db = Math.max(-earliest, beats);
  const ds = Math.min(127 - high, Math.max(-low, Math.round(semitones)));
  return notes.map((n, i) => (indexes.has(i) ? { ...n, start: n.start + db, pitch: n.pitch + ds } : n));
}

/** Changes the length of the notes at `indexes` by `beats` (each at least MIN_NOTE). */
export function resizeNotes(notes: NoteDoc[], indexes: ReadonlySet<number>, beats: number): NoteDoc[] {
  return notes.map((n, i) => (indexes.has(i) ? { ...n, length: Math.max(MIN_NOTE, n.length + beats) } : n));
}

export function removeNotes(notes: NoteDoc[], indexes: ReadonlySet<number>): NoteDoc[] {
  return notes.filter((_, i) => !indexes.has(i));
}

export function setVelocity(notes: NoteDoc[], indexes: ReadonlySet<number>, velocity: number): NoteDoc[] {
  const v = Math.min(127, Math.max(1, Math.round(velocity)));
  return notes.map((n, i) => (indexes.has(i) ? { ...n, velocity: v } : n));
}

/** The lowest and highest pitch in use, or a two-octave range around middle C. */
export function pitchRange(notes: NoteDoc[]): [number, number] {
  if (notes.length === 0) return [48, 72];
  return [Math.min(...notes.map((n) => n.pitch)), Math.max(...notes.map((n) => n.pitch))];
}

/**
 * The edits that delete the selected clips, audio and MIDI alike: one
 * `remove_clips` for the audio ones and one `set_midi_clips` per MIDI
 * track touched.
 */
export function deleteClipsOps(doc: SessionDoc, ids: ReadonlySet<string>): Op[] {
  const ops: Op[] = [];
  const audio = doc.tracks.flatMap((t) => t.clips.filter((c) => ids.has(c.id)).map((c) => c.id));
  if (audio.length) ops.push({ op: "remove_clips", ids: audio });
  for (const t of doc.tracks) {
    const clips = t.midi_clips ?? [];
    if (clips.some((c) => ids.has(c.id))) ops.push({ op: "set_midi_clips", track_id: t.id, clips: removeClips(clips, ids) });
  }
  return ops;
}

/** Every MIDI clip id in the session. */
export const allMidiClipIds = (doc: SessionDoc) => doc.tracks.flatMap((t) => (t.midi_clips ?? []).map((c) => c.id));

/**
 * Moves the start of the notes at `indexes` by `beats`, keeping their
 * ends where they are (each stays at least MIN_NOTE long and starts at 0
 * or later).
 */
export function resizeNotesStart(notes: NoteDoc[], indexes: ReadonlySet<number>, beats: number): NoteDoc[] {
  return notes.map((n, i) => {
    if (!indexes.has(i)) return n;
    const end = n.start + n.length;
    const start = Math.min(end - MIN_NOTE, Math.max(0, n.start + beats));
    return { ...n, start, length: end - start };
  });
}

/** The notes at `indexes`, moved so the earliest starts at 0: what Copy keeps. */
export function copyNotes(notes: NoteDoc[], indexes: ReadonlySet<number>): NoteDoc[] {
  const picked = notes.filter((_, i) => indexes.has(i));
  const first = Math.min(...picked.map((n) => n.start));
  return picked.map((n) => ({ ...n, start: n.start - first }));
}

/** How long a copied set of notes runs, in beats. */
export const spanOf = (copied: NoteDoc[]) => Math.max(0, ...copied.map((n) => n.start + n.length));

/** Pastes `copied` (from copyNotes) at `at` beats. Returns the notes and the pasted ones' indexes. */
export function pasteNotes(notes: NoteDoc[], copied: NoteDoc[], at: number): { notes: NoteDoc[]; pasted: Set<number> } {
  const added = copied.map((n) => ({ ...n, start: n.start + Math.max(0, at) }));
  const all = sortNotes([...notes, ...added]);
  return { notes: all, pasted: new Set(added.map((a) => all.indexOf(a))) };
}
