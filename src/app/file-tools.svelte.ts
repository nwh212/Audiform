// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * The File menu: new, open, save, import, templates, export commands,
 * recent sessions and quitting.
 */

import { revealLabel } from "../lib/platform";
import { CHAPTER_FORMATS, type ChapterFormat } from "../panels/markers/chapters";
import { sessionHistory } from "../stores/history";
import { mediaPool } from "../stores/media";
import { session } from "../stores/open-session";
import { prefs } from "../stores/prefs";
import { filePickOpen, templateSave, pickMediaFiles, pickMediaFolder, revealMedia, sessionExportSesx } from "./core";
import { DEFAULT_TARGET, DEFAULT_REGION } from "./loudness";
import { contentEnd } from "./navigation";
import type { ActionRegistry } from "./actions";
import type { createExportTools } from "./export-tools.svelte";
import type { PluginSync } from "./plugin-sync";
import type { SessionFiles } from "./session-files";
import type { LoudnessReport } from "./loudness";
import type { Exporting } from "./wave-tools.svelte";

/** What the File menu needs from the rest of the app. */
export interface FileToolsDeps {
  actions: ActionRegistry;
  showNotice: (text: string) => void;
  showError: (e: unknown) => void;
  promptText: (title: string, label: string, value: string) => Promise<string | null>;
  openPaths: (picked: string[]) => Promise<void>;
  importPaths: (raw: string[]) => Promise<void>;
  exitSafely: () => Promise<void>;
  measureLoudness: () => Promise<void>;
  KEY_LOUD_TARGET: string;
  KEY_LOUD_REGION: string;
  exp: ReturnType<typeof createExportTools>;
  collectMedia: (sessionFile: string, everything: boolean) => Promise<number>;
  plural: (n: number, one: string, many?: string) => string;
  usedMedia: () => Set<string>;
  closeWaveIfRemoved: (ids: readonly string[], remove: () => Promise<void>) => Promise<void>;
  askNewSession: () => Promise<void>;
  pluginSync: PluginSync;
  files: SessionFiles;
  KEY_CHAPTER_FORMAT: string;
  markerDoc: ReturnType<typeof createExportTools>["markerDoc"];
  exportStems: ReturnType<typeof createExportTools>["exportStems"];
  loud: () => { report: LoudnessReport | null; progress: number | null; target: string; region: string; scope: string } | null;
  set_loud: (v: { report: LoudnessReport | null; progress: number | null; target: string; region: string; scope: string } | null) => void;
  exporting: () => Exporting | null;
  set_exporting: (v: Exporting | null) => void;
}

export function createFileTools(deps: FileToolsDeps) {
  const { actions, showNotice, showError, promptText, openPaths, importPaths, exitSafely, measureLoudness, KEY_LOUD_TARGET, KEY_LOUD_REGION, exp, collectMedia, plural, usedMedia, closeWaveIfRemoved, askNewSession, pluginSync, files, KEY_CHAPTER_FORMAT, markerDoc, exportStems, loud, exporting, set_loud, set_exporting } = deps;


  /**
   * File > Open. Sessions open as usual. Audio files go into the Files
   * panel; when the session has no clips yet, each one is also placed on
   * its own track at the start so it can be played and edited right away.
   */
  async function openAny(): Promise<void> {
    await openPaths(await filePickOpen());
  }
  async function saveAsTemplate() {
    const name = await promptText("Save as Session Template", "Template name", session.current?.name ?? "My Show");
    if (!name?.trim()) return;
    try {
      const t = await templateSave(name.trim());
      showNotice(`Saved the template "${t.name}": ${t.tracks} tracks${t.clips ? `, ${t.clips} clips` : ""}. Pick it in File > New Session.`);
    } catch (e) {
      showError(e);
    }
  }

  /** Registers the menu actions. */
  function register(): (() => void)[] {
    return [
      // Unsaved-changes prompt arrives with session save (task 12).
      actions.register({ id: "file.exit", run: () => exitSafely() }),
      actions.register({
        id: "file.import-files",
        run: async () => importPaths(await pickMediaFiles()),
      }),
      actions.register({
        id: "file.import-folder",
        run: async () => {
          const folder = await pickMediaFolder();
          if (folder) await importPaths([folder]);
        },
      }),
      actions.register({
        id: "file.reveal",
        run: () => {
          const first = mediaPool.selectedItems()[0];
          return first ? revealMedia(first.path) : undefined;
        },
        enabled: () => mediaPool.current.selected.size === 1,
        label: () => revealLabel(),
        reason: () => "Select one file in the Files panel.",
      }),
      actions.register({
        id: "file.check-loudness",
        run: () => {
          set_loud({ report: null, progress: null, target: prefs.string(KEY_LOUD_TARGET, DEFAULT_TARGET), region: prefs.string(KEY_LOUD_REGION, DEFAULT_REGION), scope: "" });
          return measureLoudness();
        },
        enabled: () => exporting() === null && contentEnd() > 0,
        reason: () => "The session has no clips yet.",
      }),
      actions.register({
        id: "file.publish-package",
        run: () => exp.openPublishDialog(),
        enabled: () => exporting() === null && contentEnd() > 0,
        reason: () => "The session has no clips yet.",
      }),
      actions.register({
        id: "file.export.sesx",
        run: async () => {
          try {
            const done = await sessionExportSesx();
            if (!done) return;
            const r = done.report;
            const left: string[] = [];
            if (r.chainsLeftOut) left.push(`built-in effects in ${r.chainsLeftOut} Voice ${r.chainsLeftOut === 1 ? "Chain" : "Chains"} (plug-ins are kept)`);
            if (r.envelopesLeftOut) left.push(`envelopes on ${r.envelopesLeftOut} ${r.envelopesLeftOut === 1 ? "clip" : "clips"}`);
            if (r.clipsSimplified) left.push(`${r.clipsSimplified} looped ${r.clipsSimplified === 1 ? "clip" : "clips"} (written as plain clips)`);
            if (r.routingLeftOut) left.push(`bus routing on ${r.routingLeftOut} ${r.routingLeftOut === 1 ? "track or bus" : "tracks and buses"} (every track goes straight to the mix)`);
            if (r.markersLeftOut) left.push(`${r.markersLeftOut} ${r.markersLeftOut === 1 ? "marker" : "markers"}`);
            const name = done.path.split(/[\\/]/).pop();
            showNotice(`Exported ${name}: ${r.tracks} tracks, ${r.clips} clips.${left.length ? ` Left out: ${left.join(", ")}.` : ""} Keep saving your work as an Audiform session (.audfrm).`);
          } catch (e) {
            showError(e);
          }
        },
        enabled: () => (session.current?.tracks.length ?? 0) > 0,
        reason: () => "The session has no tracks.",
      }),
      actions.register({
        id: "file.export.markers",
        run: () => {
          const saved = prefs.string(KEY_CHAPTER_FORMAT, "timestamps");
          exp.markersExport = { format: CHAPTER_FORMATS.some((f) => f.id === saved) ? (saved as ChapterFormat) : "timestamps" };
        },
        enabled: () => (markerDoc()?.markers.length ?? 0) > 0,
        reason: () => "Add markers first (Edit > Markers).",
      }),
      actions.register({
        id: "file.export.stems",
        run: () => exportStems(),
        enabled: () => exporting() === null && (session.current?.tracks.some((t) => t.clips.length > 0 && !t.muted) ?? false),
        reason: () => "The session has no unmuted tracks with clips.",
      }),
      actions.register({
        id: "file.collect-media",
        run: async () => {
          const file = session.path;
          if (!file) return;
          const n = await collectMedia(file, true);
          showNotice(n ? `Copied ${plural(n, "file")} into the session folder. The session now uses the copies.` : "Every file is already in the session folder.");
        },
        enabled: () => !!session.path && mediaPool.size + (session.current?.media.length ?? 0) > 0,
        reason: () => (session.path ? "No audio in the session." : "Save the session first: its audio is copied into the folder beside it."),
      }),
      actions.register({
        id: "file.close-unused",
        run: async () => {
          const used = usedMedia();
          const unused = mediaPool.current.items.filter((it) => !used.has(it.id)).map((it) => it.id);
          await closeWaveIfRemoved(unused, () => mediaPool.remove(unused, sessionHistory));
          showNotice(unused.length ? `Removed ${unused.length} unused ${unused.length === 1 ? "file" : "files"}.` : "Every file is used by a clip.");
        },
        enabled: () => mediaPool.size > 0,
        reason: () => "No files in the session.",
      }),
      actions.register({ id: "file.new-session", run: () => askNewSession() }),
      actions.register({ id: "file.save-template", run: () => saveAsTemplate() }),
      actions.register({ id: "file.open", run: () => openAny() }),
      actions.register({ id: "file.save", run: () => pluginSync.captureAll().then(() => files.save()).then(() => undefined) }),
      actions.register({ id: "file.save-as", run: () => pluginSync.captureAll().then(() => files.saveAs()).then(() => undefined) }),
      actions.register({ id: "file.save-all", run: () => files.save().then(() => undefined) }),
      actions.register({ id: "file.close", run: () => files.close().then(() => undefined) }),
      actions.register({ id: "file.import-sesx", run: () => files.importSesx().then(() => undefined) }),
      actions.register({ id: "file.close-all", run: () => files.closeAll().then(() => undefined) }),
    ];
  }

  return { register };
}
