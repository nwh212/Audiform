// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/** The open multitrack session, shown as the first row of the Files panel. */
export interface SessionRow {
  id: string;
  name: string;
  dirty: boolean;
  /** Length of the audio in the session, in seconds. */
  seconds: number;
  sampleRate: number;
  /** True while the Multitrack view is showing it. */
  active: boolean;
  /** In front: the session the Multitrack editor shows. False for sessions set aside. */
  front: boolean;
}

/** Seconds as m:ss.mmm (or h:mm:ss.mmm). */
export function sessionDuration(seconds: number): string {
  const ms = Math.max(0, Math.round(seconds * 1000));
  const h = Math.floor(ms / 3_600_000);
  const m = Math.floor((ms % 3_600_000) / 60_000);
  const sec = Math.floor((ms % 60_000) / 1000);
  const frac = String(ms % 1000).padStart(3, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${String(sec).padStart(2, "0")}.${frac}` : `${m}:${String(sec).padStart(2, "0")}.${frac}`;
}
