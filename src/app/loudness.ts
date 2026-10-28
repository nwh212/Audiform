// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Loudness targets and pass/fail for the Check Loudness card.
 *
 * Podcast targets follow common platform guidance: -16 LUFS integrated
 * for stereo, -19 LUFS for mono, true peaks at or below -1 dBTP. Spotify
 * normalizes podcasts nearer -14 LUFS. Broadcast depends on the region:
 * the US default is -24 LKFS with true peaks at -2 dBTP; Europe uses -23.
 * Mastering louder than the target only gets turned down by the player
 * and costs dynamic range.
 */

export interface LoudnessReport {
  integrated: number;
  integratedMono: number;
  range: number;
  truePeak: number;
  samplePeak: number;
  seconds: number;
  cancelled: boolean;
}

export interface LoudnessTarget {
  id: string;
  name: string;
  lufs: number;
  /** Measure the mix folded to mono (for mono shows). */
  mono: boolean;
  /** Highest allowed true peak, dBTP. */
  peak: number;
}

/** Broadcast loudness rules by region. The US is the default. */
export interface BroadcastRegion {
  id: string;
  name: string;
  /** The standard's name, shown next to the region. */
  standard: string;
  lufs: number;
  peak: number;
}

export const BROADCAST_REGIONS: readonly BroadcastRegion[] = [
  { id: "us", name: "United States", standard: "ATSC A/85", lufs: -24, peak: -2 },
  { id: "canada", name: "Canada", standard: "ATSC A/85", lufs: -24, peak: -2 },
  { id: "europe", name: "Europe", standard: "EBU R128", lufs: -23, peak: -1 },
  { id: "uk", name: "United Kingdom", standard: "EBU R128", lufs: -23, peak: -1 },
  { id: "australia", name: "Australia and New Zealand", standard: "OP-59", lufs: -24, peak: -2 },
  { id: "japan", name: "Japan", standard: "ARIB TR-B32", lufs: -24, peak: -1 },
];

export const DEFAULT_REGION = "us";

export function regionById(id: string | null | undefined): BroadcastRegion {
  return BROADCAST_REGIONS.find((r) => r.id === id) ?? BROADCAST_REGIONS[0]!;
}

/** A target of the user's own (Settings > Loudness). */
export interface CustomLoudness {
  lufs: number;
  peak: number;
}

let customTarget: CustomLoudness = { lufs: -18, peak: -1 };

/** Sets the custom target, from Settings. */
export function setCustomTarget(c: CustomLoudness): void {
  customTarget = { ...c };
}

/** The targets, with broadcast set by the chosen region, and the user's own last. */
export function targetsFor(regionId: string = DEFAULT_REGION, custom: CustomLoudness = customTarget): LoudnessTarget[] {
  const r = regionById(regionId);
  return [
    { id: "podcast-stereo", name: "Podcast, stereo (-16 LUFS)", lufs: -16, mono: false, peak: -1 },
    { id: "podcast-mono", name: "Podcast, mono (-19 LUFS)", lufs: -19, mono: true, peak: -1 },
    { id: "spotify", name: "Louder platforms (-14 LUFS)", lufs: -14, mono: false, peak: -1 },
    { id: "broadcast", name: `Broadcast, ${r.name} (${r.lufs} LUFS)`, lufs: r.lufs, mono: false, peak: r.peak },
    { id: "custom", name: `Custom (${custom.lufs} LUFS, ${custom.peak} dBTP)`, lufs: custom.lufs, mono: false, peak: custom.peak },
  ];
}

export const TARGETS: readonly LoudnessTarget[] = targetsFor(DEFAULT_REGION);

export const DEFAULT_TARGET = "podcast-stereo";

export function targetById(id: string, regionId: string = DEFAULT_REGION): LoudnessTarget {
  const list = targetsFor(regionId);
  return list.find((t) => t.id === id) ?? list[0]!;
}

/** Within this many LU of the target passes; within twice it is close. */
export const TOLERANCE_LU = 1;

export type Grade = "pass" | "close" | "fail";

export interface LoudnessVerdict {
  measured: number;
  /** Loudness minus target, LU. Positive is too loud. */
  offset: number;
  loudness: Grade;
  peak: Grade;
  /** Master gain change that would hit the target, dB. */
  gainToTarget: number;
  /** True when that gain would keep true peaks within the limit. */
  gainIsSafe: boolean;
  advice: string;
}

export function evaluate(r: LoudnessReport, t: LoudnessTarget): LoudnessVerdict {
  const measured = t.mono ? r.integratedMono : r.integrated;
  if (!Number.isFinite(measured)) {
    return { measured, offset: 0, loudness: "fail", peak: "pass", gainToTarget: 0, gainIsSafe: false, advice: "The mix is silent or too short to measure." };
  }
  const offset = Math.round((measured - t.lufs) * 10) / 10;
  const loudness: Grade = Math.abs(offset) <= TOLERANCE_LU ? "pass" : Math.abs(offset) <= 2 * TOLERANCE_LU ? "close" : "fail";
  const peak: Grade = r.truePeak <= t.peak ? "pass" : r.truePeak <= t.peak + 0.5 ? "close" : "fail";
  const gainToTarget = Math.round((t.lufs - measured) * 10) / 10;
  const peakAfter = r.truePeak + gainToTarget;
  const gainIsSafe = peakAfter <= t.peak;
  let advice: string;
  if (loudness === "pass" && peak === "pass") advice = "Ready to publish at this target.";
  else if (loudness !== "pass" && gainIsSafe)
    advice = `${gainToTarget > 0 ? "Raise" : "Lower"} the Master by ${Math.abs(gainToTarget).toFixed(1)} dB to hit the target.`;
  else if (loudness !== "pass")
    advice = `Raising the Master ${gainToTarget.toFixed(1)} dB would push peaks to ${peakAfter.toFixed(1)} dBTP. Limit the voice tracks harder first (a limiter around -3 to -6 dB on vocals), then check again.`;
  else advice = `Peaks reach ${r.truePeak.toFixed(1)} dBTP, over the ${t.peak} dBTP limit. Lower the loudest moments or limit them, then check again.`;
  return { measured, offset, loudness, peak, gainToTarget, gainIsSafe, advice };
}

export interface ExportPrediction {
  /** Gain the export applies to reach the target, dB. 0 when left as mixed. */
  gain: number;
  /** Loudness of the file, LUFS. */
  loudness: number;
  /** True peak of the file, dBTP. */
  truePeak: number;
  /** The gain stopped short of the target to keep peaks under its limit. */
  limitedByPeak: boolean;
  /** How far the file lands from the target, LU (negative is quieter). Null without a target. */
  offset: number | null;
  grade: Grade;
  /** One plain sentence. */
  summary: string;
}

/**
 * What an export will come out as, from a scan of the mix: with a target the
 * export moves the whole file by one fixed gain, but never past the target's
 * peak limit; without one it is the mix as it stands. `mono` is true when
 * the file is written as mono.
 */
export function predictExport(r: LoudnessReport, t: LoudnessTarget | null, mono = false): ExportPrediction {
  // The export measures what it writes: the folded mix only when the file is mono.
  const measured = mono ? r.integratedMono : r.integrated;
  if (!Number.isFinite(measured)) {
    return { gain: 0, loudness: measured, truePeak: r.truePeak, limitedByPeak: false, offset: null, grade: "fail", summary: "The mix is silent or too short to measure." };
  }
  if (!t) {
    return { gain: 0, loudness: measured, truePeak: r.truePeak, limitedByPeak: false, offset: null, grade: "pass", summary: `Exports as mixed: ${measured.toFixed(1)} LUFS, peaks ${r.truePeak.toFixed(1)} dBTP.` };
  }
  const want = t.lufs - measured;
  const room = t.peak - r.truePeak;
  const limitedByPeak = want > room;
  const gain = Math.round(Math.min(want, room) * 10) / 10;
  const loudness = measured + gain;
  const truePeak = r.truePeak + gain;
  const offset = Math.round((loudness - t.lufs) * 10) / 10;
  const grade: Grade = Math.abs(offset) <= TOLERANCE_LU ? "pass" : Math.abs(offset) <= 2 * TOLERANCE_LU ? "close" : "fail";
  const move = Math.abs(gain) < 0.05 ? "left as it is" : `${gain > 0 ? "raised" : "lowered"} ${Math.abs(gain).toFixed(1)} dB`;
  const base = `Measures ${measured.toFixed(1)} LUFS with peaks at ${r.truePeak.toFixed(1)} dBTP. The export is ${move} and lands at ${loudness.toFixed(1)} LUFS, peaks ${truePeak.toFixed(1)} dBTP.`;
  const why = limitedByPeak ? ` That is ${Math.abs(offset).toFixed(1)} LU under the ${t.lufs} target: the peaks reach the ${t.peak} dBTP limit first. Limit the loudest moments to get closer.` : grade === "pass" ? " On target." : "";
  return { gain, loudness, truePeak, limitedByPeak, offset, grade, summary: base + why };
}
