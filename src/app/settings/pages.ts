// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

/**
 * Edit > Settings: the pages, and everything the dialog edits as one
 * draft. OK applies the whole draft; Cancel drops it. Settings that
 * already live elsewhere (the editor's time format, the recording
 * options, the loudness region) are edited here too, so one place
 * holds them all.
 */

import type { RecordOptions } from "../recording";
import type { TimeFormat } from "../../stores/editor";
import { sanitizeSettings, type AppSettings } from "../../stores/settings";

export interface SettingsDraft {
  app: AppSettings;
  editor: { followPlayhead: boolean; autoCrossfade: boolean; timeFormat: TimeFormat };
  record: RecordOptions;
  loudness: { region: string; target: string };
}

export const PAGES = [
  "General",
  "Appearance",
  "Audio Devices",
  "Channel Routing",
  "Autosave",
  "Session Media",
  "Editing",
  "Effects",
  "Cache and Files",
  "Markers",
  "Multitrack",
  "Clips",
  "Playback and Recording",
  "Spectrum Views",
  "Time Display",
  "Loudness",
  "Privacy",
] as const;

export type Page = (typeof PAGES)[number];

/** Pages with nothing that works yet: shown, but marked as later. */
export const LATER_PAGES: Partial<Record<Page, string>> = {
  "Channel Routing": "Arrives with ASIO and per-channel routing, which needs real multichannel hardware to build and test.",
};

/** The draft as it should be saved: every value checked and in range. */
export function cleanDraft(d: SettingsDraft): SettingsDraft {
  const clamp = (v: number, lo: number, hi: number, def: number) => (Number.isFinite(v) ? Math.min(hi, Math.max(lo, v)) : def);
  return {
    app: sanitizeSettings(d.app),
    editor: {
      followPlayhead: d.editor.followPlayhead === true,
      autoCrossfade: d.editor.autoCrossfade === true,
      timeFormat: (["samples", "timecode", "seconds", "bars"] as const).find((f) => f === d.editor.timeFormat) ?? "time",
    },
    record: {
      ...d.record,
      offsetMs: Math.round(clamp(d.record.offsetMs, 0, 1000, 0)),
      prerollSec: clamp(d.record.prerollSec, 0, 10, 2),
    },
    loudness: { ...d.loudness },
  };
}

/** Bytes as a short size. */
export function sizeText(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB"];
  let v = bytes / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(v < 10 ? 1 : 0)} ${units[i]}`;
}
