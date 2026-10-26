// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Routes files dragged in from the system file manager to the panel under the pointer.
 *
 * The webview handles OS file drops natively on every platform (HTML
 * drag and drop of OS files is off), so we get one stream of events
 * with a position and hit-test registered panels ourselves. A drop that lands
 * on no registered panel goes to the fallback, which imports into the
 * Files panel, so a drop never silently does nothing.
 */

import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";

export interface DropTarget {
  readonly element: HTMLElement;
  /** `at` is the drop point in CSS pixels, relative to the window. */
  onDrop(paths: string[], at: { x: number; y: number }): void;
  /** Highlight on or off while files hover over this panel. */
  onHover?(over: boolean): void;
  /**
   * Where the files are over this panel while they hover (CSS pixels,
   * relative to the window), to preview where they would land; null when
   * they leave or drop. `paths` is empty while not known yet.
   */
  onMove?(paths: string[], at: { x: number; y: number } | null): void;
}

export type DropEvent =
  | { type: "enter"; paths: string[]; x: number; y: number }
  | { type: "over"; x: number; y: number }
  | { type: "drop"; paths: string[]; x: number; y: number }
  | { type: "leave" };

export class FileDropRouter {
  private readonly targets = new Set<DropTarget>();
  private hovered: DropTarget | null = null;
  /** The files being dragged, known from the start of the drag. */
  private paths: string[] = [];

  /**
   * `sessions`, when set, takes any drop that includes a session file
   * (.altaudio or .sesx), wherever it lands, with all the dropped paths.
   */
  constructor(
    private readonly fallback: (paths: string[]) => void,
    private readonly sessions?: (paths: string[]) => void,
  ) {}

  register(target: DropTarget): () => void {
    this.targets.add(target);
    return () => {
      if (this.hovered === target) this.setHovered(null);
      this.targets.delete(target);
    };
  }

  handle(e: DropEvent): void {
    switch (e.type) {
      case "enter":
      case "over":
        if (e.type === "enter") this.paths = e.paths;
        this.setHovered(this.hit(e.x, e.y));
        this.hovered?.onMove?.(this.paths, { x: e.x, y: e.y });
        break;
      case "drop": {
        const target = this.hit(e.x, e.y);
        this.setHovered(null);
        if (e.paths.length === 0) return;
        if (this.sessions && e.paths.some(isSessionFile)) this.sessions(e.paths);
        else if (target) target.onDrop(e.paths, { x: e.x, y: e.y });
        else this.fallback(e.paths);
        break;
      }
      case "leave":
        this.setHovered(null);
        this.paths = [];
        break;
    }
  }

  /** In-app drags (Drive Manager rows, library items): highlight the panel under the pointer. */
  dragOver(x: number, y: number, from?: HTMLElement, paths: string[] = []): void {
    this.setHovered(this.hit(x, y, from));
    this.hovered?.onMove?.(paths, { x, y });
  }

  /** Ends an in-app drag. Returns true when a panel took the files. No fallback. */
  dragEnd(paths: string[], x: number, y: number, from?: HTMLElement): boolean {
    const target = this.hit(x, y, from);
    this.setHovered(null);
    if (!target || paths.length === 0) return false;
    target.onDrop(paths, { x, y });
    return true;
  }

  dragCancel(): void {
    this.setHovered(null);
  }

  /** Topmost visible target containing the point, in CSS pixels. */
  private hit(x: number, y: number, exclude?: HTMLElement): DropTarget | null {
    for (const t of this.targets) {
      if (exclude && t.element === exclude) continue;
      const r = t.element.getBoundingClientRect();
      if (r.width === 0 || r.height === 0) continue;
      if (x >= r.left && x < r.right && y >= r.top && y < r.bottom) return t;
    }
    return null;
  }

  private setHovered(t: DropTarget | null): void {
    if (t === this.hovered) return;
    this.hovered?.onMove?.([], null);
    this.hovered?.onHover?.(false);
    this.hovered = t;
    t?.onHover?.(true);
  }
}

/**
 * Feeds the window's native drag and drop events into a router.
 * Does nothing outside the desktop app (unit tests, plain browser).
 */
/**
 * Pixels per CSS pixel for drop positions. The core reports drops in
 * device pixels; measuring the webview's real size against its CSS size
 * gives the true ratio even when display scaling and page zoom disagree
 * with devicePixelRatio (seen on Windows: drops landed in the wrong
 * panel). Falls back to devicePixelRatio when the size is unknown.
 */
export function dropScale(physicalWidth: number | null, cssWidth: number, dpr: number): number {
  if (physicalWidth && cssWidth > 0) {
    const r = physicalWidth / cssWidth;
    if (r > 0.25 && r < 8) return r;
  }
  return dpr > 0 ? dpr : 1;
}

export async function bindNativeFileDrop(router: FileDropRouter): Promise<() => void> {
  if (!isTauri()) return () => {};
  const webview = getCurrentWebview();
  let physicalWidth: number | null = null;
  const measure = () =>
    webview
      .size()
      .then((s) => (physicalWidth = s.width))
      .catch(() => (physicalWidth = null));
  await measure();
  const onResize = () => void measure();
  window.addEventListener("resize", onResize);
  const off = await webview.onDragDropEvent((event) => {
    const p = event.payload;
    const scale = dropScale(physicalWidth, window.innerWidth, window.devicePixelRatio);
    switch (p.type) {
      case "enter":
        void measure();
        router.handle({ type: "enter", paths: p.paths, x: p.position.x / scale, y: p.position.y / scale });
        break;
      case "over":
        router.handle({ type: "over", x: p.position.x / scale, y: p.position.y / scale });
        break;
      case "drop":
        router.handle({ type: "drop", paths: p.paths, x: p.position.x / scale, y: p.position.y / scale });
        break;
      case "leave":
        router.handle({ type: "leave" });
        break;
    }
  });
  return () => {
    window.removeEventListener("resize", onResize);
    off();
  };
}

/** A session file the app opens: our own or a .sesx. */
export function isSessionFile(path: string): boolean {
  return /\.(altaudio|sesx)$/i.test(path);
}
