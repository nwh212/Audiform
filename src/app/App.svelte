<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  App shell: menu bar, docking layout, status bar.
-->
<script lang="ts">
  import { startMetronome } from "../stores/metronome";
  import { onMount, type Component } from "svelte";
  import {
    workspaceDelete,
    exitApp,
    getAppInfo,
    loadKeymapOverrides,
    workspaceGet,
    workspaceList,
    saveKeymapOverrides,
    workspacePut,
    type AppInfo,
  } from "./core";
  import { sessionHistory } from "../stores/history";
  import { keymap } from "../stores/keymap";
  import { bindKeymap } from "../keymap/dispatch";
  import KeyboardShortcutsDialog from "./KeyboardShortcutsDialog.svelte";
  import { DockController, type PanelContent, type WorkspaceState } from "./docking/dock";
  import { panelDef, type PanelId } from "./docking/registry";
  import DockShell from "./docking/DockShell.svelte";
  import { ActionRegistry } from "./actions";
  import { MENUS } from "./menus/model";
  import { watchThemeCursor } from "./theme-cursor";
  import MenuBar, { type PopupList } from "./menus/MenuBar.svelte";
  import type { ContextItem } from "./ContextMenu.svelte";
  import AboutDialog from "./AboutDialog.svelte";
  import HistoryPanel from "../panels/history/HistoryPanel.svelte";
  import EditorPanel from "../panels/editor/EditorPanel.svelte";
  import { peaks, session, transport } from "../stores/open-session";
  import PhaseWheelPanel from "../panels/phase/PhaseWheelPanel.svelte";
  import SpacePanel from "../panels/space/SpacePanel.svelte";
  import PropertiesPanel from "../panels/properties/PropertiesPanel.svelte";
  import LevelsPanel from "../panels/levels/LevelsPanel.svelte";
  import { PreviewMeter } from "../stores/preview-meter";
  import QuickPassPanel from "../panels/quickpass/QuickPassPanel.svelte";
  import BatchConvertPanel from "../panels/batch/BatchConvertPanel.svelte";
  import MixerPanel from "../panels/mixer/MixerPanel.svelte";
  import TimeInfoPanel from "../panels/time-info/TimeInfoPanel.svelte";
  import { editor, restoreSettings, settingsOf } from "../stores/editor";
  import { sessionEnd, setErrorSink, type ClipDoc, type Op } from "../stores/session";
  import ChoiceDialog, { type ChoiceButton } from "./ChoiceDialog.svelte";
  import ExportDialog, { type ExportScopeInfo, type ExportSettings } from "./ExportDialog.svelte";
  import SessionPropertiesDialog from "./SessionPropertiesDialog.svelte";
  import PromptDialog from "./PromptDialog.svelte";
  import ExportProgress from "./ExportProgress.svelte";
  import LoudnessDialog from "./LoudnessDialog.svelte";
  import LoudnessMatchDialog from "./LoudnessMatchDialog.svelte";
  import ExportMarkersDialog from "./ExportMarkersDialog.svelte";
  import StretchDialog from "./StretchDialog.svelte";
  import MusicFitDialog from "./MusicFitDialog.svelte";
  import { describeJoins, describePlan, FIT_XFADE, fitOps, type FitPlan } from "../panels/editor/musicfit";
  import SettingsDialog from "./settings/SettingsDialog.svelte";
  import type { SettingsDraft } from "./settings/pages";
  import { colorVars, meterPercent, sanitizeSettings, settings, type AppSettings } from "../stores/settings";
  import { appearanceChanged } from "../stores/appearance";
  import { bindScrub } from "./scrub";
  import { setMeterSpeed, type TransportStore } from "../stores/transport";
  import GenerateDialog, { type GenerateMode, type GenerateValues } from "./GenerateDialog.svelte";
  import { naturalLength, stretchTo } from "../panels/editor/stretch";
  import { matchChanges, matchItems, matchRow, type MatchItem, type MatchRow } from "./loudness-match";
  import { DEFAULT_REGION, DEFAULT_TARGET, setCustomTarget, targetById, type LoudnessReport } from "./loudness";
  import ClipColorDialog from "./ClipColorDialog.svelte";
  import MarkersPanel from "../panels/markers/MarkersPanel.svelte";
  import PlaylistPanel from "../panels/playlist/PlaylistPanel.svelte";
  import { listen } from "@tauri-apps/api/event";
  import {
    clipEnd,
    insertTime,
    clearRange,
    findClips,
    mediaOf,
    LockedError,
    type ClipBoard,
  } from "../panels/editor/clip-edit";
  import { type MarkerDoc } from "../stores/session";
  import { propertiesOp, type SessionProperties } from "./session-properties";
  import { createRecorder, KEY_RECORD } from "./recorder.svelte";
  import { createClipTools } from "./clip-tools.svelte";
  import { contentEnd, playheadNow, sessionView } from "./navigation";
  import { createEditTools } from "./edit-tools.svelte";
  import { createMultitrackTools } from "./multitrack-tools.svelte";
  import { createFileTools } from "./file-tools.svelte";
  import { createEffectsTools } from "./effects-tools.svelte";
  import { createViewTools } from "./view-tools.svelte";
  import { createWaveTools } from "./wave-tools.svelte";
  import { createExportTools } from "./export-tools.svelte";
  import RecordOptionsDialog from "./RecordOptionsDialog.svelte";
  import AddTracksDialog, { type AddTracksValues } from "./AddTracksDialog.svelte";
  import PluginManagerDialog from "./PluginManagerDialog.svelte";
  import { PluginSync } from "./plugin-sync";
  import { markStopped, pluginsVersion } from "../stores/plugins";
  import { newTrackNames } from "./track-names";
  import { CompoundCommand } from "../commands/compound";
  import { repointPlan, toCopy } from "./session-media";
  import { pluginSlots } from "./plugin-sync";
  import { exactSamples } from "../stores/exact-samples";
  import { waveChannels } from "../panels/editor/wave-channels";
  import { pathsIgnoreCase, PLATFORM } from "../lib/platform";
  import { SessionFiles, displayName, planOpen, type Choice } from "./session-files";
  import { RECENT_SLOTS } from "./menus/model";
  import {
    sessionAutosave,
    sessionClearAutosave,
    sessionDiscardRecovery,
    sessionNew,
    sessionPark,
    sessionResume,
    sessionForget,
    mediaCopyIn,
    sessionOpen,
    sessionPickOpen,
    generateAudio,
    exportMixdown,
    pluginEvents,
    pluginState,
    templateList,
    templateDelete,
    sessionDefaultFolder,
    sessionNextName,
    type TemplateEntry,
    exportCancel,
    exportPickFolder,
    unzipMedia,
    loudnessCheck,
    sessionPickSave,
    sessionImportSesx,
    sessionRecover,
    sessionRecoverable,
    sessionSave,
    setWindowTitle,
    setAudioOutput,
    setOutputMap,
    setAudioDriver,
    pluginShowBypassed,
    pluginHideWindows,
    effectsCatalog,
    pluginsRegistry,
    musicFit,
    quickpassRepairCopy,
    type SpotsRequest,
    type EffectInfo,
    type Recoverable,
    previewStatus,
    probeMedia,
    waveSetChannelsOff,
    waveLiveRender,
    waveLiveClear,
  } from "./core";
  import { isTauri } from "@tauri-apps/api/core";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import type { EffectChainDoc, EffectTarget, MediaRefDoc, SessionDoc } from "../stores/session";
  import PendingPanel from "../panels/PendingPanel.svelte";
  import VoiceChainPanel from "../panels/voice-chain/VoiceChainPanel.svelte";
  import { addSlot, chainOf, newSlot, type MenuPlugin } from "../panels/voice-chain/chain";
  import { effectSearch, vst3Items } from "../panels/voice-chain/slot-drag";
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
  import { activeArea, currentArea, trackArea } from "./area";
  import ToolStrip from "./ToolStrip.svelte";
  import NewAudioFileDialog, { type NewAudioSettings } from "./NewAudioFileDialog.svelte";
  import NewSessionDialog, { type NewSessionSettings } from "./NewSessionDialog.svelte";
  import { liveCopies, samePath } from "../stores/wave";
  import { closeWaveFile, forgetParked, newEmptyWaveFile, parkedUnsaved, newWaveFromClips, wave, waveFile, waveHistory, waveTransport } from "../stores/wave";
  import { get, writable } from "svelte/store";
  import type { SessionRow } from "../panels/files/session-row";
  import { recordControl } from "../stores/record";

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

  function nextTrackName(tracks: readonly { name: string }[]): string {
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
  async function placeOps(paths: string[], trackId: string | null, at: number, place: { index?: number; name?: string } = {}): Promise<{ ops: Op[]; clips: { id: string }[]; target: string } | null> {
    if (!session.current || paths.length === 0) return null;
    const imported = await mediaPool.importPaths(paths, sessionHistory);
    await mediaPool.whenIdle();
    const doc = session.current;
    const items = imported.ids.map((id) => mediaPool.get(id)).filter((it) => it?.status === "ready" && it.info);
    if (items.length === 0) return null;
    const ops: Op[] = [];
    const audio = doc.tracks.filter((t) => t.kind !== "midi");
    const names: { name: string }[] = doc.tracks.map((t) => ({ name: t.name }));
    /** A new track at the bottom for `it`, named after the file when files go side by side. */
    const newTrack = (it: (typeof items)[number], byFile: boolean): string => {
      const id = uuid();
      const name = place.name ?? (byFile ? stem(it!.name) : nextTrackName(names));
      ops.push({ op: "add_track", id, name, stereo: (it!.info!.channels ?? 2) > 1, index: place.index ?? names.length });
      names.push({ name });
      return id;
    };
    // Audio never lands on a MIDI track: it gets a new track instead.
    const first = trackId && audio.some((t) => t.id === trackId) ? trackId : null;
    const known = new Set(doc.media.map((m) => m.id));
    const addMedia = (it: (typeof items)[number]) => {
      const info = it!.info!;
      if (known.has(it!.id)) return;
      known.add(it!.id);
      ops.push({ op: "add_media", media: { id: it!.id, path: it!.path, relative_path: null, sample_rate: info.sampleRate, channels: info.channels, length: info.length } });
    };
    const start = Math.max(0, Math.round(at));
    const clips: { id: string }[] = [];
    if (items.length > 1 && settings.current.dropMany === "tracks") {
      // Side by side: the first on the track under the pointer, the rest on
      // the audio tracks below it, then on new tracks, all from the same time.
      const below = first ? audio.slice(audio.findIndex((t) => t.id === first)) : [];
      let target = first ?? "";
      items.forEach((it, k) => {
        addMedia(it);
        const info = it!.info!;
        const track = below[k]?.id ?? newTrack(it, true);
        if (k === 0) target = track;
        const clip = { id: uuid(), name: stem(it!.name), media_id: it!.id, start, source_in: 0, source_out: info.length };
        ops.push({ op: "add_clips", track_id: track, clips: [clip] });
        clips.push(clip);
      });
      return { ops, clips, target };
    }
    const target = first ?? newTrack(items[0]!, false);
    const list = [];
    let pos = start;
    for (const it of items) {
      addMedia(it);
      const info = it!.info!;
      list.push({ id: uuid(), name: stem(it!.name), media_id: it!.id, start: pos, source_in: 0, source_out: info.length });
      pos += Math.round((info.length * doc.sample_rate) / info.sampleRate);
    }
    ops.push({ op: "add_clips", track_id: target, clips: list });
    return { ops, clips: list, target };
  }

  async function placeFiles(paths: string[], trackId: string | null, at: number, place: { index?: number; name?: string } = {}) {
    if (!session.current || paths.length === 0) return;
    const placed = await placeOps(paths, trackId, at, place);
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

  /** Effects > Plug-in Manager. */
  let pluginManagerOpen = $state(false);

  /** Brings what is set in plug-in windows into the session (see plugin-sync.ts). */
  const pluginSync = new PluginSync({
    doc: () => session.current,
    events: () => pluginEvents(),
    state: (id) => pluginState(id),
    commit: async (ops, label = "Plug-in Settings") => {
      await session.edit(label, ops.length === 1 ? ops[0]! : { op: "batch", ops }, sessionHistory);
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

  /** Each plug-in slot's power as last told to its window, so the window's button follows the rack (and undo). */
  const shownEnabled = new Map<string, boolean>();
  function syncPluginPower() {
    const doc = session.current;
    if (!doc || !isTauri()) return;
    for (const s of pluginSlots(doc)) {
      const was = shownEnabled.get(s.id);
      shownEnabled.set(s.id, s.enabled);
      if (was !== undefined && was !== s.enabled) void pluginShowBypassed(s.id, !s.enabled).catch(() => {});
    }
  }

  /** A mixdown with every plug-in's latest settings saved in the session first. */
  async function exportMix(path: string, req: Parameters<typeof exportMixdown>[1]) {
    await pluginSync.captureAll().catch(showError);
    return exportMixdown(path, { ...req, ...(realtimeMix() ? { realtime: true } : {}) });
  }
  /** Settings > Effects: sessions with VST3 plug-ins mix down at playback speed. */
  function realtimeMix(): boolean {
    const doc = session.current;
    return settings.current.realtimePlugins && !!doc && pluginSlots(doc).length > 0;
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

  // Clipboard for clips. Lives for the app session; not the system clipboard.
  let clipBoard: ClipBoard | null = null;
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

  // Long work shows a progress dialog (see export-tools.svelte.ts for exports).
  let exporting = $state<{ title: string; progress: number; floating?: boolean } | null>(null);

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

  /** The track a Host Profile command acts on: the selected one. */
  const profileTrack = () => session.current?.tracks.find((t) => t.id === editor.current().selectedTrack) ?? null;

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

  // Keeping the session's audio in its own folder (Settings > Session Media).
  const ignorePathCase = pathsIgnoreCase();
  const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

  /** Copies `list` into the session folder; maps each path to where it is now. */
  async function copyIntoSession(list: string[], sessionFile: string): Promise<Map<string, string>> {
    if (!list.length) return new Map();
    const r = await mediaCopyIn(sessionFile, settings.current.mediaFolder, list);
    if (r.failed.length) showNotice(`${plural(r.failed.length, "file")} could not be copied into the session folder and stay where they are: ${r.failed.slice(0, 3).join("; ")}`);
    return new Map(list.map((p, i) => [p, r.paths[i] ?? p]));
  }

  mediaPool.setPlacer(async (paths) => {
    const file = session.path;
    const s = settings.current;
    if (!file || s.copyMedia === "never") return paths;
    const list = toCopy(paths, file, s.leaveLibraryFiles ? libraries.current.libs.map((l) => l.dir) : [], ignorePathCase);
    if (!list.length) return paths;
    if (s.copyMedia === "ask") {
      const v = await ask("Copy into the session folder?", `Copy ${plural(list.length, "file")} into the "${s.mediaFolder}" folder beside this session? The session then uses the copies, and the originals stay where they are.`, [
        { label: "Copy", value: "copy", primary: true },
        { label: "Leave Where They Are", value: "leave" },
      ]);
      if (v !== "copy") return paths;
    }
    const moved = await copyIntoSession(list, file);
    return paths.map((p) => moved.get(p) ?? p);
  });

  /**
   * Copies the audio the session uses from outside its folder into it,
   * and points the session and the Files panel at the copies as one
   * undoable step. `everything` includes library files.
   */
  async function collectMedia(sessionFile: string, everything: boolean): Promise<number> {
    const doc = session.current;
    if (!doc) return 0;
    const pool = mediaPool.current.items.map((it) => ({ id: it.id, path: it.path }));
    const leave = everything || !settings.current.leaveLibraryFiles ? [] : libraries.current.libs.map((l) => l.dir);
    const list = toCopy([...doc.media.map((m) => m.path), ...pool.map((p) => p.path)], sessionFile, leave, ignorePathCase);
    const moved = await copyIntoSession(list, sessionFile);
    const plan = repointPlan(doc.media, pool, moved, ignorePathCase);
    const label = "Collect Session Media";
    const parts = [...(plan.ops.length ? [session.command(label, { op: "batch", ops: plan.ops })] : []), ...[mediaPool.repathCommand(plan.pool, label)].filter((c) => c !== null)];
    if (parts.length) await sessionHistory.execute(new CompoundCommand(label, parts));
    return new Set([...plan.pool.map((p) => p.path), ...plan.ops.flatMap((o) => (o.op === "set_media" ? [o.media.path] : []))]).size;
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
      park: () => sessionPark(),
      resume: (id) => sessionResume<SessionDoc>(id),
      forget: (id) => sessionForget(id),
    },
    beforePark: () => pluginSync.captureAll(),
    onSessions: () => updateTitle(),
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
        `Audiform did not close normally. "${r.note.original_path ? displayName(r.note.original_path, null) : r.note.name}" has unsaved work from ${new Date(r.note.saved_at).toLocaleString()}. Recover it?`,
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
    beforeSave: async (target) => {
      const s = settings.current;
      if (!s.collectOnSave || s.copyMedia === "never") return;
      try {
        const n = await collectMedia(target, false);
        if (n) showNotice(`Copied ${plural(n, "file")} into the session folder.`);
      } catch (e) {
        showError(e);
      }
    },
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
  const sessionRows = writable<SessionRow[]>([]);
  function updateSessionRow() {
    const d = session.current;
    const rows: SessionRow[] = [];
    // A never-saved session with nothing in it (files imported only) is not listed as "Untitled Session".
    if (d && !session.isScratch(d)) {
      rows.push({ id: d.id, name: files.name, dirty: files.unsaved, seconds: sessionEnd(d) / Math.max(1, d.sample_rate), sampleRate: d.sample_rate, active: !inWave(), front: true });
    }
    for (const p of files.others) rows.push({ id: p.id, name: displayName(p.path, null) === "Untitled Session" ? p.name : displayName(p.path, null), dirty: p.dirty, seconds: p.seconds, sampleRate: p.sampleRate, active: false, front: false });
    const cur = get(sessionRows);
    if (JSON.stringify(cur) !== JSON.stringify(rows)) sessionRows.set(rows);
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

  /**
   * Closing the app: its prompts come first. Plug-in windows belong to
   * the main window and sit above it, so they step aside while asking,
   * and come back if the user stays.
   */
  async function exitSafely() {
    if (isTauri()) await pluginHideWindows(true).catch(() => {});
    let quitting = false;
    try {
      quitting = await askToExit();
    } finally {
      if (!quitting && isTauri()) void pluginHideWindows(false).catch(() => {});
    }
  }

  /** The unsaved-work prompts before quitting. True when the app is quitting. */
  async function askToExit(): Promise<boolean> {
    if (!(await files.confirmDiscard())) return false;
    const sessionsLeft = files.others.filter((p) => p.dirty).map((p) => p.name);
    if (sessionsLeft.length) {
      const v = await ask(
        "Other sessions have unsaved changes",
        `${sessionsLeft.join(", ")} ${sessionsLeft.length === 1 ? "has" : "have"} unsaved changes. Double-click ${sessionsLeft.length === 1 ? "it" : "each one"} in the Files panel to save, or quit without saving. Recovery copies are kept either way.`,
        [
          { label: "Cancel", value: "cancel", primary: true },
          { label: "Quit Without Saving", value: "quit" },
        ],
      );
      if (v !== "quit") return false;
    }
    if (!(await confirmWaveDiscard())) return false;
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
      if (v !== "quit") return false;
      await forgetParked();
    }
    files.stopAutosave();
    await prefs.flush();
    await exitApp();
    return true;
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
            sessions: sessionRows,
            onOpenSession: (id: string) =>
              void files
                .switchTo(id)
                .then(() => actions.run("view.editor.multitrack"))
                .catch(showError),
            onCloseSession: (id: string) => void files.closeSession(id).catch(showError),
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
        return { component: asComponent(LevelsPanel), props: { transport, wave: waveTransport, preview: previewMeter, session } };
      case "space":
        return { component: asComponent(SpacePanel), props: { session, runner: sessionHistory, onError: showError, onExport: () => void actions.run("file.export.mixdown") } };
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
            onShowChain: () => dock.show("voice-chain"),
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
            onWaveLevel: (db: number) => waveAmplify(db).catch(showError),
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

  /** Files > Remove: the picked files, and their clips if the user says so. */
  async function removeFiles(ids: string[]): Promise<void> {
    const used = usedMedia();
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
  }

  /**
   * Runs `remove`, then closes the Waveform editor's file if it was one of
   * the removed files (asking first when it has unsaved edits).
   */
  async function closeWaveIfRemoved(ids: readonly string[], remove: () => Promise<void>): Promise<void> {
    const open = waveFile.current();
    const target = open && !open.untitled ? open.path.toLowerCase() : null;
    const hit = target !== null && ids.some((id) => mediaPool.get(id)?.path.toLowerCase() === target);
    await remove();
    const gone = hit && !mediaPool.current.items.some((i) => i.path.toLowerCase() === target);
    if (gone && (await confirmWaveDiscard())) await closeWaveFile();
  }

  const actions = new ActionRegistry();

  // Waveform editor. Edits act on the selection; Amplify and Normalize
  // act on the whole file when nothing is selected.
  const inWave = () => editor.current().mode === "waveform";
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
  // Loaded up front so the Effects menu's search finds everything the first time.
  void effectsCatalog()
    .then((c) => (fxCatalog ??= c))
    .catch(() => {});
  /** Effect plug-ins for Effects > Plug-ins (instruments are picked on MIDI tracks). */
  let menuPlugins: MenuPlugin[] = [];
  /** Effects > VST3: the scanned effect plug-ins, by type and by maker, as in the Audio FX Add list. */
  function vst3Popup(): PopupList {
    const items = vst3Items(fxCatalog ?? [], menuPlugins, (id) => void addEffectFromMenu(id)) as ContextItem[];
    if (!items.length) return { items, reason: "No VST3 plug-ins found yet. Scan for them in Effects > Plug-in Manager." };
    if (!fxTarget()) return { items: [], reason: inWave() ? "Open a file in the Waveform editor first." : "Select a track or one clip first." };
    return { items };
  }

  /** The Effects menu's search box: built-in effects and plug-ins by name, added like the menu's own entries. */
  const effectsMenuSearch = {
    placeholder: "Search effects and plug-ins",
    find: (q: string): ContextItem[] => {
      const reason = fxTarget() ? null : inWave() ? "Open a file in the Waveform editor first." : "Select a track or one clip first.";
      return (effectSearch(fxCatalog ?? [], menuPlugins, (id) => void addEffectFromMenu(id))(q) as ContextItem[]).map((r) => (r.kind === "item" && reason ? { ...r, disabled: true, reason } : r));
    },
  };

  async function addEffectFromMenu(effect: string) {
    const at = fxTarget();
    if (!at) return;
    fxCatalog ??= await effectsCatalog();
    const info = fxCatalog.find((e) => e.id === effect);
    if (!info) return;
    const made = newSlot(info, uuid());
    // A plug-in on the master of a 3D Space session runs with its own channels.
    const slot = info.plugin && at.target.kind === "master" && session.current?.space ? { ...made, routing: { wide: true } } : made;
    const next = addSlot(chainOf(at.chain), slot);
    if (!next) {
      showNotice("That Audio FX is full: it holds 16 effects.");
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

  // The Waveform editor's commands (see wave-tools.svelte.ts).
  const waveTools = createWaveTools({
    actions,
    dock,
    showError,
    showNotice,
    ask,
    promptText,
    uuid,
    inWave,
    currentArea,
    selectedClipDocs,
    clipBoard: () => clipBoard,
    exporting: () => exporting,
    setExporting: (v) => (exporting = v),
    recording: () => rec.active,
    finishRecord: () => finishRecord(),
    spaceToggle,
    syncSessionMedia,
    askNewAudio,
    pasteToNewFile,
  });
  const { waveEdit, waveSetSelection, wavePlayhead, confirmWaveDiscard, openInWaveform, saveWave, waveAmplify, applyWaveChain } = waveTools;

  // Exports (see export-tools.svelte.ts).
  const exp = createExportTools({
    actions,
    dock,
    showError,
    showNotice,
    ask,
    inWave,
    timeSel,
    selectedTrackDoc,
    exporting: () => exporting,
    setExporting: (v) => (exporting = v),
    exportMix,
    realtimeMix,
    captureAll: () => pluginSync.captureAll(),
    folderBesideSession,
    placeFiles,
  });
  const { KEY_CHAPTER_FORMAT, mixRange, exportPlaylist, markerDoc, saveMarkers, finishExportDialog, mixDown, bounceRange, bounce, exportStems } = exp;

  // Clip commands (see clip-tools.svelte.ts).
  const clipTools = createClipTools({
    actions,
    dock,
    showError,
    showNotice,
    promptText,
    uuid,
    inWave,
    playheadNow,
    timeSel,
    selectedClipDocs,
    folderBesideSession,
    placeOps,
    exporting: () => exporting,
    set_exporting: (v) => (exporting = v),
    colorFor: () => colorFor,
    set_colorFor: (v) => (colorFor = v),
    fitFor: () => fitFor,
    set_fitFor: (v) => (fitFor = v),
    stretchFor: () => stretchFor,
    set_stretchFor: (v) => (stretchFor = v),
  });
  const { editClips, removeStretch } = clipTools;

  // Edit menu (see edit-tools.svelte.ts).
  const editTools = createEditTools({
    actions,
    dock,
    uuid,
    showNotice,
    promptText,
    selectedTrack,
    timeSel,
    openSettings,
    guarded,
    removeFiles,
    closeWaveIfRemoved,
    clipBoard: () => clipBoard,
    set_clipBoard: (v) => (clipBoard = v),
    shortcutsOpen: () => shortcutsOpen,
    set_shortcutsOpen: (v) => (shortcutsOpen = v),
  });
  const { selectClipActions } = editTools;

  /**
   * Back to the Multitrack editor with unsaved Waveform editor changes to
   * a file the session uses: the session plays files as saved on disk, so
   * offer to save them now.
   */
  let lastMode = editor.current().mode;
  $effect(() =>
    editor.subscribe((e) => {
      const was = lastMode;
      lastMode = e.mode;
      if (was !== "waveform" || e.mode !== "multitrack") return;
      const f = waveFile.current();
      const doc = session.current;
      if (!f || f.untitled || !doc || !waveHistory.current.dirty) return;
      const key = (p: string) => p.replace(/\\/g, "/").toLowerCase();
      if (!doc.media.some((m) => key(m.path) === key(f.path))) return;
      void ask("Save changes for the session?", `"${f.path.split(/[\\/]/).pop()}" is used in this session. The session plays files as they are saved on disk, so your Waveform editor changes show here once the file is saved.`, [
        { label: "Save", value: "save", primary: true },
        { label: "Not Now", value: "later" },
      ]).then((v) => {
        if (v === "save") void saveWave(false);
      });
    }),
  );

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
    // The session may spell the path differently (slashes, case): redraw it under its own spelling too.
    for (const m of used) {
      peaks.invalidate(m.path);
      exactSamples.forget(m.path);
    }
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
  /** The network-prompt offer was made once (or answered); it is not made again on its own. */
  const KEY_FIREWALL_OFFERED = "firewall.offered";

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
  /** Saves the settings and puts them into effect; Apply keeps the dialog open, OK closes it. */
  function applySettingsDraft(d: SettingsDraft, keepOpen = false) {
    if (!keepOpen) settingsDraft = null;
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
  // Aero themes and Phosphor: anything whose own style asks for the hand
  // or the plain arrow gets the theme's arrow too (the theme style sheets
  // style the mark).
  function markAeroCursor(e: MouseEvent) {
    const root = document.documentElement;
    if (!["meadow", "aqua", "moonlight", "prism", "phosphor"].includes(root.dataset.theme ?? "")) return;
    for (let el = e.target as HTMLElement | null; el && el !== document.body; el = el.parentElement) {
      if (el.dataset.aeroCursor !== undefined) return;
      const c = getComputedStyle(el).cursor;
      if (c === "pointer" || c === "default") {
        el.dataset.aeroCursor = "";
        return;
      }
      if (c !== "auto") return;
    }
  }
  onMount(() => {
    document.addEventListener("mouseover", markAeroCursor, true);
    // Unsaved edits in the Waveform editor are heard in the session: a moment
    // after each edit to a file the session uses, a live copy is rendered,
    // and the session plays and draws it until the edits are saved or
    // dropped. One render at a time; the last edit always gets one.
    let liveFor: string | null = null;
    let liveTimer: ReturnType<typeof setTimeout> | undefined;
    let rendering = false;
    let again = false;
    const dropLive = () => {
      if (!liveFor) return;
      const was = liveFor;
      liveFor = null;
      liveCopies.update((m) => Object.fromEntries(Object.entries(m).filter(([k]) => !samePath(k, was))));
      if (isTauri()) void waveLiveClear(was).catch(showError);
    };
    const renderLive = async () => {
      if (rendering) {
        again = true;
        return;
      }
      rendering = true;
      try {
        const c = await waveLiveRender();
        const f = waveFile.current();
        // Saved, dropped or switched while it rendered: not wanted any more.
        if (c && f && samePath(c.original, f.path) && waveHistory.current.dirty) {
          liveFor = c.original;
          liveCopies.update((m) => ({ ...Object.fromEntries(Object.entries(m).filter(([k]) => !samePath(k, c.original))), [c.original]: { path: c.path, length: c.length } }));
        } else if (c) {
          void waveLiveClear(c.original).catch(() => {});
        }
      } catch (e) {
        showError(e);
      } finally {
        rendering = false;
        if (again) {
          again = false;
          void renderLive();
        }
      }
    };
    const followLive = () => {
      clearTimeout(liveTimer);
      const f = waveFile.current();
      const dirty = waveHistory.current.dirty;
      if (liveFor && (!f || !samePath(liveFor, f.path) || !dirty)) dropLive();
      const used = !!f && !f.untitled && (session.current?.media ?? []).some((m) => samePath(m.path, f.path));
      if (!isTauri() || !f || !dirty || !used) return;
      liveTimer = setTimeout(() => void renderLive(), 600);
    };
    const stopLive = [waveHistory.subscribe(followLive), waveFile.subscribe(followLive)];
    // Switched-off channels in the Waveform editor are silent in playback too.
    const stopChannels = waveChannels.subscribe((c) => {
      if (isTauri()) void waveSetChannelsOff(c.map((on) => !on)).catch(showError);
    });
    const stopPointers = watchThemeCursor();
    return () => {
      document.removeEventListener("mouseover", markAeroCursor, true);
      stopPointers();
      stopChannels();
      for (const off of stopLive) off();
      clearTimeout(liveTimer);
    };
  });

  /** Puts settings that live outside the interface's own reads into effect. */
  function applyAppSettings(s: AppSettings, autosave: boolean) {
    if (s.theme !== "dark") document.documentElement.dataset.theme = s.theme;
    else delete document.documentElement.dataset.theme;
    const root = document.documentElement.style;
    for (const k of ["--wave", "--playhead", "--accent", "--lane-selected"]) root.removeProperty(k);
    for (const [k, v] of Object.entries(colorVars(s.colors))) root.setProperty(k, v);
    root.setProperty("--meter-yellow", `${meterPercent(s.meterYellowDb)}%`);
    root.setProperty("--meter-red", `${meterPercent(s.meterRedDb)}%`);
    // Canvas views read colors when they draw: have them draw again.
    requestAnimationFrame(() => appearanceChanged());
    setCustomTarget({ lufs: s.customLufs, peak: s.customPeak });
    setMeterSpeed(s.meterSpeed);
    if (isTauri()) {
      // Driver first, so the output name is looked up on the right one.
      void setAudioDriver(s.driver === "asio")
        .then(() => setAudioOutput(s.output))
        .then(() => setOutputMap(s.outputPair, s.speakerOutputs))
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
    if (rec.active) return finishRecord();
    return t.current.playing ? t.stop() : t.toggle();
  }

  // Recording (see recorder.svelte.ts).
  const rec = createRecorder({ showError, showNotice, inWave, playheadNow, timeSel, waveEdit, waveSetSelection, wavePlayhead, uuid });
  const toggleRecord = rec.toggle;
  const finishRecord = rec.finish;
  const syncMonitor = rec.syncMonitor;
  const recordOptions = rec.options;
  recordControl.toggle = () => void toggleRecord();

  // View, Window and Help menus (see view-tools.svelte.ts).
  const viewTools = createViewTools({
    actions,
    dock,
    inWave,
    showNotice,
    promptText,
    ask,
    timeSel,
    shortcutsOpen: () => shortcutsOpen,
    set_shortcutsOpen: (v) => (shortcutsOpen = v),
    aboutOpen: () => aboutOpen,
    set_aboutOpen: (v) => (aboutOpen = v),
    wsState: () => wsState,
    set_wsState: (v) => (wsState = v),
    openPanels: () => openPanels,
    set_openPanels: (v) => (openPanels = v),
  });

  // Effects menu (see effects-tools.svelte.ts).
  const effectsTools = createEffectsTools({
    actions,
    showNotice,
    showError,
    ask,
    uuid,
    inWave,
    dock,
    fxTarget,
    addEffectFromMenu,
    openLoudnessMatch,
    KEY_FIREWALL_OFFERED,
    fxCatalog: () => fxCatalog,
    set_fxCatalog: (v) => (fxCatalog = v),
    lm: () => lm,
    set_lm: (v) => (lm = v),
    exporting: () => exporting,
    set_exporting: (v) => (exporting = v),
    generateMode: () => generateMode,
    set_generateMode: (v) => (generateMode = v),
    pluginManagerOpen: () => pluginManagerOpen,
    set_pluginManagerOpen: (v) => (pluginManagerOpen = v),
  });
  const { offerFirewallRule } = effectsTools;

  // Multitrack menu (see multitrack-tools.svelte.ts).
  const multitrackTools = createMultitrackTools({
    actions,
    dock,
    uuid,
    showNotice,
    showError,
    inWave,
    selectedTrack,
    timeSel,
    nextTrackName,
    profileTrack,
    withUnzipped,
    stem,
    spaceToggle,
    toggleRecord,
    finishRecord,
    placeFiles,
    rec,
    balancing: () => balancing,
    set_balancing: (v) => (balancing = v),
    propertiesOpen: () => propertiesOpen,
    set_propertiesOpen: (v) => (propertiesOpen = v),
    addTracksOpen: () => addTracksOpen,
    set_addTracksOpen: (v) => (addTracksOpen = v),
    generateMode: () => generateMode,
    set_generateMode: (v) => (generateMode = v),
    openPanels: () => openPanels,
    set_openPanels: (v) => (openPanels = v),
  });
  const { KEY_RETURN_ON_STOP, visibilityActions } = multitrackTools;

  // File menu (see file-tools.svelte.ts).
  const fileTools = createFileTools({
    actions,
    showNotice,
    showError,
    promptText,
    openPaths,
    importPaths,
    exitSafely,
    measureLoudness,
    KEY_LOUD_TARGET,
    KEY_LOUD_REGION,
    exp,
    collectMedia,
    plural,
    usedMedia,
    closeWaveIfRemoved,
    askNewSession,
    pluginSync,
    files,
    KEY_CHAPTER_FORMAT,
    markerDoc,
    exportStems,
    loud: () => loud,
    set_loud: (v) => (loud = v),
    exporting: () => exporting,
    set_exporting: (v) => (exporting = v),
  });

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

  /** Paste to New File: the Waveform clipboard in the Waveform editor, otherwise the copied clips. */
  async function pasteToNewFile() {
    try {
      const waveBoard = waveTools.board;
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
  function registerActions(): (() => void)[] {
    const offs = [
    ];
    offs.push(
      ...visibilityActions(),
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
          await placeFiles(paths, editor.current().selectedTrack, sessionView()?.playhead ?? 0);
        },
        enabled: () => session.current !== null && mediaPool.selectedItems().some((x) => x.status === "ready"),
        reason: () => (session.current ? "Select files in the Files panel first." : "Open or create a session first."),
      }),
      ...(
        [
          ["file.export.mixdown", "all", "file"],
          ["file.export.selection", "selection", "file"],
          ["multitrack.mixdown.all", "all", "files-panel"],
          ["multitrack.mixdown.selection", "selection", "files-panel"],
          ["multitrack.mixdown.clips", "clips", "files-panel"],
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
      ...(
        [
          ["multitrack.bounce.track", "track", "Select a track with clips first."],
          ["multitrack.bounce.selection", "selection", "Drag across empty space or the ruler to select a time range."],
          ["multitrack.bounce.clips-selection", "clips-selection", "Select clips and a time range first."],
          ["multitrack.bounce.clips", "clips", "Select clips first."],
        ] as const
      ).map(([id, source, why]) =>
        actions.register({
          id,
          run: () => bounce(source),
          enabled: () => exporting === null && contentEnd() > 0 && bounceRange(source) !== null,
          reason: () => (contentEnd() === 0 ? "The session has no clips yet." : why),
        }),
      ),
      ...selectClipActions(),
    );
    offs.push(
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
    offs.push(...clipTools.register());
    offs.push(...editTools.register());
    offs.push(...multitrackTools.register());
    offs.push(...fileTools.register());
    offs.push(...effectsTools.register());
    offs.push(...viewTools.register());
    return offs;
  }

  // Effect envelopes need each control's range; plug-ins change after a scan.
  // Running plug-ins are kept in step with the session while the app is open.
  $effect(() => {
    pluginSync.start();
    const stopCatalog = pluginsVersion.subscribe(() => {
      void pluginsRegistry()
        .then((r) => {
          menuPlugins = r.plugins
            .filter((p) => !p.instrument)
            .map((p) => ({ effect: p.effect, name: p.name, vendor: p.vendor, kind: p.kind }));
          actions.invalidate();
        })
        .catch(() => {});
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
    offs.push(...waveTools.register());
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
        transport.returnOnStop = prefs.raw(KEY_RETURN_ON_STOP) === true;
        // Once, on Windows with plug-ins scanned: offer to stop the network prompts if nothing stops them yet.
        if (PLATFORM === "windows" && prefs.raw(KEY_FIREWALL_OFFERED) !== true)
          void pluginsRegistry()
            .then((r) => (r.plugins.length ? offerFirewallRule(true) : undefined))
            .catch(() => {});
        settings.set(sanitizeSettings(prefs.raw(KEY_SETTINGS)));
        applyAppSettings(settings.current, false);
        offs.push(startMetronome());
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
    offs.push(session.subscribe(() => syncPluginPower()));
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
    // Held Left and Right arrows scrub (the project lead's choice of keys).
    offs.push(
      bindScrub(() => {
        const w = inWave();
        const d = w ? wave.current : session.current;
        if (!d) return null;
        return {
          transport: w ? waveTransport : transport,
          playhead: () => (w ? wave.current : session.current)?.view.playhead ?? 0,
          end: () => { const x = w ? wave.current : session.current; return x ? sessionEnd(x) : 0; },
          rate: () => d.sample_rate,
        };
      }),
    );
    return () => offs.forEach((off) => off());
  });
</script>

<div class="shell">
  <header class="top">
    <MenuBar menus={MENUS} {actions} {shortcutFor} onError={showError} popups={{ "fx.vst3": vst3Popup }} searches={{ Effects: effectsMenuSearch }} />
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
    beforeRestart={() => pluginSync.captureAll()}
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

{#if rec.countdown !== null}
  <div class="countdown" role="status" aria-live="assertive">Recording in {rec.countdown}<span>Press Record again to cancel</span></div>
{/if}

{#if settingsDraft}
  <SettingsDialog initial={settingsDraft} onApply={(d, keep) => applySettingsDraft(d, keep)} onNotice={showNotice} onClose={() => (settingsDraft = null)} />
{/if}

{#if rec.optionsOpen}
  <RecordOptionsDialog
    initial={recordOptions()}
    onApply={(o) => {
      prefs.set(KEY_RECORD, o);
      rec.optionsOpen = false;
    }}
    onClose={() => (rec.optionsOpen = false)}
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

{#if exp.markersExport && markerDoc()}
  <ExportMarkersDialog
    markers={markerDoc()!.markers}
    rate={markerDoc()!.sample_rate}
    format={exp.markersExport.format}
    onFormat={(f) => {
      prefs.set(KEY_CHAPTER_FORMAT, f);
      if (exp.markersExport) exp.markersExport = { format: f };
    }}
    onSave={(content, f) => void saveMarkers(content, f).catch(showError)}
    onClose={() => (exp.markersExport = null)}
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

{#if exp.exportReq}
  <ExportDialog
    initial={exp.exportReq.initial}
    scopes={exp.exportReq.scopes}
    source={exp.exportReq.source}
    chapterCount={session.current?.markers.filter((m) => m.kind === "chapter").length ?? 0}
    spaceLayout={session.current?.space?.layout ?? null}
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
