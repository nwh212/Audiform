// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { BOTH, editTake, oneChannel, pasteTake, soloChannel, toggleChannel } from "./wave-channels";

describe("one-channel editing", () => {
  it("never switches both channels off", () => {
    const left = toggleChannel(BOTH, "right");
    expect(left).toEqual({ left: true, right: false });
    expect(toggleChannel(left, "left")).toEqual({ left: false, right: true });
    expect(toggleChannel(left, "right")).toEqual(BOTH);
    expect(soloChannel(BOTH, "right")).toEqual({ left: false, right: true });
    expect(soloChannel({ left: false, right: true }, "right")).toEqual(BOTH);
    expect(oneChannel(BOTH)).toBeNull();
  });

  it("edits only the channel that is on", () => {
    expect(editTake(BOTH)).toBeNull();
    expect(editTake({ left: true, right: false })).toEqual([
      { after: true, channel: 0 },
      { after: false, channel: 1 },
    ]);
  });

  it("pastes the left channel onto the right", () => {
    // Copied with only the left on, pasted with only the right on.
    expect(pasteTake({ left: false, right: true }, 0)).toEqual([
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
