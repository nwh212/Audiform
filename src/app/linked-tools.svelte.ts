// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Linked sessions: another session's whole mix as one clip.
 *
 * Multitrack > Insert Session as Clip renders the picked session's mix to
 * a file in the Linked Sessions folder beside this session and places it
 * on the selected track (or a new one) at the playhead, in one undo step.
 * The clip plays that file, so the session inside costs no more than one
 * audio clip. Its media remembers the session and how its file looked;
 * when the session has been saved since (checked when this session comes
 * to the front and when the app window comes back into focus), the mix
 * is rendered again and swapped in, again as one undo step, and the clip
 * follows a longer or shorter mix as an edited file would.
 */

import { listen } from "@tauri-apps/api/event";
import { findClips } from "../panels/editor/clip-edit";
import { editor } from "../stores/editor";
import { sessionHistory } from "../stores/history";
import { session } from "../stores/open-session";
import type { MediaRefDoc, Op } from "../stores/session";
import type { ActionRegistry } from "./actions";
import { linkedCheck, linkedPick, linkedRender, type LinkedRender } from "./core";
import type { Exporting } from "./wave-tools.svelte";

export interface LinkedToolsDeps {
  actions: ActionRegistry;
  showError: (e: unknown) => void;
  showNotice: (text: string) => void;
  exporting: () => Exporting | null;
  setExporting: (v: Exporting | null) => void;
  folderBesideSession: (name: string, why: string) => Promise<string | null>;
  playheadNow: () => number;
  uuid: () => string;
  /** Brings a session file to the front (opening it if needed). */
  openSession: (path: string) => Promise<boolean>;
}

const FOLDER = "Linked Sessions";

/** The folder a file is in. */
const dirOf = (p: string) => p.slice(0, Math.max(p.lastIndexOf("/"), p.lastIndexOf("\\")));
const nameOf = (p: string) => p.split(/[\\/]/).pop() ?? p;

export function createLinkedTools(deps: LinkedToolsDeps) {
  const { actions, showError, showNotice, exporting, setExporting, folderBesideSession, playheadNow, uuid, openSession } = deps;
  /** Media already reported as missing, so the notice shows once. */
  const toldMissing = new Set<string>();
  let refreshing = false;

  /** Renders a session with the progress window up. */
  async function render(title: string, sessionPath: string, folder: string): Promise<LinkedRender | null> {
    setExporting({ title, progress: 0, floating: true });
    let off: (() => void) | null = null;
    try {
      off = await listen<{ done: number; total: number }>("export-progress", (e) => {
        if (exporting()) setExporting({ ...exporting()!, progress: e.payload.total > 0 ? e.payload.done / e.payload.total : 0 });
      });
      const r = await linkedRender(sessionPath, folder);
      return r.cancelled ? null : r;
    } finally {
      off?.();
      setExporting(null);
    }
  }

  /** Multitrack > Insert Session as Clip. */
  async function insert() {
    const doc = session.current;
    if (!doc || exporting()) return;
    const path = await linkedPick().catch((e) => (showError(e), null));
    if (!path) return;
    const folder = await folderBesideSession(FOLDER, "The linked session's mix is saved beside this session.");
    if (!folder) return;
    try {
      const r = await render(`Rendering ${nameOf(path)}`, path, folder);
      if (!r) {
        showNotice("Insert Session as Clip cancelled.");
        return;
      }
      const now = session.current;
      if (!now) return;
      const ops: Op[] = [];
      const sel = now.tracks.find((t) => t.id === editor.current().selectedTrack && t.kind !== "midi");
      let trackId = sel?.id;
      if (!trackId) {
        trackId = uuid();
        ops.push({ op: "add_track", id: trackId, name: r.name, stereo: true, index: now.tracks.length });
      }
      const mediaId = uuid();
      ops.push({
        op: "add_media",
        media: { id: mediaId, path: r.path, relative_path: null, sample_rate: r.sampleRate, channels: 2, length: r.frames, linked: { path, stamp: r.stamp } },
      });
      const clip = { id: uuid(), name: r.name, media_id: mediaId, start: Math.max(0, Math.round(playheadNow())), source_in: 0, source_out: r.frames };
      ops.push({ op: "add_clips", track_id: trackId, clips: [clip] });
      await session.edit("Insert Linked Session", { op: "batch", ops }, sessionHistory);
      editor.selectClips([clip.id], trackId);
      showNotice(`${r.name} is on the track as one clip. Save that session again and the clip follows.`);
    } catch (e) {
      showError(e);
    }
  }

  /** Linked media in the session in front. */
  const linkedMedia = (): MediaRefDoc[] => (session.current?.media ?? []).filter((m) => !!m.linked);

  /** Renders each of `list` again and swaps the new file in, one undo step each. */
  async function rerender(list: MediaRefDoc[]) {
    for (const m of list) {
      const link = m.linked!;
      const r = await render(`Updating ${nameOf(link.path)}`, link.path, dirOf(m.path) || (await folderBesideSession(FOLDER, "The linked session's mix is saved beside this session.")) || "");
      if (!r) return;
      const cur = session.current?.media.find((x) => x.id === m.id);
      if (!cur) continue;
      await session.edit(
        `Update Linked Session ${r.name}`,
        { op: "set_media", media: { ...cur, path: r.path, relative_path: null, sample_rate: r.sampleRate, channels: 2, length: r.frames, linked: { path: link.path, stamp: r.stamp } } },
        sessionHistory,
      );
      showNotice(`${r.name} was saved since its clip was rendered: the clip plays the new mix.`);
    }
  }

  /** Brings linked clips whose sessions were saved since up to date. */
  async function refresh() {
    if (refreshing || exporting()) return;
    const list = linkedMedia();
    if (list.length === 0) return;
    refreshing = true;
    try {
      const states = await linkedCheck(list.map((m) => ({ id: m.id, session: m.linked!.path, stamp: m.linked!.stamp })));
      for (const s of states.filter((x) => x.missing && !toldMissing.has(x.id))) {
        toldMissing.add(s.id);
        const m = list.find((x) => x.id === s.id);
        showNotice(`The linked session ${m ? nameOf(m.linked!.path) : ""} is not where it was. Its clip keeps the last mix.`);
      }
      await rerender(list.filter((m) => states.some((s) => s.id === m.id && s.stale)));
    } catch (e) {
      showError(e);
    } finally {
      refreshing = false;
    }
  }

  /** The linked media of the selected clips. */
  function selectedLinked(): MediaRefDoc[] {
    const doc = session.current;
    if (!doc) return [];
    const ids = new Set(findClips(doc, editor.current().selectedClips).map((f) => f.clip.media_id));
    return doc.media.filter((m) => ids.has(m.id) && !!m.linked);
  }

  // Check when this session comes to the front and when the window comes back.
  let lastId: string | null = null;
  const offSession = session.subscribe((d) => {
    const id = d?.id ?? null;
    if (id && id !== lastId) {
      lastId = id;
      setTimeout(() => void refresh(), 300);
    }
  });
  const onFocus = () => void refresh();
  window.addEventListener("focus", onFocus);

  const offActions = [
    actions.register({ id: "multitrack.insert-session", run: () => insert(), enabled: () => !!session.current && !exporting() }),
    actions.register({
      id: "clip.linked.update",
      run: async () => {
        try {
          await rerender(selectedLinked());
        } catch (e) {
          showError(e);
        }
      },
      enabled: () => selectedLinked().length > 0 && !exporting(),
    }),
    actions.register({
      id: "clip.linked.open",
      run: async () => {
        const m = selectedLinked()[0];
        if (m?.linked) await openSession(m.linked.path);
      },
      enabled: () => selectedLinked().length > 0,
    }),
  ];

  return {
    insert,
    refresh,
    destroy() {
      offSession();
      window.removeEventListener("focus", onFocus);
      offActions.forEach((off) => off());
    },
  };
}
