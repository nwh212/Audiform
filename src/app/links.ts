// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

/**
 * Web links shown in the Help menu. A null link shows its menu item
 * disabled until the page exists. The core only opens https links on
 * the hosts in `src-tauri/src/commands/links.rs`.
 *
 * These are the only places the app touches the web, and only when the
 * user picks them. There is no telemetry, update check or sign-in.
 */
export const LINKS = {
  website: "https://thegcrn.com/",
  /** Plugins page. Not live yet. */
  plugins: null as string | null,
  /** Optional support page (Help > Support Altered Audio Editor). Never shown anywhere else. */
  support: "https://paypal.me/scp21" as string | null,
  /** Source code and issues. */
  github: "https://github.com/geekcastradio/altered-audio-editor",
  releases: "https://github.com/geekcastradio/altered-audio-editor/releases/latest",
  /** Opens the bug form. */
  report: "https://github.com/geekcastradio/altered-audio-editor/issues/new/choose",
  /** Community chat invite. */
  discord: "https://discord.gg/xTT5N65y",
  /** The project lead's book, Altered Audio: Podcasting Demystified. */
  book: "https://www.barnesandnoble.com/w/altered-audio-steve-phillips/1147888459",
} as const;
