// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * New, Open, Save, Save As, Close, autosave and crash recovery.
 *
 * Rules:
 * - Work is never thrown away without asking: New, Open, Close and Exit
 *   ask to save when there are unsaved changes.
 * - Autosave writes a recovery copy every 2 minutes while there are
 *   unsaved changes. A clean save or close removes it.
 * - A recovery copy found at startup means the app did not close
 *   normally; the user chooses to recover or discard it.
 */

import type { NewSessionOptions, Recoverable, SesxReport, SessionResult } from "./core";
import type { UndoStack } from "../commands";
import type { StashedHistory } from "../commands/undo-stack";
import { sessionEnd } from "../stores/session";
import type { MediaPool } from "../stores/media-pool";
import type { MediaRefDoc, SessionDoc, SessionStore } from "../stores/session";

export const AUTOSAVE_MS = 2 * 60 * 1000;
const MAX_RECENT = 10;

export interface SessionFileBackend {
  create(options?: NewSessionOptions): Promise<SessionResult<SessionDoc>>;
  save(path: string | null, media: MediaRefDoc[], libraries: string[]): Promise<SessionResult<SessionDoc>>;
  open(path: string): Promise<SessionResult<SessionDoc>>;
  autosave(media: MediaRefDoc[]): Promise<void>;
  clearAutosave(): Promise<void>;
  recoverable(): Promise<Recoverable[]>;
  recover(autosave: string): Promise<SessionResult<SessionDoc>>;
  discardRecovery(sessionId: string): Promise<void>;
  pickSave(name: string): Promise<string | null>;
  pickOpen(): Promise<string | null>;
  pickSesx(): Promise<string | null>;
  importSesx(path: string): Promise<SessionResult<SessionDoc> & { report: SesxReport }>;
  /** Sets a copy of the session in front aside, so another can take its place. Without these, one session at a time. */
  park?(): Promise<void>;
  /** Brings a set-aside session to the front. */
  resume?(id: string): Promise<SessionResult<SessionDoc>>;
  /** Drops a set-aside session. */
  forget?(id: string): Promise<void>;
}

/** An open session waiting behind the one in front, with its undo history. */
export interface ParkedSession {
  id: string;
  name: string;
  path: string | null;
  dirty: boolean;
  seconds: number;
  sampleRate: number;
  history: StashedHistory;
}

export type Choice = "save" | "discard" | "cancel";

export interface SessionFilesDeps {
  backend: SessionFileBackend;
  session: SessionStore;
  pool: MediaPool;
  stack: UndoStack;
  /** Asks to save before losing changes. */
  askSave(name: string): Promise<Choice>;
  /** Asks whether to recover an autosave. True means recover. */
  askRecover(r: Recoverable): Promise<boolean>;
  recent: { get(): string[]; set(paths: string[]): void };
  notice(text: string): void;
  /** Called after the open session is replaced, to reset view state and reopen its libraries. */
  onReplaced?(doc: SessionDoc): void;
  /** Library folders to record with the session. */
  libraries?(): string[];
  /** Runs just before saving to `target` (the session file): copies audio into its folder. */
  beforeSave?(target: string): Promise<void>;
  /** Runs before the session in front is set aside (fetches plug-in settings). */
  beforePark?(): Promise<void>;
  /** The list of open sessions changed. */
  onSessions?(): void;
}

export { isBlank } from "../stores/session";

/** Display name: the file name without extension, or the session name. */
export function displayName(path: string | null, doc: SessionDoc | null): string {
  if (path) {
    const base = path.split(/[\\/]/).pop() ?? path;
    return base.replace(/\.altaudio$/i, "");
  }
  return doc?.name ?? "Untitled Session";
}

/** One line describing what a .sesx import brought across. */
export function sesxSummary(r: SesxReport): string {
  const count = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;
  const buses = r.buses ? `, ${count(r.buses, "bus", "buses")}` : "";
  const parts = [`Imported ${count(r.tracks, "track", "tracks")}${buses} and ${count(r.clips, "clip", "clips")}.`];
  if (r.plugins) parts.push(`${count(r.plugins, "plug-in", "plug-ins")} came across with ${r.plugins === 1 ? "its" : "their"} settings.`);
  if (r.missingPlugins?.length) {
    parts.push(`Not found on this computer (install or scan them in the Plug-in Manager): ${r.missingPlugins.join(", ")}.`);
  }
  if (r.converted) parts.push(`${count(r.converted, "phase shifter came", "phase shifters came")} across as Phase Curve.`);
  if (r.stretched) parts.push(`${count(r.stretched, "clip keeps its", "clips keep their")} stretch and pitch.`);
  if (r.skippedEffects > 0) {
    const names = r.skippedNames?.length ? ` (${r.skippedNames.join(", ")})` : "";
    parts.push(`${count(r.skippedEffects, "built-in effect was", "built-in effects were")} left out${names}; rebuild them in FX Chain.`);
  }
  if (r.missingFiles.length > 0) {
    const n = r.missingFiles.length;
    parts.push(`${n} media ${n === 1 ? "file was" : "files were"} not found.`);
  }
  return parts.join(" ");
}

export class SessionFiles {
  private timer: ReturnType<typeof setInterval> | undefined;
  private parked: ParkedSession[] = [];

  constructor(private readonly d: SessionFilesDeps) {}

  get dirty(): boolean {
    return this.d.stack.current.dirty;
  }

  /** Changed and worth saving: the blank session the app opened with, with files imported only, does not count. */
  get unsaved(): boolean {
    return this.dirty && !this.d.session.isScratch();
  }

  get name(): string {
    return displayName(this.d.session.path, this.d.session.current);
  }

  /** Window title, with a star while there are unsaved changes. */
  title(): string {
    return `${this.name}${this.unsaved ? " *" : ""} - Altered Audio Editor`;
  }

  startAutosave(intervalMs = AUTOSAVE_MS): void {
    clearInterval(this.timer);
    this.timer = setInterval(() => void this.autosaveNow(), intervalMs);
  }

  stopAutosave(): void {
    clearInterval(this.timer);
    this.timer = undefined;
  }

  /** Writes a recovery copy if there is anything unsaved. */
  async autosaveNow(): Promise<boolean> {
    if (!this.unsaved) return false;
    try {
      await this.d.backend.autosave(this.mediaRefs());
      return true;
    } catch {
      return false;
    }
  }

  /** Returns false if the user cancelled. */
  async confirmDiscard(): Promise<boolean> {
    if (!this.unsaved) return true;
    const choice = await this.d.askSave(this.name);
    if (choice === "cancel") return false;
    if (choice === "save") return this.save();
    await this.d.backend.clearAutosave().catch(() => {});
    return true;
  }

  /** Sessions open behind the one in front, oldest first. */
  get others(): readonly ParkedSession[] {
    return this.parked;
  }

  /** Several sessions can be open at once (the core supports setting them aside). */
  private get multi(): boolean {
    return !!(this.d.backend.park && this.d.backend.resume);
  }

  /**
   * Makes room for another session: sets the one in front aside with its
   * edits and history, or, with one session at a time, asks to save it.
   * A blank session nobody has touched is simply dropped.
   */
  private async makeRoom(): Promise<boolean> {
    if (!this.multi) return this.confirmDiscard();
    const doc = this.d.session.current;
    if (!doc || this.d.session.isScratch()) return true;
    await this.d.beforePark?.().catch(() => {});
    const dirty = this.unsaved;
    // Unsaved work set aside still gets a recovery copy.
    if (dirty) await this.autosaveNow();
    await this.d.backend.park!();
    const history = await this.d.stack.stash();
    this.parked = [
      ...this.parked.filter((p) => p.id !== doc.id),
      { id: doc.id, name: this.name, path: this.d.session.path, dirty, seconds: sessionEnd(doc) / Math.max(1, doc.sample_rate), sampleRate: doc.sample_rate, history },
    ];
    this.d.onSessions?.();
    return true;
  }

  /** Brings an open session to the front, setting the current one aside. */
  async switchTo(id: string): Promise<boolean> {
    if (this.d.session.current?.id === id) return true;
    const p = this.parked.find((x) => x.id === id);
    if (!p || !this.multi) return false;
    if (!(await this.makeRoom())) return false;
    const r = await this.d.backend.resume!(id);
    this.parked = this.parked.filter((x) => x.id !== id);
    this.d.session.set(r.session, r.path);
    this.d.pool.merge(r.session.media.map((m) => ({ id: m.id, path: m.path })));
    await this.d.stack.restore(p.history);
    this.d.onReplaced?.(r.session);
    this.d.onSessions?.();
    return true;
  }

  async newSession(options?: NewSessionOptions): Promise<boolean> {
    if (!(await this.makeRoom())) return false;
    const r = await this.d.backend.create(options);
    await this.replace(r, "New Session", false);
    return true;
  }

  async open(path?: string): Promise<boolean> {
    const target = path ?? (await this.d.backend.pickOpen());
    if (!target) return false;
    // Already open: bring it to the front.
    const key = (p: string | null) => (p ?? "").replace(/\\/g, "/").toLowerCase();
    if (key(this.d.session.path) === key(target)) return true;
    const already = this.parked.find((p) => key(p.path) === key(target));
    if (already) return this.switchTo(already.id);
    if (!(await this.makeRoom())) return false;
    let r: SessionResult<SessionDoc>;
    try {
      r = await this.d.backend.open(target);
    } catch (e) {
      this.dropRecent(target);
      throw e;
    }
    await this.replace(r, "Open", false);
    this.addRecent(r.path ?? target);
    return true;
  }

  /** Brings in a .sesx session as a new, unsaved session. */
  async importSesx(path?: string): Promise<boolean> {
    if (!(await this.makeRoom())) return false;
    const target = path ?? (await this.d.backend.pickSesx());
    if (!target) return false;
    const r = await this.d.backend.importSesx(target);
    // The summary replaces the generic missing-files notice.
    await this.replace({ ...r, missing: [] }, "Import Session", true);
    this.d.notice(sesxSummary(r.report));
    return true;
  }

  /**
   * Closes the session in front (asking to save first). Another open
   * session comes to the front, or a new blank one when it was the last.
   */
  async close(): Promise<boolean> {
    if (!(await this.confirmDiscard())) return false;
    const next = this.parked[this.parked.length - 1];
    if (next && this.multi) {
      const r = await this.d.backend.resume!(next.id);
      this.parked = this.parked.filter((x) => x.id !== next.id);
      this.d.session.set(r.session, r.path);
      this.d.pool.merge(r.session.media.map((m) => ({ id: m.id, path: m.path })));
      await this.d.stack.restore(next.history);
      this.d.onReplaced?.(r.session);
      this.d.onSessions?.();
      return true;
    }
    // The last one closed: back to the empty start, nothing listed.
    const r = await this.d.backend.create();
    await this.replace(r, "New Session", false);
    this.d.session.markScratch?.();
    this.d.onSessions?.();
    return true;
  }

  /** Closes one open session, in front or not. */
  async closeSession(id: string): Promise<boolean> {
    if (this.d.session.current?.id !== id && !(await this.switchTo(id))) return false;
    return this.close();
  }

  /** Closes every open session, asking to save each. Stops if one is cancelled. */
  async closeAll(): Promise<boolean> {
    while (this.parked.length) if (!(await this.close())) return false;
    return this.close();
  }

  /** Saves to the current file, or asks where on first save. Returns false if cancelled. */
  async save(): Promise<boolean> {
    if (!this.d.session.path) return this.saveAs();
    return this.saveTo(null);
  }

  async saveAs(): Promise<boolean> {
    const path = await this.d.backend.pickSave(this.name);
    if (!path) return false;
    return this.saveTo(path);
  }

  /** Offers to recover autosaves left by a crash. Call once at startup. */
  async checkRecovery(): Promise<void> {
    const found = await this.d.backend.recoverable();
    for (const r of found) {
      if (await this.d.askRecover(r)) {
        const res = await this.d.backend.recover(r.autosave);
        await this.replace(res, "Recovered", true);
        // Older copies of other sessions stay for the next start.
        return;
      }
      await this.d.backend.discardRecovery(r.note.session_id);
    }
  }

  recentPaths(): string[] {
    return this.d.recent.get();
  }

  private async saveTo(path: string | null): Promise<boolean> {
    const target = path ?? this.d.session.path;
    if (target && this.d.beforeSave) await this.d.beforeSave(target);
    const r = await this.d.backend.save(path, this.mediaRefs(), this.d.libraries?.() ?? []);
    this.d.session.set(r.session, r.path);
    await this.d.stack.markSaved();
    if (r.path) this.addRecent(r.path);
    this.d.notice(`Saved ${displayName(r.path, r.session)}.`);
    return true;
  }

  private async replace(r: SessionResult<SessionDoc>, base: string, unsaved: boolean): Promise<void> {
    this.d.session.set(r.session, r.path);
    this.d.pool.merge(r.session.media.map((m) => ({ id: m.id, path: m.path })));
    await this.d.stack.clear(base);
    if (unsaved) await this.d.stack.markUnsaved();
    this.d.onReplaced?.(r.session);
    if (r.missing.length > 0) {
      const n = r.missing.length;
      this.d.notice(`${n} media ${n === 1 ? "file was" : "files were"} not found. They show as errors in the Files panel.`);
    }
  }

  private mediaRefs(): MediaRefDoc[] {
    return this.d.pool.readyItems().map((it) => ({
      id: it.id,
      path: it.path,
      relative_path: null,
      sample_rate: it.info!.sampleRate,
      channels: it.info!.channels,
      length: it.info!.length,
    }));
  }

  private addRecent(path: string): void {
    const key = (p: string) => p.replace(/\\/g, "/").toLowerCase();
    const list = [path, ...this.d.recent.get().filter((p) => key(p) !== key(path))].slice(0, MAX_RECENT);
    this.d.recent.set(list);
  }

  private dropRecent(path: string): void {
    this.d.recent.set(this.d.recent.get().filter((p) => p !== path));
  }
}

const AUDIO_EXTENSIONS = new Set(["wav", "wave", "aif", "aiff", "aifc", "mp3", "flac", "m4a", "mp4", "aac", "ogg", "oga", "opus"]);

export interface OpenPlan {
  /** Session to open (.altaudio). Only the first one picked is opened. */
  session: string | null;
  /** .sesx session to import, when no .altaudio was picked. */
  sesx: string | null;
  audio: string[];
  /** Picked files that are not opened: extra sessions and unknown types. */
  skipped: string[];
}

/** Sorts files picked in File > Open by what happens to them. */
export function planOpen(paths: readonly string[]): OpenPlan {
  const plan: OpenPlan = { session: null, sesx: null, audio: [], skipped: [] };
  const ext = (p: string) => p.slice(p.lastIndexOf(".") + 1).toLowerCase();
  for (const p of paths) {
    const e = ext(p);
    if (e === "altaudio" && !plan.session) plan.session = p;
    else if (e === "sesx" && !plan.sesx) plan.sesx = p;
    else if (AUDIO_EXTENSIONS.has(e)) plan.audio.push(p);
    else plan.skipped.push(p);
  }
  if (plan.session && plan.sesx) {
    plan.skipped.push(plan.sesx);
    plan.sesx = null;
  }
  return plan;
}
