// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform.

/**
 * Controller moves in a MIDI clip: the sustain pedal (controller 64),
 * the mod wheel, pitch bend and the rest. The piano roll's lane draws
 * and edits them one controller at a time.
 */

import type { ControlDoc } from "../../stores/session";

/** Channel aftertouch and pitch bend use numbers past the 128 MIDI controllers. */
export const AFTERTOUCH = 128;
export const PITCH_BEND = 129;

/** The controllers offered in the lane picker, most used first. */
export const CONTROLLERS: { id: number; name: string }[] = [
  { id: 64, name: "Sustain Pedal (CC 64)" },
  { id: 1, name: "Mod Wheel (CC 1)" },
  { id: 11, name: "Expression (CC 11)" },
  { id: 7, name: "Volume (CC 7)" },
  { id: 10, name: "Pan (CC 10)" },
  { id: 2, name: "Breath (CC 2)" },
  { id: 66, name: "Sostenuto Pedal (CC 66)" },
  { id: 67, name: "Soft Pedal (CC 67)" },
  { id: PITCH_BEND, name: "Pitch Bend" },
  { id: AFTERTOUCH, name: "Aftertouch" },
];

/** The controller's name for the lane, by number. */
export function controllerName(id: number): string {
  return CONTROLLERS.find((c) => c.id === id)?.name ?? `CC ${id}`;
}

/** On/off controllers: pedals read below the middle as up and above it as down. */
export function isSwitch(id: number): boolean {
  return id >= 64 && id <= 69;
}

/** The value a lane height (0 bottom to 1 top) sets: pedals are up or down, the rest in MIDI's 128 steps (pitch bend finer). */
export function shapeValue(id: number, f: number): number {
  const v = Math.min(1, Math.max(0, f));
  if (isSwitch(id)) return v >= 0.5 ? 1 : 0;
  if (id === PITCH_BEND) return Math.round(v * 16383) / 16383;
  return Math.round(v * 127) / 127;
}

/** The value shown for a move, as MIDI players read it. */
export function valueText(id: number, value: number): string {
  if (isSwitch(id)) return value >= 0.5 ? "down" : "up";
  if (id === PITCH_BEND) return `${Math.round((value - 0.5) * 2 * 8192)}`;
  return `${Math.round(value * 127)}`;
}

/** One controller's moves, in time order. */
export function movesOf(controls: ControlDoc[] | undefined, id: number): ControlDoc[] {
  return (controls ?? []).filter((c) => c.controller === id).sort((a, b) => a.at - b.at);
}

function sorted(list: ControlDoc[]): ControlDoc[] {
  return list.sort((a, b) => a.at - b.at || a.controller - b.controller);
}

/**
 * Drawing from `from` to `to` beats: that controller's moves in between
 * give way to one at `to`. Pedals keep a move only where the value changes.
 */
export function paintControl(controls: ControlDoc[] | undefined, id: number, from: number, to: number, value: number, channel = 0): ControlDoc[] {
  const lo = Math.min(from, to) - 1e-6;
  const hi = Math.max(from, to) + 1e-6;
  const at = Math.max(0, to);
  let out = (controls ?? []).filter((c) => c.controller !== id || c.at < lo || c.at > hi);
  if (isSwitch(id)) {
    const before = movesOf(out, id).filter((c) => c.at < at).pop();
    if ((before?.value ?? 0) === value) return sorted(out);
  }
  out = [...out, { at, controller: id, value, ...(channel ? { channel } : {}) }];
  return sorted(out);
}

/** Removes that controller's moves between two beats. */
export function eraseControls(controls: ControlDoc[] | undefined, id: number, from: number, to: number): ControlDoc[] {
  const lo = Math.min(from, to) - 1e-6;
  const hi = Math.max(from, to) + 1e-6;
  return (controls ?? []).filter((c) => c.controller !== id || c.at < lo || c.at > hi);
}

/** The controllers a clip has moves for, for marking them in the picker. */
export function usedControllers(controls: ControlDoc[] | undefined): Set<number> {
  return new Set((controls ?? []).map((c) => c.controller));
}
