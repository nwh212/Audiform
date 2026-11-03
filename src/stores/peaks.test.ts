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
        return { sampleRate: 48000, channels: 1, length: 512, levels: [{ block: 32, count: 16 }, { block: 256, count: 2 }] };
      },
      level: async (_path, level) => (level === 1 ? new Int16Array([-1, 1, -2, 2]).buffer : new Int16Array(32).fill(3).buffer),
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
    // Only the coarsest level came with the summary.
    expect(a.levels.map((l) => l.block)).toEqual([256]);
    expect(Array.from(a.levels[0]!.data)).toEqual([-1, 1, -2, 2]);
    expect(store.get("bad.wav")).toMatchObject({ status: "error", error: "Codec not supported" });
    expect(changes).toBeGreaterThanOrEqual(2);
  });

  const backend = (loaded: string[]) => ({
    ensure: async () => ({ sampleRate: 48000, channels: 1, length: 4096, levels: [{ block: 32, count: 128 }, { block: 256, count: 16 }, { block: 4096, count: 1 }] }),
    level: async (path: string, level: number) => {
      loaded.push(`${path}#${level}`);
      // Finest level largest: 8, 4 and 2 bytes.
      return new Int16Array([4, 2, 1][level]!).buffer;
    },
    onProgress: async () => () => {},
  });
  const settle = async () => {
    for (let i = 0; i < 4; i++) await new Promise((r) => setTimeout(r, 0));
  };

  it("loads finer levels only when the zoom asks for them, and stands in with a coarser one meanwhile", async () => {
    const loaded: string[] = [];
    const store = new PeakStore(backend(loaded), 2);
    store.get("a.wav");
    await settle();
    expect(loaded).toEqual(["a.wav#2"]);
    // Zoomed out far: the coarsest level is the right one, nothing more to load.
    expect(store.level("a.wav", 10_000)?.block).toBe(4096);
    expect(loaded).toEqual(["a.wav#2"]);
    // Zoomed in: the fine level is asked for; the coarse one draws until it lands.
    expect(store.level("a.wav", 40)?.block).toBe(4096);
    await settle();
    expect(loaded).toEqual(["a.wav#2", "a.wav#0"]);
    expect(store.level("a.wav", 40)?.block).toBe(32);
    // In between: the middle level.
    expect(store.level("a.wav", 300)?.block).toBe(32);
    await settle();
    expect(store.level("a.wav", 300)?.block).toBe(256);
  });

  it("drops the least recently drawn fine levels past the budget, never the coarsest", async () => {
    const loaded: string[] = [];
    // Budget: the two coarsest levels (2 bytes each) plus one finest (8 bytes).
    const store = new PeakStore(backend(loaded), 2, 12);
    store.get("a.wav");
    store.get("b.wav");
    await settle();
    store.level("a.wav", 40);
    await settle();
    expect(store.get("a.wav").levels.map((l) => l.block).sort()).toEqual([32, 4096].sort());
    store.level("b.wav", 40);
    await settle();
    expect(store.get("b.wav").levels.map((l) => l.block).sort()).toEqual([32, 4096].sort());
    expect(store.get("a.wav").levels.map((l) => l.block)).toEqual([4096]);
    expect(store.held).toBeLessThanOrEqual(12);
  });
});
