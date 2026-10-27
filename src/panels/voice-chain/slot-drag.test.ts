// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import type { SessionDoc } from "../../stores/session";
import { chainAt, dropEdit, effectMenu, effectSearch, ownerKey, parseOwner, vst3Items, type MenuItem } from "./slot-drag";
import type { EffectInfo } from "../../app/core";
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

describe("the Add effect menu", () => {
  const fx = (id: string, name: string, category: string, plugin = false) => ({ id, name, category, description: "", params: [], plugin }) as EffectInfo;
  const catalog = [
    fx("limiter", "Limiter", "Dynamics"),
    fx("eq", "Parametric EQ", "Filter and EQ"),
    fx("comp", "Dynamics", "Dynamics"),
    fx("vst3:a", "Pro Verb", "Plug-ins", true),
    fx("vst3:b", "air EQ", "Plug-ins", true),
    fx("vst3:c", "Big Verb", "Plug-ins", true),
  ];
  const plugins = [
    { effect: "vst3:a", name: "Pro Verb", vendor: "Zeta", kind: "Fx|Reverb" },
    { effect: "vst3:b", name: "air EQ", vendor: "acme", kind: "Fx|EQ" },
    { effect: "vst3:c", name: "Big Verb", vendor: "Acme", kind: "Fx|Reverb" },
  ];
  const labels = (rows: readonly MenuItem[]) => rows.map((r) => (r.kind === "sep" ? "-" : r.kind === "search" ? "?" : r.label));
  const sub = (rows: readonly MenuItem[], label: string) => {
    const r = rows.find((x) => x.kind === "sub" && x.label === label);
    return r && r.kind === "sub" ? r.items : [];
  };

  it("lists VST3 plug-ins by type, then maker, then name, all alphabetical", () => {
    const rows = vst3Items(catalog, plugins, () => {});
    expect(labels(rows)).toEqual(["EQ", "Reverb"]);
    expect(labels(sub(rows, "Reverb"))).toEqual(["Acme", "Zeta"]);
    expect(labels(sub(sub(rows, "Reverb"), "Acme"))).toEqual(["Big Verb"]);
  });

  it("starts with a search box, then the families alphabetically", () => {
    const rows = effectMenu(catalog, plugins, () => {});
    expect(labels(rows)).toEqual(["?", "Dynamics", "Filter and EQ", "-", "VST", "VST3"]);
    expect(labels(sub(rows, "Dynamics"))).toEqual(["Dynamics", "Limiter"]);
  });

  it("finds effects and plug-ins by name or maker, names starting with the text first", () => {
    const picked: string[] = [];
    const find = effectSearch(catalog, plugins, (id) => picked.push(id));
    expect(labels(find("verb"))).toEqual(["Big Verb", "Pro Verb"]);
    expect(labels(find("eq"))).toEqual(["air EQ", "Parametric EQ"]);
    expect(labels(find("acme"))).toEqual(["air EQ", "Big Verb"]);
    const first = find("lim")[0];
    if (first?.kind === "item") first.run();
    expect(picked).toEqual(["limiter"]);
  });
});
