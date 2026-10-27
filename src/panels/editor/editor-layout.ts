// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Sizes and small formatters shared by the Multitrack editor and its
 * track headers.
 */

import type { BusDoc } from "../../stores/session";
import { defaultTrackColor } from "./timeline";

/** Width of the track header column, in pixels. */
export const HEADER_W = 214;
/** Height of the time ruler, in pixels. */
export const RULER_H = 22;

/** Header heights at which each extra row shows. */
export const ROW_INPUT = 76;
export const ROW_OUTPUT = 98;
export const ROW_AUTO = 120;
export const ROW_SENDS = 146;

/** A bus row's height: never shorter than its controls. */
export const busHeight = (b: BusDoc) => Math.max(72, b.height);

/** A track's color for its header strip: its own color, or the palette by row. */
export function trackColor(c: string | null | undefined, row: number): string {
  const v = c ?? defaultTrackColor(row);
  return v.startsWith("--") ? `var(${v})` : v;
}

export const formatDb = (v: number) => (v > 0 ? `+${v.toFixed(1)}` : v.toFixed(1));
export const formatPan = (p: number) => (Math.round(p) === 0 ? "C" : p < 0 ? `L${Math.round(-p)}` : `R${Math.round(p)}`);
