// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { MENUS, commandList, menuActionIds, type MenuItem } from "./model";

function allItems(items: readonly MenuItem[]): MenuItem[] {
  return items.flatMap((it) =>
    it.kind === "submenu" ? [it, ...allItems(it.items)] : [it],
  );
}

describe("menu model", () => {
  it("has a mnemonic letter in every menu label, all different", () => {
    const letters = MENUS.map((m) => m.mnemonic.toLowerCase());
    expect(new Set(letters).size).toBe(letters.length);
    for (const m of MENUS)
      expect(m.label.toLowerCase()).toContain(m.mnemonic.toLowerCase());
  });

  it("never starts, ends or doubles a separator", () => {
    const check = (items: readonly MenuItem[]) => {
      items.forEach((it, i) => {
        if (it.kind === "sep") {
          expect(i).toBeGreaterThan(0);
          expect(i).toBeLessThan(items.length - 1);
          expect(items[i + 1]?.kind).not.toBe("sep");
        }
        if (it.kind === "submenu") check(it.items);
      });
    };
    for (const m of MENUS) check(m.items);
  });

  // The list of names to keep out lives in private/banned-terms.txt (one
  // regular expression per line), which is never committed. Without it,
  // for example in CI, the check is skipped.
  const found = import.meta.glob<string>("../../../private/banned-terms.txt", {
    query: "?raw",
    import: "default",
    eager: true,
  });
  const bannedText = Object.values(found)[0];
  it.skipIf(bannedText === undefined)(
    "does not use other products' feature names",
    () => {
      const terms = (bannedText ?? "")
        .split(/\r?\n/)
        .map((l) => l.trim())
        .filter(Boolean);
      const banned = new RegExp(terms.join("|"), "i");
      for (const m of MENUS) {
        for (const it of allItems(m.items)) {
          if (it.kind !== "sep") expect(it.label).not.toMatch(banned);
        }
      }
    },
  );

  it("gives every unbuilt item a reason", () => {
    const built = new Set([
      "edit.undo",
      "edit.redo",
      "file.exit",
      "window.reset-layout",
      "window.minimize",
      "edit.keyboard-shortcuts",
      "edit.settings",
      "multitrack.speaker-balance",
      "multitrack.profile.close-mic",
      "multitrack.profile.standard-mic",
      "multitrack.profile.remote-guest",
      "multitrack.profile.clear",
      "help.shortcuts",
      "help.website",
      "fx.invert",
      "fx.reverse",
      "help.discord",
      "help.update",
      "help.github",
      "help.report",
      "help.about",
      "file.import-files",
      "file.import-folder",
      "file.reveal",
      "edit.delete",
      "edit.select-all",
      "edit.deselect",
      "edit.find",
      "multitrack.add.mono",
      "multitrack.add.stereo",
      "multitrack.add.midi",
      "multitrack.add.bus",
      "multitrack.add.bus-mono",
      "fx.plugin-manager",
      "multitrack.duplicate-track",
      "multitrack.delete-track",
      "multitrack.delete-empty",
      "edit.marker.subclip",
      "edit.marker.cd-track",
      "edit.marker.cart",
      "multitrack.vis.hide-selected",
      "multitrack.vis.show-all",
      "multitrack.vis.only-audio",
      "multitrack.vis.only-clips",
      "multitrack.vis.only-selection",
      "multitrack.vis.only-playhead",
      "multitrack.insert-files",
      "multitrack.insert-tracks",
      "multitrack.insert-selected-tracks",
      "view.editor.multitrack",
      "view.zoom-in",
      "view.zoom-out",
      "view.zoom-session",
      "view.time.decimal",
      "view.time.samples",
      "clip.reveal",
      "file.close-unused",
      "edit.select-track-clips",
      "file.new-session",
      "file.open",
      "file.save",
      "file.save-as",
      "file.save-all",
      "file.close",
      "file.close-all",
      "file.import-sesx",
      "multitrack.play-pause",
      "multitrack.stop",
      "transport.go-start",
      "transport.go-end",
      "transport.prev-edge",
      "transport.next-edge",
      "transport.prev-marker",
      "transport.next-marker",
      "view.editor.toggle",
      "multitrack.properties",
      "clip.split",
      "clip.split-all",
      "clip.gain",
      "clip.lock",
      "clip.mute",
      "clip.fade-in",
      "clip.fade-out",
      "clip.trim.start",
      "clip.trim.end",
      "view.time.timecode",
      "file.export.stems",
      "clip.group",
      "help.book",
      "help.support",
      "clip.merge",
      "clip.unique",
      "clip.loop",
      "view.editor.waveform",
      "clip.edit-source",
      "fx.silence",
      "fx.amplify",
      "fx.normalize",
      "fx.spectrum.reduce-12",
      "fx.spectrum.reduce-24",
      "fx.spectrum.remove",
      "fx.spectrum.repair",
      "view.spectrum",
      "view.pitch",
      "multitrack.record",
      "tool.box",
      "tool.lasso",
      "tool.brush",
      "tool.spot",
      "fx.fade-in",
      "fx.fade-out",
      "edit.crop",
      "edit.paste-new",
      "file.new-audio",
      "file.save-template",
      "edit.mix-paste",
      "tool.move",
      "tool.razor",
      "tool.slip",
      "tool.time",
      "file.check-loudness",
      "fx.loudness-match",
      "fx.add.dynamics",
      "fx.add.limiter",
      "fx.add.breath_gate",
      "fx.add.deesser",
      "fx.add.speech_leveler",
      "fx.add.parametric_eq",
      "fx.add.hum_cut",
      "fx.add.gain",
      "clip.music-fit",
      "clip.phase-align",
      "fx.add.noise_reducer",
      "fx.add.noise_print",
      "fx.add.sound_eraser",
      "fx.add.hiss_reducer",
      "fx.add.room_reducer",
      "fx.add.click_repair",
      "fx.add.clip_repair",
      "fx.add.dc_offset",
      "fx.add.band_eq_10",
      "fx.add.band_eq_20",
      "fx.add.band_eq_30",
      "fx.add.curve_eq",
      "fx.add.precision_filter",
      "fx.add.notch",
      "fx.add.pitch_shift",
      "fx.add.center_remover",
      "fx.add.phase_rotate",
      "file.export.sesx",
      "clip.to-track-above",
      "clip.to-track-below",
      "file.export.markers",
      "edit.marker.chapter",
      "clip.stretch",
      "clip.properties",
      "fx.gen.tone",
      "fx.gen.noise",
      "fx.gen.silence",
      "multitrack.insert-silence",
      "multitrack.record-options",
      "edit.marker.ad-slot",
      "edit.marker.social",
      "multitrack.add-tracks",
      "edit.select-type.dialogue",
      "edit.select-type.music",
      "edit.select-type.sfx",
      "edit.select-type.ambience",
      "clip.type.dialogue",
      "clip.type.music",
      "clip.type.sfx",
      "clip.type.ambience",
      "clip.type.none",
      "clip.stretch-reset",
      "view.env.volume",
      "view.env.pan",
      "view.env.fx",
      "clip.ungroup",
      "edit.marker.cue",
      "edit.marker.range",
      "edit.snap.markers",
      "edit.snap.ruler",
      "clip.auto-crossfade",
      "clip.color",
      "file.export.mixdown",
      "file.export.selection",
      "multitrack.mixdown.all",
      "multitrack.mixdown.selection",
      "multitrack.mixdown.clips",
      "multitrack.bounce.selection",
      "multitrack.bounce.clips",
      "edit.cut",
      "edit.copy",
      "edit.paste",
      "edit.ripple.clips",
      "edit.ripple.time-track",
      "edit.ripple.time-all",
      "edit.ripple.gap",
      "edit.select-to-end",
      "edit.select-next-clip",
      "edit.select-clips-track-end",
      "edit.select-clips-session-start",
      "edit.select-clips-session-end",
      "edit.select-view-time",
      "edit.select-all-time",
      "edit.select-time-around-clips",
      "edit.clear-time-selection",
      "edit.snap.on",
      "edit.snap.clips",
      "edit.snap.playhead",
      "clip.trim.selection",
      "view.zoom-selection",
    ]);
    for (const m of MENUS) {
      for (const it of allItems(m.items)) {
        if (
          it.kind !== "item" ||
          built.has(it.id) ||
          it.id.startsWith("window.panel.") ||
          it.id.startsWith("file.recent.") ||
          it.id.startsWith("window.workspace.")
        )
          continue;
        expect(it.pending, it.id).toBeTruthy();
      }
    }
  });

  it("keeps the Window menu with every panel", () => {
    const win = MENUS.find((m) => m.label === "Window");
    expect(win).toBeDefined();
    const ids = win!.items.flatMap((it) => (it.kind === "item" ? [it.id] : []));
    expect(ids).toContain("window.panel.history");
    expect(ids).toContain("window.reset-layout");
  });

  it("lists bindable commands once each, without placeholders", () => {
    const list = commandList();
    const ids = list.map((c) => c.id);
    expect(new Set(ids).size).toBe(ids.length);
    expect(ids).not.toContain("fx.dynamics");
    expect(list.find((c) => c.id === "edit.select-all")?.path).toBe(
      "Edit > Select > Select All",
    );
  });

  it("lists action ids", () => {
    const ids = menuActionIds();
    expect(ids).toContain("edit.undo");
    expect(ids).toContain("window.panel.libraries");
  });
});
