// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Which part of the window the user last worked in. Commands like
 * Delete and Select All act on that area: the Files panel now, the
 * Multitrack view later. Panels mark themselves with `data-area`.
 *
 * Clicks on the menu bar and dialogs do not change it, so Edit > Delete
 * still knows which panel it is for.
 */

import { writable, type Readable } from "svelte/store";

export type Area = "files" | "libraries" | "multitrack" | "waveform" | null;

const area = writable<Area>(null);
let value: Area = null;
area.subscribe((v) => (value = v));

export const activeArea: Readable<Area> = area;

export function currentArea(): Area {
  return value;
}

/**
 * Reads the area for an event target inside the docking area.
 * Returns undefined for popups marked `data-area-keep`, such as a
 * right-click menu, which must not change the area they act on.
 */
export function areaOf(target: EventTarget | null): Area | undefined {
  if (typeof Element === "undefined" || !(target instanceof Element)) return null;
  if (target.closest("[data-area-keep]")) return undefined;
  const el = target.closest<HTMLElement>("[data-area]");
  const name = el?.dataset.area;
  return name === "files" || name === "libraries" || name === "multitrack" || name === "waveform" ? name : null;
}

/** Tracks clicks and focus inside `root`. Returns a function that stops tracking. */
export function trackArea(root: HTMLElement): () => void {
  const update = (e: Event) => {
    const next = areaOf(e.target);
    if (next !== undefined && next !== value) area.set(next);
  };
  root.addEventListener("pointerdown", update, true);
  root.addEventListener("focusin", update, true);
  return () => {
    root.removeEventListener("pointerdown", update, true);
    root.removeEventListener("focusin", update, true);
  };
}
