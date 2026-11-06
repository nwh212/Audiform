// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Audiobook delivery check (ACX style): the three numbers narration is
 * judged on besides file format.
 *
 * - Average level (RMS) between -23 and -18 dBFS.
 * - Peaks no higher than -3 dBFS.
 * - Noise floor, the room tone between lines, no louder than -60 dBFS RMS.
 *
 * The figures follow the published ACX audio submission requirements
 * as repeated on common guides; confirm them against ACX's own page
 * before relying on a pass for a submission. A pass here is a good
 * sign, not a guarantee: ACX also reviews by ear.
 */

import type { Grade, LoudnessReport } from "./loudness";

export const ACX = { rmsMin: -23, rmsMax: -18, peakMax: -3, floorMax: -60 } as const;

/** The middle of the RMS band, where gain advice aims. */
const RMS_AIM = (ACX.rmsMin + ACX.rmsMax) / 2;

export interface DeliveryRule {
  id: "rms" | "peak" | "floor";
  name: string;
  /** The measured value in dB, or null when it could not be measured. */
  value: number | null;
  /** The limit in words, for the card. */
  limit: string;
  grade: Grade;
}

export interface DeliveryVerdict {
  rules: DeliveryRule[];
  /** Every rule passes and the file has no digital silence. */
  pass: boolean;
  /** Master gain that would put the average level mid-band, dB; null when not measurable. */
  gainToTarget: number | null;
  /** That gain keeps peaks at or under the limit. */
  gainIsSafe: boolean;
  /** What to do next, most important first. */
  advice: string[];
}

const round1 = (v: number) => Math.round(v * 10) / 10;

/** Grades a value against a band: inside passes, within 1 dB outside is close. */
function band(v: number, lo: number, hi: number): Grade {
  if (v >= lo && v <= hi) return "pass";
  return v >= lo - 1 && v <= hi + 1 ? "close" : "fail";
}

export function acxCheck(r: LoudnessReport): DeliveryVerdict {
  const rms = Number.isFinite(r.rms) ? r.rms : null;
  const floor = r.noiseFloor ?? null;
  const peak = Number.isFinite(r.samplePeak) ? r.samplePeak : null;
  const rules: DeliveryRule[] = [
    { id: "rms", name: "Average level (RMS)", value: rms, limit: `${ACX.rmsMin} to ${ACX.rmsMax} dBFS`, grade: rms === null ? "fail" : band(rms, ACX.rmsMin, ACX.rmsMax) },
    { id: "peak", name: "Peak", value: peak, limit: `${ACX.peakMax} dBFS or lower`, grade: peak === null ? "fail" : band(peak, -Infinity, ACX.peakMax) },
    { id: "floor", name: "Noise floor", value: floor, limit: `${ACX.floorMax} dBFS or lower`, grade: floor === null ? "fail" : band(floor, -Infinity, ACX.floorMax) },
  ];
  const silent = (r.silentSeconds ?? 0) > 0.05;
  const gain = rms === null ? null : round1(RMS_AIM - rms);
  const gainIsSafe = gain !== null && peak !== null && peak + gain <= ACX.peakMax;
  const advice: string[] = [];
  const by = (id: DeliveryRule["id"]) => rules.find((x) => x.id === id)!;
  if (rms === null) advice.push("The file is silent or too short to measure.");
  else if (by("rms").grade !== "pass") {
    const dir = gain! > 0 ? "Raise" : "Lower";
    advice.push(
      gainIsSafe
        ? `${dir} the Master ${Math.abs(gain!).toFixed(1)} dB to bring the average level to about ${RMS_AIM} dBFS.`
        : `${gain! > 0 ? "Raising" : "Lowering"} the level ${Math.abs(gain!).toFixed(1)} dB would push peaks over ${ACX.peakMax} dBFS. Compress and limit the voice first (the Master's Audio FX with a Limiter near ${ACX.peakMax - 0.5} dBFS), then check again.`,
    );
  }
  if (peak !== null && by("peak").grade !== "pass") advice.push(`Peaks reach ${peak.toFixed(1)} dBFS. Limit the loudest moments to ${ACX.peakMax - 0.5} dBFS or lower. Lowering peaks also lowers the average, so check the level again.`);
  if (floor === null) advice.push("No quiet gap was found to measure the noise floor. Leave a second or so of room tone before the first line and after the last.");
  else if (by("floor").grade !== "pass") advice.push(`The room tone sits at ${floor.toFixed(1)} dBFS. Learn a noise print from a quiet stretch and use the Noise Reducer, or re-record somewhere quieter.`);
  if (silent) advice.push("The file has stretches of digital silence. Replace them with room tone (copy a quiet stretch of the same room over them); silence that is truly empty is rejected.");
  if (!advice.length) advice.push("Levels meet the audiobook numbers. Export as MP3, 192 kbps constant bit rate, 44.1 kHz.");
  return { rules, pass: rules.every((x) => x.grade === "pass") && !silent, gainToTarget: gain, gainIsSafe, advice };
}
