// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * The menu bar as data: File, Edit, Multitrack, Clip, Effects, View,
 * Window, Help. Item contents and grouping are our own. Every item names
 * an action id; items with no registered action show disabled with the
 * reason in `pending`. Shortcuts come from the keymap, not from here.
 */

import { PANELS } from "../docking/registry";

export type MenuItem =
  | {
      kind: "item";
      id: string;
      label: string;
      pending?: string;
      placeholder?: boolean;
    }
  | { kind: "submenu"; label: string; items: MenuItem[] }
  /** A list the app builds when it opens (scanned plug-ins), nested as deep as it needs. */
  | { kind: "popup"; id: string; label: string }
  | { kind: "sep" };

export interface Menu {
  label: string;
  /** Letter that opens the menu with Alt. Must appear in the label. */
  mnemonic: string;
  items: MenuItem[];
}

const item = (
  id: string,
  label: string,
  opts: { pending?: string; placeholder?: boolean } = {},
): MenuItem => ({
  kind: "item",
  id,
  label,
  ...opts,
});
/** A submenu listed in alphabetical order (the effect families, so they read like the Audio FX Add list). */
const sortedSub = (label: string, items: MenuItem[]): MenuItem =>
  sub(
    label,
    [...items].sort((a, b) => (a.kind === "sep" || b.kind === "sep" ? 0 : a.label.localeCompare(b.label, undefined, { numeric: true, sensitivity: "base" }))),
  );

const sub = (label: string, items: MenuItem[]): MenuItem => ({
  kind: "submenu",
  label,
  items,
});
const sep: MenuItem = { kind: "sep" };
const popup = (id: string, label: string): MenuItem => ({ kind: "popup", id, label });

/** The built-in effect families, as the effects catalog names them. */
export const EFFECT_FAMILIES = ["Amplitude", "Delay and Reverb", "Dynamics", "Filter and EQ", "Modulation", "Restoration", "Stereo", "Time and Pitch", "Vocal"] as const;

const P2 = "Arrives with playback (P2).";
const P3 = "Arrives with editing (P3).";
const P5 = "Arrives with effects (P5).";
const NEXT_FX = "Arrives in the next effects batch.";
const P7 = "Arrives with podcast publishing (P7).";
const LATER = "Not built yet.";

/** Open Recent slots. Unused slots are hidden. */
export const RECENT_SLOTS = 10;
/** Menu slots for workspaces: the built-in ones plus the user's own. */
export const WORKSPACE_SLOTS = 20;

export const MENUS: readonly Menu[] = [
  {
    label: "File",
    mnemonic: "F",
    items: [
      item("file.new-session", "New Session..."),
      item("file.new-audio", "New Audio File..."),
      item("file.open", "Open..."),
      item("file.convert-sample-type", "Convert Sample Type...", {
        pending: LATER,
      }),
      sub("Open Recent", [
        ...Array.from({ length: RECENT_SLOTS }, (_, i) =>
          item(`file.recent.${i + 1}`, `Recent ${i + 1}`, {
            placeholder: true,
          }),
        ),
        item("file.recent.none", "No recent sessions", {
          pending: "Sessions you save or open appear here.",
          placeholder: true,
        }),
      ]),
      sep,
      item("file.import-files", "Import Files..."),
      item("file.import-folder", "Import Folder..."),
      item("file.import-sesx", "Import Session (.sesx, .aup3)..."),
      sep,
      item("file.save", "Save"),
      item("file.save-as", "Save As..."),
      item("file.save-template", "Save as Session Template..."),
      item("file.save-all", "Save All"),
      sep,
      item("file.close", "Close"),
      item("file.close-all", "Close All"),
      item("file.close-unused", "Remove Unused Files"),
      item("file.collect-media", "Collect Session Media"),
      item("files.track-details", "Track Details..."),
      sep,
      sub("Export", [
        item("file.export.mixdown", "Session Mixdown..."),
        item("file.export.selection", "Time Selection..."),
        item("file.export.stems", "Track Stems..."),
        sep,
        item("file.export.markers", "Markers and Chapters..."),
        item("file.export.sesx", "Multitrack Session (.sesx)..."),
      ]),
      item("file.check-loudness", "Check Loudness..."),
      item("file.publish-package", "Publish Package..."),
      sep,
      item("file.reveal", "Show in File Manager"),
      sep,
      item("file.exit", "Exit"),
    ],
  },
  {
    label: "Edit",
    mnemonic: "E",
    items: [
      item("edit.undo", "Undo"),
      item("edit.redo", "Redo"),
      sep,
      item("edit.cut", "Cut"),
      item("edit.copy", "Copy"),
      item("edit.paste", "Paste"),
      item("edit.paste-new", "Paste to New File"),
      item("edit.mix-paste", "Mix Paste..."),
      item("edit.delete", "Delete"),
      item("edit.crop", "Crop"),
      sub("Tool", [
        item("tool.move", "Move"),
        item("tool.razor", "Razor"),
        item("tool.slip", "Slip"),
        item("tool.time", "Time Selection"),
        sep,
        item("tool.box", "Box Select (Spectrum)"),
        item("tool.lasso", "Lasso Select (Spectrum)"),
        item("tool.brush", "Brush Select (Spectrum)"),
        item("tool.spot", "Spot Repair (Spectrum)"),
      ]),
      sub("Ripple Delete", [
        item("edit.ripple.clips", "Selected Clips"),
        item("edit.ripple.time-clips", "Time Selection in Selected Clips", { pending: LATER }),
        item("edit.ripple.time-track", "Time Selection in Selected Track"),
        item("edit.ripple.time-all", "Time Selection in All Tracks"),
        item("edit.ripple.gap", "Gap Between Clips"),
      ]),
      sep,
      sub("Select", [
        item("edit.select-all", "Select All"),
        item("edit.deselect", "Deselect All"),
        item("edit.select-track-clips", "All Clips in Track"),
        item("edit.select-next-clip", "Next Clip in Selected Track"),
        item("edit.select-clips-track-end", "Clips to End of Track"),
        item("edit.select-clips-session-start", "Clips to Start of Session"),
        item("edit.select-clips-session-end", "Clips to End of Session"),
        sub("Clips of Audio Type", [
          item("edit.select-type.dialogue", "Dialogue"),
          item("edit.select-type.music", "Music"),
          item("edit.select-type.sfx", "Sound Effects"),
          item("edit.select-type.ambience", "Ambience"),
        ]),
        sep,
        item("edit.select-view-time", "Select Current View Time"),
        item("edit.select-all-time", "Select All Time"),
        item(
          "edit.select-time-around-clips",
          "Select Time Around Selected Clips",
        ),
        item("edit.clear-time-selection", "Clear Time Selection"),
        item("edit.select-to-end", "Playhead to End"),
      ]),
      item("edit.find", "Find..."),
      item("edit.repeat-last", "Repeat Last Command", { pending: LATER }),
      item("edit.repeat-last-dialog", "Repeat Last Command Dialog", {
        pending: LATER,
      }),
      sub("Markers", [
        item("edit.marker.cue", "Add Cue Marker"),
        item("edit.marker.subclip", "Add Subclip Marker"),
        item("edit.marker.cd-track", "Add CD Track Marker"),
        item("edit.marker.cart", "Add Cart Timer Marker"),
        item("edit.marker.range", "Add Range Marker"),
        item("edit.marker.chapter", "Add Chapter Marker"),
        item("edit.marker.ad-slot", "Add Ad Slot..."),
        item("edit.marker.social", "Add Social Clip"),
      ]),
      sub("Snapping", [
        item("edit.snap.on", "Snap On"),
        sep,
        item("edit.snap.clips", "To Clips"),
        item("edit.snap.playhead", "To Playhead"),
        item("edit.snap.markers", "To Markers"),
        item("edit.snap.ruler", "To Ruler Ticks"),
      ]),
      sep,
      item("edit.keyboard-shortcuts", "Keyboard Shortcuts..."),
      item("edit.settings", "Settings..."),
    ],
  },
  {
    label: "Multitrack",
    mnemonic: "M",
    items: [
      sub("Add Track", [
        item("multitrack.add.mono", "Mono Track"),
        item("multitrack.add.stereo", "Stereo Track"),
        item("multitrack.add.midi", "MIDI Track"),
        item("multitrack.add-tracks", "Add Tracks..."),
        item("multitrack.add.guest", "Guest Track", { pending: P5 }),
        item("multitrack.add.bus", "Bus Track"),
        item("multitrack.add.bus-mono", "Mono Bus Track"),
        item("multitrack.add.bus-3d", "3D Bus Track"),
      ]),
      item("multitrack.duplicate-track", "Duplicate Track"),
      item("multitrack.delete-track", "Delete Track"),
      item("multitrack.delete-empty", "Delete Empty Tracks"),
      sub("Track Visibility", [
        item("multitrack.vis.hide-selected", "Hide Selected Track"),
        item("multitrack.vis.show-all", "Show All Tracks"),
        sep,
        item("multitrack.vis.only-audio", "Show Only Audio Tracks"),
        item("multitrack.vis.only-bus", "Show Only Bus Tracks", {
          pending: "Not built yet: bus tracks always show below the audio tracks.",
        }),
        item("multitrack.vis.only-clips", "Show Only Tracks with Clips"),
        item(
          "multitrack.vis.only-selection",
          "Show Only Tracks with Clips within Time Selection",
        ),
        item(
          "multitrack.vis.only-playhead",
          "Show Only Tracks with Clips at Playhead",
        ),
        sep,
        item("multitrack.vis.load", "Load Visibility Preset", {
          pending: LATER,
        }),
        item("multitrack.vis.save", "Save Visibility Preset", {
          pending: LATER,
        }),
      ]),
      sep,
      item("multitrack.insert-files", "Insert Files at Playhead..."),
      item("multitrack.insert-tracks", "Insert Files as Tracks..."),
      item(
        "multitrack.insert-selected-tracks",
        "Insert Selected Files as Tracks",
      ),
      item("multitrack.insert-silence", "Insert Silence at Playhead..."),
      sep,
      item("multitrack.play-pause", "Play / Stop"),
      item("multitrack.pause", "Pause"),
      item("multitrack.stop", "Stop"),
      sub("Go To", [
        item("transport.go-start", "Start"),
        item("transport.go-end", "End"),
        item("transport.prev-edge", "Previous Marker, Clip or Selection Edge"),
        item("transport.next-edge", "Next Marker, Clip or Selection Edge"),
        item("transport.prev-marker", "Previous Marker"),
        item("transport.next-marker", "Next Marker"),
      ]),
      item("multitrack.return-on-stop", "Return to Start on Stop"),
      item("multitrack.meter-input", "Meter Input Signal", { pending: LATER }),
      item("multitrack.record", "Record"),
      item("multitrack.record-options", "Recording Options..."),
      sep,
      item("multitrack.speaker-balance", "Speaker Balance"),
      item("multitrack.space", "3D Space..."),
      item("multitrack.space-toggle", "3D Space Session"),
      sub("Host Profile", [
        item("multitrack.profile.close-mic", "Close Mic"),
        item("multitrack.profile.standard-mic", "Standard Mic"),
        item("multitrack.profile.remote-guest", "Remote Guest (Guest Track)"),
        sep,
        item("multitrack.profile.clear", "Clear Profile"),
      ]),
      item("multitrack.transcribe", "Transcribe Session...", {
        pending: LATER,
      }),
      item("multitrack.remove-fillers", "Remove Filler Words...", {
        pending: P7,
      }),
      sep,
      sub("Mix Down to New File", [
        item("multitrack.mixdown.all", "Entire Session"),
        item("multitrack.mixdown.selection", "Time Selection"),
        item("multitrack.mixdown.clips", "Selected Clips"),
      ]),
      sub("Bounce to New Track", [
        item("multitrack.bounce.track", "Selected Track"),
        item("multitrack.bounce.selection", "Time Selection"),
        item("multitrack.bounce.clips-selection", "Selected Clips in Time Selection"),
        item("multitrack.bounce.clips", "Selected Clips Only"),
      ]),
      sep,
      item("multitrack.properties", "Session Properties..."),
    ],
  },
  {
    label: "Clip",
    mnemonic: "C",
    items: [
      item("clip.split", "Split at Playhead"),
      sub("Nudge", [
        item("clip.nudge-left", "Nudge Left"),
        item("clip.nudge-right", "Nudge Right"),
      ]),
      item("clip.split-all", "Split All Clips at Playhead"),
      item("clip.merge", "Merge Clips"),
      item("clip.unique", "Make Unique Copy"),
      item("clip.split-channels", "Split Channels to Tracks"),
      item("clip.edit-source", "Open in Waveform Editor"),
      sep,
      item("clip.gain", "Clip Gain..."),
      item("clip.color", "Clip Color..."),
      item("clip.lock", "Lock Position"),
      item("clip.mute", "Mute Clip"),
      item("clip.loop", "Loop Clip"),
      sub("Audio Type", [
        item("clip.type.dialogue", "Dialogue"),
        item("clip.type.music", "Music"),
        item("clip.type.sfx", "Sound Effects"),
        item("clip.type.ambience", "Ambience"),
        sep,
        item("clip.type.none", "None"),
      ]),
      sep,
      sub("Fades", [
        item("clip.fade-in", "Fade In"),
        item("clip.fade-out", "Fade Out"),
        sep,
        item("clip.auto-crossfade", "Automatic Crossfades"),
      ]),
      sub("Trim", [
        item("clip.trim.selection", "To Time Selection"),
        item("clip.trim.start", "Start to Playhead"),
        item("clip.trim.end", "End to Playhead"),
      ]),
      sub("Group", [
        item("clip.group", "Group Clips"),
        item("clip.group-suspend", "Suspend Groups", { pending: LATER }),
        item("clip.group-remove-focus", "Remove Focus Clip from Group", { pending: LATER }),
        item("clip.ungroup", "Ungroup Selected Clips"),
      ]),
      item("clip.stretch", "Time Stretch..."),
      item("clip.properties", "Stretch and Pitch..."),
      item("clip.music-fit", "Fit Music to Length..."),
      item("clip.stretch-reset", "Remove Stretch"),
      item("clip.phase-align", "Phase Align"),
      item("clip.separate-quick", "Separate Stems (quick model)"),
      sep,
      item("clip.to-track-above", "Move to Track Above"),
      item("clip.to-track-below", "Move to Track Below"),
      sep,
      item("clip.remove-music", "Remove Music...", { pending: P7 }),
      item("clip.reveal", "Show in Files Panel"),
    ],
  },
  {
    label: "Effects",
    mnemonic: "T",
    items: [
      item("window.panel.voice-chain", "Audio FX"),
      item("window.panel.separation", "Audio Separation"),
      item("fx.hide-windows", "Hide All Effect Windows"),
      item("fx.capture-noise-print", "Capture Noise Print"),
      sep,
      item("fx.invert", "Invert"),
      item("fx.reverse", "Reverse"),
      item("fx.silence", "Silence"),
      sub("Spectrum", [
        item("fx.spectrum.reduce-12", "Reduce Selected Area 12 dB"),
        item("fx.spectrum.reduce-24", "Reduce Selected Area 24 dB"),
        item("fx.spectrum.remove", "Remove Selected Area"),
        item("fx.spectrum.repair", "Repair Selected Area"),
      ]),
      sub("Process", [
        item("fx.amplify", "Amplify..."),
        item("fx.normalize", "Normalize..."),
        item("fx.fade-in", "Fade In"),
        item("fx.fade-out", "Fade Out"),
        item("clip.stretch", "Time Stretch..."),
        item("clip.music-fit", "Fit Music to Length..."),
        item("fx.pitch-tune", "Pitch Tune (automatic correction)", {
          pending: NEXT_FX,
        }),
        item("fx.pitch-curve", "Pitch Curve (pitch over time)", {
          pending: NEXT_FX,
        }),
      ]),
      sub("Generate", [
        item("fx.gen.tone", "Tone..."),
        item("fx.gen.noise", "Noise..."),
        item("fx.gen.silence", "Silence..."),
      ]),
      item("fx.loudness-match", "Loudness Match..."),
      sep,
      // The effect families, filled from the same list as the Audio FX
      // rack's Add effect menu, so both read the same.
      ...EFFECT_FAMILIES.map((f) => popup(`fx.family.${f}`, f)),
      sep,
      item("fx.vst", "VST", {
        pending: "Not supported: Steinberg no longer licenses VST2. Most plug-ins also come as VST3.",
      }),
      popup("fx.vst3", "VST3"),
      item("fx.plugin-manager", "Plug-in Manager..."),
      item("fx.firewall", "Stop Network Prompts From Plug-ins..."),
    ],
  },
  {
    label: "View",
    mnemonic: "V",
    items: [
      item("view.editor.toggle", "Switch Between Editors"),
      item("view.editor.multitrack", "Multitrack Editor"),
      item("view.editor.waveform", "Waveform Editor"),
      item("view.spectrum", "Show Spectrum"),
      item("view.pitch", "Show Pitch"),
      sep,
      item("view.zoom-in", "Zoom In"),
      item("view.zoom-out", "Zoom Out"),
      item("view.zoom-session", "Zoom to Session"),
      item("view.zoom-selection", "Zoom to Selection"),
      item("view.zoom-out-full", "Zoom Out Full"),
      item("view.zoom-all-axes", "Zoom Reset (All Axes)", { pending: LATER }),
      item("view.zoom-in-vertical", "Zoom In Vertically"),
      item("view.zoom-out-vertical", "Zoom Out Vertically"),
      item("view.hud", "Show HUD", { pending: LATER }),
      item("wave.channel-left", "Activate Left Channel"),
      item("wave.channel-right", "Activate Right Channel"),
      sub("Time Format", [
        item("view.time.decimal", "Minutes and Seconds"),
        item("view.time.seconds", "Seconds"),
        item("view.time.bars", "Bars and Beats"),
        item("view.time.samples", "Samples"),
        item("view.time.timecode", "Timecode (30 fps)"),
      ]),
      sub("Envelopes", [
        item("view.env.volume", "Clip Volume"),
        item("view.env.pan", "Clip Pan"),
        item("view.env.fx", "Clip Effect Settings"),
      ]),
      sep,
      item("view.mode.podcast", "Podcast Mode", {
        pending: "Its panels arrive with P7.",
      }),
      item("view.mode.voiceover", "Voiceover Mode", { pending: LATER }),
    ],
  },
  {
    label: "Window",
    mnemonic: "W",
    items: [
      sub("Workspace", [
        ...Array.from({ length: WORKSPACE_SLOTS }, (_, i) =>
          item(`window.workspace.${i + 1}`, `Workspace ${i + 1}`, {
            placeholder: true,
          }),
        ),
        sep,
        item("window.workspace.reset", "Reset Workspace"),
        item("window.workspace.save-changes", "Save Changes to Workspace"),
        item("window.workspace.save-as", "Save as New Workspace..."),
        item("window.workspace.delete", "Delete Workspace"),
      ]),
      sep,
      ...PANELS.map((p) => item(`window.panel.${p.id}`, p.title)),
      sep,
      item("window.reset-layout", "Reset to Default Layout"),
      item("window.minimize", "Minimize"),
    ],
  },
  {
    label: "Help",
    mnemonic: "H",
    items: [
      item("help.guide", "User Guide", { pending: LATER }),
      item("help.shortcuts", "Keyboard Shortcuts"),
      sep,
      item("help.report", "Report a Problem..."),
      item("help.update", "Check for Updates..."),
      item("help.github", "Audiform Source Code on GitHub"),
      item("help.discord", "Join the Community on Discord"),
      sep,
      sub("Based on Altered Audio Editor", [
        item("help.original", "Altered Audio Editor on GitHub"),
        item("help.website", "The GCRN Website"),
        item("help.plugins", "GCRN Plugins", {
          pending: "The plugins page on thegcrn.com is not live yet.",
        }),
        item("help.book", "Book: Altered Audio, Podcasting Demystified (Steve Phillips)"),
        item("help.support", "Support the Original Author..."),
      ]),
      sep,
      item("help.about", "About Audiform"),
    ],
  },
];

/** Every action id the menus refer to. */
export function menuActionIds(menus: readonly Menu[] = MENUS): string[] {
  const ids: string[] = [];
  const walk = (items: readonly MenuItem[]) => {
    for (const it of items) {
      if (it.kind === "item") ids.push(it.id);
      else if (it.kind === "submenu") walk(it.items);
    }
  };
  for (const m of menus) walk(m.items);
  return ids;
}

/** A command that can take a shortcut, with where it lives in the menus. */
export interface CommandEntry {
  id: string;
  /** Menu path, for example "Edit > Select > Select All". */
  path: string;
}

/** Every real command in the menus, once each, in menu order. */
export function commandList(menus: readonly Menu[] = MENUS): CommandEntry[] {
  const out: CommandEntry[] = [];
  const seen = new Set<string>();
  const walk = (items: readonly MenuItem[], trail: string[]) => {
    for (const it of items) {
      if (it.kind === "submenu") walk(it.items, [...trail, it.label]);
      else if (it.kind === "item" && !it.placeholder && !seen.has(it.id)) {
        seen.add(it.id);
        out.push({
          id: it.id,
          path: [...trail, it.label.replace(/\.\.\.$/, "")].join(" > "),
        });
      }
    }
  };
  for (const m of menus) walk(m.items, [m.label]);
  return out;
}
