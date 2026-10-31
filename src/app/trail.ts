// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * The last few things done in the app (menu actions, edits, settings
 * changes), kept in memory only, so an error that comes with no useful
 * detail of its own can say what led up to it. Nothing leaves the
 * computer: the steps go into the status bar message and the audio log.
 */

const MAX = 12;
const steps: string[] = [];

/** Notes one step, with the time of day. */
export function noteStep(step: string): void {
  const t = new Date();
  const hh = String(t.getHours()).padStart(2, "0");
  const mm = String(t.getMinutes()).padStart(2, "0");
  const ss = String(t.getSeconds()).padStart(2, "0");
  steps.push(`${hh}:${mm}:${ss} ${step}`);
  if (steps.length > MAX) steps.shift();
}

/** The steps noted, oldest first. */
export function recentSteps(): string[] {
  return [...steps];
}

/** Errors whose own text does not say where they came from. */
export function needsSteps(message: string): boolean {
  return /effect_update_depth|update depth/i.test(message);
}
