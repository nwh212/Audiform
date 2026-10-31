// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * The View, Window and Help menus and the transport jumps: zoom, time
 * display, envelopes, workspaces and help links.
 */

import { fit } from "../panels/editor/timeline";
import { editor } from "../stores/editor";
import { session, transport } from "../stores/open-session";
import { minimizeWindow, openLink } from "./core";
import { LINKS } from "./links";
import { WORKSPACE_SLOTS } from "./menus/model";
import { zoomBy, contentEnd, jumpEdge, sessionView } from "./navigation";
import type { ActionRegistry } from "./actions";
import type { DockController, WorkspaceState } from "./docking/dock";
import type { ChoiceButton } from "./ChoiceDialog.svelte";
import { PANELS, type PanelId } from "./docking/registry";

/** What the View, Window and Help menus needs from the rest of the app. */
export interface ViewToolsDeps {
  actions: ActionRegistry;
  dock: DockController;
  inWave: () => boolean;
  showNotice: (text: string) => void;
  promptText: (title: string, label: string, value: string) => Promise<string | null>;
  ask: (title: string, message: string, buttons: ChoiceButton[]) => Promise<string>;
  timeSel: () => { start: number; end: number } | null;
  shortcutsOpen: () => boolean;
  set_shortcutsOpen: (v: boolean) => void;
  aboutOpen: () => boolean;
  set_aboutOpen: (v: boolean) => void;
  /** Help > Report a Problem: opens its window. */
  openReport: () => void;
  wsState: () => WorkspaceState;
  set_wsState: (v: WorkspaceState) => void;
  openPanels: () => ReadonlySet<PanelId>;
  set_openPanels: (v: ReadonlySet<PanelId>) => void;
}

export function createViewTools(deps: ViewToolsDeps) {
  const { actions, dock, inWave, showNotice, promptText, ask, timeSel, shortcutsOpen, aboutOpen, wsState, openPanels, set_shortcutsOpen, set_aboutOpen, set_wsState, set_openPanels, openReport } = deps;


  /** Registers the menu actions. */
  function register(): (() => void)[] {
    const offs = [
      actions.register({ id: "window.reset-layout", run: () => dock.resetToDefault() }),
      actions.register({ id: "window.minimize", run: () => minimizeWindow() }),
      actions.register({ id: "help.shortcuts", run: () => void (set_shortcutsOpen(true)) }),
      actions.register({ id: "help.website", run: () => openLink(LINKS.website) }),
      actions.register({ id: "help.book", run: () => openLink(LINKS.book) }),
      actions.register({ id: "help.original", run: () => openLink(LINKS.original) }),
      actions.register({ id: "help.update", run: () => openLink(LINKS.releases) }),
      actions.register({ id: "help.github", run: () => openLink(LINKS.github) }),
      actions.register({ id: "help.report", run: () => openReport() }),
      actions.register({ id: "help.about", run: () => void (set_aboutOpen(true)) }),
      actions.register({ id: "view.zoom-in", run: () => zoomBy(0.5), enabled: () => session.current !== null }),
      actions.register({ id: "view.zoom-out", run: () => zoomBy(2), enabled: () => session.current !== null }),
      actions.register({ id: "view.editor.toggle", run: async () => void (await actions.run(inWave() ? "view.editor.multitrack" : "view.editor.waveform")) }),
      actions.register({ id: "transport.go-start", run: () => transport.seek(0), enabled: () => session.current !== null }),
      actions.register({ id: "transport.go-end", run: () => transport.seek(contentEnd()), enabled: () => session.current !== null }),
      actions.register({ id: "transport.prev-edge", run: () => jumpEdge(-1, false), enabled: () => session.current !== null }),
      actions.register({ id: "transport.next-edge", run: () => jumpEdge(1, false), enabled: () => session.current !== null }),
      actions.register({ id: "transport.prev-marker", run: () => jumpEdge(-1, true), enabled: () => session.current !== null }),
      actions.register({ id: "transport.next-marker", run: () => jumpEdge(1, true), enabled: () => session.current !== null }),
      actions.register({
        id: "view.zoom-session",
        run: () => {
          const v = sessionView();
          if (!v) return;
          const f = fit(contentEnd(), editor.current().width);
          session.setView({ ...v, samples_per_pixel: f.spp, scroll: 0 });
        },
        enabled: () => contentEnd() > 0,
        reason: () => "The session has no clips yet.",
      }),
      actions.register({
        id: "view.time.decimal",
        run: () => editor.setTimeFormat("time"),
        checked: () => editor.current().timeFormat === "time",
      }),
      actions.register({
        id: "view.time.seconds",
        run: () => editor.setTimeFormat("seconds"),
        checked: () => editor.current().timeFormat === "seconds",
      }),
      actions.register({
        id: "view.time.bars",
        run: () => editor.setTimeFormat("bars"),
        checked: () => editor.current().timeFormat === "bars",
      }),
      actions.register({
        id: "view.time.samples",
        run: () => editor.setTimeFormat("samples"),
        checked: () => editor.current().timeFormat === "samples",
      }),
      actions.register({
        id: "view.env.volume",
        run: () => editor.setShowVolume(!editor.current().showVolume),
        checked: () => editor.current().showVolume,
      }),
      actions.register({
        id: "view.env.fx",
        run: () => editor.setShowFx(!editor.current().showFx),
        checked: () => editor.current().showFx,
      }),
      actions.register({
        id: "view.env.pan",
        run: () => editor.setShowPan(!editor.current().showPan),
        checked: () => editor.current().showPan,
      }),
      actions.register({
        id: "view.time.timecode",
        run: () => editor.setTimeFormat("timecode"),
        checked: () => editor.current().timeFormat === "timecode",
      }),
      ...Array.from({ length: WORKSPACE_SLOTS }, (_, i) =>
        actions.register({
          id: `window.workspace.${i + 1}`,
          run: () => {
            const w = wsState().items[i];
            if (w) return dock.switchTo(w.id);
          },
          label: () => wsState().items[i]?.name ?? null,
          visible: () => i < wsState().items.length,
          checked: () => wsState().items[i]?.id === wsState().current,
        }),
      ),
      actions.register({ id: "window.workspace.reset", run: () => dock.resetCurrent() }),
      actions.register({
        id: "window.workspace.save-changes",
        run: async () => {
          await dock.saveChanges();
          showNotice(`Saved "${dock.currentWorkspace.name}". Reset Workspace now returns to this arrangement.`);
        },
        enabled: () => !dock.currentWorkspace.builtIn,
        reason: () => "Built-in workspaces keep their own starting layout. Use Save as New Workspace.",
      }),
      actions.register({
        id: "window.workspace.save-as",
        run: async () => {
          const name = await promptText("Save as New Workspace", "Workspace name", `${dock.currentWorkspace.name} copy`);
          if (name) await dock.saveAs(name);
        },
      }),
      actions.register({
        id: "window.workspace.delete",
        run: async () => {
          const w = dock.currentWorkspace;
          const ok = await ask("Delete workspace", `Delete the workspace "${w.name}"? This cannot be undone.`, [
            { label: "Delete", value: "yes", primary: true },
            { label: "Cancel", value: "no" },
          ]);
          if (ok === "yes") await dock.remove(w.id);
        },
        enabled: () => !dock.currentWorkspace.builtIn,
        reason: () => "Built-in workspaces cannot be deleted.",
      }),
      actions.register({
        id: "view.zoom-selection",
        run: () => {
          const v = sessionView();
          const r = timeSel();
          if (!v || !r) return;
          const w = editor.current().width;
          const span = r.end - r.start;
          const f = fit(span, w);
          session.setView({ ...v, samples_per_pixel: f.spp, scroll: Math.max(0, Math.round(r.start - span * 0.025)) });
        },
        enabled: () => timeSel() !== null,
        reason: () => "Drag across empty space in the Editor to select a time range.",
      }),
    ];
    const pluginsUrl = LINKS.plugins;
    if (pluginsUrl) offs.push(actions.register({ id: "help.plugins", run: () => openLink(pluginsUrl) }));
    const supportUrl = LINKS.support;
    if (supportUrl) offs.push(actions.register({ id: "help.support", run: () => openLink(supportUrl) }));
    const discordUrl = LINKS.discord;
    if (discordUrl) offs.push(actions.register({ id: "help.discord", run: () => openLink(discordUrl) }));
    for (const p of PANELS) {
      offs.push(
        actions.register({
          id: `window.panel.${p.id}`,
          run: () => dock.toggle(p.id),
          checked: () => openPanels().has(p.id),
        }),
      );
    }
    return offs;
  }

  return { register };
}
