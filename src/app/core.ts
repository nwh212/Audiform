// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

/**
 * Typed wrappers around Tauri commands. Components call these,
 * never `invoke` directly, so the command surface stays in one place.
 */

import type { OutputFormat } from "./output-format";
import { invoke } from "@tauri-apps/api/core";
import type { EffectChainDoc, MediaRefDoc } from "../stores/session";
import type { SpectralMask } from "../panels/editor/spectrum";

export interface AppInfo {
  version: string;
  sessionSchemaVersion: number;
}

export function getAppInfo(): Promise<AppInfo> {
  return invoke<AppInfo>("app_info");
}

/** Quits the app. */
export function exitApp(): Promise<void> {
  return invoke<void>("app_exit");
}

/** Saved panel layout, or null when none has been saved. */
export function loadWorkspaceLayout(): Promise<unknown | null> {
  return invoke<unknown | null>("workspace_load");
}

export function saveWorkspaceLayout(layout: unknown): Promise<void> {
  return invoke<void>("workspace_save", { layout });
}

export function clearWorkspaceLayout(): Promise<void> {
  return invoke<void>("workspace_clear");
}

/** Minimizes the main window. */
export function minimizeWindow(): Promise<void> {
  return invoke<void>("app_minimize");
}

/** The user's saved shortcut changes. */
export function loadKeymapOverrides(): Promise<Record<string, string[]>> {
  return invoke<Record<string, string[]>>("keymap_load");
}

export function saveKeymapOverrides(bindings: Record<string, string[]>): Promise<void> {
  return invoke<void>("keymap_save", { bindings });
}

/** Opens a thegcrn.com page in the default browser. Other sites are refused by the core. */
export function openLink(url: string): Promise<void> {
  return invoke<void>("open_link", { url });
}

/** What the core reports about one audio file. */
export interface AudioInfo {
  path: string;
  name: string;
  /** Container, for example "WAV" or "M4A". */
  format: string;
  /** Codec, for example "PCM 24-bit" or "MP3". */
  codec: string;
  sampleRate: number;
  channels: number;
  /** Length in sample frames at the file's own rate. */
  length: number;
  /** Bits per sample for lossless files, otherwise null. */
  bitDepth: number | null;
  fileSize: number;
}

/** Expands folders into the audio files inside them. Files pass through. */
export function expandMediaPaths(paths: string[]): Promise<string[]> {
  return invoke<string[]>("media_expand", { paths });
}

/** Identifies and measures one audio file. Rejects with a user-facing message. */
export function probeMedia(path: string): Promise<AudioInfo> {
  return invoke<AudioInfo>("media_probe", { path });
}

/** Shows the Import Files dialog. Resolves to [] on cancel. */
export function pickMediaFiles(): Promise<string[]> {
  return invoke<string[]>("media_pick_files");
}

/** Opens the system file manager with the file selected. */
export function revealMedia(path: string): Promise<void> {
  return invoke<void>("media_reveal", { path });
}

/** Shows the Import Folder dialog. Resolves to null on cancel. */
export function pickMediaFolder(): Promise<string | null> {
  return invoke<string | null>("media_pick_folder");
}

/** A drive or common folder for the Drive Manager. */
export interface BrowseLocation {
  path: string;
  label: string;
  kind: "drive" | "place";
}

/** One row of a folder listing. */
export interface BrowseEntry {
  name: string;
  path: string;
  isDir: boolean;
  size: number;
  /** Milliseconds since 1970, or null. */
  modified: number | null;
  /** An audio file the app can open. */
  audio: boolean;
}

export function browseLocations(): Promise<BrowseLocation[]> {
  return invoke<BrowseLocation[]>("browse_locations");
}

export function browseList(path: string, audioOnly: boolean): Promise<BrowseEntry[]> {
  return invoke<BrowseEntry[]>("browse_list", { path, audioOnly });
}

export interface PreviewStatus {
  playing: boolean;
  /** Highest sample left and right since the last status, 0 to 1. */
  peak: [number, number];
  path: string | null;
  /** Seconds from the start of the file. */
  position: number;
}

/** Plays a file on the default output device, stopping any other preview. */
export function previewPlay(path: string, start = 0): Promise<void> {
  return invoke<void>("preview_play", { path, start });
}

export function previewStop(): Promise<void> {
  return invoke<void>("preview_stop");
}

export function previewStatus(): Promise<PreviewStatus> {
  return invoke<PreviewStatus>("preview_status");
}

export function loadPreferences(): Promise<Record<string, unknown>> {
  return invoke<Record<string, unknown>>("preferences_load");
}

export function savePreferences(values: Record<string, unknown>): Promise<void> {
  return invoke<void>("preferences_save", { values });
}

/** A library index (`library.json`). Field names match the file format. */
export interface Library {
  format: "altaudio.library";
  schema_version: number;
  id: string;
  name: string;
  scope: "show" | "network";
  created: string;
  items: LibraryItem[];
  collections: LibraryCollection[];
  /** Show badge color (palette token or #rrggbb). */
  color?: string | null;
}

export interface LibraryItem {
  id: string;
  name: string;
  kind: "audio";
  /** Relative to the library folder when copied in, absolute when linked. */
  file: string;
  copied: boolean;
  original_path: string | null;
  sample_rate: number;
  channels: number;
  /** Samples at the file's own rate. */
  length: number;
  tags: string[];
  notes: string;
  color: string | null;
  added: string;
}

/** A bin: a named group of library items. An item can be in several. */
export interface LibraryCollection {
  id: string;
  name: string;
  items: string[];
  /** Palette token or #rrggbb. */
  color?: string | null;
}

export interface LibraryImportResult {
  items: LibraryItem[];
  skipped: { path: string; reason: string }[];
}

export function libraryDefaultRoot(): Promise<string> {
  return invoke<string>("library_default_root");
}

export function libraryCreate(dir: string, name: string): Promise<Library> {
  return invoke<Library>("library_create", { dir, name });
}

export function libraryOpen(dir: string): Promise<Library> {
  return invoke<Library>("library_open", { dir });
}

export function librarySave(dir: string, library: Library): Promise<void> {
  return invoke<void>("library_save", { dir, library });
}

export function libraryImport(dir: string, paths: string[], copy: boolean): Promise<LibraryImportResult> {
  return invoke<LibraryImportResult>("library_import", { dir, paths, copy });
}

/** Deletes unused copied audio files, or with `dryRun` only counts them. */
export function libraryCleanUp(dir: string, library: Library, dryRun: boolean): Promise<number> {
  return invoke<number>("library_clean_up", { dir, library, dryRun });
}

export function libraryPickFolder(title: string): Promise<string | null> {
  return invoke<string | null>("library_pick_folder", { title });
}

export interface PeakSummary {
  sampleRate: number;
  channels: number;
  length: number;
  levels: { block: number; count: number }[];
}

/** Builds the waveform cache for a file if needed. Progress arrives as `peaks-progress` events. */
export function peaksEnsure(path: string): Promise<PeakSummary> {
  return invoke<PeakSummary>("peaks_ensure", { path });
}

/** One cached level: little-endian i16 min/max pairs per channel per block. */
export function peaksLevel(path: string, level: number): Promise<ArrayBuffer> {
  return invoke<ArrayBuffer>("peaks_level", { path, level });
}

/** The Waveform editor's file: its edit list as a one-track session. */
export interface WaveDocRaw<S> {
  session: S;
  path: string;
  /** Container, for example "WAV". Only WAV files save in place. */
  format: string;
  bit_depth: number | null;
  /** Made by Paste to New File and not saved anywhere yet. */
  untitled: boolean;
}

/** Paste to New File: renders clips (from 0) into a new untitled file and opens it in the Waveform editor. */
export function waveNewFromClips<S, C, M>(name: string, sampleRate: number, channels: number, clips: C[], media: M[]): Promise<WaveDocRaw<S>> {
  return invoke<WaveDocRaw<S>>("wave_new_from_clips", { name, sampleRate, channels, clips, media });
}

/** File > New Audio File: an empty untitled file in the Waveform editor. */
export function waveNew<S>(name: string, sampleRate: number, channels: number, bitDepth: number): Promise<WaveDocRaw<S>> {
  return invoke<WaveDocRaw<S>>("wave_new", { name, sampleRate, channels, bitDepth });
}

export function waveOpen<S>(path: string): Promise<WaveDocRaw<S>> {
  return invoke<WaveDocRaw<S>>("wave_open", { path });
}

export function waveGet<S>(): Promise<WaveDocRaw<S> | null> {
  return invoke<WaveDocRaw<S> | null>("wave_get");
}

export function waveClose(): Promise<void> {
  return invoke<void>("wave_close");
}

/** Sets the open Waveform file aside, unsaved edits kept in memory. */
export function wavePark(): Promise<void> {
  return invoke<void>("wave_park");
}

/** Brings back a set-aside file; null when `path` was not set aside. */
export function waveResume<S>(path: string): Promise<WaveDocRaw<S> | null> {
  return invoke<WaveDocRaw<S> | null>("wave_resume", { path });
}

/** Drops a set-aside file and its unsaved edits. */
export function waveForget(path: string): Promise<void> {
  return invoke<void>("wave_forget", { path });
}

export function waveApply<O, T>(op: O): Promise<{ session: T; inverse: O }> {
  return invoke<{ session: T; inverse: O }>("wave_apply", { op });
}

export function waveSetView<V>(view: V): Promise<void> {
  return invoke<void>("wave_set_view", { view });
}

export function wavePlay(from?: number, mode: PlayMode = "normal"): Promise<void> {
  return invoke<void>("wave_play", { from: from ?? null, mode });
}

/** Saves the edited file: in place when `path` is null, else as a new file. Reopens the saved file. */
export function waveSave<S>(path: string | null, bitDepth: number | null): Promise<WaveDocRaw<S>> {
  return invoke<WaveDocRaw<S>>("wave_save", { path, bitDepth });
}

/** Highest absolute sample of the edited file in `range` (whole file when null). */
export function wavePeak(range: [number, number] | null): Promise<number> {
  return invoke<number>("wave_peak", { range });
}

export function sessionGet<T>(): Promise<T> {
  return invoke<T>("session_get");
}

export function sessionApply<O, T>(op: O): Promise<{ session: T; inverse: O }> {
  return invoke<{ session: T; inverse: O }>("session_apply", { op });
}

export function sessionSetView<V>(view: V): Promise<void> {
  return invoke<void>("session_set_view", { view });
}

/** Result of new, open, save and recover. */
export interface SessionResult<T> {
  session: T;
  path: string | null;
  /** Media ids not found on disk. */
  missing: string[];
}

export function sessionNew<T>(options: NewSessionOptions = {}): Promise<SessionResult<T>> {
  return invoke<SessionResult<T>>("session_new", { options });
}

/** Saves the session. `backups` older copies are kept in a Backups folder beside it (0 keeps none). */
export function sessionSave<T, M>(path: string | null, media: M[], libraries: string[], backups = 0): Promise<SessionResult<T>> {
  return invoke<SessionResult<T>>("session_save", { path, media, libraries, backups });
}

export function sessionOpen<T>(path: string): Promise<SessionResult<T>> {
  return invoke<SessionResult<T>>("session_open", { path });
}

export function sessionAutosave<M>(media: M[]): Promise<void> {
  return invoke<void>("session_autosave", { media });
}

export function sessionClearAutosave(): Promise<void> {
  return invoke<void>("session_clear_autosave");
}

export interface Recoverable {
  autosave: string;
  note: { session_id: string; name: string; original_path: string | null; saved_at: string };
}

export function sessionRecoverable(): Promise<Recoverable[]> {
  return invoke<Recoverable[]>("session_recoverable");
}

export function sessionRecover<T>(autosave: string): Promise<SessionResult<T>> {
  return invoke<SessionResult<T>>("session_recover", { autosave });
}

export function sessionDiscardRecovery(sessionId: string): Promise<void> {
  return invoke<void>("session_discard_recovery", { sessionId });
}

export function sessionPickSave(name: string): Promise<string | null> {
  return invoke<string | null>("session_pick_save", { name });
}

export function sessionPickOpen(sesx = false): Promise<string | null> {
  return invoke<string | null>("session_pick_open", { sesx });
}

/** File > Open picker: sessions, .sesx sessions and audio files. Empty on cancel. */
export function filePickOpen(): Promise<string[]> {
  return invoke<string[]>("file_pick_open");
}

export function setWindowTitle(title: string): Promise<void> {
  return invoke<void>("app_set_title", { title });
}

export interface SesxReport {
  tracks: number;
  clips: number;
  skippedEffects: number;
  missingFiles: string[];
}

/** Imports a .sesx session as the new, unsaved session. */
export function sessionImportSesx<T>(path: string): Promise<SessionResult<T> & { report: SesxReport }> {
  return invoke<SessionResult<T> & { report: SesxReport }>("session_import_sesx", { path });
}

export interface TransportStatus {
  playing: boolean;
  /** Session samples. */
  position: number;
  trackIds: string[];
  /** Peak per track since the last call, 0 to 1. */
  meters: [number, number][];
  master: [number, number];
  underruns: number;
  /** Per track, its Voice Chain's peaks for the audio just heard: left in, right in, left out, right out. Null when the track's chain does nothing. */
  chainMeters?: ([number, number, number, number] | null)[];
  /** Buses, with their levels after the fader and their chains' peaks. Empty unless the mix is routed. */
  busIds?: string[];
  busMeters?: [number, number][];
  busChainMeters?: ([number, number, number, number] | null)[];
  masterChainMeter?: [number, number, number, number] | null;
}

/** How playback treats the time selection. */
export type PlayMode = "normal" | "loop" | "skip";

export function transportPlay(from?: number, mode: PlayMode = "normal"): Promise<void> {
  return invoke<void>("transport_play", { from: from ?? null, mode });
}

/** Stops playback. Resolves to the position reached, or null if it was not playing. */
export function transportStop(): Promise<number | null> {
  return invoke<number | null>("transport_stop");
}

/** Mono check: hear everything folded to mono. Listening only. */
export function setMonoCheck(on: boolean): Promise<void> {
  return invoke<void>("transport_set_mono_check", { on });
}

export function transportStatus(): Promise<TransportStatus> {
  return invoke<TransportStatus>("transport_status");
}

export interface MixdownRequest {
  /** Start and end in session samples. Omit for the whole session. */
  range?: [number, number];
  /** Only these clips. Omit for all. */
  clips?: string[];
  /** 16, 24 or 32 (float). Omit for the session bit depth. */
  bitDepth?: number;
  /** A stem of one track, the length of the whole session. */
  track?: string;
  /** File format, rate and quality; unset writes a WAV at the session rate. */
  format?: OutputFormat;
  /** Only what belongs to the clips: track volume, pan and the Master are left out. */
  clipOnly?: boolean;
  /** Leave every Voice Chain out (measuring the raw audio). */
  dry?: boolean;
}

export interface MixdownResult {
  frames: number;
  sampleRate: number;
  bitDepth: number;
  /** Highest absolute sample; above 1 means the mix clipped. */
  peak: number;
  cancelled: boolean;
}

/** Save dialog for a mixdown; `ext` is "wav" (default), "flac" or "mp3". Null on cancel. */
export function exportPickPath(
  name: string,
  folder: string | null,
  title: string | null = null,
  ext: string | null = null,
  all = false,
): Promise<string | null> {
  return invoke<string | null>("export_pick_path", { name, folder, title, ext, all });
}

/** Save dialog for a text file, then writes `content` there. Null when cancelled. */
export function exportText(name: string, folder: string | null, title: string, filter: string, ext: string, content: string): Promise<string | null> {
  return invoke<string | null>("export_text", { name, folder, title, filter, ext, content });
}

/** Which of `paths` already exist. */
export function pathsExist(paths: string[]): Promise<boolean[]> {
  return invoke<boolean[]>("paths_exist", { paths });
}

/** Renders the open session to a file. Progress arrives as `export-progress` events. */
export function exportMixdown(path: string, request: MixdownRequest): Promise<MixdownResult> {
  return invoke<MixdownResult>("export_mixdown", {
    path,
    request: {
      range: request.range ?? null,
      clips: request.clips ?? null,
      bit_depth: request.bitDepth ?? null,
      track: request.track ?? null,
      format: request.format ?? null,
    },
  });
}

export interface MergeResult extends MixdownResult {
  path: string;
}

/**
 * Merge Clips: renders the given clips with only their own settings
 * (gain, fades, envelopes) to a new 32-bit float WAV in `folder`, named
 * after `name` without overwriting anything.
 */
export function exportMerge(folder: string, name: string, request: MixdownRequest): Promise<MergeResult> {
  return invoke<MergeResult>("export_merge", {
    folder,
    name,
    request: { range: request.range ?? null, clips: request.clips ?? null, bit_depth: null, track: request.track ?? null },
  });
}

/** Make Unique Copy: copies source frames `from..to` of `source`, untouched, to a new WAV in `folder`. Returns its path. */
export function exportRegionCopy(source: string, from: number, to: number, folder: string, name: string): Promise<string> {
  return invoke<string>("export_region_copy", { source, from, to, folder, name });
}

/** Folder dialog for track stems. Null on cancel. */
export function exportPickFolder(folder: string | null, title: string | null = null): Promise<string | null> {
  return invoke<string | null>("export_pick_folder", { folder, title });
}

/** File > New Session choices. Everything is optional. */
export interface NewSessionOptions {
  name?: string;
  sample_rate?: number;
  bit_depth?: number;
  mono?: boolean;
  /** Start from this template file. */
  template?: string | null;
  /** Create the session in its own folder here and save it at once. */
  folder?: string | null;
}

export interface TemplateEntry {
  path: string;
  name: string;
  description: string;
  tracks: number;
  clips: number;
  sample_rate: number;
}

export function templateList(): Promise<TemplateEntry[]> {
  return invoke<TemplateEntry[]>("template_list");
}

export function templateSave(name: string, description: string | null = null): Promise<TemplateEntry> {
  return invoke<TemplateEntry>("template_save", { name, description });
}

export function templateDelete(path: string): Promise<void> {
  return invoke<void>("template_delete", { path });
}

/** Documents/Altered Audio/Sessions. */
export function sessionDefaultFolder(): Promise<string> {
  return invoke<string>("session_default_folder");
}

/** Measures the mix's loudness. Progress arrives as `export-progress`; `exportCancel` stops it. */
export function loudnessCheck(request: MixdownRequest): Promise<import("./loudness").LoudnessReport> {
  return invoke("loudness_check", {
    request: { range: request.range ?? null, clips: request.clips ?? null, bit_depth: null, track: request.track ?? null, clip_only: request.clipOnly ?? false, dry: request.dry ?? false },
  });
}

export function exportCancel(): Promise<void> {
  return invoke<void>("export_cancel");
}

export interface WorkspaceInfo {
  id: string;
  name: string;
  builtIn: boolean;
}

export function workspaceList(): Promise<WorkspaceInfo[]> {
  return invoke<WorkspaceInfo[]>("workspace_list");
}

export function workspaceGet(id: string): Promise<unknown | null> {
  return invoke<unknown | null>("workspace_get", { id });
}

export function workspacePut(id: string, name: string, builtIn: boolean, layout: unknown): Promise<void> {
  return invoke<void>("workspace_put", { id, name, builtIn, layout });
}

export function workspaceDelete(id: string): Promise<void> {
  return invoke<void>("workspace_delete", { id });
}

export interface AudioInput {
  name: string;
  channels: number;
  isDefault: boolean;
}

/** The computer's audio inputs, for the track input picker. Names only; nothing is opened. */
export async function audioInputs(asio?: boolean): Promise<AudioInput[]> {
  const raw = await invoke<{ name: string; channels: number; is_default: boolean }[]>("audio_inputs", { asio: asio ?? null });
  return raw.map((d) => ({ name: d.name, channels: d.channels, isDefault: d.is_default }));
}

/** The spectrogram of the Waveform editor's file between two samples: `rows` rows of `columns` bytes, top row highest. */
export async function waveSpectrogram(start: number, end: number, columns: number, rows: number): Promise<Uint8Array> {
  const buf = await invoke<ArrayBuffer>("wave_spectrogram", { start, end, columns, rows });
  return new Uint8Array(buf);
}

export interface SpectralResult {
  media: MediaRefDoc;
  start: number;
  end: number;
}

/** Spectral edit inside `mask`: "gain" turns it down by `gainDb` (-120 or lower removes it), "repair" fills it from the audio around it. */
export function waveSpectralEdit(mask: SpectralMask, mode: "gain" | "repair", gainDb?: number): Promise<SpectralResult> {
  return invoke<SpectralResult>("wave_spectral_edit", { mask, mode, gainDb: gainDb ?? null });
}

/** Renders `start..end` of the Waveform editor's file through `chain` into a new file, for replaceRange. */
/** Waveform editor Stretch: `start..end` made `frames` long, pitch kept. */
/** Reverse or invert `start..end` of the Waveform editor's file; the result swaps in with replaceRange. */
export function waveReverseInvert(start: number, end: number, mode: "reverse" | "invert"): Promise<SpectralResult> {
  return invoke<SpectralResult>("wave_reverse_invert", { start, end, mode });
}

export function waveStretch(start: number, end: number, frames: number): Promise<SpectralResult> {
  return invoke<SpectralResult>("wave_stretch", { start, end, frames });
}

export function waveApplyChain(chain: import("../stores/session").EffectChainDoc, start: number, end: number): Promise<SpectralResult> {
  return invoke<SpectralResult>("wave_apply_chain", { chain, start, end });
}

/** Pitch of the Waveform editor's file at `columns` points between two samples, in Hz (0 where unclear). */
export function wavePitch(start: number, end: number, columns: number): Promise<number[]> {
  return invoke<number[]>("wave_pitch", { start, end, columns });
}

export interface RecordTarget {
  /** Track id, or "wave" for the Waveform editor. */
  key: string;
  /** The track's input picker value; null is the default input. */
  input: string | null;
  channels: number;
  /** File name without extension. */
  name: string;
  /** Also play the input through the speakers. */
  monitor?: boolean;
}

export interface Take {
  key: string;
  path: string;
  sample_rate: number;
  channels: number;
  length: number;
}

export interface LiveLevel {
  peak: number;
  /** Channels per block (1 or 2). */
  channels: number;
  /** Per block of LIVE_BLOCK frames, per channel: min then max, 16-bit, as the peak cache stores them. */
  blocks: number[];
}

/** Frames per live waveform block (matches the core, and the finest level of a finished file's peaks). */
export const LIVE_BLOCK = 32;

export interface RecordStatus {
  recording: boolean;
  monitoring: boolean;
  rate: number;
  live: Record<string, LiveLevel>;
  seconds: number;
  peak: number;
  overruns: number;
  /** Why input monitoring could not start, when it was asked for. */
  monitorNote?: string;
}

/** Starts recording. Returns the folder the takes go in. */
export function recordStart(targets: RecordTarget[]): Promise<string> {
  return invoke<string>("record_start", { targets });
}

/** Stops recording and returns the takes. */
export function recordStop(): Promise<Take[]> {
  return invoke<Take[]>("record_stop");
}

export function recordStatus(): Promise<RecordStatus> {
  return invoke<RecordStatus>("record_status");
}

/** The speakers and headphones, the system default first. */
export function audioOutputs(asio?: boolean): Promise<{ name: string; isDefault: boolean }[]> {
  return invoke("audio_outputs", { asio: asio ?? null });
}

/** Plays through `name` from the next time playback starts; null uses the system default. */
export interface AudioDriver {
  /** This build can use ASIO (Windows release builds). */
  asioBuilt: boolean;
  /** ASIO is in use now. */
  asio: boolean;
  /** Installed ASIO drivers. */
  drivers: string[];
}

export function audioDriver(): Promise<AudioDriver> {
  return invoke<AudioDriver>("audio_driver");
}

/** ASIO (true) or the system driver (false), from the next stream opened. */
export function setAudioDriver(asio: boolean): Promise<void> {
  return invoke<void>("set_audio_driver", { asio });
}

export function setAudioOutput(name: string | null): Promise<void> {
  return invoke<void>("set_audio_output", { name });
}

export interface CacheInfo {
  path: string;
  folders: { name: string; bytes: number; clearable: boolean }[];
}

/** Where the app keeps its working files, and how big each folder is. */
export function cacheInfo(): Promise<CacheInfo> {
  return invoke<CacheInfo>("cache_info");
}

/** Deletes the saved waveform drawings (rebuilt when needed). Resolves to the bytes freed. */
export function cacheClearPeaks(): Promise<number> {
  return invoke<number>("cache_clear_peaks");
}

/** Uses `path` for working files from now on; null goes back to the default folder in Documents. */
export function cacheSetFolder(path: string | null): Promise<void> {
  return invoke<void>("cache_set_folder", { path });
}

/** Opens the system's sound settings (inputs and outputs). */
export function openSoundSettings(): Promise<void> {
  return invoke<void>("open_sound_settings");
}

/** Opens the armed tracks' inputs for their meters. An empty list closes them. */
export function recordMonitor(targets: RecordTarget[]): Promise<void> {
  return invoke<void>("record_monitor", { targets });
}

/** One file for Batch Convert. `id` ties progress events back to the list row. */
export interface BatchJob {
  id: string;
  source: string;
  dest: string;
  /** Tags to write over any copied ones. */
  tags?: Partial<Record<"title" | "artist" | "album" | "year" | "genre" | "comment", string>> | null;
}

export interface BatchProgress {
  id: string;
  status: "running" | "done" | "failed" | "stopped";
  /** Processing, Converting, Saved or Not saved. */
  stage: string;
  /** 0 to 1 while running. */
  progress: number;
  error: string | null;
  dest: string;
}

export interface BatchSummary {
  done: number;
  failed: number;
  stopped: boolean;
}

/** Converts files one at a time. Progress arrives as `batch-progress` events. */
export function batchConvert(
  jobs: BatchJob[],
  format: OutputFormat,
  overwrite: boolean,
  copyTags = false,
  process: EffectChainDoc | null = null,
  shape: { normalizeDb: number | null; fadeInMs: number; fadeOutMs: number } | null = null,
): Promise<BatchSummary> {
  return invoke<BatchSummary>("batch_convert", { jobs, format, overwrite, copyTags, process, shape });
}

/** Stops Batch Convert; the file in progress is discarded. */
export function batchCancel(): Promise<void> {
  return invoke<void>("batch_cancel");
}

/** Shows the Insert Files as Tracks dialog (audio files or zips of them). Resolves to [] on cancel. */
export function pickTrackFiles(): Promise<string[]> {
  return invoke<string[]>("media_pick_tracks");
}

/** Extracts the audio in a zip into a folder beside it and returns those files. */
export function unzipMedia(path: string): Promise<string[]> {
  return invoke<string[]>("media_unzip", { path });
}

/** How loud one clip is, window by window, for QuickPass. */
export interface ClipLevels {
  clipId: string;
  /** Timeline position of the first window, in session samples. */
  start: number;
  /** Session samples in each window. */
  window: number;
  /** RMS level of each window in dBFS. */
  db: number[];
  /** Highest sample in each window in dBFS. */
  peak: number[];
}

export function quickpassLevels(clipIds: string[], windowMs: number, wave = false): Promise<ClipLevels[]> {
  return invoke("quickpass_levels", { clipIds, windowMs, wave });
}

export interface GenerateRequest {
  kind: "tone" | "white" | "pink" | "silence";
  seconds: number;
  frequency: number;
  levelDb: number;
  sampleRate: number;
  channels: number;
}

/** Writes a tone or noise to `path` as a WAV file. */
export function generateAudio(path: string, request: GenerateRequest): Promise<string> {
  return invoke<string>("generate_audio", { path, request });
}

/** Writes chapters into an exported MP3 or FLAC. False when the format has no chapter support here. */
export function exportEmbedChapters(path: string, chapters: { title: string; startMs: number; endMs: number | null }[], totalMs: number): Promise<boolean> {
  return invoke<boolean>("export_embed_chapters", { path, chapters, totalMs });
}

/** One control of a built-in effect, as the core describes it. */
export interface EffectParamInfo {
  key: string;
  name: string;
  unit: string;
  min: number;
  max: number;
  default: number;
  step: number;
  log: boolean;
  choices?: string[];
  toggle: boolean;
  group?: string;
  /** Moving it changes the effect's look-ahead, which restarts playback, so it applies on release. */
  retimes?: boolean;
  /** Stored with the effect but not a control (a learned noise print). */
  hidden?: boolean;
}

export interface EffectInfo {
  id: string;
  name: string;
  category: string;
  description: string;
  params: EffectParamInfo[];
  /** Has a level detector that can listen to another track or bus (sidechain). */
  sidechain?: boolean;
}

/** Every built-in effect, with its controls. */
export function effectsCatalog(): Promise<EffectInfo[]> {
  return invoke<EffectInfo[]>("effects_catalog");
}

/** An EQ or filter's response: [frequency, dB] pairs from 20 Hz to 20 kHz. Empty for other effects. */
export function effectResponse(slot: import("../stores/session").EffectSlotDoc, points: number): Promise<[number, number][]> {
  return invoke<[number, number][]>("effect_response", { slot, points });
}

/** Where to learn a noise print from, in timeline samples. */
export type LearnSource = { kind: "wave"; start: number; end: number } | { kind: "track" | "clip"; id: string; start: number; end: number };

/** Learns a noise print: the settings to put on a Noise Print Reducer. */
export function learnNoisePrint(source: LearnSource): Promise<{ params: Record<string, number>; seconds: number }> {
  return invoke("learn_noise_print", { source });
}

/** How far clip `other` trails clip `reference`, searching up to `maxMs` either way. */
export function phaseAlign(reference: string, other: string, maxMs: number): Promise<import("../panels/editor/align").Alignment> {
  return invoke("phase_align", { reference, other, maxMs });
}

/** Plans Music Fit for the part of `path` from `start` to `end` seconds: up to three plans, best first, joining in different places. */
export function musicFit(path: string, start: number, end: number, target: number, xfade: number): Promise<import("../panels/editor/musicfit").FitPlan[]> {
  return invoke("music_fit", { path, start, end, target, xfade });
}

/** Clicks or clipped spots found in one clip, as [first, last] timeline samples. */
export interface ClipSpots {
  clipId: string;
  spans: [number, number][];
}

export type SpotsRequest = { kind: "clicks"; sensitivity: number; widthMs: number } | { kind: "clipping"; levelDb: number; longestMs: number };

/** Finds clicks or clipping in clips as they play. */
export function quickpassSpots(clipIds: string[], spots: SpotsRequest, wave = false): Promise<ClipSpots[]> {
  return invoke<ClipSpots[]>("quickpass_spots", { clipIds, spots, wave });
}

/** Writes a copy of source frames `from..to` with `spans` (frames from `from`) repaired. Returns its path. */
export function quickpassRepairCopy(source: string, from: number, to: number, folder: string, name: string, spots: SpotsRequest, spans: [number, number][]): Promise<string> {
  return invoke<string>("quickpass_repair_copy", { source, from, to, folder, name, spots, spans });
}

export interface SesxExportReport {
  tracks: number;
  clips: number;
  chainsLeftOut: number;
  envelopesLeftOut: number;
  clipsSimplified: number;
  markersLeftOut: number;
  /** Buses, and tracks routed to a bus, with sends or with a sidechain. */
  routingLeftOut?: number;
}

/** File > Export > Multitrack Session (.sesx). Null when cancelled. */
export function sessionExportSesx(): Promise<{ path: string; report: SesxExportReport } | null> {
  return invoke<{ path: string; report: SesxExportReport } | null>("session_export_sesx");
}
