// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { writable } from "svelte/store";
import { shared } from "./fx-catalog";

describe("shared fetch", () => {
  it("fetches once per version and hands every caller the same value", async () => {
    const version = writable(0);
    let fetches = 0;
    const list = shared(async () => {
      fetches++;
      return [{ id: `v${fetches}` }];
    }, version);
    const [a, b] = await Promise.all([list(), list()]);
    expect(a).toBe(b);
    expect(fetches).toBe(1);
    expect(await list()).toBe(a);
    version.update((n) => n + 1);
    const c = await list();
    expect(c).not.toBe(a);
    expect(c[0]!.id).toBe("v2");
    expect(fetches).toBe(2);
  });

  it("tries again after a failed fetch", async () => {
    const version = writable(0);
    let calls = 0;
    const list = shared(async () => {
      calls++;
      if (calls === 1) throw new Error("not yet");
      return ["ok"];
    }, version);
    await expect(list()).rejects.toThrow("not yet");
    expect(await list()).toEqual(["ok"]);
    expect(calls).toBe(2);
  });
});
