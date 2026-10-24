// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

import { describe, expect, it } from "vitest";
import type { SessionDoc } from "../../stores/session";
import { chainAt, dropEdit, ownerKey, parseOwner } from "./slot-drag";
import { EMPTY_CHAIN, layoutSlots } from "./chain";

const slot = (id: string, extra = {}) => ({ id, effect: "vst3:AAA", enabled: true, params: { p0: 0.3 }, preset: null, plugin_state: "STATE", ...extra });
const doc = {
  tracks: [
    { id: "t1", clips: [], effects: { ...EMPTY_CHAIN, slots: [slot("a"), slot("b")] } },
    { id: "t2", clips: [], effects: { ...EMPTY_CHAIN, slots: [slot("c")] } },
  ],
  buses: [],
  master: { effects: EMPTY_CHAIN },
} as unknown as SessionDoc;

describe("dragging effects between slots", () => {
  it("names chains as strings and back", () => {
    expect(parseOwner(ownerKey({ kind: "track", id: "t1" }))).toEqual({ kind: "track", id: "t1" });
    expect(parseOwner("master")).toEqual({ kind: "master" });
    expect(parseOwner("nope")).toBeNull();
  });

  it("moves within a chain", () => {
    const e = dropEdit(doc, { kind: "track", id: "t1" }, slot("a"), { owner: "track:t1", index: 4 }, () => "new")!;
    expect(e.label).toBe("Move Effect");
    const chain = (e.op as { chain: typeof EMPTY_CHAIN }).chain;
    expect(layoutSlots(chain).map((s) => s?.id ?? "-").slice(0, 5).join("")).toBe("-b--a");
  });

  it("copies to another track with every setting, as a new effect", () => {
    const e = dropEdit(doc, { kind: "track", id: "t1" }, slot("a"), { owner: "track:t2", index: 0 }, () => "new")!;
    expect(e.label).toBe("Copy Effect");
    expect((e.op as { target: unknown }).target).toEqual({ kind: "track", id: "t2" });
    const chain = (e.op as { chain: typeof EMPTY_CHAIN }).chain;
    expect(chain.slots.map((s) => s.id)).toEqual(["new", "c"]);
    expect(chain.slots[0]).toMatchObject({ params: { p0: 0.3 }, plugin_state: "STATE", position: 0 });
    // The source is untouched.
    expect(chainAt(doc, { kind: "track", id: "t1" })!.slots).toHaveLength(2);
  });
});
