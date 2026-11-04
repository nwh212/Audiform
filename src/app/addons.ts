// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Add-on windows: the messages an add-on's page and Audiform send each
 * other, and what the page is told about the selection. Pure logic, so
 * it can be unit tested; the window itself is `AddonPanel.svelte`.
 *
 * An add-on's page runs in its own frame with no access to the app. It
 * talks through `postMessage`, and only the messages below are heard.
 * See `docs/addons.md` for the whole contract.
 */

import { findClips } from "../panels/editor/clip-edit";
import type { SessionDoc } from "../stores/session";
import type { SeparationSource } from "./separation-tools.svelte";

/** One file the add-on may work on. */
export interface AddonFile {
  path: string;
  /** File name with extension. */
  name: string;
}

/** What is selected, as the page sees it. */
export interface AddonSelection {
  /** Where the files come from: the Waveform editor or Files panel, clips in the session, or nothing. */
  kind: "files" | "clips" | "none";
  /** A short description, for example "3 clips on 2 tracks". */
  label: string;
  files: AddonFile[];
}

/** Messages a page may send Audiform. */
export type AddonRequest =
  | { type: "ready" }
  | { type: "run"; run: string; args: string[]; input: string }
  | { type: "cancel" }
  | { type: "import"; request: string; paths: string[] }
  | { type: "pick"; request: string; folder: boolean; title: string }
  | { type: "openFolder"; path: string }
  | { type: "notice"; text: string }
  | { type: "save"; data: unknown };

export const EMPTY_SELECTION: AddonSelection = { kind: "none", label: "Nothing selected", files: [] };

export const fileName = (p: string): string => p.split(/[\\/]/).pop() ?? p;

/** The selection as the page sees it: every source file once. */
export function addonSelection(src: SeparationSource | null, doc: SessionDoc | null): AddonSelection {
  if (!src) return EMPTY_SELECTION;
  let paths: string[];
  if (src.kind === "files") paths = src.paths;
  else if (doc) {
    const found = findClips(doc, src.clipIds);
    paths = [...new Set(found.map((f) => doc.media.find((m) => m.id === f.clip.media_id)?.path).filter((p): p is string => !!p))];
  } else paths = [];
  if (paths.length === 0) return EMPTY_SELECTION;
  return { kind: src.kind, label: src.label, files: paths.map((path) => ({ path, name: fileName(path) })) };
}

const isStrings = (v: unknown): v is string[] => Array.isArray(v) && v.every((x) => typeof x === "string");
const MAX_ARGS = 200;
const MAX_INPUT = 4 * 1024 * 1024;

/** Reads one message from a page; null for anything that is not a request we know. */
export function parseAddonRequest(raw: unknown): AddonRequest | null {
  if (typeof raw !== "object" || raw === null) return null;
  const m = raw as Record<string, unknown>;
  const str = (k: string, fallback = ""): string => (typeof m[k] === "string" ? (m[k] as string) : fallback);
  switch (m.type) {
    case "ready":
      return { type: "ready" };
    case "cancel":
      return { type: "cancel" };
    case "run": {
      const args = isStrings(m.args) ? m.args : [];
      const input = str("input");
      if (args.length > MAX_ARGS || input.length > MAX_INPUT) return null;
      return { type: "run", run: str("run", "run"), args, input };
    }
    case "import":
      return isStrings(m.paths) && m.paths.length > 0 ? { type: "import", request: str("request", "import"), paths: m.paths } : null;
    case "pick":
      return { type: "pick", request: str("request", "pick"), folder: m.folder === true, title: str("title") };
    case "openFolder":
      return { type: "openFolder", path: str("path").trim() };
    case "notice":
      return str("text").trim() ? { type: "notice", text: str("text").trim().slice(0, 300) } : null;
    case "save":
      return "data" in m ? { type: "save", data: m.data } : null;
    default:
      return null;
  }
}
