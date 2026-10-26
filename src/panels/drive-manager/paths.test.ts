// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { parentFolder, shortDate } from "./paths";

describe("parentFolder", () => {
  it("walks up Windows paths and stops at the drive", () => {
    expect(parentFolder("C:\\Shows\\Raw")).toBe("C:\\Shows");
    expect(parentFolder("C:\\Shows\\Raw\\")).toBe("C:\\Shows");
    expect(parentFolder("C:\\Shows")).toBe("C:\\");
    expect(parentFolder("C:\\")).toBeNull();
    expect(parentFolder("D:")).toBeNull();
  });

  it("walks up POSIX paths and stops at root", () => {
    expect(parentFolder("/home/host/Raw")).toBe("/home/host");
    expect(parentFolder("/home")).toBe("/");
    expect(parentFolder("/")).toBeNull();
    expect(parentFolder("")).toBeNull();
  });

  it("formats dates", () => {
    expect(shortDate(Date.UTC(2026, 9, 13, 12))).toMatch(/Oct 1[23], 2026/);
  });
});
