// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Where each pre-rendered track stands (rendering, ready or waiting),
 * shared by the Audio FX panel's button and the track handle's marker.
 * The core says when it changes; `watchPrerenders` listens while a
 * component is on screen.
 */

import { writable, type Readable } from "svelte/store";
import { onPrerenderChanged, prerenderStatus, type PrerenderStatus } from "../app/core";

export type PrerenderState = PrerenderStatus["state"];

const states = writable<Record<string, PrerenderState>>({});

/** Each track's pre-render state, by track id. A track with Pre-render on but no entry yet is waiting. */
export const prerenderStates: Readable<Record<string, PrerenderState>> = { subscribe: states.subscribe };

/** Sets the store only when something differs, so nothing redraws for a same answer. */
function setIfChanged(next: Record<string, PrerenderState>) {
  states.update((cur) => {
    const a = Object.keys(cur);
    return a.length === Object.keys(next).length && a.every((k) => cur[k] === next[k]) ? cur : next;
  });
}

export function refreshPrerenders(): void {
  prerenderStatus()
    .then((list) => setIfChanged(Object.fromEntries(list.map((p) => [p.trackId, p.state]))))
    .catch(() => setIfChanged({}));
}

let watchers = 0;
let stop: (() => void) | null = null;

/** Starts listening (shared by every caller) and reads the current state. Returns the stop function. */
export function watchPrerenders(): () => void {
  refreshPrerenders();
  if (watchers++ === 0) {
    onPrerenderChanged(refreshPrerenders)
      .then((f) => (watchers === 0 ? f() : (stop = f)))
      .catch(() => {});
  }
  let done = false;
  return () => {
    if (done) return;
    done = true;
    if (--watchers === 0) {
      stop?.();
      stop = null;
    }
  };
}

/** Plain words for a state, for tooltips and the button. */
export function prerenderWords(state: PrerenderState | null): string {
  switch (state) {
    case "ready":
      return "Rendered: playback and export use the rendered track, so its effects rest.";
    case "rendering":
      return "Rendering the track's clips and effects now. The live track plays until it is ready.";
    case "waiting":
      return "Pre-render is on and waiting to render.";
    default:
      return "";
  }
}
