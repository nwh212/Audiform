// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/** Plug-in state the interface shares: when the list changed, and which plug-ins stopped. */

import { writable } from "svelte/store";

/** Goes up after every scan, so effect lists load again. */
export const pluginsVersion = writable(0);

/** Plug-ins that crashed or hung, by slot id, with the reason. They are bypassed until restarted. */
export const stoppedPlugins = writable<ReadonlyMap<string, string>>(new Map());

export function markStopped(slotId: string, why: string | null): void {
  stoppedPlugins.update((m) => {
    const next = new Map(m);
    if (why) next.set(slotId, why);
    else next.delete(slotId);
    return next;
  });
}
