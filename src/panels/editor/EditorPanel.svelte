<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Altered Audio Editor.

  Editor panel: the multitrack view. In P1 it shows the session (tracks,
  clips, waveforms, ruler, playhead) and supports adding files, track
  management and zoom. Playback arrives in P2, clip editing in P3.
-->
<script lang="ts">
  import { settings, zoomRatios } from "../../stores/settings";
  import { trackOp } from "../mixer/solo";
  import { busChoices, busOp, MASTER_ID, newSend } from "../mixer/routing";
  import SendList from "../mixer/SendList.svelte";
  import { onDestroy } from "svelte";
  import type { ActionRegistry } from "../../app/actions";
  import ContextMenu, { type ContextItem } from "../../app/ContextMenu.svelte";
  import type { FileDropRouter } from "../../app/filedrop";
  import { editor, type EditorState } from "../../stores/editor";
  import type { PeakStore } from "../../stores/peaks";
  import type { CommandRunner } from "../../stores/media-pool";
  import { clipLength, stretchOf, type BusDoc, type ClipChange, type ClipDoc, type SessionDoc, type SessionStore, type TrackChanges, type TrackDoc } from "../../stores/session";
  import { stretchTo } from "./stretch";
  import { clipsAt, fadeClip, LockedError, MIN_CLIP, moveClips, nearest, previewTracks, snapTargets, splitClips, trimClip, withCrossfades, withGroups } from "./clip-edit";
  import { shownTracks as listedTracks } from "./visibility";
  import { addPoint, chipBox, chipLabel, envChange, envName, formatValue, fxChoices, fxLaneOf, fxParam, levelAt, movePoint, pointsOf, rangeOf, restValue, snapValue, valueToY, yToValue, type EnvKind } from "./envelope";
  import LaneCanvas from "./LaneCanvas.svelte";
  import ClipColorDialog from "../../app/ClipColorDialog.svelte";
  import { laneRows, moveRowOps } from "./rows";
  import { monoCheck } from "../../stores/mono-check";
  import Dial from "./Dial.svelte";
  import TrackMeter from "./TrackMeter.svelte";
  import { inputLevels, liveTakes, type LiveTakes } from "../../stores/record";
  import { keepLast } from "../../app/settings/device-cache";
  import { audioInputs, openSoundSettings, type AudioInput } from "../../app/core";
  import WaveformView from "./WaveformView.svelte";
  import TransportBar, { type ZoomKind } from "./TransportBar.svelte";
  import type { Readable } from "svelte/store";
  import type { WaveFile } from "../../stores/wave";
  import OverviewStrip from "./OverviewStrip.svelte";
  import { KIND_COLORS, markerAt, markerCss } from "../markers/markers";
  import { type MeterLevel, type TransportState, type TransportStore } from "../../stores/transport";
  import { defaultTrackColor, trackHeight, formatIn, maxSppFor, rulerTicks, sampleAt, xOf, zoomAround, type Viewport, TRACK_GAP } from "./timeline";

  interface Props {
    session: SessionStore;
    runner: CommandRunner;
    peaks: PeakStore;
    transport: TransportStore;
    router: FileDropRouter;
    actions: ActionRegistry;
    /** Adds files as clips on a track (null makes a new track) at a session sample. */
    onPlaceFiles: (paths: string[], trackId: string | null, at: number) => void;
    shortcutFor?: (id: string) => string | undefined;
    onError?: (e: unknown) => void;
    /** Asks for one line of text. */
    prompt?: (title: string, label: string, value: string) => Promise<string | null>;
    /** The Waveform editor's document, shown instead of the lanes in waveform mode. */
    wave?: SessionStore;
    waveFile?: Readable<WaveFile | null>;
    waveTransport?: TransportStore;
    /** Waveform editor quick level: dB for the selection or whole file. */
    onWaveLevel?: (db: number) => void;
  }

  const {
    session,
    runner,
    peaks,
    transport,
    router,
    actions,
    onPlaceFiles,
    shortcutFor = () => undefined,
    onError = console.error,
    prompt = async () => null,
    wave,
    waveFile,
    waveTransport,
    onWaveLevel = () => {},
  }: Props = $props();

  let wf = $state<WaveFile | null>(null);
  $effect(() =>
    waveFile?.subscribe((f) => {
      wf = f;
    }),
  );

  let tp = $state<TransportState | null>(null);
  $effect(() =>
    transport.subscribe((t) => {
      tp = t;
    }),
  );

  const HEADER_W = 214;
  const RULER_H = 22;

  let doc = $state<SessionDoc | null>(null);
  $effect(() =>
    session.subscribe((d) => {
      doc = d;
    }),
  );

  let ed = $state<EditorState>(editor.current());
  $effect(() =>
    editor.subscribe((s) => {
      ed = s;
    }),
  );

  let peakTick = $state(0);
  $effect(() => peaks.subscribe(() => peakTick++));

  let tick = $state(0);
  $effect(() =>
    actions.changed.subscribe((n) => {
      tick = n;
    }),
  );
  const run = (id: string) => void actions.run(id);

  /** Nothing in the session yet and nothing chosen: show a start screen instead of six empty tracks. */
  let startDismissed = $state(false);
  const showStart = $derived(
    !startDismissed && doc != null && !session.path && doc.markers.length === 0 && (doc.buses?.length ?? 0) === 0 && doc.tracks.every((t) => t.clips.length === 0),
  );
  const shown = $derived(listedTracks(doc?.tracks ?? []));
  const headerTracks = $derived(showStart ? [] : shown);
  const listedIds = $derived(new Set(shown.map((t) => t.id)));
  /** Tracks with bus tracks placed among them, top to bottom. */
  const laneList = $derived(laneRows(doc, listedIds));
  const headerRows = $derived(showStart ? [] : laneList);
  /** Bus tracks, listed after the audio tracks. They hold no clips. */
  const buses = $derived(showStart ? [] : (doc?.buses ?? []));
  /** What the track headers show: the mix controls, or the output and sends. */
  let headerView = $state<"mix" | "sends">("mix");
  const busHeight = (b: BusDoc) => Math.max(headerView === "sends" ? 96 : 72, b.height);
  const can = (id: string) => {
    void tick;
    return actions.isEnabled(id);
  };
  const tip = (label: string, id: string) => {
    const k = shortcutFor(id);
    return k ? `${label} (${k})` : label;
  };

  let lanesWidth = $state(800);
  let lanesEl = $state<HTMLElement | null>(null);
  let scroller = $state<HTMLElement | null>(null);
  let ruler = $state<HTMLCanvasElement | null>(null);
  let dropping = $state(false);
  let menu = $state<{ x: number; y: number; items: ContextItem[] } | null>(null);
  let renaming = $state<string | null>(null);
  let dragTrack = $state<{ id: string; overIndex: number; y: number } | null>(null);
  /** The track or bus whose color is being picked. */
  let colorFor = $state<{ kind: "track" | "bus"; id: string; current: string | null } | null>(null);
  /** A marker being dragged on the ruler: its id and where it is now. */
  let markerDrag = $state<{ id: string; position: number } | null>(null);
  const shownMarkers = $derived(
    (doc?.markers ?? []).map((m) => (markerDrag?.id === m.id ? { ...m, position: markerDrag.position } : m)),
  );

  $effect(() => editor.setWidth(lanesWidth));

  const rate = $derived(doc?.sample_rate ?? 48000);
  const view = $derived<Viewport>({ spp: doc?.view.samples_per_pixel ?? 1024, scroll: doc?.view.scroll ?? 0 });
  const playhead = $derived(tp?.playing ? tp.position : (doc?.view.playhead ?? 0));

  // Page the view along while playing so the playhead stays visible.
  $effect(() => {
    if (!tp?.playing || lanesWidth <= 0) return;
    const x = xOf(tp.position, view);
    if (x > lanesWidth - 8 || x < 0) {
      setView({ scroll: Math.max(0, tp.position - lanesWidth * view.spp * 0.05) });
    }
  });
  const mediaMap = $derived(new Map((doc?.media ?? []).map((m) => [m.id, m])));
  const contentEnd = $derived.by(() => {
    if (!doc) return 0;
    let end = 0;
    for (const t of doc.tracks)
      for (const c of t.clips) end = Math.max(end, c.start + clipLength(c, mediaMap.get(c.media_id), rate));
    return end;
  });

  // Follow Playhead: during playback, page the lanes when the playhead
  // nears the right edge or falls off either side.
  $effect(() => {
    if (!tp?.playing || !ed.followPlayhead || lanesWidth <= 0) return;
    const x = xOf(playhead, view);
    if (x > lanesWidth * 0.92 || x < 0) setView({ scroll: playhead - lanesWidth * 0.08 * view.spp });
  });

  function setView(v: Partial<{ spp: number; scroll: number; playhead: number }>) {
    if (!doc) return;
    session.setView({
      ...doc.view,
      samples_per_pixel: v.spp ?? doc.view.samples_per_pixel,
      scroll: Math.max(0, Math.round(v.scroll ?? doc.view.scroll)),
      playhead: Math.max(0, Math.round(v.playhead ?? doc.view.playhead)),
    });
  }

  $effect(() => {
    if (!lanesEl) return;
    return router.register({
      element: lanesEl,
      onHover: (over) => (dropping = over),
      onDrop: (paths, at) => {
        const { trackId, sample } = hitLanes(at.x, at.y);
        onPlaceFiles(paths, trackId, sample);
      },
    });
  });

  /** Track and sample under a window point. */
  function hitLanes(x: number, y: number): { trackId: string | null; sample: number } {
    const rect = lanesEl!.getBoundingClientRect();
    const sample = sampleAt(x - rect.left, view);
    const rows = lanesEl!.querySelectorAll<HTMLElement>("[data-track]");
    for (const r of rows) {
      const b = r.getBoundingClientRect();
      if (y >= b.top && y < b.bottom) return { trackId: r.dataset.track!, sample };
    }
    return { trackId: null, sample };
  }

  // Ruler.
  $effect(() => {
    void tick;
    const c = ruler;
    if (!c) return;
    const dpr = window.devicePixelRatio || 1;
    const w = lanesWidth;
    c.width = Math.round(w * dpr);
    c.height = Math.round(RULER_H * dpr);
    c.style.width = `${w}px`;
    c.style.height = `${RULER_H}px`;
    const g = c.getContext("2d");
    if (!g) return;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    const cs = getComputedStyle(document.documentElement);
    g.fillStyle = cs.getPropertyValue("--surface-2");
    g.fillRect(0, 0, w, RULER_H);
    g.fillStyle = cs.getPropertyValue("--text-dim");
    g.strokeStyle = cs.getPropertyValue("--border-strong");
    g.font = `10px ${cs.getPropertyValue("--font-ui")}`;
    g.textBaseline = "top";
    for (const t of rulerTicks(view, w, rate)) {
      const x = Math.round(t.x) + 0.5;
      g.beginPath();
      g.moveTo(x, t.label ? 8 : 15);
      g.lineTo(x, RULER_H);
      g.stroke();
      if (t.label) {
        const text = ed.timeFormat === "time" ? t.label : formatIn(sampleAt(t.x, view), rate, ed.timeFormat);
        g.fillText(text, x + 3, 2);
      }
    }
    const sel = doc?.view.selection;
    if (sel) {
      const a = Math.max(0, xOf(sel.start, view));
      const b = Math.min(w, xOf(sel.end, view));
      if (b > a) {
        g.fillStyle = "rgba(255, 255, 255, 0.16)";
        g.fillRect(a, 0, b - a, RULER_H);
      }
    }
    for (const m of shownMarkers) {
      const mx = Math.round(xOf(m.position, view));
      const color = m.color || cs.getPropertyValue(KIND_COLORS[m.kind]) || "#c8b040";
      if (m.duration > 0) {
        const ex = xOf(m.position + m.duration, view);
        if (ex >= 0 && mx <= w) {
          g.fillStyle = color;
          g.globalAlpha = 0.35;
          g.fillRect(Math.max(0, mx), 0, Math.min(w, ex) - Math.max(0, mx), 6);
          g.globalAlpha = 1;
        }
      }
      if (mx < -60 || mx > w + 6) continue;
      g.fillStyle = color;
      g.beginPath();
      g.moveTo(mx, 0);
      g.lineTo(mx + 6, 0);
      g.lineTo(mx + 6, 6);
      g.lineTo(mx, 10);
      g.closePath();
      g.fill();
      g.fillRect(mx, 0, 1, RULER_H);
      const tw = g.measureText(m.name).width;
      g.fillStyle = cs.getPropertyValue("--surface-2");
      g.fillRect(mx + 7, 0, tw + 4, 12);
      g.fillStyle = cs.getPropertyValue("--text");
      g.fillText(m.name, mx + 9, 1);
    }
    const px = xOf(playhead, view);
    if (px >= -6 && px <= w + 6) {
      g.fillStyle = cs.getPropertyValue("--playhead");
      // Same pixel column as the line in the lanes, which fills [round(px), round(px) + 1].
      const cx = Math.round(px) + 0.5;
      g.beginPath();
      g.moveTo(cx - 5, RULER_H - 8);
      g.lineTo(cx + 5, RULER_H - 8);
      g.lineTo(cx, RULER_H);
      g.fill();
    }
  });

  /**
   * Drag to slide the view: left and right scroll the timeline, and with
   * `vertical` up and down scroll the tracks. A click that did not move
   * calls `onClick`. Used by the ruler and the middle mouse button.
   */
  function startPan(e: PointerEvent, vertical: boolean, onClick?: (clientX: number) => void) {
    e.preventDefault();
    const startX = e.clientX;
    const startY = e.clientY;
    const startScroll = view.scroll;
    const startTop = scroller?.scrollTop ?? 0;
    const spp = view.spp;
    let moved = false;
    const move = (ev: PointerEvent) => {
      if (!moved && Math.abs(ev.clientX - startX) < 3 && Math.abs(ev.clientY - startY) < 3) return;
      if (!moved) document.body.style.cursor = "grabbing";
      moved = true;
      setView({ scroll: Math.max(0, startScroll - (ev.clientX - startX) * spp) });
      if (vertical && scroller) scroller.scrollTop = startTop - (ev.clientY - startY);
    };
    const up = (ev: PointerEvent) => {
      window.removeEventListener("pointermove", move, true);
      window.removeEventListener("pointerup", up, true);
      document.body.style.cursor = "";
      if (!moved) onClick?.(ev.clientX);
    };
    window.addEventListener("pointermove", move, true);
    window.addEventListener("pointerup", up, true);
  }

  /**
   * Ruler: drag a marker flag to move it, click it to jump there.
   * Elsewhere, click moves the playhead and drag selects a time range.
   */
  function onRulerPointer(e: PointerEvent) {
    if (e.button !== 0 || !doc) return;
    const left = (e.currentTarget as HTMLElement).getBoundingClientRect().left;
    const hit = markerAt(doc.markers ?? [], e.clientX - left, (s) => xOf(s, view));
    if (!hit) {
      // Dragging the ruler slides the timeline; a plain click puts the playhead there.
      startPan(e, false, (clientX) => {
        const cur = session.current;
        if (cur?.view.selection) session.setView({ ...cur.view, selection: null });
        void transport.seek(sampleAt(clientX - left, view));
      });
      return;
    }
    e.preventDefault();
    const v = view;
    const startX = e.clientX;
    const targets = targetsFor({ ...doc, markers: doc.markers.filter((m) => m.id !== hit.id) }, new Set());
    let moved = false;
    const move = (ev: PointerEvent) => {
      if (!moved && Math.abs(ev.clientX - startX) < 3) return;
      moved = true;
      let p = Math.max(0, Math.round(hit.position + (ev.clientX - startX) * v.spp));
      const snapped = ed.snap.on && !ev.shiftKey ? nearest(p, targets, SNAP_PX * v.spp) : null;
      if (snapped !== null) p = snapped;
      markerDrag = { id: hit.id, position: p };
    };
    const up = () => {
      window.removeEventListener("pointermove", move, true);
      window.removeEventListener("pointerup", up, true);
      const d = markerDrag;
      markerDrag = null;
      if (!moved) {
        void transport.seek(hit.position);
        return;
      }
      if (d && d.position !== hit.position)
        void session.edit("Move Marker", { op: "set_markers", changes: [{ id: hit.id, position: d.position }] }, runner).catch(onError);
    };
    window.addEventListener("pointermove", move, true);
    window.addEventListener("pointerup", up, true);
  }

  function onRulerMenu(e: MouseEvent) {
    e.preventDefault();
    if (!doc) return;
    const left = (e.currentTarget as HTMLElement).getBoundingClientRect().left;
    const hit = markerAt(doc.markers ?? [], e.clientX - left, (s) => xOf(s, view));
    const at = sampleAt(e.clientX - left, view);
    const items: ContextItem[] = hit
      ? [
          { kind: "item", label: "Go to Marker", run: () => void transport.seek(hit.position) },
          {
            kind: "item",
            label: "Rename Marker...",
            run: async () => {
              const name = await prompt("Rename Marker", "Name", hit.name);
              if (name && name !== hit.name)
                await session.edit("Rename Marker", { op: "set_markers", changes: [{ id: hit.id, name }] }, runner).catch(onError);
            },
          },
          { kind: "item", label: "Delete Marker", run: () => void session.edit("Delete Marker", { op: "remove_markers", ids: [hit.id] }, runner).catch(onError) },
        ]
      : [
          {
            kind: "item",
            label: "Add Cue Marker Here",
            run: () => {
              void transport.seek(at).then(() => run("edit.marker.cue"));
            },
          },
          { kind: "item", label: "Add Range Marker", disabled: !can("edit.marker.range"), run: () => run("edit.marker.range") },
        ];
    menu = { x: e.clientX, y: e.clientY, items };
  }

  /** Wheel over a track header, or Alt+wheel over a lane: taller or shorter track (Shift: all tracks). Committed as one edit after the wheel stops. */
  let wheelTimer: ReturnType<typeof setTimeout> | undefined;
  function wheelResizeTracks(e: WheelEvent) {
    const head = (e.target as HTMLElement | null)?.closest<HTMLElement>("[data-header],[data-lane]");
    const id = head?.dataset.header ?? head?.dataset.lane;
    const track = id ? doc?.tracks.find((t) => t.id === id) : undefined;
    if (!track) return;
    const step = e.deltaY < 0 ? 1.12 : 1 / 1.12;
    if (e.shiftKey) {
      heightPreview = Math.max(0.2, Math.min(8, heightPreview * step));
    } else {
      const cur = resizing?.id === track.id ? resizing.h : track.height;
      resizing = { id: track.id, h: Math.max(40, Math.min(600, Math.round(cur * step))) };
    }
    clearTimeout(wheelTimer);
    wheelTimer = setTimeout(() => {
      if (heightPreview !== 1) {
        const f = heightPreview;
        heightPreview = 1;
        resizeTracks(f);
      }
      if (resizing) {
        const r = resizing;
        resizing = null;
        const t = doc?.tracks.find((x) => x.id === r.id);
        if (t && t.height !== r.h) setTrack(t, { height: r.h, collapsed: false }, "Track Height");
      }
    }, 300);
  }

  /**
   * The mouse wheel. Over the ruler it zooms the timeline; over a track's
   * header it makes the track taller or shorter (Shift: all tracks); over
   * the lanes it scrolls the tracks up and down. Alt + wheel scales the
   * waveforms, Ctrl + wheel zooms the timeline, Shift + wheel (or a
   * sideways wheel) scrolls along the timeline.
   */
  function onWheel(e: WheelEvent) {
    if (!lanesEl) return;
    const left = lanesEl.getBoundingClientRect().left;
    const overRuler = (e.target as HTMLElement | null)?.closest(".ruler-wrap") != null;
    const zoomTime = () => {
      e.preventDefault();
      const r = zoomRatios();
      const mouse = Math.max(0, e.clientX - left);
      const at = settings.current.wheelAnchor === "playhead" ? Math.max(0, Math.min(lanesEl!.clientWidth, xOf(playhead, view))) : mouse;
      const z = zoomAround(view, e.deltaY > 0 ? r.zoomOut : r.zoomIn, at, maxSppFor(lanesWidth, contentEnd, rate));
      setView({ spp: z.spp, scroll: z.scroll });
    };
    if (e.clientX < left && !overRuler) {
      e.preventDefault();
      wheelResizeTracks(e);
    } else if (e.altKey && e.shiftKey) {
      e.preventDefault();
      onZoom(e.deltaY < 0 ? "amp-in" : "amp-out");
    } else if (e.altKey) {
      // Alt+wheel over the lanes makes the track under the pointer taller or shorter.
      e.preventDefault();
      wheelResizeTracks(e);
    } else if (overRuler || e.ctrlKey) {
      zoomTime();
    } else if (e.shiftKey || Math.abs(e.deltaX) > Math.abs(e.deltaY)) {
      e.preventDefault();
      const d = e.shiftKey ? e.deltaY : e.deltaX;
      setView({ scroll: view.scroll + d * view.spp });
    }
    // Otherwise the wheel scrolls the tracks, which the scroller does by itself.
  }

  function clipAt(track: TrackDoc, x: number): ClipDoc | null {
    for (let i = track.clips.length - 1; i >= 0; i--) {
      const c = track.clips[i]!;
      const x0 = xOf(c.start, view);
      const x1 = xOf(c.start + clipLength(c, mediaMap.get(c.media_id), rate), view);
      if (x >= x0 && x <= x1) return c;
    }
    return null;
  }

  // Clip gestures. A drag shows its result live through `preview` and
  // becomes one undoable edit when the button is released.
  const EDGE_PX = 6;
  const TITLE_H = 15;
  const SNAP_PX = 8;
  type Gesture = "move" | "trim-left" | "trim-right" | "fade-in" | "fade-out" | "env-line" | "env-point";
  let preview = $state<ClipChange[]>([]);
  /** While Alt-dragging a clip edge: the stretch it would get. */
  let stretchHint = $state<string | null>(null);
  /** Value readout beside the pointer while dragging an envelope point. */
  let envTip = $state<{ x: number; y: number; text: string } | null>(null);
  let hoverCursor = $state("default");
  let levels = $state<Record<string, number>>({});
  /** Playback level of a track's channel, 0 when stopped. */
  /** A track's meter: the input while armed, otherwise playback (over lights stay after stopping). */
  function meterOf(track: TrackDoc): readonly [MeterLevel, MeterLevel] | null {
    if (track.record_armed) {
      const v = levels[track.id] ?? 0;
      const m = { level: v, peak: v, over: v >= 1 };
      return [m, m];
    }
    return tp?.tracks[track.id] ?? null;
  }
  $effect(() =>
    inputLevels.subscribe((v) => {
      levels = v;
    }),
  );
  let live = $state<LiveTakes | null>(null);
  $effect(() =>
    liveTakes.subscribe((v) => {
      live = v;
    }),
  );
  /** A razor blade for the Razor tool; the hot spot is the blade's cutting point. */
  const RAZOR_CURSOR = `url("data:image/svg+xml;utf8,${encodeURIComponent(
    '<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24"><g transform="rotate(-35 12 12)"><rect x="9" y="1" width="6" height="15" rx="1" fill="%23e8e8ee" stroke="%23111" stroke-width="1"/><path d="M12 3v11" stroke="%23111" stroke-width="1" stroke-dasharray="2 1.5"/><rect x="10" y="16" width="4" height="6" rx="1" fill="%23e5534b" stroke="%23111" stroke-width="1"/></g></svg>',
  ).replace(/%2523/g, "%23")}") 4 22, crosshair`;
  /** Edge trim cursors: a bracket with an arrow, pointing the way the edge moves out. */
  const trimCursor = (left: boolean) =>
    `url("data:image/svg+xml;utf8,${encodeURIComponent(
      `<svg xmlns="http://www.w3.org/2000/svg" width="22" height="22" viewBox="0 0 22 22"><g transform="${left ? "" : "translate(22 0) scale(-1 1)"}" stroke-linecap="square"><path d="M12 3h-3v16h3" fill="none" stroke="#111" stroke-width="3.6"/><path d="M12 3h-3v16h3" fill="none" stroke="#f2f2f5" stroke-width="1.6"/><path d="M20 11h-7M16 8l-3 3 3 3" fill="none" stroke="#111" stroke-width="3.4"/><path d="M20 11h-7M16 8l-3 3 3 3" fill="none" stroke="#f2f2f5" stroke-width="1.4"/></g></svg>`,
    )}") ${left ? 9 : 13} 11, ${left ? "w-resize" : "e-resize"}`;
  const shownTracks = $derived(doc && preview.length > 0 ? previewTracks(doc, preview) : null);
  const CURSORS: Record<Gesture, string> = {
    move: "grab",
    "trim-left": trimCursor(true),
    "trim-right": trimCursor(false),
    "fade-in": "ew-resize",
    "fade-out": "ew-resize",
    "env-line": "crosshair",
    "env-point": "move",
  };

  /** The y range the volume envelope uses inside a lane of `height` pixels. Matches LaneCanvas. */
  const envBox = (height: number) => ({ top: 1 + 14 + 2, height: height - TRACK_GAP - 2 - 14 - 4 });

  function zoneAt(track: TrackDoc, x: number, y: number): { kind: Gesture; clip: ClipDoc; index?: number; env?: EnvKind } | null {
    const clip = clipAt(track, x);
    if (!clip) return null;
    const x0 = xOf(clip.start, view);
    const x1 = xOf(clip.start + clipLength(clip, mediaMap.get(clip.media_id), rate), view);
    const w = x1 - x0;
    // Fade handles sit just under the title bar (see LaneCanvas).
    if (y > TITLE_H && y <= TITLE_H + 21 && w > 30) {
      const hit = (hx: number) => Math.abs(x - Math.min(Math.max(hx, x0 + 8), x1 - 8)) <= 8;
      if (hit(x0 + clip.fade_in.length / view.spp)) return { kind: "fade-in", clip };
      if (hit(x1 - clip.fade_out.length / view.spp)) return { kind: "fade-out", clip };
    }
    if (y > TITLE_H && !clip.locked) {
      const box = envBox(trackHeight(track));
      const lane = fxLaneOf(clip, ed.fxLanes);
      const kinds: EnvKind[] = [];
      if (ed.showVolume) kinds.push("volume");
      if (ed.showPan) kinds.push("pan");
      if (ed.showFx && lane) kinds.push("fx");
      // Points win over lines, so a point sitting on the other line stays reachable.
      for (const env of kinds) {
        const pts = pointsOf(clip, env, lane);
        const range = rangeOf(env, clip, lane);
        const hit = pts.findIndex(
          (p) => Math.abs(xOf(clip.start + p.position, view) - x) <= 5 && Math.abs(valueToY(env, p.value, box.top, box.height, range) - y) <= 5,
        );
        if (hit >= 0) return { kind: "env-point", clip, index: hit, env };
      }
      for (const env of kinds) {
        const pts = pointsOf(clip, env, lane);
        const v = pts.length ? levelAt(pts, sampleAt(x, view) - clip.start) : restValue(env, clip, lane);
        const lineY = valueToY(env, v, box.top, box.height, rangeOf(env, clip, lane));
        if (Math.abs(lineY - y) <= 4) return { kind: "env-line", clip, env };
      }
    }
    if (w > 16 && x - x0 <= EDGE_PX) return { kind: "trim-left", clip };
    if (w > 16 && x1 - x <= EDGE_PX) return { kind: "trim-right", clip };
    return { kind: "move", clip };
  }

  function laneXY(e: MouseEvent, el: HTMLElement): { x: number; y: number } {
    const r = el.getBoundingClientRect();
    return { x: e.clientX - r.left, y: e.clientY - r.top };
  }

  /** Razor tool: where a click would cut, shown as a line. Null when not over the lanes. */
  let razor = $state<{ x: number; trackId: string | null } | null>(null);

  /** Snaps a razor or slip position to the usual targets, unless snapping is off or Alt is held. */
  function snapAt(sample: number, alt: boolean): number {
    if (!doc || !ed.snap.on || alt) return sample;
    return nearest(sample, targetsFor(doc, new Set()), SNAP_PX * view.spp) ?? sample;
  }

  function onLaneHover(e: PointerEvent, track: TrackDoc) {
    if (e.buttons !== 0) return;
    const { x, y } = laneXY(e, e.currentTarget as HTMLElement);
    if (chipAt(track, x, y)) {
      hoverCursor = "pointer";
      return;
    }
    if (ed.tool === "time") {
      hoverCursor = "text";
      return;
    }
    if (ed.tool === "razor") {
      const at = snapAt(sampleAt(x, view), e.altKey);
      razor = { x: xOf(at, view), trackId: e.shiftKey ? null : track.id };
      hoverCursor = clipAt(track, x) || e.shiftKey ? RAZOR_CURSOR : "default";
      return;
    }
    const z = zoneAt(track, x, y);
    if (ed.tool === "slip" && z?.kind === "move") {
      hoverCursor = z.clip.locked ? "not-allowed" : "ew-resize";
      return;
    }
    hoverCursor = z ? (z.clip.locked ? "not-allowed" : CURSORS[z.kind]) : "default";
  }

  /** Razor: cut the clip under the pointer, or with Shift every clip at that time. */
  function razorCut(e: PointerEvent, track: TrackDoc, x: number) {
    if (!doc) return;
    const at = Math.round(snapAt(sampleAt(x, view), e.altKey));
    const hit = clipAt(track, x);
    const ids = e.shiftKey ? clipsAt(doc, at, new Set()) : hit ? [hit.id] : [];
    if (ids.length === 0) return;
    let op;
    try {
      op = splitClips(doc, ids, at, () => crypto.randomUUID());
    } catch (err) {
      if (err instanceof LockedError) return onError(err);
      throw err;
    }
    if (!op) return;
    void session.edit(e.shiftKey ? "Split All Tracks" : "Razor Split", op, runner).catch(onError);
  }

  /** Slip: drag the audio inside a clip; the clip stays where it is. */
  function startSlip(e: PointerEvent, clip: ClipDoc) {
    if (!doc || clip.locked) return;
    e.preventDefault();
    const m = mediaMap.get(clip.media_id);
    const sr = m?.sample_rate || rate;
    const toSrc = (d: number) => Math.round((d * sr) / rate / stretchOf(clip));
    const x0 = e.clientX;
    const v = view;
    let change: ClipChange | null = null;
    const move = (ev: PointerEvent) => {
      // Dragging right brings earlier audio into the clip.
      const d = toSrc((ev.clientX - x0) * v.spp);
      if (clip.looped) {
        const region = Math.max(1, clip.source_out - clip.source_in);
        const phase = (((clip.looped.phase - d) % region) + region) % region;
        change = { id: clip.id, looped: { ...clip.looped, phase } };
      } else {
        const room = m && m.length > 0 ? m.length - clip.source_out : Infinity;
        const shift = Math.max(-clip.source_in, Math.min(room, -d));
        change = { id: clip.id, source_in: clip.source_in + shift, source_out: clip.source_out + shift };
      }
      preview = [change];
    };
    const up = () => {
      window.removeEventListener("pointermove", move, true);
      window.removeEventListener("pointerup", up, true);
      preview = [];
      if (change) void session.edit("Slip Clip", { op: "set_clips", changes: [change] }, runner).catch(onError);
    };
    window.addEventListener("pointermove", move, true);
    window.addEventListener("pointerup", up, true);
  }

  /** The envelope chip of the clip under (x, y), if the point is on it. */
  function chipAt(track: TrackDoc, x: number, y: number): ClipDoc | null {
    if (y > TITLE_H) return null;
    const clip = clipAt(track, x);
    if (!clip) return null;
    const x0 = xOf(clip.start, view);
    const x1 = xOf(clip.start + clipLength(clip, mediaMap.get(clip.media_id), rate), view);
    const box = chipBox(x0, x1, chipLabel(ed.showVolume, ed.showPan, ed.showFx));
    return box && x >= box.x && x <= box.x + box.w ? clip : null;
  }

  function openChipMenu(e: PointerEvent, clip: ClipDoc) {
    const current = fxLaneOf(clip, ed.fxLanes);
    const choices = fxChoices(clip);
    const fx: ContextItem[] = choices.length
      ? [
          { kind: "item", label: "Hide Effect Envelopes", checked: !ed.showFx, run: () => editor.setShowFx(false) },
          { kind: "sep" },
          ...choices.map(
            (c): ContextItem => ({
              kind: "item",
              label: c.label,
              checked: ed.showFx && !!current && current.slot === c.lane.slot && current.param === c.lane.param,
              run: () => editor.pickFxLane(clip.id, c.lane),
            }),
          ),
        ]
      : [{ kind: "item", label: "Add an effect to this clip's Voice Chain first", disabled: true, run: () => {} }];
    menu = {
      x: e.clientX,
      y: e.clientY + 8,
      items: [
        { kind: "item", label: "Volume", checked: ed.showVolume, run: () => run("view.env.volume") },
        { kind: "item", label: "Pan", checked: ed.showPan, run: () => run("view.env.pan") },
        { kind: "sub", label: "Effect Setting", items: fx },
        { kind: "sep" },
        { kind: "item", label: "Add/Remove Effects", run: () => run("window.panel.voice-chain") },
        { kind: "sep" },
        { kind: "item", label: "Chain Input Gain", disabled: true, reason: "Chain-level envelopes are not built yet", run: () => {} },
        { kind: "item", label: "Chain Output Gain", disabled: true, reason: "Chain-level envelopes are not built yet", run: () => {} },
        { kind: "item", label: "Chain Mix", disabled: true, reason: "Chain-level envelopes are not built yet", run: () => {} },
        { kind: "item", label: "Chain Power", disabled: true, reason: "Chain-level envelopes are not built yet", run: () => {} },
      ],
    };
  }

  /** Double-click a clip: open its file in the Waveform editor. */
  function onLaneDouble(e: MouseEvent, track: TrackDoc) {
    if (ed.tool !== "move" && ed.tool !== "slip") return;
    const { x, y } = laneXY(e, e.currentTarget as HTMLElement);
    if (chipAt(track, x, y)) return;
    const z = zoneAt(track, x, y);
    if (!z) return;
    editor.selectClips([z.clip.id], track.id);
    run("clip.edit-source");
  }

  function onLanePointer(e: PointerEvent, track: TrackDoc) {
    if (e.button === 1) {
      startPan(e, true);
      return;
    }
    if (e.button !== 0 && e.button !== 2) return;
    const { x, y } = laneXY(e, e.currentTarget as HTMLElement);
    const chipClip = e.button === 0 ? chipAt(track, x, y) : null;
    if (chipClip) {
      e.preventDefault();
      openChipMenu(e, chipClip);
      return;
    }
    if (e.button === 0 && ed.tool === "time") {
      editor.selectTrack(track.id);
      startRange(e);
      return;
    }
    if (e.button === 0 && ed.tool === "razor") {
      e.preventDefault();
      razorCut(e, track, x);
      return;
    }
    const z = zoneAt(track, x, y);
    if (e.button === 0 && ed.tool === "slip" && z?.kind === "move") {
      if (!ed.selectedClips.has(z.clip.id)) editor.selectClips([z.clip.id], track.id);
      startSlip(e, z.clip);
      return;
    }
    if (!z) {
      editor.selectTrack(track.id);
      if (e.button === 0) {
        if (ed.tool === "move") startBoxSelect(e);
        else startRange(e);
      }
      return;
    }
    const clip = z.clip;
    if (e.button === 2 && ed.selectedClips.has(clip.id)) return;
    if (e.ctrlKey || e.metaKey) {
      const next = new Set(ed.selectedClips);
      const members = doc ? withGroups(doc, [clip.id]) : [clip.id];
      const on = !next.has(clip.id);
      for (const id of members) {
        if (on) next.add(id);
        else next.delete(id);
      }
      editor.selectClips(next, track.id);
      return;
    }
    if (e.shiftKey && e.button === 0 && doc) {
      // Shift+click: every clip on this track from the nearest selected clip to this one.
      const mine = track.clips.filter((c) => ed.selectedClips.has(c.id));
      if (mine.length) {
        const near = mine.reduce((a, b) => (Math.abs(a.start - clip.start) <= Math.abs(b.start - clip.start) ? a : b));
        const lo = Math.min(near.start, clip.start);
        const hi = Math.max(near.start, clip.start);
        const range = track.clips.filter((c) => c.start >= lo && c.start <= hi).map((c) => c.id);
        editor.selectClips(withGroups(doc, [...ed.selectedClips, ...range]), track.id);
        return;
      }
    }
    if (!ed.selectedClips.has(clip.id) && doc) editor.selectClips(withGroups(doc, [clip.id]), track.id);
    if (e.button === 0 && doc) {
      e.preventDefault();
      if (z.kind === "env-line" || z.kind === "env-point") {
        startEnvelope(e, (e.currentTarget as HTMLElement).getBoundingClientRect(), track, clip, z.env ?? "volume", z.index ?? null);
        return;
      }
      startGesture(e, doc, track, z.kind, clip);
    }
  }

  /**
   * Volume or pan envelope: pressing on the line adds a point there;
   * dragging moves the point in time (between its neighbors) and value.
   * Shift keeps the time and changes only the value. A tip beside the
   * pointer shows the value while dragging.
   */
  function startEnvelope(e: PointerEvent, lane: DOMRect, track: TrackDoc, clip: ClipDoc, env: EnvKind, index: number | null) {
    const v = view;
    const box = envBox(trackHeight(track));
    const len = clipLength(clip, mediaMap.get(clip.media_id), rate);
    const posAt = (cx: number) => sampleAt(cx - lane.left, v) - clip.start;
    const fxLane = env === "fx" ? fxLaneOf(clip, ed.fxLanes) : null;
    const range = rangeOf(env, clip, fxLane);
    const unit = fxParam(clip, fxLane)?.param.unit ?? "";
    const valAt = (cy: number, free = false) => {
      const v = yToValue(env, cy - lane.top, box.top, box.height, range);
      return free ? v : snapValue(env, v);
    };
    const name = envName(env, clip, fxLane);
    const change = (pts: typeof points): ClipChange => envChange(clip, env, pts, fxLane);
    let points = pointsOf(clip, env, fxLane);
    let i = index;
    let added = false;
    if (i === null) {
      const r = addPoint(points, Math.max(0, Math.min(len, posAt(e.clientX))), valAt(e.clientY));
      points = r.points;
      i = r.index;
      added = true;
      preview = [change(points)];
    }
    const at = i;
    const startPos = points[at]!.position;
    let moved = false;
    const move = (ev: PointerEvent) => {
      moved = true;
      const pos = ev.shiftKey ? startPos : posAt(ev.clientX);
      points = movePoint(points, at, pos, valAt(ev.clientY, ev.altKey), len, env, range);
      preview = [change(points)];
      envTip = { x: ev.clientX + 14, y: ev.clientY - 22, text: `${name} ${formatValue(env, points[at]!.value, unit)}` };
    };
    const up = () => {
      window.removeEventListener("pointermove", move, true);
      window.removeEventListener("pointerup", up, true);
      preview = [];
      envTip = null;
      if (!moved && !added) return;
      void session.edit(added ? `Add ${name} Point` : `Move ${name} Point`, { op: "set_clips", changes: [change(points)] }, runner).catch(onError);
    };
    window.addEventListener("pointermove", move, true);
    window.addEventListener("pointerup", up, true);
  }

  /** Snap targets for a drag, leaving out the dragged clips. */
  function targetsFor(d: SessionDoc, exclude: ReadonlySet<string>): number[] {
    const t = snapTargets(d, exclude, playhead, ed.snap);
    if (ed.snap.ruler) for (const tick of rulerTicks(view, lanesWidth, rate)) t.push(sampleAt(tick.x, view));
    return t;
  }

  /** Drag on empty lane space selects a time range; a plain click moves the playhead and clears it. */
  function startRange(e: PointerEvent, leftEdge?: number) {
    if (!doc || !lanesEl) return;
    e.preventDefault();
    const d0 = doc;
    const left = leftEdge ?? lanesEl.getBoundingClientRect().left;
    const v = view;
    const snap = ed.snap;
    const targets = targetsFor(d0, new Set());
    const at = (clientX: number, shift: boolean) => {
      const s = sampleAt(clientX - left, v);
      const hit = snap.on && !shift ? nearest(s, targets, SNAP_PX * v.spp) : null;
      return hit ?? s;
    };
    const anchor = at(e.clientX, e.shiftKey);
    const startX = e.clientX;
    let moved = false;
    const move = (ev: PointerEvent) => {
      if (!moved && Math.abs(ev.clientX - startX) < 3) return;
      moved = true;
      const p = at(ev.clientX, ev.shiftKey);
      const cur = session.current;
      if (cur) session.setView({ ...cur.view, selection: { start: Math.min(anchor, p), end: Math.max(anchor, p) } });
    };
    const up = (ev: PointerEvent) => {
      window.removeEventListener("pointermove", move, true);
      window.removeEventListener("pointerup", up, true);
      if (moved) return;
      const cur = session.current;
      if (cur?.view.selection) session.setView({ ...cur.view, selection: null });
      // While playing, only the ruler moves the playhead; a click in the lanes leaves playback alone.
      if (leftEdge !== undefined || !tp?.playing) void transport.seek(sampleAt(ev.clientX - left, v));
    };
    window.addEventListener("pointermove", move, true);
    window.addEventListener("pointerup", up, true);
  }

  /** Move tool on empty space: drag a dashed box to select the clips it touches. Shift adds. A click moves the playhead. */
  let selBox = $state<{ x: number; y: number; w: number; h: number } | null>(null);
  function startBoxSelect(e: PointerEvent) {
    if (!doc || !lanesEl) return;
    e.preventDefault();
    const box = lanesEl.getBoundingClientRect();
    const v = view;
    const x0 = e.clientX - box.left;
    const y0 = e.clientY - box.top;
    const keep = e.shiftKey ? new Set(editor.current().selectedClips) : new Set<string>();
    let moved = false;
    const pick = (x1: number, y1: number) => {
      const left = Math.min(x0, x1);
      const right = Math.max(x0, x1);
      const top = Math.min(y0, y1);
      const bottom = Math.max(y0, y1);
      selBox = { x: left, y: top, w: right - left, h: bottom - top };
      const t0 = sampleAt(left, v);
      const t1 = sampleAt(right, v);
      const ids = new Set(keep);
      for (const t of shown) {
        const row = lanesEl?.querySelector<HTMLElement>(`[data-track="${t.id}"]`);
        if (!row || row.offsetTop + row.offsetHeight < top || row.offsetTop > bottom) continue;
        for (const c of t.clips) {
          const end = c.start + clipLength(c, mediaMap.get(c.media_id), rate);
          if (end > t0 && c.start < t1) ids.add(c.id);
        }
      }
      editor.selectClips(ids);
    };
    const move = (ev: PointerEvent) => {
      const x1 = ev.clientX - box.left;
      const y1 = ev.clientY - box.top;
      if (!moved && Math.abs(x1 - x0) < 3 && Math.abs(y1 - y0) < 3) return;
      moved = true;
      pick(x1, y1);
    };
    const up = (ev: PointerEvent) => {
      window.removeEventListener("pointermove", move, true);
      window.removeEventListener("pointerup", up, true);
      selBox = null;
      if (moved) return;
      const cur = session.current;
      if (cur?.view.selection) session.setView({ ...cur.view, selection: null });
      if (!ev.shiftKey) editor.selectClips([]);
      if (!tp?.playing) void transport.seek(sampleAt(ev.clientX - box.left, v));
    };
    window.addEventListener("pointermove", move, true);
    window.addEventListener("pointerup", up, true);
  }

  function startGesture(e: PointerEvent, d0: SessionDoc, track: TrackDoc, kind: Gesture, clip: ClipDoc) {
    const startX = e.clientX;
    const startY = e.clientY;
    const selected = editor.current().selectedClips;
    // Moves take the whole selection; so do edge trims on a selected clip, by the same amount.
    const ids = kind === "move" || ((kind === "trim-left" || kind === "trim-right") && selected.has(clip.id)) ? [...selected] : [clip.id];
    const targets = targetsFor(d0, new Set(ids));
    const startRow = d0.tracks.findIndex((t) => t.id === track.id);
    const v = view;
    const end = clip.start + clipLength(clip, mediaMap.get(clip.media_id), rate);
    let row = startRow;
    let moved = false;
    let changes: ClipChange[] = [];
    // Alt while grabbing an edge stretches the clip instead of trimming it.
    const stretching = e.altKey && (kind === "trim-left" || kind === "trim-right") && !clip.looped && !clip.locked;
    let lastEv: PointerEvent | null = null;
    const move = (ev: PointerEvent) => {
      lastEv = ev;
      if (!moved && Math.abs(ev.clientX - startX) < 3 && Math.abs(ev.clientY - startY) < 3) return;
      moved = true;
      // The timeline may have scrolled under the drag (see the timer below), so add what it moved.
      const dx = (ev.clientX - startX) * v.spp + (view.scroll - v.scroll);
      const tol = ev.shiftKey || !ed.snap.on ? 0 : SNAP_PX * v.spp;
      if (kind === "move") {
        const over = hitLanes(ev.clientX, ev.clientY).trackId;
        if (over) row = d0.tracks.findIndex((t) => t.id === over);
        changes = moveClips(d0, ids, dx, row - startRow, targets, tol).changes;
        if (ed.autoCrossfade) changes = withCrossfades(d0, changes);
      } else if (stretching) {
        const m = mediaMap.get(clip.media_id);
        let edge = (kind === "trim-left" ? clip.start : end) + dx;
        if (tol > 0) edge = nearest(edge, targets, tol) ?? edge;
        edge = Math.round(edge);
        const left = kind === "trim-left" ? Math.max(0, Math.min(edge, end - MIN_CLIP)) : clip.start;
        const right = kind === "trim-right" ? Math.max(clip.start + MIN_CLIP, edge) : end;
        const r = stretchTo(clip, m, rate, right - left, clip.stretch?.keep_pitch ?? true);
        changes = r ? [left !== clip.start ? { ...r.change, start: left } : r.change] : [];
        stretchHint = r ? `${Math.round(r.ratio * 1000) / 10}%${r.limited ? " (limit)" : ""}` : null;
      } else if (kind === "trim-left" || kind === "trim-right") {
        const side = kind === "trim-left" ? "left" : "right";
        const c = trimClip(d0, clip, side, dx, targets, tol);
        changes = c ? [c] : [];
        if (c && ids.length > 1) {
          // The grabbed clip snaps; the rest follow by the same amount, each within its own limits.
          const moved = { ...clip, ...c } as ClipDoc;
          const by = side === "left" ? moved.start - clip.start : moved.start + clipLength(moved, mediaMap.get(clip.media_id), rate) - end;
          for (const t of d0.tracks)
            for (const other of t.clips) {
              if (other.id === clip.id || !ids.includes(other.id)) continue;
              const o = trimClip(d0, other, side, by);
              if (o) changes.push(o);
            }
        }
        if (ed.autoCrossfade) changes = withCrossfades(d0, changes);
      } else {
        const at = sampleAt(ev.clientX - lanesEl!.getBoundingClientRect().left, view);
        const c = fadeClip(d0, clip, kind === "fade-in" ? "in" : "out", kind === "fade-in" ? at - clip.start : end - at);
        changes = c ? [c] : [];
      }
      preview = changes;
    };
    // Holding the pointer near either end of the lanes scrolls the timeline with the drag.
    const timer = window.setInterval(() => {
      if (!moved || !lastEv || !lanesEl) return;
      const r = lanesEl.getBoundingClientRect();
      const band = 40;
      let px = 0;
      if (lastEv.clientX > r.right - band) px = Math.min(40, 4 + (lastEv.clientX - (r.right - band)) / 3);
      else if (lastEv.clientX < r.left + band) px = -Math.min(40, 4 + (r.left + band - lastEv.clientX) / 3);
      if (px === 0) return;
      const next = Math.max(0, view.scroll + px * view.spp);
      if (next === view.scroll) return;
      setView({ scroll: next });
      move(lastEv);
    }, 30);
    const up = () => {
      window.clearInterval(timer);
      window.removeEventListener("pointermove", move, true);
      window.removeEventListener("pointerup", up, true);
      preview = [];
      stretchHint = null;
      if (!moved || changes.length === 0) return;
      const label = stretching
        ? "Stretch Clip"
        : kind === "move"
          ? ids.length > 1
            ? `Move ${ids.length} Clips`
            : "Move Clip"
          : kind === "fade-in"
            ? "Fade In"
            : kind === "fade-out"
              ? "Fade Out"
              : changes.length > 1
                ? `Trim ${changes.length} Clips`
                : "Trim Clip";
      void session.edit(label, { op: "set_clips", changes }, runner).catch(onError);
    };
    window.addEventListener("pointermove", move, true);
    window.addEventListener("pointerup", up, true);
  }

  function onLaneMenu(e: MouseEvent, track: TrackDoc) {
    e.preventDefault();
    e.stopPropagation();
    const items: ContextItem[] = [];
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const z = zoneAt(track, e.clientX - r.left, e.clientY - r.top);
    if (!z) {
      // Empty space in a lane: the session-wide menu. Keep any chosen clips, but make this the chosen track.
      editor.selectClips(ed.selectedClips, track.id);
      menu = { x: e.clientX, y: e.clientY, items: emptyLaneItems() };
      return;
    }
    if (z && (z.kind === "env-point" || z.kind === "env-line")) {
      const clip = z.clip;
      const env = z.env ?? "volume";
      const fxLane = env === "fx" ? fxLaneOf(clip, ed.fxLanes) : null;
      const name = envName(env, clip, fxLane);
      const pts = pointsOf(clip, env, fxLane);
      const setPts = (label: string, next: typeof pts) =>
        void session.edit(label, { op: "set_clips", changes: [envChange(clip, env, next, fxLane)] }, runner).catch(onError);
      if (z.kind === "env-point" && z.index !== undefined) {
        const idx = z.index;
        items.push({ kind: "item", label: `Delete ${name} Point`, run: () => setPts(`Delete ${name} Point`, pts.filter((_, k) => k !== idx)) });
      }
      items.push({ kind: "item", label: `Reset ${name} Envelope`, disabled: pts.length === 0, run: () => setPts(`Reset ${name} Envelope`, []) }, { kind: "sep" });
    }
    if (ed.selectedClips.size > 0) {
      const act = (label: string, id: string): ContextItem => ({ kind: "item", label, shortcut: shortcutFor(id), disabled: !can(id), run: () => run(id) });
      items.push(
        act("Split at Playhead", "clip.split"),
        act("Trim Start to Playhead", "clip.trim.start"),
        act("Trim End to Playhead", "clip.trim.end"),
        { kind: "sep" },
        act(actions.isChecked("clip.mute") ? "Unmute Clip" : "Mute Clip", "clip.mute"),
        act(actions.isChecked("clip.lock") ? "Unlock Position" : "Lock Position", "clip.lock"),
        act("Clip Gain...", "clip.gain"),
        act("Clip Color...", "clip.color"),
        act("Group Clips", "clip.group"),
        act("Ungroup Clips", "clip.ungroup"),
        {
          kind: "sub",
          label: "Audio Type",
          items: [
            { ...(act("Dialogue", "clip.type.dialogue") as Extract<ContextItem, { kind: "item" }>), checked: actions.isChecked("clip.type.dialogue") === true },
            { ...(act("Music", "clip.type.music") as Extract<ContextItem, { kind: "item" }>), checked: actions.isChecked("clip.type.music") === true },
            { ...(act("Sound Effects", "clip.type.sfx") as Extract<ContextItem, { kind: "item" }>), checked: actions.isChecked("clip.type.sfx") === true },
            { ...(act("Ambience", "clip.type.ambience") as Extract<ContextItem, { kind: "item" }>), checked: actions.isChecked("clip.type.ambience") === true },
            { kind: "sep" },
            act("None", "clip.type.none"),
          ],
        },
        act("Time Stretch...", "clip.stretch"),
        act(actions.isChecked("clip.fade-in") ? "Remove Fade In" : "Fade In", "clip.fade-in"),
        act(actions.isChecked("clip.fade-out") ? "Remove Fade Out" : "Fade Out", "clip.fade-out"),
        { kind: "sep" },
        { kind: "item", label: ed.selectedClips.size > 1 ? `Delete ${ed.selectedClips.size} Clips` : "Delete Clip", shortcut: shortcutFor("edit.delete"), run: () => run("edit.delete") },
        { kind: "item", label: "Show in Files Panel", disabled: !can("clip.reveal"), run: () => run("clip.reveal") },
        { kind: "sep" },
      );
    }
    items.push(...trackItems(track));
    menu = { x: e.clientX, y: e.clientY, items };
  }

  /** The right-click menu for empty space in a lane. Every entry runs an app action, so it stays in step with the menu bar. */
  function emptyLaneItems(): ContextItem[] {
    const act = (label: string, id: string): ContextItem => {
      const ok = can(id);
      return {
        kind: "item",
        label,
        shortcut: shortcutFor(id),
        disabled: !ok,
        reason: ok ? null : actions.has(id) ? actions.reason(id) : "Not built yet.",
        run: () => run(id),
      };
    };
    const off = (label: string, reason: string): ContextItem => ({ kind: "item", label, disabled: true, reason, run: () => {} });
    const sub = (label: string, items: ContextItem[], reason?: string): ContextItem => ({ kind: "sub", label, items, ...(reason ? { disabled: true, reason } : {}) });
    const sep: ContextItem = { kind: "sep" };
    return [
      act("Paste", "edit.paste"),
      sep,
      sub("Ripple Delete", [
        act("Selected Clips", "edit.ripple.clips"),
        act("Time Selection in Selected Clips", "edit.ripple.time-clips"),
        act("Time Selection in Selected Track", "edit.ripple.time-track"),
        act("Time Selection in All Tracks", "edit.ripple.time-all"),
        act("Gap Between Clips", "edit.ripple.gap"),
      ]),
      sub("Select", [
        act("Select All", "edit.select-all"),
        act("Deselect All", "edit.deselect"),
        sep,
        act("All Clips in Track", "edit.select-track-clips"),
        act("Next Clip in Selected Track", "edit.select-next-clip"),
        act("Clips to End of Track", "edit.select-clips-track-end"),
        sep,
        act("Clips to Start of Session", "edit.select-clips-session-start"),
        act("Clips to End of Session", "edit.select-clips-session-end"),
        sub("Clips of Audio Type", [
          act("Dialogue", "edit.select-type.dialogue"),
          act("Music", "edit.select-type.music"),
          act("Sound Effects", "edit.select-type.sfx"),
          act("Ambience", "edit.select-type.ambience"),
        ]),
        sep,
        act("Select Current View Time", "edit.select-view-time"),
        act("Select All Time", "edit.select-all-time"),
        act("Select Time Around Selected Clips", "edit.select-time-around-clips"),
        act("Clear Time Selection", "edit.clear-time-selection"),
      ]),
      sub("Groups", [
        act("Group Clips", "clip.group"),
        act("Suspend Groups", "clip.group-suspend"),
        act("Remove Focus Clip from Group", "clip.group-remove-focus"),
        act("Ungroup Selected Clips", "clip.ungroup"),
      ]),
      sep,
      sub("Track", [
        act("Add Mono Audio Track", "multitrack.add.mono"),
        act("Add Stereo Audio Track", "multitrack.add.stereo"),
        act("Add Tracks...", "multitrack.add-tracks"),
        off("Add 5.1 Audio Track", "Surround tracks are not planned."),
        sep,
        act("Add Mono Bus Track", "multitrack.add.bus-mono"),
        act("Add Stereo Bus Track", "multitrack.add.bus"),
        off("Add 5.1 Bus Track", "Surround tracks are not planned."),
        sep,
        act("Delete Empty Tracks", "multitrack.delete-empty"),
        sep,
        act("Delete Selected Track", "multitrack.delete-track"),
        act("Duplicate Selected Track", "multitrack.duplicate-track"),
      ]),
      sub("Track Visibility", [
        act("Hide Selected Track", "multitrack.vis.hide-selected"),
        act("Show All Tracks", "multitrack.vis.show-all"),
        sep,
        act("Show Only Tracks with Clips", "multitrack.vis.only-clips"),
        act("Show Only Tracks with Clips within Time Selection", "multitrack.vis.only-selection"),
        act("Show Only Tracks with Clips at Playhead", "multitrack.vis.only-playhead"),
        sep,
        off("Show Only Bus Tracks", "Not built yet: bus tracks always show below the audio tracks."),
        off("Load and Save Visibility Presets", "Not built yet."),
      ]),
      sub("Insert", [
        act("Files at Playhead...", "multitrack.insert-files"),
        act("Files as Tracks...", "multitrack.insert-tracks"),
        act("Selected Files as Tracks", "multitrack.insert-selected-tracks"),
        sep,
        act("Silence...", "multitrack.insert-silence"),
      ]),
      sub("Marker", [
        act("Add Cue Marker", "edit.marker.cue"),
        act("Add Subclip Marker", "edit.marker.subclip"),
        act("Add CD Track Marker", "edit.marker.cd-track"),
        act("Add Cart Timer Marker", "edit.marker.cart"),
        act("Add Range Marker", "edit.marker.range"),
        act("Add Chapter Marker", "edit.marker.chapter"),
        act("Add Ad Slot", "edit.marker.ad-slot"),
        act("Add Social Clip", "edit.marker.social"),
      ]),
      sep,
      sub("Mixdown Session to New File", [
        act("Entire Session", "multitrack.mixdown.all"),
        act("Time Selection", "multitrack.mixdown.selection"),
        act("Selected Clips", "multitrack.mixdown.clips"),
      ]),
      sub("Export Mixdown", [act("Entire Session...", "file.export.mixdown"), act("Time Selection...", "file.export.selection")]),
      sub("Bounce to New Track", [
        off("Selected Track", "Not built yet."),
        act("Time Selection", "multitrack.bounce.selection"),
        off("Selected Clips in Time Selection", "Not built yet."),
        act("Selected Clips Only", "multitrack.bounce.clips"),
      ]),
      sep,
      act("Session Properties...", "multitrack.properties"),
    ];
  }

  /** Output and send choices for a track's right-click menu. */
  function routingItems(track: TrackDoc): ContextItem[] {
    if (!doc) return [];
    const choices = busChoices(doc, track.id);
    const sends = track.sends ?? [];
    const out = track.output ?? { type: "master" as const };
    const none = (doc.buses?.length ?? 0) === 0 ? "Add a bus first: Multitrack > Add > Bus Track." : "No bus can take this track without feeding back into it.";
    return [
      {
        kind: "sub",
        label: "Output To",
        items: [
          { kind: "item", label: "Master", checked: out.type === "master", run: () => setTrack(track, { output: { type: "master" } }, "Route Track Output") },
          ...choices.map((b): ContextItem => ({ kind: "item", label: b.name, checked: out.type === "bus" && out.bus_id === b.id, run: () => setTrack(track, { output: { type: "bus", bus_id: b.id } }, "Route Track Output") })),
        ],
      },
      {
        kind: "sub",
        label: "Add Send To",
        disabled: choices.length === 0,
        reason: choices.length === 0 ? none : null,
        items: choices.map((b): ContextItem => ({
          kind: "item",
          label: b.name,
          run: () => {
            void session.edit("Add Send", { op: "set_sends", owner: track.id, sends: [...sends, newSend(b.id, crypto.randomUUID())] }, runner).catch(onError);
            headerView = "sends";
          },
        })),
      },
    ];
  }

  function busItems(b: BusDoc): ContextItem[] {
    return [
      { kind: "item", label: "Rename Bus", run: () => (renaming = b.id) },
      { kind: "item", label: "Bus Color...", run: () => (colorFor = { kind: "bus", id: b.id, current: b.color }) },
      { kind: "item", label: "Delete Bus", run: () => void session.edit("Delete Bus", { op: "remove_bus", id: b.id }, runner).catch(onError) },
      { kind: "sep" },
      { kind: "item", label: "Add Bus Track", run: () => run("multitrack.add.bus") },
    ];
  }

  function setBus(b: BusDoc, changes: Record<string, unknown>, label: string) {
    void session.edit(label, busOp(doc, b.id, changes), runner).catch(onError);
  }

  function commitBusRename(b: BusDoc, value: string) {
    renaming = null;
    const v = value.trim();
    if (v && v !== b.name) setBus(b, { name: v }, "Rename Bus");
  }

  function trackItems(track: TrackDoc): ContextItem[] {
    return [
      { kind: "item", label: "Select All Clips on Track", disabled: track.clips.length === 0, run: () => editor.selectClips(withGroups(doc!, track.clips.map((c) => c.id)), track.id) },
      { kind: "sep" },
      { kind: "item", label: "Rename Track", run: () => (renaming = track.id) },
      { kind: "item", label: "Track Color...", run: () => (colorFor = { kind: "track", id: track.id, current: track.color ?? null }) },
      { kind: "item", label: "Duplicate Track", run: () => { editor.selectTrack(track.id); run("multitrack.duplicate-track"); } },
      { kind: "item", label: "Delete Track", run: () => { editor.selectTrack(track.id); run("multitrack.delete-track"); } },
      { kind: "sep" },
      { kind: "item", label: "Add Mono Track", run: () => { editor.selectTrack(track.id); run("multitrack.add.mono"); } },
      { kind: "item", label: "Add Stereo Track", run: () => { editor.selectTrack(track.id); run("multitrack.add.stereo"); } },
      { kind: "item", label: "Add Bus Track", run: () => run("multitrack.add.bus") },
      { kind: "sep" },
      ...routingItems(track),
    ];
  }

  function setTrack(track: TrackDoc, changes: TrackChanges, label: string) {
    void session.edit(label, trackOp(doc, track.id, changes as Record<string, unknown>), runner).catch(onError);
  }

  const formatDb = (v: number) => (v > 0 ? `+${v.toFixed(1)}` : v.toFixed(1));
  const formatPan = (p: number) => (Math.round(p) === 0 ? "C" : p < 0 ? `L${Math.round(-p)}` : `R${Math.round(p)}`);

  function setValue(track: TrackDoc, key: "volume_db" | "pan", value: number) {
    const label = key === "volume_db" ? "Track Volume" : "Track Pan";
    void session.edit(label, { op: "set_track", id: track.id, changes: { [key]: value } }, runner).catch(onError);
  }

  function commitRename(track: TrackDoc, value: string) {
    renaming = null;
    const v = value.trim();
    if (v && v !== track.name) setTrack(track, { name: v }, "Rename Track");
  }

  // Track reordering by dragging the header.
  function onHeaderPointer(e: PointerEvent, track: TrackDoc) {
    if (e.button !== 0 || (e.target as HTMLElement).closest("button, input")) return;
    startRowDrag(e, track.id, "Move Track");
  }

  /** Drags a track or bus row (by its header or grab strip) to a new place among all rows. */
  function startRowDrag(e: PointerEvent, id: string, label: string) {
    if (e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    editor.selectTrack(id);
    const startY = e.clientY;
    let moved = false;
    const list = () => [...(scroller?.querySelectorAll<HTMLElement>("[data-row]") ?? [])];
    let beforeId: string | null = null;
    const move = (ev: PointerEvent) => {
      if (!moved && Math.abs(ev.clientY - startY) < 5) return;
      moved = true;
      const rows = list();
      let over = rows.length;
      for (let i = 0; i < rows.length; i++) {
        const r = rows[i]!.getBoundingClientRect();
        if (ev.clientY < r.top + r.height / 2) {
          over = i;
          break;
        }
      }
      beforeId = rows[over]?.dataset.row ?? null;
      const sb = scroller!.getBoundingClientRect();
      const ref = rows[over] ?? rows[rows.length - 1];
      const y = ref ? (over < rows.length ? ref.getBoundingClientRect().top : ref.getBoundingClientRect().bottom) - sb.top + scroller!.scrollTop : 0;
      dragTrack = { id, overIndex: over, y };
    };
    const up = () => {
      window.removeEventListener("pointermove", move, true);
      window.removeEventListener("pointerup", up, true);
      const d = dragTrack;
      dragTrack = null;
      if (!moved || !d || !doc) return;
      const ops = moveRowOps(doc, listedIds, id, beforeId);
      if (ops.length === 0) return;
      void session.edit(label, ops.length === 1 ? ops[0]! : { op: "batch", ops }, runner).catch(onError);
    };
    window.addEventListener("pointermove", move, true);
    window.addEventListener("pointerup", up, true);
  }

  function setMasterVolume(db: number) {
    void session.edit("Master Volume", { op: "set_master_volume", volume_db: db }, runner).catch(onError);
  }

  // Length the overview strip scrolls over.
  const total = $derived(Math.max(contentEnd * 1.1, view.scroll + lanesWidth * view.spp, rate * 60));

  /** The track's input as shown on its picker. */
  function inputLabel(track: TrackDoc): string {
    if (track.input === "none") return "None";
    if (track.input) return track.input;
    return track.channels === "mono" ? "Default Mono Input" : "Default Stereo Input";
  }

  /** Input picker: default, None, then Mono and Stereo submenus, each grouped by device. */
  async function openInputMenu(e: MouseEvent, track: TrackDoc) {
    e.stopPropagation();
    const x = e.clientX;
    const y = e.clientY;
    let inputs: AudioInput[] = [];
    try {
      inputs = keepLast("in:track", await audioInputs()).list;
    } catch {
      inputs = [];
    }
    const pick = (value: string | null, label: string): ContextItem => ({
      kind: "item",
      label,
      checked: (track.input ?? null) === value,
      run: () => setTrack(track, { input: value }, "Track Input"),
    });
    const items: ContextItem[] = [pick(null, track.channels === "mono" ? "Default Mono Input" : "Default Stereo Input"), pick("none", "None"), { kind: "sep" }];
    if (inputs.length === 0) items.push({ kind: "item", label: "No audio inputs found", run: () => {}, disabled: true });
    // Grouped as Mono and Stereo, then by device, so a system with many
    // devices stays readable. The stored value is unchanged.
    const mono: ContextItem[] = [];
    const stereo: ContextItem[] = [];
    for (const d of inputs) {
      const m: ContextItem[] = [];
      for (let c = 1; c <= d.channels; c++) m.push({ ...pick(`Mono: ${d.name} ${c}`, `Channel ${c}`) });
      if (m.length) mono.push({ kind: "sub", label: d.name, items: m });
      const st: ContextItem[] = [];
      for (let c = 1; c + 1 <= d.channels; c += 2) st.push(pick(`Stereo: ${d.name} ${c}-${c + 1}`, `Channels ${c}-${c + 1}`));
      if (st.length) stereo.push({ kind: "sub", label: d.name, items: st });
    }
    if (mono.length) items.push({ kind: "sub", label: "Mono", items: mono });
    if (stereo.length) items.push({ kind: "sub", label: "Stereo", items: stereo });
    items.push({ kind: "sep" }, { kind: "item", label: "Audio Hardware...", run: () => void openSoundSettings().catch(onError) });
    menu = { x, y, items };
  }

  /** Waveform display scale in the lanes. View only: playback is unchanged. */
  let ampZoom = $state(1);

  function onZoom(k: ZoomKind) {
    if (k === "amp-in") ampZoom = Math.min(16, ampZoom * 2);
    else if (k === "amp-out") ampZoom = Math.max(1, ampZoom / 2);
    else if (k === "tracks-in" || k === "tracks-out") resizeTracks(k === "tracks-in" ? 1.25 : 0.8);
    else run(k === "in" ? "view.zoom-in" : k === "out" ? "view.zoom-out" : k === "selection" ? "view.zoom-selection" : "view.zoom-session");
  }

  // Track scrollbar.
  let scrollerH = $state(0);
  let rowsH = $state(1);
  let vbarH = $state(0);
  let scrollTop = $state(0);
  function syncVbar() {
    scrollTop = scroller?.scrollTop ?? 0;
  }

  function onVbarPointer(e: PointerEvent) {
    if (e.button !== 0 || !scroller) return;
    e.preventDefault();
    const bar = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const scale = rowsH / Math.max(1, bar.height);
    const thumbTop = bar.top + scrollTop / scale;
    const thumbBottom = thumbTop + scrollerH / scale;
    const y = e.clientY;
    const zoom = y - thumbTop < 6 && y >= thumbTop - 2 ? "top" : thumbBottom - y < 6 && y <= thumbBottom + 2 ? "bottom" : null;
    if (zoom) {
      // Dragging an end of the thumb makes every track taller or shorter.
      const start = (doc?.tracks ?? []).map((t) => ({ t, h: t.height }));
      const span0 = Math.max(8, thumbBottom - thumbTop);
      let factor = 1;
      const move = (ev: PointerEvent) => {
        const d = zoom === "bottom" ? ev.clientY - y : y - ev.clientY;
        factor = Math.max(0.25, Math.min(4, span0 / Math.max(8, span0 + d)));
        heightPreview = factor;
      };
      const up = () => {
        window.removeEventListener("pointermove", move, true);
        window.removeEventListener("pointerup", up, true);
        heightPreview = 1;
        const ops = start
          .map(({ t, h }) => ({ t, h: Math.max(40, Math.min(600, Math.round(h * factor))) }))
          .filter(({ t, h }) => h !== t.height)
          .map(({ t, h }) => ({ op: "set_track" as const, id: t.id, changes: { height: h } }));
        if (ops.length) void session.edit("Track Height", { op: "batch", ops }, runner).catch(onError);
      };
      window.addEventListener("pointermove", move, true);
      window.addEventListener("pointerup", up, true);
      return;
    }
    // Click outside the thumb jumps there; then drag to scroll.
    if (y < thumbTop || y > thumbBottom) scroller.scrollTop = (y - bar.top) * scale - scrollerH / 2;
    const st0 = scroller.scrollTop;
    const move = (ev: PointerEvent) => {
      if (scroller) scroller.scrollTop = st0 + (ev.clientY - y) * scale;
    };
    const up = () => {
      window.removeEventListener("pointermove", move, true);
      window.removeEventListener("pointerup", up, true);
    };
    window.addEventListener("pointermove", move, true);
    window.addEventListener("pointerup", up, true);
  }
  /** Live scale for all track heights while an end of the track scrollbar is dragged. */
  let heightPreview = $state(1);

  /** A track being resized by its bottom edge: its live height. */
  let resizing = $state<{ id: string; h: number } | null>(null);
  const shownHeight = (t: TrackDoc) =>
    resizing?.id === t.id ? resizing.h : t.collapsed ? trackHeight(t) : heightPreview !== 1 ? Math.max(40, Math.min(600, Math.round(t.height * heightPreview))) : t.height;

  function startResize(e: PointerEvent, track: TrackDoc) {
    if (e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    const y0 = e.clientY;
    const h0 = track.height;
    resizing = { id: track.id, h: h0 };
    const move = (ev: PointerEvent) => {
      resizing = { id: track.id, h: Math.max(40, Math.min(600, Math.round(h0 + ev.clientY - y0))) };
    };
    const up = () => {
      window.removeEventListener("pointermove", move, true);
      window.removeEventListener("pointerup", up, true);
      const h = resizing?.h ?? h0;
      resizing = null;
      if (h !== h0) setTrack(track, { height: h }, "Track Height");
    };
    window.addEventListener("pointermove", move, true);
    window.addEventListener("pointerup", up, true);
  }

  /** Zoom Out Full on both axes: the whole session across, every track fitted to the lanes' height. */
  async function zoomOutAll() {
    const tracks = shown;
    const avail = (scroller?.clientHeight ?? 0) - 8;
    if (tracks.length && avail > 0) {
      const h = Math.max(40, Math.min(600, Math.floor(avail / tracks.length)));
      const ops = tracks.filter((t) => t.height !== h || t.collapsed).map((t) => ({ op: "set_track" as const, id: t.id, changes: { height: h, collapsed: false } }));
      if (ops.length) await session.edit("Track Height", { op: "batch", ops }, runner).catch(onError);
    }
    // After the edit, so the new view is not replaced by the edit's result.
    run("view.zoom-session");
  }

  /** Taller or shorter tracks, all at once. Heights stay between 40 and 600 pixels. */
  function resizeTracks(factor: number) {
    const tracks = shown;
    const ops = tracks
      .map((t) => ({ t, h: Math.max(40, Math.min(600, Math.round(t.height * factor))) }))
      .filter(({ t, h }) => h !== t.height)
      .map(({ t, h }) => ({ op: "set_track" as const, id: t.id, changes: { height: h } }));
    if (ops.length) void session.edit("Track Height", { op: "batch", ops }, runner).catch(onError);
  }

  /** A track's color for its header strip: its own color, or the palette by row. */
  function trackColor(c: string | null | undefined, row: number): string {
    const v = c ?? defaultTrackColor(row);
    return v.startsWith("--") ? `var(${v})` : v;
  }

  onDestroy(() => {
    menu = null;
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
{#if ed.mode === "waveform" && wave && waveTransport}
  <WaveformView {wave} file={wf} transport={waveTransport} {peaks} {actions} {shortcutFor} onLevel={onWaveLevel} />
{:else}
<section class="editor" aria-label="Multitrack" data-area="multitrack">
  <div class="toolbar">
    <button type="button" class="tool text" title={tip("Add Mono Track", "multitrack.add.mono")} onclick={() => run("multitrack.add.mono")}>+ Mono</button>
    <button type="button" class="tool text" title={tip("Add Stereo Track", "multitrack.add.stereo")} onclick={() => run("multitrack.add.stereo")}>+ Stereo</button>
    <button type="button" class="tool text" title={tip("Add Bus Track", "multitrack.add.bus")} onclick={() => run("multitrack.add.bus")}>+ Bus</button>
    <span class="spacer"></span>
    {#if doc}<span class="rate" title="Session sample rate">{(rate / 1000).toLocaleString("en-US")} kHz</span>{/if}
  </div>

  <div class="overview-row">
    <div class="corner" style:width="{HEADER_W}px"></div>
    <OverviewStrip
      {doc}
      media={mediaMap}
      {view}
      {lanesWidth}
      {total}
      {playhead}
      onView={(v) => setView({ spp: v.spp, scroll: v.scroll })}
      onFit={() => run("view.zoom-session")}
    />
    <button type="button" class="fit-all" title="Zoom Out Full: the whole session in time, and every track in view" aria-label="Zoom out full, time and tracks" onclick={zoomOutAll}>
      <svg viewBox="0 0 16 16" aria-hidden="true"><circle cx="7" cy="7" r="4.5" fill="none" stroke="currentColor" stroke-width="1.3" /><path d="M10.5 10.5l3.5 3.5M5 7h4" stroke="currentColor" stroke-width="1.3" /><path d="M1 3V1h2M13 1h2v2M15 13v2h-2M3 15H1v-2" fill="none" stroke="currentColor" stroke-width="1" /></svg>
    </button>
  </div>

  <div class="ruler-row">
    <div class="corner header-views" style:width="{HEADER_W}px">
      <!-- What the track headers show. Only the mix controls exist so far; the
           other views arrive with recording (P4) and effects (P5). -->
      {#if ed.selectedClips.size > 1}<span class="selcount" aria-live="polite">{ed.selectedClips.size} clips selected</span>{/if}
      <div class="views" role="radiogroup" aria-label="Track header view">
        <button type="button" class="view" class:on={headerView === "mix"} role="radio" aria-checked={headerView === "mix"} title="Mix: volume, pan, mute, solo and arm" onclick={() => (headerView = "mix")}>
          <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4 2v12M8 2v12M12 2v12" stroke="currentColor" stroke-width="1" opacity="0.6" /><rect x="2.5" y="8" width="3" height="2" fill="currentColor" /><rect x="6.5" y="4.5" width="3" height="2" fill="currentColor" /><rect x="10.5" y="10" width="3" height="2" fill="currentColor" /></svg>
        </button>
        <button type="button" class="view" role="radio" aria-checked="false" disabled title="Inputs and Outputs: arrives with recording (P4)">
          <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M1.5 8h9M7.5 5l3 3-3 3" fill="none" stroke="currentColor" stroke-width="1.4" /><path d="M13 3v10" stroke="currentColor" stroke-width="1.4" /></svg>
        </button>
        <button type="button" class="view" role="radio" aria-checked="false" disabled title="Effects: arrives with the Voice Chain (P5)">
          <svg viewBox="0 0 16 16" aria-hidden="true"><text x="1.5" y="12" font-size="10" font-style="italic" font-family="serif" fill="currentColor">fx</text></svg>
        </button>
        <button type="button" class="view" class:on={headerView === "sends"} role="radio" aria-checked={headerView === "sends"} title="Sends: each track's output (the master or a bus) and its sends to buses" onclick={() => (headerView = "sends")}>
          <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M2 12h5V4h6M10 1.5L13 4l-3 2.5" fill="none" stroke="currentColor" stroke-width="1.3" /></svg>
        </button>
        <button type="button" class="view" role="radio" aria-checked="false" disabled title="EQ: arrives with the Voice Chain (P5)">
          <svg viewBox="0 0 16 16" aria-hidden="true"><rect x="2" y="7" width="2.5" height="7" fill="currentColor" /><rect x="6.75" y="3" width="2.5" height="11" fill="currentColor" /><rect x="11.5" y="9" width="2.5" height="5" fill="currentColor" /></svg>
        </button>
      </div>
      <div class="toggles" role="group" aria-label="Timeline options">
        <button type="button" class="view" disabled title="Metronome: arrives with recording (P4)" aria-label="Metronome">
          <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M6 2h4l3 12H3z" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round" /><path d="M8 11l3.5-6" stroke="currentColor" stroke-width="1.3" /></svg>
        </button>
        <button
          type="button"
          class="view"
          class:on={ed.followPlayhead}
          title="Follow Playhead: keep the playhead in view during playback"
          aria-label="Follow playhead"
          aria-pressed={ed.followPlayhead}
          onclick={() => editor.setFollowPlayhead(!ed.followPlayhead)}
        >
          <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M8 2v12" stroke="currentColor" stroke-width="1.3" /><path d="M1.5 8h4M3.5 6l2 2-2 2M14.5 8h-4M12.5 6l-2 2 2 2" fill="none" stroke="currentColor" stroke-width="1.2" /></svg>
        </button>
        <button
          type="button"
          class="view"
          class:on={ed.snap.on}
          title="Snapping: clips and selections snap to clips, the playhead and markers"
          aria-label="Snapping"
          aria-pressed={ed.snap.on}
          onclick={() => editor.setSnap({ on: !ed.snap.on })}
        >
          <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4 2.5v6a4 4 0 0 0 8 0v-6" fill="none" stroke="currentColor" stroke-width="2.4" /><path d="M2.8 3.5h2.4M10.8 3.5h2.4" stroke="var(--surface-2)" stroke-width="1.2" /></svg>
        </button>
        <button type="button" class="view" title={tip("Add Cue Marker", "edit.marker.cue")} aria-label="Add cue marker" disabled={!doc} onclick={() => run("edit.marker.cue")}>
          <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M5 2h6v6l-3 3-3-3z" fill="currentColor" /><path d="M8 11v3.5" stroke="currentColor" stroke-width="1.3" /></svg>
        </button>
      </div>
    </div>
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="ruler-wrap">
      <canvas class="ruler" bind:this={ruler} onpointerdown={onRulerPointer} onmousedown={(e) => e.button === 1 && e.preventDefault()} oncontextmenu={onRulerMenu} onwheel={onWheel}></canvas>
      {#if razor && ed.tool === "razor"}
        <div class="razor-mark" style:left="{razor.x}px" aria-hidden="true"></div>
      {/if}
    </div>
  </div>

  <div class="vwrap">
  {#if showStart}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="start"
      oncontextmenu={(e) => {
        e.preventDefault();
        menu = { x: e.clientX, y: e.clientY, items: emptyLaneItems() };
      }}
    >
      <p class="start-title">Nothing open</p>
      <p class="start-sub">Open a session or an audio file, or start a new session.</p>
      <div class="start-buttons">
        <button type="button" onclick={() => { startDismissed = true; run("file.new-session"); }}>New Session...</button>
        <button type="button" onclick={() => run("file.open")}>Open...</button>
        <button type="button" onclick={() => run("file.import-files")}>Import Files...</button>
        <button type="button" onclick={() => (startDismissed = true)}>Use an Empty Session</button>
      </div>
    </div>
  {/if}
  <div
    class="scroller"
    bind:this={scroller}
    onwheel={onWheel}
    onscroll={syncVbar}
    bind:clientHeight={scrollerH}
    oncontextmenu={(e) => {
      // Space below the last track gets the session menu too.
      e.preventDefault();
      menu = { x: e.clientX, y: e.clientY, items: emptyLaneItems() };
    }}
  >
    <div class="rows" bind:clientHeight={rowsH}>
      <div class="headers" style:width="{HEADER_W}px">
        {#each headerRows as row (row.id)}
          {#if row.kind === "track"}
          {@const track = row.track}
          {@const i = row.i}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            class="header"
            class:alt={i % 2 === 1}
            class:selected={ed.selectedTrack === track.id}
            style:--track-color={trackColor(track.color, i)}
            data-header={track.id}
            data-row={track.id}
            style:height="{shownHeight(track)}px"
            onpointerdown={(e) => onHeaderPointer(e, track)}
            oncontextmenu={(e) => {
              e.preventDefault();
              e.stopPropagation();
              menu = { x: e.clientX, y: e.clientY, items: trackItems(track) };
            }}
          >
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div class="grab" title="Drag to move the track up or down" onpointerdown={(e) => onHeaderPointer(e, track)}></div>
            <div class="line">
              <button
                type="button"
                class="fold"
                class:folded={!!track.collapsed}
                title={track.collapsed ? "Expand track" : "Collapse track to its name"}
                aria-label={track.collapsed ? "Expand track" : "Collapse track"}
                aria-expanded={!track.collapsed}
                onclick={() => setTrack(track, { collapsed: !track.collapsed }, track.collapsed ? "Expand Track" : "Collapse Track")}
              >
                <svg viewBox="0 0 10 10" aria-hidden="true"><path d="M2 3.5l3 3 3-3" fill="none" stroke="currentColor" stroke-width="1.4" /></svg>
              </button>
              {#if renaming === track.id}
                <!-- svelte-ignore a11y_autofocus -->
                <input
                  class="rename"
                  type="text"
                  value={track.name}
                  autofocus
                  onblur={(e) => commitRename(track, e.currentTarget.value)}
                  onkeydown={(e) => {
                    e.stopPropagation();
                    if (e.key === "Enter") commitRename(track, e.currentTarget.value);
                    if (e.key === "Escape") renaming = null;
                  }}
                />
              {:else}
                <!-- svelte-ignore a11y_no_static_element_interactions -->
                <span class="name" title="Double-click to rename" ondblclick={() => (renaming = track.id)}>{track.name}</span>
              {/if}
              <button type="button" class="ms" class:on={track.muted} title="Mute" aria-pressed={track.muted} onclick={() => setTrack(track, { muted: !track.muted }, track.muted ? "Unmute Track" : "Mute Track")}>M</button>
              <button type="button" class="ms solo" class:on={track.solo} title="Solo" aria-pressed={track.solo} onclick={() => setTrack(track, { solo: !track.solo }, track.solo ? "Unsolo Track" : "Solo Track")}>S</button>
              <button type="button" class="ms rec" class:on={track.record_armed} title="Arm for recording: Record records this track's input" aria-pressed={track.record_armed} onclick={() => setTrack(track, { record_armed: !track.record_armed }, "Arm Track")}>R</button>
              <button
                type="button"
                class="ms mon"
                class:on={!!track.monitor_input}
                title="Monitor Input: hear this track's input through the speakers while it is armed. Use headphones: speakers feed back into the mic."
                aria-pressed={!!track.monitor_input}
                onclick={() => setTrack(track, { monitor_input: !track.monitor_input }, "Monitor Input")}
              >
                <svg viewBox="0 0 16 16" aria-hidden="true"><circle cx="8" cy="8" r="1.6" fill="currentColor" /><path d="M5 5a4.2 4.2 0 0 0 0 6M11 5a4.2 4.2 0 0 1 0 6" fill="none" stroke="currentColor" stroke-width="1.3" /></svg>
              </button>
              <button type="button" class="ms mono short" class:on={!!track.mono} title="Mono: folds this track's left and right into one centered signal, after its effects and before its fader and pan. Part of the mix, so exports hear it too." aria-pressed={!!track.mono} onclick={() => setTrack(track, { mono: !track.mono }, track.mono ? "Track Stereo" : "Track Mono")}>MO</button>
            </div>
            {#if !track.collapsed && headerView === "sends" && doc}
              <div class="sendview">
                <SendList {doc} {session} {runner} {onError} kind="track" id={track.id} name={track.name} output={track.output ?? { type: "master" }} sends={track.sends ?? []} />
              </div>
            {/if}
            {#if !track.collapsed && headerView === "mix"}
            <div class="line">
              <Dial
                value={track.volume_db}
                min={-60}
                max={12}
                step={0.1}
                origin={0}
                label="Volume"
                icon="volume"
                format={(v) => `${formatDb(v)}`}
                onChange={(v) => setValue(track, "volume_db", v)}
              />
              <Dial value={track.pan} min={-100} max={100} step={1} origin={0} label="Pan" icon="pan" format={formatPan} onChange={(v) => setValue(track, "pan", v)} />
              <button
                type="button"
                class="ms pol"
                class:on={!!track.polarity_inverted}
                title="Invert Polarity: flips the track's waveform, to fix a mic wired backwards or two mics fighting each other"
                aria-pressed={!!track.polarity_inverted}
                onclick={() => setTrack(track, { polarity_inverted: !track.polarity_inverted }, "Invert Polarity")}
              >
                <svg viewBox="0 0 16 16" aria-hidden="true"><circle cx="8" cy="8" r="4.5" fill="none" stroke="currentColor" stroke-width="1.3" /><path d="M3.5 13.5l9-11" stroke="currentColor" stroke-width="1.3" /></svg>
              </button>
            </div>
            {/if}
            {#if !track.collapsed && headerView === "mix" && shownHeight(track) >= 76}
              <div class="line">
                <button type="button" class="pick" title="The input this track records from" onclick={(e) => void openInputMenu(e, track)}>
                  <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M1.5 8h9M7.5 5l3 3-3 3" fill="none" stroke="currentColor" stroke-width="1.4" /></svg>
                  <span>{inputLabel(track)}</span>
                  <span class="chev" aria-hidden="true">&rsaquo;</span>
                </button>
              </div>
            {/if}
            {#if !track.collapsed && headerView === "mix" && shownHeight(track) >= 94}
              <div class="line">
                <select class="auto" value="read" title="Automation mode. Read plays back the track's automation; Off, Write, Latch and Touch arrive with automation (P5).">
                  <option value="off" disabled>Off (arrives with automation)</option>
                  <option value="read">Read</option>
                  <option value="write" disabled>Write (arrives with automation)</option>
                  <option value="latch" disabled>Latch (arrives with automation)</option>
                  <option value="touch" disabled>Touch (arrives with automation)</option>
                </select>
              </div>
            {/if}
            {#if !track.collapsed}
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <div class="resize" title="Drag to change the track height" onpointerdown={(e) => startResize(e, track)}></div>
            {/if}
            <!-- Level meter: the input while armed, otherwise playback. -->
            <TrackMeter levels={meterOf(track)} armed={track.record_armed} onClear={() => transport.clearOver(track.id)} />
          </div>
          {:else}
          {@const b = row.bus}
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
          <div
            class="header bus"
            class:selected={ed.selectedTrack === b.id}
            style:--track-color={b.color ? trackColor(b.color, 0) : "var(--clip-3)"}
            style:height="{busHeight(b)}px"
            data-bus-header={b.id}
            data-row={b.id}
            onclick={() => editor.selectTrack(b.id)}
            oncontextmenu={(e) => {
              e.preventDefault();
              e.stopPropagation();
              menu = { x: e.clientX, y: e.clientY, items: busItems(b) };
            }}
          >
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div class="grab" title="Drag to move the bus up or down" onpointerdown={(e) => startRowDrag(e, b.id, "Move Bus")}></div>
            <div class="line">
              <span class="bus-tag" title="Bus track: sums the tracks routed or sent to it, then runs its own Voice Chain and fader">BUS</span>
              {#if renaming === b.id}
                <!-- svelte-ignore a11y_autofocus -->
                <input
                  class="rename"
                  type="text"
                  value={b.name}
                  autofocus
                  onblur={(e) => commitBusRename(b, e.currentTarget.value)}
                  onkeydown={(e) => {
                    e.stopPropagation();
                    if (e.key === "Enter") commitBusRename(b, e.currentTarget.value);
                    if (e.key === "Escape") renaming = null;
                  }}
                />
              {:else}
                <!-- svelte-ignore a11y_no_static_element_interactions -->
                <span class="name" title="Double-click to rename" ondblclick={() => (renaming = b.id)}>{b.name}</span>
              {/if}
              <button type="button" class="ms" class:on={b.muted} title="Mute" aria-pressed={b.muted} onclick={() => setBus(b, { muted: !b.muted }, b.muted ? "Unmute Bus" : "Mute Bus")}>M</button>
              <button type="button" class="ms solo" class:on={b.solo} title="Solo: plays this bus and everything feeding it" aria-pressed={b.solo} onclick={() => setBus(b, { solo: !b.solo }, b.solo ? "Unsolo Bus" : "Solo Bus")}>S</button>
            </div>
            {#if headerView === "mix"}
              <div class="line">
                <Dial value={b.volume_db} min={-60} max={12} step={0.1} origin={0} label="Volume" icon="volume" format={(v) => `${formatDb(v)}`} onChange={(v) => setBus(b, { volume_db: v }, "Bus Volume")} />
                <Dial value={b.pan} min={-100} max={100} step={1} origin={0} label="Pan" icon="pan" format={formatPan} onChange={(v) => setBus(b, { pan: v }, "Bus Pan")} />
                <button type="button" class="ms mono" class:on={!!b.mono} title="Mono: folds this bus's left and right into one centered signal, after its effects and before its fader and pan. Part of the mix, so exports hear it too." aria-pressed={!!b.mono} onclick={() => setBus(b, { mono: !b.mono }, b.mono ? "Bus Stereo" : "Bus Mono")}>MONO</button>
              </div>
            {:else if doc}
              <div class="sendview">
                <SendList {doc} {session} {runner} {onError} kind="bus" id={b.id} name={b.name} output={b.output} sends={b.sends} />
              </div>
            {/if}
            <TrackMeter levels={meterOf(b as unknown as TrackDoc)} armed={false} onClear={() => transport.clearOver(b.id)} />
          </div>
          {/if}
        {/each}
        {#if doc && !showStart}
          {@const mfx = doc.master?.effects?.slots.filter((x) => x.enabled).length ?? 0}
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
          <div
            class="header master"
            class:selected={ed.selectedTrack === MASTER_ID}
            style:height="72px"
            title="Master: every track and bus ends here. Click to show its Voice Chain."
            onclick={() => editor.selectTrack(MASTER_ID)}
            oncontextmenu={(e) => {
              e.preventDefault();
              e.stopPropagation();
            }}
          >
            <div class="line">
              <span class="bus-tag">OUT</span>
              <span class="name">Master</span>
              <button type="button" class="ms fx" class:on={mfx > 0} title={mfx > 0 ? `Master Voice Chain: ${mfx} effect${mfx === 1 ? "" : "s"}. Click to edit.` : "Master Voice Chain: empty. Click to add effects."} onclick={() => editor.selectTrack(MASTER_ID)}>fx</button>
            </div>
            <div class="line">
              <Dial value={doc.master?.volume_db ?? 0} min={-60} max={12} step={0.1} origin={0} label="Volume" icon="volume" format={(v) => `${formatDb(v)}`} onChange={setMasterVolume} />
              <button type="button" class="ms mono check" class:on={$monoCheck} title="Mono check: hear the whole mix folded to mono, to check how it holds up. Listening only: exports are not changed (to write a mono file, pick Mono in Export)." aria-pressed={$monoCheck} onclick={(e) => { e.stopPropagation(); void monoCheck.toggle().catch(onError); }}>MONO</button>
            </div>
            <TrackMeter levels={tp?.master ?? null} armed={false} onClear={() => transport.clearOver()} />
          </div>
        {/if}
      </div>
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="lanes" class:dropping bind:this={lanesEl} bind:clientWidth={lanesWidth} onpointerleave={() => (razor = null)}>
        {#if stretchHint}<div class="stretch-hint" aria-live="polite">Stretch {stretchHint}</div>{/if}
        {#if selBox}
          <div class="selBox" style:left="{selBox.x}px" style:top="{selBox.y}px" style:width="{selBox.w}px" style:height="{selBox.h}px"></div>
        {/if}
        {#if razor && ed.tool === "razor"}
          {@const row = razor.trackId ? lanesEl?.querySelector<HTMLElement>(`[data-track="${razor.trackId}"]`) : null}
          <div
            class="razor"
            style:left="{razor.x}px"
            style:top="{row ? row.offsetTop : 0}px"
            style:height={row ? `${row.offsetHeight}px` : "100%"}
          ></div>
        {/if}
        {#if doc}
          {#each laneList as row (row.id)}
            {#if row.kind === "track"}
            {@const track = row.track}
            {@const i = row.i}
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div
              class="lane"
              data-track={track.id}
              style:cursor={preview.length > 0 && hoverCursor === "grab" ? "grabbing" : hoverCursor}
              onmousedown={(e) => {
                if (e.button === 1) e.preventDefault();
              }}
              data-lane={track.id}
              onpointerdown={(e) => onLanePointer(e, track)}
              onpointermove={(e) => onLaneHover(e, track)}
              oncontextmenu={(e) => onLaneMenu(e, track)}
              ondblclick={(e) => onLaneDouble(e, track)}
            >
              <LaneCanvas
                track={resizing?.id === track.id || heightPreview !== 1 ? { ...(shownTracks?.[i] ?? track), height: shownHeight(track) } : (shownTracks?.[i] ?? track)}
                media={mediaMap}
                sessionRate={rate}
                {view}
                width={lanesWidth}
                {playhead}
                selectedClips={ed.selectedClips}
                trackSelected={ed.selectedTrack === track.id}
                index={i}
                {ampZoom}
                liveTake={live && live.waves[track.id] ? { at: live.at, rate: live.rate, wave: live.waves[track.id]!, tick: live.tick } : null}
                {peaks}
                {peakTick}
                showVolume={ed.showVolume}
                showPan={ed.showPan}
                showFx={ed.showFx}
                fxLanes={ed.fxLanes}
              />
            </div>
            {:else}
            {@const b = row.bus}
            <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
            <div
              class="lane bus-lane"
              class:selected={ed.selectedTrack === b.id}
              style:height="{busHeight(b)}px"
              onclick={() => editor.selectTrack(b.id)}
              oncontextmenu={(e) => {
                e.preventDefault();
                e.stopPropagation();
                menu = { x: e.clientX, y: e.clientY, items: busItems(b) };
              }}
            >
              <span>Bus {b.name}: no clips. Route tracks here with their output or a send (track header Sends view, the track's right-click menu, or the Mixer).</span>
            </div>
            {/if}
          {/each}
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
          <div class="lane bus-lane master-lane" class:selected={ed.selectedTrack === MASTER_ID} style:height="72px" onclick={() => editor.selectTrack(MASTER_ID)}>
            <span>Master: the whole mix, after every track and bus. Its fader and Voice Chain apply last.</span>
          </div>
          {#each shownMarkers as m (m.id)}
            {@const mx = xOf(m.position, view)}
            {#if mx >= 0 && mx <= lanesWidth}
              <div class="marker-line" style:left="{mx}px" style:border-color={markerCss(m)}></div>
            {/if}
          {/each}
          {#if doc.view.selection}
            {@const sx0 = xOf(doc.view.selection.start, view)}
            {@const sx1 = xOf(doc.view.selection.end, view)}
            {#if sx1 > 0 && sx0 < lanesWidth}
              <div class="range" style:left="{Math.max(0, sx0)}px" style:width="{Math.min(lanesWidth, sx1) - Math.max(0, sx0)}px"></div>
            {/if}
          {/if}
          {#if doc.tracks.length === 0}
            <p class="empty">Drag files here from the Files panel, Libraries, or your file manager. Each file becomes a clip on a new track.</p>
          {:else}
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div
              class="below"
              title="Drop files here to add a new track. Drag here to select a time range."
              onpointerdown={(e) => {
                if (e.button !== 0) return;
                if (ed.tool === "move") startBoxSelect(e);
                else startRange(e);
              }}
            ></div>
          {/if}
        {/if}
      </div>
    </div>
    {#if dragTrack}
      <div class="insert" style:top="{dragTrack.y}px"></div>
    {/if}
  </div>
  <!-- Track scrollbar: each track's color, the visible part as the thumb.
       Drag the thumb to scroll; drag its ends to make every track taller or shorter. -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="vbar" bind:clientHeight={vbarH} onpointerdown={onVbarPointer}>
    {#each shown as track, i (track.id)}
      <div class="vseg" style:height="{(shownHeight(track) / Math.max(1, rowsH)) * 100}%" style:background={trackColor(track.color, i)}></div>
    {/each}
    {#if rowsH > scrollerH + 1}
      <div class="vthumb" style:top="{(scrollTop / rowsH) * 100}%" style:height="{(scrollerH / rowsH) * 100}%">
        <span class="vend top"></span>
        <span class="vend bottom"></span>
      </div>
    {/if}
  </div>
  </div>

  <TransportBar
    {transport}
    {rate}
    {playhead}
    end={contentEnd}
    hasSelection={!!doc?.view.selection}
    {shortcutFor}
    zoom={onZoom}
    amplitude
    tracks
  />
</section>
{/if}

{#if envTip}
  <div class="env-tip" style:left="{envTip.x}px" style:top="{envTip.y}px">{envTip.text}</div>
{/if}

{#if colorFor}
  {@const target = colorFor}
  <ClipColorDialog
    title={target.kind === "bus" ? "Bus Color" : "Track Color"}
    noneLabel="Default Color"
    current={target.current}
    onDone={(c) => {
      colorFor = null;
      if (c === null) return;
      const color = c === "" ? null : c;
      const op = target.kind === "bus" ? busOp(doc, target.id, { color }) : trackOp(doc, target.id, { color });
      void session.edit(target.kind === "bus" ? "Bus Color" : "Track Color", op, runner).catch(onError);
    }}
  />
{/if}

{#if menu}
  <ContextMenu x={menu.x} y={menu.y} items={menu.items} onClose={() => (menu = null)} />
{/if}

<style>
  .env-tip {
    position: fixed;
    z-index: 4000;
    padding: 2px 6px;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    background: var(--surface-3);
    color: var(--value);
    font-family: var(--font-mono);
    font-size: 11px;
    pointer-events: none;
    white-space: nowrap;
  }

  .editor {
    display: grid;
    grid-template-rows: auto auto auto 1fr auto auto;
    grid-template-columns: minmax(0, 1fr);
    height: 100%;
    overflow: hidden;
    background: var(--surface-0);
  }

  .toolbar {
    display: flex;
    gap: 2px;
    align-items: center;
    padding: 3px 6px;
    border-bottom: 1px solid var(--border);
    background: var(--surface-1);
  }

  .tool {
    display: grid;
    place-items: center;
    min-width: 22px;
    height: 20px;
    padding: 0 4px;
    border: 1px solid transparent;
    border-radius: 2px;
    background: none;
    color: var(--text-dim);
    font: inherit;
    font-size: var(--font-size-small);
  }


  .tool:hover:not(:disabled) {
    border-color: var(--border-strong);
    color: var(--text);
  }

  .tool:disabled {
    opacity: 0.4;
  }


  .spacer {
    flex: 1;
  }


  .overview-row,
  .ruler-row {
    display: flex;
    min-width: 0;
  }

  .corner {
    flex: none;
    display: flex;
    align-items: center;
    padding: 0 8px;
    border-right: 1px solid var(--border);
    background: var(--surface-2);
    color: var(--text-faint);
    font-size: var(--font-size-small);
  }

  .overview-row {
    align-items: center;
    border-bottom: 1px solid var(--border);
  }

  .fit-all {
    display: grid;
    flex: none;
    place-items: center;
    width: 26px;
    height: 22px;
    margin: 0 2px;
    padding: 0;
    border: 1px solid transparent;
    border-radius: 3px;
    background: none;
    color: var(--text-dim);
  }

  .fit-all:hover {
    border-color: var(--border-strong);
    color: var(--text);
  }

  .fit-all svg {
    width: 15px;
    height: 15px;
  }

  .header-views {
    gap: 6px;
    padding: 0 4px;
  }

  .views {
    display: flex;
    gap: 1px;
  }

  .view {
    display: grid;
    place-items: center;
    width: 22px;
    height: 18px;
    padding: 0;
    border: 1px solid transparent;
    border-radius: 2px;
    background: none;
    color: var(--text-dim);
  }

  .view svg {
    width: 13px;
    height: 13px;
  }

  .view.on {
    border-color: var(--accent);
    color: var(--text);
  }

  .view:disabled {
    opacity: 0.35;
  }

  .toggles {
    display: flex;
    gap: 1px;
    margin-left: auto;
  }

  .toolbar .rate {
    padding: 0 4px;
    color: var(--text-faint);
    font-size: var(--font-size-small);
    white-space: nowrap;
  }

  .ruler-wrap {
    position: relative;
    display: flex;
    flex: 1;
    min-width: 0;
  }

  .razor-mark {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 0;
    border-left: 1px dashed var(--danger);
    pointer-events: none;
  }

  .razor-mark::before {
    content: "";
    position: absolute;
    bottom: 0;
    left: -5px;
    border: 5px solid transparent;
    border-bottom-color: var(--danger);
  }

  .ruler {
    flex: 1;
    min-width: 0;
    cursor: text;
    border-bottom: 1px solid var(--border);
  }

  .vwrap {
    display: flex;
    position: relative;
    min-height: 0;
  }

  .start {
    position: absolute;
    inset: 0;
    z-index: 20;
    display: grid;
    place-content: center;
    gap: 10px;
    justify-items: center;
    background: var(--surface-0);
    text-align: center;
  }

  .start-title {
    margin: 0;
    font-size: 16px;
    font-weight: 600;
  }

  .start-sub {
    margin: 0;
    color: var(--text-dim);
  }

  .start-buttons {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    justify-content: center;
  }

  .selcount {
    font-size: 11px;
    color: var(--text-dim, #a0a0a8);
    white-space: nowrap;
    padding: 0 6px;
  }

  .scroller {
    position: relative;
    flex: 1;
    min-width: 0;
    min-height: 0;
    overflow-x: hidden;
    overflow-y: auto;
    scrollbar-width: none;
  }

  .scroller::-webkit-scrollbar {
    display: none;
  }

  .vbar {
    position: relative;
    display: flex;
    flex: none;
    flex-direction: column;
    width: 12px;
    border-left: 1px solid var(--border);
    background: var(--surface-0);
    cursor: pointer;
    user-select: none;
  }

  .vseg {
    flex: none;
    margin: 0 3px;
    opacity: 0.55;
    border-bottom: 1px solid var(--surface-0);
  }

  .vthumb {
    position: absolute;
    right: 0;
    left: 0;
    box-sizing: border-box;
    border: 1px solid #b4b4bc;
    border-radius: 4px;
    background: rgba(255, 255, 255, 0.1);
    pointer-events: none;
  }

  .vend {
    position: absolute;
    left: 2px;
    right: 2px;
    height: 2px;
    border-top: 1px solid #b4b4bc;
  }

  .vend.top {
    top: 2px;
  }

  .vend.bottom {
    bottom: 2px;
  }

  .rows {
    display: flex;
    min-height: 100%;
  }

  .headers {
    flex: none;
    border-right: 3px solid var(--track-gap);
    background: var(--track-gap);
  }

  .header {
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: 3px 24px 3px 14px;
    border-left: 0;
    border-bottom: 3px solid var(--track-gap);
    box-sizing: border-box;
    position: relative;
    overflow: hidden;
    background: var(--header-a);
    cursor: grab;
    user-select: none;
  }

  .resize {
    position: absolute;
    right: 0;
    bottom: 0;
    left: 0;
    z-index: 2;
    height: 6px;
    cursor: ns-resize;
  }

  .resize:hover {
    background: color-mix(in srgb, var(--accent) 45%, transparent);
  }

  .header.alt {
    background: var(--header-b);
  }

  .header.selected {
    background: var(--lane-selected);
  }

  .header.selected .name {
    color: #fff;
    font-weight: 600;
  }

  .line {
    display: flex;
    gap: 3px;
    align-items: center;
    min-width: 0;
  }

  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .rename {
    flex: 1;
    min-width: 0;
    height: 18px;
    padding: 0 4px;
    border: 1px solid var(--accent);
    background: var(--surface-0);
    color: var(--text);
    font: inherit;
  }


  .ms {
    width: 20px;
    height: 18px;
    padding: 0;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    background: var(--surface-3);
    color: var(--text-dim);
    font: inherit;
    font-size: 10px;
    font-weight: 600;
  }

  .ms.on {
    background: var(--accent-dim);
    color: #fff;
  }

  .ms.solo.on {
    background: var(--clip-5);
    color: #111;
  }

  .ms.rec.on {
    background: var(--danger);
    color: #fff;
  }

  .ms svg {
    width: 12px;
    height: 12px;
    vertical-align: middle;
  }

  .ms.mon.on {
    background: var(--clip-1);
    color: #fff;
  }

  .ms.pol {
    margin-left: auto;
  }

  .ms.pol.on {
    background: var(--clip-4);
    color: #fff;
  }

  .fold {
    display: grid;
    flex: none;
    place-items: center;
    width: 14px;
    height: 16px;
    padding: 0;
    border: 0;
    background: none;
    color: var(--text-dim);
  }

  .fold svg {
    width: 10px;
    height: 10px;
    transition: transform 0.1s;
  }

  .fold.folded svg {
    transform: rotate(-90deg);
  }

  .fold:hover {
    color: var(--text);
  }

  .pick,
  .auto {
    flex: 1;
    min-width: 0;
    height: 18px;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    background: var(--surface-1);
    color: var(--text);
    font: inherit;
    font-size: var(--font-size-small);
  }

  .pick {
    display: flex;
    gap: 4px;
    align-items: center;
    padding: 0 4px;
    text-align: left;
  }

  .pick:hover {
    border-color: var(--accent);
  }

  .pick svg {
    flex: none;
    width: 12px;
    height: 12px;
    color: var(--text-dim);
  }

  .pick span:not(.chev) {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .chev {
    color: var(--text-dim);
  }

  .auto {
    height: 20px;
    padding: 0 2px;
    line-height: 18px;
  }

  .auto:disabled {
    color: var(--text-dim);
    opacity: 0.7;
  }




  .lanes {
    position: relative;
    flex: 1;
    min-width: 0;
  }

  .stretch-hint {
    position: absolute;
    top: 4px;
    right: 8px;
    z-index: 6;
    padding: 2px 6px;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    background: var(--surface-2);
    color: var(--text);
    font-size: 11px;
    pointer-events: none;
  }

  .selBox {
    position: absolute;
    z-index: 4;
    border: 1px dashed rgba(255, 255, 255, 0.85);
    background: rgba(255, 255, 255, 0.05);
    pointer-events: none;
  }

  .razor {
    position: absolute;
    z-index: 3;
    width: 0;
    border-left: 1px dashed var(--danger);
    pointer-events: none;
  }

  .lanes.dropping {
    outline: 2px dashed var(--accent);
    outline-offset: -2px;
    background: rgba(63, 169, 245, 0.06);
  }

  .lane {
    cursor: default;
  }

  .bus-lane {
    display: flex;
    align-items: center;
    padding: 0 12px;
    border-bottom: 3px solid var(--track-gap);
    box-sizing: border-box;
    background: repeating-linear-gradient(135deg, transparent 0 10px, color-mix(in srgb, var(--clip-3) 7%, transparent) 10px 20px);
    color: var(--text-faint);
    font-size: var(--font-size-small);
    overflow: hidden;
    white-space: nowrap;
  }

  .bus-lane.selected {
    background-color: color-mix(in srgb, var(--accent) 6%, transparent);
  }

  .grab {
    position: absolute;
    top: 0;
    bottom: 0;
    left: 0;
    z-index: 1;
    width: 10px;
    background: var(--track-color, var(--border-strong));
    background-image: repeating-linear-gradient(180deg, transparent 0 3px, rgba(0, 0, 0, 0.28) 3px 4px);
    cursor: grab;
  }

  .grab:hover {
    filter: brightness(1.25);
  }

  .grab:active {
    cursor: grabbing;
  }

  .header.master {
    border-left: 10px solid var(--accent);
    padding-left: 4px;
    background: color-mix(in srgb, var(--header-a) 80%, var(--accent) 20%);
    cursor: default;
  }

  .ms.mono {
    width: auto;
    padding: 0 3px;
    font-size: 8px;
    letter-spacing: 0.03em;
  }

  .ms.mono.short {
    width: 20px;
    padding: 0;
  }

  .ms.mono.check.on {
    background: var(--clip-5);
    color: #111;
  }

  .ms.fx {
    font-style: italic;
  }

  .master-lane {
    background: color-mix(in srgb, var(--accent) 5%, transparent);
  }

  .header.bus {
    background: color-mix(in srgb, var(--header-a) 82%, var(--clip-3) 18%);
    cursor: default;
  }

  .bus-tag {
    flex: none;
    padding: 0 3px;
    border-radius: 2px;
    background: var(--surface-3);
    color: var(--text-dim);
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.04em;
  }

  .sendview {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }

  .below {
    height: 60px;
  }

  .marker-line {
    position: absolute;
    top: 0;
    bottom: 0;
    border-left: 1px dashed;
    opacity: 0.6;
    pointer-events: none;
  }

  .range {
    position: absolute;
    top: 0;
    bottom: 0;
    background: rgba(255, 255, 255, 0.1);
    border-left: 1px solid rgba(255, 255, 255, 0.45);
    border-right: 1px solid rgba(255, 255, 255, 0.45);
    pointer-events: none;
  }

  .empty {
    margin: 0;
    padding: 40px 20px;
    color: var(--text-faint);
    text-align: center;
  }

  .insert {
    position: absolute;
    left: 0;
    right: 0;
    height: 2px;
    background: var(--accent);
    pointer-events: none;
  }
</style>
