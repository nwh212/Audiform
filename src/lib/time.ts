// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Time display helpers. Positions are stored as sample counts in the core.
 * The default display is Decimal: m:ss.ddd, or h:mm:ss.ddd past one hour.
 */

/** Convert a sample position to seconds. */
export function samplesToSeconds(samples: number, sampleRate: number): number {
  if (!Number.isFinite(sampleRate) || sampleRate <= 0) {
    throw new RangeError(`Invalid sample rate: ${sampleRate}`);
  }
  return samples / sampleRate;
}

/** Format a sample position as Decimal time, for example 1:05.250. */
export function formatDecimal(samples: number, sampleRate: number): string {
  const sign = samples < 0 ? "-" : "";
  // Round to whole milliseconds first so 59.9996 s shows as 1:00.000.
  const totalMs = Math.round(Math.abs(samplesToSeconds(samples, sampleRate)) * 1000);
  const ms = totalMs % 1000;
  const totalSeconds = Math.floor(totalMs / 1000);
  const s = totalSeconds % 60;
  const totalMinutes = Math.floor(totalSeconds / 60);
  const m = totalMinutes % 60;
  const h = Math.floor(totalMinutes / 60);

  const msText = String(ms).padStart(3, "0");
  const sText = String(s).padStart(2, "0");
  if (h > 0) {
    return `${sign}${h}:${String(m).padStart(2, "0")}:${sText}.${msText}`;
  }
  return `${sign}${m}:${sText}.${msText}`;
}
