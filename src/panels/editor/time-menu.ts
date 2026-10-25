// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

/** The time ruler's right-click choices: how times show, and snapping to the ruler's marks. */

import type { ContextItem } from "../../app/ContextMenu.svelte";
import { editor, type TimeFormat } from "../../stores/editor";

const FORMATS: [TimeFormat, string][] = [
  ["time", "Minutes and Seconds"],
  ["seconds", "Seconds"],
  ["bars", "Bars and Beats"],
  ["samples", "Samples"],
  ["timecode", "Timecode"],
];

export function timeDisplayItems(): ContextItem[] {
  const ed = editor.current();
  return [
    {
      kind: "sub",
      label: "Time Display",
      items: FORMATS.map(([f, label]): ContextItem => ({ kind: "item", label, checked: ed.timeFormat === f, run: () => editor.setTimeFormat(f) })),
    },
    { kind: "item", label: "Snap to Ruler Marks", checked: ed.snap.ruler, run: () => editor.setSnap({ ruler: !ed.snap.ruler }) },
  ];
}
