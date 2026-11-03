// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

import { bars } from "./bars";
import { electric } from "./electric";
import { lava } from "./lava";
import { plasma } from "./plasma";
import { ripples } from "./ripples";
import { stars } from "./stars";
import { tunnel } from "./tunnel";
import { waves } from "./waves";
import type { Preset } from "./types";

/** In the order the list shows them. */
export const PRESETS: readonly Preset[] = [lava, plasma, tunnel, electric, bars, waves, ripples, stars];

export function presetById(id: string): Preset {
  return PRESETS.find((p) => p.id === id) ?? PRESETS[0]!;
}
