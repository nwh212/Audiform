// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { CompoundCommand, NestedCommandError, UndoStack, type Command, type HistorySnapshot } from ".";

/** A tiny document: a list of strings. */
function makeDoc() {
  const items: string[] = [];
  const add = (name: string): Command => ({
    label: `Add ${name}`,
    do: () => {
      items.push(name);
    },
    undo: () => {
      items.pop();
    },
  });
  return { items, add };
}

/** A command that sets a number and can merge with later sets. */
class SetVolume implements Command {
  readonly label = "Track Volume";
  private before: number;
  constructor(
    private target: { volume: number },
    private value: number,
  ) {
    this.before = target.volume;
  }
  do() {
    this.target.volume = this.value;
  }
  undo() {
    this.target.volume = this.before;
  }
  mergeWith(next: Command): boolean {
    if (!(next instanceof SetVolume) || next.target !== this.target) return false;
    this.value = next.value;
    return true;
  }
}

const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));

describe("UndoStack", () => {
  it("stashes a history and restores it exactly", async () => {
    const doc = makeDoc();
    const stack = new UndoStack();
    await stack.execute(doc.add("a"));
    await stack.markSaved();
    await stack.execute(doc.add("b"));
    expect(stack.current.dirty).toBe(true);
    const kept = await stack.stash();
    expect(stack.current.canUndo).toBe(false);
    expect(stack.current.dirty).toBe(false);
    await stack.restore(kept);
    expect(stack.current.dirty).toBe(true);
    expect(stack.current.undoLabel).toBe("Add b");
    await stack.undo();
    expect(doc.items).toEqual(["a"]);
    expect(stack.current.dirty).toBe(false);
  });

  it("runs, undoes and redoes commands", async () => {
    const doc = makeDoc();
    const stack = new UndoStack();
    await stack.execute(doc.add("a"));
    await stack.execute(doc.add("b"));
    expect(doc.items).toEqual(["a", "b"]);

    expect(await stack.undo()).toBe(true);
    expect(doc.items).toEqual(["a"]);
    expect(stack.current.redoLabel).toBe("Add b");

    expect(await stack.redo()).toBe(true);
    expect(doc.items).toEqual(["a", "b"]);
    expect(stack.current.canRedo).toBe(false);
  });

  it("returns false when there is nothing to undo or redo", async () => {
    const stack = new UndoStack();
    expect(await stack.undo()).toBe(false);
    expect(await stack.redo()).toBe(false);
  });

  it("drops the redo branch when a new command runs", async () => {
    const doc = makeDoc();
    const stack = new UndoStack();
    await stack.execute(doc.add("a"));
    await stack.execute(doc.add("b"));
    await stack.undo();
    await stack.execute(doc.add("c"));
    expect(doc.items).toEqual(["a", "c"]);
    expect(stack.current.entries.map((e) => e.label)).toEqual(["Add a", "Add c"]);
    expect(stack.current.canRedo).toBe(false);
  });

  it("jumps to any point in history", async () => {
    const doc = makeDoc();
    const stack = new UndoStack();
    for (const n of ["a", "b", "c", "d"]) await stack.execute(doc.add(n));

    await stack.jumpTo(1);
    expect(doc.items).toEqual(["a"]);
    expect(stack.current.position).toBe(1);

    await stack.jumpTo(3);
    expect(doc.items).toEqual(["a", "b", "c"]);

    await stack.jumpTo(0);
    expect(doc.items).toEqual([]);

    await stack.jumpTo(99);
    expect(doc.items).toEqual(["a", "b", "c", "d"]);
  });

  it("does not record a command that throws", async () => {
    const stack = new UndoStack();
    const bad: Command = {
      label: "Broken",
      do: () => {
        throw new Error("nope");
      },
      undo: () => {},
    };
    await expect(stack.execute(bad)).rejects.toThrow("nope");
    expect(stack.current.entries).toHaveLength(0);
  });

  it("keeps position when undo throws", async () => {
    const stack = new UndoStack();
    const stuck: Command = {
      label: "Stuck",
      do: () => {},
      undo: () => {
        throw new Error("cannot undo");
      },
    };
    await stack.execute(stuck);
    await expect(stack.undo()).rejects.toThrow("cannot undo");
    expect(stack.current.position).toBe(1);
  });

  it("keeps working after a failure", async () => {
    const doc = makeDoc();
    const stack = new UndoStack();
    const bad: Command = {
      label: "Broken",
      do: () => Promise.reject(new Error("x")),
      undo: () => {},
    };
    await expect(stack.execute(bad)).rejects.toThrow();
    await stack.execute(doc.add("a"));
    expect(doc.items).toEqual(["a"]);
  });

  it("runs async commands strictly in order", async () => {
    const log: string[] = [];
    const slow = (name: string, ms: number): Command => ({
      label: name,
      do: async () => {
        await delay(ms);
        log.push(`do ${name}`);
      },
      undo: async () => {
        await delay(ms);
        log.push(`undo ${name}`);
      },
    });
    const stack = new UndoStack();
    // Fire without awaiting, like fast key presses.
    const all = Promise.all([
      stack.execute(slow("a", 20)),
      stack.execute(slow("b", 1)),
      stack.undo(),
      stack.undo(),
      stack.redo(),
    ]);
    expect(stack.current.busy).toBe(true);
    await all;
    expect(log).toEqual(["do a", "do b", "undo b", "undo a", "do a"]);
    expect(stack.current.busy).toBe(false);
  });

  it("rejects a command that runs another command", async () => {
    const stack = new UndoStack();
    let inner: Promise<void> | undefined;
    const nested: Command = {
      label: "Nested",
      do: () => {
        inner = stack.execute({ label: "Inner", do: () => {}, undo: () => {} });
      },
      undo: () => {},
    };
    await stack.execute(nested);
    await expect(inner).rejects.toBeInstanceOf(NestedCommandError);
    expect(stack.current.entries.map((e) => e.label)).toEqual(["Nested"]);
  });

  it("merges rapid edits into one entry", async () => {
    let clock = 0;
    const track = { volume: 0 };
    const stack = new UndoStack({ now: () => clock });
    await stack.execute(new SetVolume(track, -1));
    clock += 100;
    await stack.execute(new SetVolume(track, -2));
    clock += 100;
    await stack.execute(new SetVolume(track, -3));
    expect(stack.current.entries).toHaveLength(1);
    expect(track.volume).toBe(-3);

    await stack.undo();
    expect(track.volume).toBe(0);
    await stack.redo();
    expect(track.volume).toBe(-3);
  });

  it("does not merge edits far apart in time", async () => {
    let clock = 0;
    const track = { volume: 0 };
    const stack = new UndoStack({ now: () => clock, mergeWindowMs: 500 });
    await stack.execute(new SetVolume(track, -1));
    clock += 2_000;
    await stack.execute(new SetVolume(track, -2));
    expect(stack.current.entries).toHaveLength(2);
  });

  it("does not merge into a redo branch that was discarded", async () => {
    const track = { volume: 0 };
    const stack = new UndoStack({ now: () => 0 });
    await stack.execute(new SetVolume(track, -1));
    await stack.undo();
    await stack.execute(new SetVolume(track, -5));
    expect(stack.current.entries).toHaveLength(1);
    await stack.undo();
    expect(track.volume).toBe(0);
  });

  it("drops the oldest entries past the limit", async () => {
    const doc = makeDoc();
    const stack = new UndoStack({ limit: 3 });
    for (const n of ["a", "b", "c", "d", "e"]) await stack.execute(doc.add(n));
    expect(stack.current.entries.map((e) => e.label)).toEqual(["Add c", "Add d", "Add e"]);
    await stack.jumpTo(0);
    // Undo stops at the oldest kept entry.
    expect(doc.items).toEqual(["a", "b"]);
  });

  it("tracks the save point", async () => {
    const doc = makeDoc();
    const stack = new UndoStack();
    expect(stack.current.dirty).toBe(false);

    await stack.execute(doc.add("a"));
    expect(stack.current.dirty).toBe(true);

    await stack.markSaved();
    expect(stack.current.dirty).toBe(false);

    await stack.execute(doc.add("b"));
    expect(stack.current.dirty).toBe(true);

    await stack.undo();
    expect(stack.current.dirty).toBe(false);

    await stack.undo();
    expect(stack.current.dirty).toBe(true);
  });

  it("stays dirty once the save point is discarded", async () => {
    const doc = makeDoc();
    const stack = new UndoStack();
    await stack.execute(doc.add("a"));
    await stack.markSaved();
    await stack.undo();
    await stack.execute(doc.add("b"));
    await stack.undo();
    // Back at base, but the saved file had "a" in it.
    expect(stack.current.dirty).toBe(true);
  });

  it("is dirty at a trimmed base even if the original base was saved", async () => {
    const doc = makeDoc();
    const stack = new UndoStack({ limit: 1 });
    await stack.execute(doc.add("a"));
    await stack.execute(doc.add("b"));
    await stack.undo();
    expect(doc.items).toEqual(["a"]);
    expect(stack.current.dirty).toBe(true);
  });

  it("clears history and sets a new base label", async () => {
    const doc = makeDoc();
    const stack = new UndoStack();
    await stack.execute(doc.add("a"));
    await stack.clear("New Session");
    expect(stack.current.entries).toHaveLength(0);
    expect(stack.current.baseLabel).toBe("New Session");
    expect(stack.current.dirty).toBe(false);
    expect(doc.items).toEqual(["a"]);
  });

  it("notifies subscribers like a Svelte store", async () => {
    const doc = makeDoc();
    const stack = new UndoStack();
    const seen: HistorySnapshot[] = [];
    const stop = stack.subscribe((s) => seen.push(s));
    expect(seen).toHaveLength(1);
    await stack.execute(doc.add("a"));
    expect(seen.at(-1)?.undoLabel).toBe("Add a");
    stop();
    const count = seen.length;
    await stack.execute(doc.add("b"));
    expect(seen).toHaveLength(count);
  });
});

describe("CompoundCommand", () => {
  it("records children as one entry and undoes in reverse", async () => {
    const doc = makeDoc();
    const stack = new UndoStack();
    await stack.execute(new CompoundCommand("Add Two", [doc.add("a"), doc.add("b")]));
    expect(stack.current.entries).toHaveLength(1);
    expect(doc.items).toEqual(["a", "b"]);
    await stack.undo();
    expect(doc.items).toEqual([]);
  });

  it("rolls back finished children when one fails", async () => {
    const doc = makeDoc();
    const stack = new UndoStack();
    const fail: Command = {
      label: "Fail",
      do: () => {
        throw new Error("boom");
      },
      undo: () => {},
    };
    await expect(
      stack.execute(new CompoundCommand("Mixed", [doc.add("a"), doc.add("b"), fail])),
    ).rejects.toThrow("boom");
    expect(doc.items).toEqual([]);
    expect(stack.current.entries).toHaveLength(0);
  });
  it("can mark the current state unsaved", async () => {
    const stack = new UndoStack();
    expect(stack.current.dirty).toBe(false);
    await stack.markUnsaved();
    expect(stack.current.dirty).toBe(true);
    await stack.markSaved();
    expect(stack.current.dirty).toBe(false);
  });
});
