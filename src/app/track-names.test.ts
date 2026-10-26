// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { trackNameFromFile } from "./track-names";

describe("trackNameFromFile", () => {
  it("drops the extension and a recorder's speaker number", () => {
    expect(trackNameFromFile("1-host.flac")).toBe("host");
    expect(trackNameFromFile("C:\\calls\\12-guest_two.ogg")).toBe("guest two");
    expect(trackNameFromFile("2024 Interview.wav")).toBe("2024 Interview");
    expect(trackNameFromFile("3-.wav")).toBe("3-");
  });
});

import { newTrackNames } from "./track-names";

describe("newTrackNames", () => {
  it("continues Track N without clashing", () => {
    expect(newTrackNames(["Track 1", "Track 2", "Track 4"], 2)).toEqual(["Track 5", "Track 6"]);
  });
  it("uses a base name, then numbers it", () => {
    expect(newTrackNames(["Host"], 3, "Guest")).toEqual(["Guest", "Guest 2", "Guest 3"]);
    expect(newTrackNames(["Guest"], 2, "Guest")).toEqual(["Guest 2", "Guest 3"]);
  });
});
