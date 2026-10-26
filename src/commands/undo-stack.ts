// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import type { Command, HistoryEntry, HistorySnapshot } from "./types";

export interface UndoStackOptions {
  /** Label of the starting state. Default "Open". */
  baseLabel?: string;
  /** Oldest entries are dropped past this count. Default 1000. */
  limit?: number;
  /** Max gap between two commands that may merge, in ms. Default 750. */
  mergeWindowMs?: number;
  /** Clock, replaceable in tests. */
  now?: () => number;
}

interface Entry extends HistoryEntry {
  command: Command;
}

type Listener = (snapshot: HistorySnapshot) => void;

/** Thrown when a command tries to run another command from inside itself. */
export class NestedCommandError extends Error {
  constructor() {
    super("A command cannot run another command. Combine them with CompoundCommand instead.");
    this.name = "NestedCommandError";
  }
}

/**
 * Undo history for one document. Each open session gets its own stack.
 *
 * Operations run strictly one at a time in call order, even when
 * commands are async. Callers can fire Ctrl+Z repeatedly without
 * waiting and every step still happens in sequence.
 *
 * Implements the Svelte store contract, so components can read
 * `$stack` directly.
 */
/** A history taken out with `stash`. Opaque outside this file. */
export interface StashedHistory {
  readonly entries: readonly unknown[];
  readonly position: number;
  readonly savedId: number;
  readonly baseLabel: string;
  readonly baseShifted: boolean;
}

export class UndoStack {
  private entries: Entry[] = [];
  private position = 0;
  private nextId = 1;
  /** Entry id at the save point. 0 is the base state, -1 is unreachable. */
  private savedId = 0;
  private baseLabel: string;
  private readonly limit: number;
  private readonly mergeWindowMs: number;
  private readonly now: () => number;

  private queue: Promise<unknown> = Promise.resolve();
  private pending = 0;
  /** True only during the synchronous part of a command's do/undo. */
  private inCommand = false;
  /** True once old entries were trimmed, so the base is no longer the original state. */
  private baseShifted = false;
  private listeners = new Set<Listener>();
  private snapshot: HistorySnapshot;

  constructor(options: UndoStackOptions = {}) {
    this.baseLabel = options.baseLabel ?? "Open";
    this.limit = Math.max(1, options.limit ?? 1000);
    this.mergeWindowMs = options.mergeWindowMs ?? 750;
    this.now = options.now ?? Date.now;
    this.snapshot = this.buildSnapshot();
  }

  /** Svelte store contract. Calls `fn` now and on every change. */
  subscribe(fn: Listener): () => void {
    this.listeners.add(fn);
    fn(this.snapshot);
    return () => {
      this.listeners.delete(fn);
    };
  }

  /** Current state, for code outside components. */
  get current(): HistorySnapshot {
    return this.snapshot;
  }

  /**
   * Runs a command and records it. Anything that was undone is
   * discarded, as in every editor. If the command throws, nothing is
   * recorded and the error is passed to the caller.
   */
  execute(command: Command): Promise<void> {
    return this.enqueue(async () => {
      await this.guard(() => command.do());

      if (this.position < this.entries.length) {
        const dropped = this.entries.splice(this.position);
        if (dropped.some((e) => e.id === this.savedId)) {
          this.savedId = -1;
        }
      }

      const time = this.now();
      const top = this.entries[this.entries.length - 1];
      const canMerge =
        top !== undefined &&
        top.id !== this.savedId &&
        time - top.time <= this.mergeWindowMs &&
        top.command.mergeWith?.(command) === true;

      if (canMerge) {
        this.entries[this.entries.length - 1] = { ...top, time };
      } else {
        this.entries.push({ id: this.nextId++, label: command.label, time, command });
        this.position = this.entries.length;
        this.trim();
      }
    });
  }

  /** Steps back one entry. Resolves false if there was nothing to undo. */
  undo(): Promise<boolean> {
    return this.enqueue(async () => this.stepBack());
  }

  /** Steps forward one entry. Resolves false if there was nothing to redo. */
  redo(): Promise<boolean> {
    return this.enqueue(async () => this.stepForward());
  }

  /**
   * Moves to a point in history, as when clicking a row in the History
   * panel. 0 is the base state; n means the first n entries applied.
   */
  jumpTo(target: number): Promise<void> {
    return this.enqueue(async () => {
      const goal = Math.max(0, Math.min(target, this.entries.length));
      while (this.position > goal) await this.stepBack();
      while (this.position < goal) await this.stepForward();
    });
  }

  /**
   * Forgets all history and makes the current state the new base,
   * as after opening a file. Does not change any state.
   */
  clear(baseLabel?: string): Promise<void> {
    return this.enqueue(async () => {
      this.entries = [];
      this.position = 0;
      this.savedId = 0;
      this.baseShifted = false;
      if (baseLabel !== undefined) this.baseLabel = baseLabel;
    });
  }

  /**
   * Takes the whole history out (for a file set aside) and leaves this
   * stack empty. `restore` puts it back exactly, dirty state included.
   */
  stash(): Promise<StashedHistory> {
    return this.enqueue(async () => {
      const st: StashedHistory = {
        entries: this.entries,
        position: this.position,
        savedId: this.savedId,
        baseLabel: this.baseLabel,
        baseShifted: this.baseShifted,
      };
      this.entries = [];
      this.position = 0;
      this.savedId = 0;
      this.baseShifted = false;
      return st;
    });
  }

  restore(st: StashedHistory): Promise<void> {
    return this.enqueue(async () => {
      this.entries = st.entries as Entry[];
      this.position = st.position;
      this.savedId = st.savedId;
      this.baseLabel = st.baseLabel;
      this.baseShifted = st.baseShifted;
    });
  }

  /**
   * Marks the current state as not saved, for example after recovering
   * an autosave, so the user is asked to save it.
   */
  markUnsaved(): Promise<void> {
    return this.enqueue(async () => {
      this.savedId = -1;
    });
  }

  /** Marks the current state as saved, so `dirty` becomes false. */
  markSaved(): Promise<void> {
    return this.enqueue(async () => {
      this.savedId = this.currentId();
    });
  }

  // Internals

  private async stepBack(): Promise<boolean> {
    const entry = this.entries[this.position - 1];
    if (!entry) return false;
    await this.guard(() => entry.command.undo());
    this.position -= 1;
    return true;
  }

  private async stepForward(): Promise<boolean> {
    const entry = this.entries[this.position];
    if (!entry) return false;
    await this.guard(() => entry.command.do());
    this.position += 1;
    return true;
  }

  private async guard(run: () => void | Promise<void>): Promise<void> {
    let result: void | Promise<void>;
    this.inCommand = true;
    try {
      result = run();
    } finally {
      this.inCommand = false;
    }
    await result;
  }

  private trim(): void {
    const excess = this.entries.length - this.limit;
    if (excess <= 0) return;
    const dropped = this.entries.splice(0, excess);
    this.position -= excess;
    this.baseShifted = true;
    if (dropped.some((e) => e.id === this.savedId)) {
      // The save point fell off the start of history.
      this.savedId = -1;
    }
  }

  private currentId(): number {
    if (this.position === 0) {
      // -2 never matches a save point: a shifted base is a state
      // nobody saved.
      return this.baseShifted ? -2 : 0;
    }
    return this.entries[this.position - 1]!.id;
  }

  private enqueue<T>(op: () => Promise<T>): Promise<T> {
    if (this.inCommand) {
      // Called from inside a running command. Queuing would deadlock.
      // Only the synchronous part of a command can be caught here, so
      // async commands must also never call the stack.
      return Promise.reject(new NestedCommandError());
    }
    this.pending += 1;
    this.emit();
    const result = this.queue.then(op);
    this.queue = result.then(
      () => undefined,
      () => undefined,
    );
    return result.finally(() => {
      this.pending -= 1;
      this.emit();
    });
  }

  private emit(): void {
    this.snapshot = this.buildSnapshot();
    for (const fn of this.listeners) fn(this.snapshot);
  }

  private buildSnapshot(): HistorySnapshot {
    const undoEntry = this.entries[this.position - 1];
    const redoEntry = this.entries[this.position];
    return {
      baseLabel: this.baseLabel,
      entries: this.entries.map(({ id, label, time }) => ({ id, label, time })),
      position: this.position,
      canUndo: undoEntry !== undefined,
      canRedo: redoEntry !== undefined,
      undoLabel: undoEntry?.label ?? null,
      redoLabel: redoEntry?.label ?? null,
      busy: this.pending > 0,
      dirty: this.currentId() !== this.savedId,
    };
  }
}
