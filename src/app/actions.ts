// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Named app actions. Menus, shortcuts and buttons all run actions by id,
 * so each behavior has one implementation. A menu item whose id has no
 * registered action shows disabled: the feature is not built yet.
 */

import { writable, type Readable } from "svelte/store";

export interface Action {
  id: string;
  run: () => void | Promise<void>;
  /** Defaults to enabled. */
  enabled?: () => boolean;
  /** For toggle items. Undefined means not a toggle. */
  checked?: () => boolean;
  /** Overrides the menu label, for example "Undo Add Track". */
  label?: () => string | null;
  /** Tooltip while disabled, for example "Select files in the Files panel first." */
  reason?: () => string | null;
  /** Defaults to shown. Hidden items are left out of menus, such as empty Open Recent slots. */
  visible?: () => boolean;
}

export class ActionRegistry {
  private readonly actions = new Map<string, Action>();
  private readonly overlays = new Map<string, { when: () => boolean; action: Action }[]>();
  private readonly tick = writable(0);
  private n = 0;

  /** Changes whenever any action's state may have changed. Menus re-read on change. */
  readonly changed: Readable<number> = this.tick;

  register(action: Action): () => void {
    this.actions.set(action.id, action);
    this.invalidate();
    return () => {
      if (this.actions.get(action.id) === action) this.actions.delete(action.id);
      this.invalidate();
    };
  }

  /**
   * Lets another action stand in for `action.id` while `when()` is true,
   * for example Edit > Delete acting on the Waveform editor while it
   * shows. The newest overlay whose condition holds wins.
   */
  overlay(when: () => boolean, action: Action): () => void {
    const list = this.overlays.get(action.id) ?? [];
    const entry = { when, action };
    this.overlays.set(action.id, [...list, entry]);
    this.invalidate();
    return () => {
      this.overlays.set(
        action.id,
        (this.overlays.get(action.id) ?? []).filter((e) => e !== entry),
      );
      this.invalidate();
    };
  }

  private get(id: string): Action | undefined {
    const list = this.overlays.get(id);
    if (list) {
      for (let i = list.length - 1; i >= 0; i--) if (list[i]!.when()) return list[i]!.action;
    }
    return this.actions.get(id);
  }

  has(id: string): boolean {
    return this.get(id) !== undefined;
  }

  isEnabled(id: string): boolean {
    const a = this.get(id);
    return a !== undefined && (a.enabled?.() ?? true);
  }

  isChecked(id: string): boolean | undefined {
    return this.get(id)?.checked?.();
  }

  label(id: string): string | null {
    return this.get(id)?.label?.() ?? null;
  }

  isVisible(id: string): boolean {
    return this.get(id)?.visible?.() ?? true;
  }

  /** Why a registered action is disabled right now, if it says. */
  reason(id: string): string | null {
    const a = this.get(id);
    if (!a || (a.enabled?.() ?? true)) return null;
    return a.reason?.() ?? null;
  }

  /** Runs an action if it exists and is enabled. Returns whether it ran. */
  async run(id: string): Promise<boolean> {
    const a = this.get(id);
    if (!a || !(a.enabled?.() ?? true)) return false;
    await a.run();
    this.invalidate();
    return true;
  }

  /** Tells menus to re-read enabled, checked and label state. */
  invalidate(): void {
    this.tick.set(++this.n);
  }
}
