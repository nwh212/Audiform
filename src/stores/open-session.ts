// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

import { editor } from "./editor";
import { listen } from "@tauri-apps/api/event";
import { isTauri } from "@tauri-apps/api/core";
import {
  peaksEnsure,
  peaksLevel,
  sessionApply,
  sessionGet,
  sessionSetView,
  transportPlay,
  transportStatus,
  transportStop,
} from "../app/core";
import { PeakStore } from "./peaks";
import { SessionStore, type Op, type SessionDoc, type ViewDoc } from "./session";
import { TransportStore } from "./transport";

/** The open session, mirrored from the core. */
export const session = new SessionStore({
  get: () => sessionGet<{ session: SessionDoc; path: string | null }>(),
  apply: (op: Op) => sessionApply<Op, SessionDoc>(op),
  setView: (view: ViewDoc) => sessionSetView(view),
});

/** Waveform peaks for clips in the Editor. */
export const peaks = new PeakStore({
  ensure: peaksEnsure,
  level: peaksLevel,
  onProgress: async (fn) => {
    if (!isTauri()) return () => {};
    return listen<{ path: string; progress: number }>("peaks-progress", (e) => fn(e.payload.path, e.payload.progress));
  },
});

/** Play, pause, stop and meters for the open session. */
export const transport = new TransportStore(
  { play: (from) => transportPlay(from, editor.current().playMode), stop: transportStop, status: transportStatus, stopAtEnd: () => editor.current().playMode !== "loop", skipsSelection: () => editor.current().playMode === "skip" },
  session,
);
