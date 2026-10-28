// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Getting audio out: File > Export (mixdowns of the session, a
 * selection or clips, with a loudness target and chapters), Mix Down to
 * New File, Bounce to New Track, stems, the Playlist panel's exports,
 * and markers and chapters as text.
 */

import { findClips } from "../panels/editor/clip-edit";
import { CHAPTER_FORMATS, type ChapterFormat, embedList } from "../panels/markers/chapters";
import { exportNames } from "../panels/playlist/playlist";
import { editor } from "../stores/editor";
import { sessionHistory } from "../stores/history";
import { mediaPool } from "../stores/media";
import { session, transport } from "../stores/open-session";
import { prefs } from "../stores/prefs";
import { type MarkerDoc, clipLength, sessionEnd, tempoOf } from "../stores/session";
import { beatsToSeconds } from "../stores/tempo";
import { openWaveFile, wave, waveFile } from "../stores/wave";
import type { ActionRegistry } from "./actions";
import { exportBounce, exportEmbedChapters, exportMixdown, exportPickFolder, exportPickPath, exportText, exportVerify, pathsExist, publishWriteFiles } from "./core";
import type { DockController } from "./docking/dock";
import { DEFAULT_TARGET, targetsFor } from "./loudness";
import { DEFAULT_FORMAT, type OutputFormat, describeFormat, extensionOf, sanitizeFormat } from "./output-format";
import { listen } from "@tauri-apps/api/event";
import type { ExportScopeInfo, ExportSettings } from "./ExportDialog.svelte";
import type { PublishSettings } from "./PublishDialog.svelte";
import { EMPTY_EPISODE, type EpisodeInfo, packageFiles, slugOf } from "./publish";
import type { Exporting } from "./wave-tools.svelte";

export type MixSource = "all" | "selection" | "clips";
export type MixTarget = "file" | "files-panel";

/** What exporting needs from the rest of the app. */
export interface ExportToolsDeps {
  actions: ActionRegistry;
  dock: DockController;
  showError: (e: unknown) => void;
  showNotice: (text: string) => void;
  ask: (title: string, message: string, buttons: { label: string; value: string; primary?: boolean }[]) => Promise<string>;
  /** The Waveform editor is the one in front. */
  inWave: () => boolean;
  timeSel: () => { start: number; end: number } | null;
  selectedTrackDoc: () => import("../stores/session").TrackDoc | null;
  /** The progress dialog's state. */
  exporting: () => Exporting | null;
  setExporting: (v: Exporting | null) => void;
  /** A mixdown with every plug-in's latest settings saved in the session first. */
  exportMix: (path: string, req: Parameters<typeof exportMixdown>[1]) => ReturnType<typeof exportMixdown>;
  /** Plug-in sessions mix down at playback speed (Settings > Effects). */
  realtimeMix: () => boolean;
  /** Brings every running plug-in's settings into the session. */
  captureAll: () => Promise<void>;
  folderBesideSession: (name: string, why: string) => Promise<string | null>;
  placeFiles: (paths: string[], trackId: string | null, at: number, place?: { index?: number; name?: string }) => Promise<void>;
}

export function createExportTools(deps: ExportToolsDeps) {
  const { actions, dock, showError, showNotice, ask, inWave, timeSel, selectedTrackDoc, exporting, setExporting, exportMix, realtimeMix, captureAll, folderBesideSession, placeFiles } = deps;
  // Mixdown. One export at a time, with a progress dialog.


  function mixRange(source: MixSource): { range?: [number, number]; clips?: string[]; start: number } | null {
    const doc = session.current;
    if (!doc) return null;
    if (source === "selection") {
      const r = timeSel();
      return r ? { range: [r.start, r.end], start: r.start } : null;
    }
    if (source === "clips") {
      const chosen = editor.current().selectedClips;
      const found = findClips(doc, chosen);
      const midi = doc.tracks.flatMap((t) => (t.midi_clips ?? []).filter((c) => chosen.has(c.id)));
      if (found.length === 0 && midi.length === 0) return null;
      // MIDI clips sit in beats; their span in session samples follows the tempo map.
      const tempo = tempoOf(doc);
      const at = (beats: number) => Math.round(beatsToSeconds(tempo, beats) * doc.sample_rate);
      const spans = [
        ...found.map((f) => [f.clip.start, f.clip.start + clipLength(f.clip, doc.media.find((m) => m.id === f.clip.media_id), doc.sample_rate)]),
        ...midi.map((c) => [at(c.start), at(c.start + c.length)]),
      ];
      const start = Math.min(...spans.map((x) => x[0]!));
      const end = Math.max(...spans.map((x) => x[1]!));
      return { range: [start, end], clips: [...found.map((f) => f.clip.id), ...midi.map((c) => c.id)], start };
    }
    return { start: 0 };
  }

  const KEY_EXPORT = "export.mixdown";
  let exportReq = $state<{ initial: ExportSettings; scopes: ExportScopeInfo[]; source: { sampleRate: number; channels: number; bitDepth: number } } | null>(null);

  function sessionFolder(): string | null {
    const sp = session.home;
    return sp ? sp.slice(0, Math.max(sp.lastIndexOf("/"), sp.lastIndexOf("\\"))) || null : null;
  }

  /** Playlist panel: each range as its own file, numbered in playlist order, in the last export format. */
  async function exportPlaylist(items: MarkerDoc[]) {
    const doc = session.current;
    if (!doc || items.length === 0 || exporting()) return;
    const saved = prefs.raw(KEY_EXPORT) as { format?: unknown; folder?: unknown } | null;
    const format = sanitizeFormat(saved?.format);
    const start = (typeof saved?.folder === "string" && saved.folder) || sessionFolder();
    const folder = await exportPickFolder(start || null, "Export Playlist To");
    if (!folder) return;
    const sep = folder.includes("\\") ? "\\" : "/";
    const names = exportNames(items);
    const paths = names.map((n) => `${folder.replace(/[\\/]+$/, "")}${sep}${n}.${extensionOf(format.kind)}`);
    const exist = await pathsExist(paths);
    if (exist.some(Boolean)) {
      const ok = await ask("Replace files?", `${exist.filter(Boolean).length} of these files already exist in that folder.`, [
        { label: "Replace", value: "yes", primary: true },
        { label: "Cancel", value: "no" },
      ]);
      if (ok !== "yes") return;
    }
    setExporting({ title: "Exporting playlist", progress: 0, floating: true });
    try {
      for (let i = 0; i < items.length; i++) {
        const m = items[i]!;
        setExporting({ title: `Exporting ${i + 1} of ${items.length}: ${m.name}`, progress: i / items.length, floating: true });
        const r = await exportMix(paths[i]!, { range: [m.position, m.position + m.duration], format });
        if (r.cancelled) {
          showNotice(`Export stopped after ${i} of ${items.length} files.`);
          return;
        }
      }
      showNotice(`Exported ${items.length} ${items.length === 1 ? "file" : "files"} as ${describeFormat(format, { sampleRate: doc.sample_rate, channels: 2, bitDepth: doc.bit_depth })}.`);
    } finally {
      setExporting(null);
    }
  }

  // File > Export > Markers and Chapters.
  const KEY_CHAPTER_FORMAT = "export.chapters-format";
  let markersExport = $state<{ format: ChapterFormat } | null>(null);

  /** The document whose markers File > Export > Markers and Chapters writes: the open file in the Waveform editor, else the session. */
  const markerDoc = () => (inWave() ? wave.current : session.current);

  async function saveMarkers(content: string, format: ChapterFormat) {
    const doc = markerDoc();
    const f = CHAPTER_FORMATS.find((x) => x.id === format);
    if (!doc || !f) return;
    const wf = inWave() ? waveFile.current() : null;
    const name = wf ? (wf.path.split(/[\\/]/).pop() ?? "").replace(/\.[^.]+$/, "") : doc.name;
    const folder = wf ? wf.path.slice(0, Math.max(wf.path.lastIndexOf("/"), wf.path.lastIndexOf("\\"))) || null : sessionFolder();
    const base = (name || "Session").replace(/[\\/:*?"<>|]+/g, " ").trim() || "Session";
    const path = await exportText(`${base} ${format === "podcast-json" ? "chapters" : "markers"}`, folder, "Export Markers and Chapters", f.filter, f.ext, content);
    if (!path) return;
    markersExport = null;
    showNotice(`Saved ${path.split(/[\\/]/).pop()}.`);
  }

  /** File > Export: opens the Export Mixdown dialog, remembering the last format and folder. */
  function openExportDialog(source: MixSource) {
    const doc = session.current;
    if (!doc) return;
    const saved = prefs.raw(KEY_EXPORT) as { format?: unknown; folder?: unknown; addToFiles?: unknown } | null;
    const secs = (src: MixSource): number | null => {
      const r = mixRange(src);
      if (!r) return null;
      const len = r.range ? r.range[1] - r.range[0] : sessionEnd(doc);
      return len > 0 ? len / doc.sample_rate : null;
    };
    const scopes: ExportScopeInfo[] = [
      { scope: "all", label: "Whole session", seconds: secs("all"), reason: "The session has no clips yet." },
      { scope: "selection", label: "Time selection", seconds: secs("selection"), reason: "Drag across empty space or the ruler to select a time range." },
      { scope: "clips", label: "Selected clips", seconds: secs("clips"), reason: "Select clips first." },
    ];
    const folder = (typeof saved?.folder === "string" && saved.folder) || sessionFolder() || "";
    const suffix = source === "all" ? "" : source === "selection" ? " Selection" : " Clips";
    exportReq = {
      initial: {
        scope: source,
        name: `${fileSafe(doc.name)}${suffix}`,
        folder,
        format: sanitizeFormat(saved?.format),
        addToFiles: saved?.addToFiles === true,
        chapters: (saved as { chapters?: unknown } | null)?.chapters !== false,
        verify: (saved as { verify?: unknown } | null)?.verify !== false,
        normalize: (() => {
          const n = (saved as { normalize?: unknown } | null)?.normalize;
          return typeof n === "string" && targetsFor().some((t) => t.id === n) ? n : "";
        })(),
        ...(doc.space ? { speakers: doc.space.layout } : {}),
      },
      scopes,
      source: { sampleRate: doc.sample_rate, channels: doc.master?.channels === "mono" ? 1 : 2, bitDepth: doc.bit_depth },
    };
  }

  async function finishExportDialog(v: ExportSettings | null) {
    exportReq = null;
    if (!v) return;
    prefs.set(KEY_EXPORT, { format: v.format, folder: v.folder, addToFiles: v.addToFiles, chapters: v.chapters, verify: v.verify, normalize: v.normalize });
    const sep = v.folder.includes("\\") ? "\\" : "/";
    const path = `${v.folder.replace(/[\\/]+$/, "")}${sep}${fileSafe(v.name)}.${extensionOf(v.format.kind)}`;
    const [exists] = await pathsExist([path]);
    if (exists) {
      const ok = await ask("Replace the file?", `${path.split(/[\\/]/).pop()} already exists in that folder.`, [
        { label: "Replace", value: "yes", primary: true },
        { label: "Cancel", value: "no" },
      ]);
      if (ok !== "yes") return;
    }
    await runMixdown(v.scope, v.addToFiles ? "files-panel" : "file", path, v.format, v.chapters, v.speakers, v.normalize, v.verify);
  }

  // File > Publish Package.
  const KEY_PUBLISH = "publish.package";
  let publishReq = $state<{ initial: PublishSettings; source: { sampleRate: number; channels: number; bitDepth: number } } | null>(null);

  function openPublishDialog() {
    const doc = session.current;
    if (!doc) return;
    const saved = prefs.raw(KEY_PUBLISH) as { format?: unknown; folder?: unknown; normalize?: unknown } | null;
    const format = sanitizeFormat(saved?.format);
    const norm = saved?.normalize;
    publishReq = {
      initial: {
        episode: { ...EMPTY_EPISODE, title: doc.name },
        folder: (typeof saved?.folder === "string" && saved.folder) || sessionFolder() || "",
        // Hosts want MP3; a saved choice of another kind is not carried over.
        format: format.kind === "mp3" ? format : { ...DEFAULT_FORMAT, kind: "mp3", mp3Bitrate: 128 },
        normalize: typeof norm === "string" ? norm : DEFAULT_TARGET,
      },
      source: { sampleRate: doc.sample_rate, channels: doc.master?.channels === "mono" ? 1 : 2, bitDepth: doc.bit_depth },
    };
  }

  async function finishPublishDialog(v: PublishSettings | null) {
    publishReq = null;
    if (!v) return;
    prefs.set(KEY_PUBLISH, { format: v.format, folder: v.folder, normalize: v.normalize });
    const sep = v.folder.includes("\\") ? "\\" : "/";
    const slug = slugOf(v.episode.title);
    const folder = `${v.folder.replace(/[\\/]+$/, "")}${sep}${slug}`;
    const audioName = `${slug}.${extensionOf(v.format.kind)}`;
    const [exists] = await pathsExist([`${folder}${sep}${audioName}`]);
    if (exists) {
      const ok = await ask("Replace the episode?", `${slug} already has ${audioName}. The package will replace it and its files.`, [
        { label: "Replace", value: "yes", primary: true },
        { label: "Cancel", value: "no" },
      ]);
      if (ok !== "yes") return;
    }
    try {
      await publishWriteFiles(folder, []);
    } catch (e) {
      showError(e);
      return;
    }
    await runMixdown("all", "file", `${folder}${sep}${audioName}`, v.format, true, undefined, v.normalize, true, { episode: v.episode, folder, audioName });
  }

  async function mixDown(source: MixSource, target: MixTarget) {
    const doc = session.current;
    if (!doc || !mixRange(source) || exporting()) return;
    if (target === "file") {
      openExportDialog(source);
      return;
    }
    if (target === "files-panel") {
      await mixToNewFile(source);
      return;
    }
    const suffix = source === "all" ? "Mixdown" : source === "selection" ? "Selection" : "Clips";
    const path = await exportPickPath(`${doc.name} ${suffix}`, sessionFolder());
    if (!path) return;
    await runMixdown(source, target, path, null);
  }

  /**
   * Mix Down to New File: the mix is written as a 32-bit float WAV in a
   * Mixdowns folder beside the session, listed in the Files panel, and
   * opened in the Waveform editor like any other file.
   */
  async function mixToNewFile(source: MixSource) {
    const doc = session.current;
    const what = mixRange(source);
    if (!doc || !what || exporting()) return;
    const folder = await folderBesideSession("Mixdowns", "Mixdowns are saved beside the session.");
    if (!folder) return;
    const suffix = source === "all" ? "Mixdown" : source === "selection" ? "Selection" : "Clips";
    setExporting({ title: "Mixing down to a new file", progress: 0, floating: true });
    let off: (() => void) | null = null;
    try {
      off = await listen<{ done: number; total: number }>("export-progress", (e) => {
        if (exporting()) setExporting({ ...exporting()!, progress: e.payload.total > 0 ? e.payload.done / e.payload.total : 0 });
      });
      await transport.stop();
      await captureAll().catch(showError);
      const r = await exportBounce(folder, fileSafe(`${doc.name} ${suffix}`), {
        ...(realtimeMix() ? { realtime: true } : {}),
        ...(what.range ? { range: what.range } : {}),
        ...(what.clips ? { clips: what.clips } : {}),
      });
      if (r.cancelled) {
        showNotice("Mixdown cancelled.");
        return;
      }
      await mediaPool.importPaths([r.path], sessionHistory);
      await openWaveFile(r.path);
      editor.setMode("waveform");
      dock.show("editor");
      actions.invalidate();
      const name = r.path.split(/[\\/]/).pop();
      const clipped = r.peak > 1 ? ` It goes above 0 dBFS (peak +${(20 * Math.log10(r.peak)).toFixed(1)} dB); it is kept as float, so nothing is lost, but turn it down before saving to a fixed format.` : "";
      showNotice(`Mixed down to ${name}, in the Files panel and open in the Waveform editor.${clipped}`);
    } catch (e) {
      showError(e);
    } finally {
      off?.();
      setExporting(null);
    }
  }

  async function runMixdown(source: MixSource, target: MixTarget, path: string, format: OutputFormat | null, chapters = false, speakers?: string, normalize = "", verify = false, pack?: { episode: EpisodeInfo; folder: string; audioName: string }) {
    // A 3D Space speaker bed is written as mixed; every other file can be brought to a loudness.
    const bed = !!speakers && speakers !== "binaural" && speakers !== "2.0";
    const goal = normalize && !bed ? targetsFor().find((t) => t.id === normalize) : undefined;
    const what = mixRange(source);
    if (!what || exporting()) return;
    setExporting({ title: "Exporting mixdown", progress: 0, floating: true });
    let off: (() => void) | null = null;
    try {
      off = await listen<{ done: number; total: number }>("export-progress", (e) => {
        if (exporting()) setExporting({ ...exporting()!, progress: e.payload.total > 0 ? e.payload.done / e.payload.total : 0 });
      });
      const r = await exportMix(path, {
        ...(what.range ? { range: what.range } : {}),
        ...(what.clips ? { clips: what.clips } : {}),
        ...(format ? { format } : {}),
        ...(speakers === "binaural" ? { binaural: true } : speakers ? { spaceLayout: speakers } : {}),
        ...(goal ? { normalize: { lufs: goal.lufs, peak: goal.peak } } : {}),
      });
      if (r.cancelled) {
        showNotice("Export cancelled. Nothing was saved.");
        return;
      }
      const name = path.split(/[\\/]/).pop();
      let chapterNote = "";
      let writtenChapters: { title: string; startMs: number; endMs: number | null; url?: string | null }[] = [];
      const levelNote = goal ? ` Brought to ${goal.lufs} LUFS with peaks under ${goal.peak} dBTP.` : "";
      const doc = session.current;
      if (chapters && doc && (format?.kind === "mp3" || format?.kind === "flac" || format?.kind === "wav" || format?.kind === "m4a")) {
        const list = embedList(doc.markers, doc.sample_rate, what.range ?? null);
        if (list.length) {
          try {
            await exportEmbedChapters(path, list, Math.round((r.frames / r.sampleRate) * 1000));
            writtenChapters = list;
            chapterNote = ` ${list.length} ${list.length === 1 ? "chapter" : "chapters"} added.`;
          } catch (e) {
            // The audio is saved either way; say why the chapters are not in it.
            chapterNote = ` The chapters could not be added: ${e instanceof Error ? e.message : String(e)}`;
          }
        }
      }
      let checkNote = "";
      if (verify) {
        setExporting({ title: "Checking the file", progress: 0 });
        try {
          const rep = await exportVerify(path, { frames: r.frames, sampleRate: r.sampleRate, chapters: writtenChapters });
          if (rep.ok) checkNote = " Checked: it opens, decodes end to end and is the right length.";
          else {
            const bad = rep.checks.filter((c) => !c.ok).map((c) => `${c.name}: ${c.detail}`).join(" ");
            showError(`${name} was saved, but the check found a problem. ${bad} Export it again; if it repeats, report it.`);
            checkNote = " The check found a problem.";
          }
        } catch (e) {
          checkNote = ` The check could not run: ${e instanceof Error ? e.message : String(e)}`;
        }
      }
      let packNote = "";
      if (pack && doc) {
        const files = packageFiles(pack.episode, pack.audioName, Math.round((r.frames / r.sampleRate) * 1000), doc.markers, doc.sample_rate, checkNote.trim() || "The file was not checked.");
        await publishWriteFiles(pack.folder, files);
        packNote = ` Package ready in ${pack.folder}.`;
      }
      const clipped = r.peak > 1 ? ` The mix clipped (peak +${(20 * Math.log10(r.peak)).toFixed(1)} dB); turn the Master down and export again.` : "";
      if (target === "files-panel") {
        const imported = await mediaPool.importPaths([path], sessionHistory);
        mediaPool.setSelection(imported.ids);
        showNotice(`Mixed down to ${name} and added it to the Files panel.${levelNote}${chapterNote}${checkNote}${clipped}`);
      } else {
        showNotice(`Exported ${name}.${levelNote}${chapterNote}${checkNote}${packNote}${clipped}`);
      }
    } catch (e) {
      showError(e);
    } finally {
      off?.();
      setExporting(null);
    }
  }

  type BounceSource = "selection" | "clips" | "track" | "clips-selection";

  /** What a Bounce to New Track renders: its range, clips and track, and a name for the file. */
  function bounceRange(source: BounceSource): { range: [number, number]; clips?: string[]; track?: string; name: string; mono?: boolean } | null {
    const doc = session.current;
    if (!doc) return null;
    const sel = timeSel();
    const extent = (list: { start: number; end: number }[]): [number, number] | null =>
      list.length ? [Math.min(...list.map((c) => c.start)), Math.max(...list.map((c) => c.end))] : null;
    // MIDI clips sit in beats; their span in session samples follows the tempo map.
    const tempo = tempoOf(doc);
    const midiSpan = (c: { start: number; length: number }) => ({
      start: Math.round(beatsToSeconds(tempo, c.start) * doc.sample_rate),
      end: Math.round(beatsToSeconds(tempo, c.start + c.length) * doc.sample_rate),
    });
    if (source === "selection") return sel ? { range: [sel.start, sel.end], name: `${doc.name} Bounce` } : null;
    if (source === "track") {
      const t = selectedTrackDoc();
      if (!t) return null;
      const spans = [
        ...t.clips.map((c) => ({ start: c.start, end: c.start + clipLength(c, doc.media.find((m) => m.id === c.media_id), doc.sample_rate) })),
        ...(t.midi_clips ?? []).map(midiSpan),
      ];
      const r = sel ? ([sel.start, sel.end] as [number, number]) : extent(spans);
      return r ? { range: r, track: t.id, name: `${t.name} Bounce`, ...(t.channels === "mono" || t.mono ? { mono: true } : {}) } : null;
    }
    const chosen = editor.current().selectedClips;
    const found = findClips(doc, chosen);
    const midi = doc.tracks.flatMap((t) => (t.midi_clips ?? []).filter((c) => chosen.has(c.id)));
    if (found.length === 0 && midi.length === 0) return null;
    const clips = [...found.map((f) => f.clip.id), ...midi.map((c) => c.id)];
    const names = [...found.map((f) => f.clip.name), ...midi.map((c) => c.name)];
    const name = names.length === 1 ? `${names[0]} Bounce` : `${doc.name} Clips Bounce`;
    if (source === "clips-selection") return sel ? { range: [sel.start, sel.end], clips, name } : null;
    const r = extent([
      ...found.map((f) => ({ start: f.clip.start, end: f.clip.start + clipLength(f.clip, doc.media.find((m) => m.id === f.clip.media_id), doc.sample_rate) })),
      ...midi.map(midiSpan),
    ]);
    return r ? { range: r, clips, name } : null;
  }

  /**
   * Bounce to New Track: renders the part asked for as it plays to a new
   * file in the Bounced folder beside the session and places it on a new
   * track at the same time. The original tracks stay as they are.
   */
  async function bounce(source: BounceSource) {
    const what = bounceRange(source);
    if (!what || exporting()) return;
    const folder = await folderBesideSession("Bounced", "Bounced audio is saved beside the session.");
    if (!folder) return;
    setExporting({ title: "Bouncing to a new track", progress: 0, floating: true });
    let off: (() => void) | null = null;
    try {
      off = await listen<{ done: number; total: number }>("export-progress", (e) => {
        if (exporting()) setExporting({ ...exporting()!, progress: e.payload.total > 0 ? e.payload.done / e.payload.total : 0 });
      });
      await captureAll().catch(showError);
      const r = await exportBounce(folder, fileSafe(what.name), {
        range: what.range,
        ...(what.clips ? { clips: what.clips } : {}),
        ...(what.track ? { track: what.track } : {}),
        ...(what.mono ? { channels: 1 } : {}),
        ...(realtimeMix() ? { realtime: true } : {}),
      });
      if (r.cancelled) {
        showNotice("Bounce cancelled.");
        return;
      }
      // A track's bounce goes on a new track right below it.
      const from = what.track ? (session.current?.tracks.findIndex((t) => t.id === what.track) ?? -1) : -1;
      await placeFiles([r.path], null, what.range[0], from >= 0 ? { index: from + 1, name: what.name } : {});
      const clipped = r.peak > 1 ? ` It goes above 0 dBFS (peak +${(20 * Math.log10(r.peak)).toFixed(1)} dB); it is kept as float, so nothing is lost.` : "";
      showNotice(`Bounced to ${r.path.split(/[\\/]/).pop()} on a new track.${clipped}`);
    } catch (e) {
      showError(e);
    } finally {
      off?.();
      setExporting(null);
    }
  }

  /** File-safe version of a track or session name. */
  const fileSafe = (name: string) => name.replace(/[<>:"/\\|?*\u0000-\u001f]+/g, " ").replace(/\s+/g, " ").trim() || "Track";

  /**
   * One WAV per track that has clips and is not muted, each the length of
   * the whole session so they line up when imported elsewhere.
   */
  async function exportStems() {
    const doc = session.current;
    if (!doc || exporting()) return;
    const tracks = doc.tracks.filter((t) => t.clips.length > 0 && !t.muted);
    if (tracks.length === 0) {
      showNotice("No unmuted tracks with clips to export.");
      return;
    }
    const sp = session.home;
    const start = sp ? sp.slice(0, Math.max(sp.lastIndexOf("/"), sp.lastIndexOf("\\"))) || null : null;
    const folder = await exportPickFolder(start);
    if (!folder) return;
    const sep = folder.includes("\\") ? "\\" : "/";
    let off: (() => void) | null = null;
    let done = 0;
    try {
      for (const [i, t] of tracks.entries()) {
        setExporting({ title: `Exporting stem ${i + 1} of ${tracks.length}: ${t.name}`, progress: 0, floating: true });
        off?.();
        off = await listen<{ done: number; total: number }>("export-progress", (e) => {
          if (exporting()) setExporting({ ...exporting()!, progress: e.payload.total > 0 ? e.payload.done / e.payload.total : 0 });
        });
        const path = `${folder}${sep}${fileSafe(doc.name)} - ${String(i + 1).padStart(2, "0")} ${fileSafe(t.name)}.wav`;
        const r = await exportMix(path, { track: t.id });
        if (r.cancelled) {
          showNotice(`Stem export cancelled after ${done} of ${tracks.length}.`);
          return;
        }
        done++;
      }
      showNotice(`Exported ${done} ${done === 1 ? "stem" : "stems"} to ${folder}.`);
    } catch (e) {
      showError(e);
    } finally {
      off?.();
      setExporting(null);
    }
  }



  return {
    /** File > Export's dialog, while it is open. */
    get exportReq() {
      return exportReq;
    },
    set exportReq(v) {
      exportReq = v;
    },
    /** File > Publish Package's dialog, while it is open. */
    get publishReq() {
      return publishReq;
    },
    /** File > Export > Markers and Chapters' dialog, while it is open. */
    get markersExport() {
      return markersExport;
    },
    set markersExport(v) {
      markersExport = v;
    },
    KEY_CHAPTER_FORMAT,
    mixRange,
    exportPlaylist,
    markerDoc,
    saveMarkers,
    openExportDialog,
    finishExportDialog,
    openPublishDialog,
    finishPublishDialog,
    mixDown,
    bounceRange,
    bounce,
    exportStems,
  };
}
