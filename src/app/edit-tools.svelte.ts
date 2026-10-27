// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * The Edit menu: undo and redo, cut, copy, paste and delete in each area
 * of the window, selection by position, and keyboard shortcuts and settings.
 */

import { copyClips, pasteClips, rippleDeleteClips, rippleDeleteRange, gapAt } from "../panels/editor/clip-edit";
import { deleteClipsOps, allMidiClipIds } from "../panels/editor/midi";
import { newMarker, nextMarkerName } from "../panels/markers/markers";
import { editor } from "../stores/editor";
import { sessionHistory } from "../stores/history";
import { libraries } from "../stores/library";
import { mediaPool } from "../stores/media";
import { session } from "../stores/open-session";
import { AUDIO_TYPES } from "../stores/session";
import { settings } from "../stores/settings";
import { currentArea, type Area } from "./area";
import { nextClipId, anchorStart, clipsFrom, clipsUpTo } from "./clip-select";
import { playheadNow, contentEnd, sessionView, clipSpan } from "./navigation";
import type { ActionRegistry } from "./actions";
import type { DockController } from "./docking/dock";
import type { TrackDoc, Op } from "../stores/session";
import type { ClipBoard } from "../panels/editor/clip-edit";

/** What the Edit menu needs from the rest of the app. */
export interface EditToolsDeps {
  actions: ActionRegistry;
  dock: DockController;
  uuid: () => string;
  showNotice: (text: string) => void;
  promptText: (title: string, label: string, value: string) => Promise<string | null>;
  selectedTrack: () => TrackDoc | null;
  timeSel: () => { start: number; end: number } | null;
  openSettings: () => void;
  guarded: (label: string, build: () => Op | null, empty: string) => Promise<void>;
  removeFiles: (ids: string[]) => Promise<void>;
  closeWaveIfRemoved: (ids: readonly string[], remove: () => Promise<void>) => Promise<void>;
  clipBoard: () => ClipBoard | null;
  set_clipBoard: (v: ClipBoard | null) => void;
  shortcutsOpen: () => boolean;
  set_shortcutsOpen: (v: boolean) => void;
}

export function createEditTools(deps: EditToolsDeps) {
  const { actions, dock, uuid, showNotice, promptText, selectedTrack, timeSel, openSettings, guarded, removeFiles, closeWaveIfRemoved, clipBoard, shortcutsOpen, set_clipBoard, set_shortcutsOpen } = deps;

  const AREA_FIRST = "Click in the Files panel, Libraries, or the Editor first.";

  const areaOps = (): AreaOps | null => {
    const a = currentArea();
    return a ? AREA_OPS[a] : null;
  };

  function copySelected(): boolean {
    const doc = session.current;
    if (!doc) return false;
    set_clipBoard(copyClips(doc, editor.current().selectedClips));
    actions.invalidate();
    return clipBoard() !== null;
  }
  const inEditor = () => currentArea() === "multitrack";

  async function pasteAtPlayhead() {
    const doc = session.current;
    const board = clipBoard();
    if (!doc || !board) return;
    const sel = editor.current().selectedTrack;
    const row = Math.max(0, doc.tracks.findIndex((t) => t.id === sel));
    const r = pasteClips(doc, board, playheadNow(), row, uuid);
    if (!r) {
      showNotice("The copied clips' files are no longer in this session.");
      return;
    }
    await session.edit(r.ids.length === 1 ? "Paste Clip" : `Paste ${r.ids.length} Clips`, r.op, sessionHistory);
    editor.selectClips(r.ids);
  }

  /** Edit > Select: clips and time by position. */
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
          const v = sessionView();
          if (!v) return;
          const w = editor.current().width;
          session.setView({ ...v, selection: { start: Math.max(0, v.scroll), end: Math.max(0, v.scroll) + Math.round(w * v.samples_per_pixel) } });
        },
        enabled: () => session.current !== null,
      }),
      actions.register({
        id: "edit.select-all-time",
        run: () => {
          const v = sessionView();
          const end = contentEnd();
          if (v && end > 0) session.setView({ ...v, selection: { start: 0, end } });
        },
        enabled: () => contentEnd() > 0,
        reason: () => "The session has no clips yet.",
      }),
      actions.register({
        id: "edit.select-time-around-clips",
        run: () => {
          const v = sessionView();
          const span = clipSpan();
          if (v && span && editor.current().selectedClips.size > 0) session.setView({ ...v, selection: { start: span.first, end: span.last } });
        },
        enabled: () => editor.current().selectedClips.size > 0,
        reason: () => "Select clips first.",
      }),
      actions.register({
        id: "edit.clear-time-selection",
        run: () => {
          const v = sessionView();
          if (v) session.setView({ ...v, selection: null });
        },
        enabled: () => timeSel() !== null,
        reason: () => "There is no time selection.",
      }),
    ];
  };

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
        const ids = [...mediaPool.current.selected];
        await closeWaveIfRemoved(ids, () => removeFiles(ids));
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

  /** Registers the menu actions. */
  function register(): (() => void)[] {
    return [
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
      actions.register({ id: "edit.keyboard-shortcuts", run: () => void (set_shortcutsOpen(true)) }),
      actions.register({ id: "edit.settings", run: () => openSettings() }),
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
        enabled: () => inEditor() && clipBoard() !== null && (session.current?.tracks.length ?? 0) > 0,
        reason: () => (clipBoard() ? "Click in the Editor first." : "Copy or cut clips first."),
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
      actions.register({
        id: "edit.select-to-end",
        run: () => {
          const v = sessionView();
          if (!v) return;
          const end = contentEnd();
          const at = playheadNow();
          if (end > at) session.setView({ ...v, selection: { start: at, end } });
        },
        enabled: () => contentEnd() > 0,
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
    ];
  }

  return { register, selectClipActions };
}
