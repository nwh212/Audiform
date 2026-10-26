// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { loadPreferences, savePreferences } from "../app/core";
import { Preferences } from "./preferences";

/** The app's interface preferences. Loaded at startup by App. */
export const prefs = new Preferences({ load: loadPreferences, save: savePreferences });
