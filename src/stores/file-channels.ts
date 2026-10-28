// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * One channel picked for a file in the Files panel (the left of a stereo
 * file, say). Clips made from the file play only that channel, on both
 * sides, and the Waveform editor opens it with only that channel on.
 * Kept by file path for this run of the app.
 */

import { get, writable } from "svelte/store";
import type { ChannelMixDoc } from "./session";

/** Picked channel by file path; files not listed use every channel. */
export const fileChannels = writable<Record<string, number>>({});

export function pickChannel(path: string, channel: number | null): void {
  fileChannels.update((m) => {
    const next = { ...m };
    if (channel === null) delete next[path];
    else next[path] = channel;
    return next;
  });
}

/** The channel picked for `path`, if it has that many channels. */
export function pickedChannel(path: string, channels: number): number | null {
  const k = get(fileChannels)[path];
  return k !== undefined && k < channels && channels > 1 ? k : null;
}

/** Source Channels that play only channel `k` of an `n`-channel file, on both sides. */
export function onlyChannel(k: number, n: number): ChannelMixDoc {
  return { sends: Array.from({ length: n }, (_, i) => (i === k ? { left: 1, right: 1 } : { left: 0, right: 0 })) };
}
