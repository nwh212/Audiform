// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import type { Library, LibraryItem } from "../app/core";
import { UndoStack } from "../commands";
import { folderName, itemPath, joinPath, LibraryStore, type LibraryBackend } from "./libraries";

function emptyLib(name: string): Library {
  return {
    format: "altaudio.library",
    schema_version: 1,
    id: `lib-${name}`,
    name,
    scope: "show",
    created: "2026-10-13T00:00:00Z",
    items: [],
    collections: [],
  };
}

function item(id: string, file = `audio/${id}.wav`): LibraryItem {
  return {
    id,
    name: id,
    kind: "audio",
    file,
    copied: true,
    original_path: null,
    sample_rate: 48000,
    channels: 2,
    length: 48000,
    tags: [],
    notes: "",
    color: null,
    added: "2026-10-13T00:00:00Z",
  };
}

function setup(
  saved: { open: string[]; active: string | null } = { open: [], active: null },
  disk = new Map<string, Library>(),
) {
  let n = 0;
  const backend: LibraryBackend = {
    defaultRoot: async () => "C:\\Users\\host\\Documents\\Altered Audio\\Libraries",
    create: async (dir, name) => {
      const lib = emptyLib(name);
      disk.set(dir, lib);
      return lib;
    },
    open: async (dir) => {
      const lib = disk.get(dir);
      if (!lib) throw new Error("not a library");
      return structuredClone(lib);
    },
    save: async (dir, lib) => void disk.set(dir, structuredClone(lib)),
    import: async (_dir, paths) => ({ items: paths.map(() => item(`i${++n}`)), skipped: [] }),
  };
  let prefs = saved;
  const store = new LibraryStore(backend, {
    get: () => prefs,
    set: (open, active) => (prefs = { open, active }),
  });
  return { store, disk, stack: new UndoStack(), prefs: () => prefs };
}

describe("LibraryStore", () => {
  it("creates a library under the default folder and remembers it", async () => {
    const { store, prefs } = setup();
    const lib = await store.create("Network: Promos?");
    expect(lib.dir).toBe("C:\\Users\\host\\Documents\\Altered Audio\\Libraries\\Network Promos");
    expect(store.current.active?.lib.name).toBe("Network: Promos?");
    expect(prefs().open).toEqual([lib.dir]);
  });

  it("imports into the active library and the shown collection, undoably", async () => {
    const { store, stack, disk } = setup();
    const { dir } = await store.create("Beds");
    const cid = await store.addCollection("Intros", stack);
    store.showCollection(cid);
    await store.importPaths(["a.wav", "b.wav"], stack);
    expect(store.current.active?.lib.items.map((i) => i.id)).toEqual(["i1", "i2"]);
    expect(store.shownItems().map((i) => i.id)).toEqual(["i1", "i2"]);
    expect(stack.current.undoLabel).toBe("Add 2 to Library");
    expect([...store.current.selected]).toEqual(["i1", "i2"]);
    await stack.undo();
    expect(disk.get(dir)?.items).toEqual([]);
    expect(store.current.selected.size).toBe(0);
    await stack.redo();
    expect(disk.get(dir)?.items).toHaveLength(2);
  });

  it("removes items from the library and every collection", async () => {
    const { store, stack } = setup();
    await store.create("X");
    await store.importPaths(["a", "b", "c"], stack);
    const cid = await store.addCollection("C", stack, ["i1", "i2"]);
    await store.removeItems(["i2"], stack);
    const lib = store.current.active!.lib;
    expect(lib.items.map((i) => i.id)).toEqual(["i1", "i3"]);
    expect(lib.collections.find((c) => c.id === cid)?.items).toEqual(["i1"]);
    await stack.undo();
    expect(store.current.active!.lib.collections[0]?.items).toEqual(["i1", "i2"]);
  });

  it("edits item metadata and collections", async () => {
    const { store, stack } = setup();
    await store.create("X");
    await store.importPaths(["a"], stack);
    await store.updateItem("i1", { name: "Cold Open", tags: ["intro"] }, stack);
    expect(stack.current.undoLabel).toBe("Rename Library Item");
    const cid = await store.addCollection("Openers", stack);
    await store.addToCollection(cid, ["i1", "i1"], stack);
    await store.renameCollection(cid, "Cold Opens", stack);
    store.showCollection(cid);
    expect(store.shownItems().map((i) => i.name)).toEqual(["Cold Open"]);
    await store.removeFromCollection(cid, ["i1"], stack);
    expect(store.shownItems()).toEqual([]);
    await store.deleteCollection(cid, stack);
    expect(store.current.collection).toBeNull();
    await stack.undo();
    expect(store.current.active!.lib.collections[0]?.name).toBe("Cold Opens");
  });

  it("restores open libraries and skips missing ones", async () => {
    const first = setup();
    const a = await first.store.create("A");
    const b = await first.store.create("B");
    const second = setup({ open: [a.dir, "D:\\gone", b.dir, a.dir], active: a.dir }, first.disk);
    const failed = await second.store.restore();
    expect(failed).toEqual(["D:\\gone"]);
    expect(second.store.current.libs.map((l) => l.lib.name)).toEqual(["A", "B"]);
    expect(second.store.current.active?.dir).toBe(a.dir);
    // A second restore (the window reloading) adds nothing.
    expect(await second.store.restore()).toEqual([]);
    expect(second.store.current.libs).toHaveLength(2);
  });
});

describe("library paths", () => {
  it("joins and resolves item paths in either slash style", () => {
    expect(joinPath("C:\\Libs\\Beds", "audio/bed.wav")).toBe("C:\\Libs\\Beds\\audio\\bed.wav");
    expect(joinPath("/home/s/Beds/", "audio/bed.wav")).toBe("/home/s/Beds/audio/bed.wav");
    expect(itemPath("C:\\Libs", item("x", "D:\\Raw\\x.wav"))).toBe("D:\\Raw\\x.wav");
    expect(itemPath("/libs", item("x", "audio/x.wav"))).toBe("/libs/audio/x.wav");
  });

  it("makes folder-safe names", () => {
    expect(folderName("Show: Promos / Ads")).toBe("Show Promos Ads");
    expect(folderName("  ...  ")).toBe("Library");
  });
});
