// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { rawBytes } from "./core";


describe("rawBytes", () => {
  it("reads raw command bytes however the webview hands them back", () => {
    const src = new Uint8Array([2, 0, 0, 0, 0, 0, 128, 63]);
    const want = [...src];
    expect([...rawBytes(src.buffer)]).toEqual(want);
    expect([...rawBytes(new Uint8Array([9, ...src]).subarray(1))]).toEqual(want);
    expect([...rawBytes(want)]).toEqual(want);
    // The result owns its bytes, so typed views over it start at 0.
    expect(new DataView(rawBytes(want).buffer).getUint32(0, true)).toBe(2);
    expect(new Float32Array(rawBytes(want).buffer.slice(4))[0]).toBe(1);
  });
});
