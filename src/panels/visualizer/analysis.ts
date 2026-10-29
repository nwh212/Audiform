// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Turns the spectrum the core reports (levels in dBFS, 20 Hz to 20 kHz,
 * even in pitch) into what the pictures need: levels from 0 to 1, how
 * much bass, middle and treble there is, the overall level, and a beat
 * when the bass jumps above where it has been.
 */

export interface Analysis {
  /** 0 to 1 per band, low to high. */
  levels: Float32Array;
  bass: number;
  mid: number;
  treble: number;
  level: number;
  /** True for the one picture where the bass jumped. */
  beat: boolean;
  /** Seconds since the pictures began. */
  time: number;
  /** Seconds since the last picture. */
  dt: number;
}

export const BANDS = 64;

/** dBFS to 0..1: -90 dB is nothing, 0 dB is full; louder than quiet reads as more than a straight line. */
export function levelOf(db: number, sensitivity: number): number {
  if (!Number.isFinite(db)) return 0;
  const x = Math.max(0, Math.min(1, (db + 90) / 90));
  return Math.min(1, Math.pow(x, 1.6) * sensitivity * 1.25);
}

function mean(a: Float32Array, from: number, to: number): number {
  let s = 0;
  for (let i = from; i < to; i++) s += a[i] ?? 0;
  return to > from ? s / (to - from) : 0;
}

export class Analyser {
  private bassAverage = 0;
  private sinceBeat = 1;
  private time = 0;
  private smooth = new Float32Array(BANDS);

  /** `db` may be empty (nothing playing): everything reads as silence. `calm` (0 to 1) scales it down, for when stopped. */
  feed(db: Float32Array, sensitivity: number, dt: number, calm = 1): Analysis {
    this.time += dt;
    this.sinceBeat += dt;
    const levels = new Float32Array(BANDS);
    for (let i = 0; i < BANDS; i++) {
      const raw = levelOf(db[i] ?? -120, sensitivity) * calm;
      // Rise fast, fall slowly, so pictures breathe instead of flicker.
      const prev = this.smooth[i] ?? 0;
      const next = raw > prev ? prev + (raw - prev) * 0.7 : prev + (raw - prev) * Math.min(1, dt * 6);
      this.smooth[i] = next;
      levels[i] = next;
    }
    const bass = mean(levels, 0, 10);
    const mid = mean(levels, 10, 36);
    const treble = mean(levels, 36, BANDS);
    const level = mean(levels, 0, BANDS);
    this.bassAverage += (bass - this.bassAverage) * Math.min(1, dt * 1.5);
    let beat = false;
    if (calm >= 0.5 && bass > 0.22 && bass > this.bassAverage * 1.3 + 0.05 && this.sinceBeat > 0.2) {
      beat = true;
      this.sinceBeat = 0;
    }
    return { levels, bass, mid, treble, level, beat, time: this.time, dt };
  }
}
