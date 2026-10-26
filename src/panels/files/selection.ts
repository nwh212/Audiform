// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/** Standard Windows list selection: click, Ctrl+click, Shift+click. */
export function clickSelection(
  current: ReadonlySet<string>,
  anchor: string | null,
  clicked: string,
  visible: readonly string[],
  mods: { ctrl: boolean; shift: boolean },
): { selected: string[]; anchor: string } {
  if (mods.shift && anchor !== null && visible.includes(anchor)) {
    const a = visible.indexOf(anchor);
    const b = visible.indexOf(clicked);
    const range = visible.slice(Math.min(a, b), Math.max(a, b) + 1);
    const base = mods.ctrl ? [...current].filter((id) => !range.includes(id)) : [];
    return { selected: [...base, ...range], anchor };
  }
  if (mods.ctrl) {
    const next = new Set(current);
    if (next.has(clicked)) next.delete(clicked);
    else next.add(clicked);
    return { selected: [...next], anchor: clicked };
  }
  return { selected: [clicked], anchor: clicked };
}

/** Arrow-key movement. Returns the id to select, or null to stay put. */
export function stepSelection(visible: readonly string[], from: string | null, delta: number): string | null {
  if (visible.length === 0) return null;
  if (from === null || !visible.includes(from)) return delta > 0 ? visible[0]! : visible[visible.length - 1]!;
  const i = Math.min(visible.length - 1, Math.max(0, visible.indexOf(from) + delta));
  return visible[i]!;
}
