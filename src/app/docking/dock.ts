// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Owns the docking layout: builds the Default layout, turns panels on
 * and off, and saves the layout to the app config folder whenever it
 * changes.
 *
 * Layout changes are not session edits, so they do not go through the
 * undo stack.
 */

import {
  createDockview,
  type AddPanelOptions,
  type AddPanelPositionOptions,
  type DockviewApi,
  type DockviewTheme,
  type IContentRenderer,
  type ITabRenderer,
  type TabPartInitParameters,
  type IHeaderActionsRenderer,
  type IGroupHeaderProps,
  type SerializedDockview,
  type ContextMenuItem,
  type DockviewGroupPanel,
  type IDockviewPanel,
} from "dockview";
import { mount, unmount, type Component } from "svelte";
import { writable, type Readable } from "svelte/store";

import {
  DEFAULT_LAYOUT,
  PANELS,
  panelDef,
  presetById,
  resolveHome,
  WORKSPACE_PRESETS,
  workspaceId,
  type LayoutStep,
  type PanelId,
  type Placement,
} from "./registry";

/** Original theme. Colors and sizes live in styles/dock-theme.css. */
const THEME: DockviewTheme = {
  name: "altaudio",
  className: "dockview-theme-altaudio",
  colorScheme: "dark",
};

/** What a panel body is: a Svelte component and the props to give it. */
export interface PanelContent {
  component: Component<Record<string, unknown>>;
  props: Record<string, unknown>;
}

/** A workspace as menus see it. */
export interface WorkspaceEntry {
  id: string;
  name: string;
  /** Ships with the app: can be reset, not deleted. */
  builtIn: boolean;
}

/** Where named workspaces are kept. The core stores layouts as-is. */
export interface WorkspaceBackend {
  list(): Promise<WorkspaceEntry[]>;
  get(id: string): Promise<unknown | null>;
  put(
    id: string,
    name: string,
    builtIn: boolean,
    layout: unknown,
  ): Promise<void>;
  remove(id: string): Promise<void>;
}

export interface DockOptions {
  /** Returns the body for a panel id. */
  content: (id: PanelId) => PanelContent;
  workspaces: WorkspaceBackend;
  /** The workspace to start in, and where to remember a switch. */
  currentId: () => string;
  setCurrentId: (id: string) => void;
  /** Resolves once `currentId` can be read (preferences loaded). */
  ready?: () => Promise<void>;
  onError: (message: string) => void;
  /** Delay before saving after the last change. */
  saveDelayMs?: number;
}

export interface WorkspaceState {
  current: string;
  items: WorkspaceEntry[];
}

const SIDE_TO_POSITION = {
  left: "left",
  right: "right",
  above: "top",
  below: "bottom",
} as const;

function toPosition(place: Placement): AddPanelPositionOptions {
  if ("edge" in place) return { direction: place.edge };
  return { referencePanel: place.reference, direction: place.direction };
}

/**
 * Group header buttons. Docked groups have none: floating and maximizing
 * live in the panel menu (the dots on the active tab, or a right-click).
 * Floating groups get a Dock button. Rebuilt when the group moves.
 * Icons are original.
 */
function headerActions(
  dockBack: (group: IGroupHeaderProps["group"]) => void,
): IHeaderActionsRenderer {
  const element = document.createElement("div");
  element.className = "aa-group-actions";
  const subs: { dispose(): void }[] = [];

  const button = (label: string, path: string, onClick: () => void) => {
    const b = document.createElement("button");
    b.type = "button";
    b.title = label;
    b.setAttribute("aria-label", label);
    b.innerHTML = `<svg viewBox="0 0 16 16" aria-hidden="true"><path d="${path}"/></svg>`;
    b.addEventListener("click", onClick);
    element.appendChild(b);
    return b;
  };

  const render = (params: IGroupHeaderProps) => {
    element.replaceChildren();
    if (params.group.api.location.type === "floating") {
      button("Dock panel", "M3 3h10v10H3zM3 9h10", () =>
        dockBack(params.group),
      ).classList.add("always");
      return;
    }
  };

  return {
    element,
    init(params: IGroupHeaderProps) {
      render(params);
      subs.push(params.group.api.onDidLocationChange(() => render(params)));
    },
    dispose() {
      for (const s of subs) s.dispose();
    },
  };
}

/**
 * A panel tab: the title, and on the active tab a menu button (three
 * stacked dots) that opens the same menu as right-clicking the tab.
 * There is no close button, so a stray click never closes a panel.
 */
function panelTab(): ITabRenderer {
  const element = document.createElement("div");
  element.className = "aa-tab";
  const title = document.createElement("span");
  title.className = "aa-tab-title";
  const menu = document.createElement("button");
  menu.type = "button";
  menu.className = "aa-tab-menu";
  menu.setAttribute("aria-label", "Panel menu");
  menu.innerHTML =
    '<svg viewBox="0 0 16 16" aria-hidden="true"><circle cx="8" cy="3.5" r="1.4"/><circle cx="8" cy="8" r="1.4"/><circle cx="8" cy="12.5" r="1.4"/></svg>';
  // Keep a press on the button from starting a tab drag.
  menu.addEventListener("pointerdown", (e) => e.stopPropagation());
  menu.addEventListener("click", (e) => {
    e.stopPropagation();
    const r = menu.getBoundingClientRect();
    // Opens the tab's own right-click menu, under the button.
    element.dispatchEvent(
      new MouseEvent("contextmenu", {
        bubbles: true,
        cancelable: true,
        clientX: r.left,
        clientY: r.bottom + 2,
        button: 2,
      }),
    );
  });
  element.append(title, menu);
  const subs: { dispose(): void }[] = [];
  return {
    element,
    init(params: TabPartInitParameters) {
      title.textContent = params.title ?? params.api.title ?? "";
      subs.push(
        params.api.onDidTitleChange((e) => (title.textContent = e.title)),
      );
    },
    dispose() {
      for (const sub of subs) sub.dispose();
    },
  };
}

/**
 * Right-click menu for a panel tab. Closing is only possible from here
 * or the Window menu, never from a button on the tab, so a stray click
 * cannot close a panel.
 */
function tabMenu(
  panel: IDockviewPanel,
  group: DockviewGroupPanel,
  api: DockviewApi,
  dockBack: (group: DockviewGroupPanel) => void,
): ContextMenuItem[] {
  const floating = group.api.location.type === "floating";
  const others = group.panels.filter((p) => p.id !== panel.id);
  const items: ContextMenuItem[] = [
    { label: "Close Panel", action: () => api.removePanel(panel) },
  ];
  if (floating)
    items.push({ label: "Dock Panel Group", action: () => dockBack(group) });
  else
    items.push({
      label: "Undock Panel",
      action: () => api.addFloatingGroup(panel),
    });
  items.push({
    label: "Close Other Panels in Group",
    disabled: others.length === 0,
    action: () => {
      for (const p of others) api.removePanel(p);
    },
  });
  // The group-wide items, then the layout switches.
  items.push("separator", {
    label: "Close Panel Group",
    action: () => {
      for (const p of [...group.panels]) api.removePanel(p);
    },
  });
  if (!floating) {
    items.push({
      label: "Undock Panel Group",
      action: () => api.addFloatingGroup(group),
    });
    const max = group.api.isMaximized();
    items.push({
      label: max ? "Restore Panel Group" : "Maximize Panel Group",
      action: () => (max ? group.api.exitMaximized() : group.api.maximize()),
    });
  }
  const small = document.body.classList.contains("aa-small-tabs");
  items.push(
    "separator",
    { label: "Stacked Panel Group (not available yet)", disabled: true },
    { label: "Solo Panels in Stack (not available yet)", disabled: true },
    {
      label: small ? "Small Tabs (on)" : "Small Tabs",
      action: () => document.body.classList.toggle("aa-small-tabs", !small),
    },
  );
  return items;
}

/**
 * What gets saved. `defaultVersion` records which Default the layout
 * grew from, so a changed Default replaces stale saved layouts.
 */
interface StoredLayout {
  /** The preset version it grew from; 0 for user workspaces. */
  defaultVersion: number;
  dockview: SerializedDockview;
  /** User workspaces: the arrangement as saved, for Reset. */
  base?: SerializedDockview;
}

const hasPanels = (d: unknown) =>
  typeof d === "object" &&
  d !== null &&
  Object.keys((d as { panels?: object }).panels ?? {}).length > 0;

export class DockController {
  private api: DockviewApi | null = null;
  private saveTimer: ReturnType<typeof setTimeout> | null = null;
  private suppressSave = false;
  private readonly openStore = writable<ReadonlySet<PanelId>>(new Set());

  /** Ids of the panels currently open, for menus. */
  readonly open: Readable<ReadonlySet<PanelId>> = this.openStore;

  private current = "";
  private custom: WorkspaceEntry[] = [];
  /** Base arrangement of the current user workspace. */
  private base: SerializedDockview | null = null;
  private readonly wsStore = writable<WorkspaceState>({
    current: "default",
    items: [],
  });

  /** The workspaces and which one is in use, for the Window menu. */
  readonly workspaces: Readable<WorkspaceState> = this.wsStore;

  constructor(private readonly options: DockOptions) {}

  async attach(element: HTMLElement): Promise<void> {
    const { content } = this.options;
    const api = createDockview(element, {
      theme: THEME,
      // Pointer events, not HTML5 drag and drop. On Windows the webview
      // swallows HTML5 drops while native file drop is on, and native
      // file drop is how files come in from Explorer.
      dndStrategy: "pointer",
      createComponent: ({ id, name }): IContentRenderer => {
        const host = document.createElement("div");
        host.className = "aa-panel-body";
        let instance: ReturnType<typeof mount> | null = null;
        return {
          element: host,
          init: () => {
            const body = content((panelDef(name)?.id ?? id) as PanelId);
            instance = mount(body.component, {
              target: host,
              props: body.props,
            });
          },
          dispose: () => {
            if (instance) void unmount(instance);
            instance = null;
          },
        };
      },
      createRightHeaderActionComponent: () =>
        headerActions((group) => this.dockGroup(group)),
      // Every tab uses our own renderer (title plus menu button).
      defaultTabComponent: "panel-tab",
      createTabComponent: () => panelTab(),
      getTabContextMenuItems: ({ panel, group, api }) =>
        tabMenu(panel, group, api, (g) => this.dockGroup(g)),
    });
    this.api = api;

    api.onDidLayoutChange(() => {
      this.syncOpen();
      this.scheduleSave();
    });

    await this.options.ready?.().catch(() => {});
    await this.restore();
  }

  dispose(): void {
    if (this.saveTimer) clearTimeout(this.saveTimer);
    this.api?.dispose();
    this.api = null;
  }

  /** Changes a panel's tab title, for example to name the open file. */
  setTitle(id: PanelId, title: string): void {
    const p = this.api?.getPanel(id);
    if (p && p.title !== title) p.api.setTitle(title);
  }

  isOpen(id: PanelId): boolean {
    return this.api?.getPanel(id) !== undefined;
  }

  /** Turns a panel on (at its home spot) or off. */
  toggle(id: PanelId): void {
    const api = this.api;
    if (!api) return;
    const existing = api.getPanel(id);
    if (existing) {
      api.removePanel(existing);
      return;
    }
    this.add(id, resolveHome(id, this.openIds()));
  }

  /** Brings a panel to the front, opening it first if needed. */
  show(id: PanelId): void {
    const panel = this.api?.getPanel(id);
    if (panel) panel.api.setActive();
    else this.toggle(id);
  }

  /**
   * Returns every panel in a floating group to its home spot in the
   * docked layout.
   */
  dockGroup(group: IGroupHeaderProps["group"]): void {
    const api = this.api;
    if (!api) return;
    const ids = group.panels.map((p) => p.id as PanelId);
    for (const id of ids) {
      const panel = api.getPanel(id);
      if (!panel) continue;
      const docked = new Set(
        api.panels
          .filter((p) => p.group.api.location.type === "grid")
          .map((p) => p.id),
      );
      const home = resolveHome(id, docked);
      if ("edge" in home) {
        panel.api.moveTo({ position: SIDE_TO_POSITION[home.edge] });
        continue;
      }
      const target = api.getPanel(home.reference)?.group;
      if (!target) continue;
      panel.api.moveTo({
        group: target,
        position:
          home.direction === "within"
            ? "center"
            : SIDE_TO_POSITION[home.direction],
      });
    }
  }

  /** Rebuilds the current workspace: a built-in one from its preset, a user one as it was saved. */
  async resetCurrent(): Promise<void> {
    const preset = presetById(this.current);
    if (preset) this.buildSteps(preset.steps);
    else if (this.base && this.api) {
      this.suppressSave = true;
      try {
        this.api.fromJSON(this.base);
      } finally {
        this.suppressSave = false;
      }
      this.syncOpen();
    }
    this.saveNow();
  }

  /** Switches to the Default workspace and rebuilds it. */
  async resetToDefault(): Promise<void> {
    await this.switchTo("default");
    await this.resetCurrent();
  }

  /** Saves the current arrangement and opens another workspace. */
  async switchTo(id: string): Promise<void> {
    if (id === this.current || !this.api) return;
    this.saveNow();
    this.current =
      presetById(id) || this.custom.some((w) => w.id === id) ? id : "default";
    this.options.setCurrentId(this.current);
    await this.restore();
  }

  /** Saves the current arrangement as a new user workspace and switches to it. */
  async saveAs(name: string): Promise<void> {
    const api = this.api;
    const clean = name.trim();
    if (!api || !clean) return;
    const taken = new Set([
      ...WORKSPACE_PRESETS.map((p) => p.id),
      ...this.custom.map((w) => w.id),
    ]);
    const id = workspaceId(clean, taken);
    const dockview = api.toJSON();
    const layout: StoredLayout = {
      defaultVersion: 0,
      dockview,
      base: dockview,
    };
    try {
      await this.options.workspaces.put(id, clean, false, layout);
    } catch (e) {
      this.options.onError(`Workspace could not be saved: ${String(e)}`);
      return;
    }
    this.saveNow();
    this.custom = [...this.custom, { id, name: clean, builtIn: false }].sort(
      (a, b) => a.name.localeCompare(b.name),
    );
    this.current = id;
    this.base = dockview;
    this.options.setCurrentId(id);
    this.publish();
  }

  /** Makes the current arrangement what Reset returns to, for a user workspace. */
  async saveChanges(): Promise<void> {
    if (presetById(this.current) || !this.api) return;
    this.base = this.api.toJSON();
    this.saveNow();
  }

  /** Deletes a user workspace. Built-in ones cannot be deleted. */
  async remove(id: string): Promise<void> {
    if (presetById(id)) return;
    try {
      await this.options.workspaces.remove(id);
    } catch (e) {
      this.options.onError(`Workspace could not be deleted: ${String(e)}`);
      return;
    }
    this.custom = this.custom.filter((w) => w.id !== id);
    if (this.current === id) {
      this.current = "";
      await this.switchTo("default");
    }
    this.publish();
  }

  get currentWorkspace(): WorkspaceEntry {
    const p = presetById(this.current);
    if (p) return { id: p.id, name: p.name, builtIn: true };
    return (
      this.custom.find((w) => w.id === this.current) ?? {
        id: "default",
        name: "Default",
        builtIn: true,
      }
    );
  }

  // Internals

  private publish(): void {
    const items: WorkspaceEntry[] = [
      ...WORKSPACE_PRESETS.map((p) => ({
        id: p.id,
        name: p.name,
        builtIn: true,
      })),
      ...this.custom,
    ];
    this.wsStore.set({ current: this.current, items });
  }

  private async restore(): Promise<void> {
    const api = this.api;
    if (!api) return;
    if (this.custom.length === 0) {
      try {
        this.custom = (await this.options.workspaces.list()).filter(
          (w) => !w.builtIn && !presetById(w.id),
        );
      } catch (e) {
        this.options.onError(`Workspaces could not be listed: ${String(e)}`);
      }
    }
    const wanted = this.current || this.options.currentId();
    this.current =
      presetById(wanted) || this.custom.some((w) => w.id === wanted)
        ? wanted
        : "default";
    this.publish();
    let saved: unknown = null;
    try {
      saved = await this.options.workspaces.get(this.current);
    } catch (e) {
      this.options.onError(
        `Saved workspace could not be read, using its starting layout: ${String(e)}`,
      );
    }
    const stored = saved as StoredLayout | null;
    const preset = presetById(this.current);
    this.base = stored?.base && hasPanels(stored.base) ? stored.base : null;
    // A layout with no panels is never useful at startup; it can only come
    // from a failed load, so it is treated as damaged. A built-in layout
    // saved from an older version of its preset is rebuilt.
    const fresh =
      stored &&
      hasPanels(stored.dockview) &&
      (!preset || stored.defaultVersion === preset.version);
    if (fresh) {
      try {
        this.suppressSave = true;
        api.fromJSON(stored.dockview);
        this.dropRetired();
        this.syncOpen();
        return;
      } catch (e) {
        this.options.onError(
          `Saved workspace was damaged, using its starting layout: ${String(e)}`,
        );
      } finally {
        this.suppressSave = false;
      }
    }
    if (preset) this.buildSteps(preset.steps);
    else if (this.base) {
      this.suppressSave = true;
      try {
        api.fromJSON(this.base);
      } finally {
        this.suppressSave = false;
      }
      this.syncOpen();
    } else this.buildSteps(DEFAULT_LAYOUT);
    // Store the rebuilt layout right away so the file matches what is shown.
    this.saveNow();
  }

  /** Removes panels the app no longer has (a saved layout can still hold them). */
  private dropRetired(): void {
    const api = this.api;
    if (!api) return;
    for (const p of [...api.panels]) if (!panelDef(p.id)) api.removePanel(p);
  }

  private buildSteps(steps: readonly LayoutStep[]): void {
    const api = this.api;
    if (!api) return;
    this.suppressSave = true;
    try {
      api.clear();
      for (const step of steps) {
        this.add(step.id, step.placement, {
          ...(step.inactive ? { inactive: true } : {}),
          ...(step.width !== undefined ? { initialWidth: step.width } : {}),
          ...(step.height !== undefined ? { initialHeight: step.height } : {}),
        });
      }
      // Initial sizes on add are only hints and are not honored when
      // rebuilding inside an existing layout, so set them explicitly.
      for (const step of steps) {
        const group = api.getPanel(step.id)?.group;
        if (!group) continue;
        if (step.width !== undefined) group.api.setSize({ width: step.width });
        if (step.height !== undefined)
          group.api.setSize({ height: step.height });
      }
      const first =
        steps.find((st) => !st.inactive && st.placement === null) ?? steps[0];
      if (first) api.getPanel(first.id)?.api.setActive();
    } finally {
      this.suppressSave = false;
    }
    this.syncOpen();
  }

  private add(
    id: PanelId,
    placement: Placement | null,
    extra: Partial<AddPanelOptions> = {},
  ): void {
    const def = panelDef(id);
    if (!def || !this.api) return;
    const options = {
      id,
      component: id,
      title: def.title,
      ...(placement ? { position: toPosition(placement) } : {}),
      ...extra,
    } as AddPanelOptions;
    this.api.addPanel(options);
  }

  private openIds(): Set<string> {
    return new Set(this.api?.panels.map((p) => p.id) ?? []);
  }

  private syncOpen(): void {
    const known = new Set<string>(PANELS.map((p) => p.id));
    const ids = [...this.openIds()].filter((id): id is PanelId =>
      known.has(id),
    );
    this.openStore.set(new Set(ids));
  }

  private scheduleSave(): void {
    if (this.suppressSave || !this.api) return;
    if (this.saveTimer) clearTimeout(this.saveTimer);
    this.saveTimer = setTimeout(() => {
      this.saveTimer = null;
      this.saveNow();
    }, this.options.saveDelayMs ?? 400);
  }

  /** Saves the current arrangement under the current workspace now. */
  private saveNow(): void {
    if (this.saveTimer) {
      clearTimeout(this.saveTimer);
      this.saveTimer = null;
    }
    const api = this.api;
    // Never save an empty dock. It only happens while tearing down or
    // after a failed load, and would leave the next start blank.
    if (!api || api.panels.length === 0 || !this.current) return;
    const ws = this.currentWorkspace;
    const preset = presetById(ws.id);
    const layout: StoredLayout = {
      defaultVersion: preset?.version ?? 0,
      dockview: api.toJSON(),
      ...(this.base && !preset ? { base: this.base } : {}),
    };
    this.options.workspaces
      .put(ws.id, ws.name, ws.builtIn, layout)
      .catch((e: unknown) => {
        this.options.onError(`Layout could not be saved: ${String(e)}`);
      });
  }
}
