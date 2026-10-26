// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { DEFAULT_LAYOUT, PANELS, panelDef, presetById, resolveHome, WORKSPACE_PRESETS, workspaceId } from "./registry";

describe("panel registry", () => {
  it("has unique ids", () => {
    const ids = PANELS.map((p) => p.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("builds the Default layout from exactly the default panels", () => {
    const inDefault = PANELS.filter((p) => p.inDefault).map((p) => p.id).sort();
    const built = DEFAULT_LAYOUT.map((s) => s.id).sort();
    expect(built).toEqual(inDefault);
  });

  it("only references panels added earlier in the Default layout", () => {
    const added = new Set<string>();
    for (const step of DEFAULT_LAYOUT) {
      if (step.placement && "reference" in step.placement) {
        expect(added.has(step.placement.reference), `${step.id} -> ${step.placement.reference}`).toBe(true);
      }
      added.add(step.id);
    }
  });

  it("does not use other products' panel names", () => {
    const banned = /selection\/view|media browser|essential sound|effects rack|diagnostics|batch process|match loudness/i;
    for (const p of PANELS) expect(p.title).not.toMatch(banned);
  });

  it("keeps Markers, History and Favorites out of the Default layout", () => {
    const ids = DEFAULT_LAYOUT.map((s) => s.id) as string[];
    expect(ids).not.toContain("markers");
    expect(ids).not.toContain("history");
    expect(ids).not.toContain("favorites");
  });
});

describe("resolveHome", () => {
  it("joins the left tab group when it is open", () => {
    expect(resolveHome("history", new Set(["voice-chain", "editor"]))).toEqual({
      reference: "voice-chain",
      direction: "within",
    });
  });

  it("falls back down the list when the first choice is closed", () => {
    expect(resolveHome("history", new Set(["files", "editor"]))).toEqual({
      reference: "files",
      direction: "below",
    });
  });

  it("uses a window edge when nothing it knows is open", () => {
    expect(resolveHome("history", new Set())).toEqual({ edge: "left" });
  });

  it("never places a panel relative to itself", () => {
    expect(resolveHome("voice-chain", new Set(["voice-chain", "batch-convert"]))).toEqual({
      reference: "batch-convert",
      direction: "within",
    });
  });

  it("puts Libraries under the Audio FX Panel", () => {
    expect(resolveHome("libraries", new Set(["audio-fx", "editor"]))).toEqual({
      reference: "audio-fx",
      direction: "below",
    });
  });

  it("puts Time Info beside Levels", () => {
    expect(resolveHome("time-info", new Set(["levels"]))).toEqual({
      reference: "levels",
      direction: "right",
    });
  });

  it("puts Levels back at the bottom edge", () => {
    expect(resolveHome("levels", new Set(["editor"]))).toEqual({ edge: "below" });
  });
});

describe("workspace presets", () => {
  it("only place panels next to panels already added, once each", () => {
    for (const p of WORKSPACE_PRESETS) {
      const added = new Set<string>();
      for (const step of p.steps) {
        expect(added.has(step.id), `${p.id}: ${step.id} twice`).toBe(false);
        if (step.placement && "reference" in step.placement) {
          expect(added.has(step.placement.reference), `${p.id}: ${step.id} refers to ${step.placement.reference}`).toBe(true);
        }
        expect(panelDef(step.id), step.id).toBeDefined();
        added.add(step.id);
      }
    }
    expect(new Set(WORKSPACE_PRESETS.map((p) => p.id)).size).toBe(WORKSPACE_PRESETS.length);
    expect(presetById("default")?.steps).toBe(DEFAULT_LAYOUT);
  });

  it("makes safe, unique ids for new workspaces", () => {
    expect(workspaceId("My Mix!", new Set())).toBe("my-mix");
    expect(workspaceId("Default", new Set(["default"]))).toBe("default-2");
    expect(workspaceId("???", new Set())).toBe("workspace");
  });
});
