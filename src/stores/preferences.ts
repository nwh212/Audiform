// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Small interface settings saved in the app config folder: last folder,
 * toggles, and the like. Not undoable and not part of a session.
 *
 * Reads are synchronous after `load()`. Writes save after a short pause
 * so a burst of changes is one disk write.
 */

export interface PreferenceBackend {
  load(): Promise<Record<string, unknown>>;
  save(values: Record<string, unknown>): Promise<void>;
}

export class Preferences {
  private values: Record<string, unknown> = {};
  private timer: ReturnType<typeof setTimeout> | undefined;
  private loaded: Promise<void> | null = null;
  private held = false;

  constructor(
    private readonly backend: PreferenceBackend,
    private readonly delayMs = 400,
    private readonly onError: (e: unknown) => void = console.error,
  ) {}

  /** Loads once. Later calls return the same promise. */
  load(): Promise<void> {
    this.loaded ??= this.backend
      .load()
      .then((v) => {
        this.values = { ...v, ...this.values };
      })
      .catch(this.onError);
    return this.loaded;
  }

  /** The stored value as is. Callers must check its shape. */
  raw(key: string): unknown {
    return this.values[key];
  }

  string(key: string, fallback: string): string {
    const v = this.values[key];
    return typeof v === "string" ? v : fallback;
  }

  boolean(key: string, fallback: boolean): boolean {
    const v = this.values[key];
    return typeof v === "boolean" ? v : fallback;
  }

  number(key: string, fallback: number): number {
    const v = this.values[key];
    return typeof v === "number" && Number.isFinite(v) ? v : fallback;
  }

  /** Stops writing to disk (settings were restored from a backup and take effect on the next start). */
  hold(): void {
    this.held = true;
    clearTimeout(this.timer);
  }

  set(key: string, value: unknown): void {
    if (this.values[key] === value) return;
    this.values[key] = value;
    clearTimeout(this.timer);
    this.timer = setTimeout(() => void this.flush(), this.delayMs);
  }

  /** Saves now. */
  async flush(): Promise<void> {
    clearTimeout(this.timer);
    this.timer = undefined;
    if (this.held) return;
    await this.backend.save({ ...this.values }).catch(this.onError);
  }
}
