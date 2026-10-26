// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { expandMediaPaths, probeMedia } from "../app/core";
import { pathsIgnoreCase } from "../lib/platform";
import { MediaPool } from "./media-pool";

/**
 * Audio files in the open session. Saved with the session in task 12,
 * where each ready item becomes a MediaRef.
 */
export const mediaPool = new MediaPool({
  probe: probeMedia,
  expand: expandMediaPaths,
  ignorePathCase: pathsIgnoreCase(),
});
