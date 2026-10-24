<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Altered Audio Editor.

  App shell: menu bar, docking layout, status bar.
-->
<script lang="ts">
  import { onMount, type Component } from "svelte";
  import {
    workspaceDelete,
    exitApp,
    getAppInfo,
    loadKeymapOverrides,
    workspaceGet,
    workspaceList,
    minimizeWindow,
    openLink,
    pickMediaFiles,
    pickMediaFolder,
    revealMedia,
    saveKeymapOverrides,
    workspacePut,
    type AppInfo,
  } from "./core";
  import { LINKS } from "./links";
  import { sessionHistory } from "../stores/history";
  import { keymap } from "../stores/keymap";
  import { bindKeymap } from "../keymap/dispatch";
  import KeyboardShortcutsDialog from "./KeyboardShortcutsDialog.svelte";
  import { DockController, type PanelContent, type WorkspaceState } from "./docking/dock";
  import { PANELS, panelDef, type PanelId } from "./docking/registry";
  import DockShell from "./docking/DockShell.svelte";
  import { ActionRegistry } from "./actions";
  import { MENUS } from "./menus/model";
  import MenuBar from "./menus/MenuBar.svelte";
  import AboutDialog from "./AboutDialog.svelte";
  import HistoryPanel from "../panels/history/HistoryPanel.svelte";
  import EditorPanel from "../panels/editor/EditorPanel.svelte";
  import { peaks, session, transport } from "../stores/open-session";
  import PhaseWheelPanel from "../panels/phase/PhaseWheelPanel.svelte";
  import PropertiesPanel from "../panels/properties/PropertiesPanel.svelte";
  import LevelsPanel from "../panels/levels/LevelsPanel.svelte";
  import { PreviewMeter } from "../stores/preview-meter";
  import QuickPassPanel from "../panels/quickpass/QuickPassPanel.svelte";
  import BatchConvertPanel from "../panels/batch/BatchConvertPanel.svelte";
  import { trackNameFromFile } from "./track-names";
  import MixerPanel from "../panels/mixer/MixerPanel.svelte";
  import TimeInfoPanel from "../panels/time-info/TimeInfoPanel.svelte";
  import { editor, restoreSettings, settingsOf } from "../stores/editor";
  import { anchorStart, clipsFrom, clipsUpTo, nextClipId } from "./clip-select";
  import { hasClipIn, showOnly } from "../panels/editor/visibility";
  import { clipLength, sessionEnd, setErrorSink, type ClipDoc, type Op, type TrackDoc } from "../stores/session";
  import { fit, maxSppFor, zoomAround } from "../panels/editor/timeline";
  import ChoiceDialog, { type ChoiceButton } from "./ChoiceDialog.svelte";
  import ExportDialog, { type ExportScopeInfo, type ExportSettings } from "./ExportDialog.svelte";
  import { describeFormat, extensionOf, kindOfPath, sanitizeFormat, type OutputFormat } from "./output-format";
  import SessionPropertiesDialog from "./SessionPropertiesDialog.svelte";
  import PromptDialog from "./PromptDialog.svelte";
  import ExportProgress from "./ExportProgress.svelte";
  import LoudnessDialog from "./LoudnessDialog.svelte";
  import LoudnessMatchDialog from "./LoudnessMatchDialog.svelte";
  import ExportMarkersDialog from "./ExportMarkersDialog.svelte";
  import StretchDialog from "./StretchDialog.svelte";
  import MusicFitDialog from "./MusicFitDialog.svelte";
  import { describeJoins, describePlan, FIT_XFADE, fitOps, type FitPlan } from "../panels/editor/musicfit";
  import { ALIGN_MAX_MS, alignOps, alignPair, describeAlign } from "../panels/editor/align";
  import SettingsDialog from "./settings/SettingsDialog.svelte";
  import { balance, balanceItems, describeBalance } from "./speaker-balance";
  import { HOST_PROFILES, profileById, profileOp } from "./host-profiles";
  import type { SettingsDraft } from "./settings/pages";
  import { colorVars, meterPercent, sanitizeSettings, settings, type AppSettings } from "../stores/settings";
  import { setMeterSpeed, type TransportStore } from "../stores/transport";
  import GenerateDialog, { type GenerateMode, type GenerateValues } from "./GenerateDialog.svelte";
  import { naturalLength, stretchTo, unstretch } from "../panels/editor/stretch";
  import { CHAPTER_FORMATS, embedList, type ChapterFormat } from "../panels/markers/chapters";
  import { matchChanges, matchItems, matchRow, type MatchItem, type MatchRow } from "./loudness-match";
  import { DEFAULT_REGION, DEFAULT_TARGET, setCustomTarget, targetById, type LoudnessReport } from "./loudness";
  import ClipColorDialog from "./ClipColorDialog.svelte";
  import MarkersPanel from "../panels/markers/MarkersPanel.svelte";
  import PlaylistPanel from "../panels/playlist/PlaylistPanel.svelte";
  import { exportNames } from "../panels/playlist/playlist";
  import { newMarker, nextMarkerName } from "../panels/markers/markers";
  import { listen } from "@tauri-apps/api/event";
  import {
    clipEnd,
    clipsAt,
    insertTime,
    isolate,
    clearRange,
    copyClips,
    fadeClip,
    findClips,
    loopChanges,
    gapAt,
    mediaOf,
    LockedError,
    pasteClips,
    rippleDeleteClips,
    rippleDeleteRange,
    splitClips,
    trimToPlayhead,
    trimToRange,
    type ClipBoard,
  } from "../panels/editor/clip-edit";
  import { AUDIO_TYPES, type ClipChange, type MarkerDoc } from "../stores/session";
  import { propertiesOp, type SessionProperties } from "./session-properties";
  import { placeTake, punchStopAfter, recordPlan, sanitizeRecordOptions, type RecordOptions, type RecordPlan } from "./recording";
  import RecordOptionsDialog from "./RecordOptionsDialog.svelte";
  import AddTracksDialog, { type AddTracksValues } from "./AddTracksDialog.svelte";
  import PluginManagerDialog from "./PluginManagerDialog.svelte";
  import { PluginSync } from "./plugin-sync";
  import { markStopped, pluginsVersion } from "../stores/plugins";
  import { newTrackNames } from "./track-names";
  import { allMidiClipIds, deleteClipsOps } from "../panels/editor/midi";
  import { CompoundCommand } from "../commands/compound";
  import { SessionFiles, displayName, planOpen, type Choice } from "./session-files";
  import { RECENT_SLOTS, WORKSPACE_SLOTS } from "./menus/model";
  import {
    sessionAutosave,
    sessionClearAutosave,
    sessionDiscardRecovery,
    sessionNew,
    sessionOpen,
    sessionPickOpen,
    filePickOpen,
    exportPickPath,
    exportText,
    exportEmbedChapters,
    generateAudio,
    exportMixdown,
    pluginEvents,
    pluginState,
    exportMerge,
    exportRegionCopy,
    templateList,
    templateSave,
    templateDelete,
    sessionDefaultFolder, sessionNextName,
    type TemplateEntry,
    wavePeak,
    exportCancel,
    exportPickFolder,
    pathsExist,
    pickTrackFiles,
    unzipMedia,
    loudnessCheck,
    sessionPickSave,
    sessionImportSesx,
    sessionRecover,
    sessionRecoverable,
    sessionSave,
    setWindowTitle,
    waveSpectralEdit,
    waveApplyChain,
    phaseAlign,
    setAudioOutput,
    setAudioDriver,
    recordStart,
    recordStatus,
    recordStop,
    recordMonitor,
    effectsCatalog,
    musicFit,
    quickpassRepairCopy,
    type SpotsRequest,
    type EffectInfo,
    type RecordTarget,
    type Recoverable,
    previewStatus,
    probeMedia,
    sessionExportSesx,
    waveStretch,
    waveReverseInvert,
  } from "./core";
  import { isTauri } from "@tauri-apps/api/core";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import type { EffectChainDoc, EffectTarget, MediaRefDoc, SessionDoc } from "../stores/session";
  import PendingPanel from "../panels/PendingPanel.svelte";
  import VoiceChainPanel from "../panels/voice-chain/VoiceChainPanel.svelte";
  import { addSlot, chainOf, newSlot } from "../panels/voice-chain/chain";
  import { effectWindowRequest } from "../panels/voice-chain/effect-window";
  import { setFxCatalog } from "../panels/editor/envelope";
  import type { Finding } from "../panels/quickpass/quickpass";
  import FilesPanel from "../panels/files/FilesPanel.svelte";
  import DriveManagerPanel from "../panels/drive-manager/DriveManagerPanel.svelte";
  import { prefs } from "../stores/prefs";
  import { libraries } from "../stores/library";
  import LibrariesPanel from "../panels/libraries/LibrariesPanel.svelte";
  import { mediaPool } from "../stores/media";
  import { FileDropRouter, bindNativeFileDrop } from "./filedrop";
  import { activeArea, currentArea, trackArea, type Area } from "./area";
  import { revealLabel } from "../lib/platform";
  import ToolStrip from "./ToolStrip.svelte";
  import NewAudioFileDialog, { type NewAudioSettings } from "./NewAudioFileDialog.svelte";
  import NewSessionDialog, { type NewSessionSettings } from "./NewSessionDialog.svelte";
  import { closeWaveFile, forgetParked, newEmptyWaveFile, parkedUnsaved, newWaveFromClips, openWaveFile, saveWaveFile, savesInPlace, spectralSelection, wave, waveFile, waveHistory, waveTransport, waveZoom } from "../stores/wave";
  import { get, writable } from "svelte/store";
  import type { SessionRow } from "../panels/files/session-row";
  import { rasterize } from "../panels/editor/spectrum";
  import { inputLevels, liveTakes, LiveWave, recordControl, recording } from "../stores/record";
  import { stretchRange, amplifyRange, clampRange, copyRange, cropTo, deleteRange, fadeRange, insertFile, mixPasteAt, pasteAt, replaceRange, silenceRange, type WaveBoard } from "../panels/editor/wave-edit";

  let info = $state<AppInfo | null>(null);
  let coreError = $state<string | null>(null);
  let lastError = $state<string | null>(null);

  let notice = $state<string | null>(null);
  let noticeTimer: ReturnType<typeof setTimeout> | undefined;

  let errorTimer: ReturnType<typeof setTimeout> | undefined;

  function showError(e: unknown) {
    lastError = e instanceof Error ? e.message : String(e);
    console.error(e);
    clearTimeout(errorTimer);
    errorTimer = setTimeout(() => (lastError = null), 12000);
  }

  // Errors nothing else catches (in an update, an effect or a promise)
  // show in the status bar, so they can be seen and reported.
  setErrorSink((e) => showError(e));
  onMount(() => {
    const onErr = (ev: ErrorEvent) => showError(ev.error ?? ev.message);
    const onRej = (ev: PromiseRejectionEvent) => showError(ev.reason);
    window.addEventListener("error", onErr);
    window.addEventListener("unhandledrejection", onRej);
    return () => {
      window.removeEventListener("error", onErr);
      window.removeEventListener("unhandledrejection", onRej);
    };
  });

  /** A short status bar message that clears itself. */
  function showNotice(text: string) {
    notice = text;
    clearTimeout(noticeTimer);
    noticeTimer = setTimeout(() => (notice = null), 6000);
  }

  /** Replaces any zips with the audio extracted from them (into a folder beside each zip). */
  async function withUnzipped(paths: string[]): Promise<string[]> {
    const out: string[] = [];
    for (const p of paths) {
      if (!/\.zip$/i.test(p)) {
        out.push(p);
        continue;
      }
      try {
        out.push(...(await unzipMedia(p)));
      } catch (e) {
        showError(e);
      }
    }
    return out;
  }

  async function importPaths(raw: string[]) {
    const paths = await withUnzipped(raw);
    if (paths.length === 0) return;
    dock.show("files");
    const result = await mediaPool.importPaths(paths, sessionHistory);
    if (result.added === 0) {
      showNotice(result.duplicates > 0 ? "Those files are already in the session." : "No audio files found there.");
    } else if (result.duplicates > 0) {
      showNotice(`Skipped ${result.duplicates} already in the session.`);
    }
  }

  const previewMeter = new PreviewMeter({ status: () => previewStatus() });
  const uuid = () => crypto.randomUUID();

  function nextTrackName(tracks: readonly TrackDoc[]): string {
    const names = new Set(tracks.map((t) => t.name));
    let n = tracks.length + 1;
    while (names.has(`Track ${n}`)) n++;
    return `Track ${n}`;
  }

  const stem = (name: string) => name.replace(/\.[^.]+$/, "");

  /** Adds files as clips, one after another, on a track (or a new one) starting at `at`. */
  /**
   * The ops that put `paths` on a track (null makes a new track) one
   * after another from `at`. Null when none of the files can be read.
   */
  async function placeOps(paths: string[], trackId: string | null, at: number): Promise<{ ops: Op[]; clips: { id: string }[]; target: string } | null> {
    if (!session.current || paths.length === 0) return null;
    const imported = await mediaPool.importPaths(paths, sessionHistory);
    await mediaPool.whenIdle();
    const doc = session.current;
    const items = imported.ids.map((id) => mediaPool.get(id)).filter((it) => it?.status === "ready" && it.info);
    if (items.length === 0) return null;
    const ops: Op[] = [];
    let target = trackId;
    // Audio never lands on a MIDI track: it gets a new track instead.
    if (!target || !doc.tracks.some((t) => t.id === target && t.kind !== "midi")) {
      target = uuid();
      ops.push({
        op: "add_track",
        id: target,
        name: nextTrackName(doc.tracks),
        stereo: (items[0]!.info!.channels ?? 2) > 1,
        index: doc.tracks.length,
      });
    }
    const known = new Set(doc.media.map((m) => m.id));
    const clips = [];
    let pos = Math.max(0, Math.round(at));
    for (const it of items) {
      const info = it!.info!;
      if (!known.has(it!.id)) {
        known.add(it!.id);
        ops.push({
          op: "add_media",
          media: {
            id: it!.id,
            path: it!.path,
            relative_path: null,
            sample_rate: info.sampleRate,
            channels: info.channels,
            length: info.length,
          },
        });
      }
      clips.push({ id: uuid(), name: stem(it!.name), media_id: it!.id, start: pos, source_in: 0, source_out: info.length });
      pos += Math.round((info.length * doc.sample_rate) / info.sampleRate);
    }
    ops.push({ op: "add_clips", track_id: target, clips });
    return { ops, clips, target };
  }

  /**
   * Multitrack > Insert Files as Tracks: each file on its own new track,
   * all starting at `at`, named after the file. Made for per-speaker
   * recordings, which line up from the same start. One undo step.
   */
  async function placeAsTracks(raw: string[], at: number) {
    const doc0 = session.current;
    if (!doc0) return;
    const paths = await withUnzipped(raw);
    if (paths.length === 0) return;
    const imported = await mediaPool.importPaths(paths, sessionHistory);
    await mediaPool.whenIdle();
    const doc = session.current ?? doc0;
    const ids = [...new Set([...imported.ids, ...paths.map((p) => mediaPool.current.items.find((it) => it.path === p)?.id).filter((x): x is string => !!x)])];
    const items = ids.map((id) => mediaPool.get(id)).filter((it) => it?.status === "ready" && it.info);
    if (items.length === 0) {
      showNotice("None of those files could be read.");
      return;
    }
    const ops: Op[] = [];
    const known = new Set(doc.media.map((m) => m.id));
    const clipIds: string[] = [];
    const start = Math.max(0, Math.round(at));
    for (const [i, it] of items.entries()) {
      const info = it!.info!;
      const track = uuid();
      ops.push({ op: "add_track", id: track, name: trackNameFromFile(it!.name), stereo: info.channels > 1, index: doc.tracks.length + i });
      if (!known.has(it!.id)) {
        known.add(it!.id);
        ops.push({
          op: "add_media",
          media: { id: it!.id, path: it!.path, relative_path: null, sample_rate: info.sampleRate, channels: info.channels, length: info.length },
        });
      }
      const id = uuid();
      clipIds.push(id);
      ops.push({ op: "add_clips", track_id: track, clips: [{ id, name: stem(it!.name), media_id: it!.id, start, source_in: 0, source_out: info.length }] });
    }
    await session.edit(items.length === 1 ? "Insert File as Track" : `Insert ${items.length} Files as Tracks`, { op: "batch", ops }, sessionHistory);
    editor.selectClips(clipIds);
    editor.setMode("multitrack");
    dock.show("editor");
    // A fresh session shows the whole recording.
    if (doc0.tracks.every((t) => t.clips.length === 0)) await actions.run("view.zoom-session");
  }

  async function placeFiles(paths: string[], trackId: string | null, at: number) {
    if (!session.current || paths.length === 0) return;
    const placed = await placeOps(paths, trackId, at);
    if (!placed) {
      showNotice("None of those files could be read.");
      return;
    }
    const { ops, clips, target } = placed;
    await session.edit(clips.length === 1 ? "Add Clip" : `Add ${clips.length} Clips`, { op: "batch", ops }, sessionHistory);
    editor.selectClips(
      clips.map((c) => c.id),
      target,
    );
  }

  /** A folder named `name` beside the saved session, asking to save first if needed. Null if not saved. */
  async function folderBesideSession(name: string, why: string): Promise<string | null> {
    if (!session.path) {
      showNotice(`Save the session first. ${why}`);
      await actions.run("file.save-as");
      if (!session.path) return null;
    }
    const sp = session.path;
    const cut = Math.max(sp.lastIndexOf("/"), sp.lastIndexOf("\\"));
    const sep = sp.includes("\\") ? "\\" : "/";
    return `${sp.slice(0, cut)}${sep}${name}`;
  }

  /**
   * QuickPass Repair: each clip with chosen clicks or clipped spots gets a
   * copy of its audio with just those spots repaired, and plays that copy.
   * The original file is never changed; one undo puts every clip back.
   * Returns the ids of the findings that were fixed.
   */
  async function repairSpots(spots: SpotsRequest, list: Finding[]): Promise<string[]> {
    const doc0 = session.current;
    if (!doc0 || exporting) return [];
    const byClip = new Map<string, Finding[]>();
    for (const f of list) if (f.clipId) byClip.set(f.clipId, [...(byClip.get(f.clipId) ?? []), f]);
    const found = findClips(doc0, new Set(byClip.keys())).filter((f) => !f.clip.locked);
    const skipped = found.filter((f) => f.clip.looped || f.clip.stretch);
    const work = found.filter((f) => !f.clip.looped && !f.clip.stretch);
    if (skipped.length) showNotice(`Skipped ${skipped.length === 1 ? `"${skipped[0]!.clip.name}"` : `${skipped.length} clips`}: repair needs a clip that is not looped or stretched.`);
    if (work.length === 0) return [];
    const folder = await folderBesideSession("Repaired", "Repaired audio is kept in a folder next to it.");
    if (!folder) return [];
    exporting = { title: work.length === 1 ? "Repairing clip audio" : `Repairing audio for ${work.length} clips`, progress: 0 };
    try {
      const paths: string[] = [];
      for (const [i, f] of work.entries()) {
        const m = doc0.media.find((x) => x.id === f.clip.media_id);
        if (!m) throw new Error(`The file for "${f.clip.name}" is missing from this session.`);
        const ratio = (m.sample_rate || doc0.sample_rate) / doc0.sample_rate;
        const spans = byClip.get(f.clip.id)!.map((g): [number, number] => [
          Math.max(0, Math.round((g.start - f.clip.start) * ratio)),
          Math.max(0, Math.round((g.end - f.clip.start) * ratio)),
        ]);
        paths.push(await quickpassRepairCopy(m.path, f.clip.source_in, f.clip.source_out, folder, `${f.clip.name} Repaired`, spots, spans));
        exporting = { ...exporting!, progress: (i + 1) / work.length };
      }
      const imported = await mediaPool.importPaths(paths, sessionHistory);
      await mediaPool.whenIdle();
      const doc = session.current!;
      const known = new Set(doc.media.map((x) => x.id));
      const ops: Op[] = [];
      const restored: [string, ClipDoc][] = [];
      for (const [i, f] of work.entries()) {
        const it = mediaPool.get(imported.ids[i]!);
        if (!it || it.status !== "ready" || !it.info) throw new Error(`The repaired copy of "${f.clip.name}" could not be read back.`);
        if (!known.has(it.id)) {
          known.add(it.id);
          ops.push({
            op: "add_media",
            media: { id: it.id, path: it.path, relative_path: null, sample_rate: it.info.sampleRate, channels: it.info.channels, length: it.info.length },
          });
        }
        restored.push([f.track.id, { ...f.clip, media_id: it.id, source_in: 0, source_out: it.info.length }]);
      }
      ops.push({ op: "remove_clips", ids: work.map((f) => f.clip.id) }, { op: "restore_clips", clips: restored });
      const n = work.reduce((a, f) => a + byClip.get(f.clip.id)!.length, 0);
      await session.edit(spots.kind === "clicks" ? `Repair ${n === 1 ? "Click" : `${n} Clicks`}` : `Repair ${n === 1 ? "Clipped Spot" : `${n} Clipped Spots`}`, { op: "batch", ops }, sessionHistory);
      showNotice(`Repaired ${n} ${spots.kind === "clicks" ? (n === 1 ? "click" : "clicks") : n === 1 ? "clipped spot" : "clipped spots"} in ${work.length === 1 ? `"${work[0]!.clip.name}"` : `${work.length} clips`}.`);
      const ok = new Set(work.map((f) => f.clip.id));
      return list.filter((f) => f.clipId && ok.has(f.clipId)).map((f) => f.id);
    } catch (e) {
      showError(e);
      return [];
    } finally {
      exporting = null;
    }
  }

  /**
   * Clip > Make Unique Copy: each selected clip gets its own copy of the
   * audio it plays (just its part of the file, untouched), so later
   * changes to it never reach other clips sharing the original. Gain,
   * fades and envelopes stay as they are. One undo step.
   */
  async function makeUniqueCopy() {
    const doc0 = session.current;
    if (!doc0 || exporting) return;
    const found = findClips(doc0, editor.current().selectedClips).filter((f) => !f.clip.locked);
    if (found.length === 0) return;
    const folder = await folderBesideSession("Clip Copies", "Clip copies are kept in a folder next to it.");
    if (!folder) return;
    exporting = { title: found.length === 1 ? "Copying clip audio" : `Copying audio for ${found.length} clips`, progress: 0 };
    try {
      const paths: string[] = [];
      for (const [i, f] of found.entries()) {
        const m = doc0.media.find((x) => x.id === f.clip.media_id);
        if (!m) throw new Error(`The file for "${f.clip.name}" is missing from this session.`);
        paths.push(await exportRegionCopy(m.path, f.clip.source_in, f.clip.source_out, folder, `${f.clip.name} Copy`));
        exporting = { ...exporting!, progress: (i + 1) / found.length };
      }
      const imported = await mediaPool.importPaths(paths, sessionHistory);
      await mediaPool.whenIdle();
      const doc = session.current!;
      const known = new Set(doc.media.map((x) => x.id));
      const ops: Op[] = [];
      const restored: [string, ClipDoc][] = [];
      for (const [i, f] of found.entries()) {
        const it = mediaPool.get(imported.ids[i]!);
        if (!it || it.status !== "ready" || !it.info) throw new Error(`The copy of "${f.clip.name}" could not be read back.`);
        if (!known.has(it.id)) {
          known.add(it.id);
          ops.push({
            op: "add_media",
            media: { id: it.id, path: it.path, relative_path: null, sample_rate: it.info.sampleRate, channels: it.info.channels, length: it.info.length },
          });
        }
        restored.push([f.track.id, { ...f.clip, media_id: it.id, source_in: 0, source_out: it.info.length }]);
      }
      ops.push({ op: "remove_clips", ids: found.map((f) => f.clip.id) }, { op: "restore_clips", clips: restored });
      await session.edit(found.length === 1 ? "Make Unique Copy" : `Make ${found.length} Unique Copies`, { op: "batch", ops }, sessionHistory);
      editor.selectClips(found.map((f) => f.clip.id));
      showNotice(found.length === 1 ? `The clip now plays its own copy, ${paths[0]!.split(/[\\/]/).pop()}.` : `${found.length} clips now play their own copies.`);
    } catch (e) {
      showError(e);
    } finally {
      exporting = null;
    }
  }

  /** Two or more selected clips on one track, none locked: what Merge Clips works on. */
  function mergeTarget(): { trackId: string; clips: ClipDoc[] } | null {
    const doc = session.current;
    if (!doc) return null;
    const found = findClips(doc, editor.current().selectedClips);
    if (found.length < 2) return null;
    const trackId = found[0]!.track.id;
    if (found.some((f) => f.track.id !== trackId || f.clip.locked)) return null;
    return { trackId, clips: found.map((f) => f.clip).sort((a, b) => a.start - b.start) };
  }

  function mergeReason(): string {
    const doc = session.current;
    const found = doc ? findClips(doc, editor.current().selectedClips) : [];
    if (found.length < 2) return "Select two or more clips on one track.";
    if (found.some((f) => f.clip.locked)) return "One of the selected clips is locked.";
    return "The selected clips must be on the same track.";
  }

  /**
   * Clip > Merge Clips: renders the selected clips (with their gain,
   * fades and envelopes, not the track's settings) into one new file in
   * a "Merged Clips" folder beside the session, then swaps them for one
   * clip. One undo step; the original files are untouched.
   */
  async function mergeClips() {
    const m = mergeTarget();
    if (!m || exporting) return;
    const folder = await folderBesideSession("Merged Clips", "Merged clips are kept in a folder next to it.");
    if (!folder) return;
    const doc = session.current!;
    const start = m.clips[0]!.start;
    const end = Math.max(...m.clips.map((c) => c.start + clipLength(c, doc.media.find((x) => x.id === c.media_id), doc.sample_rate)));
    exporting = { title: "Merging clips", progress: 0 };
    let off: (() => void) | null = null;
    try {
      off = await listen<{ done: number; total: number }>("export-progress", (e) => {
        if (exporting) exporting = { ...exporting, progress: e.payload.total > 0 ? e.payload.done / e.payload.total : 0 };
      });
      const r = await exportMerge(folder, `${m.clips[0]!.name} Merged`, { range: [start, end], clips: m.clips.map((c) => c.id), track: m.trackId });
      if (r.cancelled) {
        showNotice("Merge cancelled. Nothing changed.");
        return;
      }
      const placed = await placeOps([r.path], m.trackId, start);
      if (!placed) {
        showNotice("The merged file could not be read back.");
        return;
      }
      const ops: Op[] = [{ op: "remove_clips", ids: m.clips.map((c) => c.id) }, ...placed.ops];
      await session.edit(`Merge ${m.clips.length} Clips`, { op: "batch", ops }, sessionHistory);
      editor.selectClips(
        placed.clips.map((c) => c.id),
        m.trackId,
      );
      const clipped = r.peak > 1 ? " The merged audio goes above 0 dBFS; it is saved as float, so nothing is lost, but turn the clip down before export." : "";
      showNotice(`Merged ${m.clips.length} clips into ${r.path.split(/[\\/]/).pop()}.${clipped}`);
    } catch (e) {
      showError(e);
    } finally {
      off?.();
      exporting = null;
    }
  }

  /**
   * File > Open. Sessions open as usual. Audio files go into the Files
   * panel; when the session has no clips yet, each one is also placed on
   * its own track at the start so it can be played and edited right away.
   */
  async function openAny(): Promise<void> {
    await openPaths(await filePickOpen());
  }

  /** Opens picked or dropped files: a session (ours or .sesx), plus any audio files. */
  async function openPaths(picked: string[]): Promise<void> {
    if (picked.length === 0) return;
    const plan = planOpen(picked);
    if (plan.session && !(await files.open(plan.session))) return;
    if (plan.sesx && !(await files.importSesx(plan.sesx))) return;
    if (plan.audio.length > 0) await openAudio(plan.audio);
    if (plan.skipped.length > 0) {
      const names = plan.skipped.map((p) => p.split(/[\\/]/).pop()).join(", ");
      showNotice(`Not opened: ${names}. Pick one session at a time, plus any audio files.`);
    }
  }

  async function openAudio(paths: string[]): Promise<void> {
    const doc = session.current;
    if (!doc) return;
    const empty = doc.tracks.every((t) => t.clips.length === 0);
    if (empty) {
      // Each file onto the next empty track (new sessions start with some), then new tracks.
      for (const [i, p] of paths.entries()) await placeFiles([p], doc.tracks[i]?.id ?? null, 0);
      await actions.run("view.zoom-session");
      return;
    }
    const imported = await mediaPool.importPaths(paths, sessionHistory);
    mediaPool.setSelection(imported.ids);
    showNotice(
      paths.length === 1
        ? "Added to the Files panel. Drag it onto a track, or use Insert Files at Playhead."
        : `Added ${paths.length} files to the Files panel. Drag them onto tracks, or use Insert Files at Playhead.`,
    );
  }

  let addTracksOpen = $state(false);
  async function addTracks(v: AddTracksValues) {
    addTracksOpen = false;
    const doc = session.current;
    if (!doc) return;
    if (v.bus) {
      const buses = doc.buses ?? [];
      const names = newTrackNames([...doc.tracks, ...buses].map((t) => t.name), v.count, v.base);
      const ids = names.map(() => uuid());
      const ops: Op[] = names.map((name, k) => ({ op: "add_bus", id: ids[k]!, name, stereo: true, index: buses.length + k }));
      await session.edit(v.count === 1 ? "Add Bus Track" : `Add ${v.count} Bus Tracks`, { op: "batch", ops }, sessionHistory);
      editor.selectTrack(ids[0]!);
      return;
    }
    const sel = editor.current().selectedTrack;
    const i = doc.tracks.findIndex((t) => t.id === sel);
    const start = i >= 0 ? i + 1 : doc.tracks.length;
    const names = newTrackNames(doc.tracks.map((t) => t.name), v.count, v.base);
    const ids = names.map(() => uuid());
    const ops: Op[] = names.map((name, k) => ({ op: "add_track", id: ids[k]!, name, stereo: v.stereo, index: start + k, ...(v.midi ? { midi: true } : {}) }));
    const what = v.midi ? "MIDI Track" : "Track";
    await session.edit(v.count === 1 ? `Add ${what}` : `Add ${v.count} ${what}s`, { op: "batch", ops }, sessionHistory);
    editor.selectTrack(ids[0]!);
  }

  function addTrack(stereo: boolean) {
    const doc = session.current;
    if (!doc) return;
    const sel = editor.current().selectedTrack;
    const i = doc.tracks.findIndex((t) => t.id === sel);
    const id = uuid();
    const index = i >= 0 ? i + 1 : doc.tracks.length;
    return session
      .edit(stereo ? "Add Stereo Track" : "Add Mono Track", { op: "add_track", id, name: nextTrackName(doc.tracks), stereo, index }, sessionHistory)
      .then(() => editor.selectTrack(id));
  }

  /** Multitrack > Add > MIDI Track: after the selected track, ready for an instrument. */
  async function addMidiTrack() {
    const doc = session.current;
    if (!doc) return;
    const sel = editor.current().selectedTrack;
    const i = doc.tracks.findIndex((t) => t.id === sel);
    const id = uuid();
    const [name] = newTrackNames(
      doc.tracks.map((t) => t.name),
      1,
      "MIDI",
    );
    await session.edit("Add MIDI Track", { op: "add_track", id, name: name!, stereo: true, index: i >= 0 ? i + 1 : doc.tracks.length, midi: true }, sessionHistory);
    editor.selectTrack(id);
    showNotice("Added a MIDI track. Pick its instrument in the track header, then double-click the lane to make a clip.");
  }

  /** Multitrack > Add > Bus Track: a bus at the end of the Mixer, shown there. */
  async function addBus(stereo = true) {
    const doc = session.current;
    if (!doc) return;
    const buses = doc.buses ?? [];
    const taken = new Set(buses.map((b) => b.name));
    let n = buses.length + 1;
    while (taken.has(`Bus ${n}`)) n++;
    const id = uuid();
    await session.edit(stereo ? "Add Bus Track" : "Add Mono Bus Track", { op: "add_bus", id, name: `Bus ${n}`, stereo, index: buses.length }, sessionHistory);
    editor.selectTrack(id);
    showNotice(`Added Bus ${n}. Route a track to it with the track's output or a send (Sends view in the track headers, or the Mixer).`);
  }

  /** Effects > Plug-in Manager. */
  let pluginManagerOpen = $state(false);

  /** Brings what is set in plug-in windows into the session (see plugin-sync.ts). */
  const pluginSync = new PluginSync({
    doc: () => session.current,
    events: () => pluginEvents(),
    state: (id) => pluginState(id),
    commit: async (ops) => {
      await session.edit("Plug-in Settings", ops.length === 1 ? ops[0]! : { op: "batch", ops }, sessionHistory);
    },
    onStopped: (slot, why) => {
      markStopped(slot.id, why);
      showNotice(`A plug-in stopped: ${why} Restart it from its effect window.`);
    },
    onLatency: () => {
      // The plug-in's delay changed: start again where it is, so every track lines up.
      if (transport.current.playing) void transport.play(transport.current.position).catch(showError);
    },
  });

  /** A mixdown with every plug-in's latest settings saved in the session first. */
  async function exportMix(path: string, req: Parameters<typeof exportMixdown>[1]) {
    await pluginSync.captureAll().catch(showError);
    return exportMixdown(path, req);
  }

  /** Gives every `id` field inside a copied object a new value. */
  function freshIds<T>(value: T): T {
    if (Array.isArray(value)) return value.map(freshIds) as T;
    if (value && typeof value === "object") {
      const out: Record<string, unknown> = {};
      for (const [k, v] of Object.entries(value)) out[k] = k === "id" ? uuid() : freshIds(v);
      return out as T;
    }
    return value;
  }

  /** Track Visibility: set which tracks the Editor and Mixer list, as one undoable step. */
  function visibilityActions() {
    const tracks = () => session.current?.tracks ?? [];
    const apply = async (label: string, keep: (t: TrackDoc) => boolean) => {
      const changes = showOnly(tracks(), keep);
      if (!changes.length) return;
      await session.edit(label, { op: "batch", ops: changes.map((c) => ({ op: "set_track" as const, id: c.id, changes: { hidden: c.hidden } })) }, sessionHistory);
    };
    const spans = (t: TrackDoc) => {
      const d = session.current;
      return t.clips.map((c) => ({ start: c.start, length: clipLength(c, d?.media.find((m) => m.id === c.media_id), d?.sample_rate ?? 48000) }));
    };
    const mk = (id: string, label: string, keep: (t: TrackDoc) => boolean, enabled: () => boolean = () => tracks().length > 0, reason = "No tracks.") =>
      actions.register({ id, run: () => apply(label, keep), enabled, reason: () => reason });
    return [
      actions.register({
        id: "multitrack.vis.hide-selected",
        run: async () => {
          const t = selectedTrack();
          if (t) await apply("Hide Track", (x) => x.id !== t.id && !x.hidden);
        },
        enabled: () => selectedTrack() !== null,
        reason: () => "Select a track first.",
      }),
      mk("multitrack.vis.show-all", "Show All Tracks", () => true, () => tracks().some((t) => t.hidden), "No tracks are hidden."),
      mk("multitrack.vis.only-audio", "Show Only Audio Tracks", () => true, () => tracks().some((t) => t.hidden), "Bus tracks always show below the audio tracks."),
      mk("multitrack.vis.only-clips", "Show Only Tracks with Clips", (t) => t.clips.length > 0),
      mk("multitrack.vis.only-selection", "Show Only Tracks with Clips in Selection", (t) => {
        const s = timeSel();
        return !!s && hasClipIn(spans(t), s.start, s.end);
      }, () => tracks().length > 0 && timeSel() !== null, "Make a time selection first."),
      mk("multitrack.vis.only-playhead", "Show Only Tracks with Clips at Playhead", (t) => {
        const p = session.current?.view.playhead ?? 0;
        return hasClipIn(spans(t), p, p);
      }),
    ];
  }

  const selectedTrack = () => session.current?.tracks.find((t) => t.id === editor.current().selectedTrack) ?? null;

  function usedMedia(): Set<string> {
    const used = new Set<string>();
    for (const t of session.current?.tracks ?? []) for (const c of t.clips) used.add(c.media_id);
    return used;
  }

  // Questions that need an answer before work continues.
  let promptReq = $state<{ title: string; label: string; value: string; resolve: (v: string | null) => void } | null>(null);
  function promptText(title: string, label: string, value: string): Promise<string | null> {
    return new Promise((resolve) => {
      promptReq = { title, label, value, resolve };
    });
  }

  // Clip commands. They act on the selected clips; split and trim also
  // need the playhead inside the clip.
  const selectedClipDocs = () => {
    const doc = session.current;
    return doc ? findClips(doc, editor.current().selectedClips).map((l) => l.clip) : [];
  };
  const playheadNow = () => (transport.current.playing ? transport.current.position : (session.current?.view.playhead ?? 0));

  async function editClips(label: string, changes: ClipChange[]) {
    if (changes.length > 0) await session.edit(label, { op: "set_clips", changes }, sessionHistory);
  }

  async function split(all: boolean) {
    const doc = session.current;
    if (!doc) return;
    const at = playheadNow();
    const ids = all ? clipsAt(doc, at, new Set()) : clipsAt(doc, at, editor.current().selectedClips);
    const op = splitClips(doc, ids, at, uuid);
    if (!op) {
      showNotice("Move the playhead inside a clip to split it.");
      return;
    }
    await session.edit(ids.length > 1 ? "Split Clips" : "Split Clip", op, sessionHistory);
  }

  async function trimTo(edge: "start" | "end") {
    const doc = session.current;
    if (!doc) return;
    const at = playheadNow();
    const ids = clipsAt(doc, at, editor.current().selectedClips).filter((id) => editor.current().selectedClips.has(id));
    const changes = trimToPlayhead(doc, ids, at, edge);
    if (changes.length === 0) showNotice("Select a clip and put the playhead inside it.");
    await editClips(edge === "start" ? "Trim Start" : "Trim End", changes);
  }

  async function toggleFade(which: "in" | "out") {
    const doc = session.current;
    if (!doc) return;
    const clips = selectedClipDocs();
    const on = clips.every((c) => (which === "in" ? c.fade_in : c.fade_out).length > 0);
    const second = doc.sample_rate;
    const changes = clips
      .map((c) => fadeClip(doc, c, which, on ? 0 : second))
      .filter((c): c is ClipChange => c !== null);
    await editClips(on ? `Remove Fade ${which === "in" ? "In" : "Out"}` : `Fade ${which === "in" ? "In" : "Out"}`, changes);
  }

  async function clipGain() {
    const clips = selectedClipDocs();
    if (clips.length === 0) return;
    const v = await promptText("Clip Gain", "Gain in dB (-96 to +24)", clips[0]!.gain_db.toFixed(1));
    if (v === null) return;
    const db = Number(v.replace(",", "."));
    if (!Number.isFinite(db)) {
      showNotice("Clip gain must be a number of dB, like -3 or 2.5.");
      return;
    }
    const g = Math.max(-96, Math.min(24, db));
    await editClips("Clip Gain", clips.map((c) => ({ id: c.id, gain_db: g })));
  }

  // Clipboard for clips. Lives for the app session; not the system clipboard.
  let clipBoard: ClipBoard | null = null;
  const inEditor = () => currentArea() === "multitrack";
  const timeSel = () => session.current?.view.selection ?? null;

  /** Runs an edit that may hit a locked clip, showing why instead of failing silently. */
  async function guarded(label: string, build: () => Op | null, empty: string) {
    let op: Op | null;
    try {
      op = build();
    } catch (e) {
      if (e instanceof LockedError) {
        showNotice(e.message);
        return;
      }
      throw e;
    }
    if (!op) {
      showNotice(empty);
      return;
    }
    await session.edit(label, op, sessionHistory);
  }

  function copySelected(): boolean {
    const doc = session.current;
    if (!doc) return false;
    clipBoard = copyClips(doc, editor.current().selectedClips);
    actions.invalidate();
    return clipBoard !== null;
  }

  async function pasteAtPlayhead() {
    const doc = session.current;
    if (!doc || !clipBoard) return;
    const sel = editor.current().selectedTrack;
    const row = Math.max(0, doc.tracks.findIndex((t) => t.id === sel));
    const r = pasteClips(doc, clipBoard, playheadNow(), row, uuid);
    if (!r) {
      showNotice("The copied clips' files are no longer in this session.");
      return;
    }
    await session.edit(r.ids.length === 1 ? "Paste Clip" : `Paste ${r.ids.length} Clips`, r.op, sessionHistory);
    editor.selectClips(r.ids);
  }

  // Mixdown. One export at a time, with a progress dialog.
  let exporting = $state<{ title: string; progress: number; floating?: boolean } | null>(null);

  type MixSource = "all" | "selection" | "clips";
  type MixTarget = "file" | "files-panel" | "track";

  function mixRange(source: MixSource): { range?: [number, number]; clips?: string[]; start: number } | null {
    const doc = session.current;
    if (!doc) return null;
    if (source === "selection") {
      const r = timeSel();
      return r ? { range: [r.start, r.end], start: r.start } : null;
    }
    if (source === "clips") {
      const found = findClips(doc, editor.current().selectedClips);
      if (found.length === 0) return null;
      const start = Math.min(...found.map((f) => f.clip.start));
      const end = Math.max(...found.map((f) => f.clip.start + clipLength(f.clip, doc.media.find((m) => m.id === f.clip.media_id), doc.sample_rate)));
      return { range: [start, end], clips: found.map((f) => f.clip.id), start };
    }
    return { start: 0 };
  }

  const KEY_EXPORT = "export.mixdown";
  let exportReq = $state<{ initial: ExportSettings; scopes: ExportScopeInfo[]; source: { sampleRate: number; channels: number; bitDepth: number } } | null>(null);

  function sessionFolder(): string | null {
    const sp = session.path;
    return sp ? sp.slice(0, Math.max(sp.lastIndexOf("/"), sp.lastIndexOf("\\"))) || null : null;
  }

  // Effects > Generate, and Insert Silence.
  const KEY_GENERATE = "generate.last";
  let generateMode = $state<GenerateMode | null>(null);
  const generateDefaults = (): GenerateValues => {
    const saved = prefs.raw(KEY_GENERATE) as Partial<GenerateValues> | null;
    return {
      seconds: typeof saved?.seconds === "number" ? saved.seconds : 5,
      frequency: typeof saved?.frequency === "number" ? saved.frequency : 1000,
      levelDb: typeof saved?.levelDb === "number" ? saved.levelDb : -18,
      noise: saved?.noise === "white" ? "white" : "pink",
      channels: saved?.channels === 1 ? 1 : 2,
      allTracks: saved?.allTracks !== false,
      overwrite: false,
    };
  };
  const selectedTrackDoc = () => session.current?.tracks.find((t) => t.id === editor.current().selectedTrack) ?? null;

  async function applyGenerate(mode: GenerateMode, v: GenerateValues) {
    generateMode = null;
    prefs.set(KEY_GENERATE, v);
    const doc = session.current;
    if (!doc) return;
    const at = playheadNow();
    const len = Math.round(v.seconds * doc.sample_rate);
    if (mode === "silence") {
      const tracks = v.allTracks ? doc.tracks.map((t) => t.id) : [editor.current().selectedTrack ?? ""];
      const sel = timeSel();
      if (v.overwrite && sel) {
        await guarded("Silence Time Selection", () => clearRange(doc, tracks, sel.start, sel.end, uuid), "There is no audio in the time selection.");
        return;
      }
      await guarded("Insert Silence", () => insertTime(doc, tracks, at, len, uuid), "Nothing to move.");
      return;
    }
    const folder = await folderBesideSession("Generated", "Generated audio is saved beside the session.");
    if (!folder) return;
    const sep = folder.includes("\\") ? "\\" : "/";
    const what = mode === "tone" ? `Tone ${Math.round(v.frequency)} Hz` : `${v.noise === "pink" ? "Pink" : "White"} noise`;
    const stamp = new Date().toISOString().replace(/[-:]/g, "").replace("T", "-").slice(0, 15);
    const path = `${folder}${sep}${what} ${stamp}.wav`;
    await generateAudio(path, {
      kind: mode === "tone" ? "tone" : v.noise,
      seconds: v.seconds,
      frequency: v.frequency,
      levelDb: v.levelDb,
      sampleRate: doc.sample_rate,
      channels: v.channels,
    });
    await placeFiles([path], editor.current().selectedTrack, at);
  }

  // Clip > Time Stretch.
  // Clip > Fit Music to Length.
  let fitFor = $state<string | null>(null);
  const fitClip = $derived.by(() => {
    const doc = session.current;
    if (!doc || !fitFor) return null;
    for (const t of doc.tracks) for (const c of t.clips) if (c.id === fitFor) return { clip: c, trackId: t.id };
    return null;
  });
  function planFit(seconds: number): Promise<FitPlan[]> {
    const doc = session.current;
    const f = fitClip;
    const m = f ? doc?.media.find((x) => x.id === f.clip.media_id) : undefined;
    if (!doc || !f || !m) return Promise.reject(new Error("The clip is gone."));
    const rate = m.sample_rate || doc.sample_rate;
    return musicFit(m.path, f.clip.source_in / rate, f.clip.source_out / rate, seconds, FIT_XFADE);
  }
  // Multitrack > Speaker Balance.
  let balancing = $state(false);
  async function runSpeakerBalance() {
    const doc = session.current;
    if (!doc || balancing) return;
    const items = balanceItems(doc);
    if (items.length < 2) return showNotice("Speaker Balance needs at least two voice tracks with audio.");
    balancing = true;
    try {
      const reports: LoudnessReport[] = [];
      for (const [i, item] of items.entries()) {
        showNotice(`Speaker Balance: measuring ${item.label} (${i + 1} of ${items.length})…`);
        reports.push(await loudnessCheck({ track: item.key, clips: item.clipIds, clipOnly: true, dry: true }));
      }
      const now = session.current ?? doc;
      const b = balance(now, items, reports);
      if (b.changes.length > 0) await session.edit("Speaker Balance", { op: "set_clips", changes: b.changes }, sessionHistory);
      showNotice(describeBalance(b));
    } finally {
      balancing = false;
    }
  }

  /** The track a Host Profile command acts on: the selected one. */
  const profileTrack = () => session.current?.tracks.find((t) => t.id === editor.current().selectedTrack) ?? null;
  async function applyProfile(id: string | null) {
    const t = profileTrack();
    if (!t) return;
    const p = id ? (profileById(id) ?? null) : null;
    await session.edit(p ? `Host Profile: ${p.name}` : "Clear Host Profile", profileOp(t, p, uuid), sessionHistory);
    if (p) {
      dock.show("voice-chain");
      showNotice(`${t.name}: ${p.name} profile applied (gate at ${p.gateDb} dB${p.role === "guest" ? ", guest track" : ""}).`);
    }
  }

  async function runPhaseAlign() {
    const d = session.current;
    const pair = d ? alignPair(d, editor.current().selectedClips) : null;
    if (!d || !pair) return;
    const a = await phaseAlign(pair.reference, pair.other, ALIGN_MAX_MS);
    const now = session.current ?? d;
    const op = alignOps(now, pair.other, pair.trackId, a);
    const track = now.tracks.find((t) => t.id === pair.trackId);
    const clip = track?.clips.find((c) => c.id === pair.other);
    if (op) await session.edit("Phase Align", op, sessionHistory);
    showNotice(describeAlign(a, now.sample_rate, clip?.name ?? "the clip", track?.name ?? "its track"));
  }

  async function applyFit(plan: FitPlan) {
    const doc = session.current;
    const f = fitClip;
    const m = f ? doc?.media.find((x) => x.id === f.clip.media_id) : undefined;
    fitFor = null;
    if (!doc || !f || !m) return;
    await session.edit("Fit Music to Length", fitOps(doc, f.trackId, f.clip, m, plan, uuid), sessionHistory);
    const origin = f.clip.source_in / (m.sample_rate || doc.sample_rate);
    showNotice(`Fit ${f.clip.name}: ${describePlan(plan)}. ${describeJoins(plan, origin)}.`);
  }

  let stretchFor = $state<string | null>(null);
  const stretchClip = $derived.by(() => {
    const doc = session.current;
    if (!doc || !stretchFor) return null;
    for (const t of doc.tracks) for (const c of t.clips) if (c.id === stretchFor) return c;
    return null;
  });

  async function applyStretch(seconds: number, keepPitch: boolean, toSelection: boolean) {
    const doc = session.current;
    const clip = stretchClip;
    if (!doc || !clip) return;
    const m = doc.media.find((x) => x.id === clip.media_id);
    const r = stretchTo(clip, m, doc.sample_rate, Math.round(seconds * doc.sample_rate), keepPitch);
    stretchFor = null;
    if (!r) return showNotice("Nothing to change.");
    const sel = toSelection ? timeSel() : null;
    if (sel) r.change.start = sel.start;
    await guarded("Time Stretch", () => ({ op: "set_clips", changes: [r.change] }), "Nothing to change.");
    if (r.limited) showNotice("A clip plays from 25% to 400% of its natural length, so the stretch was limited.");
  }

  async function removeStretch(ids: Iterable<string>) {
    const doc = session.current;
    if (!doc) return;
    const media = mediaOf(doc);
    const changes = findClips(doc, ids).flatMap(({ clip }) => {
      const ch = unstretch(clip, media.get(clip.media_id), doc.sample_rate);
      return ch ? [ch] : [];
    });
    if (changes.length) await editClips("Remove Stretch", changes);
  }

  /** Playlist panel: each range as its own file, numbered in playlist order, in the last export format. */
  async function exportPlaylist(items: MarkerDoc[]) {
    const doc = session.current;
    if (!doc || items.length === 0 || exporting) return;
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
    exporting = { title: "Exporting playlist", progress: 0, floating: true };
    try {
      for (let i = 0; i < items.length; i++) {
        const m = items[i]!;
        exporting = { title: `Exporting ${i + 1} of ${items.length}: ${m.name}`, progress: i / items.length, floating: true };
        const r = await exportMix(paths[i]!, { range: [m.position, m.position + m.duration], format });
        if (r.cancelled) {
          showNotice(`Export stopped after ${i} of ${items.length} files.`);
          return;
        }
      }
      showNotice(`Exported ${items.length} ${items.length === 1 ? "file" : "files"} as ${describeFormat(format, { sampleRate: doc.sample_rate, channels: 2, bitDepth: doc.bit_depth })}.`);
    } finally {
      exporting = null;
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
      },
      scopes,
      source: { sampleRate: doc.sample_rate, channels: doc.master?.channels === "mono" ? 1 : 2, bitDepth: doc.bit_depth },
    };
  }

  async function finishExportDialog(v: ExportSettings | null) {
    exportReq = null;
    if (!v) return;
    prefs.set(KEY_EXPORT, { format: v.format, folder: v.folder, addToFiles: v.addToFiles, chapters: v.chapters });
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
    await runMixdown(v.scope, v.addToFiles ? "files-panel" : "file", path, v.format, v.chapters);
  }

  async function mixDown(source: MixSource, target: MixTarget) {
    const doc = session.current;
    if (!doc || !mixRange(source) || exporting) return;
    if (target === "file") {
      openExportDialog(source);
      return;
    }
    const suffix = source === "all" ? "Mixdown" : source === "selection" ? "Selection" : "Clips";
    const path = await exportPickPath(`${doc.name} ${suffix}`, sessionFolder());
    if (!path) return;
    await runMixdown(source, target, path, null);
  }

  async function runMixdown(source: MixSource, target: MixTarget, path: string, format: OutputFormat | null, chapters = false) {
    const what = mixRange(source);
    if (!what || exporting) return;
    exporting = { title: target === "track" ? "Bouncing to a new track" : "Exporting mixdown", progress: 0, floating: true };
    let off: (() => void) | null = null;
    try {
      off = await listen<{ done: number; total: number }>("export-progress", (e) => {
        if (exporting) exporting = { ...exporting, progress: e.payload.total > 0 ? e.payload.done / e.payload.total : 0 };
      });
      const r = await exportMix(path, {
        ...(what.range ? { range: what.range } : {}),
        ...(what.clips ? { clips: what.clips } : {}),
        ...(format ? { format } : {}),
      });
      if (r.cancelled) {
        showNotice("Export cancelled. Nothing was saved.");
        return;
      }
      const name = path.split(/[\\/]/).pop();
      let chapterNote = "";
      const doc = session.current;
      if (chapters && doc && (format?.kind === "mp3" || format?.kind === "flac")) {
        const list = embedList(doc.markers, doc.sample_rate, what.range ?? null);
        if (list.length) {
          await exportEmbedChapters(path, list, Math.round((r.frames / r.sampleRate) * 1000));
          chapterNote = ` ${list.length} ${list.length === 1 ? "chapter" : "chapters"} added.`;
        }
      }
      const clipped = r.peak > 1 ? ` The mix clipped (peak +${(20 * Math.log10(r.peak)).toFixed(1)} dB); turn the Master down and export again.` : "";
      if (target === "files-panel") {
        const imported = await mediaPool.importPaths([path], sessionHistory);
        mediaPool.setSelection(imported.ids);
        showNotice(`Mixed down to ${name} and added it to the Files panel.${chapterNote}${clipped}`);
      } else if (target === "track") {
        await placeFiles([path], null, what.start);
        showNotice(`Bounced to ${name} on a new track.${clipped}`);
      } else {
        showNotice(`Exported ${name}.${chapterNote}${clipped}`);
      }
    } catch (e) {
      showError(e);
    } finally {
      off?.();
      exporting = null;
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
    if (!doc || exporting) return;
    const tracks = doc.tracks.filter((t) => t.clips.length > 0 && !t.muted);
    if (tracks.length === 0) {
      showNotice("No unmuted tracks with clips to export.");
      return;
    }
    const sp = session.path;
    const start = sp ? sp.slice(0, Math.max(sp.lastIndexOf("/"), sp.lastIndexOf("\\"))) || null : null;
    const folder = await exportPickFolder(start);
    if (!folder) return;
    const sep = folder.includes("\\") ? "\\" : "/";
    let off: (() => void) | null = null;
    let done = 0;
    try {
      for (const [i, t] of tracks.entries()) {
        exporting = { title: `Exporting stem ${i + 1} of ${tracks.length}: ${t.name}`, progress: 0, floating: true };
        off?.();
        off = await listen<{ done: number; total: number }>("export-progress", (e) => {
          if (exporting) exporting = { ...exporting, progress: e.payload.total > 0 ? e.payload.done / e.payload.total : 0 };
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
      exporting = null;
    }
  }

  // Check Loudness: measures the whole session, or the time selection when there is one.
  const KEY_LOUD_TARGET = "loudness.target";
  const KEY_LOUD_REGION = "loudness.region";
  let loud = $state<{ report: LoudnessReport | null; progress: number | null; target: string; region: string; scope: string } | null>(null);

  async function measureLoudness() {
    if (!loud || loud.progress !== null) return;
    const r = timeSel();
    loud = { ...loud, progress: 0, scope: r ? "the time selection" : "the whole session" };
    let off: (() => void) | null = null;
    try {
      off = await listen<{ done: number; total: number }>("export-progress", (e) => {
        if (loud && loud.progress !== null) loud = { ...loud, progress: e.payload.total > 0 ? e.payload.done / e.payload.total : 0 };
      });
      const report = await loudnessCheck(r ? { range: [r.start, r.end] } : {});
      if (loud) loud = { ...loud, progress: null, report: report.cancelled ? loud.report : report };
    } catch (e) {
      if (loud) loud = { ...loud, progress: null };
      showError(e);
    } finally {
      off?.();
    }
  }

  async function applyMasterGain(db: number) {
    const cur = session.current?.master?.volume_db ?? 0;
    const next = Math.round((cur + db) * 10) / 10;
    await session.edit("Master Volume", { op: "set_master_volume", volume_db: next }, sessionHistory);
    showNotice(`Master set to ${next > 0 ? "+" : ""}${next.toFixed(1)} dB. Measuring again.`);
    if (loud) loud = { ...loud, report: null };
    await measureLoudness();
  }

  // Effects > Loudness Match: clip gain so each selected clip, or each track, reaches a target.
  const KEY_MATCH_TARGET = "loudness.match-target";
  let lm = $state<{ mode: "clips" | "tracks"; items: MatchItem[]; rows: MatchRow[] | null; progress: number | null; target: string; region: string } | null>(null);
  let lmStop = false;

  function openLoudnessMatch() {
    const doc = session.current;
    if (!doc) return;
    const { mode, items } = matchItems(doc, editor.current().selectedClips);
    if (items.length === 0) return showNotice("There are no clips to match.");
    lm = { mode, items, rows: null, progress: null, target: prefs.string(KEY_MATCH_TARGET, DEFAULT_TARGET), region: prefs.string(KEY_LOUD_REGION, DEFAULT_REGION) };
    void measureMatch();
  }

  async function measureMatch() {
    const doc = session.current;
    if (!lm || !doc || lm.progress !== null) return;
    const t = targetById(lm.target, lm.region);
    const media = mediaOf(doc);
    const rows: MatchRow[] = [];
    lmStop = false;
    lm = { ...lm, progress: 0 };
    try {
      for (let i = 0; i < lm.items.length; i++) {
        const item = lm.items[i]!;
        let report: LoudnessReport;
        if (lm.mode === "clips") {
          const clip = doc.tracks.flatMap((x) => x.clips).find((c) => c.id === item.clipIds[0]);
          if (!clip) continue;
          report = await loudnessCheck({ range: [clip.start, clipEnd(clip, media, doc.sample_rate)], clips: [clip.id], clipOnly: true });
        } else {
          report = await loudnessCheck({ track: item.key, clipOnly: true });
        }
        if (report.cancelled || lmStop || !lm) break;
        rows.push(matchRow(item, report, t));
        lm = { ...lm, progress: (i + 1) / lm.items.length };
      }
      if (lm) lm = { ...lm, progress: null, rows: lmStop ? lm.rows : rows };
    } catch (e) {
      if (lm) lm = { ...lm, progress: null };
      showError(e);
    }
  }

  async function applyMatch(rows: MatchRow[]) {
    const doc = session.current;
    if (!doc) return;
    const changes = matchChanges(doc, rows);
    if (changes.length === 0) return showNotice("Nothing needed changing.");
    await session.edit("Loudness Match", { op: "set_clips", changes }, sessionHistory);
    showNotice(`Clip gain changed on ${changes.length} ${changes.length === 1 ? "clip" : "clips"}. Measuring again.`);
    if (lm) lm = { ...lm, rows: null };
    await measureMatch();
  }

  let colorFor = $state<{ ids: string[]; current: string | null } | null>(null);

  let propertiesOpen = $state(false);

  async function applyProperties(next: SessionProperties | null) {
    propertiesOpen = false;
    const doc = session.current;
    if (!next || !doc) return;
    const op = propertiesOp(doc, next);
    if (op) await session.edit("Session Properties", op, sessionHistory);
  }

  let choice = $state<{ title: string; message: string; buttons: ChoiceButton[]; resolve: (v: string) => void } | null>(null);

  function ask(title: string, message: string, buttons: ChoiceButton[]): Promise<string> {
    return new Promise((resolve) => {
      choice = { title, message, buttons, resolve };
    });
  }

  const KEY_RECENT = "sessions.recent";
  const KEY_EDITOR = "editor.settings";

  const files = new SessionFiles({
    backend: {
      create: (options) => sessionNew<SessionDoc>(options),
      save: (path, media, libs) => sessionSave<SessionDoc, MediaRefDoc>(path, media, libs, settings.current.backups ? settings.current.backupCount : 0),
      open: (path) => sessionOpen<SessionDoc>(path),
      autosave: (media) => sessionAutosave(media),
      clearAutosave: () => sessionClearAutosave(),
      recoverable: () => sessionRecoverable(),
      recover: (p) => sessionRecover<SessionDoc>(p),
      discardRecovery: (id) => sessionDiscardRecovery(id),
      pickSave: (name) => sessionPickSave(name),
      pickOpen: () => sessionPickOpen(false),
      pickSesx: () => sessionPickOpen(true),
      importSesx: (p) => sessionImportSesx<SessionDoc>(p),
    },
    session,
    pool: mediaPool,
    stack: sessionHistory,
    askSave: (name) =>
      ask("Unsaved changes", `Save changes to "${name}" before closing it?`, [
        { label: "Save", value: "save", primary: true },
        { label: "Don't Save", value: "discard" },
        { label: "Cancel", value: "cancel" },
      ]) as Promise<Choice>,
    askRecover: (r: Recoverable) =>
      ask(
        "Recover unsaved work?",
        `Altered Audio Editor did not close normally. "${r.note.original_path ? displayName(r.note.original_path, null) : r.note.name}" has unsaved work from ${new Date(r.note.saved_at).toLocaleString()}. Recover it?`,
        [
          { label: "Recover", value: "yes", primary: true },
          { label: "Discard", value: "no" },
        ],
      ).then((v) => v === "yes"),
    recent: {
      get: () => {
        const raw = prefs.raw(KEY_RECENT);
        return Array.isArray(raw) ? raw.filter((v): v is string => typeof v === "string") : [];
      },
      set: (paths) => prefs.set(KEY_RECENT, paths),
    },
    notice: (t) => showNotice(t),
    libraries: () => libraries.current.libs.map((l) => l.dir),
    onReplaced: (doc) => {
      editor.selectTrack(null);
      // Reopen the libraries this session uses.
      for (const dir of doc.libraries ?? []) {
        if (!libraries.current.libs.some((l) => l.dir === dir)) {
          libraries.openDir(dir).catch(() => showNotice(`Library not found: ${dir}`));
        }
      }
    },
  });

  let lastTitle = "";
  /** The Editor tab names what it shows: the session, or the file in the Waveform editor. */
  function updateEditorTitle() {
    const f = waveFile.current();
    const what = inWave() && f ? (f.path.split(/[\\/]/).pop() ?? "") : files.name;
    dock.setTitle("editor", what ? `Editor - ${what}${inWave() ? "" : files.unsaved ? " *" : ""}` : "Editor");
  }
  /** The open session as a row at the top of the Files panel. */
  const sessionRow = writable<SessionRow | null>(null);
  function updateSessionRow() {
    const d = session.current;
    if (!d) {
      sessionRow.set(null);
      return;
    }
    // A never-saved session with nothing in it (files imported only) is not listed as "Untitled Session".
    if (session.isScratch(d)) {
      if (get(sessionRow)) sessionRow.set(null);
      return;
    }
    const seconds = sessionEnd(d) / Math.max(1, d.sample_rate);
    const row: SessionRow = { name: files.name, dirty: files.unsaved, seconds, sampleRate: d.sample_rate, active: !inWave() };
    const cur = get(sessionRow);
    if (!cur || cur.name !== row.name || cur.dirty !== row.dirty || cur.seconds !== row.seconds || cur.sampleRate !== row.sampleRate || cur.active !== row.active) sessionRow.set(row);
  }
  function updateTitle() {
    updateSessionRow();
    updateEditorTitle();
    const t = files.title();
    if (t !== lastTitle) {
      lastTitle = t;
      void setWindowTitle(t).catch(() => {});
    }
  }

  async function exitSafely() {
    if (!(await files.confirmDiscard())) return;
    if (!(await confirmWaveDiscard())) return;
    const others = parkedUnsaved();
    if (others.length) {
      const v = await ask(
        "Other files have unsaved edits",
        `${others.join(", ")} ${others.length === 1 ? "has" : "have"} unsaved edits in the Waveform editor. Open ${others.length === 1 ? "it" : "each one"} from the Files panel to save, or quit without saving.`,
        [
          { label: "Cancel", value: "cancel", primary: true },
          { label: "Quit Without Saving", value: "quit" },
        ],
      );
      if (v !== "quit") return;
      await forgetParked();
    }
    files.stopAutosave();
    await prefs.flush();
    await exitApp();
  }

  // A dropped session file (ours or .sesx) opens wherever it lands.
  const fileDrop = new FileDropRouter(
    (paths) => void importPaths(paths).catch(showError),
    (paths) => void openPaths(paths).catch(showError),
  );

  const asComponent = (c: unknown) => c as Component<Record<string, unknown>>;

  function content(id: PanelId): PanelContent {
    switch (id) {
      case "history":
        return {
          component: asComponent(HistoryPanel),
          props: { stack: sessionHistory, waveStack: waveHistory, shortcutFor, onError: showError },
        };
      case "files":
        return {
          component: asComponent(FilesPanel),
          props: {
            pool: mediaPool,
            actions,
            drop: fileDrop,
            shortcutFor,
            onDropPaths: (paths: string[]) => void importPaths(paths).catch(showError),
            onOpen: (path: string) => void openInWaveform(path),
            session: sessionRow,
            onOpenSession: () => void actions.run("view.editor.multitrack"),
          },
        };
      case "drive-manager":
        return {
          component: asComponent(DriveManagerPanel),
          props: {
            router: fileDrop,
            prefs,
            onImport: (paths: string[]) => void importPaths(paths).catch(showError),
            onError: showError,
          },
        };
      case "mixer":
        return {
          component: asComponent(MixerPanel),
          props: { session, transport, runner: sessionHistory, onError: showError },
        };
      case "time-info":
        return { component: asComponent(TimeInfoPanel), props: { session, transport, wave, waveTransport } };
      case "levels":
        return { component: asComponent(LevelsPanel), props: { transport, wave: waveTransport, preview: previewMeter } };
      case "phase-wheel":
        return { component: asComponent(PhaseWheelPanel), props: { transport, wave: waveTransport } };
      case "batch-convert":
        return {
          component: asComponent(BatchConvertPanel),
          props: { pool: mediaPool, drop: fileDrop, prefs, onNotice: showNotice, onError: showError },
        };
      case "voice-chain":
        return {
          component: asComponent(VoiceChainPanel),
          props: { session, runner: sessionHistory, prefs, ask: promptText, transport, wave, waveTransport, onApplyWave: applyWaveChain, onNotice: showNotice, onError: showError },
        };
      case "quickpass":
        return {
          component: asComponent(QuickPassPanel),
          props: { session, transport, runner: sessionHistory, prefs, ask: promptText, onRepair: repairSpots, onNotice: showNotice, onError: showError, wave, waveTransport, waveRunner: waveHistory },
        };
      case "libraries":
        return {
          component: asComponent(LibrariesPanel),
          props: {
            store: libraries,
            runner: sessionHistory,
            router: fileDrop,
            peaks,
            shortcutFor,
            onAddToSession: (paths: string[]) => void importPaths(paths).catch(showError),
            onNotice: showNotice,
            onError: showError,
          },
        };
      case "properties":
        return { component: asComponent(PropertiesPanel), props: { session, onEdit: editClips, onError: showError } };
      case "playlist":
        return {
          component: asComponent(PlaylistPanel),
          props: { session, transport, runner: sessionHistory, onExport: (items: MarkerDoc[]) => void exportPlaylist(items).catch(showError), onError: showError },
        };
      case "markers":
        return {
          component: asComponent(MarkersPanel),
          props: { session, transport, runner: sessionHistory, actions, onError: showError, wave, waveRunner: waveHistory, waveTransport, waveFile },
        };
      case "editor":
        return {
          component: asComponent(EditorPanel),
          props: {
            session,
            runner: sessionHistory,
            peaks,
            transport,
            router: fileDrop,
            actions,
            shortcutFor,
            onError: showError,
            onPlaceFiles: (paths: string[], trackId: string | null, at: number) =>
              void placeFiles(paths, trackId, at).catch(showError),
            prompt: promptText,
            wave,
            waveFile,
            waveTransport,
            onWaveLevel: (db: number) => void waveAmplify(db).catch(showError),
          },
        };
      default: {
        const def = panelDef(id);
        return {
          component: asComponent(PendingPanel),
          props: { title: def?.title ?? id, note: def?.pending ?? "Not built yet." },
        };
      }
    }
  }

  const KEY_WORKSPACE = "workspace.current";
  let wsState: WorkspaceState = { current: "default", items: [] };
  const dock = new DockController({
    content,
    workspaces: { list: workspaceList, get: workspaceGet, put: workspacePut, remove: workspaceDelete },
    currentId: () => prefs.string(KEY_WORKSPACE, "default"),
    setCurrentId: (id) => prefs.set(KEY_WORKSPACE, id),
    ready: () => prefs.load(),
    onError: showError,
  });

  let aboutOpen = $state(false);
  let shortcutsOpen = $state(false);

  const shortcutFor = (id: string) => keymap.displayFor(id);

  function saveKeymap() {
    saveKeymapOverrides(keymap.getOverrides()).catch((e: unknown) =>
      showError(`Shortcuts could not be saved: ${String(e)}`),
    );
  }
  let openPanels: ReadonlySet<PanelId> = new Set();
  let body = $state<HTMLElement | null>(null);

  /** What Delete, Select All and Deselect do in each area of the window. */
  interface AreaOps {
    deleteLabel: string;
    selected(): number;
    total(): number;
    remove(): Promise<void> | void;
    selectAll(): void;
    deselect(): void;
  }

  const AREA_OPS: Record<NonNullable<Area>, AreaOps> = {
    files: {
      deleteLabel: "Remove from Session",
      selected: () => mediaPool.current.selected.size,
      total: () => mediaPool.size,
      remove: async () => {
        const used = usedMedia();
        const ids = [...mediaPool.current.selected];
        const inUse = ids.filter((id) => used.has(id));
        const doc = session.current;
        if (inUse.length === 0 || !doc) return mediaPool.remove(ids, sessionHistory);
        // Files on the timeline: remove them with their clips (one undo step), or keep those.
        const clipIds = doc.tracks.flatMap((t) => t.clips.filter((c) => inUse.includes(c.media_id)).map((c) => c.id));
        const n = inUse.length;
        const choice = await ask(
          "Files in use",
          `${n === 1 ? "This file is" : `${n} of these files are`} used by ${clipIds.length} ${clipIds.length === 1 ? "clip" : "clips"} on the timeline. Remove ${n === 1 ? "it" : "them"} and ${clipIds.length === 1 ? "that clip" : "those clips"} too?`,
          [
            { label: "Remove with Clips", value: "all" },
            { label: ids.length > n ? "Keep Those, Remove the Rest" : "Keep It", value: "keep" },
            { label: "Cancel", value: "cancel" },
          ],
        );
        if (choice === "cancel" || choice === null) return;
        if (choice === "keep") return mediaPool.remove(ids.filter((id) => !used.has(id)), sessionHistory);
        const files = mediaPool.removeCommand(ids);
        const parts = [session.command("Remove Clips", { op: "remove_clips", ids: clipIds }), ...(files ? [files] : [])];
        const label = ids.length === 1 ? "Remove File and Clips" : `Remove ${ids.length} Files and Clips`;
        await sessionHistory.execute(new CompoundCommand(label, parts));
        editor.selectClips([]);
      },
      selectAll: () => mediaPool.selectAll(),
      deselect: () => mediaPool.setSelection([]),
    },
    multitrack: {
      deleteLabel: "Delete Clips",
      selected: () => editor.current().selectedClips.size,
      total: () => (session.current?.tracks ?? []).reduce((n, t) => n + t.clips.length + (t.midi_clips?.length ?? 0), 0),
      remove: async () => {
        const doc = session.current;
        const ids = [...editor.current().selectedClips];
        if (!ids.length || !doc) return;
        const ops = deleteClipsOps(doc, new Set(ids));
        if (!ops.length) return;
        await session.edit(ids.length === 1 ? "Delete Clip" : `Delete ${ids.length} Clips`, ops.length === 1 ? ops[0]! : { op: "batch", ops }, sessionHistory);
        editor.selectClips([]);
      },
      selectAll: () => {
        const doc = session.current;
        if (doc) editor.selectClips([...doc.tracks.flatMap((t) => t.clips.map((c) => c.id)), ...allMidiClipIds(doc)]);
      },
      deselect: () => {
        editor.selectClips([]);
        const v = session.current?.view;
        if (v?.selection) session.setView({ ...v, selection: null });
      },
    },
    // Edit actions reach the Waveform editor through overlays; these are the fallbacks.
    waveform: {
      deleteLabel: "Delete Selection",
      selected: () => 0,
      total: () => 0,
      remove: () => {},
      selectAll: () => {},
      deselect: () => {},
    },
    libraries: {
      deleteLabel: "Remove from Library",
      selected: () => libraries.current.selected.size,
      total: () => libraries.shownItems().length,
      remove: () => libraries.removeItems([...libraries.current.selected], sessionHistory),
      selectAll: () => libraries.setSelection(libraries.shownItems().map((i) => i.id)),
      deselect: () => libraries.setSelection([]),
    },
  };

  const areaOps = (): AreaOps | null => {
    const a = currentArea();
    return a ? AREA_OPS[a] : null;
  };
  const AREA_FIRST = "Click in the Files panel, Libraries, or the Editor first.";

  const actions = new ActionRegistry();

  // Waveform editor. Edits act on the selection; Amplify and Normalize
  // act on the whole file when nothing is selected.
  const inWave = () => editor.current().mode === "waveform";
  // Effects menu: adds an effect to the Voice Chain of the one selected
  // clip, or else the selected track, and shows the panel.
  const FX_MENU = [
    "dynamics",
    "limiter",
    "breath_gate",
    "deesser",
    "speech_leveler",
    "parametric_eq",
    "hum_cut",
    "gain",
    "noise_reducer",
    "noise_print",
    "sound_eraser",
    "hiss_reducer",
    "room_reducer",
    "click_repair",
    "clip_repair",
    "dc_offset",
    "band_eq_10",
    "band_eq_20",
    "band_eq_30",
    "curve_eq",
    "precision_filter",
    "notch",
    "pitch_shift",
    "center_remover",
    "phase_rotate",
  ];
  function fxTarget(): { target: EffectTarget; chain: EffectChainDoc | undefined; name: string; wave?: boolean } | null {
    if (inWave()) {
      const t = wave.current?.tracks[0];
      return t ? { target: { kind: "track", id: t.id }, chain: t.effects, name: "the preview chain", wave: true } : null;
    }
    const d = session.current;
    const ed = editor.current();
    if (!d) return null;
    if (ed.selectedClips.size === 1) {
      const id = [...ed.selectedClips][0]!;
      for (const t of d.tracks) {
        const c = t.clips.find((x) => x.id === id);
        if (c) return { target: { kind: "clip", id }, chain: c.effects, name: c.name };
      }
    }
    const t = d.tracks.find((x) => x.id === ed.selectedTrack);
    return t ? { target: { kind: "track", id: t.id }, chain: t.effects, name: t.name } : null;
  }
  let fxCatalog: EffectInfo[] | null = null;
  async function addEffectFromMenu(effect: string) {
    const at = fxTarget();
    if (!at) return;
    fxCatalog ??= await effectsCatalog();
    const info = fxCatalog.find((e) => e.id === effect);
    if (!info) return;
    const slot = newSlot(info, uuid());
    const next = addSlot(chainOf(at.chain), slot);
    if (!next) {
      showNotice("That Voice Chain is full: it holds 16 effects.");
      return;
    }
    const op = { op: "set_effects", target: at.target, chain: next } as const;
    // In the Waveform editor the chain is a preview, not an edit; Apply in the panel writes it.
    if (at.wave) await wave.apply(op);
    else await session.edit(`Add ${info.name}`, op, sessionHistory);
    if (settings.current.openChainOnAdd) dock.show("voice-chain");
    effectWindowRequest.set(slot.id);
    showNotice(at.wave ? `Added ${info.name}. Save writes it into the file; bypass the chain to save without it.` : `Added ${info.name} to ${at.name}.`);
  }

  const waveIsOpen = () => inWave() && waveFile.current() !== null;
  const waveFocus = () => waveIsOpen() && currentArea() !== "files" && currentArea() !== "libraries";
  const waveSel = (): [number, number] | null => {
    const d = wave.current;
    const s = d?.view.selection;
    return d && s ? clampRange(d, s.start, s.end) : null;
  };
  const waveAll = (): [number, number] | null => {
    const d = wave.current;
    return d ? (waveSel() ?? clampRange(d, 0, sessionEnd(d))) : null;
  };
  const NEED_WAVE_SEL = "Drag across the waveform to select audio first.";
  let waveBoard: WaveBoard | null = null;

  async function waveEdit(label: string, op: Op | null): Promise<boolean> {
    if (!op) return false;
    await wave.edit(label, op, waveHistory);
    return true;
  }

  function waveSetSelection(sel: [number, number] | null, playhead?: number) {
    const d = wave.current;
    if (!d) return;
    wave.setView({
      ...d.view,
      selection: sel ? { start: sel[0], end: sel[1] } : null,
      playhead: Math.max(0, Math.round(playhead ?? d.view.playhead)),
    });
  }

  const wavePlayhead = () => (waveTransport.current.playing ? waveTransport.current.position : (wave.current?.view.playhead ?? 0));
  const waveName = () => waveFile.current()?.path.split(/[\\/]/).pop() ?? "the file";

  /** Asks about unsaved Waveform edits. False means stop. */
  async function confirmWaveDiscard(): Promise<boolean> {
    if (!waveFile.current() || !waveHistory.current.dirty) return true;
    const v = await ask("Unsaved changes", `Save changes to ${waveName()}?`, [
      { label: "Save", value: "save", primary: true },
      { label: "Don't Save", value: "discard" },
      { label: "Cancel", value: "cancel" },
    ]);
    if (v === "cancel") return false;
    if (v === "save") return saveWave(false);
    return true;
  }

  /** Opens a file in the Waveform editor and shows it. */
  async function openInWaveform(path: string) {
    // The file open now is set aside with its edits, not closed.
    await transport.stop();
    try {
      await openWaveFile(path);
    } catch (e) {
      showError(e);
      return;
    }
    editor.setMode("waveform");
    dock.show("editor");
    actions.invalidate();
  }

  /** Saves the Waveform editor's file. Returns false if cancelled or failed. */
  async function saveWave(as: boolean): Promise<boolean> {
    const f = waveFile.current();
    if (!f || exporting) return false;
    const cut = Math.max(f.path.lastIndexOf("/"), f.path.lastIndexOf("\\"));
    const folder = cut > 0 ? f.path.slice(0, cut) : null;
    const stemName = waveName().replace(/\.[^.]+$/, "");
    let path: string | null = null;
    if (as || !savesInPlace(f)) {
      const why = !as && !savesInPlace(f) ? `Save ${f.format || "this file"} as WAV, FLAC, MP3 or AAC` : "Save Audio As";
      path = await exportPickPath(stemName, folder, why, kindOfPath(f.path) ?? "wav", true);
      if (!path) return false;
      if (kindOfPath(path) === null) path = `${path}.wav`;
      const lossyKind = kindOfPath(path);
      if (lossyKind === "mp3" || lossyKind === "m4a") {
        const name = lossyKind === "mp3" ? "MP3" : "AAC";
        const ok = await ask(`Save as ${name}?`, `${name} is lossy: fine for publishing, but keep a WAV or FLAC copy if you'll edit it again.`, [
          { label: `Save ${name}`, value: "yes", primary: true },
          { label: "Cancel", value: "no" },
        ]);
        if (ok !== "yes") return false;
      }
    } else if (f.format.toUpperCase() === "MP3") {
      const ok = await ask(
        "Re-encode the MP3?",
        `Saving ${waveName()} encodes it as MP3 again, which loses a little quality each time. Use Save Audio As and pick WAV or FLAC to keep full quality.`,
        [
          { label: "Save MP3", value: "yes", primary: true },
          { label: "Cancel", value: "no" },
        ],
      );
      if (ok !== "yes") return false;
    }
    if (path === null && session.current?.media.some((m) => m.path === f.path)) {
      const ok = await ask(
        "Save over the original?",
        `Clips in the session play ${waveName()}, so saving changes them too. To keep them as they are, cancel and use Clip > Make Unique Copy first, or use Save As.`,
        [
          { label: "Save", value: "yes", primary: true },
          { label: "Cancel", value: "no" },
        ],
      );
      if (ok !== "yes") return false;
    }
    // Save writes the Voice Chain into the file, unless the chain is bypassed.
    // One undo step takes the applied chain back out.
    const pending = chainOf(wave.current?.tracks[0]?.effects);
    if (pending.enabled && pending.slots.some((x) => x.enabled)) {
      try {
        await applyWaveChain(pending, true);
      } catch (e) {
        showError(e);
        return false;
      }
    }
    exporting = { title: "Saving audio", progress: 0 };
    let off: (() => void) | null = null;
    try {
      off = await listen<{ done: number; total: number }>("export-progress", (e) => {
        if (exporting) exporting = { ...exporting, progress: e.payload.total > 0 ? e.payload.done / e.payload.total : 0 };
      });
      const markerCount = wave.current?.markers.length ?? 0;
      await saveWaveFile(path);
      const saved = waveFile.current()?.path ?? f.path;
      peaks.invalidate(saved);
      await syncSessionMedia(saved);
      const keepsMarkers = /\.wave?$/i.test(saved);
      showNotice(
        markerCount && !keepsMarkers
          ? `Saved ${saved.split(/[\\/]/).pop()}. Its markers stay only while it is open: only WAV files store them.`
          : `Saved ${saved.split(/[\\/]/).pop()}.`,
      );
      return true;
    } catch (e) {
      showError(e);
      return false;
    } finally {
      off?.();
      exporting = null;
      actions.invalidate();
    }
  }

  /**
   * After the Waveform editor saves a file the session also uses, the
   * session learns its new length (for example after recording more in the
   * Waveform editor), and clips that ran to the old end run to the new one.
   */
  async function syncSessionMedia(path: string) {
    const doc = session.current;
    if (!doc) return;
    const key = (p: string) => p.replace(/\\/g, "/").toLowerCase();
    const used = doc.media.filter((m) => key(m.path) === key(path));
    if (!used.length) return;
    try {
      const info = await probeMedia(path);
      for (const m of used) {
        if (m.length === info.length && m.sample_rate === info.sampleRate && m.channels === info.channels) continue;
        await session.edit(
          "Update Changed File",
          { op: "set_media", media: { ...m, length: info.length, sample_rate: info.sampleRate, channels: info.channels } },
          sessionHistory,
        );
      }
    } catch (e) {
      showError(e);
    }
  }

  /** Selected clips moved one track up (-1) or down (1), or null when they can't all move. */
  function clipsTrackMove(dir: -1 | 1): ClipChange[] | null {
    const d = session.current;
    const sel = editor.current().selectedClips;
    if (!d || sel.size === 0) return null;
    const changes: ClipChange[] = [];
    // Hidden tracks are skipped, the same as the Editor lists them.
    const listed = d.tracks.filter((t) => !t.hidden);
    for (let i = 0; i < listed.length; i++) {
      for (const c of listed[i]!.clips) {
        if (!sel.has(c.id)) continue;
        const to = listed[i + dir];
        if (!to || c.locked) return null;
        changes.push({ id: c.id, track_id: to.id });
      }
    }
    return changes.length ? changes : null;
  }
  async function moveClipsToTrack(dir: -1 | 1) {
    const changes = clipsTrackMove(dir);
    const d = session.current;
    if (!changes || !d) return;
    await session.edit(dir < 0 ? "Move to Track Above" : "Move to Track Below", { op: "set_clips", changes }, sessionHistory);
    const listed = d.tracks.filter((t) => !t.hidden);
    const first = listed.findIndex((t) => t.clips.some((c) => c.id === changes[0]!.id));
    const target = listed[first + dir];
    if (target) editor.selectClips(editor.current().selectedClips, target.id);
  }

  /** "30", "30.5", "0:30.5" or "1:02:03.5" as seconds; null when it is not a time. */
  function parseSeconds(text: string): number | null {
    const parts = text.trim().split(":").map((p) => p.trim());
    if (parts.length === 0 || parts.length > 3 || parts.some((p) => p === "" || !/^\d*\.?\d+$/.test(p))) return null;
    return parts.reduce((acc, p) => acc * 60 + Number(p), 0);
  }

  /** Effects > Pitch and Time > Stretch (Waveform editor): the selection, or the whole file, to a new length. */
  async function waveReverseOrInvert(mode: "reverse" | "invert") {
    const d = wave.current;
    const r = waveAll();
    const t = d?.tracks[0];
    if (!d || !r || !t) return;
    const label = mode === "reverse" ? "Reverse" : "Invert";
    try {
      const res = await waveReverseInvert(r[0], r[1], mode);
      if (wave.current?.tracks[0]?.id !== t.id) return showNotice("You switched files while that was running, so it was not applied.");
      await waveEdit(label, replaceRange(wave.current ?? d, res.start, res.end, res.media, label, uuid));
    } catch (e) {
      showError(e);
    }
  }

  async function waveStretchTo() {
    const d = wave.current;
    const r = waveAll();
    if (!d || !r) return;
    const rate = d.sample_rate;
    const now = (r[1] - r[0]) / rate;
    const what = waveSel() ? "the selection" : "the whole file";
    const answer = await promptText("Stretch", `New length for ${what}, now ${now.toFixed(3)} s. Seconds, or minutes:seconds. The pitch stays the same.`, now.toFixed(3));
    if (answer === null) return;
    const secs = parseSeconds(answer);
    if (secs === null || secs <= 0) return showNotice("Type a length like 30, 14.99 or 1:02.5.");
    const frames = Math.round(secs * rate);
    if (frames === r[1] - r[0]) return;
    try {
      const res = await waveStretch(r[0], r[1], frames);
      const label = `Stretch to ${secs.toFixed(3)} s`;
      const op = stretchRange(wave.current ?? d, res.start, res.end, res.media, label, uuid);
      if (op) await waveEdit(label, op);
      showNotice(`${label}: ${now.toFixed(3)} s became ${secs.toFixed(3)} s (${((secs / now) * 100).toFixed(1)}%). Changes past about 25% can sound processed.`);
    } catch (e) {
      showError(e);
    }
  }

  async function waveAmplify(db: number) {
    const d = wave.current;
    const r = waveAll();
    if (!d || !r || !db) return;
    await waveEdit(`Amplify ${db > 0 ? "+" : ""}${db.toFixed(1)} dB`, amplifyRange(d, r[0], r[1], db, uuid));
  }

  /** Writes the Waveform preview chain into the selection (or the whole file), then clears the preview. */
  async function applyWaveChain(chain: EffectChainDoc, wholeFile = false) {
    const d = wave.current;
    const r = wholeFile && d ? clampRange(d, 0, sessionEnd(d)) : waveAll();
    const t = d?.tracks[0];
    if (!d || !r || !t) return;
    const label = !wholeFile && waveSel() ? "Apply Voice Chain to Selection" : "Apply Voice Chain";
    const res = await waveApplyChain(chain, r[0], r[1]);
    // The result belongs to the file the job started on. If another file is open now, do not apply it there.
    if (wave.current?.tracks[0]?.id !== t.id) {
      showNotice("You switched files while the effect was running, so it was not applied. Stay on the file until it finishes. To process many files, use Batch Convert.");
      return;
    }
    await waveEdit(label, replaceRange(wave.current ?? d, res.start, res.end, res.media, label, uuid));
    await wave.apply({ op: "set_effects", target: { kind: "track", id: t.id }, chain: chainOf(null) });
    if (!wholeFile) showNotice(`${label}: done. Undo takes it back.`);
  }

  /** Spectral edit on the current spectral selection: turn it down by `db` (-120 removes it), or repair it. */
  async function waveSpectral(mode: "gain" | "repair", db: number, label: string) {
    const shapes = get(spectralSelection);
    const d = wave.current;
    const mask = shapes ? rasterize(shapes) : null;
    // A Spot Repair stroke is used once, not kept as a selection.
    if (editor.current().spectralTool === "spot") spectralSelection.set(null);
    if (!mask || !d) return;
    try {
      const r = await waveSpectralEdit(mask, mode, db);
      await waveEdit(label, replaceRange(wave.current ?? d, r.start, r.end, r.media, label, uuid));
    } catch (e) {
      showError(e);
    }
  }

  // Edit > Settings.
  const KEY_SETTINGS = "app.settings";
  let settingsDraft = $state<SettingsDraft | null>(null);
  function openSettings() {
    const ed = editor.current();
    settingsDraft = {
      app: structuredClone(settings.current),
      editor: { followPlayhead: ed.followPlayhead, autoCrossfade: ed.autoCrossfade, timeFormat: ed.timeFormat },
      record: recordOptions(),
      loudness: { region: prefs.string(KEY_LOUD_REGION, DEFAULT_REGION), target: prefs.string(KEY_LOUD_TARGET, DEFAULT_TARGET) },
    };
  }
  function applySettingsDraft(d: SettingsDraft) {
    settingsDraft = null;
    const autosaveChanged = d.app.autosave !== settings.current.autosave || d.app.autosaveMinutes !== settings.current.autosaveMinutes;
    settings.set(d.app);
    prefs.set(KEY_SETTINGS, settings.current);
    editor.setFollowPlayhead(d.editor.followPlayhead);
    editor.setAutoCrossfade(d.editor.autoCrossfade);
    editor.setTimeFormat(d.editor.timeFormat);
    prefs.set(KEY_RECORD, d.record);
    prefs.set(KEY_LOUD_REGION, d.loudness.region);
    prefs.set(KEY_LOUD_TARGET, d.loudness.target);
    applyAppSettings(settings.current, autosaveChanged);
  }
  /** Puts settings that live outside the interface's own reads into effect. */
  function applyAppSettings(s: AppSettings, autosave: boolean) {
    const root = document.documentElement.style;
    for (const k of ["--wave", "--playhead", "--accent", "--lane-selected"]) root.removeProperty(k);
    for (const [k, v] of Object.entries(colorVars(s.colors))) root.setProperty(k, v);
    root.setProperty("--meter-yellow", `${meterPercent(s.meterYellowDb)}%`);
    root.setProperty("--meter-red", `${meterPercent(s.meterRedDb)}%`);
    setCustomTarget({ lufs: s.customLufs, peak: s.customPeak });
    setMeterSpeed(s.meterSpeed);
    if (isTauri()) {
      // Driver first, so the output name is looked up on the right one.
      void setAudioDriver(s.driver === "asio")
        .then(() => setAudioOutput(s.output))
        .catch(showError);
    }
    if (autosave) restartAutosave();
  }
  function restartAutosave() {
    files.stopAutosave();
    if (settings.current.autosave) files.startAutosave(settings.current.autosaveMinutes * 60_000);
  }
  /** Space bar: stops when recording or playing, otherwise plays (the multitrack, or the open Waveform file). */
  function spaceToggle(t: TransportStore): Promise<void> {
    if (take) return finishRecord();
    return t.current.playing ? t.stop() : t.toggle();
  }

  // Recording. Multitrack: every armed track records its input into
  // its own file, from the playhead, while the session plays. Waveform
  // editor: the take goes into the file at the playhead.
  /** Loudest input peak seen during the current recording (0 to 1). */
  let recPeakMax = 0;
  let take: { mode: "multitrack" | "wave"; at: number; plan?: RecordPlan; stopAfter?: number | null } | null = null;
  /** While recording, playback keeps going past the end of the old audio. */
  function holdEnd(on: boolean) {
    transport.holdAtEnd = on;
    waveTransport.holdAtEnd = on;
  }
  const KEY_RECORD = "record.options";
  const recordOptions = (): RecordOptions => sanitizeRecordOptions(prefs.raw(KEY_RECORD));
  let recordOptionsOpen = $state(false);
  let recPoll: ReturnType<typeof setInterval> | undefined;

  function stamp(): string {
    const d = new Date();
    const p = (n: number) => String(n).padStart(2, "0");
    return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}${p(d.getMinutes())}${p(d.getSeconds())}`;
  }

  // Settings > Playback and Recording: a count before recording starts.
  let countdown = $state<number | null>(null);
  async function countIn(): Promise<boolean> {
    const secs = settings.current.countdownSec;
    if (secs <= 0) return true;
    for (let n = secs; n > 0; n--) {
      countdown = n;
      await new Promise((r) => setTimeout(r, 1000));
      if (countdown === null) return false;
    }
    countdown = null;
    return true;
  }

  async function toggleRecord() {
    if (countdown !== null) {
      // Pressing Record again during the count cancels it.
      countdown = null;
      return;
    }
    if (take) {
      await finishRecord();
      return;
    }
    if (!(await countIn())) return;
    try {
      if (inWave()) {
        const d = wave.current;
        const f = waveFile.current();
        if (!d || !f) {
          showNotice("Open or create a file in the Waveform editor first.");
          return;
        }
        await waveTransport.stop();
        const base = f.path.split(/[\\/]/).pop()?.replace(/\.[^.]+$/, "") ?? "Take";
        const channels = Math.min(2, Math.max(1, d.media[0]?.channels ?? 1));
        const at = wavePlayhead();
        await recordStart([{ key: "wave", input: settings.current.waveInput, channels, name: `${base} take ${stamp()}` }]);
        take = { mode: "wave", at };
        liveTakes.set({ at, rate: 0, waves: {}, tick: 0 });
        holdEnd(true);
      } else {
        const doc = session.current;
        if (!doc) return;
        let armed = doc.tracks.filter((t) => t.record_armed);
        if (armed.length === 0) {
          const sel = doc.tracks.find((t) => t.id === editor.current().selectedTrack);
          if (!sel) {
            showNotice("Arm a track first: click R on the track you want to record.");
            return;
          }
          await session.edit("Arm Track", { op: "set_track", id: sel.id, changes: { record_armed: true } }, sessionHistory);
          armed = [sel];
        }
        const targets: RecordTarget[] = armed.map((t) => ({
          key: t.id,
          input: t.input ?? null,
          channels: t.channels === "mono" ? 1 : 2,
          name: `${t.name} ${stamp()}`,
          monitor: !!t.monitor_input,
        }));
        const plan = recordPlan(playheadNow(), timeSel(), recordOptions(), doc.sample_rate);
        const at = plan.at;
        await transport.stop();
        await recordStart(targets);
        take = { mode: "multitrack", at, plan, stopAfter: punchStopAfter(plan, doc.sample_rate) };
        holdEnd(true);
        if (plan.punch) showNotice(`Punch-in: recording stops at the end of the time selection.`);
        liveTakes.set({ at, rate: 0, waves: {}, tick: 0 });
        void transport.play(at);
      }
      recPeakMax = 0;
      recording.set({ active: true, seconds: 0, peak: 0 });
      startInputPoll();
    } catch (e) {
      take = null;
      holdEnd(false);
      liveTakes.set(null);
      showError(e);
    }
  }

  // Input meters: armed tracks' inputs stay open for their meters while
  // not recording, and the same poll feeds the live waveform while recording.
  let lastMonitorNote = "";
  function startInputPoll() {
    if (recPoll) return;
    recPoll = setInterval(async () => {
      const st = await recordStatus().catch(() => null);
      if (!st || (!st.recording && !st.monitoring)) {
        inputLevels.set({});
        return;
      }
      if (st.recording) {
        recPeakMax = Math.max(recPeakMax, st.peak);
        recording.set({ active: true, seconds: st.seconds, peak: st.peak });
      }
      if (st.monitorNote && st.monitorNote !== lastMonitorNote) showNotice(st.monitorNote);
      lastMonitorNote = st.monitorNote ?? "";
      // A punch-in stops by itself at the end of the selection.
      if (st.recording && take?.stopAfter != null && st.seconds >= take.stopAfter) {
        take = { ...take, stopAfter: null };
        void finishRecord();
        return;
      }
      inputLevels.update((old) => {
        const next: Record<string, number> = {};
        for (const k of new Set([...Object.keys(old), ...Object.keys(st.live)])) {
          const fall = (old[k] ?? 0) * 0.8;
          next[k] = Math.max(st.live[k]?.peak ?? 0, fall < 0.002 ? 0 : fall);
        }
        return next;
      });
      if (st.recording) {
        liveTakes.update((lt) => {
          if (!lt) return lt;
          const waves = { ...lt.waves };
          for (const [k, v] of Object.entries(st.live)) {
            if (!v.blocks.length) continue;
            (waves[k] ??= new LiveWave()).push(v.channels, v.blocks, st.rate);
          }
          return { ...lt, rate: st.rate, waves, tick: lt.tick + 1 };
        });
      }
    }, 80);
  }

  function stopInputPoll() {
    clearInterval(recPoll);
    recPoll = undefined;
    inputLevels.set({});
  }

  /** Opens or closes the armed tracks' inputs for their meters. */
  let monitorKey = "";
  function syncMonitor(force = false) {
    if (take) return;
    const armed = (session.current?.tracks ?? []).filter((t) => t.record_armed);
    const key = armed.map((t) => `${t.id}:${t.input ?? ""}:${t.channels}:${t.monitor_input ? 1 : 0}`).join("|");
    if (!force && key === monitorKey) return;
    monitorKey = key;
    const targets: RecordTarget[] = armed.map((t) => ({ key: t.id, input: t.input ?? null, channels: t.channels === "mono" ? 1 : 2, name: t.name, monitor: !!t.monitor_input }));
    void recordMonitor(targets).catch(() => {});
    if (targets.length) startInputPoll();
    else stopInputPoll();
  }

  /** Stops recording and places the take. The live waveform stays until the finished clip is in, so nothing is redrawn in between. */
  async function finishRecord() {
    takePaths = [];
    try {
      await finishRecordInner();
      // Keep drawing the live waveform until the finished take's own drawing is ready (up to 4 seconds).
      for (let i = 0; i < 40 && takePaths.some((p) => peaks.get(p).status === "building"); i++) await new Promise((r) => setTimeout(r, 100));
    } finally {
      liveTakes.set(null);
    }
  }
  let takePaths: string[] = [];

  async function finishRecordInner() {
    const t = take;
    take = null;
    holdEnd(false);
    stopInputPoll();
    recording.set({ active: false, seconds: 0, peak: 0 });
    let takes;
    try {
      takes = await recordStop();
    } catch (e) {
      showError(e);
      syncMonitor(true);
      return;
    } finally {
      // Back to metering the armed inputs.
      setTimeout(() => syncMonitor(true), 0);
    }
    if (!t) return;
    const silent = recPeakMax < 0.0005; // about -66 dBFS: nothing is arriving
    if (t.mode === "multitrack") await transport.stop();
    const usable = takes.filter((k) => k.length > 0);
    takePaths = usable.map((k) => k.path);
    if (usable.length === 0) {
      showNotice("Nothing was recorded.");
      return;
    }
    if (silent && usable[0]!.length / Math.max(1, usable[0]!.sample_rate) >= 1) {
      showNotice(
        "No sound arrived while recording. On Mac, check System Settings > Privacy & Security > Microphone. On Windows, check Settings > Privacy > Microphone. Also check the input choice and its cable or gain.",
      );
    }
    const media = (k: (typeof usable)[number]) => ({
      id: uuid(),
      path: k.path,
      relative_path: null,
      sample_rate: k.sample_rate,
      channels: k.channels,
      length: k.length,
    });
    if (t.mode === "wave") {
      const d = wave.current;
      if (!d) return;
      const r = insertFile(d, t.at, media(usable[0]!), "Take", uuid);
      if (r && (await waveEdit("Record", r.op))) waveSetSelection([t.at, t.at + r.length], t.at);
      return;
    }
    const ops: Op[] = [];
    const doc = session.current;
    const opts = recordOptions();
    const plan: RecordPlan = t.plan ?? { at: t.at, punch: null };
    // A punch-in replaces what was in its range on each recorded track.
    if (doc && plan.punch && opts.replace) {
      try {
        const { ops: cut, inside } = isolate(doc, usable.map((k) => k.key), plan.punch.start, plan.punch.end, uuid);
        ops.push(...cut);
        if (inside.length) ops.push({ op: "remove_clips", ids: inside });
      } catch (e) {
        if (e instanceof LockedError) showNotice("A locked clip is under the punch-in, so the new take was laid on top of it.");
        else throw e;
      }
    }
    for (const k of usable) {
      const placed = placeTake(plan, { length: k.length, sampleRate: k.sample_rate }, doc?.sample_rate ?? k.sample_rate, opts.offsetMs);
      if (!placed) continue;
      const m = media(k);
      ops.push({ op: "add_media", media: m });
      const name = k.path.split(/[\\/]/).pop()?.replace(/\.wav$/i, "") ?? "Take";
      ops.push({ op: "add_clips", track_id: k.key, clips: [{ id: uuid(), name, media_id: m.id, start: placed.start, source_in: placed.sourceIn, source_out: placed.sourceOut }] });
    }
    if (!ops.some((o) => o.op === "add_clips")) {
      showNotice("The take was shorter than the recording offset, so nothing was added.");
      return;
    }
    try {
      await session.edit(usable.length === 1 ? "Record" : `Record ${usable.length} Tracks`, { op: "batch", ops }, sessionHistory);
      // The takes show in the Files panel too.
      await mediaPool.importPaths(
        usable.map((k) => k.path),
        sessionHistory,
      );
    } catch (e) {
      showError(e);
    }
  }
  recordControl.toggle = () => void toggleRecord();

  async function waveNormalize() {
    const r = waveAll();
    if (!r) return;
    const v = await promptText("Normalize", "Peak level in dB (for example -1.0)", "-1.0");
    if (v === null) return;
    const target = Number(v);
    if (!Number.isFinite(target) || target > 0) {
      showNotice("Enter a level of 0 dB or below, for example -1.0.");
      return;
    }
    const peak = await wavePeak(r);
    if (peak <= 0) {
      showNotice("The selection is silent, so there is nothing to normalize.");
      return;
    }
    const db = Math.round((target - 20 * Math.log10(peak)) * 100) / 100;
    if (Math.abs(db) < 0.01) {
      showNotice(`Already peaks at ${target.toFixed(1)} dB.`);
      return;
    }
    await waveEdit(`Normalize to ${target.toFixed(1)} dB`, amplifyRange(wave.current!, r[0], r[1], db, uuid));
  }

  // File > New Audio File.
  let newAudio = $state<NewAudioSettings | null>(null);
  let newAudioCount = 0;
  const KEY_NEW_AUDIO = "newAudio.defaults";
  const KEY_NEW_SESSION = "newSession.defaults";
  /** Last-used settings from prefs, kept only when each field has the right type. */
  function lastUsed<T extends Record<string, unknown>>(key: string, base: T): T {
    const raw = prefs.raw(key);
    if (!raw || typeof raw !== "object") return base;
    const out = { ...base };
    for (const k of Object.keys(base) as (keyof T)[]) {
      const v = (raw as Record<string, unknown>)[k as string];
      if (typeof v === typeof base[k] || (base[k] === null && (typeof v === "string" || v === null))) out[k] = v as T[keyof T];
    }
    return out;
  }

  function askNewAudio() {
    const d = session.current;
    const last = lastUsed(KEY_NEW_AUDIO, { sampleRate: d?.sample_rate ?? 48_000, channels: 2, bitDepth: d?.bit_depth ?? 32 });
    newAudio = { name: `Untitled ${newAudioCount + 1}`, sampleRate: last.sampleRate, channels: last.channels === 1 ? 1 : 2, bitDepth: last.bitDepth };
  }
  async function makeNewAudio(v: NewAudioSettings) {
    prefs.set(KEY_NEW_AUDIO, { sampleRate: v.sampleRate, channels: v.channels, bitDepth: v.bitDepth });
    try {
      await newEmptyWaveFile(v.name, v.sampleRate, v.channels, v.bitDepth);
      newAudioCount += 1;
    } catch (e) {
      showError(e);
      return;
    }
    await transport.stop();
    editor.setMode("waveform");
    dock.show("editor");
    actions.invalidate();
  }

  // File > New Session.
  let newSessionReq = $state<{ initial: NewSessionSettings; templates: TemplateEntry[]; error?: string } | null>(null);
  let newSessionCount = 0;
  async function askNewSession() {
    // Each lookup on its own, so one failing never hides the other.
    const templates = await templateList().catch((e: unknown): TemplateEntry[] => (showError(e), []));
    const folder = await sessionDefaultFolder().catch(() => "");
    const last = lastUsed(KEY_NEW_SESSION, { folder, template: null as string | null, sampleRate: 48_000, bitDepth: 32, mono: false });
    newSessionCount += 1;
    // The next name not taken in the folder it will be saved in (Untitled Session 2 when 1 exists).
    const where = last.folder || folder;
    const name = where ? await sessionNextName(where, "Untitled Session").catch(() => `Untitled Session ${newSessionCount}`) : `Untitled Session ${newSessionCount}`;
    newSessionReq = { initial: { name, ...last }, templates };
  }
  async function makeNewSession(v: NewSessionSettings) {
    prefs.set(KEY_NEW_SESSION, { folder: v.folder, template: v.template, sampleRate: v.sampleRate, bitDepth: v.bitDepth, mono: v.mono });
    try {
      await files.newSession({
        name: v.name,
        sample_rate: v.sampleRate,
        bit_depth: v.bitDepth,
        mono: v.mono,
        template: v.template,
        folder: v.folder || null,
      });
    } catch (e) {
      // A name or folder problem: back to the dialog with what was typed.
      const message = e instanceof Error ? e.message : String(e);
      const templates = await templateList().catch((): TemplateEntry[] => []);
      newSessionReq = { initial: v, templates, error: message };
      return;
    }
    if (inWave()) editor.setMode("multitrack");
    await actions.run("view.zoom-session");
  }
  async function deleteTemplate(t: TemplateEntry): Promise<boolean> {
    const ok = await ask("Delete template", `Delete the template "${t.name}"? Sessions made from it are not affected.`, [
      { label: "Delete", value: "yes", primary: true },
      { label: "Cancel", value: "no" },
    ]);
    if (ok !== "yes") return false;
    try {
      await templateDelete(t.path);
    } catch (e) {
      showError(e);
      return false;
    }
    if (newSessionReq) newSessionReq = { ...newSessionReq, templates: newSessionReq.templates.filter((x) => x.path !== t.path) };
    return true;
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

  /** Paste to New File: the Waveform clipboard in the Waveform editor, otherwise the copied clips. */
  async function pasteToNewFile() {
    try {
      if (inWave() && waveBoard) {
        await newWaveFromClips(waveBoard.rate, waveBoard.clips, waveBoard.media);
      } else if (clipBoard && session.current) {
        const doc = session.current;
        const clips = clipBoard.items.map((i) => ({ ...i.clip, id: uuid() }));
        const used = new Set(clips.map((c) => c.media_id));
        await newWaveFromClips(doc.sample_rate, clips, doc.media.filter((m) => used.has(m.id)));
      } else return;
    } catch (e) {
      showError(e);
      return;
    }
    await transport.stop();
    editor.setMode("waveform");
    dock.show("editor");
    actions.invalidate();
  }

  function registerWaveActions(): (() => void)[] {
    const ov = actions.overlay.bind(actions);
    const NOT_WAVE = "Works in the Waveform editor. Double-click a file in the Files panel to open it there.";
    return [
      actions.register({
        id: "view.editor.multitrack",
        run: () => {
          void waveTransport.stop();
          editor.setMode("multitrack");
          dock.show("editor");
        },
        checked: () => !inWave(),
      }),
      actions.register({ id: "file.new-audio", run: () => askNewAudio() }),
      actions.register({
        id: "view.editor.waveform",
        run: () => {
          // With no file open, switching to the Waveform editor starts a new one.
          if (!waveFile.current()) {
            askNewAudio();
            return;
          }
          void transport.stop();
          editor.setMode("waveform");
          dock.show("editor");
        },
        checked: () => inWave(),
      }),
      ...(["move", "razor", "slip", "time"] as const).map((t) =>
        actions.register({
          id: `tool.${t}`,
          run: () => {
            if (inWave() && t === "time") {
              editor.setSpectralTool("time");
              return;
            }
            if (inWave()) editor.setMode("multitrack");
            editor.setTool(t);
          },
          checked: () => (inWave() ? t === "time" && editor.current().spectralTool === "time" : editor.current().tool === t),
        }),
      ),
      // Spectral tools work on the Waveform editor's spectrum.
      ...(["box", "lasso", "brush", "spot"] as const).map((t) =>
        actions.register({
          id: `tool.${t}`,
          run: () => {
            const e = editor.current();
            if (!e.showSpectrum && !e.showPitch) editor.setShowSpectrum(true);
            editor.setSpectralTool(t);
          },
          enabled: () => waveIsOpen(),
          reason: () => "Open a file in the Waveform editor to use the spectral tools.",
          checked: () => inWave() && editor.current().spectralTool === t,
        }),
      ),
      actions.register({
        id: "view.pitch",
        run: () => editor.setShowPitch(!editor.current().showPitch),
        label: () => (editor.current().showPitch ? "Hide Pitch" : "Show Pitch"),
        checked: () => editor.current().showPitch,
      }),
      actions.register({
        id: "clip.edit-source",
        run: () => {
          const c = selectedClipDocs()[0];
          const m = c ? session.current?.media.find((x) => x.id === c.media_id) : undefined;
          if (m) return openInWaveform(m.path);
        },
        enabled: () => selectedClipDocs().length === 1,
        reason: () => "Select one clip.",
      }),
      // Waveform-only commands: disabled with a reason elsewhere.
      actions.register({
        id: "edit.paste-new",
        run: () => pasteToNewFile(),
        enabled: () => (inWave() ? waveBoard !== null : clipBoard !== null),
        reason: () => "Copy audio in the Waveform editor, or clips in the Multitrack, first.",
      }),
      actions.register({
        id: "view.spectrum",
        run: () => editor.setShowSpectrum(!editor.current().showSpectrum),
        label: () => (editor.current().showSpectrum ? "Hide Spectrum" : "Show Spectrum"),
        checked: () => editor.current().showSpectrum,
      }),
      ...["fx.spectrum.reduce-12", "fx.spectrum.reduce-24", "fx.spectrum.remove", "fx.spectrum.repair"].map((id) =>
        actions.register({ id, run: () => {}, enabled: () => false, reason: () => NOT_WAVE }),
      ),
      ...["edit.mix-paste", "edit.crop", "fx.invert", "fx.reverse", "fx.silence", "fx.fade-in", "fx.fade-out", "fx.amplify", "fx.normalize"].map((id) =>
        actions.register({ id, run: () => {}, enabled: () => false, reason: () => NOT_WAVE }),
      ),
      ov(waveFocus, {
        id: "edit.undo",
        run: () => waveHistory.undo().then(() => undefined),
        enabled: () => waveHistory.current.canUndo,
        label: () => (waveHistory.current.undoLabel ? `Undo ${waveHistory.current.undoLabel}` : null),
      }),
      ov(waveFocus, {
        id: "edit.redo",
        run: () => waveHistory.redo().then(() => undefined),
        enabled: () => waveHistory.current.canRedo,
        label: () => (waveHistory.current.redoLabel ? `Redo ${waveHistory.current.redoLabel}` : null),
      }),
      ov(waveFocus, {
        id: "edit.select-all",
        run: () => {
          const d = wave.current;
          if (d) waveSetSelection([0, sessionEnd(d)]);
        },
      }),
      ov(waveFocus, { id: "edit.deselect", run: () => waveSetSelection(null), enabled: () => waveSel() !== null, reason: () => "Nothing is selected." }),
      ov(waveFocus, {
        id: "edit.delete",
        run: async () => {
          const r = waveSel();
          if (r && (await waveEdit("Delete", deleteRange(wave.current!, r[0], r[1], uuid)))) waveSetSelection(null, r[0]);
        },
        enabled: () => waveSel() !== null,
        label: () => "Delete Selection",
        reason: () => NEED_WAVE_SEL,
      }),
      ov(waveFocus, {
        id: "edit.copy",
        run: () => {
          const r = waveSel();
          if (r) waveBoard = copyRange(wave.current!, r[0], r[1]);
          actions.invalidate();
        },
        enabled: () => waveSel() !== null,
        reason: () => NEED_WAVE_SEL,
      }),
      ov(waveFocus, {
        id: "edit.cut",
        run: async () => {
          const r = waveSel();
          if (!r) return;
          waveBoard = copyRange(wave.current!, r[0], r[1]);
          if (await waveEdit("Cut", deleteRange(wave.current!, r[0], r[1], uuid))) waveSetSelection(null, r[0]);
        },
        enabled: () => waveSel() !== null,
        reason: () => NEED_WAVE_SEL,
      }),
      ov(waveFocus, {
        id: "edit.paste",
        run: async () => {
          if (!waveBoard) return;
          // Pasting over a selection replaces it.
          const r = waveSel();
          if (r) await waveEdit("Delete", deleteRange(wave.current!, r[0], r[1], uuid));
          const at = r ? r[0] : wavePlayhead();
          const p = pasteAt(wave.current!, waveBoard, at, uuid);
          if (p && (await waveEdit("Paste", p.op))) waveSetSelection([at, at + p.length], at);
        },
        enabled: () => waveBoard !== null,
        reason: () => "Copy or cut audio in the Waveform editor first.",
      }),
      ov(waveFocus, {
        id: "edit.mix-paste",
        run: async () => {
          if (!waveBoard) return;
          const v = await promptText("Mix Paste", "Level of the pasted audio (dB)", "0.0");
          if (v === null) return;
          const db = Number(v);
          if (!Number.isFinite(db) || Math.abs(db) > 48) {
            showNotice("Enter a number of decibels between -48 and 48.");
            return;
          }
          const at = waveSel()?.[0] ?? wavePlayhead();
          const p = mixPasteAt(wave.current!, waveBoard, at, db, uuid);
          if (p && (await waveEdit("Mix Paste", p.op))) waveSetSelection([at, at + p.length], at);
        },
        enabled: () => waveBoard !== null,
        reason: () => "Copy or cut audio in the Waveform editor first.",
      }),
      ov(waveFocus, {
        id: "edit.crop",
        run: async () => {
          const r = waveSel();
          if (r && (await waveEdit("Crop", cropTo(wave.current!, r[0], r[1], uuid)))) waveSetSelection([0, r[1] - r[0]], 0);
        },
        enabled: () => waveSel() !== null,
        reason: () => NEED_WAVE_SEL,
      }),
      ov(waveIsOpen, {
        id: "fx.silence",
        run: async () => {
          const r = waveSel();
          if (r) await waveEdit("Silence", silenceRange(wave.current!, r[0], r[1], uuid));
        },
        enabled: () => waveSel() !== null,
        reason: () => NEED_WAVE_SEL,
      }),
      ov(waveIsOpen, {
        id: "fx.fade-in",
        run: async () => {
          const r = waveSel();
          if (r) await waveEdit("Fade In", fadeRange(wave.current!, r[0], r[1], "in", uuid));
        },
        enabled: () => waveSel() !== null,
        reason: () => NEED_WAVE_SEL,
      }),
      ov(waveIsOpen, {
        id: "fx.fade-out",
        run: async () => {
          const r = waveSel();
          if (r) await waveEdit("Fade Out", fadeRange(wave.current!, r[0], r[1], "out", uuid));
        },
        enabled: () => waveSel() !== null,
        reason: () => NEED_WAVE_SEL,
      }),
      ov(waveIsOpen, {
        id: "fx.amplify",
        run: async () => {
          const v = await promptText("Amplify", waveSel() ? "Change the selection by (dB)" : "Change the whole file by (dB)", "3.0");
          if (v === null) return;
          const db = Number(v);
          if (!Number.isFinite(db) || Math.abs(db) > 48) {
            showNotice("Enter a number of decibels between -48 and 48.");
            return;
          }
          await waveAmplify(db);
        },
      }),
      ov(waveIsOpen, { id: "fx.normalize", run: () => waveNormalize() }),
      ov(waveIsOpen, { id: "fx.invert", run: () => waveReverseOrInvert("invert") }),
      ov(waveIsOpen, { id: "fx.reverse", run: () => waveReverseOrInvert("reverse") }),
      ov(waveIsOpen, { id: "clip.stretch", run: () => waveStretchTo(), label: () => "Stretch Selection or File..." }),
      ...(
        [
          ["fx.spectrum.reduce-12", "gain", -12, "Spectral Reduce 12 dB"],
          ["fx.spectrum.reduce-24", "gain", -24, "Spectral Reduce 24 dB"],
          ["fx.spectrum.remove", "gain", -120, "Spectral Remove"],
          ["fx.spectrum.repair", "repair", 0, "Spot Repair"],
        ] as const
      ).map(([id, mode, db, label]) =>
        ov(waveIsOpen, {
          id,
          run: () => waveSpectral(mode, db, label),
          enabled: () => get(spectralSelection) !== null,
          reason: () => "Show the spectrum and drag a rectangle on it first.",
        }),
      ),
      ov(waveIsOpen, { id: "file.save", run: () => saveWave(false).then(() => undefined), label: () => "Save Audio" }),
      ov(waveIsOpen, { id: "file.save-as", run: () => saveWave(true).then(() => undefined), label: () => "Save Audio As..." }),
      ov(waveIsOpen, {
        id: "file.close",
        run: async () => {
          if (await confirmWaveDiscard()) await closeWaveFile();
        },
        label: () => "Close Audio File",
      }),
      ov(waveIsOpen, {
        id: "edit.marker.cue",
        run: async () => {
          const d = wave.current;
          if (!d) return;
          const m = newMarker(uuid(), nextMarkerName(d.markers, settings.current.markerName), wavePlayhead());
          await waveEdit("Add Marker", { op: "add_markers", markers: [m] });
        },
      }),
      ov(waveIsOpen, {
        id: "edit.marker.subclip",
        run: async () => {
          const d = wave.current;
          const r = waveSel();
          if (!d || !r) return;
          const m = newMarker(uuid(), nextMarkerName(d.markers, "Subclip"), r[0], r[1] - r[0], "subclip");
          await waveEdit("Add Subclip Marker", { op: "add_markers", markers: [m] });
        },
        enabled: () => waveSel() !== null,
        reason: () => NEED_WAVE_SEL,
      }),
      ov(waveIsOpen, {
        id: "edit.marker.cd-track",
        run: async () => {
          const d = wave.current;
          if (!d) return;
          const n = d.markers.filter((x) => x.kind === "cd_track").length + 1;
          await waveEdit("Add CD Track Marker", { op: "add_markers", markers: [newMarker(uuid(), `Track ${n}`, wavePlayhead(), 0, "cd_track")] });
        },
      }),
      ov(waveIsOpen, {
        id: "edit.marker.cart",
        run: async () => {
          const d = wave.current;
          if (!d) return;
          const r = waveSel();
          const m = r
            ? newMarker(uuid(), nextMarkerName(d.markers, "Cart"), r[0], r[1] - r[0], "cart_timer")
            : newMarker(uuid(), nextMarkerName(d.markers, "Cart"), wavePlayhead(), 0, "cart_timer");
          await waveEdit("Add Cart Timer Marker", { op: "add_markers", markers: [m] });
        },
      }),
      ov(waveIsOpen, {
        id: "edit.marker.range",
        run: async () => {
          const d = wave.current;
          const r = waveSel();
          if (!d || !r) return;
          const m = newMarker(uuid(), nextMarkerName(d.markers, "Range"), r[0], r[1] - r[0]);
          await waveEdit("Add Range Marker", { op: "add_markers", markers: [m] });
        },
        enabled: () => waveSel() !== null,
        reason: () => NEED_WAVE_SEL,
      }),
      ov(waveIsOpen, { id: "multitrack.play-pause", run: () => spaceToggle(waveTransport) }),
      ov(waveIsOpen, { id: "multitrack.stop", run: () => (take ? finishRecord() : waveTransport.stop()) }),
      ov(waveIsOpen, { id: "transport.go-start", run: () => waveTransport.seek(0) }),
      ov(waveIsOpen, { id: "view.zoom-in", run: () => waveZoom.run?.("in") }),
      ov(waveIsOpen, { id: "view.zoom-out", run: () => waveZoom.run?.("out") }),
      ov(waveIsOpen, { id: "view.zoom-session", run: () => waveZoom.run?.("full") }),
      ov(waveIsOpen, { id: "view.zoom-selection", run: () => waveZoom.run?.("selection"), enabled: () => waveSel() !== null }),
      waveHistory.subscribe(() => actions.invalidate()),
      waveFile.subscribe(() => actions.invalidate()),
      editor.subscribe(() => actions.invalidate()),
      wave.subscribe(() => actions.invalidate()),
    ];
  }

  function registerActions(): (() => void)[] {
    const offs = [
      actions.register({
        id: "edit.undo",
        run: () => sessionHistory.undo().then(() => undefined),
        enabled: () => sessionHistory.current.canUndo,
        label: () => (sessionHistory.current.undoLabel ? `Undo ${sessionHistory.current.undoLabel}` : null),
      }),
      actions.register({
        id: "edit.redo",
        run: () => sessionHistory.redo().then(() => undefined),
        enabled: () => sessionHistory.current.canRedo,
        label: () => (sessionHistory.current.redoLabel ? `Redo ${sessionHistory.current.redoLabel}` : null),
      }),
      // Unsaved-changes prompt arrives with session save (task 12).
      actions.register({ id: "file.exit", run: () => exitSafely() }),
      actions.register({ id: "window.reset-layout", run: () => dock.resetToDefault() }),
      actions.register({ id: "window.minimize", run: () => minimizeWindow() }),
      actions.register({ id: "edit.keyboard-shortcuts", run: () => void (shortcutsOpen = true) }),
      actions.register({ id: "edit.settings", run: () => openSettings() }),
      actions.register({ id: "help.shortcuts", run: () => void (shortcutsOpen = true) }),
      actions.register({ id: "help.website", run: () => openLink(LINKS.website) }),
      actions.register({ id: "help.book", run: () => openLink(LINKS.book) }),
      actions.register({ id: "help.discord", run: () => openLink(LINKS.discord) }),
      actions.register({ id: "help.github", run: () => openLink(LINKS.github) }),
      actions.register({ id: "help.report", run: () => openLink(LINKS.report) }),
      actions.register({ id: "help.about", run: () => void (aboutOpen = true) }),
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
        id: "edit.delete",
        run: () => areaOps()?.remove(),
        enabled: () => (areaOps()?.selected() ?? 0) > 0,
        label: () => areaOps()?.deleteLabel ?? null,
        reason: () => (areaOps() ? "Nothing is selected." : AREA_FIRST),
      }),
      actions.register({
        id: "edit.select-all",
        run: () => areaOps()?.selectAll(),
        enabled: () => (areaOps()?.total() ?? 0) > 0,
        reason: () => (areaOps() ? "Nothing to select." : AREA_FIRST),
      }),
      actions.register({
        id: "edit.deselect",
        run: () => areaOps()?.deselect(),
        enabled: () => (areaOps()?.selected() ?? 0) > 0,
        reason: () => (areaOps() ? "Nothing is selected." : AREA_FIRST),
      }),
      actions.register({
        id: "edit.find",
        run: () => {
          // The Editor has nothing to search yet, so Find goes to the Files panel.
          const area = currentArea() === "libraries" ? "libraries" : "files";
          dock.show(area);
          // The panel may have just been opened; wait a frame for it to render.
          requestAnimationFrame(() =>
            document.querySelector<HTMLInputElement>(`[data-area="${area}"] input[type="search"]`)?.focus(),
          );
        },
      }),
    ];
    const view = () => session.current?.view;
    const zoomBy = (factor: number) => {
      const v = view();
      if (!v) return;
      const w = editor.current().width;
      const z = zoomAround({ spp: v.samples_per_pixel, scroll: v.scroll }, factor, w / 2, maxSppFor(w, contentEnd(), session.current?.sample_rate ?? 48000));
      session.setView({ ...v, samples_per_pixel: z.spp, scroll: z.scroll });
    };
    /** Moves the playhead to the previous or next marker, or (not `markersOnly`) clip edge or selection edge. */
    const jumpEdge = (dir: -1 | 1, markersOnly: boolean) => {
      const doc = session.current;
      if (!doc) return Promise.resolve();
      const here = transport.current.position;
      const edges = new Set<number>(doc.markers.map((m) => m.position));
      if (!markersOnly) {
        const media = new Map(doc.media.map((m) => [m.id, m]));
        for (const t of doc.tracks)
          for (const c of t.clips) {
            edges.add(c.start);
            edges.add(c.start + clipLength(c, media.get(c.media_id), doc.sample_rate));
          }
        const sel = doc.view.selection;
        if (sel) {
          edges.add(sel.start);
          edges.add(sel.end);
        }
      }
      const slack = Math.max(1, Math.round(doc.sample_rate / 1000));
      const list = [...edges].filter((e) => (dir < 0 ? e < here - slack : e > here + slack));
      const to = list.length ? (dir < 0 ? Math.max(...list) : Math.min(...list)) : dir < 0 ? 0 : contentEnd();
      return transport.seek(to);
    };
    const contentEnd = () => {
      const doc = session.current;
      if (!doc) return 0;
      const media = new Map(doc.media.map((m) => [m.id, m]));
      let end = 0;
      for (const t of doc.tracks) for (const c of t.clips) end = Math.max(end, c.start + clipLength(c, media.get(c.media_id), doc.sample_rate));
      return end;
    };
    /** Where a "clips to ..." selection starts or ends: the chosen clips, or the playhead when none are chosen. */
    const clipSpan = () => {
      const doc = session.current;
      if (!doc) return null;
      const media = new Map(doc.media.map((m) => [m.id, m]));
      let first = Infinity;
      let last = -Infinity;
      let any = false;
      for (const t of doc.tracks)
        for (const c of t.clips) {
          if (!editor.current().selectedClips.has(c.id)) continue;
          any = true;
          first = Math.min(first, c.start);
          last = Math.max(last, c.start + clipLength(c, media.get(c.media_id), doc.sample_rate));
        }
      return any ? { first, last } : { first: playheadNow(), last: playheadNow() };
    };
    const selectClipActions = () => {
      const pick = (label: string, ids: string[], track: string | null) => {
        if (ids.length === 0) {
          showNotice(`${label}: no clips there.`);
          return;
        }
        editor.selectClips(ids, track);
      };
      return [
        actions.register({
          id: "edit.select-next-clip",
          run: () => {
            const t = selectedTrack();
            if (!t) return;
            const id = nextClipId(t.clips, editor.current().selectedClips, playheadNow());
            if (id) editor.selectClips([id], t.id);
            else showNotice("There is no next clip in this track.");
          },
          enabled: () => (selectedTrack()?.clips.length ?? 0) > 0,
          reason: () => "Select a track with clips first.",
        }),
        actions.register({
          id: "edit.select-clips-track-end",
          run: () => {
            const t = selectedTrack();
            if (!t) return;
            const from = anchorStart(t.clips, editor.current().selectedClips, playheadNow());
            pick("Clips to End of Track", clipsFrom(t.clips, from), t.id);
          },
          enabled: () => (selectedTrack()?.clips.length ?? 0) > 0,
          reason: () => "Select a track with clips first.",
        }),
        actions.register({
          id: "edit.select-clips-session-start",
          run: () => {
            const doc = session.current;
            if (!doc) return;
            const all = doc.tracks.flatMap((t) => t.clips);
            const to = anchorStart(all, editor.current().selectedClips, playheadNow());
            pick("Clips to Start of Session", all.length ? clipsUpTo(all, to) : [], editor.current().selectedTrack);
          },
          enabled: () => contentEnd() > 0,
          reason: () => "The session has no clips yet.",
        }),
        actions.register({
          id: "edit.select-clips-session-end",
          run: () => {
            const doc = session.current;
            if (!doc) return;
            const all = doc.tracks.flatMap((t) => t.clips);
            const from = anchorStart(all, editor.current().selectedClips, playheadNow());
            pick("Clips to End of Session", clipsFrom(all, from), editor.current().selectedTrack);
          },
          enabled: () => contentEnd() > 0,
          reason: () => "The session has no clips yet.",
        }),
        actions.register({
          id: "edit.select-view-time",
          run: () => {
            const v = view();
            if (!v) return;
            const w = editor.current().width;
            session.setView({ ...v, selection: { start: Math.max(0, v.scroll), end: Math.max(0, v.scroll) + Math.round(w * v.samples_per_pixel) } });
          },
          enabled: () => session.current !== null,
        }),
        actions.register({
          id: "edit.select-all-time",
          run: () => {
            const v = view();
            const end = contentEnd();
            if (v && end > 0) session.setView({ ...v, selection: { start: 0, end } });
          },
          enabled: () => contentEnd() > 0,
          reason: () => "The session has no clips yet.",
        }),
        actions.register({
          id: "edit.select-time-around-clips",
          run: () => {
            const v = view();
            const span = clipSpan();
            if (v && span && editor.current().selectedClips.size > 0) session.setView({ ...v, selection: { start: span.first, end: span.last } });
          },
          enabled: () => editor.current().selectedClips.size > 0,
          reason: () => "Select clips first.",
        }),
        actions.register({
          id: "edit.clear-time-selection",
          run: () => {
            const v = view();
            if (v) session.setView({ ...v, selection: null });
          },
          enabled: () => timeSel() !== null,
          reason: () => "There is no time selection.",
        }),
      ];
    };
    offs.push(
      actions.register({ id: "multitrack.play-pause", run: () => spaceToggle(transport) }),
      actions.register({
        id: "multitrack.record",
        run: () => toggleRecord(),
        label: () => (take ? "Stop Recording" : "Record"),
      }),
      actions.register({ id: "multitrack.stop", run: () => (take ? finishRecord() : transport.stop()) }),
      actions.register({
        id: "multitrack.properties",
        enabled: () => session.current !== null,
        run: () => {
          propertiesOpen = true;
        },
      }),
      actions.register({ id: "multitrack.add.mono", run: () => addTrack(false) }),
      actions.register({
        id: "multitrack.add-tracks",
        run: () => {
          addTracksOpen = true;
        },
        enabled: () => session.current !== null,
      }),
      actions.register({ id: "multitrack.add.stereo", run: () => addTrack(true) }),
      actions.register({ id: "multitrack.add.midi", run: () => addMidiTrack(), enabled: () => session.current !== null }),
      actions.register({ id: "multitrack.add.bus", run: () => addBus(true), enabled: () => session.current !== null }),
      actions.register({ id: "multitrack.add.bus-mono", run: () => addBus(false), enabled: () => session.current !== null }),
      actions.register({
        id: "multitrack.delete-track",
        run: async () => {
          const t = selectedTrack();
          if (t) await session.edit("Delete Track", { op: "remove_track", id: t.id }, sessionHistory);
          editor.selectTrack(null);
        },
        enabled: () => selectedTrack() !== null,
        reason: () => "Select a track first.",
      }),
      actions.register({
        id: "multitrack.duplicate-track",
        run: async () => {
          const doc = session.current;
          const t = selectedTrack();
          if (!doc || !t) return;
          const copy = freshIds(structuredClone(t));
          copy.name = `${t.name} copy`;
          const index = doc.tracks.findIndex((x) => x.id === t.id) + 1;
          await session.edit("Duplicate Track", { op: "restore_track", track: copy, index }, sessionHistory);
          editor.selectTrack(copy.id);
        },
        enabled: () => selectedTrack() !== null,
        reason: () => "Select a track first.",
      }),
      actions.register({
        id: "multitrack.delete-empty",
        run: async () => {
          const empty = (session.current?.tracks ?? []).filter((t) => t.clips.length === 0);
          if (!empty.length) return;
          await session.edit(
            empty.length === 1 ? "Delete Empty Track" : "Delete Empty Tracks",
            { op: "batch", ops: empty.map((t) => ({ op: "remove_track" as const, id: t.id })) },
            sessionHistory,
          );
        },
        enabled: () => (session.current?.tracks ?? []).some((t) => t.clips.length === 0),
        reason: () => "No empty tracks.",
      }),
      ...visibilityActions(),
      actions.register({
        id: "multitrack.insert-files",
        run: async () => {
          const paths = await pickMediaFiles();
          if (paths.length) await placeFiles(paths, editor.current().selectedTrack, view()?.playhead ?? 0);
        },
      }),
      actions.register({
        id: "multitrack.insert-tracks",
        run: async () => {
          const paths = await pickTrackFiles();
          if (paths.length) await placeAsTracks(paths, 0);
        },
        enabled: () => session.current !== null,
        reason: () => "Open or create a session first.",
      }),
      // Files panel right-click.
      actions.register({
        id: "files.edit",
        run: () => {
          const it = mediaPool.selectedItems().find((x) => x.status === "ready");
          if (it) void openInWaveform(it.path);
        },
        enabled: () => mediaPool.selectedItems().some((x) => x.status === "ready"),
        reason: () => "Select a file that has finished reading.",
      }),
      actions.register({
        id: "files.insert-at-playhead",
        run: async () => {
          const paths = mediaPool.selectedItems().filter((x) => x.status === "ready").map((x) => x.path);
          if (!paths.length) return;
          if (inWave()) editor.setMode("multitrack");
          await placeFiles(paths, editor.current().selectedTrack, view()?.playhead ?? 0);
        },
        enabled: () => session.current !== null && mediaPool.selectedItems().some((x) => x.status === "ready"),
        reason: () => (session.current ? "Select files in the Files panel first." : "Open or create a session first."),
      }),
      actions.register({
        id: "multitrack.insert-selected-tracks",
        run: () => placeAsTracks(mediaPool.selectedItems().map((it) => it.path), 0),
        enabled: () => session.current !== null && mediaPool.selectedItems().length > 0,
        reason: () => (session.current ? "Select files in the Files panel first." : "Open or create a session first."),
      }),
      actions.register({ id: "view.zoom-in", run: () => zoomBy(0.5), enabled: () => session.current !== null }),
      actions.register({ id: "view.zoom-out", run: () => zoomBy(2), enabled: () => session.current !== null }),
      actions.register({ id: "view.editor.toggle", run: async () => void (await actions.run(inWave() ? "view.editor.multitrack" : "view.editor.waveform")) }),
      actions.register({ id: "transport.go-start", run: () => transport.seek(0), enabled: () => session.current !== null }),
      actions.register({ id: "transport.go-end", run: () => transport.seek(contentEnd()), enabled: () => session.current !== null }),
      actions.register({ id: "transport.prev-edge", run: () => jumpEdge(-1, false), enabled: () => session.current !== null }),
      actions.register({ id: "transport.next-edge", run: () => jumpEdge(1, false), enabled: () => session.current !== null }),
      actions.register({ id: "transport.prev-marker", run: () => jumpEdge(-1, true), enabled: () => session.current !== null }),
      actions.register({ id: "transport.next-marker", run: () => jumpEdge(1, true), enabled: () => session.current !== null }),
      actions.register({
        id: "view.zoom-session",
        run: () => {
          const v = view();
          if (!v) return;
          const f = fit(contentEnd(), editor.current().width);
          session.setView({ ...v, samples_per_pixel: f.spp, scroll: 0 });
        },
        enabled: () => contentEnd() > 0,
        reason: () => "The session has no clips yet.",
      }),
      actions.register({
        id: "edit.select-track-clips",
        run: () => {
          const t = selectedTrack();
          if (t) editor.selectClips(t.clips.map((c) => c.id), t.id);
        },
        enabled: () => (selectedTrack()?.clips.length ?? 0) > 0,
        reason: () => "Select a track with clips first.",
      }),
      actions.register({
        id: "view.time.decimal",
        run: () => editor.setTimeFormat("time"),
        checked: () => editor.current().timeFormat === "time",
      }),
      actions.register({
        id: "view.time.samples",
        run: () => editor.setTimeFormat("samples"),
        checked: () => editor.current().timeFormat === "samples",
      }),
      actions.register({
        id: "view.env.volume",
        run: () => editor.setShowVolume(!editor.current().showVolume),
        checked: () => editor.current().showVolume,
      }),
      actions.register({
        id: "view.env.fx",
        run: () => editor.setShowFx(!editor.current().showFx),
        checked: () => editor.current().showFx,
      }),
      actions.register({
        id: "view.env.pan",
        run: () => editor.setShowPan(!editor.current().showPan),
        checked: () => editor.current().showPan,
      }),
      actions.register({
        id: "view.time.timecode",
        run: () => editor.setTimeFormat("timecode"),
        checked: () => editor.current().timeFormat === "timecode",
      }),
      ...(
        [
          ["file.export.mixdown", "all", "file"],
          ["file.export.selection", "selection", "file"],
          ["multitrack.mixdown.all", "all", "files-panel"],
          ["multitrack.mixdown.selection", "selection", "files-panel"],
          ["multitrack.mixdown.clips", "clips", "files-panel"],
          ["multitrack.bounce.selection", "selection", "track"],
          ["multitrack.bounce.clips", "clips", "track"],
        ] as const
      ).map(([id, source, target]) =>
        actions.register({
          id,
          run: () => mixDown(source, target),
          enabled: () => exporting === null && mixRange(source) !== null && contentEnd() > 0,
          reason: () =>
            contentEnd() === 0
              ? "The session has no clips yet."
              : source === "selection"
                ? "Drag across empty space or the ruler to select a time range."
                : "Select clips first.",
        }),
      ),
      ...Array.from({ length: WORKSPACE_SLOTS }, (_, i) =>
        actions.register({
          id: `window.workspace.${i + 1}`,
          run: () => {
            const w = wsState.items[i];
            if (w) return dock.switchTo(w.id);
          },
          label: () => wsState.items[i]?.name ?? null,
          visible: () => i < wsState.items.length,
          checked: () => wsState.items[i]?.id === wsState.current,
        }),
      ),
      actions.register({ id: "window.workspace.reset", run: () => dock.resetCurrent() }),
      actions.register({
        id: "window.workspace.save-changes",
        run: async () => {
          await dock.saveChanges();
          showNotice(`Saved "${dock.currentWorkspace.name}". Reset Workspace now returns to this arrangement.`);
        },
        enabled: () => !dock.currentWorkspace.builtIn,
        reason: () => "Built-in workspaces keep their own starting layout. Use Save as New Workspace.",
      }),
      actions.register({
        id: "window.workspace.save-as",
        run: async () => {
          const name = await promptText("Save as New Workspace", "Workspace name", `${dock.currentWorkspace.name} copy`);
          if (name) await dock.saveAs(name);
        },
      }),
      actions.register({
        id: "window.workspace.delete",
        run: async () => {
          const w = dock.currentWorkspace;
          const ok = await ask("Delete workspace", `Delete the workspace "${w.name}"? This cannot be undone.`, [
            { label: "Delete", value: "yes", primary: true },
            { label: "Cancel", value: "no" },
          ]);
          if (ok === "yes") await dock.remove(w.id);
        },
        enabled: () => !dock.currentWorkspace.builtIn,
        reason: () => "Built-in workspaces cannot be deleted.",
      }),
      actions.register({
        id: "clip.group",
        run: () => {
          const g = uuid();
          return editClips("Group Clips", selectedClipDocs().map((c) => ({ id: c.id, group: g })));
        },
        enabled: () => editor.current().selectedClips.size > 1,
        reason: () => "Select two or more clips (Ctrl+click) first.",
      }),
      actions.register({
        id: "clip.ungroup",
        run: () =>
          editClips(
            "Ungroup Clips",
            selectedClipDocs()
              .filter((c) => c.group)
              .map((c) => ({ id: c.id, ungroup: true })),
          ),
        enabled: () => selectedClipDocs().some((c) => c.group),
        reason: () => "Select a grouped clip first.",
      }),
      actions.register({
        id: "file.check-loudness",
        run: () => {
          loud = { report: null, progress: null, target: prefs.string(KEY_LOUD_TARGET, DEFAULT_TARGET), region: prefs.string(KEY_LOUD_REGION, DEFAULT_REGION), scope: "" };
          return measureLoudness();
        },
        enabled: () => exporting === null && contentEnd() > 0,
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
            showNotice(`Exported ${name}: ${r.tracks} tracks, ${r.clips} clips.${left.length ? ` Left out: ${left.join(", ")}.` : ""} Keep saving your work as .altaudio.`);
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
          markersExport = { format: CHAPTER_FORMATS.some((f) => f.id === saved) ? (saved as ChapterFormat) : "timestamps" };
        },
        enabled: () => (markerDoc()?.markers.length ?? 0) > 0,
        reason: () => "Add markers first (Edit > Markers).",
      }),
      ...FX_MENU.map((short) =>
        actions.register({
          id: `fx.add.${short}`,
          run: () => addEffectFromMenu(`altaudio.${short}`),
          enabled: () => fxTarget() !== null,
          reason: () => (inWave() ? "Open a file in the Waveform editor first." : "Select a track or one clip first."),
        }),
      ),
      actions.register({
        id: "fx.loudness-match",
        run: () => openLoudnessMatch(),
        enabled: () => exporting === null && lm === null && (session.current?.tracks.some((t) => t.clips.length > 0) ?? false),
        reason: () => "The session has no clips yet.",
      }),
      actions.register({
        id: "file.export.stems",
        run: () => exportStems(),
        enabled: () => exporting === null && (session.current?.tracks.some((t) => t.clips.length > 0 && !t.muted) ?? false),
        reason: () => "The session has no unmuted tracks with clips.",
      }),
      actions.register({
        id: "edit.marker.cue",
        run: async () => {
          const doc = session.current;
          if (!doc) return;
          const m = newMarker(uuid(), nextMarkerName(doc.markers, settings.current.markerName), playheadNow());
          await session.edit("Add Marker", { op: "add_markers", markers: [m] }, sessionHistory);
        },
        enabled: () => session.current !== null,
      }),
      actions.register({
        id: "edit.marker.subclip",
        run: async () => {
          const doc = session.current;
          const r = timeSel();
          if (!doc || !r) return;
          const m = newMarker(uuid(), nextMarkerName(doc.markers, "Subclip"), r.start, r.end - r.start, "subclip");
          await session.edit("Add Subclip Marker", { op: "add_markers", markers: [m] }, sessionHistory);
        },
        enabled: () => timeSel() !== null,
        reason: () => "Select the part to mark with the Time Selection tool first.",
      }),
      actions.register({
        id: "edit.marker.cd-track",
        run: async () => {
          const doc = session.current;
          if (!doc) return;
          const n = doc.markers.filter((x) => x.kind === "cd_track").length + 1;
          const m = newMarker(uuid(), `Track ${n}`, playheadNow(), 0, "cd_track");
          await session.edit("Add CD Track Marker", { op: "add_markers", markers: [m] }, sessionHistory);
        },
        enabled: () => session.current !== null,
      }),
      actions.register({
        id: "edit.marker.cart",
        run: async () => {
          const doc = session.current;
          if (!doc) return;
          const r = timeSel();
          const m = r
            ? newMarker(uuid(), nextMarkerName(doc.markers, "Cart"), r.start, r.end - r.start, "cart_timer")
            : newMarker(uuid(), nextMarkerName(doc.markers, "Cart"), playheadNow(), 0, "cart_timer");
          await session.edit("Add Cart Timer Marker", { op: "add_markers", markers: [m] }, sessionHistory);
        },
        enabled: () => session.current !== null,
      }),
      actions.register({
        id: "edit.marker.chapter",
        run: async () => {
          const doc = session.current;
          if (!doc) return;
          const r = timeSel();
          const m = r
            ? newMarker(uuid(), nextMarkerName(doc.markers, "Chapter"), r.start, r.end - r.start, "chapter")
            : newMarker(uuid(), nextMarkerName(doc.markers, "Chapter"), playheadNow(), 0, "chapter");
          await session.edit("Add Chapter Marker", { op: "add_markers", markers: [m] }, sessionHistory);
        },
        enabled: () => session.current !== null,
      }),
      actions.register({
        id: "edit.marker.ad-slot",
        run: async () => {
          const doc = session.current;
          if (!doc) return;
          const r = timeSel();
          const secs = await promptText("Add Ad Slot", "Contracted length of the read (seconds)", r ? ((r.end - r.start) / doc.sample_rate).toFixed(1) : "30");
          if (secs === null) return;
          const len = Number(secs);
          if (!Number.isFinite(len) || len <= 0 || len > 3600) return showNotice("Enter a length in seconds, for example 30 or 60.");
          const contracted = Math.round(len * doc.sample_rate);
          const m = r
            ? newMarker(uuid(), nextMarkerName(doc.markers, "Ad"), r.start, r.end - r.start, "ad_slot")
            : newMarker(uuid(), nextMarkerName(doc.markers, "Ad"), playheadNow(), contracted, "ad_slot");
          await session.edit("Add Ad Slot", { op: "add_markers", markers: [{ ...m, contracted_duration: contracted }] }, sessionHistory);
        },
        enabled: () => session.current !== null,
      }),
      actions.register({
        id: "edit.marker.social",
        run: async () => {
          const doc = session.current;
          const r = timeSel();
          if (!doc || !r) return;
          const m = newMarker(uuid(), nextMarkerName(doc.markers, "Social Clip"), r.start, r.end - r.start, "social_clip");
          await session.edit("Add Social Clip", { op: "add_markers", markers: [m] }, sessionHistory);
        },
        enabled: () => timeSel() !== null,
        reason: () => "Select the part to share with the Time Selection tool first.",
      }),
      actions.register({
        id: "edit.marker.range",
        run: async () => {
          const doc = session.current;
          const r = timeSel();
          if (!doc || !r) return;
          const m = newMarker(uuid(), nextMarkerName(doc.markers, "Range"), r.start, r.end - r.start);
          await session.edit("Add Range Marker", { op: "add_markers", markers: [m] }, sessionHistory);
        },
        enabled: () => timeSel() !== null,
        reason: () => "Drag across empty space or the ruler to select a time range.",
      }),
      actions.register({
        id: "edit.snap.markers",
        run: () => editor.setSnap({ markers: !editor.current().snap.markers }),
        checked: () => editor.current().snap.markers,
        enabled: () => editor.current().snap.on,
      }),
      actions.register({
        id: "edit.snap.ruler",
        run: () => editor.setSnap({ ruler: !editor.current().snap.ruler }),
        checked: () => editor.current().snap.ruler,
        enabled: () => editor.current().snap.on,
      }),
      actions.register({
        id: "clip.auto-crossfade",
        run: () => editor.setAutoCrossfade(!editor.current().autoCrossfade),
        checked: () => editor.current().autoCrossfade,
      }),
      actions.register({
        id: "clip.color",
        run: () => {
          const clips = selectedClipDocs();
          if (clips.length) colorFor = { ids: clips.map((c) => c.id), current: clips[0]!.color };
        },
        enabled: () => editor.current().selectedClips.size > 0,
        reason: () => "Select a clip first.",
      }),
      actions.register({
        id: "edit.copy",
        run: () => void copySelected(),
        enabled: () => inEditor() && editor.current().selectedClips.size > 0,
        reason: () => (inEditor() ? "Select clips first." : "Copy works on clips in the Editor."),
      }),
      actions.register({
        id: "edit.cut",
        run: async () => {
          if (!copySelected()) return;
          const ids = [...editor.current().selectedClips];
          await session.edit(ids.length === 1 ? "Cut Clip" : `Cut ${ids.length} Clips`, { op: "remove_clips", ids }, sessionHistory);
          editor.selectClips([]);
        },
        enabled: () => inEditor() && editor.current().selectedClips.size > 0,
        reason: () => (inEditor() ? "Select clips first." : "Cut works on clips in the Editor."),
      }),
      actions.register({
        id: "edit.paste",
        run: () => pasteAtPlayhead(),
        enabled: () => inEditor() && clipBoard !== null && (session.current?.tracks.length ?? 0) > 0,
        reason: () => (clipBoard ? "Click in the Editor first." : "Copy or cut clips first."),
      }),
      actions.register({
        id: "edit.ripple.clips",
        run: () => guarded("Ripple Delete", () => rippleDeleteClips(session.current!, editor.current().selectedClips), "Select clips first."),
        enabled: () => editor.current().selectedClips.size > 0,
        reason: () => "Select clips first.",
      }),
      actions.register({
        id: "edit.ripple.time-track",
        run: () => {
          const r = timeSel();
          const t = editor.current().selectedTrack;
          if (!r || !t) return;
          return guarded("Ripple Delete", () => rippleDeleteRange(session.current!, [t], r.start, r.end, uuid), "Nothing to delete in that range.");
        },
        enabled: () => timeSel() !== null && editor.current().selectedTrack !== null,
        reason: () => (timeSel() ? "Select a track first." : "Drag across empty space in the Editor to select a time range."),
      }),
      actions.register({
        id: "edit.ripple.time-all",
        run: () => {
          const r = timeSel();
          const doc = session.current;
          if (!r || !doc) return;
          return guarded(
            "Ripple Delete",
            () => rippleDeleteRange(doc, doc.tracks.map((t) => t.id), r.start, r.end, uuid),
            "Nothing to delete in that range.",
          ).then(() => {
            const cur = session.current;
            if (cur) session.setView({ ...cur.view, selection: null, playhead: r.start });
          });
        },
        enabled: () => timeSel() !== null,
        reason: () => "Drag across empty space in the Editor to select a time range.",
      }),
      actions.register({
        id: "edit.ripple.gap",
        run: () => {
          const doc = session.current;
          const t = editor.current().selectedTrack;
          if (!doc || !t) return;
          const g = gapAt(doc, t, playheadNow());
          if (!g) {
            showNotice("Put the playhead in an empty space between two clips on the selected track.");
            return;
          }
          return guarded("Close Gap", () => rippleDeleteRange(doc, [t], g.start, g.end, uuid), "Nothing to close.");
        },
        enabled: () => editor.current().selectedTrack !== null,
        reason: () => "Select a track first.",
      }),
      ...selectClipActions(),
      actions.register({
        id: "edit.select-to-end",
        run: () => {
          const v = view();
          if (!v) return;
          const end = contentEnd();
          const at = playheadNow();
          if (end > at) session.setView({ ...v, selection: { start: at, end } });
        },
        enabled: () => contentEnd() > 0,
      }),
      actions.register({
        id: "view.zoom-selection",
        run: () => {
          const v = view();
          const r = timeSel();
          if (!v || !r) return;
          const w = editor.current().width;
          const span = r.end - r.start;
          const f = fit(span, w);
          session.setView({ ...v, samples_per_pixel: f.spp, scroll: Math.max(0, Math.round(r.start - span * 0.025)) });
        },
        enabled: () => timeSel() !== null,
        reason: () => "Drag across empty space in the Editor to select a time range.",
      }),
      actions.register({
        id: "clip.trim.selection",
        run: () => {
          const doc = session.current;
          const r = timeSel();
          if (!doc || !r) return;
          return editClips("Trim to Selection", trimToRange(doc, editor.current().selectedClips, r.start, r.end));
        },
        enabled: () => timeSel() !== null && editor.current().selectedClips.size > 0,
        reason: () => "Select clips and a time range first.",
      }),
      actions.register({
        id: "edit.snap.on",
        run: () => editor.setSnap({ on: !editor.current().snap.on }),
        checked: () => editor.current().snap.on,
      }),
      actions.register({
        id: "edit.snap.clips",
        run: () => editor.setSnap({ clips: !editor.current().snap.clips }),
        checked: () => editor.current().snap.clips,
        enabled: () => editor.current().snap.on,
      }),
      actions.register({
        id: "edit.snap.playhead",
        run: () => editor.setSnap({ playhead: !editor.current().snap.playhead }),
        checked: () => editor.current().snap.playhead,
        enabled: () => editor.current().snap.on,
      }),
      ...([-1, 1] as const).map((dir) =>
        actions.register({
          id: dir < 0 ? "clip.to-track-above" : "clip.to-track-below",
          run: () => moveClipsToTrack(dir),
          enabled: () => clipsTrackMove(dir) !== null,
          reason: () => (editor.current().selectedClips.size === 0 ? "Select clips first." : dir < 0 ? "The clips are already on the top track, or one is locked." : "The clips are already on the bottom track, or one is locked."),
        }),
      ),
      actions.register({ id: "clip.split", run: () => split(false), enabled: () => (session.current?.tracks.some((t) => t.clips.length > 0) ?? false) }),
      actions.register({ id: "clip.split-all", run: () => split(true), enabled: () => (session.current?.tracks.some((t) => t.clips.length > 0) ?? false) }),
      actions.register({
        id: "clip.trim.start",
        run: () => trimTo("start"),
        enabled: () => editor.current().selectedClips.size > 0,
        reason: () => "Select a clip first.",
      }),
      actions.register({
        id: "clip.trim.end",
        run: () => trimTo("end"),
        enabled: () => editor.current().selectedClips.size > 0,
        reason: () => "Select a clip first.",
      }),
      actions.register({
        id: "fx.gen.tone",
        run: () => {
          generateMode = "tone";
        },
        enabled: () => !inWave() && session.current !== null,
        reason: () => "Generate works in the Multitrack editor.",
      }),
      actions.register({
        id: "fx.gen.noise",
        run: () => {
          generateMode = "noise";
        },
        enabled: () => !inWave() && session.current !== null,
        reason: () => "Generate works in the Multitrack editor.",
      }),
      actions.register({
        id: "fx.gen.silence",
        run: () => {
          generateMode = "silence";
        },
        enabled: () => !inWave() && (session.current?.tracks.length ?? 0) > 0,
        reason: () => "Insert Silence works in the Multitrack editor.",
      }),
      actions.register({
        id: "multitrack.insert-silence",
        run: () => {
          generateMode = "silence";
        },
        enabled: () => !inWave() && (session.current?.tracks.length ?? 0) > 0,
        reason: () => "Insert Silence works in the Multitrack editor.",
      }),
      actions.register({
        id: "multitrack.record-options",
        run: () => {
          recordOptionsOpen = true;
        },
      }),
      ...AUDIO_TYPES.map((t) =>
        actions.register({
          id: `clip.type.${t.id}`,
          run: () => editClips(`Audio Type: ${t.label}`, selectedClipDocs().map((c) => ({ id: c.id, audio_type: t.id }))),
          enabled: () => selectedClipDocs().length > 0,
          reason: () => "Select clips first.",
          checked: () => {
            const c = selectedClipDocs();
            return c.length > 0 && c.every((x) => x.audio_type === t.id);
          },
        }),
      ),
      actions.register({
        id: "clip.type.none",
        run: () => editClips("Clear Audio Type", selectedClipDocs().filter((c) => c.audio_type).map((c) => ({ id: c.id, clear_audio_type: true }))),
        enabled: () => selectedClipDocs().some((c) => !!c.audio_type),
        reason: () => "None of the selected clips has an audio type.",
      }),
      ...AUDIO_TYPES.map((t) =>
        actions.register({
          id: `edit.select-type.${t.id}`,
          run: () => {
            const doc = session.current;
            if (!doc) return;
            const ids = doc.tracks.flatMap((tr) => tr.clips.filter((c) => c.audio_type === t.id).map((c) => c.id));
            editor.selectClips(ids);
            if (ids.length === 0) showNotice(`No clips are marked ${t.label}. Set a clip's type with Clip > Audio Type.`);
          },
          enabled: () => session.current?.tracks.some((tr) => tr.clips.some((c) => c.audio_type === t.id)) ?? false,
          reason: () => `No clips are marked ${t.label} yet. Set one with Clip > Audio Type.`,
        }),
      ),
      actions.register({
        id: "clip.stretch",
        run: () => {
          const c = selectedClipDocs();
          if (c.length === 1) stretchFor = c[0]!.id;
        },
        enabled: () => {
          const c = selectedClipDocs();
          return c.length === 1 && !c[0]!.locked && !c[0]!.looped;
        },
        reason: () => {
          const c = selectedClipDocs();
          if (c.length !== 1) return "Select one clip.";
          if (c[0]!.locked) return "The clip is locked. Unlock it first (Clip > Lock Position).";
          return "Looped clips cannot stretch. Turn off Loop Clip first.";
        },
      }),
      actions.register({
        id: "multitrack.speaker-balance",
        run: () => void runSpeakerBalance().catch(showError),
        enabled: () => !balancing && session.current !== null && balanceItems(session.current).length >= 2,
        reason: () => (balancing ? "Measuring now." : "Needs two or more voice tracks with audio (music, effects and ambience tracks are left out)."),
      }),
      ...HOST_PROFILES.map((p) =>
        actions.register({
          id: `multitrack.profile.${p.id}`,
          run: () => void applyProfile(p.id).catch(showError),
          enabled: () => profileTrack() !== null,
          reason: () => "Select a track first.",
        }),
      ),
      actions.register({
        id: "multitrack.profile.clear",
        run: () => void applyProfile(null).catch(showError),
        enabled: () => !!profileTrack()?.host_profile_id,
        reason: () => "The selected track has no host profile.",
      }),
      actions.register({
        id: "clip.phase-align",
        run: () => void runPhaseAlign().catch(showError),
        enabled: () => {
          const d = session.current;
          return d !== null && alignPair(d, editor.current().selectedClips) !== null;
        },
        reason: () => "Select two clips of the same sound on two tracks. The upper one stays put; the lower one moves.",
      }),
      actions.register({
        id: "clip.music-fit",
        run: () => {
          const c = selectedClipDocs();
          if (c.length === 1) fitFor = c[0]!.id;
        },
        enabled: () => {
          const c = selectedClipDocs();
          return c.length === 1 && !c[0]!.locked && !c[0]!.looped;
        },
        reason: () => {
          const c = selectedClipDocs();
          if (c.length !== 1) return "Select one music clip.";
          if (c[0]!.locked) return "The clip is locked. Unlock it first (Clip > Lock Position).";
          return "Turn off Loop Clip first.";
        },
      }),
      actions.register({
        id: "clip.stretch-reset",
        run: () => removeStretch(editor.current().selectedClips),
        enabled: () => selectedClipDocs().some((c) => !!c.stretch && !c.locked),
        reason: () => "Select a stretched clip.",
      }),
      actions.register({
        id: "clip.loop",
        run: () => {
          const doc = session.current;
          if (!doc) return;
          const { changes, on } = loopChanges(doc, editor.current().selectedClips);
          return editClips(on ? "Loop Clip" : "Stop Looping Clip", changes);
        },
        enabled: () => selectedClipDocs().some((c) => !c.locked),
        reason: () => "Select a clip that is not locked.",
        checked: () => {
          const c = selectedClipDocs();
          return c.length > 0 && c.every((x) => !!x.looped);
        },
      }),
      actions.register({
        id: "clip.unique",
        run: () => makeUniqueCopy(),
        enabled: () => exporting === null && selectedClipDocs().some((c) => !c.locked),
        reason: () => (exporting ? "Wait for the current export to finish." : "Select a clip that is not locked."),
      }),
      actions.register({
        id: "clip.merge",
        run: () => mergeClips(),
        enabled: () => exporting === null && mergeTarget() !== null,
        reason: () => (exporting ? "Wait for the current export to finish." : mergeReason()),
      }),
      actions.register({
        id: "clip.mute",
        run: () => {
          const clips = selectedClipDocs();
          const on = clips.every((c) => c.muted);
          return editClips(on ? "Unmute Clip" : "Mute Clip", clips.map((c) => ({ id: c.id, muted: !on })));
        },
        enabled: () => editor.current().selectedClips.size > 0,
        checked: () => {
          const c = selectedClipDocs();
          return c.length > 0 && c.every((x) => x.muted);
        },
        reason: () => "Select a clip first.",
      }),
      actions.register({
        id: "clip.lock",
        run: () => {
          const clips = selectedClipDocs();
          const on = clips.every((c) => c.locked);
          return editClips(on ? "Unlock Clip" : "Lock Clip", clips.map((c) => ({ id: c.id, locked: !on })));
        },
        enabled: () => editor.current().selectedClips.size > 0,
        checked: () => {
          const c = selectedClipDocs();
          return c.length > 0 && c.every((x) => x.locked);
        },
        reason: () => "Select a clip first.",
      }),
      actions.register({
        id: "clip.fade-in",
        run: () => toggleFade("in"),
        enabled: () => editor.current().selectedClips.size > 0,
        checked: () => {
          const c = selectedClipDocs();
          return c.length > 0 && c.every((x) => x.fade_in.length > 0);
        },
        reason: () => "Select a clip first.",
      }),
      actions.register({
        id: "clip.fade-out",
        run: () => toggleFade("out"),
        enabled: () => editor.current().selectedClips.size > 0,
        checked: () => {
          const c = selectedClipDocs();
          return c.length > 0 && c.every((x) => x.fade_out.length > 0);
        },
        reason: () => "Select a clip first.",
      }),
      actions.register({
        id: "clip.gain",
        run: () => clipGain(),
        enabled: () => editor.current().selectedClips.size > 0,
        reason: () => "Select a clip first.",
      }),
      actions.register({
        id: "clip.properties",
        run: () => dock.show("properties"),
        enabled: () => editor.current().selectedClips.size > 0,
        reason: () => "Select a clip first.",
      }),
      actions.register({
        id: "clip.reveal",
        run: () => {
          const ids = editor.current().selectedClips;
          const media = new Set<string>();
          for (const t of session.current?.tracks ?? []) for (const c of t.clips) if (ids.has(c.id)) media.add(c.media_id);
          dock.show("files");
          mediaPool.setSelection(media);
        },
        enabled: () => editor.current().selectedClips.size > 0,
        reason: () => "Select a clip first.",
      }),
      actions.register({
        id: "file.close-unused",
        run: async () => {
          const used = usedMedia();
          const unused = mediaPool.current.items.filter((it) => !used.has(it.id)).map((it) => it.id);
          await mediaPool.remove(unused, sessionHistory);
          showNotice(unused.length ? `Removed ${unused.length} unused ${unused.length === 1 ? "file" : "files"}.` : "Every file is used by a clip.");
        },
        enabled: () => mediaPool.size > 0,
        reason: () => "No files in the session.",
      }),
    );
    offs.push(
      actions.register({ id: "file.new-session", run: () => askNewSession() }),
      actions.register({ id: "file.save-template", run: () => saveAsTemplate() }),
      actions.register({ id: "file.open", run: () => openAny() }),
      actions.register({ id: "file.save", run: () => pluginSync.captureAll().then(() => files.save()).then(() => undefined) }),
      actions.register({ id: "file.save-as", run: () => pluginSync.captureAll().then(() => files.saveAs()).then(() => undefined) }),
      actions.register({
        id: "fx.plugin-manager",
        run: () => {
          pluginManagerOpen = true;
        },
      }),
      actions.register({ id: "file.save-all", run: () => files.save().then(() => undefined) }),
      actions.register({ id: "file.close", run: () => files.close().then(() => undefined) }),
      actions.register({ id: "file.import-sesx", run: () => files.importSesx().then(() => undefined) }),
      actions.register({ id: "file.close-all", run: () => files.close().then(() => undefined) }),
      actions.register({
        id: "file.recent.none",
        run: () => {},
        enabled: () => false,
        visible: () => files.recentPaths().length === 0,
        reason: () => "Sessions you save or open appear here.",
      }),
    );
    for (let i = 0; i < RECENT_SLOTS; i++) {
      offs.push(
        actions.register({
          id: `file.recent.${i + 1}`,
          run: async () => {
            const p = files.recentPaths()[i];
            if (p) await files.open(p);
          },
          visible: () => files.recentPaths()[i] !== undefined,
          label: () => {
            const p = files.recentPaths()[i];
            return p ? `${i + 1}  ${p}` : null;
          },
        }),
      );
    }
    const pluginsUrl = LINKS.plugins;
    if (pluginsUrl) offs.push(actions.register({ id: "help.plugins", run: () => openLink(pluginsUrl) }));
    const supportUrl = LINKS.support;
    if (supportUrl) offs.push(actions.register({ id: "help.support", run: () => openLink(supportUrl) }));
    for (const p of PANELS) {
      offs.push(
        actions.register({
          id: `window.panel.${p.id}`,
          run: () => dock.toggle(p.id),
          checked: () => openPanels.has(p.id),
        }),
      );
    }
    return offs;
  }

  // Effect envelopes need each control's range; plug-ins change after a scan.
  // Running plug-ins are kept in step with the session while the app is open.
  $effect(() => {
    pluginSync.start();
    const stopCatalog = pluginsVersion.subscribe(() => {
      void effectsCatalog()
        .then((c) => {
          fxCatalog = c;
          setFxCatalog(c);
        })
        .catch(() => {});
    });
    return () => {
      stopCatalog();
      pluginSync.stop();
    };
  });

  onMount(() => {
    void prefs
      .load()
      .then(() => libraries.restore())
      .then((missing) => {
        if (missing.length) showNotice(`Could not open ${missing.length === 1 ? "a library" : `${missing.length} libraries`}: ${missing.join(", ")}`);
      })
      .catch(showError);
    getAppInfo()
      .then((i) => (info = i))
      .catch((e: unknown) => (coreError = e instanceof Error ? e.message : String(e)));
    const offs = registerActions();
    offs.push(...registerWaveActions());
    offs.push(sessionHistory.subscribe(() => actions.invalidate()));
    offs.push(
      dock.open.subscribe((ids) => {
        openPanels = ids;
        actions.invalidate();
      }),
    );
    offs.push(keymap.changed.subscribe(() => actions.invalidate()));
    offs.push(mediaPool.subscribe(() => actions.invalidate()));
    offs.push(libraries.subscribe(() => actions.invalidate()));
    offs.push(session.subscribe(() => actions.invalidate()));
    offs.push(editor.subscribe(() => actions.invalidate()));
    offs.push(
      dock.workspaces.subscribe((w) => {
        wsState = w;
        actions.invalidate();
      }),
    );
    session
      .load()
      .then(() => prefs.load())
      .then(() => {
        // Editor settings (snapping, crossfades, time format) carry over between runs.
        restoreSettings(prefs.raw(KEY_EDITOR));
        settings.set(sanitizeSettings(prefs.raw(KEY_SETTINGS)));
        applyAppSettings(settings.current, false);
        // Settings changed outside the Settings window (a view's right-click colors) are kept too.
        let lastSettings = JSON.stringify(settings.current);
        offs.push(
          settings.subscribe((v) => {
            const now = JSON.stringify(v);
            if (now === lastSettings) return;
            lastSettings = now;
            prefs.set(KEY_SETTINGS, v);
          }),
        );
        let last = JSON.stringify(settingsOf(editor.current()));
        offs.push(
          editor.subscribe((st) => {
            const now = JSON.stringify(settingsOf(st));
            if (now === last) return;
            last = now;
            prefs.set(KEY_EDITOR, settingsOf(st));
          }),
        );
      })
      .then(() => files.checkRecovery())
      .catch(showError)
      .finally(() => restartAutosave());
    offs.push(() => files.stopAutosave());
    offs.push(sessionHistory.subscribe(updateTitle));
    offs.push(session.subscribe(updateTitle));
    offs.push(session.subscribe(() => syncMonitor()));
    offs.push(editor.subscribe(() => (updateEditorTitle(), updateSessionRow())));
    offs.push(wave.subscribe(() => updateEditorTitle()));
    if (isTauri()) {
      let stopClose: (() => void) | null = null;
      let gone = false;
      void getCurrentWindow()
        .onCloseRequested(async (e) => {
          e.preventDefault();
          await exitSafely();
        })
        .then((off) => (gone ? off() : (stopClose = off)));
      offs.push(() => {
        gone = true;
        stopClose?.();
      });
    }
    offs.push(activeArea.subscribe(() => actions.invalidate()));
    if (body) offs.push(trackArea(body));
    let unbindDrop: (() => void) | null = null;
    let stopped = false;
    bindNativeFileDrop(fileDrop)
      .then((off) => (stopped ? off() : (unbindDrop = off)))
      .catch(showError);
    offs.push(() => {
      stopped = true;
      unbindDrop?.();
    });
    loadKeymapOverrides()
      .then((o) => keymap.setOverrides(o))
      .catch((e: unknown) => showError(`Saved shortcuts could not be read, using defaults: ${String(e)}`));
    offs.push(bindKeymap(keymap, actions, showError));
    return () => offs.forEach((off) => off());
  });
</script>

<div class="shell">
  <header class="top">
    <MenuBar menus={MENUS} {actions} {shortcutFor} onError={showError} />
  </header>
  <ToolStrip {actions} {shortcutFor} />

  <main class="body" bind:this={body}>
    <DockShell {dock} />
  </main>

  <footer class="status">
    {#if info}
      <span>Core {info.version}</span>
      <span>Session format v{info.sessionSchemaVersion}</span>
    {:else if coreError}
      <span class="warn">Core not connected: {coreError}</span>
    {:else}
      <span>Connecting to core...</span>
    {/if}
    {#if notice}
      <span>{notice}</span>
    {/if}
    {#if lastError}
      <span class="warn">{lastError}</span>
    {/if}
  </footer>
</div>

{#if shortcutsOpen}
  <KeyboardShortcutsDialog {keymap} onChange={saveKeymap} onClose={() => (shortcutsOpen = false)} />
{/if}

{#if colorFor}
  <ClipColorDialog
    current={colorFor.current}
    onDone={(c) => {
      const target = colorFor;
      colorFor = null;
      if (target && c !== null) void editClips("Clip Color", target.ids.map((id) => ({ id, color: c }))).catch(showError);
    }}
  />
{/if}

{#if loud}
  <LoudnessDialog
    report={loud.report}
    progress={loud.progress}
    targetId={loud.target}
    region={loud.region}
    onRegion={(id) => {
      prefs.set(KEY_LOUD_REGION, id);
      if (loud) loud = { ...loud, region: id };
    }}
    scope={loud.scope}
    onTarget={(id) => {
      prefs.set(KEY_LOUD_TARGET, id);
      if (loud) loud = { ...loud, target: id };
    }}
    onMeasure={() => void measureLoudness()}
    onCancel={() => void exportCancel()}
    onApplyGain={(db) => void applyMasterGain(db).catch(showError)}
    onClose={() => (loud = null)}
  />
{/if}

{#if pluginManagerOpen}
  <PluginManagerDialog
    pickFolder={() => exportPickFolder(null, "Add a Plug-in Folder")}
    onNotice={showNotice}
    onError={showError}
    onClose={() => (pluginManagerOpen = false)}
  />
{/if}

{#if addTracksOpen && session.current}
  <AddTracksDialog
    existing={session.current.tracks.map((t) => t.name)}
    afterName={selectedTrackDoc()?.name ?? null}
    onApply={(v) => void addTracks(v).catch(showError)}
    onClose={() => (addTracksOpen = false)}
  />
{/if}

{#if countdown !== null}
  <div class="countdown" role="status" aria-live="assertive">Recording in {countdown}<span>Press Record again to cancel</span></div>
{/if}

{#if settingsDraft}
  <SettingsDialog initial={settingsDraft} onApply={applySettingsDraft} onNotice={showNotice} onClose={() => (settingsDraft = null)} />
{/if}

{#if recordOptionsOpen}
  <RecordOptionsDialog
    initial={recordOptions()}
    onApply={(o) => {
      prefs.set(KEY_RECORD, o);
      recordOptionsOpen = false;
    }}
    onClose={() => (recordOptionsOpen = false)}
  />
{/if}

{#if generateMode}
  {@const mode = generateMode}
  <GenerateDialog
    {mode}
    initial={generateDefaults()}
    trackName={selectedTrackDoc()?.name ?? null}
    selection={mode === "silence" && timeSel() ? (timeSel()!.end - timeSel()!.start) / (session.current?.sample_rate ?? 48000) : null}
    onApply={(v) => void applyGenerate(mode, v).catch(showError)}
    onClose={() => (generateMode = null)}
  />
{/if}

{#if fitClip && session.current}
  {@const doc = session.current}
  {@const sel = timeSel()}
  {@const fm = doc.media.find((x) => x.id === fitClip.clip.media_id)}
  <MusicFitDialog
    clipName={fitClip.clip.name}
    origin={fitClip.clip.source_in / (fm?.sample_rate || doc.sample_rate)}
    current={clipEnd(fitClip.clip, mediaOf(doc), doc.sample_rate) / doc.sample_rate - fitClip.clip.start / doc.sample_rate}
    selection={sel ? (sel.end - sel.start) / doc.sample_rate : null}
    plan={planFit}
    onApply={(p) => void applyFit(p).catch(showError)}
    onClose={() => (fitFor = null)}
  />
{/if}

{#if stretchClip && session.current}
  {@const doc = session.current}
  {@const m = doc.media.find((x) => x.id === stretchClip.media_id)}
  {@const sel = timeSel()}
  <StretchDialog
    clipName={stretchClip.name}
    natural={naturalLength(stretchClip, m, doc.sample_rate) / doc.sample_rate}
    current={clipEnd(stretchClip, mediaOf(doc), doc.sample_rate) / doc.sample_rate - stretchClip.start / doc.sample_rate}
    keepPitch={stretchClip.stretch?.keep_pitch ?? true}
    selection={sel ? (sel.end - sel.start) / doc.sample_rate : null}
    stretched={!!stretchClip.stretch}
    onApply={(secs, pitch, toSel) => void applyStretch(secs, pitch, toSel).catch(showError)}
    onReset={() => {
      const id = stretchClip.id;
      stretchFor = null;
      void removeStretch([id]).catch(showError);
    }}
    onClose={() => (stretchFor = null)}
  />
{/if}

{#if markersExport && markerDoc()}
  <ExportMarkersDialog
    markers={markerDoc()!.markers}
    rate={markerDoc()!.sample_rate}
    format={markersExport.format}
    onFormat={(f) => {
      prefs.set(KEY_CHAPTER_FORMAT, f);
      if (markersExport) markersExport = { format: f };
    }}
    onSave={(content, f) => void saveMarkers(content, f).catch(showError)}
    onClose={() => (markersExport = null)}
  />
{/if}

{#if lm}
  <LoudnessMatchDialog
    mode={lm.mode}
    rows={lm.rows}
    progress={lm.progress}
    targetId={lm.target}
    region={lm.region}
    onTarget={(id) => {
      prefs.set(KEY_MATCH_TARGET, id);
      if (lm) lm = { ...lm, target: id, rows: null };
      void measureMatch();
    }}
    onRegion={(id) => {
      prefs.set(KEY_LOUD_REGION, id);
      if (lm) lm = { ...lm, region: id, rows: null };
      void measureMatch();
    }}
    onMeasure={() => void measureMatch()}
    onCancel={() => {
      lmStop = true;
      void exportCancel();
    }}
    onApply={(rows) => void applyMatch(rows).catch(showError)}
    onClose={() => (lm = null)}
  />
{/if}

{#if exporting}
  <ExportProgress title={exporting.title} progress={exporting.progress} floating={exporting.floating === true} onCancel={() => void exportCancel()} />
{/if}

{#if promptReq}
  <PromptDialog
    title={promptReq.title}
    label={promptReq.label}
    value={promptReq.value}
    onDone={(v) => {
      const r = promptReq?.resolve;
      promptReq = null;
      r?.(v);
    }}
  />
{/if}

{#if propertiesOpen && session.current}
  <SessionPropertiesDialog doc={session.current} path={session.path} onDone={(v) => void applyProperties(v)} />
{/if}

{#if exportReq}
  <ExportDialog
    initial={exportReq.initial}
    scopes={exportReq.scopes}
    source={exportReq.source}
    chapterCount={session.current?.markers.filter((m) => m.kind === "chapter").length ?? 0}
    onBrowse={(from) => exportPickFolder(from || null, "Export To")}
    onDone={(v) => void finishExportDialog(v)}
  />
{/if}

{#if choice}
  <ChoiceDialog
    title={choice.title}
    message={choice.message}
    buttons={choice.buttons}
    onChoose={(v) => {
      const r = choice?.resolve;
      choice = null;
      r?.(v);
    }}
  />
{/if}

{#if newSessionReq}
  <NewSessionDialog
    initial={newSessionReq.initial}
    templates={newSessionReq.templates}
    error={newSessionReq.error ?? null}
    onBrowse={(current) => exportPickFolder(current || null, "Folder for New Sessions")}
    onDeleteTemplate={deleteTemplate}
    onDone={(v) => {
      newSessionReq = null;
      if (v) void makeNewSession(v);
    }}
  />
{/if}

{#if newAudio}
  <NewAudioFileDialog
    initial={newAudio}
    onDone={(v) => {
      newAudio = null;
      if (v) void makeNewAudio(v);
    }}
  />
{/if}

{#if aboutOpen}
  <AboutDialog version={info?.version ?? null} onClose={() => (aboutOpen = false)} />
{/if}

<style>
  .countdown {
    position: fixed;
    top: 40%;
    left: 50%;
    z-index: 6000;
    display: grid;
    gap: 4px;
    padding: 14px 28px;
    transform: translate(-50%, -50%);
    border: 1px solid var(--danger);
    border-radius: 4px;
    background: var(--surface-2);
    box-shadow: 0 12px 30px rgba(0, 0, 0, 0.6);
    color: var(--text);
    font-size: 28px;
    font-weight: 600;
    text-align: center;
    pointer-events: none;
  }
  .countdown span {
    color: var(--text-dim);
    font-size: 12px;
    font-weight: 400;
  }
  .shell {
    display: grid;
    grid-template-rows: 24px auto 1fr var(--row-height);
    height: 100%;
  }

  .top {
    display: flex;
    align-items: stretch;
    padding: 0;
    background: var(--surface-2);
    border-bottom: 1px solid var(--border);
  }

  .body {
    min-height: 0;
    background: var(--surface-0);
  }

  .status {
    display: flex;
    gap: var(--space-4);
    align-items: center;
    padding: 0 var(--space-3);
    background: var(--surface-2);
    border-top: 1px solid var(--border);
    color: var(--text-dim);
    font-size: var(--font-size-small);
  }

  .warn {
    color: var(--danger);
  }
</style>
