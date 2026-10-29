// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/** The Reverb window's tabs: the made spaces, then the recorded one. */
export const REVERB_KINDS = ["Room", "Hall", "Plate", "Spring", "Tape", "Impulse Response"] as const;
export const REVERB_TIPS = [
  "A small room: strong early reflections and a short, natural tail",
  "A large hall: slow, soft and wide, with a long build",
  "A plate: no reflections, a dense, bright wash",
  "A spring: short and boingy, dark at both ends",
  "A ring of tape echoes that blur into a tail, with wow and saturation",
  "A recorded space: load any audio file of a room's, hall's or plate's echo",
] as const;

export const REVERB_ID = "altaudio.reverb";
export const IMPULSE_ID = "altaudio.convolution_reverb";

const toDb = (pct: number) => (pct <= 0 ? -48 : Math.max(-48, Math.min(6, 20 * Math.log10(pct / 100))));
const toPct = (db: number) => (db <= -48 ? 0 : Math.max(0, Math.min(100, 100 * Math.pow(10, db / 20))));

/** Settings that carry over when a Reverb slot becomes the Impulse Response one. */
export function toImpulse(params: Record<string, number>): Record<string, number> {
  const out: Record<string, number> = {};
  if (params.dry !== undefined) out.dry = toDb(params.dry);
  if (params.wet !== undefined) out.wet = toDb(params.wet);
  for (const k of ["predelay", "low_cut"]) if (params[k] !== undefined) out[k] = params[k]!;
  if (params.high_cut !== undefined) out.high_cut = params.high_cut;
  if (params.width !== undefined) out.width = params.width;
  return out;
}

/** And back: an Impulse Response slot becoming a Reverb of kind `kind`. */
export function fromImpulse(params: Record<string, number>, kind: number): Record<string, number> {
  const out: Record<string, number> = { kind };
  if (params.dry !== undefined) out.dry = toPct(params.dry);
  if (params.wet !== undefined) out.wet = toPct(params.wet);
  for (const k of ["predelay", "low_cut", "high_cut"]) if (params[k] !== undefined) out[k] = params[k]!;
  if (params.width !== undefined) out.width = Math.min(100, params.width);
  return out;
}
