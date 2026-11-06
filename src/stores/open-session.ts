// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

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
  transportSetLive,
  transportStatus,
  transportStop,
} from "../app/core";
import { PeakStore } from "./peaks";
import { SessionStore, type Op, type SessionDoc, type ViewDoc } from "./session";
import { TransportStore } from "./transport";
import { AutoWriter } from "./auto-write";
import { sessionHistory } from "./history";
import { fxCatalog } from "./fx-catalog";
import { autoRange, autoRest } from "../panels/editor/automation";
import type { EffectInfo } from "../app/core";

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
export const transport: TransportStore = new TransportStore(
  // While recording, playback runs straight on: it never loops back or
  // skips, so a take can go on past the end of the old audio.
  { play: (from) => transportPlay(from, transport.holdAtEnd ? "normal" : editor.current().playMode), stop: transportStop, status: transportStatus, stopAtEnd: () => editor.current().playMode !== "loop", skipsSelection: () => editor.current().playMode === "skip", loops: () => editor.current().playMode === "loop" },
  session,
);

/**
 * Automation writing for the open session: control moves while playing
 * become lane points on tracks set to Write, Latch or Touch.
 */
let catalog: readonly EffectInfo[] = [];
fxCatalog.subscribe((c) => (catalog = c));
export const autoWriter = new AutoWriter({
  session,
  runner: () => sessionHistory,
  rangeOf: (o, t) => autoRange(o, catalog, t),
  restOf: (o, t) => autoRest(o, catalog, t),
  onLive: (lanes) => {
    if (isTauri()) void transportSetLive(lanes).catch(console.error);
  },
});
session.rewrite = (op, label) => autoWriter.rewrite(op, label);
transport.subscribe((s) => autoWriter.onTransport(s));
