// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

import { bars } from "./bars";
import { lava } from "./lava";
import { plasma } from "./plasma";
import { radial } from "./radial";
import { ripples } from "./ripples";
import { stars } from "./stars";
import { waves } from "./waves";
import type { Preset } from "./types";

/** In the order the list shows them. */
export const PRESETS: readonly Preset[] = [lava, bars, waves, plasma, ripples, stars, radial];

export function presetById(id: string): Preset {
  return PRESETS.find((p) => p.id === id) ?? PRESETS[0]!;
}
