// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import type { Command } from "./types";

/**
 * Several commands recorded as one history entry, such as deleting
 * five selected clips. Runs children in order and undoes them in
 * reverse. If a child fails partway, the children that already ran
 * are rolled back before the error is rethrown.
 */
export class CompoundCommand implements Command {
  readonly label: string;
  private readonly children: readonly Command[];

  constructor(label: string, children: readonly Command[]) {
    this.label = label;
    this.children = [...children];
  }

  async do(): Promise<void> {
    const done: Command[] = [];
    try {
      for (const child of this.children) {
        await child.do();
        done.push(child);
      }
    } catch (error) {
      for (const child of done.reverse()) {
        await child.undo();
      }
      throw error;
    }
  }

  async undo(): Promise<void> {
    for (const child of [...this.children].reverse()) {
      await child.undo();
    }
  }
}
