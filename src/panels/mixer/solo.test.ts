// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import type { SessionDoc } from "../../stores/session";
import { trackOp } from "./solo";

const doc = { tracks: [{ id: "a", solo: true }, { id: "b", solo: false }, { id: "c", solo: true }] } as unknown as SessionDoc;

describe("track changes", () => {
  it("unsolos the others only when solo is exclusive", () => {
    expect(trackOp(doc, "b", { solo: true }, false)).toEqual({ op: "set_track", id: "b", changes: { solo: true } });
    expect(trackOp(doc, "b", { solo: true }, true)).toEqual({
      op: "batch",
      ops: [
        { op: "set_track", id: "b", changes: { solo: true } },
        { op: "set_track", id: "a", changes: { solo: false } },
        { op: "set_track", id: "c", changes: { solo: false } },
      ],
    });
    expect(trackOp(doc, "a", { muted: true }, true)).toEqual({ op: "set_track", id: "a", changes: { muted: true } });
  });

  it("unsolos soloed buses too", () => {
    const d = { tracks: [{ id: "a", solo: false }], buses: [{ id: "b", solo: true }] } as unknown as SessionDoc;
    expect(trackOp(d, "a", { solo: true }, true)).toEqual({
      op: "batch",
      ops: [
        { op: "set_track", id: "a", changes: { solo: true } },
        { op: "set_bus", id: "b", changes: { solo: false } },
      ],
    });
  });
});
