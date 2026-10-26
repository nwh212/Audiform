// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * The last non-empty device lists per driver. An ASIO driver opens for one
 * program at a time, so while it records, a fresh lookup can come back
 * empty; Settings then shows the last list instead of forgetting everything.
 */
const lists = new Map<string, unknown[]>();

/** Returns `fresh` when it has items (and remembers it), else the last good list, else `fresh`. */
export function keepLast<T>(key: string, fresh: T[]): { list: T[]; stale: boolean } {
  if (fresh.length) {
    lists.set(key, fresh);
    return { list: fresh, stale: false };
  }
  const old = lists.get(key) as T[] | undefined;
  return old?.length ? { list: old, stale: true } : { list: fresh, stale: false };
}
