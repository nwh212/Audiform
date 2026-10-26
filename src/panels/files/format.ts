// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/** Display helpers for the Files panel columns. */

import type { AudioInfo } from "../../app/core";
import type { MediaItem } from "../../stores/media-pool";

/** Length as m:ss.mmm, or h:mm:ss.mmm past an hour. */
export function formatDuration(info: AudioInfo): string {
  const totalMs = info.sampleRate > 0 ? Math.round((info.length / info.sampleRate) * 1000) : 0;
  const ms = totalMs % 1000;
  const totalSec = Math.floor(totalMs / 1000);
  const s = totalSec % 60;
  const totalMin = Math.floor(totalSec / 60);
  const m = totalMin % 60;
  const h = Math.floor(totalMin / 60);
  const pad = (n: number, w = 2) => String(n).padStart(w, "0");
  return h > 0 ? `${h}:${pad(m)}:${pad(s)}.${pad(ms, 3)}` : `${m}:${pad(s)}.${pad(ms, 3)}`;
}

export function formatRate(hz: number): string {
  return `${hz.toLocaleString("en-US")} Hz`;
}

export function formatChannels(n: number): string {
  if (n === 1) return "Mono";
  if (n === 2) return "Stereo";
  return `${n} ch`;
}

export function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB"];
  let v = bytes / 1024;
  let u = 0;
  while (v >= 1024 && u < units.length - 1) {
    v /= 1024;
    u++;
  }
  return `${v.toFixed(v < 10 ? 1 : 0)} ${units[u]}`;
}

export function statusText(item: MediaItem): string {
  switch (item.status) {
    case "probing":
      return "Reading";
    case "ready":
      return "Ready";
    case "error":
      return "Error";
  }
}

export type SortKey = "name" | "status" | "duration" | "rate" | "channels";

/** Sorts a copy. Items still being read sort after measured ones. */
export function sortItems(items: readonly MediaItem[], key: SortKey | null, descending: boolean): MediaItem[] {
  if (key === null) return [...items];
  const value = (it: MediaItem): string | number | null => {
    switch (key) {
      case "name":
        return it.name.toLowerCase();
      case "status":
        return it.status;
      case "duration":
        return it.info && it.info.sampleRate > 0 ? it.info.length / it.info.sampleRate : null;
      case "rate":
        return it.info?.sampleRate ?? null;
      case "channels":
        return it.info?.channels ?? null;
    }
  };
  const dir = descending ? -1 : 1;
  return [...items].sort((a, b) => {
    const va = value(a);
    const vb = value(b);
    if (va === null && vb === null) return 0;
    if (va === null) return 1;
    if (vb === null) return -1;
    if (typeof va === "string" && typeof vb === "string") return dir * va.localeCompare(vb, undefined, { numeric: true });
    return dir * ((va as number) - (vb as number));
  });
}

/** Search box match on name, folder, format or codec. */
export function matches(item: MediaItem, query: string): boolean {
  const q = query.trim().toLowerCase();
  if (!q) return true;
  const hay = [item.path, item.info?.format ?? "", item.info?.codec ?? ""].join(" ").toLowerCase();
  return q.split(/\s+/).every((word) => hay.includes(word));
}
