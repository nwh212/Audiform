// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Editing some channels of a file in the Waveform editor. Switch a
 * channel off with its button beside the level scale (or Edit > Activate
 * Left or Right Channel) and edits, effects, copy and paste touch only
 * the channels still on. The core renders such an edit and keeps the
 * other channels exactly as they were (`wave_channel_edit`). Works for
 * stereo and for multichannel files (5.1, 7.1.4 and so on).
 */

import { writable } from "svelte/store";
import { channelLabels } from "../properties/channel-mix";

/** Which channels edits reach, one flag per channel of the open file. Never all off. */
export type ActiveChannels = boolean[];

/** Every channel of an `n`-channel file on. */
export const allOn = (n: number): ActiveChannels => Array.from({ length: Math.max(1, n) }, () => true);

/** Both channels of a stereo file on. */
export const BOTH: ActiveChannels = allOn(2);

export const waveChannels = writable<ActiveChannels>(BOTH);

/** Where one output channel comes from: the file before or after the edit, and its channel. */
export interface ChannelSource {
  after: boolean;
  channel: number;
}
export type ChannelTake = ChannelSource[];

const isAll = (c: ActiveChannels) => c.every(Boolean);

/** Switches channel `k` on or off. Switching off the only one left switches the others on. */
export function toggleChannel(c: ActiveChannels, k: number): ActiveChannels {
  const next = c.map((on, i) => (i === k ? !on : on));
  return next.some(Boolean) ? next : c.map((_, i) => i !== k);
}

/** Channel `k` alone, or every channel again when it already is. */
export function soloChannel(c: ActiveChannels, k: number): ActiveChannels {
  const only = c.map((_, i) => i === k);
  return c.every((on, i) => on === only[i]) ? allOn(c.length) : only;
}

/** The one channel on, or null when more (or all) are. */
export function oneChannel(c: ActiveChannels): number | null {
  const on = c.flatMap((v, i) => (v ? [i] : []));
  return on.length === 1 && c.length > 1 ? on[0]! : null;
}

/** True when some channels are off, so edits reach only part of the file. */
export function someOff(c: ActiveChannels): boolean {
  return c.length > 1 && !isAll(c);
}

/** For an edit in place: channels that are on take the edited audio, the others keep their own. Null with all on. */
export function editTake(c: ActiveChannels): ChannelTake | null {
  if (isAll(c)) return null;
  return c.map((on, k) => ({ after: on, channel: k }));
}

/**
 * For a paste over the audio: each channel that is on takes the pasted
 * audio's `from` channel (its own when the copy had every channel), the
 * others keep their own. Null with all on and a full copy (an ordinary paste).
 */
export function pasteTake(c: ActiveChannels, from: number | undefined): ChannelTake | null {
  if (isAll(c) && from === undefined) return null;
  return c.map((on, k) => (on ? { after: true, channel: from ?? k } : { after: false, channel: k }));
}

/** " (Left Channel)", " (C, LFE)" and so on for edit labels, or "" with all on. */
export function channelLabel(c: ActiveChannels): string {
  if (isAll(c)) return "";
  if (c.length === 2) return c[0] ? " (Left Channel)" : " (Right Channel)";
  const names = channelLabels(c.length);
  return ` (${c.flatMap((on, k) => (on ? [names[k]] : [])).join(", ")})`;
}
