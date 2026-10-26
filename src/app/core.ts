// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Typed wrappers around Tauri commands. Components call these,
 * never `invoke` directly, so the command surface stays in one place.
 */

import { listen } from "@tauri-apps/api/event";
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
export interface CopyInResult {
  /** Where each file is now, in the order given. */
  paths: string[];
  copied: number;
  reused: number;
  failed: string[];
}

/** Copies audio into `folder` in the folder of the session saved at `session`. */
export function mediaCopyIn(session: string, folder: string, paths: string[]): Promise<CopyInResult> {
  return invoke<CopyInResult>("media_copy_in", { session, folder, paths });
}

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

/**
 * Raw bytes from a command, as a fresh Uint8Array. Some webviews hand raw
 * bytes back as an ArrayBuffer, others as a typed view or a plain array.
 */
export function rawBytes(buf: unknown): Uint8Array {
  if (buf instanceof ArrayBuffer) return new Uint8Array(buf.slice(0));
  if (ArrayBuffer.isView(buf)) return new Uint8Array(buf.buffer.slice(buf.byteOffset, buf.byteOffset + buf.byteLength));
  return Uint8Array.from(buf as ArrayLike<number>);
}

/** Exact samples of a file: `count` frames from frame `start` at its own rate, interleaved. */
export async function mediaFrames(path: string, start: number, count: number): Promise<{ channels: number; data: Float32Array }> {
  const bytes = rawBytes(await invoke<unknown>("media_frames", { path, start: Math.max(0, Math.floor(start)), count: Math.max(0, Math.ceil(count)) }));
  if (bytes.byteLength < 4) return { channels: 1, data: new Float32Array(0) };
  const channels = new DataView(bytes.buffer).getUint32(0, true) || 1;
  const n = Math.floor((bytes.byteLength - 4) / 4);
  return { channels, data: new Float32Array(bytes.buffer.slice(4, 4 + n * 4)) };
}

/** Plays one note through a MIDI track's instrument (velocity 0 to 1). False when nothing played: the session is playing, or no instrument. */
export function midiAudition(trackId: string, pitch: number, velocity: number, seconds: number, channel = 0): Promise<boolean> {
  return invoke<boolean>("midi_audition", { trackId, pitch, velocity, seconds, channel });
}

/**
 * The spectrum of what is playing (or, with `at`, of the audio just
 * before that spot of the session, or of the Waveform editor's file with
 * `wave`): `points` levels in dBFS from 20 Hz to 20 kHz, even in pitch.
 */
export async function scopeSpectrum(size: number, points: number, at: number | null, wave: boolean): Promise<Float32Array> {
  const bytes = rawBytes(await invoke<unknown>("scope_spectrum", { size, points, at: at === null ? null : Math.max(0, Math.round(at)), wave }));
  return new Float32Array(bytes.buffer, 0, Math.floor(bytes.byteLength / 4));
}

/** What the Phase Wheel compares. */
export type ScopeSignal = "left" | "right" | "mid" | "side";

/**
 * The Phase Wheel's picture of what is playing now: the correlation of
 * `a` and `b`, and per frequency band the phase of `b` relative to `a`
 * (radians, 0 in phase), its level, its pitch (20 Hz to 20 kHz, log) and
 * how consistent its phase is (each 0 to 1), packed in fours.
 */
export async function scopePhase(size: number, a: ScopeSignal, b: ScopeSignal): Promise<{ correlation: number; bins: Float32Array }> {
  const bytes = rawBytes(await invoke<unknown>("scope_phase", { size, a, b }));
  if (bytes.byteLength < 4) return { correlation: 0, bins: new Float32Array(0) };
  const all = new Float32Array(bytes.buffer, 0, Math.floor(bytes.byteLength / 4));
  return { correlation: all[0] ?? 0, bins: all.subarray(1) };
}

/**
 * The same picture while stopped: the `size` frames of the session (or the
 * Waveform editor's file, with `wave`) that end at `position`.
 */
export async function scopePhaseAt(position: number, size: number, a: ScopeSignal, b: ScopeSignal, wave: boolean): Promise<{ correlation: number; bins: Float32Array }> {
  const bytes = rawBytes(await invoke<unknown>("scope_phase_at", { position: Math.max(0, Math.round(position)), size, a, b, wave }));
  if (bytes.byteLength < 4) return { correlation: 0, bins: new Float32Array(0) };
  const all = new Float32Array(bytes.buffer, 0, Math.floor(bytes.byteLength / 4));
  return { correlation: all[0] ?? 0, bins: all.subarray(1) };
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

/**
 * Mix Down to New File: renders the session's mix (or a range or some
 * clips) into a new untitled file and opens it in the Waveform editor.
 * Null when cancelled. Progress arrives as `export-progress` events.
 */
export async function waveNewFromMix<S>(name: string, request: { range?: [number, number]; clips?: string[]; realtime?: boolean }): Promise<{ doc: WaveDocRaw<S>; result: MixdownResult } | null> {
  const r = await invoke<[WaveDocRaw<S>, MixdownResult] | null>("wave_new_from_mix", {
    name,
    request: { range: request.range ?? null, clips: request.clips ?? null, bit_depth: null, track: null, realtime: request.realtime ?? false },
  });
  return r ? { doc: r[0], result: r[1] } : null;
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

/** Sets a copy of the session in front aside, so another can take its place. */
export function sessionPark(): Promise<void> {
  return invoke<void>("session_park");
}

/** Brings a set-aside session to the front. */
export function sessionResume<T>(id: string): Promise<SessionResult<T>> {
  return invoke<SessionResult<T>>("session_resume", { id });
}

/** Drops a set-aside session. */
export function sessionForget(id: string): Promise<void> {
  return invoke<void>("session_forget", { id });
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
  /** Names of the effects left out, each once. */
  skippedNames?: string[];
  buses?: number;
  /** VST3 plug-ins brought in with their settings. */
  plugins?: number;
  /** Plug-ins brought in that this computer has not scanned. */
  missingPlugins?: string[];
  /** Clips with a time stretch or pitch change. */
  stretched?: number;
  /** The other editor's built-in effects brought in as ours (its per-frequency phase shifter as Phase Curve). */
  converted?: number;
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
  /** Per track, its Audio FX chain's peaks for the audio just heard: left in, right in, left out, right out. Null when the track's chain does nothing. */
  chainMeters?: ([number, number, number, number] | null)[];
  /** Buses, with their levels after the fader and their chains' peaks. Empty unless the mix is routed. */
  busIds?: string[];
  busMeters?: [number, number][];
  busChainMeters?: ([number, number, number, number] | null)[];
  masterChainMeter?: [number, number, number, number] | null;
  /** 3D Space sessions: the master's speaker channels, after its fader, in the layout's order. */
  bedMeters?: number[];
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

/** Shows an Open dialog for an amp capture (.nam) or a cabinet response; null on cancel. */
export function ampPick(kind: "model" | "ir"): Promise<string | null> {
  return invoke<string | null>("amp_pick", { kind });
}

/** Reads a capture into an Amp Model slot's data; returns the new data. */
export function ampLoadModel(path: string, data: string | null): Promise<string> {
  return invoke<string>("amp_load_model", { path, data });
}

/** Reads a cabinet response into an Amp Model slot's data; returns the new data. */
export function ampLoadIr(path: string, data: string | null): Promise<string> {
  return invoke<string>("amp_load_ir", { path, data });
}

/** What the metronome plays, as the core takes it. */
export interface MetronomeCore {
  on: boolean;
  levelDb: number;
  sound: "tick" | "wood" | "beep" | "shaker";
  accent: boolean;
  subdivide: "none" | "halves" | "triplets" | "quarters";
}

/** The metronome's settings; heard within the audio already rendered ahead. */
export function setMetronome(settings: MetronomeCore): Promise<void> {
  return invoke<void>("transport_set_metronome", { settings });
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
  /** Leave every Audio FX chain out (measuring the raw audio). */
  dry?: boolean;
  /** 1 writes a mono file (left and right averaged). */
  channels?: number;
  /** Render at playback speed, for plug-ins that only work in real time. */
  realtime?: boolean;
  /** 3D Space sessions: the speaker layout to write (omit for the session's). */
  spaceLayout?: string;
  /** 3D Space sessions: a stereo file for headphones. */
  binaural?: boolean;
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
      realtime: request.realtime ?? false,
      space_layout: request.spaceLayout ?? null,
      binaural: request.binaural ?? false,
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

/**
 * Bounce to New Track: renders part of the mix as it plays (a track with
 * its processing, or the whole mix of a range or of some clips) to a new
 * 32-bit float WAV in `folder`, named after `name` without overwriting
 * anything.
 */
export function exportBounce(folder: string, name: string, request: MixdownRequest): Promise<MergeResult> {
  return invoke<MergeResult>("export_bounce", {
    folder,
    name,
    request: { range: request.range ?? null, clips: request.clips ?? null, bit_depth: null, track: request.track ?? null, realtime: request.realtime ?? false, channels: request.channels ?? null },
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

/** Documents/Audiform/Sessions. */
export function sessionDefaultFolder(): Promise<string> {
  return invoke<string>("session_default_folder");
}

/** The first "`base` N" with no session folder of that name in `folder` yet. */
export function sessionNextName(folder: string, base: string): Promise<string> {
  return invoke<string>("session_next_name", { folder, base });
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
/**
 * The spectrogram of `start..end`. `hz` (optional) gives each row's edges
 * from the top down, one more than `rows`, for a zoomed or linear axis;
 * `fft` the analysis frame length (resolution).
 */
export async function waveSpectrogram(start: number, end: number, columns: number, rows: number, hz?: number[], fft?: number): Promise<Uint8Array> {
  const buf = await invoke<ArrayBuffer>("wave_spectrogram", { start, end, columns, rows, hz: hz ?? null, fft: fft ?? null });
  return rawBytes(buf);
}

/** An edit limited to one channel: `op` applied to a copy, `start..end` rendered, each channel taken as `take` says. */
export function waveChannelEdit(op: import("../stores/session").Op, start: number, end: number, take: import("../panels/editor/wave-channels").ChannelTake): Promise<SpectralResult> {
  return invoke<SpectralResult>("wave_channel_edit", { op, start, end, take });
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

/** Which device outputs (from 0) playback uses: the stereo pair, and each speaker by name in 3D Space sessions heard on speakers. */
export function setOutputMap(stereo: [number, number], speakers: Record<string, number>): Promise<void> {
  return invoke<void>("set_output_map", { stereo, speakers });
}

/** How many outputs the playback device has. */
export function outputChannels(): Promise<number> {
  return invoke<number>("output_channels");
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
  /** A VST3 plug-in. */
  plugin?: boolean;
  /** A plug-in with a window of its own. */
  hasWindow?: boolean;
  /** No longer offered; older sessions still play it. */
  retired?: boolean;
}

/** The Plug-in Manager's list. */
export interface PluginRegistry {
  folders: { path: string; standard: boolean; on: boolean }[];
  plugins: { effect: string; name: string; vendor: string; version: string; kind: string; bundle: string; latency: number; hasWindow: boolean; instrument: boolean; note?: string | null; controls: number }[];
  failed: { bundle: string; error: string }[];
  /** Makers whose plug-ins run together in one process (lower case). */
  shared: string[];
}

/** Sets the makers whose plug-ins run together in one process. */
export function pluginsSetShared(makers: string[]): Promise<PluginRegistry> {
  return invoke<PluginRegistry>("plugins_set_shared", { makers });
}

/** What one running plug-in reported. */
export interface PluginEvents {
  slotId: string;
  /** Controls moved in its window: [key, value 0 to 1]. */
  edits: [string, number][];
  dirty: boolean;
  windowClosed: boolean;
  latencyChanged: boolean;
  /** Why it stopped (crashed or hung); it is bypassed. */
  stopped: string | null;
}

export function pluginsRegistry(): Promise<PluginRegistry> {
  return invoke<PluginRegistry>("plugins_registry");
}

/** Scans the plug-in folders; `all` also rescans bundles that did not change. */
/** Scan progress: bundles read so far, of how many, and the latest one's name. */
export interface PluginScanProgress {
  done: number;
  total: number;
  name: string;
}

/** Calls `fn` as a scan goes; resolves to a function that stops listening. */
export function onPluginScanProgress(fn: (p: PluginScanProgress) => void): Promise<() => void> {
  return listen<PluginScanProgress>("plugins-scan-progress", (e) => fn(e.payload));
}

/** Stops the scan in progress; what it read so far is kept. */
export function pluginsScanStop(): Promise<void> {
  return invoke<void>("plugins_scan_stop");
}

export function pluginsScan(all: boolean): Promise<[{ scanned: number; found: number; failed: number; stopped: boolean }, PluginRegistry]> {
  return invoke("plugins_scan", { all });
}

/** Adds or removes a plug-in folder (a standard one is turned off or on). */
export function pluginsSetFolder(path: string, on: boolean): Promise<PluginRegistry> {
  return invoke<PluginRegistry>("plugins_set_folder", { path, on });
}

export function pluginOpenWindow(slot: import("../stores/session").EffectSlotDoc, title: string): Promise<void> {
  return invoke<void>("plugin_open_window", { slot, title });
}

export function pluginCloseWindow(slotId: string): Promise<void> {
  return invoke<void>("plugin_close_window", { slotId });
}

export function pluginEvents(): Promise<PluginEvents[]> {
  return invoke<PluginEvents[]>("plugin_events");
}

export function pluginState(slotId: string): Promise<string | null> {
  return invoke<string | null>("plugin_state", { slotId });
}

export function pluginRestart(slotId: string): Promise<void> {
  return invoke<void>("plugin_restart", { slotId });
}

/** Every built-in effect, with its controls. */
export function effectsCatalog(): Promise<EffectInfo[]> {
  return invoke<EffectInfo[]>("effects_catalog");
}

/** An EQ or filter's response: [frequency, dB] pairs from 20 Hz to 20 kHz. Empty for other effects. */
/** The Shaped Noise Reducer's learned noise and threshold, in dB at `points` frequencies, on the scale of a `scope`-point spectrum. */
export function shapedNoiseCurves(slot: import("../stores/session").EffectSlotDoc, points: number, scope: number): Promise<{ print: [number, number][]; threshold: [number, number][] }> {
  return invoke("shaped_noise_curves", { slot, points, scope });
}

/** An effect's own meter: gain reduction (dB) and output level (dBFS), or null before it has run. */
export function effectMeter(slot: string): Promise<[number, number] | null> {
  return invoke<[number, number] | null>("effect_meter", { slot });
}

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

/** A VST3 preset file found for a plug-in. `own` ones are in the user's folder and can be deleted. */
export interface PluginPresetFile {
  name: string;
  path: string;
  own: boolean;
}

/** Presets for a plug-in in the standard VST3 preset folders (shared with other VST3 programs). */
export function pluginPresets(effect: string): Promise<PluginPresetFile[]> {
  return invoke<PluginPresetFile[]>("plugin_presets", { effect });
}

/** A preset file's settings, for a slot's `plugin_state`. */
export function pluginPresetRead(path: string): Promise<string> {
  return invoke<string>("plugin_preset_read", { path });
}

/** Saves a plug-in slot's current settings as a preset file. Returns its path. */
export function pluginPresetSave(slot: import("../stores/session").EffectSlotDoc, name: string): Promise<string> {
  return invoke<string>("plugin_preset_save", { slot, name });
}

export function pluginPresetDelete(path: string): Promise<void> {
  return invoke<void>("plugin_preset_delete", { path });
}

/** The channels a slot's effect hears and makes, by speaker name (L and R for built-in effects). */
export function effectChannels(slot: import("../stores/session").EffectSlotDoc): Promise<{ inputs: string[]; outputs: string[] }> {
  return invoke("effect_channels", { slot });
}

/** A note for {@link midiPlayNotes}: times in seconds from the start, velocity 0 to 1. */
export interface SeqNote {
  pitch: number;
  velocity: number;
  channel?: number;
  pan?: number;
  on: number;
  off: number;
}

/** Plays notes alone through a MIDI track's instrument (the piano roll's Play). False when nothing played. */
export function midiPlayNotes(trackId: string, notes: SeqNote[]): Promise<boolean> {
  return invoke<boolean>("midi_play_notes", { trackId, notes });
}
