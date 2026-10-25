// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

/**
 * The open session as the interface sees it. The core owns the session;
 * edits are sent as ops (see src-tauri/src/session/ops.rs) and the core
 * returns the new session plus the op that undoes the edit.
 *
 * Field names match the `.altaudio` file format (snake_case).
 */

import type { Command } from "../commands";
import type { CommandRunner } from "./media-pool";

export interface MediaRefDoc {
  id: string;
  path: string;
  relative_path: string | null;
  sample_rate: number;
  channels: number;
  length: number;
}

export interface FadeDoc {
  length: number;
  shape: string;
  curve: number;
}

/** One effect in a Voice Chain. */
export interface EffectSlotDoc {
  id: string;
  /** Effect id, for example `altaudio.parametric_eq`. */
  effect: string;
  enabled: boolean;
  /** Values by parameter key; missing keys take the effect's default. */
  params: Record<string, number>;
  preset: string | null;
  /** Sidechain: the track or bus this effect's detector listens to. Missing listens to its own input. */
  sidechain?: string | null;
  /** A VST3 plug-in's own settings (set in its window), base64. */
  plugin_state?: string | null;
  /** Which numbered slot it shows in (0 first), so a chain can leave gaps. Missing follows the one before. */
  position?: number | null;
}

/** A Voice Chain on a clip, a track or the master. */
export interface EffectChainDoc {
  enabled: boolean;
  input_gain_db: number;
  output_gain_db: number;
  /** 0 dry to 1 wet. */
  mix: number;
  slots: EffectSlotDoc[];
}

export type EffectTarget =
  | { kind: "track"; id: string }
  | { kind: "clip"; id: string }
  | { kind: "bus"; id: string }
  | { kind: "master" };

/** Where a track or bus sends its sound after its fader. */
export type OutputDoc = { type: "master" } | { type: "bus"; bus_id: string };

/** An aux send: a copy of the sound sent on to a bus at its own level. */
export interface SendDoc {
  id: string;
  bus_id: string;
  level_db: number;
  /** -100 left to 100 right. */
  pan: number;
  /** Before the fader: ignores the sender's fader and pan (still follows mute and solo). */
  pre_fader: boolean;
  enabled: boolean;
}

/** A bus track: sums what feeds it, runs its own chain and fader. */
export interface BusDoc {
  id: string;
  name: string;
  channels: "mono" | "stereo";
  color: string | null;
  height: number;
  volume_db: number;
  pan: number;
  muted: boolean;
  solo: boolean;
  output: OutputDoc;
  sends: SendDoc[];
  effects: EffectChainDoc;
  /** Shows below this many tracks in the Editor; missing shows it below the last. */
  position?: number | null;
  /** Folded to mono after its Voice Chain. Missing when off. */
  mono?: boolean;
  [key: string]: unknown;
}

export interface ClipDoc {
  id: string;
  name: string;
  media_id: string;
  start: number;
  source_in: number;
  source_out: number;
  gain_db: number;
  muted: boolean;
  locked: boolean;
  color: string | null;
  fade_in: FadeDoc;
  fade_out: FadeDoc;
  /** Clips sharing a group id are selected and moved together. */
  group?: string | null;
  envelopes?: EnvelopeDoc[];
  /** Loop Clip: the source region repeats to fill `length` session samples, starting `phase` source samples into it. */
  looped?: LoopDoc | null;
  /** Time stretch. Ignored while the clip loops. */
  stretch?: StretchDoc | null;
  /** What the clip holds. */
  audio_type?: AudioType | null;
  effects?: EffectChainDoc;
  [key: string]: unknown;
}

export type AudioType = "dialogue" | "music" | "sfx" | "ambience";

export const AUDIO_TYPES: { id: AudioType; label: string }[] = [
  { id: "dialogue", label: "Dialogue" },
  { id: "music", label: "Music" },
  { id: "sfx", label: "Sound Effects" },
  { id: "ambience", label: "Ambience" },
];

export interface StretchDoc {
  /** Timeline length over natural length: 1.25 plays 25 percent longer. */
  ratio: number;
  /** True keeps the pitch; false changes pitch with speed, like tape (varispeed). */
  keep_pitch: boolean;
  /** Pitch change in semitones, -24 to +24 (pitch-keeping only). */
  pitch?: number;
  /** Several notes at once (music): longer analysis frames. */
  polyphonic?: boolean;
  /** Worked out ahead rather than live. Sounds the same here; kept for round trips. */
  rendered?: boolean;
  advanced?: StretchAdvancedDoc | null;
}

export interface StretchAdvancedDoc {
  window_ms: number;
  transient_sensitivity: number;
  preserve_formants: boolean;
  high_precision: boolean;
}

export const PITCH_MIN = -24;
export const PITCH_MAX = 24;

export const STRETCH_MIN = 0.25;
export const STRETCH_MAX = 4;

/** The stretch ratio in effect: 1 when not stretched or when looping. */
export function stretchOf(clip: ClipDoc): number {
  if (clip.looped || !clip.stretch) return 1;
  return Math.max(STRETCH_MIN, Math.min(STRETCH_MAX, clip.stretch.ratio));
}

export interface LoopDoc {
  length: number;
  phase: number;
}

export interface EnvelopePointDoc {
  /** Session samples from the clip start. */
  position: number;
  /** Volume: dB from -60 (silent) to +12. */
  value: number;
  curve: "linear" | "hold" | "spline";
}

export interface EnvelopeDoc {
  id: string;
  target:
    | { type: "volume" }
    | { type: "pan" }
    | { type: "effect_param"; slot_id: string; param: string };
  enabled: boolean;
  points: EnvelopePointDoc[];
}

export interface TrackDoc {
  id: string;
  name: string;
  role: "standard" | "guest";
  host_profile_id?: string | null;
  channels: "mono" | "stereo";
  color: string | null;
  height: number;
  volume_db: number;
  pan: number;
  muted: boolean;
  solo: boolean;
  record_armed: boolean;
  /** Missing in files where it is off. */
  polarity_inverted?: boolean;
  /** Folded to mono after its Voice Chain. Missing in files where it is off. */
  mono?: boolean;
  /** Folded to its name row. Missing in files where it is off. */
  collapsed?: boolean;
  /** Hidden from the Editor and Mixer lists. Missing in files where it is off. */
  hidden?: boolean;
  monitor_input?: boolean;
  input?: string | null;
  effects?: EffectChainDoc;
  output?: OutputDoc;
  sends?: SendDoc[];
  clips: ClipDoc[];
  /** "midi" for MIDI tracks. Missing for audio tracks. */
  kind?: "audio" | "midi";
  /** MIDI tracks: the VST3 instrument that plays the notes. */
  instrument?: EffectSlotDoc | null;
  /** MIDI tracks: their clips. Missing when there are none. */
  midi_clips?: MidiClipDoc[];
  [key: string]: unknown;
}

/** A note in a MIDI clip. Times are in beats from the clip's start. */
export interface NoteDoc {
  /** 0 to 127; 60 is middle C. */
  pitch: number;
  /** 1 to 127. */
  velocity: number;
  start: number;
  length: number;
}

/** A MIDI clip. Its start and length are in beats, so it follows the tempo. */
export interface MidiClipDoc {
  id: string;
  name: string;
  start: number;
  length: number;
  muted?: boolean;
  color?: string | null;
  /** Clip level in dB: its notes play harder or softer by this much. */
  gain_db?: number;
  notes: NoteDoc[];
}

export interface TempoDoc {
  bpm: number;
  beats_per_bar: number;
}

export const DEFAULT_TEMPO: TempoDoc = { bpm: 120, beats_per_bar: 4 };
export const BPM_MIN = 20;
export const BPM_MAX = 400;

/** The session's tempo (the default when the file has none). */
export const tempoOf = (doc: SessionDoc): TempoDoc => (doc.tempo as TempoDoc | undefined) ?? DEFAULT_TEMPO;

export const isMidi = (t: TrackDoc) => t.kind === "midi";

export interface ViewDoc {
  samples_per_pixel: number;
  scroll: number;
  scroll_y: number;
  playhead: number;
  selection: { start: number; end: number } | null;
}

export interface SessionDoc {
  format: "altaudio.session";
  schema_version: number;
  id: string;
  name: string;
  sample_rate: number;
  bit_depth: number;
  media: MediaRefDoc[];
  tracks: TrackDoc[];
  /** Bus tracks, in mixer order. */
  buses?: BusDoc[];
  markers: MarkerDoc[];
  /** Playlist panel order: ids of range markers. */
  playlist?: string[];
  view: ViewDoc;
  libraries: string[];
  master?: {
    volume_db: number;
    channels: string;
    height: number;
    effects?: EffectChainDoc;
    [key: string]: unknown;
  };
  /** Missing at the default, 120 BPM in 4. */
  tempo?: TempoDoc;
  [key: string]: unknown;
}

export type MarkerKind =
  "cue" | "subclip" | "chapter" | "ad_slot" | "social_clip" | "cd_track" | "cart_timer";

export interface MarkerDoc {
  id: string;
  name: string;
  kind: MarkerKind;
  /** Session samples. */
  position: number;
  /** Session samples. 0 is a point marker. */
  duration: number;
  description: string;
  chapter: unknown;
  contracted_duration: number | null;
  /** `#rrggbb`. Null or missing uses the type's color. */
  color?: string | null;
}

export interface MarkerChange {
  id: string;
  name?: string;
  kind?: MarkerKind;
  position?: number;
  duration?: number;
  description?: string;
  /** `#rrggbb`, or "" for the type's color. */
  color?: string;
  /** Ad slots: contracted length of the read, in session samples. */
  contracted_duration?: number;
  /** Removes the contracted length. */
  clear_contracted?: boolean;
}

/** Changes to one clip. Fields left out are unchanged; `track_id` moves it. */
export interface ClipChange {
  id: string;
  track_id?: string;
  name?: string;
  start?: number;
  source_in?: number;
  source_out?: number;
  gain_db?: number;
  muted?: boolean;
  locked?: boolean;
  fade_in?: FadeDoc;
  fade_out?: FadeDoc;
  /** `#rrggbb`, or "" for the track color. */
  color?: string;
  group?: string;
  ungroup?: boolean;
  /** Replaces the volume envelope points. Empty removes the envelope. */
  volume?: EnvelopePointDoc[];
  /** Replaces the pan envelope points (-100 left to 100 right). Empty removes the envelope. */
  pan?: EnvelopePointDoc[];
  /** Replaces the points of one effect setting's envelope. Empty removes it. */
  effect_points?: {
    slot_id: string;
    param: string;
    points: EnvelopePointDoc[];
  }[];
  /** Loops the clip, or changes its loop. */
  looped?: LoopDoc;
  /** Stops looping. */
  unloop?: boolean;
  /** Stretches the clip, or changes its stretch. */
  stretch?: StretchDoc;
  /** Stops stretching. */
  unstretch?: boolean;
  /** Sets what the clip holds. */
  audio_type?: AudioType;
  /** Clears the audio type. */
  clear_audio_type?: boolean;
  /** Place in the track's stacking order after the change: 0 is the bottom; past the end is the top. */
  layer?: number;
}

export interface ClipSpec {
  id: string;
  name: string;
  media_id: string;
  start: number;
  source_in: number;
  source_out: number;
}

export interface TrackChanges {
  name?: string;
  muted?: boolean;
  solo?: boolean;
  record_armed?: boolean;
  height?: number;
  color?: string | null;
  volume_db?: number;
  pan?: number;
  polarity_inverted?: boolean;
  mono?: boolean;
  collapsed?: boolean;
  hidden?: boolean;
  monitor_input?: boolean;
  input?: string | null;
  role?: "standard" | "guest";
  /** The host profile applied; "" clears it. */
  host_profile_id?: string;
  output?: OutputDoc;
  /** MIDI tracks: sets the instrument. */
  instrument?: EffectSlotDoc;
  /** MIDI tracks: takes the instrument out. */
  clear_instrument?: boolean;
}

export interface BusChanges {
  name?: string;
  muted?: boolean;
  solo?: boolean;
  color?: string | null;
  volume_db?: number;
  pan?: number;
  height?: number;
  output?: OutputDoc;
  /** Shows the bus below this many tracks in the Editor. */
  position?: number;
  /** Shows the bus below the last track again. */
  clear_position?: boolean;
  mono?: boolean;
}

/** An edit the core understands. */
export type Op =
  | {
      op: "add_track";
      id: string;
      name: string;
      stereo: boolean;
      index: number;
      /** A MIDI track instead of an audio track. */
      midi?: boolean;
    }
  | { op: "restore_track"; track: TrackDoc; index: number }
  | { op: "remove_track"; id: string }
  | { op: "move_track"; id: string; to: number }
  | { op: "set_track"; id: string; changes: TrackChanges }
  | { op: "add_media"; media: MediaRefDoc }
  | { op: "remove_media"; id: string }
  | { op: "set_media"; media: MediaRefDoc; clip_outs?: [string, number][] }
  | { op: "add_clips"; track_id: string; clips: ClipSpec[] }
  | { op: "restore_clips"; clips: [string, ClipDoc][] }
  | { op: "remove_clips"; ids: string[] }
  | { op: "set_sample_rate"; sample_rate: number; exact?: number[] }
  | { op: "set_session_info"; name?: string; bit_depth?: number }
  | { op: "set_clips"; changes: ClipChange[] }
  | { op: "add_markers"; markers: MarkerDoc[] }
  | { op: "remove_markers"; ids: string[] }
  | { op: "set_markers"; changes: MarkerChange[] }
  | { op: "set_playlist"; ids: string[] }
  | { op: "set_effects"; target: EffectTarget; chain: EffectChainDoc }
  | { op: "set_master_volume"; volume_db: number }
  | { op: "add_bus"; id: string; name: string; stereo: boolean; index: number }
  | { op: "restore_bus"; bus: BusDoc; index: number }
  | { op: "remove_bus"; id: string }
  | { op: "move_bus"; id: string; to: number }
  | { op: "set_bus"; id: string; changes: BusChanges }
  | { op: "set_sends"; owner: string; sends: SendDoc[] }
  | { op: "set_midi_clips"; track_id: string; clips: MidiClipDoc[] }
  | { op: "set_tempo"; bpm: number; beats_per_bar: number }
  | { op: "batch"; ops: Op[] };

export interface SessionBackend {
  get(): Promise<{ session: SessionDoc; path: string | null }>;
  apply(op: Op): Promise<{ session: SessionDoc; inverse: Op }>;
  setView(view: ViewDoc): Promise<void>;
}

type Listener = (doc: SessionDoc | null) => void;

/** Timeline length of a clip, in session samples. */
export function clipLength(
  clip: ClipDoc,
  media: MediaRefDoc | undefined,
  sessionRate: number,
): number {
  if (clip.looped) return clip.looped.length;
  const srcRate = media?.sample_rate || sessionRate;
  const natural = Math.round(
    ((clip.source_out - clip.source_in) * sessionRate) / srcRate,
  );
  const r = stretchOf(clip);
  return r === 1 ? natural : Math.round(natural * r);
}

/**
 * True for a session with nothing in it worth keeping: no clips (audio or
 * MIDI), markers, buses or effects. Files imported into such a session
 * sit in the Files panel to open on their own, without making a session
 * anyone has to save.
 */
export function isBlank(doc: SessionDoc | null): boolean {
  if (!doc) return true;
  const fx = (c: { slots?: unknown[] } | undefined) => (c?.slots?.length ?? 0) > 0;
  return (
    doc.tracks.every((t) => t.clips.length === 0 && !(t.midi_clips?.length ?? 0) && !fx(t.effects)) &&
    doc.markers.length === 0 &&
    !(doc.buses?.length ?? 0) &&
    !fx(doc.master?.effects)
  );
}

/** End of the last MIDI clip, in session samples (the core's sums). */
export function midiEnd(doc: SessionDoc, rate = doc.sample_rate): number {
  const t = tempoOf(doc);
  const bpm = Math.min(BPM_MAX, Math.max(BPM_MIN, t.bpm));
  let end = 0;
  for (const tr of doc.tracks)
    for (const c of tr.midi_clips ?? []) end = Math.max(end, Math.round(((c.start + c.length) * 60 * rate) / bpm));
  return end;
}

/** End of the last clip (audio or MIDI), in session samples. */
export function sessionEnd(doc: SessionDoc): number {
  const media = new Map(doc.media.map((m) => [m.id, m]));
  let end = midiEnd(doc);
  for (const t of doc.tracks) {
    for (const c of t.clips)
      end = Math.max(
        end,
        c.start + clipLength(c, media.get(c.media_id), doc.sample_rate),
      );
  }
  return end;
}

/** Where errors no caller can catch go; the app shows them in the status bar. */
let errorSink: (e: unknown) => void = (e) => console.error(e);
export function setErrorSink(fn: (e: unknown) => void): void {
  errorSink = fn;
}
export function reportError(e: unknown): void {
  errorSink(e);
}

export class SessionStore {
  private doc: SessionDoc | null = null;
  private filePath: string | null = null;
  private readonly listeners = new Set<Listener>();
  private viewTimer: ReturnType<typeof setTimeout> | undefined;

  constructor(private readonly backend: SessionBackend) {}

  subscribe(fn: Listener): () => void {
    this.listeners.add(fn);
    fn(this.doc);
    return () => {
      this.listeners.delete(fn);
    };
  }

  get current(): SessionDoc | null {
    return this.doc;
  }

  /** The session's file, or null before the first save. */
  get path(): string | null {
    return this.filePath;
  }

  async load(): Promise<SessionDoc> {
    const r = await this.backend.get();
    this.set(r.session, r.path);
    if (r.path === null) this.launch = { id: r.session.id, tracks: r.session.tracks.map((t) => t.id).join() };
    return this.doc!;
  }

  /** The never-saved session the app opened with, and its tracks then. */
  private launch: { id: string; tracks: string } | null = null;

  /**
   * True while the open session is still the blank one the app started
   * with: never saved, its tracks as they were, and nothing in it (files
   * imported into the Files panel do not count). It is not listed, starred
   * or offered for saving, and the Editor shows its start screen. A
   * session made with New Session or opened from a file never is.
   */
  isScratch(doc: SessionDoc | null = this.doc): boolean {
    return (
      !!doc &&
      !!this.launch &&
      this.filePath === null &&
      doc.id === this.launch.id &&
      doc.tracks.map((t) => t.id).join() === this.launch.tracks &&
      isBlank(doc)
    );
  }

  /**
   * Tells every listener. One that throws is reported and skipped, so a
   * fault in one part of the interface never leaves the rest showing an
   * old session (tester report: tracks added but never shown).
   */
  private notify(doc: SessionDoc | null): void {
    for (const fn of this.listeners) {
      try {
        fn(doc);
      } catch (e) {
        reportError(e);
      }
    }
  }

  /** Replaces the whole document, for example after opening a file. */
  set(doc: SessionDoc, path?: string | null): void {
    this.doc = doc;
    if (path !== undefined) this.filePath = path;
    this.notify(doc);
  }

  /** Runs an op as one undoable step. */
  /** An op as a command not run yet, to combine with others into one undo step. */
  command(label: string, op: Op): Command {
    return new OpCommand(this, op, label);
  }

  edit(label: string, op: Op, runner: CommandRunner): Promise<void> {
    return runner.execute(new OpCommand(this, op, label));
  }

  /** Applies an op directly. For commands only. */
  async apply(op: Op): Promise<Op> {
    const r = await this.backend.apply(op);
    this.set(r.session);
    return r.inverse;
  }

  /** View changes (zoom, scroll, playhead) are not edits; sent to the core after a pause. */
  setView(view: ViewDoc): void {
    if (!this.doc) return;
    this.doc = { ...this.doc, view };
    this.notify(this.doc);
    clearTimeout(this.viewTimer);
    this.viewTimer = setTimeout(() => {
      this.viewTimer = undefined;
      void this.backend.setView(view).catch(console.error);
    }, 300);
  }

  /**
   * Sends a view change still waiting on the timer now. Playback reads the
   * time selection from the core, so a loop over a selection made a moment
   * ago needs it there first.
   */
  async flushView(): Promise<void> {
    if (this.viewTimer === undefined || !this.doc) return;
    clearTimeout(this.viewTimer);
    this.viewTimer = undefined;
    await this.backend.setView(this.doc.view).catch(console.error);
  }
}

/** Track settings a fader-style drag changes many times in a row. */
function mergeKey(op: Op): string | null {
  if (op.op === "set_master_volume") return "master";
  if (op.op === "set_sends") return `sends:${op.owner}`;
  if (op.op !== "set_track" && op.op !== "set_bus") return null;
  const keys = Object.keys(op.changes);
  if (keys.length !== 1 || !["volume_db", "pan", "height"].includes(keys[0]!))
    return null;
  return `${op.id}:${keys[0]}`;
}

class OpCommand implements Command {
  private inverse: Op | null = null;

  constructor(
    private readonly store: SessionStore,
    private op: Op,
    readonly label: string,
  ) {}

  /** A run of drags on one fader is one undo step back to where it started. */
  mergeWith(next: Command): boolean {
    if (!(next instanceof OpCommand)) return false;
    // A slider dragged in the Voice Chain applies as it moves; the drag is one step.
    if (this.op.op === "set_effects" && next.op.op === "set_effects") {
      const a = this.op.target;
      const b = next.op.target;
      if (
        this.label !== next.label ||
        a.kind !== b.kind ||
        ("id" in a ? a.id : null) !== ("id" in b ? b.id : null)
      )
        return false;
      this.op = next.op;
      return true;
    }
    const key = mergeKey(this.op);
    if (key === null || key !== mergeKey(next.op)) return false;
    this.op = next.op;
    return true;
  }

  async do(): Promise<void> {
    this.inverse = await this.store.apply(this.op);
  }

  async undo(): Promise<void> {
    if (this.inverse) await this.store.apply(this.inverse);
  }
}
