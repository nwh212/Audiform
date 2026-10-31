// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Clip commands for the Multitrack editor: split, trim, fades, level,
 * merge, unique copies, stretch, nudge and moving between tracks, split
 * channels, phase align, and the Clip menu's actions.
 */

import { newTrackNames } from "./track-names";
import { alignPair, ALIGN_MAX_MS, alignOps, describeAlign } from "../panels/editor/align";
import { findClips, mediaOf, clipsAt, splitClips, fadeClip, trimToPlayhead, trimToRange, loopChanges, copyMoved } from "../panels/editor/clip-edit";
import { unstretch } from "../panels/editor/stretch";
import { splitChannelsOp, splittable } from "../panels/properties/split-channels";
import { editor } from "../stores/editor";
import { sessionHistory } from "../stores/history";
import { mediaPool } from "../stores/media";
import { session } from "../stores/open-session";
import { type ClipChange, type Op, type ClipDoc, clipLength, AUDIO_TYPES } from "../stores/session";
import { exportRegionCopy, exportMerge, phaseAlign } from "./core";
import { listen } from "@tauri-apps/api/event";
import type { ActionRegistry } from "./actions";
import type { DockController } from "./docking/dock";
import type { Exporting } from "./wave-tools.svelte";

/** What clip editing needs from the rest of the app. */
export interface ClipToolsDeps {
  actions: ActionRegistry;
  dock: DockController;
  showError: (e: unknown) => void;
  showNotice: (text: string) => void;
  promptText: (title: string, label: string, value: string) => Promise<string | null>;
  uuid: () => string;
  inWave: () => boolean;
  playheadNow: () => number;
  timeSel: () => { start: number; end: number } | null;
  selectedClipDocs: () => ClipDoc[];
  folderBesideSession: (name: string, why: string) => Promise<string | null>;
  placeOps: (paths: string[], trackId: string | null, at: number, place?: { index?: number; name?: string }) => Promise<{ ops: Op[]; clips: { id: string }[]; target: string } | null>;
  exporting: () => Exporting | null;
  set_exporting: (v: Exporting | null) => void;
  colorFor: () => { ids: string[]; current: string | null } | null;
  set_colorFor: (v: { ids: string[]; current: string | null } | null) => void;
  fitFor: () => string | null;
  set_fitFor: (v: string | null) => void;
  stretchFor: () => string | null;
  set_stretchFor: (v: string | null) => void;
}

export function createClipTools(deps: ClipToolsDeps) {
  const { actions, dock, showError, showNotice, promptText, uuid, inWave, playheadNow, timeSel, selectedClipDocs, folderBesideSession, placeOps, exporting, colorFor, fitFor, stretchFor, set_exporting, set_colorFor, set_fitFor, set_stretchFor } = deps;


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

  async function editClips(label: string, changes: ClipChange[]) {
    if (changes.length > 0) await session.edit(label, { op: "set_clips", changes }, sessionHistory);
  }

  /**
   * Clip > Make Unique Copy: each selected clip gets its own copy of the
   * audio it plays (just its part of the file, untouched), so later
   * changes to it never reach other clips sharing the original. Gain,
   * fades and envelopes stay as they are. One undo step.
   */
  async function makeUniqueCopy() {
    const doc0 = session.current;
    if (!doc0 || exporting()) return;
    const found = findClips(doc0, editor.current().selectedClips).filter((f) => !f.clip.locked);
    if (found.length === 0) return;
    const folder = await folderBesideSession("Clip Copies", "Clip copies are kept in a folder next to it.");
    if (!folder) return;
    set_exporting({ title: found.length === 1 ? "Copying clip audio" : `Copying audio for ${found.length} clips`, progress: 0 });
    try {
      const paths: string[] = [];
      for (const [i, f] of found.entries()) {
        const m = doc0.media.find((x) => x.id === f.clip.media_id);
        if (!m) throw new Error(`The file for "${f.clip.name}" is missing from this session.`);
        paths.push(await exportRegionCopy(m.path, f.clip.source_in, f.clip.source_out, folder, `${f.clip.name} Copy`));
        set_exporting({ ...exporting()!, progress: (i + 1) / found.length });
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
      set_exporting(null);
    }
  }

  /**
   * Clip > Merge Clips: renders the selected clips (with their gain,
   * fades and envelopes, not the track's settings) into one new file in
   * a "Merged Clips" folder beside the session, then swaps them for one
   * clip. One undo step; the original files are untouched.
   */
  async function mergeClips() {
    const m = mergeTarget();
    if (!m || exporting()) return;
    const folder = await folderBesideSession("Merged Clips", "Merged clips are kept in a folder next to it.");
    if (!folder) return;
    const doc = session.current!;
    const start = m.clips[0]!.start;
    const end = Math.max(...m.clips.map((c) => c.start + clipLength(c, doc.media.find((x) => x.id === c.media_id), doc.sample_rate)));
    set_exporting({ title: "Merging clips", progress: 0 });
    let off: (() => void) | null = null;
    try {
      off = await listen<{ done: number; total: number }>("export-progress", (e) => {
        if (exporting()) set_exporting({ ...exporting()!, progress: e.payload.total > 0 ? e.payload.done / e.payload.total : 0 });
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
      set_exporting(null);
    }
  }

  function mergeReason(): string {
    const doc = session.current;
    const found = doc ? findClips(doc, editor.current().selectedClips) : [];
    if (found.length < 2) return "Select two or more clips on one track.";
    if (found.some((f) => f.clip.locked)) return "One of the selected clips is locked.";
    return "The selected clips must be on the same track.";
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

  /** Split Channels to Tracks: one track per channel under the clip's track, as one undoable step. */
  async function splitChannels() {
    const doc = session.current;
    if (!doc) return;
    const op = splitChannelsOp(doc, findClips(doc, editor.current().selectedClips).filter((f) => !f.clip.locked), uuid);
    if (!op) {
      showNotice("Select a clip from a file with two or more channels.");
      return;
    }
    await session.edit("Split Channels to Tracks", op, sessionHistory);
    showNotice("Split into one track per channel. The original clip is muted. The LFE track starts muted, as the default fold-down leaves it out.");
  }
  /** Clip > Nudge: the selected audio clips a little left or right; null when none can move. */
  /** Selected clips (not locked) whose file has two or more channels. */
  const splittableClips = () => {
    const doc = session.current;
    return doc ? splittable(doc, findClips(doc, editor.current().selectedClips).filter((f) => !f.clip.locked)) : [];
  };

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

  async function trimTo(edge: "start" | "end") {
    const doc = session.current;
    if (!doc) return;
    const at = playheadNow();
    const ids = clipsAt(doc, at, editor.current().selectedClips).filter((id) => editor.current().selectedClips.has(id));
    const changes = trimToPlayhead(doc, ids, at, edge);
    if (changes.length === 0) showNotice("Select a clip and put the playhead inside it.");
    await editClips(edge === "start" ? "Trim Start" : "Trim End", changes);
  }

  /** Selected clips moved one track up (-1) or down (1), or null when they can't all move. */
  /** The id a new track below the last one gets while a move is worked out. */
  const NEW_TRACK = "__new_track__";
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
        if (c.locked) return null;
        // Below the bottom track: a new track opens there.
        if (!to && !(dir > 0 && !listed[i]!.midi_clips?.length)) return null;
        changes.push({ id: c.id, track_id: to ? to.id : NEW_TRACK });
      }
    }
    return changes.length ? changes : null;
  }

  function clipsNudge(dir: -1 | 1): ClipChange[] | null {
    const d = session.current;
    const sel = editor.current().selectedClips;
    if (!d || sel.size === 0) return null;
    const step = Math.max(1, Math.round(d.view.samples_per_pixel * NUDGE_PX));
    const picked = d.tracks.flatMap((t) => t.clips.filter((c) => sel.has(c.id)));
    if (!picked.length || picked.some((c) => c.locked)) return null;
    // Moving left stops at the session start, keeping the clips' spacing.
    const by = dir < 0 ? -Math.min(step, Math.min(...picked.map((c) => c.start))) : step;
    return by === 0 ? null : picked.map((c) => ({ id: c.id, start: c.start + by }));
  }
  /**
   * Alt+Up and Alt+Down: copies of the selected clips on the track above
   * or below, the originals left where they are. Below the bottom track a
   * new, plain track opens for them, in the same undoable step. The copies
   * are selected, so pressing again copies on down the tracks.
   */
  async function moveClipsToTrack(dir: -1 | 1) {
    let changes = clipsTrackMove(dir);
    const d = session.current;
    if (!changes || !d) return;
    const adds: Op[] = [];
    if (changes.some((c) => c.track_id === NEW_TRACK)) {
      const last = d.tracks.filter((t) => !t.hidden).at(-1)!;
      const id = uuid();
      changes = changes.map((c) => (c.track_id === NEW_TRACK ? { ...c, track_id: id } : c));
      const name = newTrackNames(d.tracks.map((t) => t.name), 1)[0]!;
      adds.push({ op: "add_track", id, name, stereo: last.channels !== "mono", index: d.tracks.length } as Op);
    }
    const copy = copyMoved(d, changes.map((c) => c.id), changes, uuid);
    const label = adds.length ? "Copy to New Track Below" : dir < 0 ? "Copy to Track Above" : "Copy to Track Below";
    await session.edit(label, adds.length ? { op: "batch", ops: [...adds, copy.op] } : copy.op, sessionHistory);
    const target = changes[0]!.track_id;
    editor.selectClips(copy.ids, target ?? null);
  }
  /** How far one nudge moves clips: five pixels at the current zoom. */
  const NUDGE_PX = 5;

  /** Registers the menu actions. */
  function register(): (() => void)[] {
    return [
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
        id: "clip.auto-crossfade",
        run: () => editor.setAutoCrossfade(!editor.current().autoCrossfade),
        checked: () => editor.current().autoCrossfade,
      }),
      actions.register({
        id: "clip.color",
        run: () => {
          const clips = selectedClipDocs();
          if (clips.length) set_colorFor({ ids: clips.map((c) => c.id), current: clips[0]!.color });
        },
        enabled: () => editor.current().selectedClips.size > 0,
        reason: () => "Select a clip first.",
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
      ...([-1, 1] as const).map((dir) =>
        actions.register({
          id: dir < 0 ? "clip.to-track-above" : "clip.to-track-below",
          run: () => moveClipsToTrack(dir),
          enabled: () => clipsTrackMove(dir) !== null,
          reason: () => (editor.current().selectedClips.size === 0 ? "Select clips first." : dir < 0 ? "The clips are already on the top track, or one is locked." : "A selected clip is locked, or is on a MIDI track at the bottom."),
        }),
      ),
      ...([-1, 1] as const).map((dir) =>
        actions.register({
          id: dir < 0 ? "clip.nudge-left" : "clip.nudge-right",
          run: async () => {
            const changes = clipsNudge(dir);
            if (changes) await session.edit(dir < 0 ? "Nudge Left" : "Nudge Right", { op: "set_clips", changes }, sessionHistory);
          },
          enabled: () => clipsNudge(dir) !== null,
          reason: () => (editor.current().selectedClips.size === 0 ? "Select audio clips first." : "A selected clip is locked, or the clips are already at the start."),
        }),
      ),
      actions.register({
        id: "clip.split-channels",
        run: () => splitChannels(),
        enabled: () => !inWave() && exporting() === null && splittableClips().length > 0,
        reason: () => "Select a clip from a file with two or more channels.",
      }),
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
      actions.register({
        id: "clip.stretch",
        run: () => {
          const c = selectedClipDocs();
          if (c.length === 1) set_stretchFor(c[0]!.id);
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
          if (c.length === 1) set_fitFor(c[0]!.id);
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
        enabled: () => exporting() === null && selectedClipDocs().some((c) => !c.locked),
        reason: () => (exporting() ? "Wait for the current export to finish." : "Select a clip that is not locked."),
      }),
      actions.register({
        id: "clip.merge",
        run: () => mergeClips(),
        enabled: () => exporting() === null && mergeTarget() !== null,
        reason: () => (exporting() ? "Wait for the current export to finish." : mergeReason()),
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
    ];
  }

  return { register, editClips, removeStretch };
}
