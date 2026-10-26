// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Bumped whenever the theme or the color settings change, so views drawn
 * on a canvas (which read their colors once per draw) repaint at once.
 */

import { writable } from "svelte/store";

export const appearanceTick = writable(0);

export function appearanceChanged(): void {
  appearanceTick.update((n) => n + 1);
}
