// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { UndoStack } from "../commands";
import { MediaPool } from "../stores/media-pool";
import { SessionStore, type SessionDoc } from "../stores/session";
import type { Recoverable, SessionResult } from "./core";
import { displayName, isBlank, planOpen, SessionFiles, type Choice, type SessionFileBackend } from "./session-files";

function doc(name = "Untitled Session", media: SessionDoc["media"] = []): SessionDoc {
  return {
    format: "altaudio.session",
    schema_version: 1,
    id: `id-${name}`,
    name,
    sample_rate: 48000,
    bit_depth: 32,
    media,
    tracks: [],
    markers: [],
    view: { samples_per_pixel: 1024, scroll: 0, scroll_y: 0, playhead: 0, selection: null },
    libraries: [],
  };
}

const result = (session: SessionDoc, path: string | null, missing: string[] = []): SessionResult<SessionDoc> => ({
  session,
  path,
  missing,
});

/** A session with something in it worth saving (a marker). */
const withContent = (d: SessionDoc): SessionDoc => ({ ...d, markers: [{ id: "mk", name: "Intro", kind: "cue", start: 0 } as unknown as SessionDoc["markers"][number]] });

function setup(opts: { choice?: Choice; pickSave?: string | null; recoverable?: Recoverable[]; recover?: boolean; blank?: boolean } = {}) {
  const calls: string[] = [];
  let recent: string[] = [];
  const notices: string[] = [];
  let savedPath: string | null = null;
  const backend: SessionFileBackend = {
    create: async () => (calls.push("create"), result(doc(), null)),
    save: async (path, media, _libs) => {
      calls.push(`save:${path}:${media.length}`);
      savedPath = path ?? savedPath;
      return result(doc("Ep"), savedPath);
    },
    open: async (path) => (
      calls.push(`open:${path}`),
      result(doc("Opened", [{ id: "m1", path: "C:\\a.wav", relative_path: null, sample_rate: 48000, channels: 2, length: 10 }]), path, ["m1"])
    ),
    autosave: async (media) => void calls.push(`autosave:${media.length}`),
    clearAutosave: async () => void calls.push("clear"),
    recoverable: async () => opts.recoverable ?? [],
    recover: async (p) => (calls.push(`recover:${p}`), result(doc("Recovered"), "C:\\Shows\\R.altaudio")),
    discardRecovery: async (id) => void calls.push(`discard:${id}`),
    pickSave: async () => (opts.pickSave === undefined ? "C:\\Shows\\New.altaudio" : opts.pickSave),
    pickOpen: async () => "C:\\Shows\\Picked.altaudio",
    pickSesx: async () => "C:\\Shows\\EP440.sesx",
    importSesx: async (p) => (
      calls.push(`sesx:${p}`),
      { ...result(doc("EP440"), null), report: { tracks: 6, clips: 6, skippedEffects: 19, missingFiles: ["M:\\x.wav"] } }
    ),
  };
  const session = new SessionStore({
    get: async () => ({ session: doc(), path: null }),
    apply: async () => {
      throw new Error("unused");
    },
    setView: async () => {},
  });
  session.set(opts.blank ? doc() : withContent(doc()), null);
  const pool = new MediaPool({ expand: async (p) => p, probe: async () => ({ path: "", name: "", format: "WAV", codec: "PCM", sampleRate: 48000, channels: 2, length: 10, bitDepth: 16, fileSize: 1 }) });
  const stack = new UndoStack();
  const files = new SessionFiles({
    backend,
    session,
    pool,
    stack,
    askSave: async () => opts.choice ?? "save",
    askRecover: async () => opts.recover ?? true,
    recent: { get: () => recent, set: (v) => (recent = v) },
    notice: (t) => void notices.push(t),
  });
  const makeDirty = () => stack.execute({ label: "Edit", do() {}, undo() {} });
  return { files, calls, session, stack, pool, notices, recent: () => recent, makeDirty };
}

describe("SessionFiles", () => {
  it("imports a .sesx session as unsaved work and says what came across", async () => {
    const { files, calls, stack, notices, session } = setup();
    expect(await files.importSesx()).toBe(true);
    expect(calls).toEqual(["sesx:C:\\Shows\\EP440.sesx"]);
    expect(session.path).toBeNull();
    expect(stack.current.dirty).toBe(true);
    expect(notices.at(-1)).toBe(
      "Imported 6 tracks and 6 clips. 19 built-in effects were left out; rebuild them in Audio FX. 1 media file was not found.",
    );
  });

  it("names the window after the file and marks unsaved changes", async () => {
    const { files, makeDirty } = setup();
    expect(files.title()).toBe("Untitled Session - Audiform");
    await makeDirty();
    expect(files.title()).toBe("Untitled Session * - Audiform");
    expect(displayName("C:\\Shows\\Show 440.altaudio", null)).toBe("Show 440");
  });

  it("the blank session the app opened with is not one to save", async () => {
    const { files, calls, makeDirty, session } = setup({ blank: true });
    await session.load();
    // Files imported into the Files panel only: no star, no question, no recovery copy.
    await makeDirty();
    expect(files.dirty).toBe(true);
    expect(files.unsaved).toBe(false);
    expect(files.title()).toBe("Untitled Session - Audiform");
    expect(await files.autosaveNow()).toBe(false);
    expect(await files.newSession()).toBe(true);
    expect(calls).toEqual(["create"]);
    expect(isBlank(withContent(doc()))).toBe(false);
  });

  it("a new session, or a track added, is a real session (tester report: tracks would not show)", async () => {
    const { files, session, makeDirty } = setup({ blank: true });
    await session.load();
    expect(session.isScratch()).toBe(true);
    // A track added to the launch session ends it being scratch.
    session.set({ ...doc(), tracks: [{ id: "t1", name: "Track 1", clips: [] } as unknown as SessionDoc["tracks"][number]] });
    expect(session.isScratch()).toBe(false);
    // So does a session made with New Session, even with nothing in it.
    await files.newSession();
    expect(session.current?.id).toBe("id-Untitled Session");
    session.set({ ...doc(), id: "fresh" });
    await makeDirty();
    expect(session.isScratch()).toBe(false);
    expect(files.unsaved).toBe(true);
  });

  it("asks where to save the first time, then saves in place", async () => {
    const { files, calls, session, stack, makeDirty, recent } = setup();
    await makeDirty();
    expect(await files.save()).toBe(true);
    expect(calls).toEqual(["save:C:\\Shows\\New.altaudio:0"]);
    expect(session.path).toBe("C:\\Shows\\New.altaudio");
    expect(stack.current.dirty).toBe(false);
    await files.save();
    expect(calls[1]).toBe("save:null:0");
    expect(recent()).toEqual(["C:\\Shows\\New.altaudio"]);
  });

  it("cancelling the save dialog keeps the changes", async () => {
    const { files, calls, makeDirty, stack } = setup({ pickSave: null });
    await makeDirty();
    expect(await files.save()).toBe(false);
    expect(calls).toEqual([]);
    expect(stack.current.dirty).toBe(true);
  });

  it("asks before replacing unsaved work and respects Cancel", async () => {
    const { files, calls, makeDirty } = setup({ choice: "cancel" });
    await makeDirty();
    expect(await files.newSession()).toBe(false);
    expect(calls).toEqual([]);
  });

  it("Don't Save drops the recovery copy and continues", async () => {
    const { files, calls, makeDirty, stack } = setup({ choice: "discard" });
    await makeDirty();
    expect(await files.open()).toBe(true);
    expect(calls).toEqual(["clear", "open:C:\\Shows\\Picked.altaudio"]);
    expect(stack.current.entries).toHaveLength(0);
    expect(stack.current.baseLabel).toBe("Open");
  });

  it("opening fills the Files panel and reports missing media", async () => {
    const { files, pool, notices } = setup();
    await files.open("C:\\Shows\\Ep.altaudio");
    expect(pool.current.items.map((i) => i.id)).toEqual(["m1"]);
    expect(notices[0]).toMatch(/1 media file was not found/);
  });

  it("autosaves only when there are unsaved changes", async () => {
    const { files, calls, makeDirty } = setup();
    expect(await files.autosaveNow()).toBe(false);
    await makeDirty();
    expect(await files.autosaveNow()).toBe(true);
    expect(calls).toEqual(["autosave:0"]);
  });

  it("recovers an autosave as unsaved work, or discards it", async () => {
    const rec: Recoverable = {
      autosave: "/data/autosave/x.altaudio",
      note: { session_id: "x", name: "Ep", original_path: null, saved_at: "2026-10-14T03:00:00Z" },
    };
    const yes = setup({ recoverable: [rec], recover: true });
    await yes.files.checkRecovery();
    expect(yes.calls).toEqual(["recover:/data/autosave/x.altaudio"]);
    expect(yes.stack.current.dirty).toBe(true);
    expect(yes.session.current?.name).toBe("Recovered");

    const no = setup({ recoverable: [rec], recover: false });
    await no.files.checkRecovery();
    expect(no.calls).toEqual(["discard:x"]);
  });
});

describe("planOpen", () => {
  it("sorts picked files by type", () => {
    expect(planOpen(["C:/a/EP1.altaudio", "C:/a/host.WAV", "C:/a/old.sesx", "C:/a/notes.txt", "C:/a/b.altaudio", "C:/a/g.mp3"])).toEqual({
      session: "C:/a/EP1.altaudio",
      sesx: null,
      audio: ["C:/a/host.WAV", "C:/a/g.mp3"],
      skipped: ["C:/a/notes.txt", "C:/a/b.altaudio", "C:/a/old.sesx"],
    });
    expect(planOpen(["/x/old.sesx", "/x/a.flac"])).toEqual({ session: null, sesx: "/x/old.sesx", audio: ["/x/a.flac"], skipped: [] });
  });
});

describe("several open sessions", () => {
  it("sets the session in front aside on Open, switches back with its history, and closes to the next one", async () => {
    const { files, session, stack, makeDirty } = setup();
    const parked = new Map<string, { s: SessionDoc; p: string | null }>();
    const b = (files as unknown as { d: { backend: SessionFileBackend } }).d.backend;
    b.park = async () => void parked.set(session.current!.id, { s: session.current!, p: session.path });
    b.resume = async (id) => {
      const x = parked.get(id)!;
      parked.delete(id);
      return result(x.s, x.p);
    };
    await makeDirty();
    const first = session.current!.id;
    expect(await files.open("C:\\Shows\\Two.altaudio")).toBe(true);
    expect(files.others.map((o) => o.id)).toEqual([first]);
    expect(files.others[0]!.dirty).toBe(true);
    expect(stack.current.dirty).toBe(false);

    // Back to the first: its unsaved edit comes with it.
    expect(await files.switchTo(first)).toBe(true);
    expect(session.current!.id).toBe(first);
    expect(stack.current.canUndo).toBe(true);
    expect(files.others.map((o) => o.name)).toEqual(["Two"]);

    // Opening the second again just brings it to the front.
    expect(await files.open("C:\\Shows\\Two.altaudio")).toBe(true);
    expect(files.others.map((o) => o.id)).toEqual([first]);

    // Closing the one in front brings the other back.
    expect(await files.close()).toBe(true);
    expect(session.current!.id).toBe(first);
    expect(files.others).toEqual([]);
  });
});
