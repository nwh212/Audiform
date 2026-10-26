// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it, vi } from "vitest";
import { Preferences } from "./preferences";

describe("Preferences", () => {
  it("reads typed values with fallbacks", async () => {
    const p = new Preferences({ load: async () => ({ a: "x", b: true, c: 3, d: 5 }), save: async () => {} });
    await p.load();
    expect(p.string("a", "def")).toBe("x");
    expect(p.string("b", "def")).toBe("def");
    expect(p.boolean("b", false)).toBe(true);
    expect(p.number("c", 0)).toBe(3);
    expect(p.number("missing", 7)).toBe(7);
  });

  it("batches writes", async () => {
    vi.useFakeTimers();
    const saves: Record<string, unknown>[] = [];
    const p = new Preferences({ load: async () => ({}), save: async (v) => void saves.push(v) }, 100);
    p.set("x", 1);
    p.set("x", 2);
    p.set("y", "z");
    expect(saves).toHaveLength(0);
    await vi.advanceTimersByTimeAsync(150);
    expect(saves).toEqual([{ x: 2, y: "z" }]);
    vi.useRealTimers();
  });

  it("keeps values set before loading finished", async () => {
    let release!: (v: Record<string, unknown>) => void;
    const p = new Preferences({ load: () => new Promise((r) => (release = r)), save: async () => {} }, 10_000);
    const loading = p.load();
    p.set("k", "new");
    release({ k: "old", other: 1 });
    await loading;
    expect(p.string("k", "")).toBe("new");
    expect(p.number("other", 0)).toBe(1);
  });
});
