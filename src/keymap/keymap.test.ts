// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { chordFromEvent, parseChord } from "./chord";
import { decide } from "./dispatch";
import { DEFAULT_BINDINGS, Keymap } from "./keymap";
import defaultFile from "./default.json";
import { MENUS, menuActionIds } from "../app/menus/model";

/** The shortcut table in CLAUDE.md. The shipped keymap may contain these and nothing else. */
const APPROVED: Record<string, string[]> = {
  "edit.undo": ["Ctrl+Z"],
  "edit.redo": ["Ctrl+Y"],
  "file.save": ["Ctrl+S"],
  "file.save-as": ["Ctrl+Shift+S"],
  "file.open": ["Ctrl+O"],
  "file.new-session": ["Ctrl+N"],
  "edit.select-all": ["Ctrl+A"],
  "edit.cut": ["Ctrl+X"],
  "edit.copy": ["Ctrl+C"],
  "edit.paste": ["Ctrl+V"],
  "edit.delete": ["Delete", "Backspace"],
  "multitrack.play-pause": ["Space"],
  "multitrack.record": ["Shift+Space"],
  "clip.split": ["Ctrl+K"],
  "view.zoom-in": ["Ctrl+=", "="],
  "view.zoom-out": ["Ctrl+-", "-"],
  "edit.find": ["Ctrl+F"],
  // Approved by the project lead.
  "tool.razor": ["R"],
  "tool.move": ["V"],
  "tool.time": ["T"],
  "tool.slip": ["Y"],
  // Approved 2026-10-20, assigned in 0.1.9 (unbuilt features show as disabled menu items).
  "transport.go-start": ["Home"],
  "transport.go-end": ["End"],
  "transport.prev-edge": ["Ctrl+ArrowLeft"],
  "transport.next-edge": ["Ctrl+ArrowRight"],
  "transport.prev-marker": ["Ctrl+Alt+ArrowLeft"],
  "transport.next-marker": ["Ctrl+Alt+ArrowRight"],
  "edit.marker.cue": ["M", "Shift+*", "*"],
  "view.zoom-session": ["\\"],
  "view.zoom-selection": ["Shift+/", "?"],
  "view.zoom-out-full": ["Shift+\\", "|"],
  "view.zoom-all-axes": ["Ctrl+\\"],
  "view.editor.toggle": ["1"],
  "view.editor.multitrack": ["0"],
  "view.editor.waveform": ["9"],
  "view.spectrum": ["Shift+D"],
  "view.hud": ["Shift+U"],
  "view.zoom-in-vertical": ["Alt+="],
  "view.zoom-out-vertical": ["Alt+-"],
  "multitrack.return-on-stop": ["Shift+X"],
  "fx.hide-windows": ["Ctrl+Shift+H"],
  "multitrack.meter-input": ["Alt+I"],
  "edit.repeat-last": ["Shift+R"],
  "edit.repeat-last-dialog": ["Ctrl+R"],
  "file.convert-sample-type": ["Shift+T"],
  "fx.capture-noise-print": ["Shift+P"],
  "wave.channel-left": ["ArrowUp"],
  "wave.channel-right": ["ArrowDown"],
  "clip.nudge-left": ["Alt+,"],
  "clip.nudge-right": ["Alt+."],
  "clip.to-track-above": ["Alt+ArrowUp"],
  "clip.to-track-below": ["Alt+ArrowDown"],
  "edit.marker.cd-track": ["Shift+M"],
  "edit.marker.cart": ["C"],
  "multitrack.add.stereo": ["Alt+A"],
  "multitrack.add.bus": ["Alt+B"],
  "multitrack.add-tracks": ["Shift+A"],
  "multitrack.delete-empty": ["Shift+E"],
  "multitrack.delete-track": ["Shift+Delete"],
  "clip.group": ["Ctrl+G"],
  "clip.group-suspend": ["Ctrl+Shift+G"],
  "edit.ripple.clips": ["Shift+Backspace"],
  "edit.ripple.time-clips": ["Alt+Backspace"],
  "edit.ripple.time-all": ["Ctrl+Shift+Backspace"],
  "edit.deselect": ["Ctrl+Shift+A"],
  "edit.select-next-clip": ["."],
  "edit.select-clips-track-end": ["Ctrl+Alt+T"],
  "edit.clear-time-selection": ["G"],
};

const key = (
  k: string,
  m: Partial<{ ctrl: boolean; alt: boolean; shift: boolean }> = {},
) => ({
  key: k,
  ctrlKey: m.ctrl ?? false,
  altKey: m.alt ?? false,
  shiftKey: m.shift ?? false,
  metaKey: false,
  repeat: false,
  defaultPrevented: false,
});

describe("chords", () => {
  it("canonicalizes spelling and modifier order", () => {
    expect(parseChord("shift+ctrl+k")).toBe("Ctrl+Shift+K");
    expect(parseChord("Control + del")).toBe("Ctrl+Delete");
    expect(parseChord("spacebar")).toBe("Space");
    expect(parseChord("ctrl+=")).toBe("Ctrl+=");
    expect(parseChord("Ctrl++")).toBe("Ctrl++");
    expect(parseChord("f5")).toBe("F5");
  });

  it("rejects nonsense", () => {
    expect(parseChord("")).toBeNull();
    expect(parseChord("Hyper+K")).toBeNull();
    expect(parseChord("Ctrl+Banana")).toBeNull();
  });

  it("reads key events", () => {
    expect(chordFromEvent(key("z", { ctrl: true }))).toBe("Ctrl+Z");
    expect(chordFromEvent(key(" "))).toBe("Space");
    expect(chordFromEvent(key("-", { ctrl: true }))).toBe("Ctrl+-");
    expect(chordFromEvent(key("Control", { ctrl: true }))).toBeNull();
  });

  it("reads the physical key when Ctrl+Alt types something else (AltGr on Windows)", () => {
    const ev = (k: string, code: string) => ({ key: k, code, ctrlKey: true, altKey: true, shiftKey: false, metaKey: false });
    expect(chordFromEvent(ev("\x7f", "Backspace"))).toBe("Ctrl+Alt+Backspace");
    expect(chordFromEvent(ev("\b", "Backspace"))).toBe("Ctrl+Alt+Backspace");
    expect(chordFromEvent(ev("Backspace", "Backspace"))).toBe("Ctrl+Alt+Backspace");
    expect(chordFromEvent(ev("ŧ", "KeyT"))).toBe("Ctrl+Alt+T");
    expect(chordFromEvent(ev("ArrowLeft", "ArrowLeft"))).toBe("Ctrl+Alt+ArrowLeft");
  });
});

describe("shipped keymap", () => {
  it("matches the approved shortcut table exactly", () => {
    expect(DEFAULT_BINDINGS).toEqual(APPROVED);
    expect(defaultFile.format).toBe("altaudio.keymap");
    expect(defaultFile.schema_version).toBe(1);
  });

  it("binds only actions that exist in the menus", () => {
    const ids = new Set(menuActionIds(MENUS));
    for (const id of Object.keys(DEFAULT_BINDINGS))
      expect(ids.has(id), id).toBe(true);
  });

  it("never binds one chord twice", () => {
    const all = Object.values(DEFAULT_BINDINGS).flat();
    expect(new Set(all).size).toBe(all.length);
  });
});

describe("Keymap", () => {
  it("looks up actions by chord", () => {
    const km = new Keymap();
    expect(km.actionFor("Ctrl+Z")).toBe("edit.undo");
    expect(km.actionFor("Backspace")).toBe("edit.delete");
    expect(km.displayFor("edit.delete")).toBe("Delete");
  });

  it("reassigns a chord and takes it from its old owner", () => {
    const km = new Keymap();
    const lost = km.assign("edit.find", "ctrl+k");
    expect(lost).toBe("clip.split");
    expect(km.actionFor("Ctrl+K")).toBe("edit.find");
    expect(km.chordsFor("clip.split")).toEqual([]);
    expect(km.actionFor("Ctrl+F")).toBeUndefined();
  });

  it("saves only the user's changes", () => {
    const km = new Keymap();
    km.assign("help.about", "F1");
    expect(km.getOverrides()).toEqual({ "help.about": ["F1"] });
    km.assign("help.about", "F1");
    expect(km.isChanged("edit.undo")).toBe(false);
  });

  it("forgets a change that matches the default again", () => {
    const km = new Keymap();
    km.clear("edit.undo");
    expect(km.isChanged("edit.undo")).toBe(true);
    km.assign("edit.undo", "Ctrl+Z");
    expect(km.isChanged("edit.undo")).toBe(false);
  });

  it("resets one action and takes its chord back", () => {
    const km = new Keymap();
    km.assign("help.about", "Ctrl+Z");
    expect(km.actionFor("Ctrl+Z")).toBe("help.about");
    km.reset("edit.undo");
    expect(km.actionFor("Ctrl+Z")).toBe("edit.undo");
    expect(km.chordsFor("help.about")).toEqual([]);
  });

  it("resets everything", () => {
    const km = new Keymap();
    km.assign("help.about", "F1");
    km.clear("edit.undo");
    km.resetAll();
    expect(km.getOverrides()).toEqual({});
    expect(km.actionFor("Ctrl+Z")).toBe("edit.undo");
  });

  it("loads saved changes and ignores invalid chords", () => {
    const km = new Keymap();
    km.setOverrides({ "edit.undo": ["ctrl+u", "Nope+Nope"] });
    expect(km.chordsFor("edit.undo")).toEqual(["Ctrl+U"]);
    expect(km.actionFor("Ctrl+Z")).toBeUndefined();
  });

  it("rejects an invalid chord on assign", () => {
    expect(() => new Keymap().assign("edit.undo", "Banana")).toThrow();
  });
});

describe("dispatch", () => {
  const opts = { inTextField: false, modalOpen: false, release: true };

  it("runs the bound action and stops the webview default", () => {
    expect(decide(key("z", { ctrl: true }), new Keymap(), opts)).toEqual({
      action: "edit.undo",
      prevent: true,
    });
  });

  it("leaves text fields alone", () => {
    expect(
      decide(key("z", { ctrl: true }), new Keymap(), {
        ...opts,
        inTextField: true,
      }),
    ).toEqual({
      prevent: false,
    });
  });

  it("does nothing while a dialog is open", () => {
    expect(
      decide(key(" "), new Keymap(), { ...opts, modalOpen: true }).action,
    ).toBeUndefined();
  });

  it("never lets Ctrl+A select the page's text under a dialog", () => {
    expect(
      decide(key("a", { ctrl: true }), new Keymap(), {
        ...opts,
        modalOpen: true,
      }),
    ).toEqual({ prevent: true });
    expect(
      decide(key("a", { ctrl: true }), new Keymap(), {
        ...opts,
        modalOpen: true,
        inTextField: true,
      }).prevent,
    ).toBe(false);
  });

  it("fires Space once when held", () => {
    expect(decide({ ...key(" "), repeat: true }, new Keymap(), opts)).toEqual({
      prevent: true,
    });
  });

  it("repeats undo when held", () => {
    expect(
      decide({ ...key("z", { ctrl: true }), repeat: true }, new Keymap(), opts)
        .action,
    ).toBe("edit.undo");
  });

  it("blocks reload in release builds only", () => {
    expect(decide(key("F5"), new Keymap(), opts)).toEqual({ prevent: true });
    expect(
      decide(key("F5"), new Keymap(), { ...opts, release: false }),
    ).toEqual({ prevent: false });
  });

  it("skips keys another handler already took", () => {
    expect(
      decide({ ...key(" "), defaultPrevented: true }, new Keymap(), opts),
    ).toEqual({ prevent: false });
  });
});
