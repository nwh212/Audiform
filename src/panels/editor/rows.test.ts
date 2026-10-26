// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import type { BusDoc, SessionDoc, TrackDoc } from "../../stores/session";
import { laneRows, moveRowOps } from "./rows";

const t = (id: string, color: string | null = null) => ({ id, name: id, color, clips: [] }) as unknown as TrackDoc;
const b = (id: string, position?: number) => ({ id, name: id, position }) as unknown as BusDoc;
const doc = (tracks: TrackDoc[], buses: BusDoc[]) => ({ tracks, buses }) as unknown as SessionDoc;
const ids = (d: SessionDoc, listed = new Set(d.tracks.map((x) => x.id))) => laneRows(d, listed).map((r) => r.id);

describe("Editor rows", () => {
  it("places buses among the tracks", () => {
    const d = doc([t("a"), t("b"), t("c")], [b("x"), b("y", 1), b("z", 0), b("far", 9)]);
    expect(ids(d)).toEqual(["z", "a", "y", "b", "c", "x", "far"]);
    // A hidden track keeps its slot.
    expect(ids(d, new Set(["a", "c"]))).toEqual(["z", "a", "y", "c", "x", "far"]);
    const rows = laneRows(d, new Set(["a", "c"]));
    expect(rows.find((r) => r.id === "c")).toMatchObject({ kind: "track", i: 1 });
  });

  it("moves a bus between tracks", () => {
    const d = doc([t("a"), t("b")], [b("x")]);
    expect(moveRowOps(d, new Set(["a", "b"]), "x", "b")).toEqual([{ op: "set_bus", id: "x", changes: { position: 1 } }]);
    // Already at the bottom.
    expect(moveRowOps(d, new Set(["a", "b"]), "x", null)).toEqual([]);
    const d2 = doc([t("a"), t("b")], [b("x", 0), b("y")]);
    // y to the top, above x.
    expect(moveRowOps(d2, new Set(["a", "b"]), "y", "x")).toEqual([
      { op: "move_bus", id: "y", to: 0 },
      { op: "set_bus", id: "y", changes: { position: 0 } },
    ]);
    // x back to the bottom.
    expect(moveRowOps(d2, new Set(["a", "b"]), "x", null)).toEqual([
      { op: "move_bus", id: "x", to: 1 },
      { op: "set_bus", id: "x", changes: { clear_position: true } },
    ]);
  });

  it("moves a track, keeping colors and the buses' places", () => {
    // a, x(bus), b, c: move a just above c. The bus stays on screen where
    // it was, now with no track above it.
    const d = doc([t("a"), t("b", "#123456"), t("c")], [b("x", 1)]);
    const ops = moveRowOps(d, new Set(["a", "b", "c"]), "a", "c");
    expect(ops).toEqual([
      { op: "set_track", id: "a", changes: { color: "--clip-1" } },
      { op: "set_track", id: "c", changes: { color: "--clip-3" } },
      { op: "move_track", id: "a", to: 1 },
      { op: "set_bus", id: "x", changes: { position: 0 } },
    ]);
    // Moving c up between a and the bus: the bus now has two tracks above it.
    expect(moveRowOps(d, new Set(["a", "b", "c"]), "c", "x")).toContainEqual({ op: "set_bus", id: "x", changes: { position: 2 } });
    // Moving a to the bottom leaves no track above the bus.
    expect(moveRowOps(d, new Set(["a", "b", "c"]), "a", null)).toContainEqual({ op: "set_bus", id: "x", changes: { position: 0 } });
  });
});
