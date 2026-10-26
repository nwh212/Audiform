// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * The few places the interface differs by operating system. Windows
 * ships first, Linux (Zorin OS) second, macOS third. Keep every
 * platform difference in the interface here so it is easy to audit.
 */

export type Platform = "windows" | "linux" | "macos";

export function detectPlatform(userAgent: string): Platform {
  if (/Windows/i.test(userAgent)) return "windows";
  if (/Mac OS X|Macintosh/i.test(userAgent)) return "macos";
  return "linux";
}

export const PLATFORM: Platform =
  typeof navigator === "undefined" ? "windows" : detectPlatform(navigator.userAgent);

/** What users call the system file browser. */
export function fileManagerName(p: Platform = PLATFORM): string {
  switch (p) {
    case "windows":
      return "File Explorer";
    case "macos":
      return "Finder";
    case "linux":
      return "your file manager";
  }
}

/** Menu label for opening a file's folder with the file selected. */
export function revealLabel(p: Platform = PLATFORM): string {
  switch (p) {
    case "windows":
      return "Show in Explorer";
    case "macos":
      return "Reveal in Finder";
    case "linux":
      return "Show in File Manager";
  }
}

/**
 * Whether two paths that differ only in letter case are the same file.
 * True on Windows and on default macOS disks, false on Linux.
 */
export function pathsIgnoreCase(p: Platform = PLATFORM): boolean {
  return p !== "linux";
}
