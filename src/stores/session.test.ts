// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { UndoStack } from "../commands";
import { SessionStore, type Op, type SessionDoc } from "./session";

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
