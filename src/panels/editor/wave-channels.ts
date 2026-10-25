// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

/**
 * Editing one channel of a stereo file in the Waveform editor. Switch a
 * channel off with its button beside the level scale (or Edit > Activate
 * Left or Right Channel) and edits, effects, copy and paste touch only
 * the channel still on. The core renders such an edit and keeps the
 * other channel exactly as it was (`wave_channel_edit`).
 */

import { writable } from "svelte/store";

export interface ActiveChannels {
  left: boolean;
  right: boolean;
}

export const BOTH: ActiveChannels = { left: true, right: true };

/** Which channels edits reach. Never both off. */
export const waveChannels = writable<ActiveChannels>(BOTH);

/** Where one output channel comes from: the file before or after the edit, and its channel. */
export interface ChannelSource {
  after: boolean;
  channel: 0 | 1;
}
export type ChannelTake = [ChannelSource, ChannelSource];

/** Switches a channel on or off. Switching off the only one left switches the other on. */
export function toggleChannel(c: ActiveChannels, ch: "left" | "right"): ActiveChannels {
  const next = { ...c, [ch]: !c[ch] };
  if (!next.left && !next.right) return ch === "left" ? { left: false, right: true } : { left: true, right: false };
  return next;
}

/** Edit > Activate Left or Right Channel: that channel alone, or both again when it already is. */
export function soloChannel(c: ActiveChannels, ch: "left" | "right"): ActiveChannels {
  const only = ch === "left" ? { left: true, right: false } : { left: false, right: true };
  return c.left === only.left && c.right === only.right ? BOTH : only;
}

/** The one channel on (0 left, 1 right), or null when both are. */
export function oneChannel(c: ActiveChannels): 0 | 1 | null {
  if (c.left && c.right) return null;
  return c.left ? 0 : 1;
}

/** For an edit in place: the channel that is on takes the edited audio, the other keeps its own. Null with both on. */
export function editTake(c: ActiveChannels): ChannelTake | null {
  const one = oneChannel(c);
  if (one === null) return null;
  return [
    { after: one === 0, channel: 0 },
    { after: one === 1, channel: 1 },
  ];
}

/**
 * For a paste over the audio: each channel that is on takes the pasted
 * audio's `from` channel (its own when the copy had both), the other
 * keeps its own. Null with both on and a two-channel copy (an ordinary paste).
 */
export function pasteTake(c: ActiveChannels, from: 0 | 1 | undefined): ChannelTake | null {
  if (c.left && c.right && from === undefined) return null;
  const src = (ch: 0 | 1): ChannelSource => ((ch === 0 ? c.left : c.right) ? { after: true, channel: from ?? ch } : { after: false, channel: ch });
  return [src(0), src(1)];
}

/** "Left Channel" or "Right Channel" for edit labels, or "" with both on. */
export function channelLabel(c: ActiveChannels): string {
  const one = oneChannel(c);
  return one === null ? "" : one === 0 ? " (Left Channel)" : " (Right Channel)";
}
