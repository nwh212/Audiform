// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it, vi } from "vitest";
import { UndoStack } from "../commands";
import { SessionStore, type Op, type SessionDoc, type ViewDoc } from "./session";

function doc(volume: number): SessionDoc {
  return {
    format: "altaudio.session",
    schema_version: 1,
    id: "s",
    name: "s",
    sample_rate: 48000,
    bit_depth: 32,
    media: [],
    tracks: [
      {
        id: "t",
        name: "Host",
        role: "standard",
        channels: "mono",
        color: null,
        height: 96,
        volume_db: volume,
        pan: 0,
        muted: false,
        solo: false,
        record_armed: false,
        clips: [],
      },
    ],
    markers: [],
    view: { samples_per_pixel: 1024, scroll: 0, scroll_y: 0, playhead: 0, selection: null },
    libraries: [],
  };
}

/** A fake core that applies volume changes and returns the inverse. */
function setup() {
  let volume = 0;
  const store = new SessionStore({
    get: async () => ({ session: doc(volume), path: null }),
    setView: async () => {},
    apply: async (op: Op) => {
      if (op.op !== "set_track" || op.changes.volume_db === undefined) throw new Error("unexpected");
      const old = volume;
      volume = op.changes.volume_db;
      return { session: doc(volume), inverse: { op: "set_track", id: "t", changes: { volume_db: old } } };
    },
  });
  return { store, stack: new UndoStack(), volume: () => volume };
}

describe("SessionStore", () => {
  it("merges a fader drag into one undo step back to the start", async () => {
    const { store, stack, volume } = setup();
    for (const v of [-1, -2, -3.5]) {
      await store.edit("Track Volume", { op: "set_track", id: "t", changes: { volume_db: v } }, stack);
    }
    expect(stack.current.entries).toHaveLength(1);
    expect(volume()).toBe(-3.5);
    await stack.undo();
    expect(volume()).toBe(0);
    await stack.redo();
    expect(volume()).toBe(-3.5);
    expect(store.current?.tracks[0]?.volume_db).toBe(-3.5);
  });
});

describe("fader drags", () => {
  it("skip steps a newer one replaced, and still end on the last value", async () => {
    const { store, stack, volume } = setup();
    let calls = 0;
    const apply = store.apply.bind(store);
    store.apply = async (op) => {
      calls++;
      return apply(op);
    };
    // Edits made faster than the core answers: all queued at once.
    const all = [-1, -2, -3, -4, -5].map((v) => store.edit("Track Volume", { op: "set_track", id: "t", changes: { volume_db: v } }, stack));
    await Promise.all(all);
    expect(volume()).toBe(-5);
    expect(calls).toBeLessThan(5);
    expect(stack.current.entries).toHaveLength(1);
    await stack.undo();
    expect(volume()).toBe(0);
    await stack.redo();
    expect(volume()).toBe(-5);
  });
});

describe("view changes", () => {
  const zoomed = (): ViewDoc => ({ samples_per_pixel: 256, scroll: 48_000, scroll_y: 0, playhead: 96_000, selection: null });

  it("reach view listeners only, never document listeners", async () => {
    const { store } = setup();
    await store.load();
    const docs = vi.fn();
    const views = vi.fn();
    store.subscribe(docs);
    store.subscribeView(views);
    expect(docs).toHaveBeenCalledTimes(1);
    expect(views).toHaveBeenCalledTimes(1);
    const before = store.current;
    store.setView(zoomed());
    // A wheel zoom on a session with many tracks: the panels keyed on the document hear nothing.
    expect(docs).toHaveBeenCalledTimes(1);
    expect(views).toHaveBeenCalledTimes(2);
    expect(views).toHaveBeenLastCalledWith(zoomed());
    // The document still carries the view, for saving and for anything that reads it on demand.
    expect(store.current?.view).toEqual(zoomed());
    expect(store.current?.tracks).toBe(before?.tracks);
  });

  it("still reach the core after a pause, and at once on flush", async () => {
    vi.useFakeTimers();
    try {
      const sent: ViewDoc[] = [];
      const store = new SessionStore({
        get: async () => ({ session: doc(0), path: null }),
        setView: async (v) => void sent.push(v),
        apply: async () => {
          throw new Error("unexpected");
        },
      });
      await store.load();
      store.setView(zoomed());
      store.setView({ ...zoomed(), scroll: 0 });
      expect(sent).toHaveLength(0);
      await vi.advanceTimersByTimeAsync(300);
      expect(sent).toEqual([{ ...zoomed(), scroll: 0 }]);
      store.setView(zoomed());
      await store.flushView();
      expect(sent).toHaveLength(2);
      await vi.advanceTimersByTimeAsync(1000);
      expect(sent).toHaveLength(2);
    } finally {
      vi.useRealTimers();
    }
  });

  it("follow a loaded, undone or redone document", async () => {
    const { store, stack } = setup();
    await store.load();
    const views = vi.fn();
    store.subscribeView(views);
    store.setView(zoomed());
    // The core answers an edit with its own view (the one it was last told).
    await store.edit("Track Volume", { op: "set_track", id: "t", changes: { volume_db: -3 } }, stack);
    expect(store.current?.view.samples_per_pixel).toBe(1024);
    expect(views).toHaveBeenLastCalledWith(doc(0).view);
    const calls = views.mock.calls.length;
    // A document with the same view says nothing to view listeners.
    await stack.undo();
    expect(views.mock.calls.length).toBe(calls);
  });
});
