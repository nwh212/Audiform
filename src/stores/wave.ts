// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * The Waveform editor's document: one audio file, held by the core as a
 * one-track session whose clips are the edit list. It has its own undo
 * history and its own transport, so it never disturbs the multitrack
 * session.
 */

import { editor } from "./editor";
import { writable, type Readable } from "svelte/store";
import { UndoStack } from "../commands";
import type { StashedHistory } from "../commands/undo-stack";
import { transportStatus, transportStop, waveApply, waveClose, waveForget, waveNew, waveNewFromClips, type MixdownResult, waveOpen, wavePark, wavePlay, waveResume, waveSave, waveSetView, type WaveDocRaw } from "../app/core";
import { SessionStore, type ClipDoc, type MediaRefDoc, type Op, type SessionDoc, type ViewDoc } from "./session";
import { TransportStore } from "./transport";
import type { SpectralShape } from "../panels/editor/spectrum";

export interface WaveFile {
  /** The file being edited. */
  path: string;
  /** Container, for example "WAV". Only WAV files save in place. */
  format: string;
  bitDepth: number | null;
  /** Not saved anywhere yet: Save goes to Save As. */
  untitled: boolean;
}

const fileState = writable<WaveFile | null>(null);
let fileValue: WaveFile | null = null;
fileState.subscribe((v) => (fileValue = v));

/** The open file, or null when the Waveform editor is empty. */
export const waveFile: Readable<WaveFile | null> & { current(): WaveFile | null } = {
  subscribe: fileState.subscribe,
  current: () => fileValue,
};

/** The edit list, mirrored from the core. */
export const wave = new SessionStore({
  get: async () => {
    throw new Error("Open a file in the Waveform editor first.");
  },
  apply: (op: Op) => waveApply<Op, SessionDoc>(op),
  setView: (view: ViewDoc) => waveSetView(view),
});

/** Zoom commands of the visible Waveform view, set while it is mounted. */
export const waveZoom: { run: ((kind: "in" | "out" | "full" | "selection" | "amp-in" | "amp-out") => void) | null } = { run: null };

/** Undo history for the Waveform editor's file. */
export const waveHistory = new UndoStack({ baseLabel: "Open" });

/** Play, pause and stop for the Waveform editor's file. */
export const waveTransport = new TransportStore({ play: (from) => wavePlay(from, editor.current().playMode), stop: transportStop, status: transportStatus, stopAtEnd: () => editor.current().playMode !== "loop", skipsSelection: () => editor.current().playMode === "skip" }, wave);

function adopt(doc: WaveDocRaw<SessionDoc>): void {
  wave.set(doc.session, doc.path);
  fileState.set({ path: doc.path, format: doc.format, bitDepth: doc.bit_depth, untitled: doc.untitled });
}

/**
 * Files set aside when another one was opened, keyed by path, with their
 * undo history. Switching files never asks to save: the edits wait here
 * until the file is opened again, saved, or the app quits.
 */
const parked = new Map<string, { history: StashedHistory; dirty: boolean; format: string; bitDepth: number | null; untitled: boolean }>();

/** Names of set-aside files with unsaved edits. */
export function parkedUnsaved(): string[] {
  return [...parked.entries()].filter(([, v]) => v.dirty).map(([p]) => p.split(/[\\/]/).pop() ?? p);
}

/** Sets the open file aside, if any, leaving the editor empty. */
async function parkCurrent(): Promise<void> {
  const f = fileValue;
  if (!f) return;
  const dirty = waveHistory.current.dirty;
  const history = await waveHistory.stash();
  await wavePark();
  parked.set(f.path, { history, dirty, format: f.format, bitDepth: f.bitDepth, untitled: f.untitled });
  fileState.set(null);
}

/** Opens a file. The one open now is set aside with its edits, not closed. */
export async function openWaveFile(path: string): Promise<void> {
  if (fileValue?.path === path) return;
  spectralSelection.set(null);
  await waveTransport.stop();
  await parkCurrent();
  const back = parked.get(path);
  const kept = back ? await waveResume<SessionDoc>(path) : null;
  if (back && kept) {
    parked.delete(path);
    adopt(kept);
    await waveHistory.restore(back.history);
    return;
  }
  parked.delete(path);
  const doc = await waveOpen<SessionDoc>(path);
  adopt(doc);
  await waveHistory.clear("Open");
}

/** Drops every set-aside file and its edits (after the user chose to discard them). */
export async function forgetParked(): Promise<void> {
  for (const p of [...parked.keys()]) {
    parked.delete(p);
    await waveForget(p);
  }
}

let untitledCount = 0;

/** Paste to New File: a new untitled file made from clips placed from 0. */
export async function newWaveFromClips(sampleRate: number, clips: ClipDoc[], media: MediaRefDoc[]): Promise<void> {
  spectralSelection.set(null);
  await waveTransport.stop();
  await parkCurrent();
  const channels = Math.min(2, Math.max(1, ...media.map((m) => m.channels)));
  untitledCount += 1;
  const doc = await waveNewFromClips<SessionDoc, ClipDoc, MediaRefDoc>(`Untitled ${untitledCount}`, sampleRate, channels, clips, media);
  adopt(doc);
  await waveHistory.clear("New");
  // A new file is unsaved work.
  await waveHistory.markUnsaved();
}

/** Mix Down to New File: the session's mix as a new untitled file. Null when cancelled. */
/** File > New Audio File: an empty untitled file. */
export async function newEmptyWaveFile(name: string, sampleRate: number, channels: number, bitDepth: number): Promise<void> {
  spectralSelection.set(null);
  await waveTransport.stop();
  await parkCurrent();
  const doc = await waveNew<SessionDoc>(name, sampleRate, channels, bitDepth);
  adopt(doc);
  await waveHistory.clear("New");
}

export async function closeWaveFile(): Promise<void> {
  spectralSelection.set(null);
  await waveTransport.stop();
  await waveClose();
  fileState.set(null);
  await waveHistory.clear("Open");
}

/** True when the file is a WAV, the only kind saved in place. */
export function savesInPlace(f: WaveFile | null): boolean {
  return !!f && !f.untitled && ["WAV", "FLAC", "MP3"].includes(f.format.toUpperCase());
}

/**
 * Saves the edits. Null `path` replaces the open file; a path writes a
 * new file and the editor moves to it. Either way the edit list starts
 * fresh on the saved audio, so the history is cleared.
 */
export async function saveWaveFile(path: string | null, bitDepth: number | null = null): Promise<void> {
  await waveTransport.stop();
  const doc = await waveSave<SessionDoc>(path, bitDepth);
  adopt(doc);
  await waveHistory.clear(path ? "Saved As" : "Saved");
}

/**
 * Live copies of files with unsaved Waveform editor edits, by the file's
 * path: the session plays, exports and draws these in their place until
 * the edits are saved or dropped (see src-tauri/src/commands/live.rs).
 */
export const liveCopies = writable<Record<string, { path: string; length: number }>>({});

/** The same file, however its path is spelled. */
export function samePath(a: string, b: string): boolean {
  const n = (p: string) => p.replace(/\\/g, "/").toLowerCase();
  return a === b || n(a) === n(b);
}

/** The live copy standing in for `path`, if any. */
export function liveCopyOf(copies: Record<string, { path: string; length: number }>, path: string): { path: string; length: number } | null {
  for (const [k, v] of Object.entries(copies)) if (samePath(k, path)) return v;
  return null;
}

/** The current spectral selection in the Waveform editor: boxes, lassos and brush strokes. Null when none. */
export const spectralSelection = writable<readonly SpectralShape[] | null>(null);
