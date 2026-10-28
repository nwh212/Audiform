// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { onlyChannel, pickChannel, pickedChannel } from "./file-channels";

describe("one channel of a file", () => {
  it("remembers the pick by path and plays it on both sides", () => {
    pickChannel("/a.wav", 1);
    expect(pickedChannel("/a.wav", 2)).toBe(1);
    expect(pickedChannel("/a.wav", 1)).toBeNull();
    expect(onlyChannel(1, 2).sends).toEqual([{ left: 0, right: 0 }, { left: 1, right: 1 }]);
    pickChannel("/a.wav", null);
    expect(pickedChannel("/a.wav", 2)).toBeNull();
  });
});
