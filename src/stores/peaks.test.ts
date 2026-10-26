// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { PeakStore } from "./peaks";

describe("PeakStore", () => {
  it("loads each file once and reports errors", async () => {
    const ensured: string[] = [];
    const store = new PeakStore({
      ensure: async (path) => {
        ensured.push(path);
        if (path === "bad.wav") throw new Error("Codec not supported");
        return { sampleRate: 48000, channels: 1, length: 512, levels: [{ block: 256, count: 2 }] };
      },
      level: async () => new Int16Array([-1, 1, -2, 2]).buffer,
      onProgress: async () => () => {},
    });
    let changes = 0;
    store.subscribe(() => changes++);
    expect(store.get("a.wav").status).toBe("building");
    store.get("a.wav");
    store.get("bad.wav");
    await new Promise((r) => setTimeout(r, 0));
    await new Promise((r) => setTimeout(r, 0));
    expect(ensured).toEqual(["a.wav", "bad.wav"]);
    const a = store.get("a.wav");
    expect(a.status).toBe("ready");
    expect(Array.from(a.levels[0]!.data)).toEqual([-1, 1, -2, 2]);
    expect(store.get("bad.wav")).toMatchObject({ status: "error", error: "Codec not supported" });
    expect(changes).toBeGreaterThanOrEqual(2);
  });
});
