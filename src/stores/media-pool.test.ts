// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

import { describe, expect, it } from "vitest";
import type { AudioInfo } from "../app/core";
import { UndoStack } from "../commands";
import { MediaPool, fileName } from "./media-pool";

function info(path: string): AudioInfo {
  return {
    path,
    name: fileName(path),
    format: "WAV",
    codec: "PCM 16-bit",
    sampleRate: 48000,
    channels: 2,
    length: 48000,
    bitDepth: 16,
    fileSize: 1000,
  };
}

function setup(opts: { fail?: string[]; folders?: Record<string, string[]> } = {}) {
  let n = 0;
  const probed: string[] = [];
  const pool = new MediaPool({
    newId: () => `m${++n}`,
    expand: async (paths) => paths.flatMap((p) => opts.folders?.[p] ?? [p]),
    probe: async (path) => {
      probed.push(path);
      if (opts.fail?.includes(path)) throw new Error("Codec not supported: test");
      return info(path);
    },
  });
  const stack = new UndoStack();
  return { pool, stack, probed };
}

const names = (pool: MediaPool) => pool.current.items.map((it) => it.name);

describe("MediaPool", () => {
  it("imports, measures and selects new files as one undo step", async () => {
    const { pool, stack } = setup();
    const r = await pool.importPaths(["C:\\a\\one.wav", "C:\\a\\two.mp3"], stack);
    expect(r).toMatchObject({ added: 2, duplicates: 0 });
    expect(stack.current.undoLabel).toBe("Import 2 Files");
    expect([...pool.current.selected]).toEqual(["m1", "m2"]);
    await pool.whenIdle();
    expect(pool.current.items.map((it) => it.status)).toEqual(["ready", "ready"]);
    expect(pool.current.items[0]?.info?.sampleRate).toBe(48000);
  });

  it("expands folders and skips files already listed, ignoring case and slash style", async () => {
    const { pool, stack } = setup({ folders: { "C:\\show": ["C:\\show\\a.wav", "C:\\show\\b.wav"] } });
    await pool.importPaths(["C:\\show\\a.wav"], stack);
    const r = await pool.importPaths(["C:\\show", "c:/SHOW/a.wav"], stack);
    expect(r).toMatchObject({ added: 1, duplicates: 2 });
    expect(names(pool)).toEqual(["a.wav", "b.wav"]);
    expect(r.ids).toEqual(["m1", "m2"]);
    expect(stack.current.undoLabel).toBe("Import File");
  });

  it("keeps paths that differ only in case apart on Linux", async () => {
    const pool = new MediaPool({ ignorePathCase: false, expand: async (p) => p, probe: async (p) => info(p) });
    const r = await pool.importPaths(["/home/s/Intro.wav", "/home/s/intro.wav"], new UndoStack());
    expect(r).toMatchObject({ added: 2, duplicates: 0 });
  });

  it("adds nothing to history when every file is a duplicate", async () => {
    const { pool, stack } = setup();
    await pool.importPaths(["a.wav"], stack);
    await pool.importPaths(["a.wav"], stack);
    expect(stack.current.entries).toHaveLength(1);
  });

  it("undo and redo of an import keep the measured result", async () => {
    const { pool, stack, probed } = setup();
    await pool.importPaths(["a.wav"], stack);
    await pool.whenIdle();
    await stack.undo();
    expect(pool.current.items).toHaveLength(0);
    await stack.redo();
    expect(pool.current.items[0]?.status).toBe("ready");
    expect(probed).toEqual(["a.wav"]);
  });

  it("records errors per file and can try again", async () => {
    const { pool, stack, probed } = setup({ fail: ["bad.m4a"] });
    await pool.importPaths(["good.wav", "bad.m4a"], stack);
    await pool.whenIdle();
    const bad = pool.current.items[1]!;
    expect(bad.status).toBe("error");
    expect(bad.error).toBe("Codec not supported: test");
    pool.retry(bad.id);
    expect(pool.current.items[1]?.status).toBe("probing");
    await pool.whenIdle();
    expect(probed.filter((p) => p === "bad.m4a")).toHaveLength(2);
  });

  it("removes files and restores them to the same positions on undo", async () => {
    const { pool, stack } = setup();
    await pool.importPaths(["a.wav", "b.wav", "c.wav", "d.wav"], stack);
    await pool.remove(["m2", "m4"], stack);
    expect(names(pool)).toEqual(["a.wav", "c.wav"]);
    expect(stack.current.undoLabel).toBe("Remove 2 Files");
    expect([...pool.current.selected]).toEqual(["m1", "m3"]);
    await stack.undo();
    expect(names(pool)).toEqual(["a.wav", "b.wav", "c.wav", "d.wav"]);
    expect([...pool.current.selected]).toEqual(["m2", "m4"]);
  });

  it("limits how many files are measured at once", async () => {
    let active = 0;
    let peak = 0;
    const pool = new MediaPool({
      concurrency: 2,
      expand: async (p) => p,
      probe: async (path) => {
        active++;
        peak = Math.max(peak, active);
        await new Promise((r) => setTimeout(r, 1));
        active--;
        return info(path);
      },
    });
    await pool.importPaths(["1.wav", "2.wav", "3.wav", "4.wav", "5.wav"], new UndoStack());
    await pool.whenIdle();
    expect(peak).toBe(2);
    expect(pool.current.items.every((it) => it.status === "ready")).toBe(true);
  });

  it("replaces the whole list without touching history", async () => {
    const { pool, stack } = setup({ fail: ["C:\\gone.wav"] });
    await pool.importPaths(["a.wav"], stack);
    pool.replaceAll([
      { id: "x1", path: "C:\\Shows\\ep.wav" },
      { id: "x2", path: "C:\\gone.wav" },
    ]);
    expect(names(pool)).toEqual(["ep.wav", "gone.wav"]);
    expect(stack.current.entries).toHaveLength(1);
    await pool.whenIdle();
    expect(pool.readyItems().map((i) => i.id)).toEqual(["x1"]);
    expect(pool.get("x2")?.status).toBe("error");
  });

  it("keeps open files when a session's files are added", async () => {
    const { pool, stack } = setup();
    await pool.importPaths(["C:\\a.wav", "C:\\b.wav"], stack);
    await pool.whenIdle();
    const b = pool.current.items[1]!.id;
    pool.merge([
      { id: "s1", path: "c:\\B.wav" },
      { id: "s2", path: "C:\\c.wav" },
    ]);
    expect(names(pool)).toEqual(["a.wav", "b.wav", "c.wav"]);
    // The listed file now goes by the session's id, which its clips use.
    expect(pool.get("s1")?.path).toBe("C:\\b.wav");
    expect(pool.current.items.some((it) => it.id === b)).toBe(false);
    await pool.whenIdle();
    expect(pool.readyItems().map((i) => i.id)).toEqual([pool.current.items[0]!.id, "s1", "s2"]);
  });

  it("reads file names from either slash style", () => {
    expect(fileName("C:\\Shows\\ep 12.wav")).toBe("ep 12.wav");
    expect(fileName("/home/host/bumper.mp3")).toBe("bumper.mp3");
  });
});
