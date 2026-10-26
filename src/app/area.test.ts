// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { areaOf } from "./area";

describe("areaOf", () => {
  it("is null for anything that is not an element", () => {
    expect(areaOf(null)).toBeNull();
    expect(areaOf({} as EventTarget)).toBeNull();
  });
});
