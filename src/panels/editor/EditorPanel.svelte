<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  Editor panel: the multitrack view. In P1 it shows the session (tracks,
  clips, waveforms, ruler, playhead) and supports adding files, track
  management and zoom. Playback arrives in P2, clip editing in P3.
-->
<script lang="ts">
  import { settings, wheelAnchorX, zoomRatios } from "../../stores/settings";
  import { trackOp } from "../mixer/solo";
  import { busOp, MASTER_ID } from "../mixer/routing";
  import { onDestroy, tick as nextFrame, untrack } from "svelte";
  import type { ActionRegistry } from "../../app/actions";
  import { liveCopies, liveCopyOf } from "../../stores/wave";
  import ContextMenu, { type ContextItem } from "../../app/ContextMenu.svelte";
  import type { FileDropRouter } from "../../app/filedrop";
  import { editor, type EditorState } from "../../stores/editor";
  import type { PeakStore } from "../../stores/peaks";
  import type { CommandRunner } from "../../stores/media-pool";
  import { clipLength, isMidi, midiEnd, tempoOf, type BusDoc, type ClipChange, type ClipDoc, type MidiClipDoc, type SessionDoc, type SessionStore, type TempoDoc, type TrackChanges, type TrackDoc } from "../../stores/session";
  import MidiLane from "./MidiLane.svelte";
  import PianoRollDialog from "./PianoRollDialog.svelte";
  import { replaceClip, transferClips } from "./midi";
  import { transportAuditionClip, effectsCatalog, midiAudition, midiPlayNotes, previewStop, pluginsRegistry, type EffectInfo } from "../../app/core";
  import type { MenuPlugin } from "../voice-chain/chain";
  import type { EnvelopeDoc, Op } from "../../stores/session";
  import { pluginsVersion } from "../../stores/plugins";
  import { dragGain, gainText } from "./clip-gain";
  import { clipsAt, copyMoved, LockedError, moveClips, nearest, previewTracks, snapTargets, splitClips, withCrossfades,
    withLayers, withGroups } from "./clip-edit";
  import { shownTracks as listedTracks } from "./visibility";
  import { addPoint, envChange, envName, formatValue, fxLaneOf, fxParam, movePoint, pointsOf, rangeOf, snapValue, yToValue, type EnvKind } from "./envelope";
  import LaneCanvas from "./LaneCanvas.svelte";
  import ClipColorDialog from "../../app/ClipColorDialog.svelte";
  import { laneRows, moveRowOps } from "./rows";
  import { inputLevels, liveTakes, type LiveTakes } from "../../stores/record";
  import { probeMedia } from "../../app/core";
  import { laneWidths } from "../time-info/time-info";
  import WaveformView from "./WaveformView.svelte";
  import TransportBar, { type ZoomKind } from "./TransportBar.svelte";
  import TempoStrip from "./TempoStrip.svelte";
  import MetronomeRow from "./MetronomeRow.svelte";
  import AutoLane from "./AutoLane.svelte";
  import { laneHeight, autoRange, autoRest, newLane, nextTarget, putLane, shownLanes, type AutoTarget } from "./automation";
  import type { Readable } from "svelte/store";
  import type { WaveFile } from "../../stores/wave";
  import OverviewStrip from "./OverviewStrip.svelte";
  import { markerAt, markerCss } from "../markers/markers";
  import type { MeterLevel, TransportState, TransportStore } from "../../stores/transport";
  import { appearanceTick } from "../../stores/appearance";
  import { trackHeight, maxSppFor, ticksFor, setDisplayTempo, sampleAt, xOf, zoomAround, type Viewport, TRACK_GAP } from "./timeline";
  import { busHeight, HEADER_W, trackColor } from "./editor-layout";
  import { chipAt as hitChip, clipAt as hitClip, CURSORS, envBox, RAZOR_CURSOR, SNAP_PX, zoneAt as hitZone, type Gesture, type LaneGeometry } from "./lane-hit";
  import { chipMenuItems, clipZoneItems, emptyLaneItems, rulerMenuItems, type MenuCtx } from "./lane-menus";
  import { autoLaneItems, busItems as buildBusItems, midiClipItems as buildMidiClipItems, trackItems as buildTrackItems, type TrackMenuExtra } from "./track-menus";
  import { drawRuler } from "./ruler-draw";
  import { fadeDrag, gestureLabel, slipChange, stretchDrag, trimDrag } from "./clip-drag";
  import EditorToolbar from "./EditorToolbar.svelte";
  import HeaderViews from "./HeaderViews.svelte";
  import StartScreen from "./StartScreen.svelte";
  import TrackHeaders from "./TrackHeaders.svelte";
  import TrackScrollbar from "./TrackScrollbar.svelte";
  import LaneOverlays from "./LaneOverlays.svelte";
  import { ghostItems, type DropGhost } from "./drop-ghost";

  interface Props {
    session: SessionStore;
    runner: CommandRunner;
    peaks: PeakStore;
    transport: TransportStore;
    router: FileDropRouter;
    actions: ActionRegistry;
    /** Adds files as clips on a track (null makes a new track) at a session sample. */
    /** `asTracks`: several files go one per track, all from `at`. */
    onPlaceFiles: (paths: string[], trackId: string | null, at: number, asTracks: boolean) => void;
    shortcutFor?: (id: string) => string | undefined;
    onError?: (e: unknown) => void;
    /** Asks for one line of text. */
    prompt?: (title: string, label: string, value: string) => Promise<string | null>;
    /** The Waveform editor's document, shown instead of the lanes in waveform mode. */
    wave?: SessionStore;
    waveFile?: Readable<WaveFile | null>;
    waveTransport?: TransportStore;
    /** Waveform editor quick level: dB for the selection or whole file. */
    onWaveLevel?: (db: number) => void | Promise<void>;
    /** Brings the Audio FX panel forward. */
    onShowChain?: () => void;
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
    onShowChain = () => {},
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
  // The theme or colors changed: the ruler and overview draw again.
  $effect(() => appearanceTick.subscribe(() => untrack(() => peakTick++)));

  let tick = $state(0);
  $effect(() =>
    actions.changed.subscribe((n) => {
      tick = n;
    }),
  );
  const run = (id: string) => void actions.run(id);

  // Only the blank session the app opened with shows the start screen
  // (not six empty tracks); a new or opened session, or any track or
  // clip added, puts the tracks up.
  const showStart = $derived(doc != null && session.isScratch(doc));
  let toolbarH = $state(0);
  const shown = $derived(listedTracks(doc?.tracks ?? []));
  const headerTracks = $derived(showStart ? [] : shown);
  const listedIds = $derived(new Set(shown.map((t) => t.id)));
  /** Tracks with bus tracks placed among them, top to bottom. */
  const laneList = $derived(laneRows(doc, listedIds));
  const headerRows = $derived(showStart ? [] : laneList);
  /** Bus tracks, listed after the audio tracks. They hold no clips. */
  const buses = $derived(showStart ? [] : (doc?.buses ?? []));
  /** What the track headers show: the mix controls, or the output and sends. */
  /** What the headers show: mix controls (with input, output and sends as room allows) or the effects. */
  let headerView = $state<"mix" | "fx">("mix");
  const can = (id: string) => {
    void tick;
    return actions.isEnabled(id);
  };
  const tip = (label: string, id: string) => {
    const k = shortcutFor(id);
    return k ? `${label} (${k})` : label;
  };

  /** What the menu builders read and run. */
  const menuCtx: MenuCtx = {
    doc: () => doc,
    ed: () => ed,
    edit: (label, op) => session.edit(label, op, runner),
    onError: (e) => onError(e),
    run,
    can,
    has: (id) => actions.has(id),
    reason: (id) => actions.reason(id),
    isChecked: (id) => actions.isChecked(id),
    shortcutFor: (id) => shortcutFor(id),
  };

  let lanesWidth = $state(800);
  $effect(() => {
    const w = lanesWidth;
    laneWidths.update((l) => (l.multitrack === w ? l : { ...l, multitrack: w }));
  });
  let lanesEl = $state<HTMLElement | null>(null);
  let scroller = $state<HTMLElement | null>(null);
  let ruler = $state<HTMLCanvasElement | null>(null);
  let dropping = $state(false);
  let menu = $state<{ x: number; y: number; items: ContextItem[] } | null>(null);

  // A newly selected track or bus (a new one is always selected) scrolls into
  // view if it is off screen, so adding a track below the visible ones shows it.
  let lastSelected: string | null = null;
  $effect(() => {
    const id = ed.selectedTrack;
    void doc?.tracks.length;
    void doc?.buses?.length;
    if (!id || id === lastSelected) return;
    void nextFrame().then(() => {
      const row = scroller?.querySelector<HTMLElement>(`[data-row="${CSS.escape(id)}"]`);
      if (!row) return; // Not drawn yet: tried again when the rows change.
      lastSelected = id;
      row.scrollIntoView({ block: "nearest" });
    });
  });

  // MIDI tracks: the instruments to pick from, the piano roll, the tempo.
  let instruments = $state<{ effect: string; name: string; vendor: string; kind: string; hasWindow: boolean }[]>([]);
  /** For the headers' Effects view: every effect, and the effect plug-ins for the Add menu. */
  let fxCatalog = $state<EffectInfo[]>([]);
  let fxPlugins = $state<MenuPlugin[]>([]);
  $effect(() =>
    pluginsVersion.subscribe(() => {
      void pluginsRegistry()
        .then((r) => {
          // One per id, whatever the list holds, so the picker can never break.
          instruments = r.plugins.filter((p, i, all) => p.instrument && all.findIndex((q) => q.effect.toUpperCase() === p.effect.toUpperCase()) === i);
          fxPlugins = r.plugins.filter((p) => !p.instrument);
        })
        .catch(() => (instruments = []));
      void effectsCatalog()
        .then((c) => (fxCatalog = c))
        .catch(() => (fxCatalog = []));
    }),
  );

  let pianoRoll = $state<{ trackId: string; clipId: string } | null>(null);
  const tempo = $derived(doc ? tempoOf(doc) : { bpm: 120, beats_per_bar: 4 });

  /** The MIDI track a dragged MIDI clip is over, highlighted. */
  let midiDrop = $state<string | null>(null);

  /** MIDI clips dragged onto another MIDI track: moved (or copied) there as one undoable edit. */
  function transferMidiClips(from: TrackDoc, ids: string[], delta: number, toId: string, copy: boolean) {
    const to = doc?.tracks.find((t) => t.id === toId);
    if (!to || !isMidi(to)) return;
    const r = transferClips(from.midi_clips ?? [], to.midi_clips ?? [], new Set(ids), delta, copy, () => crypto.randomUUID());
    const n = ids.length;
    const what = n > 1 ? `${n} MIDI Clips` : "MIDI Clip";
    const ops: Op[] = [{ op: "set_midi_clips", track_id: to.id, clips: r.to }];
    if (!copy) ops.unshift({ op: "set_midi_clips", track_id: from.id, clips: r.from });
    void session
      .edit(`${copy ? "Copy" : "Move"} ${what} to ${to.name}`, { op: "batch", ops }, runner)
      .then(() => editor.selectClips(copy ? r.to.filter((c) => !(to.midi_clips ?? []).some((o) => o.id === c.id)).map((c) => c.id) : ids, to.id))
      .catch(onError);
  }

  function setMidiClips(track: TrackDoc, clips: MidiClipDoc[], label: string) {
    void session.edit(label, { op: "set_midi_clips", track_id: track.id, clips }, runner).catch(onError);
  }

  // The metronome's on/off (app settings, not the session): it shows its track while on.
  let metOn = $state(settings.current.metronome.on);
  $effect(() =>
    settings.subscribe((v) => {
      metOn = v.metronome.on;
    }),
  );

  /** Replaces the tempo map (tempo, time signature and their changes) as one undoable step. */
  function setTempoMap(t: TempoDoc, label: string) {
    void session.edit(label, { op: "set_tempo", tempo: { bpm: t.bpm, beats_per_bar: t.beats_per_bar, beat_unit: t.beat_unit ?? 4, changes: t.changes ?? [] } }, runner).catch(onError);
  }

  function midiClipItems(track: TrackDoc, clip: MidiClipDoc | null, beat: number): ContextItem[] {
    return buildMidiClipItems(menuCtx, track, clip, beat, {
      tempo: () => tempo,
      setMidiClips,
      openPianoRoll: (trackId, clipId) => (pianoRoll = { trackId, clipId }),
      prompt: (t, l, v) => prompt(t, l, v),
    });
  }
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
  /** While the playhead is being dragged on the ruler: where the pointer has it, shown at once. */
  let dragHead = $state<number | null>(null);
  const playhead = $derived(dragHead ?? (tp?.playing ? tp.position : (doc?.view.playhead ?? 0)));
  /** The start marker: shown while playing or paused, where playback began. */
  const startMark = $derived(tp && (tp.playing || tp.paused) ? tp.anchor : null);

  // Following the playhead while playing is Follow Playhead's job (below):
  // it lets go when you move the view yourself.
  // Files with unsaved Waveform editor edits draw (and measure) as their live copies.
  let liveFiles = $state<Record<string, { path: string; length: number }>>({});
  $effect(() => liveCopies.subscribe((v) => (liveFiles = v)));
  const mediaMap = $derived(
    new Map(
      (doc?.media ?? []).map((m) => {
        const c = liveCopyOf(liveFiles, m.path);
        return [m.id, c ? { ...m, path: c.path, length: c.length } : m];
      }),
    ),
  );
  const contentEnd = $derived.by(() => {
    if (!doc) return 0;
    let end = midiEnd(doc, rate);
    for (const t of doc.tracks)
      for (const c of t.clips) end = Math.max(end, c.start + clipLength(c, mediaMap.get(c.media_id), rate));
    return end;
  });

  // Follow Playhead: during playback, page the lanes when the playhead
  // nears the right edge or falls off either side. Scrolling or zooming
  // yourself while it plays (overview bar, wheel, scrollbar) lets go of it,
  // so you can look at another part of the session while it plays. It
  // follows again once you stop moving the view with the playhead in view
  // (or when playback starts, or the playhead is moved).
  // Bars and beats everywhere follow the session tempo.
  $effect(() => setDisplayTempo(doc?.tempo));
  let followHeld = false;
  let wasPlaying = false;
  /** When you last moved the view yourself (ms). */
  let userViewAt = 0;
  // Switching Follow Playhead on picks the following up again at once
  // (only on the switch: the editor state changes for many other reasons).
  let followWas = editor.current().followPlayhead;
  $effect(() => {
    const on = ed.followPlayhead;
    if (on && !followWas) followHeld = false;
    followWas = on;
  });
  $effect(() => {
    const playingNow = !!tp?.playing;
    if (playingNow && !wasPlaying) followHeld = false;
    wasPlaying = playingNow;
    if (!playingNow || !ed.followPlayhead || lanesWidth <= 0) return;
    const x = xOf(playhead, view);
    if (followHeld) {
      // Back in view and left alone for a moment: follow it again.
      if (x >= 0 && x <= lanesWidth && performance.now() - userViewAt > 400) followHeld = false;
      return;
    }
    if (x > lanesWidth * 0.92 || x < 0) setView({ scroll: playhead - lanesWidth * 0.08 * view.spp }, true);
  });

  /** Changes the view. `auto` is the follow's own paging; anything else is the user's. */
  function setView(v: Partial<{ spp: number; scroll: number; playhead: number }>, auto = false) {
    if (!doc) return;
    if (!auto && tp?.playing) {
      if (v.playhead !== undefined) followHeld = false;
      else if ((v.scroll !== undefined && v.scroll !== doc.view.scroll) || (v.spp !== undefined && v.spp !== doc.view.samples_per_pixel)) {
        followHeld = true;
        userViewAt = performance.now();
      }
    }
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
      onMove: (paths, at, alt) => showDropGhost(paths, at, alt),
      // Without Alt, several files go one per track; with Alt, one after another on one track.
      onDrop: (paths, at, alt) => {
        const { trackId, sample } = hitLanes(at.x, at.y);
        onPlaceFiles(paths, trackId, dropSample(sample), !alt);
      },
    });
  });

  /** Track and sample under a window point. */
  /**
   * While files hover over the lanes: where each would land if dropped
   * here (one after another from the pointer, on the track under it, or
   * on a new track below the last), with its name and length.
   */
  let dropGhost = $state<DropGhost | null>(null);
  const probed = new Map<string, Promise<{ name: string; len: number } | null>>();
  let ghostSeq = 0;
  function probeLength(path: string): Promise<{ name: string; len: number } | null> {
    let p = probed.get(path);
    if (!p) {
      p = probeMedia(path)
        .then((i) => ({ name: i.name, len: Math.round((i.length * rate) / Math.max(1, i.sampleRate)) }))
        .catch(() => null);
      probed.set(path, p);
    }
    return p;
  }
  function showDropGhost(paths: string[], at: { x: number; y: number } | null, alt: boolean) {
    const seq = ++ghostSeq;
    if (!at || paths.length === 0 || !lanesEl || !doc) {
      dropGhost = null;
      return;
    }
    const { trackId, sample } = hitLanes(at.x, at.y);
    const track = trackId ? doc.tracks.find((t) => t.id === trackId) : null;
    const lanesBox = lanesEl.getBoundingClientRect();
    const row = track && !isMidi(track) ? lanesEl.querySelector<HTMLElement>(`[data-track="${CSS.escape(track.id)}"]`) : null;
    const rows = [...lanesEl.querySelectorAll<HTMLElement>("[data-track]")];
    const lastBottom = rows.length ? rows[rows.length - 1]!.getBoundingClientRect().bottom - lanesBox.top : 0;
    const top = row ? row.getBoundingClientRect().top - lanesBox.top : lastBottom;
    const height = row ? row.offsetHeight - TRACK_GAP : 60;
    void Promise.all(paths.map(probeLength)).then((infos) => {
      if (seq !== ghostSeq) return;
      const items = ghostItems(infos, dropSample(sample), view, !alt);
      dropGhost = items.length ? { top, height, newTrack: !row, items } : null;
    });
  }

  /**
   * Where dropped files start: with snapping on, the usual targets, and the
   * session start from further away (a drop near it is meant for it).
   */
  function dropSample(sample: number): number {
    const p = Math.max(0, Math.round(sample));
    if (!doc || !ed.snap.on) return p;
    if (p <= SNAP_PX * 3 * view.spp) return 0;
    return nearest(p, targetsFor(doc, new Set()), SNAP_PX * view.spp) ?? p;
  }

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
    drawRuler(c, { width: lanesWidth, view, rate, timeFormat: ed.timeFormat, selection: doc?.view.selection, markers: shownMarkers, playhead, startMark });
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
      scrubRuler(e, left);
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

  /**
   * Click or drag on the ruler: the playhead goes to the mouse and follows
   * it. While playing, playback moves there when the button is let go.
   * The middle mouse button still slides the timeline.
   */
  function scrubRuler(e: PointerEvent, left: number) {
    e.preventDefault();
    const cur = session.current;
    if (cur?.view.selection) session.setView({ ...cur.view, selection: null });
    const at = (clientX: number) => Math.max(0, sampleAt(Math.max(0, Math.min(lanesWidth, clientX - left)), view));
    let last = at(e.clientX);
    // The line follows the pointer at once; the playhead itself moves once
    // a frame at most, and playback jumps there when the button is let go.
    dragHead = last;
    let queued = false;
    const settleSoon = () => {
      if (queued) return;
      queued = true;
      requestAnimationFrame(() => {
        queued = false;
        if (dragHead !== null && !(tp?.playing ?? false)) void transport.seek(last);
      });
    };
    settleSoon();
    document.body.style.cursor = "ew-resize";
    const move = (ev: PointerEvent) => {
      last = at(ev.clientX);
      dragHead = last;
      settleSoon();
    };
    const up = () => {
      window.removeEventListener("pointermove", move, true);
      window.removeEventListener("pointerup", up, true);
      document.body.style.cursor = "";
      void transport.seek(last).finally(() => (dragHead = null));
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
    const items = rulerMenuItems(menuCtx, hit, at, { seek: (s) => transport.seek(s), prompt: (t, l, v) => prompt(t, l, v), tempo: () => tempo, rate: () => rate, setTempoMap });
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
   * The mouse wheel. Over the ruler and the overview it zooms the
   * timeline; over the lanes it scrolls the tracks up and down; over the
   * headers it zooms the tracks (as the track scrollbar does). Alt + wheel makes the track under the pointer taller or shorter,
   * Alt + Shift + wheel scales the waveforms, Ctrl + wheel zooms the
   * timeline, Shift + wheel (or a sideways wheel) scrolls along it.
   */
  function onWheel(e: WheelEvent) {
    if (!lanesEl) return;
    const left = lanesEl.getBoundingClientRect().left;
    const overRuler = (e.target as HTMLElement | null)?.closest(".ruler-wrap") != null;
    const zoomTime = () => {
      e.preventDefault();
      const r = zoomRatios();
      const mouse = Math.max(0, e.clientX - left);
      // Over the ruler the zoom always goes toward the pointer.
      const at = wheelAnchorX(mouse, lanesEl!.clientWidth, xOf(playhead, view), overRuler);
      const z = zoomAround(view, e.deltaY > 0 ? r.zoomOut : r.zoomIn, at, maxSppFor(lanesWidth, contentEnd, rate));
      setView({ spp: z.spp, scroll: z.scroll });
    };
    if (e.clientX < left && !overRuler) {
      // Over the track headers the wheel zooms the tracks, as over the track
      // scrollbar: every open track taller or shorter. Alt+wheel resizes only
      // the one under the pointer. (Over a header's effect slots it scrolls.)
      if (e.altKey) {
        e.preventDefault();
        wheelResizeTracks(e);
      } else if (!e.shiftKey && Math.abs(e.deltaY) >= Math.abs(e.deltaX)) {
        onVbarWheel(e);
      }
      return;
    } else if (e.altKey && e.shiftKey) {
      e.preventDefault();
      onZoom(e.deltaY < 0 ? "amp-in" : "amp-out");
    } else if (e.altKey) {
      // Alt+wheel over the lanes makes the track under the pointer taller or shorter.
      e.preventDefault();
      wheelResizeTracks(e);
    } else if (e.shiftKey || Math.abs(e.deltaX) > Math.abs(e.deltaY)) {
      e.preventDefault();
      const d = e.shiftKey ? e.deltaY : e.deltaX;
      setView({ scroll: view.scroll + d * view.spp });
    } else if (overRuler || e.ctrlKey) {
      // Over the ruler (and the overview above it) the wheel zooms in time;
      // Ctrl+wheel zooms anywhere.
      zoomTime();
    }
    // Over the lanes and headers the wheel scrolls the tracks, which the
    // scroller does by itself. The track scrollbar zooms them (onVbarWheel).
  }

  /**
   * The wheel over the track scrollbar zooms vertically: every open track
   * the same height, taller or shorter, as dragging the thumb's ends does.
   * One undoable step once the wheel stops.
   */
  let vbarTimer: ReturnType<typeof setTimeout> | undefined;
  function onVbarWheel(e: WheelEvent) {
    e.preventDefault();
    e.stopPropagation();
    const open = shown.filter((t) => !t.collapsed);
    if (!open.length || !scroller) return;
    const cur = uniformPreview ?? open[0]!.height;
    const h = Math.max(40, Math.min(600, Math.round(cur * (e.deltaY < 0 ? 1.12 : 1 / 1.12))));
    if (h === cur) return;
    // Zoom toward the track the pointer is over on the bar: that spot of
    // the session keeps its place in the view.
    const sc = scroller;
    const bar = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const share = Math.min(1, Math.max(0, (e.clientY - bar.top) / Math.max(1, bar.height)));
    const spot = share * sc.scrollHeight;
    const offset = Math.min(sc.clientHeight, Math.max(0, spot - sc.scrollTop));
    uniformPreview = h;
    void nextFrame().then(() => {
      sc.scrollTop = share * sc.scrollHeight - offset;
    });
    clearTimeout(vbarTimer);
    vbarTimer = setTimeout(() => {
      const size = uniformPreview;
      if (size === null) return;
      const ops = open.filter((t) => t.height !== size).map((t) => ({ op: "set_track" as const, id: t.id, changes: { height: size } }));
      if (!ops.length) {
        uniformPreview = null;
        return;
      }
      void session
        .edit("Track Height", { op: "batch", ops }, runner)
        .then(() => (uniformPreview = null))
        .catch((err) => {
          uniformPreview = null;
          onError(err);
        });
    }, 350);
  }

  /** What hit-testing measures against: the view, the files and the session rate. */
  const geometry = (): LaneGeometry => ({ view, media: mediaMap, rate });
  const clipAt = (track: TrackDoc, x: number) => hitClip(geometry(), track, x);

  // Clip gestures. A drag shows its result live through `preview` and
  // becomes one undoable edit when the button is released.
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
  const shownTracks = $derived(doc && preview.length > 0 ? previewTracks(doc, preview) : null);

  const zoneAt = (track: TrackDoc, x: number, y: number) => hitZone(geometry(), ed, track, x, y);

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
    const x0 = e.clientX;
    const v = view;
    let change: ClipChange | null = null;
    const move = (ev: PointerEvent) => {
      change = slipChange(clip, m, rate, (ev.clientX - x0) * v.spp);
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
  const chipAt = (track: TrackDoc, x: number, y: number) => hitChip(geometry(), ed, track, x, y);

  function openChipMenu(e: PointerEvent, clip: ClipDoc) {
    menu = { x: e.clientX, y: e.clientY + 8, items: chipMenuItems(menuCtx, clip) };
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
    if (ed.snap.ruler) for (const tick of ticksFor(view, lanesWidth, rate, ed.timeFormat)) t.push(sampleAt(tick.x, view));
    return t;
  }

  /**
   * A press on a bus or master lane, or below the last track: the same
   * drag as on empty track space, so a selection can start anywhere (a
   * box with the Move tool, a time range otherwise) and sweep over tracks.
   */
  function startSelectFromLane(e: PointerEvent) {
    if (e.button !== 0) return;
    if (ed.tool === "move") startBoxSelect(e);
    else startRange(e);
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

  /**
   * A clip's level handle is heard at once while dragged: playback scales
   * that track over the clip as it plays, and the edit made on release
   * takes over. Where playback cannot (a mix with buses), each step is
   * an edit, heard a moment later, merged into one undo step.
   */
  let auditionFrame = 0;
  let auditionEnd: ReturnType<typeof setTimeout> | undefined;
  function auditionGain(trackId: string, clip: ClipDoc, db: number, end: number) {
    clearTimeout(auditionEnd);
    cancelAnimationFrame(auditionFrame);
    auditionFrame = requestAnimationFrame(() => {
      void transportAuditionClip(trackId, db, clip.start, end)
        .then((heard) => {
          if (!heard) void session.edit("Clip Level", { op: "set_clips", changes: [{ id: clip.id, gain_db: clip.gain_db + db }] }, runner).catch(onError);
        })
        .catch(() => {});
    });
  }
  /** After the edit: playback has picked up the new level where it renders, so the audition can go. */
  function endAudition(trackId: string) {
    cancelAnimationFrame(auditionFrame);
    clearTimeout(auditionEnd);
    auditionEnd = setTimeout(() => void transportAuditionClip(trackId, 0, 0, 0).catch(() => {}), 1500);
  }

  function startGesture(e: PointerEvent, d0: SessionDoc, track: TrackDoc, gesture: Gesture, clip: ClipDoc) {
    // A stretch handle works like Alt on the clip's edge.
    const handle = gesture === "stretch-left" || gesture === "stretch-right";
    const kind: Gesture = gesture === "stretch-left" ? "trim-left" : gesture === "stretch-right" ? "trim-right" : gesture;
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
    const stretching = (handle || e.altKey) && (kind === "trim-left" || kind === "trim-right") && !clip.looped && !clip.locked;
    let lastEv: PointerEvent | null = null;
    const move = (ev: PointerEvent) => {
      lastEv = ev;
      if (!moved && Math.abs(ev.clientX - startX) < 3 && Math.abs(ev.clientY - startY) < 3) return;
      moved = true;
      if (kind === "gain") {
        const db = dragGain(clip.gain_db, ev.clientX - startX, ev.clientY - startY, ev.shiftKey);
        changes = [{ id: clip.id, gain_db: db }];
        preview = changes;
        envTip = { x: ev.clientX, y: ev.clientY, text: db === 0 ? "0.0 dB" : gainText(db) };
        auditionGain(track.id, clip, db - clip.gain_db, end);
        return;
      }
      // The timeline may have scrolled under the drag (see the timer below), so add what it moved.
      const dx = (ev.clientX - startX) * v.spp + (view.scroll - v.scroll);
      const tol = ev.shiftKey || !ed.snap.on ? 0 : SNAP_PX * v.spp;
      if (kind === "move") {
        const over = hitLanes(ev.clientX, ev.clientY).trackId;
        if (over) row = d0.tracks.findIndex((t) => t.id === over);
        changes = moveClips(d0, ids, dx, row - startRow, targets, tol).changes;
        if (ed.autoCrossfade) changes = withCrossfades(d0, changes);
        changes = withLayers(d0, changes);
        // Alt held: the drop makes copies and leaves the originals.
        stretchHint = ev.altKey ? (ids.length > 1 ? `Copy ${ids.length} clips` : "Copy") : null;
      } else if (stretching) {
        const r = stretchDrag(clip, mediaMap.get(clip.media_id), rate, kind === "trim-left" ? "left" : "right", end, dx, targets, tol);
        changes = r.changes;
        stretchHint = r.hint;
      } else if (kind === "trim-left" || kind === "trim-right") {
        changes = trimDrag(d0, clip, kind === "trim-left" ? "left" : "right", dx, targets, tol, ids, mediaMap.get(clip.media_id), rate, end);
        if (ed.autoCrossfade) changes = withCrossfades(d0, changes);
      } else {
        // Across sets the fade's length; up and down bends its curve.
        const at = sampleAt(ev.clientX - lanesEl!.getBoundingClientRect().left, view);
        const r = fadeDrag(d0, clip, kind === "fade-in" ? "in" : "out", at, end, Math.abs(ev.clientX - startX) >= 3, ev.clientY - startY, rate);
        changes = r.changes;
        envTip = { x: ev.clientX, y: ev.clientY, text: r.tip };
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
      if (kind === "gain" || kind === "fade-in" || kind === "fade-out") envTip = null;
      if (!moved || changes.length === 0) return;
      if (kind === "move" && lastEv?.altKey) {
        // Alt+drag: copies where they were dropped; the originals stay.
        const copy = copyMoved(d0, ids, changes, () => crypto.randomUUID());
        void session
          .edit(copy.ids.length > 1 ? `Copy ${copy.ids.length} Clips` : "Copy Clip", copy.op, runner)
          .then(() => editor.selectClips(copy.ids))
          .catch(onError);
        return;
      }
      const label = gestureLabel(kind, stretching, ids.length, changes);
      void session
        .edit(label, { op: "set_clips", changes }, runner)
        .catch(onError)
        .finally(() => kind === "gain" && endAudition(track.id));
    };
    window.addEventListener("pointermove", move, true);
    window.addEventListener("pointerup", up, true);
  }

  function onLaneMenu(e: MouseEvent, track: TrackDoc) {
    e.preventDefault();
    e.stopPropagation();
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const z = zoneAt(track, e.clientX - r.left, e.clientY - r.top);
    if (!z) {
      // Empty space in a lane: the session-wide menu. Keep any chosen clips, but make this the chosen track.
      editor.selectClips(ed.selectedClips, track.id);
      menu = { x: e.clientX, y: e.clientY, items: emptyLaneItems(menuCtx) };
      return;
    }
    const items = clipZoneItems(menuCtx, z);
    // A fade handle's menu stands alone; elsewhere the track's entries follow.
    if (z.kind !== "fade-in" && z.kind !== "fade-out") items.push(...trackItems(track));
    menu = { x: e.clientX, y: e.clientY, items };
  }

  /** What the track and bus menus change in the editor itself. */
  const rowMenuExtra: TrackMenuExtra = {
    rename: (id) => (renaming = id),
    pickColor: (target) => (colorFor = target),
    toggleLanes,
    setTrack,
    showMix: () => (headerView = "mix"),
  };
  const trackItems = (track: TrackDoc) => buildTrackItems(menuCtx, track, rowMenuExtra);
  const busItems = (b: BusDoc) => buildBusItems(menuCtx, b, rowMenuExtra);

  function setTrack(track: TrackDoc, changes: TrackChanges, label: string) {
    void session.edit(label, trackOp(doc, track.id, changes as Record<string, unknown>), runner).catch(onError);
  }

  // Automation lanes under a track.
  const lanesHeight = (t: TrackDoc) => (t.show_automation && !t.collapsed ? shownLanes(t).reduce((h, l) => h + liveLaneH(l), 0) : 0);
  /** Lane heights while their edge is being dragged, shown at once. */
  let laneDrag = $state<{ id: string; height: number } | null>(null);
  const liveLaneH = (l: { id: string; height?: number }) => (laneDrag && laneDrag.id === l.id ? laneDrag.height : laneHeight(l));
  /** Dragging a lane's bottom edge sets its height (one undoable step). */
  function resizeLane(e: PointerEvent, track: TrackDoc, lane: EnvelopeDoc) {
    if (e.button !== 0 || !lane.id) return;
    e.preventDefault();
    e.stopPropagation();
    const y0 = e.clientY;
    const h0 = laneHeight(lane);
    const move = (ev: PointerEvent) => (laneDrag = { id: lane.id, height: Math.max(24, Math.min(400, Math.round(h0 + ev.clientY - y0))) });
    const up = () => {
      window.removeEventListener("pointermove", move, true);
      window.removeEventListener("pointerup", up, true);
      const d = laneDrag;
      laneDrag = null;
      if (d && d.height !== h0) setLanes(track, putLane(track, { ...lane, height: d.height }), "Lane Height");
    };
    window.addEventListener("pointermove", move, true);
    window.addEventListener("pointerup", up, true);
  }

  function setLanes(track: TrackDoc, envelopes: EnvelopeDoc[], label: string) {
    void session.edit(label, { op: "set_envelopes", owner: track.id, envelopes }, runner).catch(onError);
  }

  function toggleLanes(track: TrackDoc) {
    setTrack(track, { show_automation: !track.show_automation }, track.show_automation ? "Hide Automation Lanes" : "Show Automation Lanes");
  }

  function addLane(track: TrackDoc, target: AutoTarget | null = nextTarget(track, fxCatalog, !!doc?.space)) {
    if (!target) return;
    // The starting Volume lane is not saved until it has points; keep it when adding another.
    const base = (track.envelopes?.length ?? 0) === 0 ? [newLane({ type: "volume" })] : (track.envelopes ?? []);
    const envelopes = [...base, newLane(target)];
    const ops: Op[] = [{ op: "set_envelopes", owner: track.id, envelopes }];
    if (!track.show_automation) ops.push({ op: "set_track", id: track.id, changes: { show_automation: true } });
    void session.edit("Add Automation Lane", ops.length === 1 ? ops[0]! : { op: "batch", ops }, runner).catch(onError);
  }

  function laneItems(track: TrackDoc, lane: EnvelopeDoc, index: number): ContextItem[] {
    return autoLaneItems(menuCtx, track, lane, index, { fxCatalog: () => fxCatalog, playhead: () => playhead, setLanes, addLane: (t) => addLane(t), toggleLanes });
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

  // Length the overview strip scrolls over.
  const total = $derived(Math.max(contentEnd * 1.1, view.scroll + lanesWidth * view.spp, rate * 60));

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
      // Dragging an end of the thumb zooms vertically: the thumb covers the
      // share of the session that fits the view, and every (open) track gets
      // the same height so that it does.
      const open = shown.filter((t) => !t.collapsed);
      if (!open.length) return;
      const content = [...(scroller.querySelectorAll<HTMLElement>(".headers > *") ?? [])].reduce((sum, el) => sum + el.offsetHeight, 0);
      const fixed = Math.max(0, content - open.reduce((sum, t) => sum + shownHeight(t), 0));
      const span0 = Math.max(8, thumbBottom - thumbTop);
      const topShare = (thumbTop - bar.top) / bar.height;
      const bottomShare = (thumbBottom - bar.top) / bar.height;
      let h = open[0]!.height;
      const move = (ev: PointerEvent) => {
        const d = zoom === "bottom" ? ev.clientY - y : y - ev.clientY;
        const share = Math.min(1, Math.max(8, span0 + d) / bar.height);
        h = Math.max(40, Math.min(600, Math.round((scrollerH / share - fixed) / open.length)));
        uniformPreview = h;
      };
      const up = () => {
        window.removeEventListener("pointermove", move, true);
        window.removeEventListener("pointerup", up, true);
        uniformPreview = null;
        const ops = open.filter((t) => t.height !== h).map((t) => ({ op: "set_track" as const, id: t.id, changes: { height: h } }));
        if (!ops.length) return;
        const total = fixed + h * open.length;
        void session
          .edit("Track Height", { op: "batch", ops }, runner)
          .then(() => nextFrame())
          .then(() => {
            // Keep the end not dragged where it was.
            if (scroller) scroller.scrollTop = zoom === "bottom" ? topShare * total : bottomShare * total - scrollerH;
          })
          .catch(onError);
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
  /** Live scale for all track heights (vertical zoom keys). */
  let heightPreview = $state(1);
  /** The one height every open track takes while an end of the track scrollbar is dragged. */
  let uniformPreview = $state<number | null>(null);

  /** Double-click the track scrollbar: every open track the same height, all fitting the view. */
  function fitAllTracks() {
    const open = shown.filter((t) => !t.collapsed);
    if (!open.length || !scroller) return;
    const content = [...scroller.querySelectorAll<HTMLElement>(".headers > *")].reduce((sum, el) => sum + el.offsetHeight, 0);
    const fixed = Math.max(0, content - open.reduce((sum, t) => sum + shownHeight(t), 0));
    const h = Math.max(40, Math.min(600, Math.floor((scrollerH - fixed) / open.length)));
    const ops = open.filter((t) => t.height !== h).map((t) => ({ op: "set_track" as const, id: t.id, changes: { height: h } }));
    if (ops.length) void session.edit("Track Height", { op: "batch", ops }, runner).catch(onError);
  }

  /** A track being resized by its bottom edge: its live height. */
  let resizing = $state<{ id: string; h: number } | null>(null);
  const shownHeight = (t: TrackDoc) =>
    resizing?.id === t.id ? resizing.h : t.collapsed ? trackHeight(t) : uniformPreview !== null ? uniformPreview : heightPreview !== 1 ? Math.max(40, Math.min(600, Math.round(t.height * heightPreview))) : t.height;

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

  // View > Zoom In/Out Vertically (Alt+= and Alt+-) and Zoom Out Full (Shift+\\): every track taller or shorter, or everything in view.
  $effect(() => {
    const offs = [
      actions.register({ id: "view.zoom-in-vertical", run: () => resizeTracks(1.25), enabled: () => shown.length > 0 }),
      actions.register({ id: "view.zoom-out-vertical", run: () => resizeTracks(0.8), enabled: () => shown.length > 0 }),
      actions.register({ id: "view.zoom-out-full", run: () => zoomOutAll(), enabled: () => shown.length > 0 }),
    ];
    return () => offs.forEach((off) => off());
  });

  onDestroy(() => {
    menu = null;
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
{#if ed.mode === "waveform" && wave && waveTransport}
  <WaveformView {wave} file={wf} transport={waveTransport} {peaks} {actions} {shortcutFor} onLevel={onWaveLevel} />
{:else}
<section class="editor" aria-label="Multitrack" data-area="multitrack">
  <EditorToolbar {rate} hasDoc={!!doc} {run} {tip} bind:height={toolbarH} />

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
      <HeaderViews {ed} bind:headerView {metOn} hasDoc={!!doc} {run} {tip} />
    </div>
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="ruler-wrap">
      <canvas class="ruler" bind:this={ruler} onpointerdown={onRulerPointer} onmousedown={(e) => e.button === 1 && e.preventDefault()} oncontextmenu={onRulerMenu} onwheel={onWheel}></canvas>
      {#if razor && ed.tool === "razor"}
        <div class="razor-mark" style:left="{razor.x}px" aria-hidden="true"></div>
      {/if}
    </div>
  </div>
  {#if doc && metOn}
    <MetronomeRow {tempo} {view} width={lanesWidth} {rate} {playhead} headerW={HEADER_W} onTempo={setTempoMap} />
  {/if}
  {#if doc && ((tempo.changes?.length ?? 0) > 0 || ed.timeFormat === "bars")}
    <div class="ruler-row tempo-row">
      <div class="corner tempo-corner" style:width="{HEADER_W}px" title="Tempo and time signature changes. The tempo and time signature where the playhead is are next to the metronome, under the tracks.">Tempo</div>
      <TempoStrip {tempo} {view} width={lanesWidth} {rate} onChange={setTempoMap} onMenu={(e, items) => (menu = { x: e.clientX, y: e.clientY, items })} />
    </div>
  {/if}

  {#if showStart}
    <StartScreen top={toolbarH} onMenu={(e) => (menu = { x: e.clientX, y: e.clientY, items: emptyLaneItems(menuCtx) })} />
  {/if}
  <div class="vwrap">
  <div
    class="scroller"
    bind:this={scroller}
    onwheel={onWheel}
    onscroll={syncVbar}
    bind:clientHeight={scrollerH}
    oncontextmenu={(e) => {
      // Space below the last track gets the session menu too.
      e.preventDefault();
      menu = { x: e.clientX, y: e.clientY, items: emptyLaneItems(menuCtx) };
    }}
  >
    <div class="rows" bind:clientHeight={rowsH}>
      <TrackHeaders
        rows={headerRows}
        showMaster={!showStart}
        {doc}
        {session}
        {runner}
        {transport}
        {tp}
        {ed}
        {headerView}
        {renaming}
        {instruments}
        {fxCatalog}
        {fxPlugins}
        {onError}
        {run}
        {onShowChain}
        onMenu={(x, y, items) => (menu = { x, y, items })}
        onRename={(id) => (renaming = id)}
        trackMenu={trackItems}
        busMenu={busItems}
        {shownHeight}
        {liveLaneH}
        {meterOf}
        {setTrack}
        {setLanes}
        {toggleLanes}
        addLane={(t) => addLane(t)}
        {startRowDrag}
        {startResize}
        {resizeLane}
      />
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="lanes" class:dropping bind:this={lanesEl} bind:clientWidth={lanesWidth} onpointerleave={() => (razor = null)}>
        <LaneOverlays {stretchHint} {dropGhost} {dropping} {selBox} razor={ed.tool === "razor" ? razor : null} {lanesEl} />
        {#if doc}
          {#each laneList as row (row.id)}
            {#if row.kind === "track" && isMidi(row.track)}
            {@const track = row.track}
            <div class="lane" data-track={track.id} data-lane={track.id}>
              <MidiLane
                {track}
                {tempo}
                {rate}
                {view}
                width={lanesWidth}
                height={shownHeight(track)}
                color={trackColor(track.color, row.i)}
                selected={ed.selectedClips}
                onSelect={(ids) => editor.selectClips(ids, track.id)}
                stretchHandles={ed.stretchHandles}
                onSetClips={(clips, label) => setMidiClips(track, clips, label)}
                onOpen={(clipId) => (pianoRoll = { trackId: track.id, clipId })}
                onMenu={(e, clip, beat) => (menu = { x: e.clientX, y: e.clientY, items: midiClipItems(track, clip, beat) })}
                onTransfer={(ids, delta, to, copy) => transferMidiClips(track, ids, delta, to, copy)}
                onDragOver={(id) => (midiDrop = id)}
                dropTarget={midiDrop === track.id}
              />
            </div>
            {@render autoLanes(track, row.i)}
            {:else if row.kind === "track"}
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
                track={resizing?.id === track.id || heightPreview !== 1 || uniformPreview !== null ? { ...(shownTracks?.[i] ?? track), height: shownHeight(track) } : (shownTracks?.[i] ?? track)}
                media={mediaMap}
                sessionRate={rate}
                {view}
                width={lanesWidth}
                {playhead}
                {startMark}
                selectedClips={ed.selectedClips}
                trackSelected={ed.selectedTrack === track.id}
                index={i}
                {ampZoom}
                liveTake={live && live.waves[track.id] ? { at: live.starts?.[track.id] ?? live.at, rate: live.rate, wave: live.waves[track.id]!, tick: live.tick } : null}
                {peaks}
                {peakTick}
                showVolume={ed.showVolume}
                showPan={ed.showPan}
                showFx={ed.showFx}
                fxLanes={ed.fxLanes}
                stretchHandles={ed.stretchHandles}
              />
            </div>
            {@render autoLanes(track, i)}
            {:else}
            {@const b = row.bus}
            <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
            <div
              class="lane bus-lane"
              class:selected={ed.selectedTrack === b.id}
              style:height="{busHeight(b)}px"
              onpointerdown={startSelectFromLane}
              onclick={() => editor.selectTrack(b.id)}
              oncontextmenu={(e) => {
                e.preventDefault();
                e.stopPropagation();
                menu = { x: e.clientX, y: e.clientY, items: busItems(b) };
              }}
            >
              <span>Bus {b.name}: no clips. Route tracks here with their output or a send (in the track header when it is tall enough, the track's right-click menu, or the Mixer).</span>
            </div>
            {/if}
          {/each}
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
          <div class="lane bus-lane master-lane" class:selected={ed.selectedTrack === MASTER_ID} style:height="72px" onpointerdown={startSelectFromLane} onclick={() => editor.selectTrack(MASTER_ID)}>
            <span>Master: the whole mix, after every track and bus. Its fader and Audio FX apply last.</span>
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
              onpointerdown={startSelectFromLane}
            ></div>
          {/if}
        {/if}
      </div>
    </div>
    {#if dragTrack}
      <div class="insert" style:top="{dragTrack.y}px"></div>
    {/if}
  </div>
  <TrackScrollbar
    tracks={shown}
    segHeight={(t) => shownHeight(t) + lanesHeight(t)}
    {rowsH}
    {scrollTop}
    {scrollerH}
    onPointer={onVbarPointer}
    onDouble={fitAllTracks}
    onWheel={onVbarWheel}
  />
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

{#snippet autoLanes(track: TrackDoc, i: number)}
  {#if track.show_automation && !track.collapsed}
    {#each shownLanes(track) as lane (lane.id || "new")}
      <AutoLane
        {lane}
        range={autoRange(track, fxCatalog, lane.target)}
        rest={autoRest(track, fxCatalog, lane.target)}
        {view}
        width={lanesWidth}
        height={liveLaneH(lane)}
        color={trackColor(track.color, i)}
        {playhead}
        onPoints={(points, label) => setLanes(track, putLane(track, { ...lane, points }), label)}
        onMenu={(e, index) => (menu = { x: e.clientX, y: e.clientY, items: laneItems(track, lane, index) })}
      />
    {/each}
  {/if}
{/snippet}

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

{#if pianoRoll && doc}
  {@const pr = pianoRoll}
  {@const prTrack = doc.tracks.find((t) => t.id === pr.trackId)}
  {@const prClip = prTrack?.midi_clips?.find((c) => c.id === pr.clipId)}
  {#if prTrack && prClip}
    <PianoRollDialog
      clip={prClip}
      {tempo}
      color={prClip.color ? (prClip.color.startsWith("--") ? `var(${prClip.color})` : prClip.color) : trackColor(prTrack.color, doc.tracks.indexOf(prTrack))}
      instrument={prTrack.instrument ? (instruments.find((x) => x.effect === prTrack.instrument!.effect)?.name ?? null) : null}
      onApply={(c) => {
        const t = prTrack;
        setMidiClips(t, replaceClip(t.midi_clips ?? [], c), "Edit Notes");
        pianoRoll = null;
      }}
      onClose={() => (pianoRoll = null)}
      onAudition={(pitch, velocity, seconds, channel) => void midiAudition(prTrack.id, pitch, velocity / 127, seconds, channel).catch(() => {})}
      onPlay={(notes) => midiPlayNotes(prTrack.id, notes).catch(() => false)}
      onStop={() => void previewStop().catch(() => {})}
    />
  {/if}
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

  /* A column: the strips above and below keep their height and only the
     tracks stretch, however many strips (metronome, tempo) show. */
  .editor {
    position: relative;
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
    background: var(--surface-0);
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

  .ruler-wrap {
    position: relative;
    display: flex;
    flex: 1;
    min-width: 0;
  }

  .tempo-corner {
    height: 16px;
    border-bottom: 1px solid var(--border);
    color: var(--text-faint);
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
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

  /* Exactly as wide as the lanes (set in code), never stretched, so the
     playhead's triangle sits on its line whatever sits to the right. */
  .ruler {
    flex: none;
    min-width: 0;
    cursor: text;
    border-bottom: 1px solid var(--border);
  }

  .editor > :global(*) {
    flex: none;
    min-width: 0;
  }

  .editor > .vwrap {
    flex: 1 1 0;
  }

  .vwrap {
    display: flex;
    position: relative;
    min-height: 0;
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

  .rows {
    display: flex;
    min-height: 100%;
  }

  .lanes {
    position: relative;
    flex: 1;
    min-width: 0;
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

  .master-lane {
    background: color-mix(in srgb, var(--accent) 5%, transparent);
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
