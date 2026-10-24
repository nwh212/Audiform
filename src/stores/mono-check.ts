// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

/**
 * The master's Mono button: hear the whole mix folded to mono, to check
 * how it holds up. Listening only, so it is not saved and never changes
 * an export (Export's own Mono choice writes a mono file).
 */

import { writable, type Readable } from "svelte/store";
import { setMonoCheck } from "../app/core";

const state = writable(false);
let on = false;

export const monoCheck: Readable<boolean> & { toggle(): Promise<void> } = {
  subscribe: state.subscribe,
  async toggle() {
    const next = !on;
    await setMonoCheck(next);
    on = next;
    state.set(next);
  },
};
