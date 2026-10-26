// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { allOn, BOTH, channelLabel, editTake, oneChannel, pasteTake, soloChannel, someOff, toggleChannel } from "./wave-channels";

describe("editing some channels", () => {
  it("never switches every channel off", () => {
    const left = toggleChannel(BOTH, 1);
    expect(left).toEqual([true, false]);
    expect(toggleChannel(left, 0)).toEqual([false, true]);
    expect(toggleChannel(left, 1)).toEqual(BOTH);
    expect(soloChannel(BOTH, 1)).toEqual([false, true]);
    expect(soloChannel([false, true], 1)).toEqual(BOTH);
    expect(oneChannel(BOTH)).toBeNull();
    // Six channels: switching off the last one on brings the rest back.
    expect(toggleChannel(soloChannel(allOn(6), 2), 2)).toEqual([true, true, false, true, true, true]);
  });

  it("edits only the channels that are on", () => {
    expect(editTake(BOTH)).toBeNull();
    expect(editTake([true, false])).toEqual([
      { after: true, channel: 0 },
      { after: false, channel: 1 },
    ]);
    const surround = [false, false, true, true, false, false];
    expect(someOff(surround)).toBe(true);
    expect(oneChannel(surround)).toBeNull();
    expect(editTake(surround)!.map((t) => t.after)).toEqual(surround);
    expect(channelLabel(surround)).toBe(" (C, LFE)");
    expect(channelLabel([true, false])).toBe(" (Left Channel)");
  });

  it("pastes the left channel onto the right", () => {
    // Copied with only the left on, pasted with only the right on.
    expect(pasteTake([false, true], 0)).toEqual([
      { after: false, channel: 0 },
      { after: true, channel: 0 },
    ]);
    // A one-channel copy pasted with both on fills both.
    expect(pasteTake(BOTH, 0)).toEqual([
      { after: true, channel: 0 },
      { after: true, channel: 0 },
    ]);
    expect(pasteTake(BOTH, undefined)).toBeNull();
  });
});
