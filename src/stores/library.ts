// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import {
  libraryCreate,
  libraryDefaultRoot,
  libraryImport,
  libraryOpen,
  librarySave,
} from "../app/core";
import { LibraryStore } from "./libraries";
import { prefs } from "./prefs";

const KEY_OPEN = "libraries.open";
const KEY_ACTIVE = "libraries.active";

/** The app's open libraries. Restored by App after preferences load. */
export const libraries = new LibraryStore(
  {
    defaultRoot: libraryDefaultRoot,
    create: libraryCreate,
    open: libraryOpen,
    save: librarySave,
    import: libraryImport,
  },
  {
    get: () => {
      const raw = prefs.raw(KEY_OPEN);
      const open = Array.isArray(raw) ? raw.filter((v): v is string => typeof v === "string") : [];
      const active = prefs.string(KEY_ACTIVE, "");
      return { open, active: active || null };
    },
    set: (open, active) => {
      prefs.set(KEY_OPEN, open);
      prefs.set(KEY_ACTIVE, active ?? "");
    },
  },
);
