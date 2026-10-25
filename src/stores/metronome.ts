// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

/**
 * The metronome: its settings live with the app settings (kept between
 * runs), and the core is told what to play whenever they change or a
 * recording starts or stops (for "while recording only").
 */

import { setMetronome } from "../app/core";
import { settings, type MetronomePrefs } from "./settings";

let recordingNow = false;
let last = "";

function push(m: MetronomePrefs): void {
  const on = m.on && !m.muted && (m.when === "always" || recordingNow);
  const msg = { on, levelDb: m.levelDb, sound: m.sound, accent: m.accent, subdivide: m.subdivide };
  const key = JSON.stringify(msg);
  if (key === last) return;
  last = key;
  void setMetronome(msg).catch(() => {
    last = "";
  });
}

/** Starts telling the core about the metronome. Call once at startup. */
export function startMetronome(): () => void {
  return settings.subscribe((s) => push(s.metronome));
}

/** A recording is starting or has stopped. Call before playback starts, so the first click is not missed. */
export function metronomeRecording(on: boolean): void {
  recordingNow = on;
  push(settings.current.metronome);
}

/** Changes some of the metronome's settings (kept between runs). */
export function setMetronomePrefs(change: Partial<MetronomePrefs>): void {
  const c = settings.current;
  settings.set({ ...c, metronome: { ...c.metronome, ...change } });
}
