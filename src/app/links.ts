// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Web links shown in the Help menu: Audiform's own (source, releases,
 * problem reports), then the original project it is based on, Altered
 * Audio Editor by Steve Phillips and The GeekCast Radio Network, LLC
 * (their website, the book, the original source and its support page). A null link shows its menu item
 * disabled until the page exists. The core only opens https links on
 * the hosts in `src-tauri/src/commands/links.rs`.
 *
 * These are the only places the app touches the web, and only when the
 * user picks them. There is no telemetry, update check or sign-in.
 */
export const LINKS = {
  /** Audiform's source code and issues. */
  github: "https://github.com/nwh212/audiform",
  releases: "https://github.com/nwh212/audiform/releases/latest",
  /** Opens Audiform's bug form: problems with Audiform come here, not to the original project. */
  report: "https://github.com/nwh212/audiform/issues/new/choose",
  /** Community chat invite. None for Audiform yet. */
  discord: null as string | null,
  /** The original project, Altered Audio Editor. */
  original: "https://github.com/geekcastradio/altered-audio-editor",
  /** The GeekCast Radio Network, who make Altered Audio Editor. */
  website: "https://thegcrn.com/",
  /** The GCRN plugins page. Not live yet. */
  plugins: null as string | null,
  /** Supports the original author (Help > Support the Original Author). Never shown anywhere else. */
  support: "https://paypal.me/scp21" as string | null,
  /** Steve Phillips's book, Altered Audio: Podcasting Demystified. */
  book: "https://www.barnesandnoble.com/w/altered-audio-steve-phillips/1147888459",
} as const;
