// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Desktop notifications when long work finishes: on Linux only, when the
 * setting is on, and only while Audiform's window is in the background
 * (in front, the message at the bottom of the window is enough). Callers
 * show their in-app message as before and call this beside it.
 */

import { isTauri } from "@tauri-apps/api/core";
import { PLATFORM, type Platform } from "../lib/platform";
import { settings } from "../stores/settings";
import { desktopNotify } from "./core";

/** Whether a finished job should also tell the desktop. */
export function shouldNotify(platform: Platform, enabled: boolean, focused: boolean): boolean {
  return platform === "linux" && enabled && !focused;
}

/** Tells the desktop that long work finished, when it should (see above). Never throws. */
export function notifyFinished(text: string): void {
  if (!isTauri() || !shouldNotify(PLATFORM, settings.current.desktopNotifications, document.hasFocus())) return;
  desktopNotify(text).catch(() => {});
}
