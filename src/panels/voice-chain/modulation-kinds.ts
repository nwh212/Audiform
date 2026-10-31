// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/** The Modulation window's tabs, and where each kind starts when picked. */
export const MOD_KINDS = ["Chorus", "Flanger", "Phaser", "Double Track"] as const;
export const MOD_TIPS = [
  "A few copies drifting in time and pitch: thicker, wider, shimmering",
  "A short swept delay fed back on itself: the jet sweep",
  "Moving notches from a chain of all-pass filters: the swirling sweep",
  "A second take of the same part, slightly late and never quite steady",
] as const;

/** The shared settings each kind starts from when its tab is picked. */
export const MOD_START: Record<number, Record<string, number>> = {
  0: { rate: 0.6, depth: 50, feedback: 0, wet: 50 },
  1: { rate: 0.25, depth: 70, feedback: 50, wet: 50 },
  2: { rate: 0.4, depth: 60, feedback: 30, wet: 100 },
  3: { rate: 0.8, depth: 50, feedback: 0, wet: 70 },
};

/** The settings shown under each kind, beyond the shared ones. */
export const MOD_OWN: Record<number, string[]> = {
  0: ["voices", "chorus_delay"],
  1: ["flange_delay"],
  2: ["stages", "center"],
  3: ["dt_delay", "wander"],
};
