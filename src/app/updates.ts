// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Check for Updates. Help > Check for Updates asks at once; the weekly
 * check (Settings > General, off by default) asks at most once a week,
 * a minute after the app opens and never while playing or recording.
 * A newer version is offered as a link to its release page: nothing is
 * downloaded or installed by the app.
 */

import { openLink, updateCheck, type UpdateInfo } from "./core";
import type { ChoiceButton } from "./ChoiceDialog.svelte";

export const WEEK_MS = 7 * 24 * 60 * 60 * 1000;
/** When the last weekly check ran (ms since 1970), kept in the settings file. */
export const LAST_CHECK_KEY = "updates.lastCheck";

/** True when the weekly check is on and a week has passed since the last one. */
export function dueForCheck(weekly: boolean, last: number, now: number): boolean {
  return weekly && (!Number.isFinite(last) || now - last >= WEEK_MS || now < last);
}

export interface UpdateDeps {
  ask: (title: string, message: string, buttons: ChoiceButton[]) => Promise<string>;
  showNotice: (text: string) => void;
  showError: (e: unknown) => void;
}

/** Help > Check for Updates: says at once that it is checking, then shows what it found in a dialog, offering the release page when newer. */
export async function checkNow(deps: UpdateDeps): Promise<UpdateInfo | null> {
  deps.showNotice("Checking for updates...");
  let info: UpdateInfo;
  try {
    info = await updateCheck();
  } catch (e) {
    await deps.ask("Check for Updates", `Could not check for updates: ${e instanceof Error ? e.message : String(e)}`, [{ label: "OK", value: "ok", primary: true }]);
    return null;
  }
  if (!info.newer) {
    await deps.ask("Check for Updates", `You have the newest version, Audiform ${info.current}.`, [{ label: "OK", value: "ok", primary: true }]);
    return info;
  }
  const go = await deps.ask("A new version is out", `Audiform ${info.latest} is out (you have ${info.current}). Open its download page in your browser?`, [
    { label: "Open Download Page", value: "open", primary: true },
    { label: "Not Now", value: "no" },
  ]);
  if (go === "open") await openLink(info.page);
  return info;
}

/** The weekly check: one quiet line when something newer is out, nothing otherwise. */
export async function checkQuietly(showNotice: (text: string) => void): Promise<void> {
  try {
    const info = await updateCheck();
    if (info.newer) showNotice(`Audiform ${info.latest} is out. Help > Check for Updates to get it.`);
  } catch {
    // No connection is not worth a message on a check nobody asked for just now.
  }
}
