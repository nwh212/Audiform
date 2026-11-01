// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * One effects catalog and one plug-in registry for the whole interface,
 * fetched once per plug-in scan and shared by every caller. Both are
 * large (every plug-in with its controls: tens of MB each), so a
 * component fetching and keeping a copy of its own, as the sends row of
 * every track and bus once did, cost hundreds of MB.
 */

import { get, readable, type Readable } from "svelte/store";
import { effectsCatalog, pluginsRegistry, type EffectInfo } from "../app/core";
import { pluginsVersion } from "./plugins";

/** A value fetched once per `version` and handed to every caller; a failed fetch is tried again next time. */
export function shared<T>(fetch: () => Promise<T>, version: Readable<number>): () => Promise<T> {
  let seen = -1;
  let pending: Promise<T> | null = null;
  return () => {
    const v = get(version);
    if (!pending || v !== seen) {
      seen = v;
      const p = fetch();
      pending = p;
      p.catch(() => {
        if (pending === p) pending = null;
      });
    }
    return pending;
  };
}

/** The effects catalog: built-in effects and scanned plug-ins, with their controls. Shared: never change what it holds. */
export const sharedCatalog = shared(effectsCatalog, pluginsVersion);

/** The plug-in registry: folders and scanned plug-ins. Shared: never change what it holds. */
export const sharedRegistry = shared(pluginsRegistry, pluginsVersion);

/** The shared catalog as a store: empty until loaded, loaded again after each scan. */
export const fxCatalog: Readable<readonly EffectInfo[]> = readable<readonly EffectInfo[]>([], (set) =>
  pluginsVersion.subscribe(() => {
    sharedCatalog()
      .then(set)
      .catch(() => set([]));
  }),
);
