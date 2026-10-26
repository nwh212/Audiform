// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/** Choices and edit building for the Session Properties dialog. */

import type { Op, SessionDoc } from "../stores/session";

export const SAMPLE_RATES = [22_050, 32_000, 44_100, 48_000, 88_200, 96_000] as const;

export const BIT_DEPTHS: { value: number; label: string }[] = [
  { value: 16, label: "16-bit" },
  { value: 24, label: "24-bit" },
  { value: 32, label: "32-bit float" },
];

export interface SessionProperties {
  name: string;
  sampleRate: number;
  bitDepth: number;
}

export function currentProperties(doc: SessionDoc): SessionProperties {
  return { name: doc.name, sampleRate: doc.sample_rate, bitDepth: doc.bit_depth };
}

/** The edit for the changed values, or null when nothing changed. */
export function propertiesOp(doc: SessionDoc, next: SessionProperties): Op | null {
  const ops: Op[] = [];
  const name = next.name.trim();
  const info: { op: "set_session_info"; name?: string; bit_depth?: number } = { op: "set_session_info" };
  if (name && name !== doc.name) info.name = name;
  if (next.bitDepth !== doc.bit_depth) info.bit_depth = next.bitDepth;
  if (info.name !== undefined || info.bit_depth !== undefined) ops.push(info);
  if (next.sampleRate !== doc.sample_rate) ops.push({ op: "set_sample_rate", sample_rate: next.sampleRate });
  if (ops.length === 0) return null;
  return ops.length === 1 ? ops[0]! : { op: "batch", ops };
}

/** Bytes for a mixdown of `samples` at the session rate. */
export function mixdownBytes(samples: number, bitDepth: number, channels = 2): number {
  return samples * channels * (bitDepth / 8);
}
