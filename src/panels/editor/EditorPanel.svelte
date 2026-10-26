<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  Editor panel: the multitrack view. In P1 it shows the session (tracks,
  clips, waveforms, ruler, playhead) and supports adding files, track
  management and zoom. Playback arrives in P2, clip editing in P3.
-->
<script lang="ts">
  import MonoIcon from "../../app/MonoIcon.svelte";
  import { settings, wheelAnchorX, zoomRatios } from "../../stores/settings";
  import { trackOp } from "../mixer/solo";
  import { busChoices, busOp, MASTER_ID, newSend } from "../mixer/routing";
  import SendList from "../mixer/SendList.svelte";
  import { onDestroy, tick as nextFrame, untrack } from "svelte";
  import type { ActionRegistry } from "../../app/actions";
  import ContextMenu, { type ContextItem } from "../../app/ContextMenu.svelte";
  import type { FileDropRouter } from "../../app/filedrop";
  import { editor, type EditorState } from "../../stores/editor";
  import type { PeakStore } from "../../stores/peaks";
  import type { CommandRunner } from "../../stores/media-pool";
  import { clipLength, isMidi, midiEnd, stretchOf, tempoOf, type BusDoc, type ClipChange, type ClipDoc, type MidiClipDoc, type SessionDoc, type SessionStore, type TempoDoc, type TrackChanges, type TrackDoc } from "../../stores/session";
  import MidiLane from "./MidiLane.svelte";
  import PianoRollDialog from "./PianoRollDialog.svelte";
  import { duplicateClips, newClip, replaceClip, transferClips } from "./midi";
  import { clipReversePrepare, effectsCatalog, midiAudition, midiPlayNotes, previewStop, pluginOpenWindow, pluginsRegistry, type EffectInfo } from "../../app/core";
  import HeaderFx from "./HeaderFx.svelte";
  import { rowTarget } from "../voice-chain/slot-drag";
  import { pluginMenuGroups, type MenuPlugin } from "../voice-chain/chain";
  import { effectWindowRequest } from "../voice-chain/effect-window";
  import type { EffectSlotDoc, EnvelopeDoc, Op } from "../../stores/session";
  import { pluginsVersion } from "../../stores/plugins";
  import { stretchTo } from "./stretch";
  import { timeDisplayItems } from "./time-menu";
  import { dragGain, gainBox, gainText } from "./clip-gain";
  import { clipsAt, copyMoved, fadeClip, LockedError, MIN_CLIP, moveClips, nearest, previewTracks, snapTargets, splitClips, trimClip, withCrossfades,
    withLayers, withGroups } from "./clip-edit";
  import { shownTracks as listedTracks } from "./visibility";
  import { addPoint, chipBox, chipLabel, envChange, envName, formatValue, fxChoices, fxLaneOf, fxParam, levelAt, movePoint, pointsOf, rangeOf, restValue, snapValue, valueToY, yToValue, type EnvKind } from "./envelope";
  import LaneCanvas from "./LaneCanvas.svelte";
  import ClipColorDialog from "../../app/ClipColorDialog.svelte";
  import { laneRows, moveRowOps } from "./rows";
  import { monoCheck } from "../../stores/mono-check";
  import { chainTarget } from "../../stores/chain-target";
  import Dial from "./Dial.svelte";
  import TrackMeter from "./TrackMeter.svelte";
  import { inputLevels, liveTakes, type LiveTakes } from "../../stores/record";
  import { keepLast } from "../../app/settings/device-cache";
  import { audioInputs, openSoundSettings, probeMedia, type AudioInput } from "../../app/core";
  import { laneWidths } from "../time-info/time-info";
  import WaveformView from "./WaveformView.svelte";
  import TransportBar, { type ZoomKind } from "./TransportBar.svelte";
  import TempoStrip from "./TempoStrip.svelte";
  import MetronomeRow from "./MetronomeRow.svelte";
  import AutoLane from "./AutoLane.svelte";
  import { AUTO_H, laneHeight, autoAt, autoChoices, autoMode, autoName, autoRange, autoRest, dropLane, isOrphan, newLane, nextTarget, putLane, setMode, shownLanes, targetKey, type AutoTarget } from "./automation";
  import { setMetronomePrefs } from "../../stores/metronome";
  import { addChange, nearestBar, secondsToBeats } from "../../stores/tempo";
  import type { Readable } from "svelte/store";
  import type { WaveFile } from "../../stores/wave";
  import OverviewStrip from "./OverviewStrip.svelte";
  import { KIND_COLORS, markerAt, markerCss } from "../markers/markers";
  import { quiet, type MeterLevel, type TransportState, type TransportStore } from "../../stores/transport";
  import { channelNames } from "../space/space";
  import { appearanceTick } from "../../stores/appearance";
  import { defaultTrackColor, trackHeight, formatIn, maxSppFor, rulerTicks, ticksFor, setDisplayTempo, sampleAt, xOf, zoomAround, type Viewport, TRACK_GAP } from "./timeline";

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
  // The theme or colors changed: the ruler and overview draw again.
  $effect(() => appearanceTick.subscribe(() => untrack(() => peakTick++)));

  let tick = $state(0);
  $effect(() =>
    actions.changed.subscribe((n) => {
      tick = n;
    }),
  );
  const run = (id: string) => void actions.run(id);

  /** Nothing in the session yet and nothing chosen: show a start screen instead of six empty tracks. */
  let startDismissed = $state(false);
  // A new empty start (the last session closed) shows the start screen again.
  let startFor: string | null = null;
  $effect(() => {
    const id = doc?.id ?? null;
    if (id !== startFor) {
      startFor = id;
      startDismissed = false;
    }
  });
  // Only the blank session the app opened with shows the start screen; a
  // new or opened session, or any track or clip added, puts the tracks up.
  const showStart = $derived(!startDismissed && doc != null && session.isScratch(doc));
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
  const busHeight = (b: BusDoc) => Math.max(72, b.height);
  /** Header heights at which each extra row shows. */
  const ROW_INPUT = 76;
  const ROW_OUTPUT = 98;
  const ROW_AUTO = 120;
  const ROW_SENDS = 146;
  const can = (id: string) => {
    void tick;
    return actions.isEnabled(id);
  };
  const tip = (label: string, id: string) => {
    const k = shortcutFor(id);
    return k ? `${label} (${k})` : label;
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

  /** Opens an effect from a header: a plug-in in its own window, anything else in the Audio FX chain. */
  function openHeaderEffect(rowId: string, rowName: string, slot: EffectSlotDoc) {
    const info = fxCatalog.find((e) => e.id === slot.effect);
    if (info?.plugin && info.hasWindow) {
      void pluginOpenWindow(slot, `${info.name} - ${rowName}`).catch(onError);
      return;
    }
    editor.selectTrack(rowId);
    onShowChain();
    effectWindowRequest.set(slot.id);
  }
  function headerEdit(label: string, op: Op) {
    void session.edit(label, op, runner).catch(onError);
  }
  /** Where a track's or bus's sound goes: the master or a bus. */
  function setOutput(kind: "track" | "bus", id: string, value: string) {
    const o = value === MASTER_ID ? { type: "master" as const } : { type: "bus" as const, bus_id: value };
    headerEdit(kind === "bus" ? "Route Bus Output" : "Route Track Output", kind === "bus" ? busOp(doc, id, { output: o }) : trackOp(doc, id, { output: o }));
  }
  const outName = (o: { type: string; bus_id?: string } | undefined) => (o?.type === "bus" ? (doc?.buses?.find((b) => b.id === o.bus_id)?.name ?? "Missing bus") : "Master");
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

  function setInstrument(track: TrackDoc, effect: string) {
    if (!effect) {
      setTrack(track, { clear_instrument: true }, "Remove Instrument");
      return;
    }
    const slot = { id: crypto.randomUUID(), effect, enabled: true, params: {}, preset: null };
    setTrack(track, { instrument: slot }, "Set Instrument");
  }

  /** The instrument picker: by type, then by maker, as in the Add effect menu. */
  function openInstrumentMenu(e: MouseEvent, track: TrackDoc) {
    const el = e.currentTarget as HTMLElement;
    const r = (el.closest(".instrument") ?? el).getBoundingClientRect();
    const current = track.instrument?.effect ?? "";
    const pick = (effect: string) => () => setInstrument(track, effect);
    const list = (ps: MenuPlugin[]): ContextItem[] => ps.map((p) => ({ kind: "item", label: p.name, checked: p.effect === current, run: pick(p.effect) }));
    const { types, makers } = pluginMenuGroups(instruments);
    const items: ContextItem[] = instruments.length
      ? [
          ...types.map(([t, ps]): ContextItem => ({ kind: "sub", label: t, items: list(ps) })),
          { kind: "sep" },
          ...makers.map(([m, ps]): ContextItem => ({ kind: "sub", label: m, items: list(ps) })),
          { kind: "sep" },
          { kind: "item", label: "No Instrument", disabled: !track.instrument, run: pick("") },
        ]
      : [{ kind: "item", label: "No instruments found. Scan in Effects > Plug-in Manager.", disabled: true, run: () => {} }];
    menu = { x: r.left, y: r.bottom, items };
  }

  function openInstrument(track: TrackDoc) {
    if (!track.instrument) return;
    const name = instruments.find((i) => i.effect === track.instrument!.effect)?.name ?? "Instrument";
    void pluginOpenWindow(track.instrument, `${name}: ${track.name}`).catch(onError);
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
    const clips = track.midi_clips ?? [];
    if (!clip) {
      return [
        {
          kind: "item",
          label: "New MIDI Clip Here",
          run: () => {
            const id = crypto.randomUUID();
            setMidiClips(track, [...clips, newClip(id, beat, tempo)].sort((a, b) => a.start - b.start), "New MIDI Clip");
            editor.selectClips([id], track.id);
          },
        },
      ];
    }
    const ids = new Set(clips.filter((c) => ed.selectedClips.has(c.id)).map((c) => c.id));
    if (!ids.size) ids.add(clip.id);
    const many = ids.size > 1;
    return [
      { kind: "item", label: "Edit Notes...", run: () => (pianoRoll = { trackId: track.id, clipId: clip.id }) },
      { kind: "sep" },
      {
        kind: "item",
        label: many ? `Duplicate ${ids.size} Clips` : "Duplicate Clip",
        run: () => {
          const d = duplicateClips(clips, ids, () => crypto.randomUUID());
          setMidiClips(track, d.clips, many ? "Duplicate Clips" : "Duplicate Clip");
          editor.selectClips(d.added, track.id);
        },
      },
      {
        kind: "item",
        label: clip.muted ? "Unmute Clip" : "Mute Clip",
        run: () => setMidiClips(track, clips.map((c) => (ids.has(c.id) ? { ...c, muted: !clip.muted } : c)), clip.muted ? "Unmute Clip" : "Mute Clip"),
      },
      {
        kind: "item",
        label: "Rename Clip...",
        run: () =>
          void prompt("Rename Clip", "Name", clip.name).then((name) => {
            if (name && name.trim() && name.trim() !== clip.name) setMidiClips(track, replaceClip(clips, { ...clip, name: name.trim() }), "Rename Clip");
          }),
      },
      { kind: "sep" },
      {
        kind: "item",
        label: many ? `Delete ${ids.size} Clips` : "Delete Clip",
        shortcut: shortcutFor("edit.delete"),
        run: () => {
          setMidiClips(track, clips.filter((c) => !ids.has(c.id)), many ? "Delete Clips" : "Delete Clip");
          editor.selectClips([], track.id);
        },
      },
    ];
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

  // Following the playhead while playing is Follow Playhead's job (below):
  // it lets go when you move the view yourself.
  const mediaMap = $derived(new Map((doc?.media ?? []).map((m) => [m.id, m])));
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
      onMove: (paths, at) => showDropGhost(paths, at),
      onDrop: (paths, at) => {
        const { trackId, sample } = hitLanes(at.x, at.y);
        onPlaceFiles(paths, trackId, dropSample(sample));
      },
    });
  });

  /** Track and sample under a window point. */
  /**
   * While files hover over the lanes: where each would land if dropped
   * here (one after another from the pointer, on the track under it, or
   * on a new track below the last), with its name and length.
   */
  let dropGhost = $state<{ top: number; height: number; newTrack: boolean; items: { name: string; x: number; w: number }[] } | null>(null);
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
  function showDropGhost(paths: string[], at: { x: number; y: number } | null) {
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
      const start = dropSample(sample);
      let pos = start;
      const items = [];
      const side = settings.current.dropMany === "tracks" && infos.filter(Boolean).length > 1;
      for (const i of infos) {
        if (!i) continue;
        items.push({ name: i.name, x: xOf(pos, view), w: Math.max(4, i.len / view.spp) });
        // Side by side, the ghost shows the first file; the rest start at the same time below.
        if (side) break;
        pos += i.len;
      }
      const more = side ? infos.filter(Boolean).length - 1 : 0;
      if (more > 0) items[0]!.name = `${items[0]!.name} (+${more} on the tracks below)`;
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
    for (const t of ticksFor(view, w, rate, ed.timeFormat)) {
      const x = Math.round(t.x) + 0.5;
      g.beginPath();
      g.moveTo(x, t.label ? 8 : 15);
      g.lineTo(x, RULER_H);
      g.stroke();
      if (t.label) {
        const text = ed.timeFormat === "time" || ed.timeFormat === "bars" ? t.label : formatIn(sampleAt(t.x, view), rate, ed.timeFormat);
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
          {
            kind: "item",
            label: "Add Tempo Change at Nearest Bar",
            run: () => {
              const bar = Math.max(1, nearestBar(tempo, secondsToBeats(tempo, at / rate)));
              const r = addChange(tempo, bar, crypto.randomUUID());
              if (r.tempo !== tempo) setTempoMap(r.tempo, "Add Tempo Change");
            },
          },
          { kind: "sep" },
          ...timeDisplayItems(),
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
  type Gesture = "move" | "trim-left" | "trim-right" | "stretch-left" | "stretch-right" | "fade-in" | "fade-out" | "env-line" | "env-point" | "gain";
  /** Stretch handles: triangles this many pixels wide in a clip's top corners (see LaneCanvas). */
  const STRETCH_PX = 11;
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
    "stretch-left": "ew-resize",
    "stretch-right": "ew-resize",
    "fade-in": "ew-resize",
    "fade-out": "ew-resize",
    "env-line": "crosshair",
    "env-point": "move",
    gain: "ns-resize",
  };

  /** The y range the volume envelope uses inside a lane of `height` pixels. Matches LaneCanvas. */
  const envBox = (height: number) => ({ top: 1 + 14 + 2, height: height - TRACK_GAP - 2 - 14 - 4 });

  function zoneAt(track: TrackDoc, x: number, y: number): { kind: Gesture; clip: ClipDoc; index?: number; env?: EnvKind } | null {
    const clip = clipAt(track, x);
    if (!clip) return null;
    const x0 = xOf(clip.start, view);
    const x1 = xOf(clip.start + clipLength(clip, mediaMap.get(clip.media_id), rate), view);
    const w = x1 - x0;
    // Stretch handles fill the title bar's corners when they are shown.
    if (ed.stretchHandles && y <= TITLE_H + 1 && w > 2 * STRETCH_PX + 4 && !clip.looped && !clip.locked) {
      if (x - x0 <= STRETCH_PX) return { kind: "stretch-left", clip };
      if (x1 - x <= STRETCH_PX) return { kind: "stretch-right", clip };
    }
    // The level handle in the bottom-left corner (see clip-gain.ts).
    const gb = gainBox(x0, x1, trackHeight(track) - TRACK_GAP - 2, TITLE_H + 1);
    if (gb && !clip.locked && x >= gb.x - 2 && x <= gb.x + gb.w + 2 && y >= gb.y - 2 && y <= gb.y + gb.h + 2) return { kind: "gain", clip };
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
      : [{ kind: "item", label: "Add an effect to this clip's Audio FX chain first", disabled: true, run: () => {} }];
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
        const m = mediaMap.get(clip.media_id);
        let edge = (kind === "trim-left" ? clip.start : end) + dx;
        if (tol > 0) edge = nearest(edge, targets, tol) ?? edge;
        edge = Math.round(edge);
        const left = kind === "trim-left" ? Math.max(0, Math.min(edge, end - MIN_CLIP)) : clip.start;
        const right = kind === "trim-right" ? Math.max(clip.start + MIN_CLIP, edge) : end;
        const r = stretchTo(clip, m, rate, right - left, clip.stretch?.keep_pitch ?? true);
        changes = r ? [left !== clip.start ? { ...r.change, start: left } : r.change] : [];
        stretchHint = r ? `Stretch ${Math.round(r.ratio * 1000) / 10}%${r.limited ? " (limit)" : ""}` : null;
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
        // Across sets the fade's length; up and down bends its curve (-100 to 100).
        const which = kind === "fade-in" ? "in" : "out";
        const at = sampleAt(ev.clientX - lanesEl!.getBoundingClientRect().left, view);
        const was = which === "in" ? clip.fade_in : clip.fade_out;
        const across = Math.abs(ev.clientX - startX) >= 3;
        const bend = Math.max(-1, Math.min(1, Math.round(((was.curve ?? 0) - (ev.clientY - startY) / 80) * 100) / 100));
        const c = across ? fadeClip(d0, clip, which, which === "in" ? at - clip.start : end - at) : null;
        const fade = { ...was, ...(c ? (which === "in" ? c.fade_in : c.fade_out) : {}), curve: Math.abs(bend) < 0.02 ? 0 : bend };
        changes = fade.length > 0 ? [which === "in" ? { id: clip.id, fade_in: fade } : { id: clip.id, fade_out: fade }] : [];
        const skew = Math.round(fade.curve * 100);
        envTip = { x: ev.clientX, y: ev.clientY, text: `${which === "in" ? "Fade in" : "Fade out"} ${(fade.length / rate).toFixed(2)} s, ${fade.shape === "cosine" ? "curve" : "straight"} ${skew > 0 ? "+" : ""}${skew}` };
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
      const label = kind === "gain"
        ? "Clip Level"
        : stretching
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

  /** Reverse and Invert for the selected clips: toggles that leave the files alone. */
  function clipFlipItems(): ContextItem[] {
    const found = (doc?.tracks ?? []).flatMap((t) => t.clips).filter((c) => ed.selectedClips.has(c.id));
    if (!found.length) return [];
    const allRev = found.every((c) => c.reversed);
    const allInv = found.every((c) => c.inverted);
    const reverse = async () => {
      const on = !allRev;
      if (on) {
        const paths = [...new Set(found.map((c) => doc?.media.find((m) => m.id === c.media_id)?.path).filter((p): p is string => !!p))];
        await Promise.all(paths.map((p) => clipReversePrepare(p)));
      }
      // The region flips with it: positions count from the file's end while reversed.
      const flip = (c: ClipDoc) => {
        const len = doc?.media.find((m) => m.id === c.media_id)?.length ?? c.source_out;
        return { source_in: Math.max(0, len - c.source_out), source_out: Math.max(0, len - c.source_in) };
      };
      await session.edit(on ? "Reverse Clip" : "Play Clip Forwards", { op: "set_clips", changes: found.filter((c) => !!c.reversed !== on).map((c) => ({ id: c.id, reversed: on, ...flip(c) })) }, runner);
    };
    return [
      { kind: "sep" },
      { kind: "item", label: "Reverse", checked: allRev, run: () => void reverse().catch(onError) },
      { kind: "item", label: "Invert Polarity", checked: allInv, run: () => void session.edit(allInv ? "Polarity Back" : "Invert Polarity", { op: "set_clips", changes: found.map((c) => ({ id: c.id, inverted: !allInv })) }, runner).catch(onError) },
      { kind: "sep" },
    ];
  }

  /** Fade shapes for a fade handle's menu: straight or curved, each bent by dragging the handle up or down. */
  const FADE_SHAPES = [
    ["linear", "Straight"],
    ["cosine", "Curve"],
  ] as const;

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
    if (z.kind === "fade-in" || z.kind === "fade-out") {
      const which = z.kind === "fade-in" ? "fade_in" : "fade_out";
      const word = z.kind === "fade-in" ? "Fade In" : "Fade Out";
      // The clip under the pointer, and the other selected clips with it.
      const ids = ed.selectedClips.has(z.clip.id) ? [...ed.selectedClips] : [z.clip.id];
      const clips = (doc?.tracks ?? []).flatMap((t) => t.clips).filter((c) => ids.includes(c.id));
      const current = z.clip[which].shape;
      const setShape = (shape: string, label: string) =>
        void session.edit(`${word} Shape: ${label}`, { op: "set_clips", changes: clips.map((c) => ({ id: c.id, [which]: { ...c[which], shape } })) }, runner).catch(onError);
      for (const [shape, label] of FADE_SHAPES) items.push({ kind: "item", label, checked: current === shape || (shape === "linear" && current !== "cosine"), run: () => setShape(shape, label) });
      const bent = clips.some((c) => (c[which].curve ?? 0) !== 0);
      items.push({ kind: "item", label: `Skew: ${Math.round((z.clip[which].curve ?? 0) * 100)} (drag the handle up or down)`, disabled: true, run: () => {} });
      items.push({ kind: "item", label: "Reset Skew", disabled: !bent, run: () => void session.edit(`${word} Skew`, { op: "set_clips", changes: clips.map((c) => ({ id: c.id, [which]: { ...c[which], curve: 0 } })) }, runner).catch(onError) });
      items.push(
        { kind: "sep" },
        { kind: "item", label: `Remove ${word}`, disabled: clips.every((c) => c[which].length === 0), run: () => void session.edit(`Remove ${word}`, { op: "set_clips", changes: clips.map((c) => ({ id: c.id, [which]: { ...c[which], length: 0 } })) }, runner).catch(onError) },
      );
      menu = { x: e.clientX, y: e.clientY, items };
      return;
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
        ...clipFlipItems(),
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
        act("Stretch and Pitch...", "clip.properties"),
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
        act("Add MIDI Track", "multitrack.add.midi"),
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
        act("Selected Track", "multitrack.bounce.track"),
        act("Time Selection", "multitrack.bounce.selection"),
        act("Selected Clips in Time Selection", "multitrack.bounce.clips-selection"),
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
            // Tall enough to show the send in the header.
            const ops: Op[] = [{ op: "set_sends", owner: track.id, sends: [...sends, newSend(b.id, crypto.randomUUID())] }];
            if (!track.collapsed && track.height < ROW_SENDS + 24) ops.push({ op: "set_track", id: track.id, changes: { height: ROW_SENDS + 24 } } as Op);
            void session.edit("Add Send", ops.length === 1 ? ops[0]! : { op: "batch", ops }, runner).catch(onError);
            headerView = "mix";
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
      { kind: "item", label: track.show_automation ? "Hide Automation Lanes" : "Show Automation Lanes", run: () => toggleLanes(track) },
      { kind: "item", label: "Track Color...", run: () => (colorFor = { kind: "track", id: track.id, current: track.color ?? null }) },
      { kind: "item", label: "Duplicate Track", run: () => { editor.selectTrack(track.id); run("multitrack.duplicate-track"); } },
      { kind: "item", label: "Bounce Track to New Track", disabled: track.clips.length === 0 && (track.midi_clips ?? []).length === 0, run: () => { editor.selectTrack(track.id); run("multitrack.bounce.track"); } },
      { kind: "item", label: "Delete Track", run: () => { editor.selectTrack(track.id); run("multitrack.delete-track"); } },
      { kind: "sep" },
      { kind: "item", label: "Add Mono Track", run: () => { editor.selectTrack(track.id); run("multitrack.add.mono"); } },
      { kind: "item", label: "Add Stereo Track", run: () => { editor.selectTrack(track.id); run("multitrack.add.stereo"); } },
      { kind: "item", label: "Add MIDI Track", run: () => { editor.selectTrack(track.id); run("multitrack.add.midi"); } },
      { kind: "item", label: "Add Bus Track", run: () => run("multitrack.add.bus") },
      { kind: "sep" },
      ...routingItems(track),
    ];
  }

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

  /** Points a lane at another setting. Its points are kept only when the value range is the same kind. */
  function pickTarget(track: TrackDoc, lane: EnvelopeDoc, key: string) {
    const c = autoChoices(track, fxCatalog, !!doc?.space).find((x) => targetKey(x.target) === key);
    if (!c || targetKey(c.target) === targetKey(lane.target)) return;
    const same = c.target.type === lane.target.type && c.target.type !== "effect_param";
    setLanes(track, putLane(track, { ...lane, target: c.target, points: same ? lane.points : [] }), `Automate ${c.label}`);
  }

  function laneItems(track: TrackDoc, lane: EnvelopeDoc, index: number): ContextItem[] {
    const name = autoName(track, fxCatalog, lane.target);
    const at = (v: number) => putLane(track, { ...lane, points: lane.points.filter((_, k) => k !== v) });
    return [
      { kind: "item", label: "Delete Point", disabled: index < 0, run: () => setLanes(track, at(index), "Delete Automation Point") },
      {
        kind: "item",
        label: "Add Point at Playhead",
        run: () => {
          const v = lane.points.length ? autoAt(lane.points, playhead, autoRest(track, fxCatalog, lane.target)) : autoRest(track, fxCatalog, lane.target);
          const pts = lane.points.filter((p) => p.position !== Math.round(playhead)).concat({ position: Math.max(0, Math.round(playhead)), value: v, curve: "linear" as const });
          pts.sort((a, b) => a.position - b.position);
          setLanes(track, putLane(track, { ...lane, points: pts }), "Add Automation Point");
        },
      },
      {
        kind: "item",
        label: "Delete Points in Time Selection",
        disabled: !doc?.view.selection || !lane.points.some((p) => p.position >= doc!.view.selection!.start && p.position <= doc!.view.selection!.end),
        run: () => {
          const sel = doc?.view.selection;
          if (!sel) return;
          setLanes(track, putLane(track, { ...lane, points: lane.points.filter((p) => p.position < sel.start || p.position > sel.end) }), "Delete Automation Points");
        },
      },
      { kind: "item", label: `Clear ${name} Lane`, disabled: lane.points.length === 0, run: () => setLanes(track, putLane(track, { ...lane, points: [] }), "Clear Automation Lane") },
      { kind: "sep" },
      { kind: "item", label: lane.enabled ? "Turn Lane Off" : "Turn Lane On", disabled: !lane.id, run: () => setLanes(track, putLane(track, { ...lane, enabled: !lane.enabled }), lane.enabled ? "Automation Lane Off" : "Automation Lane On") },
      { kind: "item", label: "Add Lane", disabled: !nextTarget(track, fxCatalog, !!doc?.space), run: () => addLane(track) },
      { kind: "item", label: "Remove Lane", disabled: !lane.id, run: () => setLanes(track, dropLane(track, lane.id), "Remove Automation Lane") },
      { kind: "item", label: "Hide Automation Lanes", run: () => toggleLanes(track) },
    ];
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
    // Controls work as controls; only the rest of the header picks up and drags the track.
    if (e.button !== 0 || (e.target as HTMLElement).closest("button, input, select, textarea, label, a")) return;
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

  /** Over the track scrollbar: a resize cursor near the thumb's ends. */
  function onVbarHover(e: PointerEvent) {
    const el = e.currentTarget as HTMLElement;
    const bar = el.getBoundingClientRect();
    const scale = rowsH / Math.max(1, bar.height);
    const top = bar.top + scrollTop / scale;
    const bottom = top + scrollerH / scale;
    const near = Math.abs(e.clientY - top) < 7 || Math.abs(e.clientY - bottom) < 7;
    el.style.cursor = near ? "ns-resize" : "pointer";
  }

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
    <button type="button" class="tool text" title={tip("Add MIDI Track", "multitrack.add.midi")} onclick={() => run("multitrack.add.midi")}>+ MIDI</button>
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
        <button type="button" class="view" class:on={headerView === "mix"} role="radio" aria-checked={headerView === "mix"} title="Mix: volume, pan, mute, solo and arm, then input, output and sends as the track gets taller" onclick={() => (headerView = "mix")}>
          <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4 2v12M8 2v12M12 2v12" stroke="currentColor" stroke-width="1" opacity="0.6" /><rect x="2.5" y="8" width="3" height="2" fill="currentColor" /><rect x="6.5" y="4.5" width="3" height="2" fill="currentColor" /><rect x="10.5" y="10" width="3" height="2" fill="currentColor" /></svg>
        </button>
        <button type="button" class="view" role="radio" aria-checked="false" disabled title="Inputs and Outputs: arrives with recording (P4)">
          <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M1.5 8h9M7.5 5l3 3-3 3" fill="none" stroke="currentColor" stroke-width="1.4" /><path d="M13 3v10" stroke="currentColor" stroke-width="1.4" /></svg>
        </button>
        <button type="button" class="view" class:on={headerView === "fx"} role="radio" aria-checked={headerView === "fx"} title="Effects: each track's Audio FX chain in its header. Drag an effect to move it, or onto another track to copy it." onclick={() => (headerView = "fx")}>
          <svg viewBox="0 0 16 16" aria-hidden="true"><text x="1.5" y="12" font-size="10" font-style="italic" font-family="serif" fill="currentColor">fx</text></svg>
        </button>
        <button type="button" class="view" role="radio" aria-checked="false" disabled title="EQ: arrives with the Audio FX chain (P5)">
          <svg viewBox="0 0 16 16" aria-hidden="true"><rect x="2" y="7" width="2.5" height="7" fill="currentColor" /><rect x="6.75" y="3" width="2.5" height="11" fill="currentColor" /><rect x="11.5" y="9" width="2.5" height="5" fill="currentColor" /></svg>
        </button>
      </div>
      <div class="toggles" role="group" aria-label="Timeline options">
        <button
          type="button"
          class="view"
          class:on={metOn}
          title="Metronome: a click on every beat while playing and recording, on its own track at the top (never in an export)"
          aria-label="Metronome"
          aria-pressed={metOn}
          onclick={() => setMetronomePrefs({ on: !metOn })}
        >
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
          class:on={ed.stretchHandles}
          title="Stretch Handles: each clip shows a triangle in its top corners; drag one to stretch the clip (no Alt needed)"
          aria-label="Stretch handles"
          aria-pressed={ed.stretchHandles}
          onclick={() => editor.setStretchHandles(!ed.stretchHandles)}
        >
          <svg viewBox="0 0 16 16" aria-hidden="true"><circle cx="8" cy="9" r="5" fill="none" stroke="currentColor" stroke-width="1.3" /><path d="M8 9V6.2M8 9l2 1.4M6.5 2h3" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" /></svg>
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
  {#if doc && metOn}
    <MetronomeRow {tempo} {view} width={lanesWidth} {rate} {playhead} headerW={HEADER_W} onTempo={setTempoMap} />
  {/if}
  {#if doc && ((tempo.changes?.length ?? 0) > 0 || ed.timeFormat === "bars")}
    <div class="ruler-row tempo-row">
      <div class="corner tempo-corner" style:width="{HEADER_W}px" title="Tempo and time signature changes. The tempo and time signature where the playhead is are next to the metronome, under the tracks.">Tempo</div>
      <TempoStrip {tempo} {view} width={lanesWidth} {rate} onChange={setTempoMap} onMenu={(e, items) => (menu = { x: e.clientX, y: e.clientY, items })} />
    </div>
  {/if}

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
            class:fx={$chainTarget === track.id}
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
            <div class="grab" title="Click to select the track (its effects show in the Audio FX chain). Drag to move it up or down." onpointerdown={(e) => onHeaderPointer(e, track)}></div>
            {#if $chainTarget === track.id}<span class="fx-tag" title="The Audio FX chain is showing this track's effects">FX</span>{/if}
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
              {#if !isMidi(track)}
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
              {/if}
              <button type="button" class="ms mono short" class:on={!!track.mono} title="Mono: folds this track's left and right into one centered signal, after its effects and before its fader and pan. Part of the mix, so exports hear it too." aria-pressed={!!track.mono} onclick={() => setTrack(track, { mono: !track.mono }, track.mono ? "Track Stereo" : "Track Mono")}><MonoIcon /></button>
            </div>
            {#if !track.collapsed && headerView === "fx" && doc}
              <HeaderFx
                {doc}
                target={rowTarget(track.id, false)}
                chain={track.effects}
                catalog={fxCatalog}
                plugins={fxPlugins}
                onEdit={headerEdit}
                onOpen={(slot) => openHeaderEffect(track.id, track.name, slot)}
                onMenu={(x, y, items) => (menu = { x, y, items: items as ContextItem[] })}
              />
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
              {#if doc?.space}
                <button
                  type="button"
                  class="ms space3d"
                  title="3D Space session: this track plays at a spot in the room instead of a pan. Click to place it in the 3D Space panel."
                  onclick={() => {
                    editor.selectTrack(track.id);
                    run("multitrack.space");
                  }}>3D</button
                >
              {:else}
                <Dial value={track.pan} min={-100} max={100} step={1} origin={0} label="Pan" icon="pan" format={formatPan} onChange={(v) => setValue(track, "pan", v)} />
              {/if}
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
            {#if !track.collapsed && isMidi(track)}
              {@const inst = track.instrument ? instruments.find((x) => x.effect === track.instrument!.effect) : null}
              <div class="line">
<div class="instrument">
                  <button
                    type="button"
                    class="inst-name"
                    title={inst ? `Open ${inst.name}. The arrow changes the instrument.` : track.instrument ? "This instrument is not installed (or not scanned yet). Effects > Plug-in Manager." : "The VST3 instrument that plays this track's notes. Grouped by type and by maker."}
                    onclick={(e) => (inst ? openInstrument(track) : openInstrumentMenu(e, track))}
                  >{inst ? `${inst.name}${inst.vendor ? ` (${inst.vendor})` : ""}` : track.instrument ? "Missing instrument" : instruments.length ? "No instrument" : "No instruments found"}</button>
                  <button type="button" class="chev" aria-haspopup="menu" title="Change the instrument" aria-label="Change the instrument" onclick={(e) => openInstrumentMenu(e, track)}>▾</button>
                </div>
              </div>
            {/if}
            {#if !track.collapsed && headerView === "mix" && shownHeight(track) >= ROW_INPUT && !isMidi(track)}
              <div class="line">
                <button type="button" class="pick" title="The input this track records from" onclick={(e) => void openInputMenu(e, track)}>
                  <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M1.5 8h9M7.5 5l3 3-3 3" fill="none" stroke="currentColor" stroke-width="1.4" /></svg>
                  <span>{inputLabel(track)}</span>
                  <span class="chev" aria-hidden="true">&rsaquo;</span>
                </button>
              </div>
            {/if}
            {#if !track.collapsed && headerView === "mix" && shownHeight(track) >= ROW_OUTPUT && doc}
              {@const out = track.output ?? { type: "master" }}
              <div class="line">
                <label class="pick outpick" title="Where this track's sound goes after its fader: the master or a bus">
                  <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M14.5 8h-9M8.5 5l-3 3 3 3" fill="none" stroke="currentColor" stroke-width="1.4" /><path d="M3 3v10" stroke="currentColor" stroke-width="1.4" /></svg>
                  <select aria-label="{track.name} output" value={out.type === "bus" ? out.bus_id : MASTER_ID} onchange={(e) => setOutput("track", track.id, e.currentTarget.value)}>
                    <option value={MASTER_ID}>Master</option>
                    {#each busChoices(doc, track.id) as bc (bc.id)}<option value={bc.id}>{bc.name}</option>{/each}
                    {#if out.type === "bus" && !busChoices(doc, track.id).some((bc) => out.type === "bus" && bc.id === out.bus_id)}<option value={out.bus_id}>{outName(out)}</option>{/if}
                  </select>
                </label>
                {#if shownHeight(track) < ROW_AUTO}{@render lanesToggle(track)}{/if}
              </div>
            {/if}
            {#if !track.collapsed && headerView === "mix" && shownHeight(track) >= ROW_AUTO}
              <div class="line">
                <select
                  class="auto"
                  value={autoMode(track)}
                  title="Automation mode. Read plays the track's automation lanes; Off plays none of them (the fader, pan and effects hold still). Write, Latch and Touch (recording moves while playing) are not built yet."
                  onchange={(e) => setLanes(track, setMode(track, e.currentTarget.value === "off" ? "off" : "read"), "Automation Mode")}
                >
                  <option value="off" disabled={(track.envelopes?.length ?? 0) === 0}>Off</option>
                  <option value="read">Read</option>
                  <option value="write" disabled>Write (not built yet)</option>
                  <option value="latch" disabled>Latch (not built yet)</option>
                  <option value="touch" disabled>Touch (not built yet)</option>
                </select>
                {@render lanesToggle(track)}
              </div>
            {/if}
            {#if !track.collapsed && headerView === "mix" && shownHeight(track) >= ROW_SENDS && doc}
              <div class="sendview">
                <SendList {doc} {session} {runner} {onError} kind="track" id={track.id} name={track.name} output={track.output ?? { type: "master" }} sends={track.sends ?? []} noOutput />
              </div>
            {/if}
            {#if !track.collapsed}
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <div class="resize" title="Drag to change the track height" onpointerdown={(e) => startResize(e, track)}></div>
            {/if}
            <!-- Level meter: the input while armed, otherwise playback. -->
            <TrackMeter levels={meterOf(track)} armed={track.record_armed} onClear={() => transport.clearOver(track.id)} />
          </div>
          {#if track.show_automation && !track.collapsed}
            {@const lanes = shownLanes(track)}
            {#each lanes as lane, k (lane.id || "new")}
              {@const choices = autoChoices(track, fxCatalog, !!doc?.space)}
              {@const taken = (c: (typeof choices)[number]) => targetKey(c.target) !== targetKey(lane.target) && lanes.some((l) => targetKey(l.target) === targetKey(c.target))}
              <div class="header auto-head" class:alt={i % 2 === 1} style:--track-color={trackColor(track.color, i)} style:height="{liveLaneH(lane)}px">
                <!-- svelte-ignore a11y_no_static_element_interactions -->
                <div class="lane-edge" title="Drag to make this lane taller or shorter" onpointerdown={(e) => resizeLane(e, track, lane)}></div>
                <div class="line">
                  <select
                    class="auto-pick"
                    value={targetKey(lane.target)}
                    title="The setting this lane moves"
                    onchange={(e) => pickTarget(track, lane, e.currentTarget.value)}
                  >
                    {#if isOrphan(track, fxCatalog, lane.target)}<option value={targetKey(lane.target)}>Removed effect</option>{/if}
                    {#each choices.filter((c) => !c.group) as c (targetKey(c.target))}
                      <option value={targetKey(c.target)} disabled={taken(c)}>{c.label}</option>
                    {/each}
                    <!-- Each effect (plug-ins too), with its controls under it. -->
                    {#each [...new Set(choices.flatMap((c) => (c.group ? [c.group] : [])))] as g (g)}
                      <optgroup label={g.slice(0, g.lastIndexOf("#"))}>
                        {#each choices.filter((c) => c.group === g) as c (targetKey(c.target))}
                          <option value={targetKey(c.target)} disabled={taken(c)}>{c.name}</option>
                        {/each}
                      </optgroup>
                    {/each}
                  </select>
                </div>
                <div class="line">
                  <button
                    type="button"
                    class="ms wide"
                    class:on={lane.enabled}
                    title={lane.enabled ? "This lane plays (Read). Click to turn it off and hold the setting still." : "This lane is off. Click to play it."}
                    aria-pressed={lane.enabled}
                    disabled={!lane.id}
                    onclick={() => setLanes(track, putLane(track, { ...lane, enabled: !lane.enabled }), lane.enabled ? "Automation Lane Off" : "Automation Lane On")}
                  >{lane.enabled ? "Read" : "Off"}</button>
                  <span class="auto-count">{lane.points.length === 0 ? "Click the lane to add points" : `${lane.points.length} point${lane.points.length === 1 ? "" : "s"}`}</span>
                  {#if k === lanes.length - 1}
                    <button type="button" class="ms" title="Add a lane for another setting" disabled={!nextTarget(track, fxCatalog, !!doc?.space)} onclick={() => addLane(track)}>+</button>
                  {/if}
                  <button type="button" class="ms" title="Remove this lane and its points" disabled={!lane.id} onclick={() => setLanes(track, dropLane(track, lane.id), "Remove Automation Lane")}>×</button>
                </div>
              </div>
            {/each}
          {/if}
          {:else}
          {@const b = row.bus}
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
          <div
            class="header bus"
            class:selected={ed.selectedTrack === b.id}
            class:fx={$chainTarget === b.id}
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
              <span class="bus-tag" title="Bus track: sums the tracks routed or sent to it, then runs its own Audio FX chain and fader">BUS</span>
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
                <button type="button" class="ms mono" class:on={!!b.mono} title="Mono: folds this bus's left and right into one centered signal, after its effects and before its fader and pan. Part of the mix, so exports hear it too." aria-pressed={!!b.mono} onclick={() => setBus(b, { mono: !b.mono }, b.mono ? "Bus Stereo" : "Bus Mono")}><MonoIcon /></button>
              </div>
              {#if busHeight(b) >= ROW_OUTPUT - 8 && doc}
                <div class="line">
                  <label class="pick outpick" title="Where this bus's sound goes after its fader: the master or another bus">
                    <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M14.5 8h-9M8.5 5l-3 3 3 3" fill="none" stroke="currentColor" stroke-width="1.4" /><path d="M3 3v10" stroke="currentColor" stroke-width="1.4" /></svg>
                    <select aria-label="{b.name} output" value={b.output.type === "bus" ? b.output.bus_id : MASTER_ID} onchange={(e) => setOutput("bus", b.id, e.currentTarget.value)}>
                      <option value={MASTER_ID}>Master</option>
                      {#each busChoices(doc, b.id) as bc (bc.id)}<option value={bc.id}>{bc.name}</option>{/each}
                    </select>
                  </label>
                </div>
              {/if}
              {#if busHeight(b) >= ROW_SENDS - 30 && doc}
                <div class="sendview">
                  <SendList {doc} {session} {runner} {onError} kind="bus" id={b.id} name={b.name} output={b.output} sends={b.sends} noOutput />
                </div>
              {/if}
            {:else if doc}
              <HeaderFx
                {doc}
                target={rowTarget(b.id, true)}
                chain={b.effects}
                catalog={fxCatalog}
                plugins={fxPlugins}
                onEdit={headerEdit}
                onOpen={(slot) => openHeaderEffect(b.id, b.name, slot)}
                onMenu={(x, y, items) => (menu = { x, y, items: items as ContextItem[] })}
              />
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
            class:fx={$chainTarget === MASTER_ID}
            style:height="72px"
            title="Master: every track and bus ends here. Click to show its Audio FX chain."
            onclick={() => editor.selectTrack(MASTER_ID)}
            oncontextmenu={(e) => {
              e.preventDefault();
              e.stopPropagation();
            }}
          >
            <div class="line">
              <span class="bus-tag">OUT</span>
              <span class="name">Master</span>
              <button type="button" class="ms fx" class:on={mfx > 0} title={mfx > 0 ? `Master Audio FX: ${mfx} effect${mfx === 1 ? "" : "s"}. Click to edit.` : "Master Audio FX: empty. Click to add effects."} onclick={() => editor.selectTrack(MASTER_ID)}>fx</button>
            </div>
            <div class="line">
              <Dial value={doc.master?.volume_db ?? 0} min={-60} max={12} step={0.1} origin={0} label="Volume" icon="volume" format={(v) => `${formatDb(v)}`} onChange={setMasterVolume} />
              <button type="button" class="ms mono check" class:on={$monoCheck} title="Mono check: hear the whole mix folded to mono, to check how it holds up. Listening only: exports are not changed (to write a mono file, pick Mono in Export)." aria-pressed={$monoCheck} onclick={(e) => { e.stopPropagation(); void monoCheck.toggle().catch(onError); }}><MonoIcon /></button>
            </div>
            {#if doc.space && channelNames(doc.space.layout).length > 2}
              {@const names = channelNames(doc.space.layout)}
              <TrackMeter levels={tp?.bed?.length === names.length ? tp.bed : names.map(() => quiet())} {names} armed={false} onClear={() => transport.clearOver()} />
            {:else}
              <TrackMeter levels={tp?.master ?? null} armed={false} onClear={() => transport.clearOver()} />
            {/if}
          </div>
        {/if}
      </div>
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="lanes" class:dropping bind:this={lanesEl} bind:clientWidth={lanesWidth} onpointerleave={() => (razor = null)}>
        {#if stretchHint}<div class="stretch-hint" aria-live="polite">{stretchHint}</div>{/if}
        {#if dropGhost && dropping}
          {#each dropGhost.items as g, k (k)}
            <div class="drop-ghost" style:left="{g.x}px" style:width="{g.w}px" style:top="{dropGhost.top + 1}px" style:height="{dropGhost.height - 2}px">
              <span>{g.name}{dropGhost.newTrack && k === 0 ? " (new track)" : ""}</span>
            </div>
          {/each}
        {/if}
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
  <!-- Track scrollbar: each track's color, the visible part as the thumb.
       Drag the thumb to scroll; drag its ends to make every track taller or shorter. -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="vbar" bind:clientHeight={vbarH} onpointerdown={onVbarPointer} onpointermove={onVbarHover} ondblclick={fitAllTracks} onwheel={onVbarWheel} title="Drag to scroll the tracks. Drag the thumb's ends, or use the mouse wheel here, to zoom: every track the same height. Double-click to fit them all.">
    {#each shown as track, i (track.id)}
      <div class="vseg" style:height="{((shownHeight(track) + lanesHeight(track)) / Math.max(1, rowsH)) * 100}%" style:background={trackColor(track.color, i)}></div>
    {/each}
    {#if rowsH > 0}
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

{#snippet lanesToggle(track: TrackDoc)}
<button
  type="button"
  class="ms autos"
  class:on={!!track.show_automation}
  class:has={(track.envelopes?.length ?? 0) > 0}
  title={track.show_automation ? "Hide automation lanes" : (track.envelopes?.length ?? 0) > 0 ? `Show automation lanes (${track.envelopes!.length})` : "Show automation lanes: draw volume, pan or effect settings over time"}
  aria-pressed={!!track.show_automation}
  onclick={() => toggleLanes(track)}
>
  <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M1.5 11.5L5 6l4 3.5 5.5-6" fill="none" stroke="currentColor" stroke-width="1.4" /><circle cx="5" cy="6" r="1.4" fill="currentColor" /><circle cx="9" cy="9.5" r="1.4" fill="currentColor" /></svg>
</button>
{/snippet}

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
    display: flex;
    flex-direction: column;
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

  .instrument {
    display: flex;
    flex: 1;
    min-width: 0;
    height: 20px;
    border: 1px solid var(--border);
    border-radius: 2px;
    background: var(--surface-2);
  }

  .instrument button {
    height: 100%;
    padding: 0 4px;
    border: 0;
    border-radius: 0;
    background: none;
    color: var(--text);
    font: inherit;
    font-size: var(--font-size-small);
  }

  .instrument button:hover {
    background: var(--surface-3, var(--surface-1));
  }

  .instrument .inst-name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-align: left;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .instrument .chev {
    flex: none;
    border-left: 1px solid var(--border);
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
    width: 14px;
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

  /* Grips on the thumb's ends: drag them to zoom vertically. */
  .vend {
    position: absolute;
    left: 1px;
    right: 1px;
    height: 5px;
    border-radius: 2px;
    background: repeating-linear-gradient(180deg, #d0d0d6 0 1px, transparent 1px 2px);
    opacity: 0.85;
  }

  .vend.top {
    top: 1px;
  }

  .vend.bottom {
    bottom: 1px;
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

  /* The row whose effects the Audio FX chain shows. */
  .header.fx::after {
    content: "";
    position: absolute;
    inset: 0;
    border: 1px solid var(--accent);
    border-left-width: 3px;
    pointer-events: none;
  }

  /* On the grab strip, written sideways, so it never covers a control. */
  .fx-tag {
    position: absolute;
    bottom: 3px;
    left: 0;
    z-index: 2;
    width: 10px;
    padding: 2px 0;
    background: var(--accent);
    color: #fff;
    font-size: 8px;
    font-weight: 700;
    line-height: 10px;
    text-align: center;
    writing-mode: vertical-rl;
    transform: rotate(180deg);
    pointer-events: none;
  }

  .header.selected .name {
    color: var(--selected-text, #fff);
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

  .ms.autos {
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }

  .ms.autos svg {
    width: 13px;
    height: 13px;
  }

  .ms.autos.has:not(.on) {
    color: var(--accent);
  }

  .ms.wide {
    width: auto;
    padding: 0 6px;
  }

  .lane-edge {
    position: absolute;
    right: 0;
    bottom: -3px;
    left: 0;
    z-index: 2;
    height: 6px;
    cursor: ns-resize;
  }
  .lane-edge:hover {
    background: color-mix(in srgb, var(--accent) 45%, transparent);
  }
  .header.auto-head {
    position: relative;
    justify-content: center;
    padding-left: 22px;
    background: color-mix(in srgb, var(--header-a) 88%, var(--track-color) 12%);
    cursor: default;
  }

  .auto-pick {
    flex: 1;
    min-width: 0;
    font-size: var(--font-size-small);
  }

  .auto-count {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    color: var(--text-faint);
    font-size: var(--font-size-small);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .ms.space3d {
    width: 30px;
    font-size: 10px;
    font-weight: 700;
    color: var(--accent);
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

  /* Where dragged-in files would land. */
  .drop-ghost {
    position: absolute;
    z-index: 5;
    box-sizing: border-box;
    overflow: hidden;
    border: 1px dashed var(--accent);
    border-radius: 2px;
    background: color-mix(in srgb, var(--accent) 22%, transparent);
    pointer-events: none;
  }

  .drop-ghost span {
    display: block;
    padding: 1px 4px;
    overflow: hidden;
    background: color-mix(in srgb, var(--accent) 55%, transparent);
    color: #fff;
    font-size: 11px;
    text-overflow: ellipsis;
    white-space: nowrap;
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

  /* A slim bar in the track's color. Pointing at it widens it and shows
     grip dots, so it reads as the place to drag the track from. The hit
     area stays 10 px wide either way. */
  .grab {
    position: absolute;
    top: 0;
    bottom: 0;
    left: 0;
    z-index: 1;
    width: 10px;
    cursor: grab;
  }

  .grab::before {
    content: "";
    position: absolute;
    inset: 0 auto 0 0;
    width: 4px;
    background: var(--track-color, var(--border-strong));
    transition: width 0.12s ease;
  }

  .grab::after {
    content: "";
    position: absolute;
    top: 50%;
    left: 3px;
    width: 4px;
    height: 22px;
    transform: translateY(-50%);
    background-image: radial-gradient(circle, rgba(0, 0, 0, 0.5) 1.1px, transparent 1.4px);
    background-size: 4px 5.5px;
    opacity: 0;
    transition: opacity 0.12s ease;
  }

  .grab:hover::before,
  .grab:active::before {
    width: 10px;
  }

  .grab:hover::after,
  .grab:active::after {
    opacity: 1;
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

  .outpick {
    position: relative;
  }
  .outpick select {
    flex: 1;
    min-width: 0;
    height: 16px;
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--text);
    font: inherit;
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
