// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { addonSelection, parseAddonRequest } from "./addons";
import { addonIdOf, addonPanelId, panelDef, resolveHome, setAddonPanels } from "./docking/registry";
import type { SessionDoc } from "../stores/session";

describe("add-on messages", () => {
  it("reads the requests a page may send and nothing else", () => {
    expect(parseAddonRequest({ type: "ready" })).toEqual({ type: "ready" });
    expect(parseAddonRequest({ type: "run", run: "r1", args: ["--pitch", "2"], input: "{}" })).toEqual({ type: "run", run: "r1", args: ["--pitch", "2"], input: "{}" });
    expect(parseAddonRequest({ type: "run", args: [1] })).toEqual({ type: "run", run: "run", args: [], input: "" });
    expect(parseAddonRequest({ type: "import", paths: ["C:\\out\\a.wav"] })).toMatchObject({ type: "import", paths: ["C:\\out\\a.wav"] });
    expect(parseAddonRequest({ type: "import", paths: [] })).toBeNull();
    expect(parseAddonRequest({ type: "pick", folder: true, title: "Model folder" })).toMatchObject({ folder: true, title: "Model folder" });
    expect(parseAddonRequest({ type: "openFolder", path: "models" })).toEqual({ type: "openFolder", path: "models" });
    expect(parseAddonRequest({ type: "notice", text: "  done " })).toEqual({ type: "notice", text: "done" });
    expect(parseAddonRequest({ type: "notice", text: "" })).toBeNull();
    expect(parseAddonRequest({ type: "save", data: { a: 1 } })).toEqual({ type: "save", data: { a: 1 } });
    expect(parseAddonRequest({ type: "eval", code: "x" })).toBeNull();
    expect(parseAddonRequest("ready")).toBeNull();
    expect(parseAddonRequest(null)).toBeNull();
  });

  it("describes the selection with each file once", () => {
    const doc = {
      media: [
        { id: "m1", path: "/a/song.wav" },
        { id: "m2", path: "/a/other.wav" },
      ],
      tracks: [{ id: "t", clips: [{ id: "c1", media_id: "m1" }, { id: "c2", media_id: "m1" }, { id: "c3", media_id: "m2" }] }],
    } as unknown as SessionDoc;
    const sel = addonSelection({ kind: "clips", clipIds: ["c1", "c2", "c3"], label: "3 clips" }, doc);
    expect(sel.kind).toBe("clips");
    expect(sel.files.map((f) => f.name)).toEqual(["song.wav", "other.wav"]);
    expect(addonSelection({ kind: "files", paths: ["C:\\x\\v.flac"], label: "v.flac" }, null).files).toEqual([{ path: "C:\\x\\v.flac", name: "v.flac" }]);
    expect(addonSelection(null, doc).kind).toBe("none");
    expect(addonSelection({ kind: "clips", clipIds: ["zz"], label: "" }, doc).kind).toBe("none");
  });
});

describe("add-on panels", () => {
  it("get panel ids and homes like any other panel", () => {
    expect(addonPanelId("voice-tool")).toBe("addon:voice-tool");
    expect(addonIdOf("addon:voice-tool")).toBe("voice-tool");
    expect(addonIdOf("files")).toBeNull();
    setAddonPanels([{ id: "voice-tool", title: "Voice Tool" }]);
    expect(panelDef("addon:voice-tool")?.title).toBe("Voice Tool");
    expect(resolveHome("addon:voice-tool", new Set(["separation", "editor"]))).toEqual({ reference: "separation", direction: "within" });
    expect(resolveHome("addon:voice-tool", new Set(["editor"]))).toEqual({ reference: "editor", direction: "right" });
    setAddonPanels([]);
    expect(panelDef("addon:voice-tool")).toBeUndefined();
  });
});
