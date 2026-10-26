// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Every state change in the app is a Command. Components never mutate
 * state directly. They build a command and hand it to an UndoStack.
 *
 * `do` and `undo` may be async because most edits will round-trip to
 * the Rust core. The stack runs them one at a time, in order.
 */
export interface Command {
  /** Name shown in the History panel and the Edit menu, e.g. "Move Clip". */
  readonly label: string;

  /** Applies the change. Also used for redo, so it must be repeatable. */
  do(): void | Promise<void>;

  /** Reverses exactly what `do` changed. */
  undo(): void | Promise<void>;

  /**
   * Optional. Lets a rapid series of edits collapse into one history
   * entry, such as dragging a volume fader. Called on the newest entry
   * with the incoming command, after the incoming command has run.
   * Return true if this command absorbed `next`; it then stays as the
   * single entry and must undo both changes.
   */
  mergeWith?(next: Command): boolean;
}

/** One row in the History panel. */
export interface HistoryEntry {
  readonly id: number;
  readonly label: string;
  /** Milliseconds since epoch when the entry was last changed. */
  readonly time: number;
}

/** What the History panel and menus need to render. */
export interface HistorySnapshot {
  /** Label of the starting state, shown as the first row ("Open", "New Session"). */
  readonly baseLabel: string;
  readonly entries: readonly HistoryEntry[];
  /**
   * How many entries are applied. 0 means at the base state.
   * Entries at index >= position are undone and can be redone.
   */
  readonly position: number;
  readonly canUndo: boolean;
  readonly canRedo: boolean;
  /** Label for "Undo <label>" in the Edit menu, or null. */
  readonly undoLabel: string | null;
  /** Label for "Redo <label>" in the Edit menu, or null. */
  readonly redoLabel: string | null;
  /** True while a command is running. */
  readonly busy: boolean;
  /** True when the state differs from the last save point. */
  readonly dirty: boolean;
}
