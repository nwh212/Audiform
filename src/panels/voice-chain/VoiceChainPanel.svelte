<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  Audio FX: up to 16 effects in order on the selected clip or track.
  Pick a preset or add effects one by one, bypass or reorder them, and
  set each effect's controls below the list. Every change is one
  undoable step, and playback picks it up from where it is: sliders
  apply as they move (a drag is one step), and the meters show the
  track chain's level going in and coming out.

  In the Waveform editor the chain is a preview on the open file: it
  plays but changes nothing until Apply writes it into the selection
  (or the whole file) as one undoable edit.
-->
<script lang="ts">
  import { BAND_EQ_KINDS, carryBands } from "./band-eq";
  import { fromImpulse, IMPULSE_ID, REVERB_ID, REVERB_KINDS, toImpulse } from "./reverb-kinds";
  import EffectPresetBar from "./EffectPresetBar.svelte";
  import ChannelRouting from "./ChannelRouting.svelte";
  import { STEREO } from "./routing";
  import { channelNames } from "../space/space";
  import { untrack } from "svelte";
  import Knob from "../../app/Knob.svelte";
  import { effectsCatalog, importRackPresets, learnNoisePrint, pluginOpenWindow, pluginRestart, pluginsRegistry, type EffectInfo, type EffectParamInfo, type LearnSource, type RackImport } from "../../app/core";
  import { markStopped, pluginsVersion, stoppedPlugins } from "../../stores/plugins";
  import { editor, type EditorState } from "../../stores/editor";
  import { chainTarget } from "../../stores/chain-target";
  import { prerenderStates, prerenderWords, refreshPrerenders, watchPrerenders, type PrerenderState } from "../../stores/prerender";
  import type { CommandRunner } from "../../stores/media-pool";
  import type { Preferences } from "../../stores/preferences";
  import { MASTER_CHAIN, meterFraction, type MeterLevel, type TransportState, type TransportStore } from "../../stores/transport";
  import { keyChoices, MASTER_ID } from "../mixer/routing";
  import { trackOp } from "../mixer/solo";
  import { defaultTrackColor } from "../editor/timeline";
  import type { EffectChainDoc, EffectSlotDoc, EffectTarget, SessionDoc, SessionStore } from "../../stores/session";
  import {
    addsOne,
    addSlot,
    addSlotAt,
    layoutSlots,
    moveSlotToIndex,
    applyPreset,
    BUILTIN_PRESETS,
    CATEGORY_COLORS,
    chainOf,
    formatParam,
    fromSlider,
    MAX_SLOTS,
    moveSlot,
    replaceSlot,
    newSlot,
    paramValue,
    presetFrom,
    removeSlot,
    sanitizePresets,
    setParam,
    setSlot,
    toSlider,
    pluginMenuGroups,
    type ChainPreset,
    type MenuPlugin,
  } from "./chain";
  import ContextMenu, { type ContextItem } from "../../app/ContextMenu.svelte";
  import EffectControls from "./EffectControls.svelte";
  import { groupShown, PHASE_CURVE } from "./phase-curve";
  import { GRAPHED } from "./graph";
  import { clampBox, effectWindowRequest, HIDE_EFFECT_WINDOWS, portal, trialSlot, type WindowBox } from "./effect-window";
  import { dropEdit, effectMenu, ownerKey, slotOver, startSlotDrag, type SlotSpot } from "./slot-drag";

  interface Props {
    session: SessionStore;
    runner: CommandRunner;
    prefs: Preferences;
    ask: (title: string, label: string, value: string) => Promise<string | null>;
    /** Playback, for the chain meters. */
    transport?: TransportStore;
    /** The Waveform editor's file and playback, for the preview chain. */
    wave?: SessionStore;
    waveTransport?: TransportStore;
    /** Writes the preview chain into the file (selection or whole file). */
    onApplyWave?: (chain: EffectChainDoc) => Promise<void>;
    onNotice?: (m: string) => void;
    onError?: (e: unknown) => void;
  }

  const { session, runner, prefs, ask, transport, wave, waveTransport, onApplyWave, onNotice = () => {}, onError = console.error }: Props = $props();
  const KEY_PRESETS = "voiceChain.presets";
  const KEY_TAB = "voiceChain.tab";

  let doc = $state<SessionDoc | null>(null);
  $effect(() =>
    session.subscribe((d) => {
      doc = d;
    }),
  );
  let waveDoc = $state<SessionDoc | null>(null);
  $effect(() =>
    wave?.subscribe((d) => {
      waveDoc = d;
    }),
  );
  let ed = $state<EditorState>(editor.current());
  $effect(() =>
    editor.subscribe((s) => {
      ed = s;
    }),
  );

  let catalog = $state<EffectInfo[]>([]);
  let loadError = $state<string | null>(null);
  // Loads again after a plug-in scan.
  $effect(() =>
    pluginsVersion.subscribe(() => {
      void effectsCatalog()
        .then((c) => (catalog = c))
        .catch((e) => (loadError = String(e)));
      void pluginsRegistry()
        .then((r) => (menuPlugins = r.plugins.filter((p) => !p.instrument)))
        .catch(() => (menuPlugins = []));
    }),
  );
  /** Installed effect plug-ins with their maker and type, for the Add effect menu. */
  let menuPlugins = $state<MenuPlugin[]>([]);

  /**
   * Opens a slot the way its effect is best edited: a plug-in in its own
   * window (when it has one and is running), anything else in the
   * settings window here.
   */
  function openSlot(s: EffectSlotDoc) {
    const info = byId.get(s.effect);
    if (info?.plugin && info.hasWindow && !$stoppedPlugins.has(s.id)) openPluginWindow(s);
    else openWindow(s.id);
  }

  /** Opens a plug-in's own window (it runs in its own process). */
  function openPluginWindow(s: EffectSlotDoc) {
    const name = byId.get(s.effect)?.name ?? "Plug-in";
    // Opening it may show inputs the scan missed (a sidechain): the lists are read again.
    void pluginOpenWindow($state.snapshot(s) as EffectSlotDoc, label ? `${name} - ${label}` : name)
      .then(() => pluginsVersion.update((n) => n + 1))
      .catch(onError);
  }

  /** Starts a stopped plug-in again; it comes back with the next playback. */
  function restartPlugin(s: EffectSlotDoc) {
    void pluginRestart(s.id)
      .then(() => {
        markStopped(s.id, null);
        onNotice("The plug-in starts again with the next playback.");
      })
      .catch(onError);
  }
  const byId = $derived(new Map(catalog.map((e) => [e.id, e])));
  const known = $derived(new Set(catalog.map((e) => e.id)));
  const categories = $derived([...new Set(catalog.map((e) => e.category))]);

  let userPresets = $state<ChainPreset[]>([]);
  let tab = $state<"clip" | "track">("track");
  // svelte-ignore state_referenced_locally
  void prefs.load().then(() => {
    userPresets = sanitizePresets(prefs.raw(KEY_PRESETS));
    if (prefs.raw(KEY_TAB) === "clip") tab = "clip";
  });
  function setTab(t: "clip" | "track") {
    tab = t;
    prefs.set(KEY_TAB, t);
  }

  // What the chain belongs to. While an effect's window is open, the panel
  // stays on that chain, so clicking another track or clip in the editor
  // does not take the window away.
  let hold = $state<{ owner: string | null; clip: string | null } | null>(null);
  const oneClip = $derived(hold ? hold.clip : ed.selectedClips.size === 1 ? [...ed.selectedClips][0]! : null);
  const clipTrack = $derived(oneClip ? doc?.tracks.find((t) => t.clips.some((c) => c.id === oneClip)) : undefined);
  // The track picked in the panel wins; otherwise the selected track,
  // the selected clip's track, or the first track, so the chain is never
  // hidden just because nothing is selected yet.
  let pickedTrack = $state<string | null>(null);
  // Derived values only signal when they change, so this runs on a new
  // selection, not on every editor update.
  const selTrack = $derived(ed.selectedTrack);
  $effect(() => {
    // A new selection in the editor takes over from the panel's pick (not while a window holds it).
    void selTrack;
    void oneClip;
    if (!hold) pickedTrack = null;
  });
  // Picking another row closes an open effect window, so the panel shows the new row's chain.
  $effect(() => {
    const id = selTrack;
    const h = untrack(() => hold);
    if (h && id && id !== h.owner) {
      winOpen = false;
      hold = null;
    }
  });
  // Picking a row (a track header, a MIDI track, a bus or the master) shows
  // its chain on Track Effects: a bus or the master has no clips, and a row
  // picked without a clip means its own chain.
  $effect(() => {
    const id = selTrack;
    const c = oneClip;
    if (hold || !id || untrack(() => tab) !== "clip") return;
    const isRow = id === MASTER_ID || !!untrack(() => doc?.buses?.some((b) => b.id === id)) || c === null;
    if (isRow) untrack(() => setTab("track"));
  });
  const track = $derived(
    (hold ? doc?.tracks.find((t) => t.id === hold!.owner) : undefined) ??
      doc?.tracks.find((t) => t.id === pickedTrack) ??
      doc?.tracks.find((t) => t.id === ed.selectedTrack) ??
      clipTrack ??
      doc?.tracks[0],
  );
  const clip = $derived(oneClip ? clipTrack?.clips.find((c) => c.id === oneClip) : undefined);
  const inWave = $derived(ed.mode === "waveform");
  const waveTrack = $derived(waveDoc?.tracks[0]);
  // Track Effects also shows a bus's or the master's chain, picked here or in the Mixer.
  const ownerId = $derived(hold ? hold.owner : (pickedTrack ?? ed.selectedTrack));
  const onTrackTab = $derived(!inWave && tab === "track");
  const bus = $derived(onTrackTab ? doc?.buses?.find((b) => b.id === ownerId) : undefined);
  const onMaster = $derived(onTrackTab && !!doc && ownerId === MASTER_ID);
  /** The chain's channels: a 3D Space master runs on its speakers. */
  const chainNames = $derived(onMaster && doc?.space && !inWave ? channelNames(doc.space.layout) : STEREO);
  /** A plug-in added to a 3D Space master runs with its own channels. */
  const fresh = (info: EffectInfo, id: string): EffectSlotDoc => {
    const s = newSlot(info, id);
    return info.plugin && chainNames.length > 2 ? { ...s, routing: { wide: true } } : s;
  };
  let routingOpen = $state(false);
  /** The (i) in an effect window's title bar: shows what the effect does and how to use it. */
  let howto = $state(false);
  // Tell the Editor whose chain this is, so it can mark that row.
  $effect(() => {
    chainTarget.set(onTrackTab ? (onMaster ? MASTER_ID : (bus?.id ?? track?.id ?? null)) : null);
  });
  $effect(() => () => chainTarget.set(null));
  const target = $derived.by((): EffectTarget | null => {
    if (inWave) return waveTrack ? { kind: "track", id: waveTrack.id } : null;
    if (tab === "clip") return clip ? { kind: "clip", id: clip.id } : null;
    if (onMaster) return { kind: "master" };
    if (bus) return { kind: "bus", id: bus.id };
    return track ? { kind: "track", id: track.id } : null;
  });
  /** The color of whose chain this is (the track's, bus's or clip's), shown along the panel. */
  const cssColor = (c: string) => (c.startsWith("--") ? `var(${c})` : c);
  const accent = $derived.by(() => {
    if (inWave) return "var(--accent)";
    if (onMaster) return "var(--accent)";
    if (bus) return bus.color ? cssColor(bus.color) : "var(--clip-3)";
    const t = tab === "clip" ? (clipTrack ?? track) : track;
    if (tab === "clip" && clip?.color) return cssColor(clip.color);
    if (!t || !doc) return "var(--accent)";
    return cssColor(t.color ?? defaultTrackColor(doc.tracks.indexOf(t)));
  });
  /** Whose chain the sidechain choices are for: a track, bus or the master. */
  const keyOwner = $derived(onMaster ? MASTER_ID : (bus?.id ?? track?.id ?? null));
  const why = $derived(
    inWave
      ? "Open a file in the Waveform editor to try an Audio FX chain on it."
      : tab === "clip"
        ? ed.selectedClips.size > 1
          ? "Select one clip to see its effects."
          : "Select a clip to give it effects."
        : "Add a track to the session to give it effects.",
  );
  const label = $derived(
    inWave ? "Preview on the open file" : tab === "clip" ? (clip ? `Clip: ${clip.name}` : "") : onMaster ? "Master" : bus ? `Bus: ${bus.name}` : track ? `Track: ${track.name}` : "",
  );
  const chain = $derived(
    chainOf(inWave ? waveTrack?.effects : tab === "clip" ? clip?.effects : onMaster ? doc?.master?.effects : bus ? bus.effects : track?.effects),
  );

  let selectedSlot = $state<string | null>(null);
  const slot = $derived(chain.slots.find((s) => s.id === selectedSlot) ?? chain.slots[0] ?? null);
  const slotInfo = $derived(slot ? byId.get(slot.effect) : undefined);

  // Effect settings open in a floating window, one effect at a time.
  let winOpen = $state(false);
  // Effects > Hide All Effect Windows.
  $effect(() => {
    const hide = () => (winOpen = false);
    window.addEventListener(HIDE_EFFECT_WINDOWS, hide);
    return () => window.removeEventListener(HIDE_EFFECT_WINDOWS, hide);
  });
  let winBox = $state<WindowBox>({ x: 340, y: 140 });
  let winEl = $state<HTMLElement | null>(null);
  const winSlot = $derived(winOpen ? (chain.slots.find((s) => s.id === selectedSlot) ?? null) : null);
  const winInfo = $derived(winSlot ? byId.get(winSlot.effect) : undefined);
  const winIndex = $derived(winSlot ? chain.slots.indexOf(winSlot) : -1);
  function openWindow(id: string) {
    selectedSlot = id;
    winOpen = true;
  }
  // Hold the chain while a window is open; let go when it closes.
  $effect(() => {
    if (winOpen && !hold) hold = { owner: onMaster ? MASTER_ID : (bus?.id ?? track?.id ?? null), clip: tab === "clip" ? (clip?.id ?? null) : null };
    if (!winOpen && hold) hold = null;
  });
  // A trial effect (picked from the Effects menu in the Waveform editor)
  // leaves the preview once its window closes, applied or not.
  let trialSeen = false;
  $effect(() => {
    const t = $trialSlot;
    if (!t) {
      trialSeen = false;
      return;
    }
    if (winOpen && winSlot?.id === t) {
      trialSeen = true;
      return;
    }
    if (!trialSeen) return;
    trialSeen = false;
    untrack(() => {
      if (inWave && chain.slots.some((x) => x.id === t)) commit("Close Effect", removeSlot(chain, t));
    });
    trialSlot.set(null);
  });
  async function applyTrial() {
    if (!onApplyWave || applying || !winSlot) return;
    applying = true;
    try {
      await onApplyWave({ ...($state.snapshot(chain) as EffectChainDoc), slots: [$state.snapshot(winSlot) as EffectSlotDoc] });
      winOpen = false;
    } catch (e) {
      onError(e);
    } finally {
      applying = false;
    }
  }
  // Effects added from the Effects menu open here too.
  $effect(() => {
    const id = $effectWindowRequest;
    if (!id) return;
    if (!chain.slots.some((x) => x.id === id) && !inWave) {
      if (clip?.effects?.slots.some((x) => x.id === id) && tab !== "clip") setTab("clip");
      else if (track?.effects?.slots.some((x) => x.id === id) && tab !== "track") setTab("track");
      else return;
    }
    effectWindowRequest.set(null);
    // A plug-in opens in its own window, like a click on its slot.
    const slot = [...chain.slots, ...(clip?.effects?.slots ?? []), ...(track?.effects?.slots ?? [])].find((x) => x.id === id);
    if (slot) untrack(() => openSlot(slot));
    else openWindow(id);
  });
  function dragWindow(e: PointerEvent) {
    if ((e.target as HTMLElement).closest("button")) return;
    e.preventDefault();
    const sx = e.clientX - winBox.x;
    const sy = e.clientY - winBox.y;
    const move = (m: PointerEvent) => {
      const r = winEl?.getBoundingClientRect();
      winBox = clampBox({ x: m.clientX - sx, y: m.clientY - sy }, r?.width ?? 360, r?.height ?? 300, window.innerWidth, window.innerHeight);
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  }

  // Pre-render: the track's clips and Audio FX rendered ahead, so heavy
  // plug-ins rest. The core renders again whenever the sound changes.
  let prerenders = $state<Record<string, PrerenderState>>({});
  const canPrerender = $derived(onTrackTab && !!track && !bus && !onMaster);
  const prerenderState = $derived<PrerenderState | null>(track?.prerender ? (prerenders[track.id] ?? "waiting") : null);
  $effect(() => prerenderStates.subscribe((v) => (prerenders = v)));
  $effect(() => watchPrerenders());
  $effect(() => {
    void doc;
    untrack(refreshPrerenders);
  });
  function togglePrerender() {
    if (!track) return;
    const on = !track.prerender;
    void session.edit(on ? "Pre-render Track" : "Stop Pre-rendering Track", trackOp(doc, track.id, { prerender: on }), runner).catch(onError);
  }

  function commit(what: string, next: EffectChainDoc | null) {
    if (!target) return;
    if (!next) {
      onNotice(`An Audio FX chain holds ${MAX_SLOTS} effects.`);
      return;
    }
    const op = { op: "set_effects", target, chain: next } as const;
    // The Waveform preview is not an edit: it never enters undo, and Apply is the edit.
    if (inWave) void wave?.apply(op).catch(onError);
    else void session.edit(what, op, runner).catch(onError);
  }

  const waveSel = $derived(waveDoc?.view.selection && waveDoc.view.selection.end > waveDoc.view.selection.start ? waveDoc.view.selection : null);
  let applying = $state(false);
  async function applyWave() {
    if (!onApplyWave || applying) return;
    applying = true;
    try {
      await onApplyWave($state.snapshot(chain) as EffectChainDoc);
    } catch (e) {
      onError(e);
    } finally {
      applying = false;
    }
  }

  function add(id: string) {
    const info = byId.get(id);
    if (!info) return;
    const s = newSlot(info, crypto.randomUUID());
    commit(`Add ${info.name}`, addSlot(chain, s));
    openSlot(s);
  }

  /** Adds an effect into numbered slot `at` (0 first). */
  function addAt(id: string, at: number) {
    const info = byId.get(id);
    if (!info) return;
    const s = fresh(info, crypto.randomUUID());
    commit(`Add ${info.name}`, addSlotAt(chain, s, at));
    openSlot(s);
  }

  /** An empty slot's Add button: the families as submenus, adding into that slot. */
  function openAddMenu(e: MouseEvent, at: number) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    slotMenu = { x: r.left, y: r.bottom, items: familyItems((id) => addAt(id, at)) };
  }

  /** Where this chain's slot rows say they belong, for dragging effects between chains. */
  const owner = $derived(target && !inWave ? ownerKey(target) : "wave");
  const rows = $derived(layoutSlots(chain));
  /** A dragged effect let go over a slot: moved here, or copied from another chain. */
  function dropSlot(s: EffectSlotDoc, to: SlotSpot) {
    if (to.owner === owner) {
      commit("Move Effect", moveSlotToIndex(chain, s.id, to.index));
      return;
    }
    if (inWave || !target || !doc) return;
    const ed = dropEdit(doc, target, $state.snapshot(s) as EffectSlotDoc, to, () => crypto.randomUUID());
    if (ed) void session.edit(ed.label, ed.op, runner).catch(onError);
    else onNotice(`An Audio FX chain holds ${MAX_SLOTS} effects.`);
  }

  // Slot rows: drag to reorder, Alt+Up or Alt+Down on a row, and a right-click menu.
  let slotMenu = $state<{ x: number; y: number; items: ContextItem[] } | null>(null);
  function onRowKey(e: KeyboardEvent, id: string) {
    if (!e.altKey || (e.key !== "ArrowUp" && e.key !== "ArrowDown")) return;
    e.preventDefault();
    commit("Move Effect", moveSlot(chain, id, e.key === "ArrowUp" ? -1 : 1));
  }
  /** The effects of each family as submenus, for adding or replacing a slot. */
  function familyItems(pick: (id: string) => void): ContextItem[] {
    return effectMenu(catalog, menuPlugins, pick) as ContextItem[];
  }
  /** The menu for a slot, empty or not: Edit and Remove first, then the effects to put there. */
  function slotItems(s: EffectSlotDoc | undefined, at: number): ContextItem[] {
    const info = s ? byId.get(s.effect) : undefined;
    const none = "There is no effect in this slot yet.";
    const items: ContextItem[] = s
      ? [
          { kind: "item", label: info?.plugin && info.hasWindow ? "Open Plug-in Window" : "Edit Effect", run: () => openSlot(s) },
          ...(info?.plugin ? [{ kind: "item", label: "Show Controls", run: () => openWindow(s.id) } as ContextItem] : []),
          { kind: "item", label: "Remove Effect", run: () => commit(`Remove ${info?.name ?? "Effect"}`, removeSlot(chain, s.id)) },
        ]
      : [
          { kind: "item", label: "Edit Effect", disabled: true, reason: none, run: () => {} },
          { kind: "item", label: "Remove Effect", disabled: true, reason: none, run: () => {} },
        ];
    items.push({ kind: "sep" });
    items.push(
      ...familyItems((id) => {
        if (!s) {
          addAt(id, at);
          return;
        }
        const ni = byId.get(id);
        if (!ni) return;
        const made = fresh(ni, crypto.randomUUID());
        commit(`Replace with ${ni.name}`, replaceSlot(chain, s.id, { ...made, position: s.position ?? null }));
        openSlot(made);
      }),
    );
    return items;
  }
  function openSlotMenu(e: MouseEvent, s: EffectSlotDoc | undefined, at: number) {
    e.preventDefault();
    slotMenu = { x: e.clientX, y: e.clientY, items: slotItems(s, at) };
  }
  /** The arrow at the end of a row opens the same menu beside the button. */
  function openSlotArrow(e: MouseEvent, s: EffectSlotDoc | undefined, at: number) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    slotMenu = { x: r.left, y: r.bottom, items: slotItems(s, at) };
  }

  function choosePreset(name: string) {
    const p = [...userPresets, ...BUILTIN_PRESETS].find((x) => x.name === name);
    if (!p) return;
    const next = applyPreset(chain, p, known, () => crypto.randomUUID());
    selectedSlot = next?.slots[addsOne(p) ? next.slots.length - 1 : 0]?.id ?? null;
    commit(`Preset ${p.name}`, next);
  }

  async function savePreset() {
    const name = (await ask("Save Audio FX Preset", "Preset name", ""))?.trim();
    if (!name) return;
    if (BUILTIN_PRESETS.some((p) => p.name === name)) {
      onNotice(`"${name}" is a built-in preset. Pick another name.`);
      return;
    }
    userPresets = [...userPresets.filter((p) => p.name !== name), presetFrom(name, chain)].sort((a, b) => a.name.localeCompare(b.name));
    prefs.set(KEY_PRESETS, $state.snapshot(userPresets));
    onNotice(`Saved preset ${name}.`);
  }

  async function importPresets() {
    let r: RackImport | null;
    try {
      r = await importRackPresets();
    } catch (e) {
      onNotice(`Could not import presets: ${String(e)}`);
      return;
    }
    if (!r) return;
    if (r.presets.length === 0) {
      onNotice("No effects rack presets were found in that file.");
      return;
    }
    const taken = new Set([...userPresets, ...BUILTIN_PRESETS].map((p) => p.name));
    const added: ChainPreset[] = [];
    for (const p of r.presets) {
      let name = p.name;
      for (let n = 2; taken.has(name); n++) name = `${p.name} (${n})`;
      taken.add(name);
      added.push(presetFrom(name, p.chain));
    }
    userPresets = [...userPresets, ...added].sort((a, b) => a.name.localeCompare(b.name));
    prefs.set(KEY_PRESETS, $state.snapshot(userPresets));
    const notes = [
      `Imported ${added.length} preset${added.length === 1 ? "" : "s"} (under Yours).`,
      r.converted ? `${r.converted} built-in effect${r.converted === 1 ? "" : "s"} of the other editor came in as ours.` : "",
      r.missingPlugins.length ? `Not found on this computer (install or scan them in the Plug-in Manager): ${r.missingPlugins.join(", ")}.` : "",
      r.skipped.length ? `Left out: ${r.skipped.join(", ")}.` : "",
    ];
    onNotice(notes.filter(Boolean).join(" "));
  }

  function deletePreset(name: string) {
    userPresets = userPresets.filter((p) => p.name !== name);
    prefs.set(KEY_PRESETS, $state.snapshot(userPresets));
    onNotice(`Deleted preset ${name}.`);
  }

  // Sliders show the value while dragging and apply as they move, a
  // few times a second, so playback follows. Consecutive changes to one
  // control merge into one undo step. Controls that change an effect's
  // look-ahead would restart playback each time, so they apply on release.
  let draft = $state<{ key: string; value: number } | null>(null);
  const LIVE_MS = 90;
  let liveTimer: ReturnType<typeof setTimeout> | undefined;
  let liveNext: (() => void) | null = null;
  function live(fn: () => void) {
    liveNext = fn;
    if (liveTimer) return;
    liveTimer = setTimeout(() => {
      liveTimer = undefined;
      const f = liveNext;
      liveNext = null;
      f?.();
    }, LIVE_MS);
  }
  function settle() {
    clearTimeout(liveTimer);
    liveTimer = undefined;
    liveNext = null;
  }
  $effect(() => () => clearTimeout(liveTimer));
  const paramLabel = (s: EffectSlotDoc, p: EffectParamInfo) => `${byId.get(s.effect)?.name ?? "Effect"} ${p.name}`;
  function shown(s: EffectSlotDoc, p: EffectParamInfo): number {
    return draft && draft.key === `${s.id}.${p.key}` ? draft.value : paramValue(s, p);
  }
  function drag(s: EffectSlotDoc, p: EffectParamInfo, v: number) {
    draft = { key: `${s.id}.${p.key}`, value: v };
    if (p.retimes) return;
    const id = s.id;
    live(() => {
      const cur = chain.slots.find((x) => x.id === id);
      if (cur && paramValue(cur, p) !== v) commit(paramLabel(cur, p), setParam(chain, id, p, v));
    });
  }
  function setValue(s: EffectSlotDoc, p: EffectParamInfo, v: number) {
    settle();
    draft = null;
    const cur = chain.slots.find((x) => x.id === s.id) ?? s;
    if (v === paramValue(cur, p)) return;
    commit(paramLabel(cur, p), setParam(chain, s.id, p, v));
  }
  /** A Curve EQ point dragged on the graph: frequency and gain together. */
  function dragPoint(s: EffectSlotDoc, pf: EffectParamInfo, f: number, pg: EffectParamInfo, g: number, done: boolean) {
    const id = s.id;
    const apply = () => {
      const next = setParam(setParam(chain, id, pf, f), id, pg, g);
      commit(`${paramLabel(s, pf).replace(pf.name, "").trim() || "Curve"} Point`, next);
    };
    if (done) {
      settle();
      apply();
    } else live(apply);
  }

  /** Several controls of one effect moved together (the EQ graph's nodes); one undoable step on release. */
  function dragParams(s: EffectSlotDoc, changes: [EffectParamInfo, number][], label: string, done: boolean) {
    const id = s.id;
    const apply = () => commit(label, changes.reduce((c, [p, v]) => setParam(c, id, p, v), chain));
    if (done) {
      settle();
      apply();
    } else live(apply);
  }

  /** Input, output and mix: the same, on the chain itself. */
  function dragChain(key: "input_gain_db" | "output_gain_db" | "mix", label: string, shownValue: number, value: number) {
    draft = { key: `chain.${key}`, value: shownValue };
    live(() => {
      if (chain[key] !== value) commit(label, { ...chain, [key]: value });
    });
  }
  function setChain(key: "input_gain_db" | "output_gain_db" | "mix", label: string, value: number) {
    settle();
    draft = null;
    if (chain[key] !== value) commit(label, { ...chain, [key]: value });
  }

  // Noise Print Reducer: learn the print from the time selection. A bus
  // or the master has no audio of its own to learn from yet.
  const canLearn = $derived(inWave || target?.kind === "track" || target?.kind === "clip");
  /** Effects whose window draws all of its own controls. */
  const BAND_EQS: string[] = BAND_EQ_KINDS.map((k) => k.id);
  const OWN_CONTROLS = new Set(["altaudio.parametric_eq", "altaudio.phase_curve", ...BAND_EQS, "altaudio.notch", "altaudio.center_extractor", "altaudio.convolution_reverb", "altaudio.shaped_noise", "altaudio.opto_leveler", "altaudio.tube_preamp", "altaudio.amp_model", "altaudio.vox_strip", "altaudio.stereo_width", "altaudio.limiter", "altaudio.channel_mixer", "altaudio.reverb", "altaudio.echo", "altaudio.modulation"]);
  const LEARNS = new Set(["altaudio.noise_print", "altaudio.sound_eraser", "altaudio.shaped_noise"]);
  const learnSel = $derived.by(() => {
    const v = inWave ? waveDoc?.view.selection : doc?.view.selection;
    return v && v.end > v.start ? v : null;
  });
  let learning = $state(false);
  async function learnPrint(s: EffectSlotDoc) {
    if (!learnSel || !target || learning || !canLearn) return;
    const source: LearnSource =
      target.kind === "master" || target.kind === "bus" || inWave
        ? { kind: "wave", start: learnSel.start, end: learnSel.end }
        : { kind: target.kind, id: target.id, start: learnSel.start, end: learnSel.end };
    learning = true;
    try {
      const r = await learnNoisePrint(source);
      const cur = chain.slots.find((x) => x.id === s.id);
      if (!cur) return;
      commit("Learn Noise Print", { ...chain, slots: chain.slots.map((x) => (x.id === s.id ? { ...x, params: { ...x.params, ...r.params }, preset: null } : x)) });
      onNotice(`Learned ${s.effect === "altaudio.sound_eraser" ? "the sound" : "the noise"} from ${r.seconds.toFixed(2)} s.`);
    } catch (e) {
      onError(e);
    } finally {
      learning = false;
    }
  }

  // Meters: the track chain's level in and out, as playback hears it.
  let ts = $state<TransportState | null>(null);
  const tp = $derived(inWave ? waveTransport : transport);
  $effect(() =>
    tp?.subscribe((x) => {
      ts = x;
    }),
  );
  const meterTrack = $derived(inWave ? waveTrack : tab === "track" ? track : clipTrack);
  const meterKey = $derived(onMaster ? MASTER_CHAIN : bus ? bus.id : meterTrack?.id);
  const chainMeter = $derived(meterKey ? ts?.chains[meterKey] : undefined);
  const louder = (a?: MeterLevel, b?: MeterLevel) => (a && b ? (a.peak >= b.peak ? a : b) : (a ?? b));
  const dbText = (m: MeterLevel | undefined) => (m && m.peak > 0.00001 ? (20 * Math.log10(m.peak)).toFixed(1) : "-inf");

  const GAIN = (key: string, name: string): EffectParamInfo => ({ key, name, unit: "dB", min: -24, max: 24, default: 0, step: 0.1, log: false, toggle: false });
  const mixShown = $derived(draft?.key === "chain.mix" ? draft.value : Math.round(chain.mix * 100));
  const MIX: EffectParamInfo = { key: "mix", name: "Mix", unit: "%", min: 0, max: 100, default: 100, step: 1, log: false, toggle: false };

</script>

<section class="vc" aria-label="Audio FX" style:--rack={accent}>
  <div class="tabs" role="tablist" hidden={inWave}>
    <button type="button" role="tab" aria-selected={tab === "clip"} class:on={tab === "clip"} onclick={() => setTab("clip")}>Clip Effects</button>
    <button type="button" role="tab" aria-selected={tab === "track"} class:on={tab === "track"} onclick={() => setTab("track")}>Track Effects</button>
  </div>

  {#if loadError}
    <p class="empty">Effects could not load: {loadError}</p>
  {:else if !target}
    <p class="empty">{why}</p>
  {:else}
    <div class="top">
      <div class="line">
        <button
          type="button"
          class="power"
          class:on={chain.enabled}
          title={chain.enabled ? "Chain on: click to bypass every effect" : "Chain bypassed: click to turn it on"}
          aria-pressed={chain.enabled}
          onclick={() => commit(chain.enabled ? "Bypass Audio FX" : "Turn On Audio FX", { ...chain, enabled: !chain.enabled })}
        >
          <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M8 2v6" stroke="currentColor" stroke-width="1.6" /><path d="M4.6 4.4a5 5 0 1 0 6.8 0" fill="none" stroke="currentColor" stroke-width="1.4" /></svg>
        </button>
        {#if canPrerender}
          <button
            type="button"
            class="prerender"
            class:on={!!track?.prerender}
            class:busy={prerenderState === "rendering"}
            class:ready={prerenderState === "ready"}
            title={prerenderState ? `${prerenderWords(prerenderState)} Any change to clips or effects renders again. Click to turn off.` : "Pre-render: render this track's clips and effects ahead, so heavy plug-ins rest during playback. Renders again after any change."}
            aria-pressed={!!track?.prerender}
            onclick={togglePrerender}
          >
            <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M2 8h1.5M5 5v6M8 3v10M11 6v4" stroke="currentColor" stroke-width="1.4" fill="none" /><rect x="12.5" y="3" width="2" height="10" fill="currentColor" /></svg>
            <span>{prerenderState === "rendering" ? "Rendering…" : prerenderState === "ready" ? "Rendered" : prerenderState === "waiting" ? "Pre-render on" : "Pre-render"}</span>
          </button>
        {/if}
        <select
          class="grow"
          aria-label="Presets"
          value=""
          onchange={(e) => {
            choosePreset(e.currentTarget.value);
            e.currentTarget.value = "";
          }}
        >
          <option value="" disabled>Presets…</option>
          {#if userPresets.length}
            <optgroup label="Yours">
              {#each userPresets as p (p.name)}<option value={p.name}>{p.name}</option>{/each}
            </optgroup>
          {/if}
          {#each ["Voice chains", "Breath gates", "Processing", "Delay", "Repair", "EQ"] as g (g)}
            <optgroup label={g === "Voice chains" ? "Voice chains (replace the chain)" : `${g} (add one effect)`}>
              {#each BUILTIN_PRESETS.filter((p) => p.group === g) as p (p.name)}<option value={p.name}>{p.name}</option>{/each}
            </optgroup>
          {/each}
        </select>
        <button type="button" title="Save this chain as a preset" disabled={chain.slots.length === 0} onclick={() => void savePreset()}>Save…</button>
        <button type="button" title="Import effects rack presets from preset files (XML). Built-in effects of the other editor come in as ours, as in a session import; plug-ins keep their settings." onclick={() => void importPresets()}>Import…</button>
        {#if userPresets.length}
          <select
            class="del"
            aria-label="Delete a saved preset"
            title="Delete a saved preset"
            value=""
            onchange={(e) => {
              deletePreset(e.currentTarget.value);
              e.currentTarget.value = "";
            }}
          >
            <option value="" disabled>Delete…</option>
            {#each userPresets as p (p.name)}<option value={p.name}>{p.name}</option>{/each}
          </select>
        {/if}
      </div>
      <span class="target" title="Pick a track, bus or the master in the session to see its chain here">{label}</span>
    </div>

    <ol class="slots" class:off={!chain.enabled}>
      {#each rows as s, i (i)}
        {@const info = s ? byId.get(s.effect) : undefined}
        <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
        <li
          class:sel={!!s && s.id === slot?.id}
          class:bypassed={!!s && !s.enabled}
          class:over={$slotOver?.owner === owner && $slotOver.index === i}
          data-slot-owner={owner}
          data-slot-index={i}
          tabindex={s ? 0 : -1}
          title={s ? "Drag to another slot to move it, or onto another track's effects (Editor, Effects view) to copy it" : undefined}
          onpointerdown={(e) => s && startSlotDrag(e, (to) => dropSlot(s, to))}
          onkeydown={(e) => s && onRowKey(e, s.id)}
          oncontextmenu={(e) => openSlotMenu(e, s ?? undefined, i)}
        >
          <span class="n">{i + 1}</span>
          {#if s}
            <button
              type="button"
              class="pow"
              class:on={s.enabled}
              title={s.enabled ? "On: click to bypass" : "Off: click to turn on"}
              aria-label={s.enabled ? "Bypass effect" : "Turn effect on"}
              aria-pressed={s.enabled}
              onclick={() => commit(s.enabled ? "Bypass Effect" : "Turn On Effect", setSlot(chain, s.id, { enabled: !s.enabled }))}
            >
              <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M8 2v6" stroke="currentColor" stroke-width="1.8" /><path d="M4.6 4.4a5 5 0 1 0 6.8 0" fill="none" stroke="currentColor" stroke-width="1.6" /></svg>
            </button>
            <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
            <span
              class="name"
              class:stopped={$stoppedPlugins.has(s.id)}
              onclick={() => openSlot(s)}
              title={$stoppedPlugins.get(s.id) ?? info?.description ?? (s.effect.startsWith("vst3:") ? "This plug-in is not installed (or not scanned yet): kept in the file, skipped on playback. See Effects > Plug-in Manager." : "Not a built-in effect: kept in the file, skipped on playback.")}
            >
              {info?.name ?? s.effect}
            </span>
            {#if info?.plugin}
              <button type="button" class="mini" title="Show its controls as a list (and restart it if it stopped)" aria-label="Show its controls" onclick={() => openWindow(s.id)}>☰</button>
            {/if}
            <button type="button" class="mini" title={info?.plugin && info.hasWindow ? "Open the plug-in's window" : "Open its settings"} aria-label={info?.plugin && info.hasWindow ? "Open the plug-in's window" : "Open its settings"} onclick={() => openSlot(s)}>⚙</button>
            <button type="button" class="mini" title="Remove" aria-label="Remove" onclick={() => commit(`Remove ${info?.name ?? "Effect"}`, removeSlot(chain, s.id))}>✕</button>
            <button type="button" class="mini arrow" title="Effect menu" aria-label="Effect menu for slot {i + 1}" aria-haspopup="menu" onclick={(e) => openSlotArrow(e, s, i)}>▸</button>
          {:else}
            <!-- One wide button: it opens the same menu the row's arrow did. -->
            <button type="button" class="add" aria-haspopup="menu" title="Add an effect in slot {i + 1}" onclick={(e) => openAddMenu(e, i)}>+ Add effect <span class="chev" aria-hidden="true">▾</span></button>
          {/if}
        </li>
      {/each}
    </ol>

    {#if chain.slots.length === 0}
      <p class="hint pad">Add an effect or pick a preset. Effects run top to bottom. Each effect opens in its own window.</p>
    {/if}

    <!-- In and out levels and the mix (and in the Waveform editor, Apply), pinned to the bottom over the effects list. -->
      <div class="levels pinned">
      {#snippet bars(name: string, k: number)}
          <div class="mbars" aria-label="{name} level">
            {#each [k, k + 1] as ch (ch)}
              {@const m = chainMeter?.[ch]}
              <div class="mbar" role="meter" aria-label="{name} {ch % 2 === 0 ? 'left' : 'right'}" aria-valuemin={-60} aria-valuemax={0} aria-valuenow={m && m.peak > 0 ? Math.max(-60, Math.round(20 * Math.log10(m.peak))) : -60}>
                <div class="mcover" style:width="{(1 - meterFraction(m?.level ?? 0)) * 100}%"></div>
                {#if m && m.peak > 0}<div class="mpeak" style:left="{meterFraction(m.peak) * 100}%"></div>{/if}
              </div>
            {/each}
          </div>
        {/snippet}
        <div class="iopair">
        {#each [[GAIN("input_gain_db", "Input"), 0], [GAIN("output_gain_db", "Output"), 2]] as const as [p, k] (p.key)}
          {@const key = p.key as "input_gain_db" | "output_gain_db"}
          {@const cur = draft?.key === `chain.${key}` ? draft.value : chain[key]}
          <div class="io">
            <span class="pname">{k === 0 ? "In" : "Out"}</span>
            {@render bars(k === 0 ? "In" : "Out", k)}
            <Knob
              label="{p.name} gain"
              value={toSlider(p, cur)}
              home={toSlider(p, 0)}
              text={formatParam(p, cur)}
              title="Drag to turn (Ctrl for fine, Shift for coarse). Double-click for 0 dB."
              oninput={(t) => {
                const v = fromSlider(p, t);
                dragChain(key, `Audio FX ${p.name} Gain`, v, v);
              }}
              oncommit={(t) => setChain(key, `Audio FX ${p.name} Gain`, fromSlider(p, t))}
            />
            <span class="val">{formatParam(p, cur)}</span>
          </div>
        {/each}
        </div>
        <div class="io mixrow">
          <span class="pname" title="How much of the effected sound you hear">Mix</span>
          <span class="end">Dry</span>
          <input
            type="range"
            min="0"
            max="100"
            step="1"
            aria-label="Mix"
            value={mixShown}
            oninput={(e) => dragChain("mix", "Audio FX Mix", Number(e.currentTarget.value), Number(e.currentTarget.value) / 100)}
            onchange={(e) => setChain("mix", "Audio FX Mix", Number(e.currentTarget.value) / 100)}
            ondblclick={() => setChain("mix", "Audio FX Mix", 1)}
            title="Double-click for 100 %"
          />
          <span class="end">Wet</span>
          <span class="val">{formatParam(MIX, mixShown)}</span>
        </div>
        {#if inWave}
        <div class="apply">
            <p class="hint">You hear this chain when you play the file. The file changes only when you apply it, and Undo takes it back.</p>
            <div class="line">
              <button type="button" class="primary" disabled={!onApplyWave || applying || !chain.enabled || chain.slots.length === 0} onclick={() => void applyWave()}>
                {applying ? "Applying…" : waveSel ? "Apply to Selection" : "Apply to Whole File"}
              </button>
              <button type="button" disabled={applying || chain.slots.length === 0} onclick={() => commit("Clear", chainOf(null))}>Clear Preview</button>
            </div>
          </div>
        {/if}
      </div>

    {#if winSlot}
      <div class="fxwin" class:howto use:portal bind:this={winEl} style:left="{winBox.x}px" style:top="{winBox.y}px" role="dialog" aria-label="{winInfo?.name ?? winSlot.effect} settings" tabindex="-1" onkeydown={(e) => e.key === "Escape" && (winOpen = false)}>
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <header onpointerdown={dragWindow}>
          <span class="badge" style:background={winInfo ? (CATEGORY_COLORS[winInfo.category] ?? "#888") : "#666"}></span>
          <span class="wtitle">{winInfo?.name ?? winSlot.effect}{winSlot.preset ? ` · ${winSlot.preset}` : ""}</span>
          <button type="button" class="mini" class:on={winSlot.enabled} title={winSlot.enabled ? "Bypass" : "Turn on"} aria-pressed={winSlot.enabled} onclick={() => commit(winSlot.enabled ? "Bypass Effect" : "Turn On Effect", setSlot(chain, winSlot.id, { enabled: !winSlot.enabled }))}>{winSlot.enabled ? "On" : "Off"}</button>
          <button type="button" class="mini" class:on={routingOpen} aria-pressed={routingOpen} title="Channel routing: which of the chain's channels this effect hears and writes to" aria-label="Channel routing" onclick={() => (routingOpen = !routingOpen)}>
            <svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true"><path d="M2 4h5l3 8h4M2 12h5l3-8h4" fill="none" stroke="currentColor" stroke-width="1.4" /></svg>
          </button>
          <button type="button" class="mini info" class:on={howto} aria-pressed={howto} title="How to use this effect" aria-label="How to use this effect" onclick={() => (howto = !howto)}>i</button>
          <button type="button" class="mini" title="Close (Esc)" aria-label="Close" onclick={() => (winOpen = false)}>✕</button>
        </header>
        <div class="editor">
        {#if howto && winInfo?.description}<p class="about">{winInfo.description}</p>{/if}
      {#if winSlot && winInfo}
        {#if routingOpen}
          <ChannelRouting slot={winSlot} info={winInfo} chain={chainNames} {prefs} {ask} {onNotice} onChange={(routing, label) => commit(label, setSlot(chain, winSlot.id, { routing }))} />
        {/if}
        <EffectPresetBar slot={winSlot} info={winInfo} {prefs} {ask} {onNotice} {onError} onChange={(change, label) => commit(label, setSlot(chain, winSlot.id, change))} />
                {#if winSlot.effect === "altaudio.parametric_eq"}
          {#await import("./EqGraph.svelte") then { default: EqGraph }}
          <EqGraph
            slot={winSlot}
            info={winInfo}
            playing={!!ts?.playing}
            playhead={ts?.playing ? ts.position : ((inWave ? waveDoc : doc)?.view.playhead ?? 0)}
            wave={inWave}
            onParams={(changes, label, done) => dragParams(winSlot, changes, label, done)}
          />
          {/await}
        {:else if winSlot.effect === "altaudio.shaped_noise"}
          {#await import("./NoiseShapeGraph.svelte") then { default: NoiseShapeGraph }}
          <NoiseShapeGraph
            slot={winSlot}
            info={winInfo}
            playing={!!ts?.playing}
            playhead={ts?.playing ? ts.position : ((inWave ? waveDoc : doc)?.view.playhead ?? 0)}
            wave={inWave}
            onParams={(changes, label, done) => dragParams(winSlot, changes, label, done)}
          />
          {/await}
        {:else if winSlot.effect === "altaudio.opto_leveler" || winSlot.effect === "altaudio.tube_preamp"}
          {#await import("./TubePanel.svelte") then { default: TubePanel }}
          <TubePanel slot={winSlot} info={winInfo} onParams={(changes, label, done) => dragParams(winSlot, changes, label, done)} />
          {/await}
        {:else if winSlot.effect === "altaudio.stereo_width"}
          {#await import("./WidthPanel.svelte") then { default: WidthPanel }}
          <WidthPanel slot={winSlot} info={winInfo} onParams={(changes, label, done) => dragParams(winSlot, changes, label, done)} />
          {/await}
        {:else if winSlot.effect === "altaudio.vox_strip"}
          {#await import("./VoxStripPanel.svelte") then { default: VoxStripPanel }}
          <VoxStripPanel slot={winSlot} info={winInfo} tempo={doc?.tempo?.bpm} onParams={(changes, label, done) => dragParams(winSlot, changes, label, done)} />
          {/await}
        {:else if winSlot.effect === "altaudio.amp_model"}
          {#await import("./AmpPanel.svelte") then { default: AmpPanel }}
          <AmpPanel
            slot={winSlot}
            info={winInfo}
            rate={(inWave ? waveDoc : doc)?.sample_rate ?? 48000}
            onParams={(changes, label, done) => dragParams(winSlot, changes, label, done)}
            onData={(data, label) => commit(label, setSlot(chain, winSlot.id, { data }))}
            {onError}
          />
          {/await}
        {:else if winSlot.effect === "altaudio.limiter"}
          {#await import("./LimiterPanel.svelte") then { default: LimiterPanel }}
          <LimiterPanel slot={winSlot} info={winInfo} onParams={(changes, label, done) => dragParams(winSlot, changes, label, done)} />
          {/await}
        {:else if winSlot.effect === "altaudio.channel_mixer"}
          {#await import("./MixerPanel.svelte") then { default: MixerPanel }}
          <MixerPanel slot={winSlot} info={winInfo} onParams={(changes, label, done) => dragParams(winSlot, changes, label, done)} />
          {/await}
        {:else if winSlot.effect === "altaudio.echo"}
          {#await import("./DelayPanel.svelte") then { default: DelayPanel }}
          <DelayPanel slot={winSlot} info={winInfo} tempo={doc?.tempo?.bpm} rate={(inWave ? waveDoc : doc)?.sample_rate ?? 48000} onParams={(changes, label, done) => dragParams(winSlot, changes, label, done)} />
          {/await}
        {:else if winSlot.effect === "altaudio.modulation"}
          {#await import("./ModPanel.svelte") then { default: ModPanel }}
          <ModPanel slot={winSlot} info={winInfo} onParams={(changes, label, done) => dragParams(winSlot, changes, label, done)} />
          {/await}
        {:else if winSlot.effect === REVERB_ID}
          {#await import("./VerbPanel.svelte") then { default: VerbPanel }}
          <VerbPanel
            slot={winSlot}
            info={winInfo}
            onParams={(changes, label, done) => dragParams(winSlot, changes, label, done)}
            onImpulse={() => commit("Reverb: Impulse Response", setSlot(chain, winSlot.id, { effect: IMPULSE_ID, params: toImpulse(winSlot.params) }))}
          />
          {/await}
        {:else if winSlot.effect === "altaudio.convolution_reverb"}
          {#await import("./ReverbPanel.svelte") then { default: ReverbPanel }}
          <ReverbPanel
            onKind={(kind) => commit(`Reverb: ${REVERB_KINDS[kind]}`, setSlot(chain, winSlot.id, { effect: REVERB_ID, params: fromImpulse(winSlot.params, kind), data: null }))}
            slot={winSlot}
            info={winInfo}
            onParams={(changes, label, done) => dragParams(winSlot, changes, label, done)}
            onData={(data, label) => commit(label, setSlot(chain, winSlot.id, { data }))}
            {onError}
          />
          {/await}
        {:else if winSlot.effect === "altaudio.center_extractor"}
          {#await import("./CenterPanel.svelte") then { default: CenterPanel }}
          <CenterPanel slot={winSlot} info={winInfo} onParams={(changes, label, done) => dragParams(winSlot, changes, label, done)} />
          {/await}
        {:else if winSlot.effect === "altaudio.notch"}
          {#await import("./NotchGraph.svelte") then { default: NotchGraph }}
          <NotchGraph slot={winSlot} info={winInfo} onParams={(changes, label, done) => dragParams(winSlot, changes, label, done)} />
          {/await}
        {:else if winSlot.effect === PHASE_CURVE}
          {#await import("./PhaseCurveGraph.svelte") then { default: PhaseCurveGraph }}
          <PhaseCurveGraph slot={winSlot} info={winInfo} onParams={(changes, label, done) => dragParams(winSlot, changes, label, done)} />
          {/await}
        {:else if GRAPHED.has(winSlot.effect)}
          {#await import("./ResponseGraph.svelte") then { default: ResponseGraph }}
          <ResponseGraph slot={winSlot} info={winInfo} onPoint={(pf, f, pg, g, done) => dragPoint(winSlot, pf, f, pg, g, done)} />
          {/await}
        {/if}
        {#if BAND_EQS.includes(winSlot.effect)}
          {#await import("./BandEqPanel.svelte") then { default: BandEqPanel }}
          <BandEqPanel
            slot={winSlot}
            info={winInfo}
            onParams={(changes, label, done) => dragParams(winSlot, changes, label, done)}
            onKind={(id) => {
              const to = byId.get(id);
              if (to) commit(`Band EQ: ${to.name}`, setSlot(chain, winSlot.id, { effect: id, params: carryBands(winInfo, winSlot.params, to) }));
            }}
          />
          {/await}
        {/if}
        {#if winInfo.sidechain && onTrackTab && doc && keyOwner}
          <label class="param" title="Sidechain: let another track's or bus's sound drive this effect, for example to duck music under a voice. It listens before that track's fader.">
            <span class="pname">Listen to</span>
            <select
              value={winSlot.sidechain ?? ""}
              onchange={(e) => commit("Set Sidechain", setSlot(chain, winSlot.id, { sidechain: e.currentTarget.value || null }))}
            >
              <option value="">Its own input</option>
              {#each keyChoices(doc, keyOwner) as k (k.id)}<option value={k.id}>{k.bus ? "Bus: " : ""}{k.name}</option>{/each}
            </select>
          </label>
        {/if}
        {#if winInfo.plugin}
          <div class="learn">
            {#if $stoppedPlugins.has(winSlot.id)}
              <span class="hint stopped">{$stoppedPlugins.get(winSlot.id)}</span>
              <button type="button" onclick={() => restartPlugin(winSlot)}>Restart Plug-in</button>
            {:else}
              <button type="button" disabled={!winInfo.hasWindow} title={winInfo.hasWindow ? "Open the plug-in's own window. What you set there is saved with the session." : "This plug-in has no window of its own; use the controls below."} onclick={() => openPluginWindow(winSlot)}>Open Plug-in Window</button>
              <span class="hint how">{winInfo.hasWindow ? "Its own window has every setting. Controls below move it too." : "Controls only."}</span>
            {/if}
          </div>
        {/if}
        {#if LEARNS.has(winSlot.effect)}
          {@const len = winSlot.params.printlen ?? 0}
          {@const what = winSlot.effect === "altaudio.sound_eraser" ? "sound" : "noise"}
          <div class="learn">
            <button type="button" disabled={!learnSel || learning || !canLearn} title={!canLearn ? `Learn on a track or clip, or in the Waveform editor; a bus or the master has no ${what} of its own to learn from` : learnSel ? `Learn the ${what} in the time selection` : `Select a stretch with only the ${what} in it first`} onclick={() => void learnPrint(winSlot)}>
              {learning ? "Learning…" : "Learn from Selection"}
            </button>
            <span class="hint">
              {#if len > 0}{what === "sound" ? "Sound" : "Noise"} learned from {len.toFixed(2)} s.{:else if what === "sound"}Nothing learned yet: select the sound to remove, as clean as you can find it, then Learn. Until then it changes nothing.{:else}Nothing learned yet: select a pause with only the noise, then Learn. Until then it changes nothing.{/if}
            </span>
          </div>
        {/if}
        {#if !OWN_CONTROLS.has(winSlot.effect)}<EffectControls
          info={winInfo}
          value={(p) => shown(winSlot, p)}
          shown={(g) => groupShown(winSlot.effect, winSlot, g)}
          oninput={(p, v) => drag(winSlot, p, v)}
          oncommit={(p, v) => setValue(winSlot, p, v)}
        />{/if}
      {:else}
        <p class="hint">This effect is not built in. It stays in the file and is skipped on playback.</p>
      {/if}
          {#if !winInfo}<p class="hint">This effect is not built in. It stays in the file and is skipped on playback.</p>{/if}
        </div>
        <footer>
          <button
            type="button"
            class="power"
            class:on={winSlot?.enabled}
            title={winSlot?.enabled ? "On: click to bypass (the same switch as on the rack)" : "Bypassed: click to turn on"}
            aria-label={winSlot?.enabled ? "Bypass effect" : "Turn on effect"}
            aria-pressed={winSlot?.enabled}
            onclick={() => winSlot && commit(winSlot.enabled ? "Bypass Effect" : "Turn On Effect", setSlot(chain, winSlot.id, { enabled: !winSlot.enabled }))}
          >
            <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M8 2v6" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" /><path d="M4.6 4.4a5 5 0 1 0 6.8 0" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" /></svg>
            {winSlot?.enabled ? "On" : "Bypassed"}
          </button>
          {#if inWave && $trialSlot === winSlot?.id}
            <span class="grow">Heard on the file while this window is open.</span>
            <button type="button" class="primary" disabled={applying || !onApplyWave} title={waveSel ? "Write this effect into the selection" : "Write this effect into the whole file"} onclick={() => void applyTrial()}>{applying ? "Applying…" : "Apply"}</button>
            <button type="button" onclick={() => (winOpen = false)}>Close</button>
          {:else}
            <span>{label} · Slot {winIndex + 1}</span>
          {/if}
        </footer>
      </div>
    {/if}

  {/if}
</section>

{#if slotMenu}
  <ContextMenu x={slotMenu.x} y={slotMenu.y} items={slotMenu.items} onClose={() => (slotMenu = null)} />
{/if}

<style>
  .vc {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    min-width: 0;
    overflow-x: hidden;
    overflow-y: auto;
    background: var(--surface-1);
    font-size: 12px;
  }
  .tabs {
    display: flex;
    border-bottom: 1px solid var(--border);
  }
  .tabs button {
    flex: 1;
    padding: 4px 6px;
    background: none;
    border: 0;
    border-bottom: 2px solid transparent;
    color: var(--text-dim);
    font: inherit;
  }
  .tabs button.on {
    color: var(--text);
    border-bottom-color: var(--rack, var(--accent));
  }
  .top {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: 4px;
    padding: 6px 8px;
    border-bottom: 1px solid var(--border);
    /* The chain's owner color along the top, so you can see whose effects these are. */
    box-shadow: inset 3px 0 0 var(--rack, var(--accent));
    background: color-mix(in srgb, var(--surface-1) 92%, var(--rack, var(--accent)) 8%);
  }
  .line {
    display: flex;
    min-width: 0;
    gap: 4px;
    align-items: center;
  }
  .grow {
    flex: 1;
    min-width: 0;
  }
  .del {
    width: 70px;
  }
  .target {
    overflow: hidden;
    color: var(--text-dim);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  select,
  button {
    background: var(--surface-0);
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    color: var(--text);
    font: inherit;
    padding: 1px 4px;
  }
  button:disabled {
    opacity: 0.45;
  }
  .power {
    display: grid;
    width: 24px;
    height: 22px;
    padding: 0;
    place-items: center;
    color: var(--text-faint);
  }
  .power.on {
    color: var(--accent);
  }
  .prerender {
    display: flex;
    flex: none;
    gap: 4px;
    height: 22px;
    padding: 0 6px;
    align-items: center;
    color: var(--text-dim);
    white-space: nowrap;
  }
  .prerender.on {
    color: var(--accent);
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 22%, transparent);
  }
  .prerender.ready {
    background: color-mix(in srgb, var(--accent) 38%, transparent);
    font-weight: 600;
  }
  .prerender.busy svg {
    animation: prerender-pulse 1s ease-in-out infinite alternate;
  }
  @keyframes prerender-pulse {
    to {
      opacity: 0.35;
    }
  }
  .prerender svg {
    width: 14px;
    height: 14px;
  }
  .power svg {
    width: 14px;
    height: 14px;
  }
  .slots {
    margin: 0;
    padding: 4px 0;
    list-style: none;
    border-bottom: 1px solid var(--border);
  }
  .slots.off {
    opacity: 0.55;
  }
  .slots li {
    display: flex;
    gap: 3px;
    align-items: center;
    height: 22px;
    padding: 0 8px;
  }
  .slots li.sel {
    box-shadow: inset 2px 0 0 var(--rack, var(--accent));
    background: var(--surface-2);
  }
  .slots li.bypassed .name {
    color: var(--text-faint);
    text-decoration: line-through;
  }
  .n {
    width: 16px;
    color: var(--text-faint);
    font-variant-numeric: tabular-nums;
    text-align: right;
  }
  .name {
    display: flex;
    flex: 1;
    gap: 6px;
    min-width: 0;
    overflow: hidden;
    align-items: center;
    cursor: pointer;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .pow {
    flex: none;
    display: grid;
    place-items: center;
    width: 20px;
    height: 20px;
    padding: 0;
    border: none;
    background: none;
    color: var(--text-faint);
    cursor: pointer;
  }
  .pow svg {
    width: 14px;
    height: 14px;
  }
  .pow.on {
    color: #5fd38d;
  }
  .slots li.over {
    box-shadow: inset 0 2px 0 var(--accent);
  }
  .slots li:has(.pow) {
    cursor: grab;
  }
  .add {
    display: flex;
    flex: 1;
    justify-content: space-between;
    color: var(--text-dim);
    text-align: left;
  }
  /* Every empty slot is a dashed box that says Add effect. */
  .add {
    height: 18px;
    border: 1px dashed color-mix(in srgb, var(--border-strong) 80%, transparent);
    border-radius: 2px;
    background: color-mix(in srgb, var(--surface-0) 60%, transparent);
  }
  .add:hover,
  .add:focus-visible {
    border-color: var(--rack, var(--accent));
    border-style: solid;
  }
  /* Centered both ways whatever the theme's own button padding. */
  .mini {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    box-sizing: border-box;
    min-width: 20px;
    height: 18px;
    padding: 0 4px !important;
    font-size: 10px;
    line-height: 1;
  }
  .mini.on {
    color: var(--accent);
  }
  .fxwin {
    position: fixed;
    z-index: 900;
    width: 380px;
    /* Graph editors (the Parametric EQ) get room to work in. */
    &:has(:global(.pc)) {
      width: 560px;
    }
    /* The Parametric EQ's band table needs a column per band. */
    &:has(:global(.eqwin)) {
      width: min(780px, 96vw);
    }
    /* A Band EQ fits its row of sliders. */
    &:has(:global(.beq)) {
      width: auto;
      min-width: 380px;
      max-width: 96vw;
    }
    /* Hardware faceplates (Delay and Echo, Modulation, Reverb). */
    &:has(:global(.fp-unit)) {
      width: min(660px, 96vw);
    }
    &:has(:global(.cce)) {
      width: min(560px, 96vw);
    }
    &:has(:global(.notchwin)) {
      width: min(640px, 96vw);
    }
    &:has(:global(.ns)) {
      width: 660px;
    }
    &:has(:global(.width)) {
      width: min(560px, 96vw);
    }
    &:has(:global(.vox)) {
      width: min(860px, 96vw);
    }
    &:has(:global(.tube)),
    &:has(:global(.amp)) {
      width: 620px;
    }
    max-width: calc(100vw - 16px);
    max-height: calc(100vh - 40px);
    display: flex;
    flex-direction: column;
    background: var(--surface-2);
    color: var(--text);
    border: 1px solid var(--border);
    border-radius: 6px;
    box-shadow: 0 8px 28px rgba(0, 0, 0, 0.45);
    font-size: 12px;
  }
  .fxwin header {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 6px 5px 10px;
    border-bottom: 1px solid var(--border);
    cursor: move;
    user-select: none;
  }
  .fxwin .wtitle {
    flex: 1;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  /* One column: the presets, then the effect's own panel at full width. */
  .fxwin .editor {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-height: 0;
    padding: 8px 10px;
    overflow: auto;
  }
  .fxwin footer {
    display: flex;
    gap: 10px;
    align-items: center;
    padding: 4px 10px;
    border-top: 1px solid var(--border);
    color: var(--text-dim);
    font-size: 11px;
  }
  .pad {
    padding: 6px 8px;
  }
  .fxwin footer .power {
    display: inline-flex;
    gap: 5px;
    align-items: center;
    justify-content: center;
    box-sizing: border-box;
    height: 22px;
    padding: 0 9px 0 7px !important;
    line-height: 1;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: transparent;
    color: var(--text-dim);
    font: inherit;
    cursor: pointer;
  }
  .fxwin footer .grow {
    flex: 1;
  }
  .fxwin footer .primary {
    border-color: var(--accent);
    background: var(--accent);
    color: var(--on-accent, #fff);
  }
  .fxwin footer .power svg {
    display: block;
    flex: none;
    width: 14px;
    height: 14px;
  }
  .fxwin footer .power.on {
    border-color: color-mix(in srgb, var(--ok, #3fb950) 60%, transparent);
    color: var(--ok, #3fb950);
  }
  .param {
    display: grid;
    grid-template-columns: 88px minmax(0, 1fr) 72px;
    gap: 6px;
    align-items: center;
  }
  .pname {
    overflow: hidden;
    color: var(--text-dim);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .val {
    font-variant-numeric: tabular-nums;
    text-align: right;
  }
  input[type="range"] {
    width: 100%;
  }
  .empty,
  .hint {
    margin: 0;
    color: var(--text-faint);
  }
  .learn {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    align-items: center;
  }
  .tabs[hidden] {
    display: none;
  }
  .apply {
    display: grid;
    gap: 4px;
    padding-top: 4px;
    border-top: 1px solid var(--border);
  }
  .apply .primary {
    border-color: var(--accent);
    background: var(--accent);
    color: var(--on-accent, #fff);
  }
  .io {
    display: grid;
    grid-template-columns: 52px minmax(0, 1fr) 28px 64px;
    gap: 6px;
    align-items: center;
  }
  .io.mixrow {
    grid-template-columns: 52px auto minmax(0, 1fr) auto 44px;
  }
  /* In and out levels and the mix, condensed under the presets. */
  .levels {
    display: grid;
    gap: 1px;
    padding-top: 2px;
  }
  .levels.pinned {
    position: sticky;
    bottom: 0;
    z-index: 2;
    margin-top: auto;
    padding: 5px 8px 6px;
    border-top: 1px solid var(--border);
    background: var(--surface-1);
    box-shadow: 0 -4px 8px rgba(0, 0, 0, 0.25);
  }
  .top > .target {
    overflow: hidden;
    color: var(--text-dim);
    font-size: 11px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .levels .iopair {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }
  .levels .io {
    grid-template-columns: 22px minmax(0, 1fr) 20px 46px;
    gap: 4px;
    font-size: 11px;
  }
  .levels .io.mixrow {
    grid-template-columns: 22px auto minmax(0, 1fr) auto 40px;
  }
  .end {
    color: var(--text-faint);
  }
  .io input[type="range"] {
    appearance: none;
    height: 4px;
    margin: 0;
    border-radius: 2px;
    background: var(--surface-3);
    outline-offset: 4px;
  }
  .io input[type="range"]::-webkit-slider-thumb {
    appearance: none;
    width: 12px;
    height: 12px;
    border: 1px solid var(--border-strong);
    border-radius: 50%;
    background: var(--text-dim);
  }
  .io input[type="range"]:hover::-webkit-slider-thumb {
    background: var(--accent);
  }
  .mbars {
    display: grid;
    gap: 2px;
  }
  .mbar {
    position: relative;
    height: 5px;
    overflow: hidden;
    background: linear-gradient(to right, #3d8f6a 0 var(--meter-yellow, 80%), #c9b33f var(--meter-yellow, 80%) var(--meter-red, 95%), #e5534b var(--meter-red, 95%) 100%);
  }
  .mcover {
    position: absolute;
    top: 0;
    right: 0;
    height: 100%;
    background: var(--surface-0);
  }
  .mpeak {
    position: absolute;
    top: 0;
    width: 1px;
    height: 100%;
    background: var(--text);
  }
  .empty {
    padding: 12px 8px;
  }
  .name.stopped,
  .hint.stopped {
    color: var(--danger);
  }
  /* How-to text stays hidden until the (i) in the title bar is on. */
  .fxwin:not(.howto) :global(.how) {
    display: none;
  }
  .mini.info {
    font-family: Georgia, "Times New Roman", serif;
    font-style: italic;
    font-weight: 700;
    min-width: 18px;
    width: 18px;
    border-radius: 50% !important;
  }
  .about {
    margin: 0 0 6px;
    padding: 6px 8px;
    border-left: 2px solid var(--accent);
    background: var(--surface-1);
    color: var(--text-dim);
    font-size: var(--font-size-small);
    line-height: 1.4;
  }
</style>
