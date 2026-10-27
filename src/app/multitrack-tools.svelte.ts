// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * The Multitrack menu: adding and deleting tracks and buses, track
 * visibility, host profiles, Speaker Balance, playback and recording
 * commands, and inserting files as tracks.
 */

import { showOnly, hasClipIn } from "../panels/editor/visibility";
import { spaceToggle as spaceOnOff, LAYOUT_NAMES } from "../panels/space/space";
import { editor } from "../stores/editor";
import { sessionHistory } from "../stores/history";
import { mediaPool } from "../stores/media";
import { session, transport } from "../stores/open-session";
import { prefs } from "../stores/prefs";
import { type Op, type TrackDoc, clipLength } from "../stores/session";
import { loudnessCheck, pickMediaFiles, pickTrackFiles } from "./core";
import { profileById, profileOp, HOST_PROFILES } from "./host-profiles";
import { type LoudnessReport } from "./loudness";
import { sessionView } from "./navigation";
import { balanceItems, balance, describeBalance } from "./speaker-balance";
import { newTrackNames, trackNameFromFile } from "./track-names";
import type { ActionRegistry } from "./actions";
import type { DockController } from "./docking/dock";
import type { TransportStore } from "../stores/transport";
import type { PanelId } from "./docking/registry";
import type { GenerateMode } from "./GenerateDialog.svelte";

/** What the Multitrack menu needs from the rest of the app. */
export interface MultitrackToolsDeps {
  actions: ActionRegistry;
  dock: DockController;
  uuid: () => string;
  showNotice: (text: string) => void;
  showError: (e: unknown) => void;
  inWave: () => boolean;
  selectedTrack: () => TrackDoc | null;
  timeSel: () => { start: number; end: number } | null;
  nextTrackName: (tracks: readonly { name: string }[]) => string;
  profileTrack: () => TrackDoc | null;
  withUnzipped: (paths: string[]) => Promise<string[]>;
  stem: (name: string) => string;
  spaceToggle: (t: TransportStore) => Promise<void>;
  toggleRecord: () => Promise<void>;
  finishRecord: () => Promise<void>;
  placeFiles: (paths: string[], trackId: string | null, at: number, place?: { index?: number; name?: string }) => Promise<void>;
  rec: { readonly active: boolean; optionsOpen: boolean };
  balancing: () => boolean;
  set_balancing: (v: boolean) => void;
  propertiesOpen: () => boolean;
  set_propertiesOpen: (v: boolean) => void;
  addTracksOpen: () => boolean;
  set_addTracksOpen: (v: boolean) => void;
  generateMode: () => GenerateMode | null;
  set_generateMode: (v: GenerateMode | null) => void;
  openPanels: () => ReadonlySet<PanelId>;
  set_openPanels: (v: ReadonlySet<PanelId>) => void;
}

export function createMultitrackTools(deps: MultitrackToolsDeps) {
  const { actions, dock, uuid, showNotice, showError, inWave, selectedTrack, timeSel, nextTrackName, profileTrack, withUnzipped, stem, spaceToggle, toggleRecord, finishRecord, placeFiles, rec, balancing, propertiesOpen, addTracksOpen, generateMode, openPanels, set_balancing, set_propertiesOpen, set_addTracksOpen, set_generateMode, set_openPanels } = deps;


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
    showNotice(`Added Bus ${n}. Route a track to it with the track's output or a send (the track header, made taller, the right-click menu, or the Mixer).`);
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
  async function runSpeakerBalance() {
    const doc = session.current;
    if (!doc || balancing()) return;
    const items = balanceItems(doc);
    if (items.length < 2) return showNotice("Speaker Balance needs at least two voice tracks with audio.");
    set_balancing(true);
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
      set_balancing(false);
    }
  }
  /** What Delete Track removes: the picked track or bus, else the track holding the picked clips. */
  function trackToDelete(): { kind: "track" | "bus"; id: string } | null {
    const doc = session.current;
    if (!doc) return null;
    const ed = editor.current();
    if (ed.selectedTrack) {
      if (doc.tracks.some((t) => t.id === ed.selectedTrack)) return { kind: "track", id: ed.selectedTrack };
      if ((doc.buses ?? []).some((b) => b.id === ed.selectedTrack)) return { kind: "bus", id: ed.selectedTrack };
    }
    const clip = [...ed.selectedClips][0];
    const t = clip ? doc.tracks.find((x) => x.clips.some((c) => c.id === clip) || (x.midi_clips ?? []).some((c) => c.id === clip)) : undefined;
    return t ? { kind: "track", id: t.id } : null;
  }

  /** Return to Start on Stop, kept between runs. */
  const KEY_RETURN_ON_STOP = "transport.return-on-stop";

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

  /** Registers the menu actions. */
  function register(): (() => void)[] {
    return [
      actions.register({ id: "multitrack.play-pause", run: () => spaceToggle(transport) }),
      actions.register({
        id: "multitrack.record",
        run: () => toggleRecord(),
        label: () => (rec.active ? "Stop Recording" : "Record"),
      }),
      actions.register({ id: "multitrack.pause", run: () => transport.pause(), enabled: () => transport.current.playing || transport.current.paused }),
      actions.register({ id: "multitrack.stop", run: () => (rec.active ? finishRecord() : transport.stop()) }),
      actions.register({
        id: "multitrack.properties",
        enabled: () => session.current !== null,
        run: () => {
          set_propertiesOpen(true);
        },
      }),
      actions.register({ id: "multitrack.add.mono", run: () => addTrack(false) }),
      actions.register({
        id: "multitrack.add-tracks",
        run: () => {
          set_addTracksOpen(true);
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
          const t = trackToDelete();
          if (t?.kind === "track") await session.edit("Delete Track", { op: "remove_track", id: t.id }, sessionHistory);
          else if (t?.kind === "bus") await session.edit("Delete Bus", { op: "remove_bus", id: t.id }, sessionHistory);
          editor.selectTrack(null);
        },
        enabled: () => trackToDelete() !== null,
        reason: () => "Select a track first.",
      }),
      actions.register({
        id: "multitrack.duplicate-track",
        run: async () => {
          const doc = session.current;
          const t = selectedTrack();
          if (!doc || !t) return;
          const copy = freshIds(JSON.parse(JSON.stringify(t)) as typeof t);
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
      actions.register({
        id: "multitrack.insert-files",
        run: async () => {
          const paths = await pickMediaFiles();
          if (paths.length) await placeFiles(paths, editor.current().selectedTrack, sessionView()?.playhead ?? 0);
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
      actions.register({
        id: "multitrack.insert-selected-tracks",
        run: () => placeAsTracks(mediaPool.selectedItems().map((it) => it.path), 0),
        enabled: () => session.current !== null && mediaPool.selectedItems().length > 0,
        reason: () => (session.current ? "Select files in the Files panel first." : "Open or create a session first."),
      }),
      actions.register({
        id: "multitrack.return-on-stop",
        run: () => {
          transport.returnOnStop = !transport.returnOnStop;
          prefs.set(KEY_RETURN_ON_STOP, transport.returnOnStop);
        },
        checked: () => transport.returnOnStop,
      }),
      actions.register({
        id: "multitrack.insert-silence",
        run: () => {
          set_generateMode("silence");
        },
        enabled: () => !inWave() && (session.current?.tracks.length ?? 0) > 0,
        reason: () => "Insert Silence works in the Multitrack editor.",
      }),
      actions.register({
        id: "multitrack.record-options",
        run: () => {
          rec.optionsOpen = true;
        },
      }),
      actions.register({
        id: "multitrack.speaker-balance",
        run: () => void runSpeakerBalance().catch(showError),
        enabled: () => !balancing() && session.current !== null && balanceItems(session.current).length >= 2,
        reason: () => (balancing() ? "Measuring now." : "Needs two or more voice tracks with audio (music, effects and ambience tracks are left out)."),
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
        id: "multitrack.space",
        run: () => {
          if (!openPanels().has("space")) dock.toggle("space");
        },
      }),
      actions.register({
        id: "multitrack.space-toggle",
        run: () => {
          const d = session.current;
          if (!d) return;
          const t = spaceOnOff(d);
          void session.edit(t.label, { op: "set_space", space: t.space }, sessionHistory).then(() => {
            if (t.space) showNotice(`3D Space is on (${LAYOUT_NAMES[t.space.layout]}). Tracks play from their spots in the room.`);
            else showNotice("3D Space is off: a stereo session again. Spots are kept for when you turn it back on.");
          }, showError);
        },
        enabled: () => session.current !== null,
        checked: () => !!session.current?.space,
      }),
    ];
  }

  return { register, KEY_RETURN_ON_STOP, visibilityActions };
}
