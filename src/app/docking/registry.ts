// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Every dockable panel, where it lives in the Default layout, and where
 * it returns to when the user turns it back on.
 *
 * Pure data and logic, no DOM, so it can be unit tested.
 */

export type PanelId =
  | "files"
  | "voice-chain"
  | "batch-convert"
  | "editor"
  | "mixer"
  | "levels"
  | "time-info"
  | "history"
  | "drive-manager"
  | "libraries"
  | "markers"
  | "playlist"
  | "loudness-match"
  | "properties"
  | "phase-wheel"
  | "visualizer"
  | "space"
  | "separation"
  | "track-details";

export type Side = "left" | "right" | "above" | "below";

/** Where to put a panel: next to a panel, inside its tab group, or at an edge of the window. */
export type Placement =
  { reference: PanelId; direction: Side | "within" } | { edge: Side };

export interface PanelDef {
  id: PanelId;
  title: string;
  /** Shown in the Default layout. */
  inDefault: boolean;
  /** Where the panel goes when turned on, first match wins. An edge always matches. */
  home: Placement[];
  /** Short note shown while the panel has no content yet. Null when built. */
  pending: string | null;
}

const LEFT_TABS: Placement[] = [
  { reference: "voice-chain", direction: "within" },
  { reference: "batch-convert", direction: "within" },
  { reference: "files", direction: "below" },
  { reference: "editor", direction: "left" },
  { edge: "left" },
];

export const PANELS: readonly PanelDef[] = [
  {
    id: "files",
    title: "Files",
    inDefault: true,
    home: [
      { reference: "voice-chain", direction: "above" },
      { reference: "editor", direction: "left" },
      { edge: "left" },
    ],
    pending: null,
  },
  {
    id: "voice-chain",
    title: "Audio FX",
    inDefault: true,
    home: LEFT_TABS,
    pending: null,
  },
  {
    id: "batch-convert",
    title: "Batch Convert",
    inDefault: true,
    home: LEFT_TABS,
    pending: null,
  },
  {
    id: "editor",
    title: "Editor",
    inDefault: true,
    home: [{ reference: "mixer", direction: "within" }, { edge: "right" }],
    pending: "The multitrack view arrives in P1 task 10.",
  },
  {
    id: "mixer",
    title: "Mixer",
    inDefault: true,
    home: [{ reference: "editor", direction: "within" }, { edge: "right" }],
    pending: null,
  },
  {
    id: "levels",
    title: "Levels",
    inDefault: true,
    home: [{ edge: "below" }],
    pending: null,
  },
  {
    id: "libraries",
    title: "Libraries",
    inDefault: true,
    home: [{ reference: "editor", direction: "right" }, { edge: "right" }],
    pending: null,
  },
  {
    id: "time-info",
    title: "Time Info",
    inDefault: true,
    home: [{ reference: "levels", direction: "right" }, { edge: "below" }],
    pending: null,
  },
  {
    id: "history",
    title: "History",
    inDefault: false,
    home: LEFT_TABS,
    pending: null,
  },
  {
    id: "drive-manager",
    title: "Drive Manager",
    inDefault: false,
    home: LEFT_TABS,
    pending: null,
  },
  {
    id: "markers",
    title: "Markers",
    inDefault: false,
    home: LEFT_TABS,
    pending: null,
  },
  {
    id: "playlist",
    title: "Playlist",
    inDefault: false,
    home: LEFT_TABS,
    pending: null,
  },
  {
    id: "loudness-match",
    title: "Loudness Match",
    inDefault: false,
    home: LEFT_TABS,
    pending: "Loudness Match is in the Effects menu.",
  },
  {
    id: "properties",
    title: "Properties",
    inDefault: false,
    home: LEFT_TABS,
    pending: null,
  },
  {
    id: "phase-wheel",
    title: "Phase Wheel",
    inDefault: false,
    home: [{ reference: "libraries", direction: "within" }, { reference: "levels", direction: "right" }, { edge: "right" }],
    pending: null,
  },
  {
    id: "visualizer",
    title: "Visualizer",
    inDefault: false,
    // Behind the Phase Wheel, or History.
    home: [{ reference: "phase-wheel", direction: "within" }, { reference: "history", direction: "within" }, { reference: "libraries", direction: "within" }, { reference: "levels", direction: "right" }, { edge: "right" }],
    pending: null,
  },
  {
    id: "space",
    title: "3D Space",
    inDefault: false,
    // Beside the Editor and Mixer: the room needs room.
    home: [{ reference: "editor", direction: "within" }, { reference: "mixer", direction: "within" }, { edge: "right" }],
    pending: null,
  },
  {
    id: "separation",
    title: "Audio Separation",
    inDefault: false,
    // Beside 3D Space when it is open.
    home: [{ reference: "space", direction: "within" }, { reference: "phase-wheel", direction: "within" }, { reference: "editor", direction: "right" }, { edge: "right" }],
    pending: null,
  },
  {
    id: "track-details",
    title: "Track Details",
    inDefault: false,
    // With Properties or History, else with Audio FX.
    home: [{ reference: "properties", direction: "within" }, { reference: "history", direction: "within" }, ...LEFT_TABS],
    pending: null,
  },
];

const BY_ID = new Map(PANELS.map((p) => [p.id, p]));

export function panelDef(id: string): PanelDef | undefined {
  return BY_ID.get(id as PanelId);
}

/** Picks the first home placement that is possible with the panels currently open. */
export function resolveHome(id: PanelId, open: ReadonlySet<string>): Placement {
  const def = BY_ID.get(id);
  for (const place of def?.home ?? []) {
    if ("edge" in place) return place;
    if (place.reference !== id && open.has(place.reference)) return place;
  }
  return { edge: "right" };
}

/** Default layout sizes in pixels. */
export const DEFAULT_SIZES = {
  leftWidth: 270,
  rightWidth: 270,
  levelsHeight: 128,
  timeInfoWidth: 270,
} as const;

/** One step of building the Default layout. */
export interface LayoutStep {
  id: PanelId;
  placement: Placement | null;
  inactive?: boolean;
  width?: number;
  height?: number;
}

/**
 * The Default layout as an ordered list of add operations.
 * Order matters: the center and right columns and the Levels row under
 * them are built first, then the left column is added at the window's
 * left edge so it runs the full height. Levels sits
 * only under the center and right columns, with Time Info at its right
 * end under the right column.
 */
export const DEFAULT_LAYOUT: readonly LayoutStep[] = [
  { id: "editor", placement: null },
  {
    id: "mixer",
    placement: { reference: "editor", direction: "within" },
    inactive: true,
  },
  {
    id: "libraries",
    placement: { reference: "editor", direction: "right" },
    width: DEFAULT_SIZES.rightWidth,
  },
  {
    id: "levels",
    placement: { edge: "below" },
    height: DEFAULT_SIZES.levelsHeight,
  },
  {
    id: "time-info",
    placement: { reference: "levels", direction: "right" },
    width: DEFAULT_SIZES.timeInfoWidth,
  },
  { id: "files", placement: { edge: "left" }, width: DEFAULT_SIZES.leftWidth },
  { id: "voice-chain", placement: { reference: "files", direction: "below" } },
  {
    id: "batch-convert",
    placement: { reference: "voice-chain", direction: "within" },
    inactive: true,
  },
];

/**
 * Bump when DEFAULT_LAYOUT changes. A saved layout from an older
 * Default is replaced by the new Default on next launch.
 */
export const DEFAULT_LAYOUT_VERSION = 7;

/** A workspace that ships with the app. */
export interface WorkspacePreset {
  id: string;
  name: string;
  /** One line for menus and tooltips. */
  description: string;
  steps: readonly LayoutStep[];
  /** Bump when `steps` changes, so saved arrangements of it are rebuilt. */
  version: number;
}

const S = DEFAULT_SIZES;

/**
 * Built-in workspaces. Each is built like the Default: center, right
 * and bottom first, then the left column at the window edge so it runs
 * the full height. Names describe the job, not another product.
 */
export const WORKSPACE_PRESETS: readonly WorkspacePreset[] = [
  {
    id: "default",
    name: "Default",
    description:
      "Everything for a podcast edit: files, effects, editor, libraries and levels.",
    steps: DEFAULT_LAYOUT,
    version: DEFAULT_LAYOUT_VERSION,
  },
  {
    id: "focused-edit",
    name: "Focused Edit",
    description:
      "As much room as possible for the timeline. Files on the left, levels below.",
    version: 1,
    steps: [
      { id: "editor", placement: null },
      {
        id: "mixer",
        placement: { reference: "editor", direction: "within" },
        inactive: true,
      },
      { id: "levels", placement: { edge: "below" }, height: S.levelsHeight },
      {
        id: "time-info",
        placement: { reference: "levels", direction: "right" },
        width: S.timeInfoWidth,
      },
      { id: "files", placement: { edge: "left" }, width: 220 },
    ],
  },
  {
    id: "mixing",
    name: "Mixing",
    description:
      "The Mixer up front with tall meters, files and markers on the left.",
    version: 2,
    steps: [
      // Editor stays the first tab; the Mixer is added after it and comes to the front.
      { id: "editor", placement: null },
      { id: "mixer", placement: { reference: "editor", direction: "within" } },
      {
        id: "libraries",
        placement: { reference: "editor", direction: "right" },
        width: S.rightWidth,
      },
      { id: "levels", placement: { edge: "below" }, height: 170 },
      {
        id: "time-info",
        placement: { reference: "levels", direction: "right" },
        width: S.timeInfoWidth,
      },
      { id: "files", placement: { edge: "left" }, width: S.leftWidth },
      { id: "markers", placement: { reference: "files", direction: "below" } },
    ],
  },  {
    id: "music",
    name: "Music",
    description:
      "Tracks in the center; effects, properties and history on the left; Phase Wheel, Visualizer, 3D Space and Audio Separation on the right.",
    version: 4,
    steps: [
      { id: "editor", placement: null },
      { id: "mixer", placement: { reference: "editor", direction: "within" }, inactive: true },
      {
        id: "phase-wheel",
        placement: { reference: "editor", direction: "right" },
        width: S.rightWidth,
      },
      { id: "space", placement: { reference: "phase-wheel", direction: "below" } },
      { id: "separation", placement: { reference: "space", direction: "within" }, inactive: true },
      { id: "visualizer", placement: { reference: "phase-wheel", direction: "within" }, inactive: true },
      { id: "levels", placement: { edge: "below" }, height: S.levelsHeight },
      {
        id: "time-info",
        placement: { reference: "levels", direction: "right" },
        width: S.timeInfoWidth,
      },
      { id: "files", placement: { edge: "left" }, width: S.leftWidth },
      { id: "voice-chain", placement: { reference: "files", direction: "below" } },
      { id: "properties", placement: { reference: "voice-chain", direction: "within" }, inactive: true },
      { id: "batch-convert", placement: { reference: "voice-chain", direction: "within" }, inactive: true },
      { id: "history", placement: { reference: "voice-chain", direction: "below" } },
    ],
  },

  {
    id: "voiceover",
    name: "Voiceover",
    description:
      "Takes and markers on the left, the editor in the middle, libraries on the right.",
    version: 1,
    steps: [
      { id: "editor", placement: null },
      {
        id: "mixer",
        placement: { reference: "editor", direction: "within" },
        inactive: true,
      },
      {
        id: "libraries",
        placement: { reference: "editor", direction: "right" },
        width: S.rightWidth,
      },
      { id: "levels", placement: { edge: "below" }, height: S.levelsHeight },
      {
        id: "time-info",
        placement: { reference: "levels", direction: "right" },
        width: S.timeInfoWidth,
      },
      { id: "files", placement: { edge: "left" }, width: S.leftWidth },
      { id: "markers", placement: { reference: "files", direction: "below" } },
      {
        id: "voice-chain",
        placement: { reference: "markers", direction: "within" },
        inactive: true,
      },
    ],
  },
  {
    id: "review",
    name: "Review and Export",
    description:
      "For the final listen: markers and history on the left, wide meters below.",
    version: 1,
    steps: [
      { id: "editor", placement: null },
      {
        id: "mixer",
        placement: { reference: "editor", direction: "within" },
        inactive: true,
      },
      { id: "levels", placement: { edge: "below" }, height: 150 },
      {
        id: "time-info",
        placement: { reference: "levels", direction: "right" },
        width: S.timeInfoWidth,
      },
      { id: "markers", placement: { edge: "left" }, width: S.leftWidth },
      {
        id: "history",
        placement: { reference: "markers", direction: "below" },
      },
    ],
  },
];

export function presetById(id: string): WorkspacePreset | undefined {
  return WORKSPACE_PRESETS.find((p) => p.id === id);
}

/** A file-safe id for a new workspace name that does not clash with `taken`. */
export function workspaceId(name: string, taken: ReadonlySet<string>): string {
  const base =
    name
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-+|-+$/g, "")
      .slice(0, 48) || "workspace";
  let id = base;
  for (let n = 2; taken.has(id); n++) id = `${base}-${n}`;
  return id;
}
