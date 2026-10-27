// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { addsOne, BUILTIN_PRESETS } from "./chain";

const KEYS = ["time", "feedback", "tone", "wet", "pingpong"];

describe("Delay presets", () => {
  const delay = BUILTIN_PRESETS.filter((p) => p.group === "Delay");

  it("offers four, each adding one Delay and Echo effect", () => {
    expect(delay.map((p) => p.name)).toEqual(["Slapback Echo", "Podcast Room Echo", "Ping-Pong Echo", "Long Dark Echo"]);
    for (const p of delay) {
      expect(addsOne(p)).toBe(true);
      expect(p.chain.slots[0]?.effect).toBe("altaudio.echo");
    }
  });

  it("only sets settings the effect has, inside its ranges", () => {
    for (const p of delay) {
      const params = p.chain.slots[0]!.params;
      for (const k of Object.keys(params)) expect(KEYS).toContain(k);
      expect(params.time).toBeGreaterThanOrEqual(1);
      expect(params.time).toBeLessThanOrEqual(2000);
      expect(params.feedback).toBeLessThanOrEqual(95);
      expect(params.tone).toBeGreaterThanOrEqual(500);
      expect(params.tone).toBeLessThanOrEqual(16000);
    }
  });

  it("has no repeated names across all built-in presets", () => {
    const names = BUILTIN_PRESETS.map((p) => p.name);
    expect(new Set(names).size).toBe(names.length);
  });
});
