// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Built-in presets for the Band EQ, the same set for 10, 20 and 30 bands. Each is written
 * once as a curve, a few (frequency, dB) points, and each band takes the curve's level at
 * its centre, read between the points evenly in octaves (as the window's tabs carry a curve
 * from one band count to another, `carryBands`). Our own starting points, named for the job.
 */

/** Band centres per band count, as the core has them (`src-tauri/src/dsp/effects/filters.rs`). */
const OCTAVE = [31.5, 63, 125, 250, 500, 1000, 2000, 4000, 8000, 16_000];
const THIRD = [
  25, 31.5, 40, 50, 63, 80, 100, 125, 160, 200, 250, 315, 400, 500, 630, 800, 1000, 1250, 1600, 2000, 2500, 3150, 4000,
  5000, 6300, 8000, 10_000, 12_500, 16_000, 20_000,
];
export function bandCentres(n: 10 | 20 | 30): number[] {
  if (n === 10) return OCTAVE;
  if (n === 30) return THIRD;
  return Array.from({ length: 20 }, (_, k) => 31.5 * 2 ** (k / 2));
}

/** A curve: (Hz, dB) points, lowest first. Below the first and above the last it holds level. */
export type Curve = readonly (readonly [number, number])[];

export function curveAt(curve: Curve, hz: number): number {
  if (curve.length === 0) return 0;
  if (hz <= curve[0]![0]) return curve[0]![1];
  for (let i = 1; i < curve.length; i++) {
    const [fa, ga] = curve[i - 1]!;
    const [fb, gb] = curve[i]!;
    if (hz <= fb) return ga + ((gb - ga) * Math.log(hz / fa)) / Math.log(fb / fa);
  }
  return curve[curve.length - 1]![1];
}

/** The band settings (`g01`…) for a curve, to the slider's 0.1 dB step, flat bands left out. */
export function bandParams(curve: Curve, n: 10 | 20 | 30): Record<string, number> {
  const out: Record<string, number> = {};
  bandCentres(n).forEach((hz, i) => {
    const v = Math.round(curveAt(curve, hz) * 10) / 10;
    if (v !== 0) out[`g${String(i + 1).padStart(2, "0")}`] = Math.max(-15, Math.min(15, v));
  });
  return out;
}

export interface BandPreset {
  name: string;
  curve: Curve;
}

// The first four were the 10-band presets before there was a curve for them: their points sit
// on the 10-band centres, so the 10-band settings are unchanged.
export const BAND_EQ_PRESETS: readonly BandPreset[] = [
  { name: "Smile Curve", curve: [[31.5, 4], [63, 3], [125, 1], [250, 0], [500, -1], [1000, -1], [2000, 0], [4000, 1], [8000, 3], [16_000, 4]] },
  { name: "Bass Boost", curve: [[31.5, 5], [63, 4], [125, 2], [250, 1], [500, 0]] },
  { name: "Treble Lift", curve: [[1000, 0], [2000, 1], [4000, 2], [8000, 3], [16_000, 4]] },
  { name: "Mud Cut", curve: [[63, 0], [125, -3], [250, -3], [500, -1], [1000, 0]] },
  // Lows.
  { name: "Sub Bass Boost", curve: [[31.5, 5], [63, 3], [125, 0]] },
  { name: "Low-End Punch", curve: [[40, 1], [63, 4], [100, 3], [160, 0], [315, -2], [630, 0]] },
  { name: "Bass Cut", curve: [[31.5, -9], [63, -6], [125, -3], [250, -1], [500, 0]] },
  // Mids.
  { name: "Mid Lift", curve: [[125, 0], [250, 1], [500, 3], [1000, 4], [2000, 3], [4000, 1], [8000, 0]] },
  { name: "Mid Scoop", curve: [[125, 0], [250, -1], [500, -3], [1000, -4], [2000, -3], [4000, -1], [8000, 0]] },
  { name: "Presence Lift", curve: [[1000, 0], [2000, 2], [3150, 3.5], [5000, 2.5], [8000, 0]] },
  { name: "Boxiness Cut", curve: [[200, 0], [400, -2], [630, -3], [1000, -2], [2000, 0]] },
  // Highs.
  { name: "Air Lift", curve: [[4000, 0], [8000, 1.5], [12_500, 3.5], [16_000, 4]] },
  { name: "Treble Cut", curve: [[1000, 0], [2000, -1], [4000, -2.5], [8000, -4], [16_000, -6]] },
  // Voices.
  { name: "Vocal Clarity", curve: [[63, -4], [125, -2], [250, -1.5], [500, 0], [1000, 0], [2000, 1.5], [4000, 3], [8000, 2], [16_000, 1.5]] },
  { name: "Vocal Warmth", curve: [[63, -2], [125, 1.5], [250, 2.5], [500, 1], [1000, 0], [4000, 0], [8000, -1.5], [16_000, -2]] },
  { name: "Vocal De-Harsh", curve: [[1000, 0], [2000, -1], [3150, -3], [5000, -2], [8000, -0.5], [12_500, 0]] },
  { name: "Thin Voice, Fuller", curve: [[63, 0], [125, 3], [200, 3.5], [400, 1.5], [800, 0]] },
  // Instruments and the whole mix.
  { name: "Acoustic Guitar Sparkle", curve: [[63, -4], [125, -2], [250, -1.5], [500, 0], [2000, 0], [5000, 2], [10_000, 3], [16_000, 2]] },
  { name: "Loudness, Low Volume", curve: [[31.5, 5], [63, 4], [125, 2], [250, 0.5], [500, 0], [4000, 0], [8000, 1.5], [16_000, 3]] },
];

const KEYS = { 10: "band_eq_10", 20: "band_eq_20", 30: "band_eq_30" } as const;

/** Each preset for each band count: the 10-band one by its own name, the others with the count. */
export function bandEqPresets(): { name: string; effect: string; bands: 10 | 20 | 30; params: Record<string, number> }[] {
  return ([10, 20, 30] as const).flatMap((n) =>
    BAND_EQ_PRESETS.map((p) => ({
      name: n === 10 ? p.name : `${p.name} (${n} bands)`,
      effect: KEYS[n],
      bands: n,
      params: bandParams(p.curve, n),
    })),
  );
}
