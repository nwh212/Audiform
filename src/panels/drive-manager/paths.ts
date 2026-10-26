// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * The folder above `path`, or null at a drive or filesystem root.
 * Handles Windows (`C:\Shows\Raw`) and POSIX (`/home/host/Raw`) paths.
 */
export function parentFolder(path: string): string | null {
  if (!path) return null;
  const win = /^[A-Za-z]:[\\/]?$/;
  if (win.test(path) || path === "/") return null;
  const sep = path.includes("\\") ? "\\" : "/";
  const trimmed = path.endsWith(sep) ? path.slice(0, -1) : path;
  const i = trimmed.lastIndexOf(sep);
  if (i < 0) return null;
  const up = trimmed.slice(0, i);
  if (/^[A-Za-z]:$/.test(up)) return `${up}\\`;
  return up === "" ? "/" : up;
}

/** Date for a listing column: "Oct 13, 2026". */
export function shortDate(ms: number): string {
  return new Date(ms).toLocaleDateString("en-US", { month: "short", day: "numeric", year: "numeric" });
}
