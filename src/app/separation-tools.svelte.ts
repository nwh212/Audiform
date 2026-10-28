// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Audio Separation: what the panel separates, the first-use download,
 * running the engine and placing the results.
 *
 * What gets separated follows what is in front: the file open in the
 * Waveform editor, else the selected clips in the session, else the files
 * selected in the Files panel. Results always go into the Files panel.
 * For clips, Audiform then offers two new tracks under each clip's track,
 * copying that track's effects and settings, with copies of the clips
 * playing the results. One undo removes them.
 */

import { findClips } from "../panels/editor/clip-edit";
import { sizeText } from "./settings/pages";
import { editor } from "../stores/editor";
import { sessionHistory } from "../stores/history";
import { mediaPool } from "../stores/media";
import { session } from "../stores/open-session";
import type { Op, TrackDoc } from "../stores/session";
import { settings } from "../stores/settings";
import { waveFile } from "../stores/wave";
import {
  onSeparationProgress,
  separationCancel,
  separationCheckEngine,
  separationDownload,
  separationRemove,
  separationRun,
  separationStatus,
  type SeparationModel,
  type SeparationResult,
  type SeparationStatus,
} from "./core";
import { separatedTrack } from "./separation";

export interface SeparationToolsDeps {
  showError: (e: unknown) => void;
  showNotice: (text: string) => void;
  ask: (title: string, message: string, buttons: { label: string; value: string; primary?: boolean }[]) => Promise<string>;
  /** The Waveform editor is the one in front. */
  inWave: () => boolean;
}

/** What Separate works on right now. */
export type SeparationSource =
  | { kind: "files"; paths: string[]; label: string }
  | { kind: "clips"; clipIds: string[]; label: string };

/** A download or separation in progress. */
export interface SeparationJob {
  title: string;
  /** 0..1, or null while the size is not known. */
  progress: number | null;
  detail: string;
}

export function createSeparationTools(deps: SeparationToolsDeps) {
  const { showError, showNotice, ask, inWave } = deps;
  let status = $state<SeparationStatus | null>(null);
  let job = $state<SeparationJob | null>(null);
  /** The last "check for update" on the engine: unknown until asked. */
  let engineUpdate = $state<"unknown" | "current" | "available">("unknown");

  async function refresh() {
    try {
      status = await separationStatus();
    } catch (e) {
      showError(e);
    }
  }

  const fileName = (p: string) => p.split(/[\\/]/).pop() ?? p;
  const dirOf = (p: string) => p.slice(0, Math.max(p.lastIndexOf("/"), p.lastIndexOf("\\")));
  const sepOf = (p: string) => (p.includes("\\") ? "\\" : "/");

  /** What Separate would work on, or null with nothing to separate. */
  function source(): SeparationSource | null {
    const wf = waveFile.current();
    if (inWave() && wf && !wf.untitled) return { kind: "files", paths: [wf.path], label: fileName(wf.path) };
    const doc = session.current;
    if (doc && !inWave()) {
      const found = findClips(doc, editor.current().selectedClips);
      if (found.length) {
        const tracks = new Set(found.map((f) => f.track.id)).size;
        return {
          kind: "clips",
          clipIds: found.map((f) => f.clip.id),
          label: `${found.length === 1 ? `"${found[0]!.clip.name}"` : `${found.length} clips`}${tracks > 1 ? ` on ${tracks} tracks` : ""}`,
        };
      }
    }
    const files = mediaPool.selectedItems().filter((it) => it.status === "ready");
    if (files.length) return { kind: "files", paths: files.map((f) => f.path), label: files.length === 1 ? fileName(files[0]!.path) : `${files.length} files` };
    return null;
  }

  /** Runs `fn` with a progress line; downloads and runs report through the same event. */
  async function withProgress<T>(title: string, detail: string, fn: () => Promise<T>): Promise<T> {
    job = { title, progress: null, detail };
    const off = await onSeparationProgress((p) => {
      if (!job) return;
      if (p.stage === "download") {
        job = { ...job, progress: p.total > 0 ? p.done / p.total : null, detail: p.total > 0 ? `${sizeText(p.done)} of ${sizeText(p.total)}` : sizeText(p.done) };
      } else {
        job = { ...job, progress: p.done, detail: `${Math.round(p.done * 100)}%` };
      }
    });
    try {
      return await fn();
    } finally {
      off();
      job = null;
    }
  }

  async function download(id: string): Promise<boolean> {
    const s = status ?? (await separationStatus());
    const what = id === "engine" ? "the separation engine" : (s.models.find((m) => m.id === id)?.name ?? id);
    try {
      await withProgress(`Downloading ${what}`, "", () => separationDownload(id));
      if (id === "engine") engineUpdate = "current";
      return true;
    } catch (e) {
      if (!String(e).includes("cancelled")) showError(e);
      return false;
    } finally {
      await refresh();
    }
  }

  async function remove(m: SeparationModel) {
    const ok = await ask(`Remove ${m.name}?`, `This deletes the downloaded model (${sizeText(m.bytes)}) from Audiform's files. You can download it again any time.`, [
      { label: "Remove", value: "yes", primary: true },
      { label: "Cancel", value: "no" },
    ]);
    if (ok !== "yes") return;
    try {
      await separationRemove(m.id);
    } catch (e) {
      showError(e);
    }
    await refresh();
  }

  /** Asks whether a newer engine is published (fetches a small checksum file). */
  async function checkEngine() {
    try {
      engineUpdate = (await separationCheckEngine()) ? "available" : "current";
      if (engineUpdate === "current") showNotice("The separation engine is up to date.");
    } catch (e) {
      showError(e);
    }
  }

  async function removeEngine() {
    const ok = await ask("Remove the separation engine?", "This deletes the downloaded separation engine from Audiform's files. Your models stay. It is downloaded again the next time you separate.", [
      { label: "Remove", value: "yes", primary: true },
      { label: "Cancel", value: "no" },
    ]);
    if (ok !== "yes") return;
    try {
      await separationRemove("engine");
      engineUpdate = "unknown";
    } catch (e) {
      showError(e);
    }
    await refresh();
  }

  /** First use: offers to download what is missing. False if the user said no or it failed. */
  async function ensureReady(m: SeparationModel): Promise<boolean> {
    const s = status ?? (await separationStatus());
    if (!s.engineAvailable) {
      showNotice("Audio Separation is not available for this computer yet.");
      return false;
    }
    const needEngine = !s.engineInstalled;
    const needModel = !s.models.find((x) => x.id === m.id)?.installed;
    if (!needEngine && !needModel) return true;
    const parts = [needModel ? `${m.name} (${sizeText(m.bytes)})` : null, needEngine ? "the separation engine (about 10 MB, once)" : null].filter(Boolean);
    const ok = await ask(
      "Download first?",
      `To separate with ${m.name}, Audiform needs to download ${parts.join(" and ")} into its own files, where it is kept for next time. Only what is needed for demixing is downloaded; separation itself runs on your own computer.`,
      [
        { label: "Download", value: "yes", primary: true },
        { label: "Cancel", value: "no" },
      ],
    );
    if (ok !== "yes") return false;
    if (needEngine && !(await download("engine"))) return false;
    if (needModel && !(await download(m.id))) return false;
    return true;
  }

  /** Where results go: a Separated folder beside the saved session, else beside the source file. */
  function outFolder(sourcePath: string): string {
    const sp = session.path;
    const base = sp && !inWave() ? dirOf(sp) : dirOf(sourcePath);
    return `${base}${sepOf(base)}Separated`;
  }

  /** One file through the engine, trying the processor if the graphics card cannot be used. */
  async function runOne(path: string, m: SeparationModel, n: string): Promise<SeparationResult> {
    const gpu = settings.current.separationDevice === "gpu";
    const overlap = settings.current.separationOverlap;
    return withProgress(`Separating ${fileName(path)}${n}`, "Starting", async () => {
      try {
        return await separationRun(path, m.id, gpu, overlap, outFolder(path));
      } catch (e) {
        if (gpu && String(e).startsWith("GPU:")) {
          const go = await ask("Use the processor?", "The graphics card could not be used on this computer. Separate on the processor instead? It is slower. (Settings > Audio Separation can turn the graphics card off.)", [
            { label: "Use the Processor", value: "yes", primary: true },
            { label: "Cancel", value: "no" },
          ]);
          if (go === "yes") return separationRun(path, m.id, false, overlap, outFolder(path));
        }
        throw e;
      }
    });
  }

  async function separate(m: SeparationModel) {
    const src = source();
    if (!src || job) return;
    if (!(await ensureReady(m))) return;
    const doc0 = session.current;
    const found = src.kind === "clips" && doc0 ? findClips(doc0, src.clipIds) : [];
    // Each source file once, even when several clips play it.
    const paths =
      src.kind === "files"
        ? src.paths
        : [...new Set(found.map((f) => doc0!.media.find((x) => x.id === f.clip.media_id)?.path).filter((p): p is string => !!p))];
    const results = new Map<string, SeparationResult>();
    try {
      for (const [i, p] of paths.entries()) results.set(p, await runOne(p, m, paths.length > 1 ? ` (${i + 1} of ${paths.length})` : ""));
    } catch (e) {
      if (!String(e).includes("cancelled")) showError(e);
      if (results.size === 0) return;
    }
    const outs = [...results.values()].flatMap((r) => [r.target, r.rest]);
    const imported = await mediaPool.importPaths(outs, sessionHistory);
    await mediaPool.whenIdle();
    showNotice(`Separated ${results.size === 1 ? fileName([...results.keys()][0]!) : `${results.size} files`} into ${m.targetName} and ${m.restName}; the results are in the Files panel.`);
    if (src.kind !== "clips" || !session.current) return;

    const go = await ask("Add as new tracks?", `Put the ${m.targetName.toLowerCase()} and ${m.restName.toLowerCase()} on two new tracks below each clip's track? They copy that track's effects and settings.`, [
      { label: "Add Tracks", value: "yes", primary: true },
      { label: "Files Only", value: "no" },
    ]);
    if (go !== "yes") return;
    const doc = session.current;
    const known = new Set(doc.media.map((x) => x.id));
    const ops: Op[] = [];
    // Media ids for each result, added to the session as needed.
    const mediaFor = (path: string) => {
      const idx = outs.indexOf(path);
      const it = idx >= 0 ? mediaPool.get(imported.ids[idx]!) : undefined;
      if (!it || it.status !== "ready" || !it.info) return undefined;
      if (!known.has(it.id)) {
        known.add(it.id);
        ops.push({ op: "add_media", media: { id: it.id, path: it.path, relative_path: null, sample_rate: it.info.sampleRate, channels: it.info.channels, length: it.info.length } });
      }
      return { id: it.id, length: it.info.length };
    };
    const byTrack = new Map<string, { track: TrackDoc; index: number; clips: typeof found }>();
    for (const f of found) {
      const g = byTrack.get(f.track.id) ?? { track: f.track, index: f.trackIndex, clips: [] };
      g.clips.push(f);
      byTrack.set(f.track.id, g);
    }
    // Bottom track first, so the indexes above stay right.
    const groups = [...byTrack.values()].sort((a, b) => b.index - a.index);
    const uuid = () => crypto.randomUUID();
    const pathOfMedia = (id: string) => doc.media.find((x) => x.id === id)?.path;
    let added = 0;
    for (const g of groups) {
      const clips = g.clips.map((f) => f.clip);
      const pick = (which: "target" | "rest") => (oldMedia: string) => {
        const r = results.get(pathOfMedia(oldMedia) ?? "");
        return r ? mediaFor(r[which]) : undefined;
      };
      const a = separatedTrack(g.track, clips, pick("target"), `${g.track.name} ${m.targetName}`, uuid);
      const b = separatedTrack(g.track, clips, pick("rest"), `${g.track.name} ${m.restName}`, uuid);
      if (a.clips.length === 0 && b.clips.length === 0) continue;
      ops.push({ op: "restore_track", track: a, index: g.index + 1 }, { op: "restore_track", track: b, index: g.index + 2 });
      added += 2;
    }
    if (added === 0) return;
    // Media first, then tracks: restore_track needs the media to exist.
    ops.sort((x, y) => (x.op === "add_media" ? 0 : 1) - (y.op === "add_media" ? 0 : 1));
    try {
      await session.edit(`Add Separated Tracks`, { op: "batch", ops }, sessionHistory);
    } catch (e) {
      showError(e);
    }
  }

  return {
    get status() {
      return status;
    },
    get job() {
      return job;
    },
    get engineUpdate() {
      return engineUpdate;
    },
    checkEngine,
    refresh,
    source,
    download,
    remove,
    removeEngine,
    separate,
    cancel: () => void separationCancel(),
  };
}

export type SeparationTools = ReturnType<typeof createSeparationTools>;
