// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Recorded MIDI: the notes played on an armed MIDI track while recording
 * become one clip, from the bar the first note starts in to the end of
 * the bar the last one ends in.
 */

import type { MidiTakeNote } from "./core";
import { MIN_NOTE, samplesToBeats } from "../panels/editor/midi";
import type { MidiClipDoc, NoteDoc, TempoDoc } from "../stores/session";
import { barAt, barStart } from "../stores/tempo";

/**
 * The clip for one track's take. `at` is the session sample recording
 * started at; with `punch`, only notes starting inside it are kept.
 * Null when no note is left.
 */
export function midiTakeClip(
  notes: MidiTakeNote[],
  at: number,
  rate: number,
  tempo: TempoDoc,
  punch: { start: number; end: number } | null,
  id: string,
  name: string,
): MidiClipDoc | null {
  const placed = notes
    .map((n) => ({ n, on: at + n.start * rate, off: at + n.end * rate }))
    .filter((x) => !punch || (x.on >= punch.start && x.on < punch.end))
    .map((x) => ({ n: x.n, on: samplesToBeats(x.on, tempo, rate), off: samplesToBeats(x.off, tempo, rate) }));
  if (!placed.length) return null;
  const first = Math.min(...placed.map((x) => x.on));
  const last = Math.max(...placed.map((x) => Math.max(x.off, x.on + MIN_NOTE)));
  const start = barStart(tempo, barAt(tempo, Math.max(0, first)).bar);
  const endBar = barAt(tempo, last);
  const end = endBar.beat < 1e-6 ? barStart(tempo, endBar.bar) : barStart(tempo, endBar.bar + 1);
  const out: NoteDoc[] = placed.map(({ n, on, off }) => ({
    pitch: Math.min(127, Math.max(0, n.pitch)),
    velocity: Math.min(127, Math.max(1, Math.round(n.velocity * 127))),
    start: on - start,
    length: Math.max(MIN_NOTE, off - on),
    ...(n.channel ? { channel: n.channel } : {}),
  }));
  out.sort((a, b) => a.start - b.start || a.pitch - b.pitch);
  return { id, name, start, length: Math.max(end - start, MIN_NOTE), notes: out };
}
