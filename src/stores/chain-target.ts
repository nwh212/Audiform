// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Whose FX Chain the FX Chain panel shows on its Track Effects tab:
 * a track id, a bus id, the master's id, or null (clip effects, the
 * Waveform editor). The Editor marks that row, so it is always clear
 * where effects go.
 */

import { writable } from "svelte/store";

export const chainTarget = writable<string | null>(null);
