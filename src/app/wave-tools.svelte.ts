// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * The Waveform editor's commands: edits on the selection (or the whole
 * file), copy and paste, Save and Save As, opening files, effects applied
 * into the file, spectral edits, and their menu actions.
 */

import { type ClipBoard } from "../panels/editor/clip-edit";
import { rasterize } from "../panels/editor/spectrum";
import { type ChannelTake, channelLabel, editTake, oneChannel, pasteTake, soloChannel, someOff, waveChannels } from "../panels/editor/wave-channels";
import { type WaveBoard, amplifyRange, clampRange, copyRange, cropTo, deleteRange, fadeRange, mixPasteAt, pasteAt, replaceRange, silenceRange, stretchRange } from "../panels/editor/wave-edit";
import { embedList } from "../panels/markers/chapters";
import { newMarker, nextMarkerName } from "../panels/markers/markers";
import { chainOf } from "../panels/voice-chain/chain";
import { editor } from "../stores/editor";
import { exactSamples } from "../stores/exact-samples";
import { peaks, session, transport } from "../stores/open-session";
import { type ClipDoc, type EffectChainDoc, type Op, sessionEnd } from "../stores/session";
import { settings } from "../stores/settings";
import { type TransportStore } from "../stores/transport";
import { closeWaveFile, openWaveFile, saveWaveFile, savesInPlace, spectralSelection, wave, waveFile, waveHistory, waveTransport, waveZoom } from "../stores/wave";
import type { ActionRegistry } from "./actions";
import { exportEmbedChapters, exportPickPath, waveApplyChain, waveChannelEdit, wavePeak, waveReverseInvert, waveSpectralEdit, waveStretch } from "./core";
import type { DockController } from "./docking/dock";
import { kindOfPath } from "./output-format";
import { listen } from "@tauri-apps/api/event";
import { get } from "svelte/store";

/** What the Waveform editor's commands need from the rest of the app. */
export interface WaveToolsDeps {
  actions: ActionRegistry;
  dock: DockController;
  showError: (e: unknown) => void;
  showNotice: (text: string) => void;
  ask: (title: string, message: string, buttons: { label: string; value: string; primary?: boolean }[]) => Promise<string>;
  promptText: (title: string, label: string, value: string) => Promise<string | null>;
  uuid: () => string;
  /** The Waveform editor is the one in front. */
  inWave: () => boolean;
  currentArea: () => string | null;
  selectedClipDocs: () => ClipDoc[];
  /** What the Multitrack's Copy holds, if anything. */
  clipBoard: () => ClipBoard | null;
  /** The progress dialog's state. */
  exporting: () => Exporting | null;
  setExporting: (v: Exporting | null) => void;
  /** Recording now. */
  recording: () => boolean;
  finishRecord: () => Promise<void>;
  spaceToggle: (t: TransportStore) => Promise<void>;
  /** The session follows a file saved from the Waveform editor. */
  syncSessionMedia: (path: string) => Promise<void>;
  askNewAudio: () => void;
  pasteToNewFile: () => Promise<void>;
}

/** The progress dialog for long work. */
export interface Exporting {
  title: string;
  progress: number;
  floating?: boolean;
}

/** "30", "30.5", "0:30.5" or "1:02:03.5" as seconds; null when it is not a time. */
export function parseSeconds(text: string): number | null {
  const parts = text.trim().split(":").map((p) => p.trim());
  if (parts.length === 0 || parts.length > 3 || parts.some((p) => p === "" || !/^\d*\.?\d+$/.test(p))) return null;
  return parts.reduce((acc, p) => acc * 60 + Number(p), 0);
}


export function createWaveTools(deps: WaveToolsDeps) {
  const { actions, dock, showError, showNotice, ask, promptText, uuid, inWave, currentArea, selectedClipDocs, clipBoard, exporting, setExporting, recording, finishRecord, spaceToggle, syncSessionMedia, askNewAudio, pasteToNewFile } = deps;

  const waveIsOpen = () => inWave() && waveFile.current() !== null;
  const waveFocus = () => waveIsOpen() && currentArea() !== "files" && currentArea() !== "libraries";
  const waveSel = (): [number, number] | null => {
    const d = wave.current;
    const s = d?.view.selection;
    return d && s ? clampRange(d, s.start, s.end) : null;
  };
  const waveAll = (): [number, number] | null => {
    const d = wave.current;
    return d ? (waveSel() ?? clampRange(d, 0, sessionEnd(d))) : null;
  };
  const NEED_WAVE_SEL = "Drag across the waveform to select audio first.";
  let waveBoard: WaveBoard | null = null;
  /** Copy in the Waveform editor: with one channel on, the copy remembers which. */
  function copyOf(r: readonly [number, number]): WaveBoard | null {
    const b = copyRange(wave.current!, r[0], r[1]);
    const one = (wave.current?.media[0]?.channels ?? 1) > 1 ? oneChannel(get(waveChannels)) : null;
    return b && one !== null ? { ...b, channel: one } : b;
  }

  /**
   * Runs a Waveform editor edit. With `range` and only one channel of a
   * stereo file on, the edit reaches that channel alone (or as `take`
   * says): the core renders the range and the other channel keeps what
   * it had.
   */
  async function waveEdit(label: string, op: Op | null, range?: readonly [number, number], take?: ChannelTake | null): Promise<boolean> {
    if (!op) return false;
    const d = wave.current;
    const how = take !== undefined ? take : editTake(get(waveChannels));
    if (range && how && d && (d.media[0]?.channels ?? 1) > 1) {
      try {
        const res = await waveChannelEdit(op, range[0], range[1], how);
        const one = replaceRange(wave.current ?? d, res.start, res.end, res.media, label, uuid);
        if (!one) return false;
        op = one;
        label += channelLabel(get(waveChannels));
      } catch (e) {
        showError(e);
        return false;
      }
    }
    await wave.edit(label, op, waveHistory);
    return true;
  }
  /** Whether some channels of the Waveform editor's file are switched off. */
  const oneWaveChannel = () => (wave.current?.media[0]?.channels ?? 1) > 1 && someOff(get(waveChannels));

  function waveSetSelection(sel: [number, number] | null, playhead?: number) {
    const d = wave.current;
    if (!d) return;
    wave.setView({
      ...d.view,
      selection: sel ? { start: sel[0], end: sel[1] } : null,
      playhead: Math.max(0, Math.round(playhead ?? d.view.playhead)),
    });
  }

  const wavePlayhead = () => (waveTransport.current.playing ? waveTransport.current.position : (wave.current?.view.playhead ?? 0));
  const waveName = () => waveFile.current()?.path.split(/[\\/]/).pop() ?? "the file";

  /** Asks about unsaved Waveform edits. False means stop. */
  async function confirmWaveDiscard(): Promise<boolean> {
    if (!waveFile.current() || !waveHistory.current.dirty) return true;
    const v = await ask("Unsaved changes", `Save changes to ${waveName()}?`, [
      { label: "Save", value: "save", primary: true },
      { label: "Don't Save", value: "discard" },
      { label: "Cancel", value: "cancel" },
    ]);
    if (v === "cancel") return false;
    if (v === "save") return saveWave(false);
    return true;
  }

  /** Opens a file in the Waveform editor and shows it. */
  async function openInWaveform(path: string) {
    // The file open now is set aside with its edits, not closed.
    await transport.stop();
    try {
      await openWaveFile(path);
    } catch (e) {
      showError(e);
      return;
    }
    editor.setMode("waveform");
    dock.show("editor");
    actions.invalidate();
  }

  /** Saves the Waveform editor's file. Returns false if cancelled or failed. */
  async function saveWave(as: boolean): Promise<boolean> {
    const f = waveFile.current();
    if (!f || exporting()) return false;
    const cut = Math.max(f.path.lastIndexOf("/"), f.path.lastIndexOf("\\"));
    const folder = cut > 0 ? f.path.slice(0, cut) : null;
    const stemName = waveName().replace(/\.[^.]+$/, "");
    let path: string | null = null;
    if (as || !savesInPlace(f)) {
      const why = !as && !savesInPlace(f) ? `Save ${f.format || "this file"} as WAV, FLAC, MP3 or AAC` : "Save Audio As";
      path = await exportPickPath(stemName, folder, why, kindOfPath(f.path) ?? "wav", true);
      if (!path) return false;
      if (kindOfPath(path) === null) path = `${path}.wav`;
      const lossyKind = kindOfPath(path);
      if (lossyKind === "mp3" || lossyKind === "m4a") {
        const name = lossyKind === "mp3" ? "MP3" : "AAC";
        const ok = await ask(`Save as ${name}?`, `${name} is lossy: fine for publishing, but keep a WAV or FLAC copy if you'll edit it again.`, [
          { label: `Save ${name}`, value: "yes", primary: true },
          { label: "Cancel", value: "no" },
        ]);
        if (ok !== "yes") return false;
      }
    } else if (f.format.toUpperCase() === "MP3") {
      const ok = await ask(
        "Re-encode the MP3?",
        `Saving ${waveName()} encodes it as MP3 again, which loses a little quality each time. Use Save Audio As and pick WAV or FLAC to keep full quality.`,
        [
          { label: "Save MP3", value: "yes", primary: true },
          { label: "Cancel", value: "no" },
        ],
      );
      if (ok !== "yes") return false;
    }
    if (path === null && session.current?.media.some((m) => m.path === f.path)) {
      const ok = await ask(
        "Save over the original?",
        `Clips in the session play ${waveName()}, so saving changes them too. To keep them as they are, cancel and use Clip > Make Unique Copy first, or use Save As.`,
        [
          { label: "Save", value: "yes", primary: true },
          { label: "Cancel", value: "no" },
        ],
      );
      if (ok !== "yes") return false;
    }
    // Save writes the Audio FX chain into the file, unless the chain is bypassed.
    // One undo step takes the applied chain back out.
    const pending = chainOf(wave.current?.tracks[0]?.effects);
    if (pending.enabled && pending.slots.some((x) => x.enabled)) {
      try {
        if (!(await applyWaveChain(pending, true))) return false;
      } catch (e) {
        showError(e);
        return false;
      }
    }
    setExporting({ title: "Saving audio", progress: 0 });
    let off: (() => void) | null = null;
    try {
      off = await listen<{ done: number; total: number }>("export-progress", (e) => {
        const cur = exporting();
        if (cur) setExporting({ ...cur, progress: e.payload.total > 0 ? e.payload.done / e.payload.total : 0 });
      });
      const markerCount = wave.current?.markers.length ?? 0;
      await saveWaveFile(path);
      const saved = waveFile.current()?.path ?? f.path;
      // MP3, FLAC and M4A carry chapters inside the file (WAV keeps its markers as cue points).
      // Written before the peaks and the session read the file again, so they see the finished file.
      const saveDoc = wave.current;
      const chapters = saveDoc && /\.(mp3|flac|m4a)$/i.test(saved) ? embedList(saveDoc.markers, saveDoc.sample_rate, null) : [];
      let added = chapters.length;
      if (saveDoc && added > 0) {
        const totalMs = Math.round((sessionEnd(saveDoc) / saveDoc.sample_rate) * 1000);
        await exportEmbedChapters(saved, chapters, totalMs).catch((e) => {
          added = 0;
          showError(e);
        });
      }
      peaks.invalidate(saved);
      exactSamples.forget(saved);
      await syncSessionMedia(saved);
      const keepsMarkers = /\.wave?$/i.test(saved);
      const left = keepsMarkers ? 0 : markerCount - added;
      const chapterNote = added > 0 ? ` ${added} ${added === 1 ? "chapter" : "chapters"} added.` : "";
      const leftNote = left > 0 ? ` ${left} other ${left === 1 ? "marker stays" : "markers stay"} only while it is open: only WAV files store them.` : "";
      showNotice(`Saved ${saved.split(/[\\/]/).pop()}.${chapterNote}${leftNote}`);
      return true;
    } catch (e) {
      showError(e);
      return false;
    } finally {
      off?.();
      setExporting(null);
      actions.invalidate();
    }
  }

  /** Effects > Pitch and Time > Stretch (Waveform editor): the selection, or the whole file, to a new length. */
  async function waveReverseOrInvert(mode: "reverse" | "invert") {
    const d = wave.current;
    const r = waveAll();
    const t = d?.tracks[0];
    if (!d || !r || !t) return;
    const label = mode === "reverse" ? "Reverse" : "Invert";
    try {
      const res = await waveReverseInvert(r[0], r[1], mode);
      if (wave.current?.tracks[0]?.id !== t.id) return showNotice("You switched files while that was running, so it was not applied.");
      await waveEdit(label, replaceRange(wave.current ?? d, res.start, res.end, res.media, label, uuid), [res.start, res.end]);
    } catch (e) {
      showError(e);
    }
  }

  async function waveStretchTo() {
    const d = wave.current;
    const r = waveAll();
    if (!d || !r) return;
    const rate = d.sample_rate;
    const now = (r[1] - r[0]) / rate;
    const what = waveSel() ? "the selection" : "the whole file";
    const answer = await promptText("Stretch", `New length for ${what}, now ${now.toFixed(3)} s. Seconds, or minutes:seconds. The pitch stays the same.`, now.toFixed(3));
    if (answer === null) return;
    const secs = parseSeconds(answer);
    if (secs === null || secs <= 0) return showNotice("Type a length like 30, 14.99 or 1:02.5.");
    const frames = Math.round(secs * rate);
    if (frames === r[1] - r[0]) return;
    try {
      const res = await waveStretch(r[0], r[1], frames);
      const label = `Stretch to ${secs.toFixed(3)} s`;
      const op = stretchRange(wave.current ?? d, res.start, res.end, res.media, label, uuid);
      if (op) await waveEdit(label, op);
      showNotice(`${label}: ${now.toFixed(3)} s became ${secs.toFixed(3)} s (${((secs / now) * 100).toFixed(1)}%). Changes past about 25% can sound processed.`);
    } catch (e) {
      showError(e);
    }
  }

  async function waveAmplify(db: number) {
    const d = wave.current;
    const r = waveAll();
    if (!d || !r || !db) return;
    await waveEdit(`Amplify ${db > 0 ? "+" : ""}${db.toFixed(1)} dB`, amplifyRange(d, r[0], r[1], db, uuid), r);
  }

  /** Writes the Waveform preview chain into the selection (or the whole file), then clears the preview. */
  async function applyWaveChain(chain: EffectChainDoc, wholeFile = false): Promise<boolean> {
    const d = wave.current;
    const r = wholeFile && d ? clampRange(d, 0, sessionEnd(d)) : waveAll();
    const t = d?.tracks[0];
    if (!d || !r || !t) return false;
    const label = !wholeFile && waveSel() ? "Apply Audio FX to Selection" : "Apply Audio FX";
    if (exporting()) {
      showNotice("Wait for the current export to finish.");
      return false;
    }
    // Plug-ins can take a while, so show how far along it is and about how long is left.
    const startedAt = Date.now();
    const heading = wholeFile || !waveSel() ? "Applying the Audio FX to the file" : "Applying the Audio FX to the selection";
    const timeLeft = (s: number) => (s < 60 ? `${s} s` : `${Math.floor(s / 60)} min ${s % 60} s`);
    setExporting({ title: heading, progress: 0 });
    let off: (() => void) | null = null;
    let res: Awaited<ReturnType<typeof waveApplyChain>>;
    try {
      off = await listen<{ done: number; total: number }>("export-progress", (e) => {
        const cur = exporting();
        if (!cur) return;
        const p = e.payload.total > 0 ? e.payload.done / e.payload.total : 0;
        const elapsed = (Date.now() - startedAt) / 1000;
        const left = p >= 0.05 ? Math.max(1, Math.round((elapsed * (1 - p)) / p)) : null;
        setExporting({ ...cur, progress: p, title: left === null ? heading : `${heading}, about ${timeLeft(left)} left` });
      });
      res = await waveApplyChain(chain, r[0], r[1]);
    } catch (e) {
      if (String(e).includes("Cancelled")) {
        showNotice("Apply cancelled. The file was not changed.");
        return false;
      }
      throw e;
    } finally {
      off?.();
      setExporting(null);
    }
    // The result belongs to the file the job started on. If another file is open now, do not apply it there.
    if (wave.current?.tracks[0]?.id !== t.id) {
      showNotice("You switched files while the effect was running, so it was not applied. Stay on the file until it finishes. To process many files, use Batch Convert.");
      return false;
    }
    await waveEdit(label, replaceRange(wave.current ?? d, res.start, res.end, res.media, label, uuid), [res.start, res.end]);
    // What was applied leaves the preview; anything else in it stays.
    const now = wave.current?.tracks[0]?.effects;
    const rest = (now?.slots ?? []).filter((x) => !chain.slots.some((y) => y.id === x.id));
    await wave.apply({ op: "set_effects", target: { kind: "track", id: t.id }, chain: rest.length ? { ...chainOf(now), slots: rest } : chainOf(null) });
    if (!wholeFile) showNotice(`${label}: done. Undo takes it back.`);
    return true;
  }

  /** Spectral edit on the current spectral selection: turn it down by `db` (-120 removes it), or repair it. */
  async function waveSpectral(mode: "gain" | "repair", db: number, label: string) {
    const shapes = get(spectralSelection);
    const d = wave.current;
    const mask = shapes ? rasterize(shapes) : null;
    // A Spot Repair stroke is used once, not kept as a selection.
    if (editor.current().spectralTool === "spot") spectralSelection.set(null);
    if (!mask || !d) return;
    try {
      const r = await waveSpectralEdit(mask, mode, db);
      await waveEdit(label, replaceRange(wave.current ?? d, r.start, r.end, r.media, label, uuid), [r.start, r.end]);
    } catch (e) {
      showError(e);
    }
  }

  async function waveNormalize() {
    const r = waveAll();
    if (!r) return;
    const v = await promptText("Normalize", "Peak level in dB (for example -1.0)", "-1.0");
    if (v === null) return;
    const target = Number(v);
    if (!Number.isFinite(target) || target > 0) {
      showNotice("Enter a level of 0 dB or below, for example -1.0.");
      return;
    }
    const peak = await wavePeak(r);
    if (peak <= 0) {
      showNotice("The selection is silent, so there is nothing to normalize.");
      return;
    }
    const db = Math.round((target - 20 * Math.log10(peak)) * 100) / 100;
    if (Math.abs(db) < 0.01) {
      showNotice(`Already peaks at ${target.toFixed(1)} dB.`);
      return;
    }
    await waveEdit(`Normalize to ${target.toFixed(1)} dB`, amplifyRange(wave.current!, r[0], r[1], db, uuid), r);
  }

  function registerWaveActions(): (() => void)[] {
    const ov = actions.overlay.bind(actions);
    const NOT_WAVE = "Works in the Waveform editor. Double-click a file in the Files panel to open it there.";
    return [
      actions.register({
        id: "view.editor.multitrack",
        run: () => {
          void waveTransport.stop();
          editor.setMode("multitrack");
          dock.show("editor");
        },
        checked: () => !inWave(),
      }),
      actions.register({ id: "file.new-audio", run: () => askNewAudio() }),
      actions.register({
        id: "view.editor.waveform",
        run: () => {
          // With no file open, switching to the Waveform editor starts a new one.
          if (!waveFile.current()) {
            askNewAudio();
            return;
          }
          void transport.stop();
          editor.setMode("waveform");
          dock.show("editor");
        },
        checked: () => inWave(),
      }),
      ...(["move", "razor", "slip", "time"] as const).map((t) =>
        actions.register({
          id: `tool.${t}`,
          run: () => {
            if (inWave() && t === "time") {
              editor.setSpectralTool("time");
              return;
            }
            if (inWave()) editor.setMode("multitrack");
            editor.setTool(t);
          },
          checked: () => (inWave() ? t === "time" && editor.current().spectralTool === "time" : editor.current().tool === t),
        }),
      ),
      // Spectral tools work on the Waveform editor's spectrum.
      ...(["box", "lasso", "brush", "spot"] as const).map((t) =>
        actions.register({
          id: `tool.${t}`,
          run: () => {
            const e = editor.current();
            if (!e.showSpectrum && !e.showPitch) editor.setShowSpectrum(true);
            editor.setSpectralTool(t);
          },
          enabled: () => waveIsOpen(),
          reason: () => "Open a file in the Waveform editor to use the spectral tools.",
          checked: () => inWave() && editor.current().spectralTool === t,
        }),
      ),
      actions.register({
        id: "view.pitch",
        run: () => editor.setShowPitch(!editor.current().showPitch),
        label: () => (editor.current().showPitch ? "Hide Pitch" : "Show Pitch"),
        checked: () => editor.current().showPitch,
      }),
      actions.register({
        id: "clip.edit-source",
        run: () => {
          const c = selectedClipDocs()[0];
          const m = c ? session.current?.media.find((x) => x.id === c.media_id) : undefined;
          if (m) return openInWaveform(m.path);
        },
        enabled: () => selectedClipDocs().length === 1,
        reason: () => "Select one clip.",
      }),
      // Waveform-only commands: disabled with a reason elsewhere.
      actions.register({
        id: "edit.paste-new",
        run: () => pasteToNewFile(),
        enabled: () => (inWave() ? waveBoard !== null : clipBoard() !== null),
        reason: () => "Copy audio in the Waveform editor, or clips in the Multitrack, first.",
      }),
      actions.register({
        id: "view.spectrum",
        run: () => editor.setShowSpectrum(!editor.current().showSpectrum),
        label: () => (editor.current().showSpectrum ? "Hide Spectrum" : "Show Spectrum"),
        checked: () => editor.current().showSpectrum,
      }),
      ...["fx.spectrum.reduce-12", "fx.spectrum.reduce-24", "fx.spectrum.remove", "fx.spectrum.repair"].map((id) =>
        actions.register({ id, run: () => {}, enabled: () => false, reason: () => NOT_WAVE }),
      ),
      ...["edit.mix-paste", "edit.crop", "fx.invert", "fx.reverse", "fx.silence", "fx.fade-in", "fx.fade-out", "fx.amplify", "fx.normalize"].map((id) =>
        actions.register({ id, run: () => {}, enabled: () => false, reason: () => NOT_WAVE }),
      ),
      ov(waveFocus, {
        id: "edit.undo",
        run: () => waveHistory.undo().then(() => undefined),
        enabled: () => waveHistory.current.canUndo,
        label: () => (waveHistory.current.undoLabel ? `Undo ${waveHistory.current.undoLabel}` : null),
      }),
      ov(waveFocus, {
        id: "edit.redo",
        run: () => waveHistory.redo().then(() => undefined),
        enabled: () => waveHistory.current.canRedo,
        label: () => (waveHistory.current.redoLabel ? `Redo ${waveHistory.current.redoLabel}` : null),
      }),
      ov(waveFocus, {
        id: "edit.select-all",
        run: () => {
          const d = wave.current;
          if (d) waveSetSelection([0, sessionEnd(d)]);
        },
      }),
      ov(waveFocus, { id: "edit.deselect", run: () => waveSetSelection(null), enabled: () => waveSel() !== null, reason: () => "Nothing is selected." }),
      ...(["left", "right"] as const).map((ch) =>
        actions.register({
          id: `wave.channel-${ch}`,
          run: () => waveChannels.update((c) => soloChannel(c, ch === "left" ? 0 : 1)),
          enabled: () => (wave.current?.media[0]?.channels ?? 1) > 1 && inWave(),
          reason: () => "Open a stereo file in the Waveform editor first.",
        }),
      ),
      ov(waveFocus, {
        id: "edit.delete",
        run: async () => {
          const r = waveSel();
          // One channel can't get shorter than the other: deleting from one silences it.
          if (r && oneWaveChannel()) await waveEdit("Delete", silenceRange(wave.current!, r[0], r[1], uuid), r);
          else if (r && (await waveEdit("Delete", deleteRange(wave.current!, r[0], r[1], uuid)))) waveSetSelection(null, r[0]);
        },
        enabled: () => waveSel() !== null,
        label: () => "Delete Selection",
        reason: () => NEED_WAVE_SEL,
      }),
      ov(waveFocus, {
        id: "edit.copy",
        run: () => {
          const r = waveSel();
          if (r) waveBoard = copyOf(r);
          actions.invalidate();
        },
        enabled: () => waveSel() !== null,
        reason: () => NEED_WAVE_SEL,
      }),
      ov(waveFocus, {
        id: "edit.cut",
        run: async () => {
          const r = waveSel();
          if (!r) return;
          waveBoard = copyOf(r);
          if (oneWaveChannel()) await waveEdit("Cut", silenceRange(wave.current!, r[0], r[1], uuid), r);
          else if (await waveEdit("Cut", deleteRange(wave.current!, r[0], r[1], uuid))) waveSetSelection(null, r[0]);
        },
        enabled: () => waveSel() !== null,
        reason: () => NEED_WAVE_SEL,
      }),
      ov(waveFocus, {
        id: "edit.paste",
        run: async () => {
          if (!waveBoard) return;
          const take = pasteTake(get(waveChannels), waveBoard.channel);
          if (take && (wave.current?.media[0]?.channels ?? 1) > 1) {
            // Onto one channel (or a one-channel copy onto both): it goes over the audio from the selection or playhead, keeping the file's length.
            const d = wave.current!;
            const at = waveSel()?.[0] ?? wavePlayhead();
            const len = Math.round(waveBoard.length * (d.sample_rate / waveBoard.rate));
            const end = Math.min(sessionEnd(d), at + len);
            const quiet = silenceRange(d, at, end, uuid);
            const p = mixPasteAt(d, waveBoard, at, 0, uuid);
            if (!quiet || !p || end <= at) return;
            if (await waveEdit("Paste", { op: "batch", ops: [quiet, p.op] }, [at, end], take)) waveSetSelection([at, end], at);
            return;
          }
          // Pasting over a selection replaces it.
          const r = waveSel();
          if (r) await waveEdit("Delete", deleteRange(wave.current!, r[0], r[1], uuid));
          const at = r ? r[0] : wavePlayhead();
          const p = pasteAt(wave.current!, waveBoard, at, uuid);
          if (p && (await waveEdit("Paste", p.op))) waveSetSelection([at, at + p.length], at);
        },
        enabled: () => waveBoard !== null,
        reason: () => "Copy or cut audio in the Waveform editor first.",
      }),
      ov(waveFocus, {
        id: "edit.mix-paste",
        run: async () => {
          if (!waveBoard) return;
          const v = await promptText("Mix Paste", "Level of the pasted audio (dB)", "0.0");
          if (v === null) return;
          const db = Number(v);
          if (!Number.isFinite(db) || Math.abs(db) > 48) {
            showNotice("Enter a number of decibels between -48 and 48.");
            return;
          }
          const at = waveSel()?.[0] ?? wavePlayhead();
          const p = mixPasteAt(wave.current!, waveBoard, at, db, uuid);
          const take = pasteTake(get(waveChannels), waveBoard.channel);
          if (p && take && (wave.current?.media[0]?.channels ?? 1) > 1) {
            // Mixed into one channel only: from the original channel plus the pasted one.
            const end = Math.min(sessionEnd(wave.current!), at + p.length);
            if (end > at && (await waveEdit("Mix Paste", p.op, [at, end], take))) waveSetSelection([at, end], at);
            return;
          }
          if (p && (await waveEdit("Mix Paste", p.op))) waveSetSelection([at, at + p.length], at);
        },
        enabled: () => waveBoard !== null,
        reason: () => "Copy or cut audio in the Waveform editor first.",
      }),
      ov(waveFocus, {
        id: "edit.crop",
        run: async () => {
          const r = waveSel();
          if (r && (await waveEdit("Crop", cropTo(wave.current!, r[0], r[1], uuid)))) waveSetSelection([0, r[1] - r[0]], 0);
        },
        enabled: () => waveSel() !== null,
        reason: () => NEED_WAVE_SEL,
      }),
      ov(waveIsOpen, {
        id: "fx.silence",
        run: async () => {
          const r = waveSel();
          if (r) await waveEdit("Silence", silenceRange(wave.current!, r[0], r[1], uuid), r);
        },
        enabled: () => waveSel() !== null,
        reason: () => NEED_WAVE_SEL,
      }),
      ov(waveIsOpen, {
        id: "fx.fade-in",
        run: async () => {
          const r = waveSel();
          if (r) await waveEdit("Fade In", fadeRange(wave.current!, r[0], r[1], "in", uuid), r);
        },
        enabled: () => waveSel() !== null,
        reason: () => NEED_WAVE_SEL,
      }),
      ov(waveIsOpen, {
        id: "fx.fade-out",
        run: async () => {
          const r = waveSel();
          if (r) await waveEdit("Fade Out", fadeRange(wave.current!, r[0], r[1], "out", uuid), r);
        },
        enabled: () => waveSel() !== null,
        reason: () => NEED_WAVE_SEL,
      }),
      ov(waveIsOpen, {
        id: "fx.amplify",
        run: async () => {
          const v = await promptText("Amplify", waveSel() ? "Change the selection by (dB)" : "Change the whole file by (dB)", "3.0");
          if (v === null) return;
          const db = Number(v);
          if (!Number.isFinite(db) || Math.abs(db) > 48) {
            showNotice("Enter a number of decibels between -48 and 48.");
            return;
          }
          await waveAmplify(db);
        },
      }),
      ov(waveIsOpen, { id: "fx.normalize", run: () => waveNormalize() }),
      ov(waveIsOpen, { id: "fx.invert", run: () => waveReverseOrInvert("invert") }),
      ov(waveIsOpen, { id: "fx.reverse", run: () => waveReverseOrInvert("reverse") }),
      ov(waveIsOpen, { id: "clip.stretch", run: () => waveStretchTo(), label: () => "Stretch Selection or File..." }),
      ...(
        [
          ["fx.spectrum.reduce-12", "gain", -12, "Spectral Reduce 12 dB"],
          ["fx.spectrum.reduce-24", "gain", -24, "Spectral Reduce 24 dB"],
          ["fx.spectrum.remove", "gain", -120, "Spectral Remove"],
          ["fx.spectrum.repair", "repair", 0, "Spot Repair"],
        ] as const
      ).map(([id, mode, db, label]) =>
        ov(waveIsOpen, {
          id,
          run: () => waveSpectral(mode, db, label),
          enabled: () => get(spectralSelection) !== null,
          reason: () => "Show the spectrum and drag a rectangle on it first.",
        }),
      ),
      ov(waveIsOpen, { id: "file.save", run: () => saveWave(false).then(() => undefined), label: () => "Save Audio" }),
      ov(waveIsOpen, { id: "file.save-as", run: () => saveWave(true).then(() => undefined), label: () => "Save Audio As..." }),
      ov(waveIsOpen, {
        id: "file.close",
        run: async () => {
          if (await confirmWaveDiscard()) await closeWaveFile();
        },
        label: () => "Close Audio File",
      }),
      ov(waveIsOpen, {
        id: "edit.marker.cue",
        run: async () => {
          const d = wave.current;
          if (!d) return;
          const m = newMarker(uuid(), nextMarkerName(d.markers, settings.current.markerName), wavePlayhead());
          await waveEdit("Add Marker", { op: "add_markers", markers: [m] });
        },
      }),
      ov(waveIsOpen, {
        id: "edit.marker.subclip",
        run: async () => {
          const d = wave.current;
          const r = waveSel();
          if (!d || !r) return;
          const m = newMarker(uuid(), nextMarkerName(d.markers, "Subclip"), r[0], r[1] - r[0], "subclip");
          await waveEdit("Add Subclip Marker", { op: "add_markers", markers: [m] });
        },
        enabled: () => waveSel() !== null,
        reason: () => NEED_WAVE_SEL,
      }),
      ov(waveIsOpen, {
        id: "edit.marker.cd-track",
        run: async () => {
          const d = wave.current;
          if (!d) return;
          const n = d.markers.filter((x) => x.kind === "cd_track").length + 1;
          await waveEdit("Add CD Track Marker", { op: "add_markers", markers: [newMarker(uuid(), `Track ${n}`, wavePlayhead(), 0, "cd_track")] });
        },
      }),
      ov(waveIsOpen, {
        id: "edit.marker.cart",
        run: async () => {
          const d = wave.current;
          if (!d) return;
          const r = waveSel();
          const m = r
            ? newMarker(uuid(), nextMarkerName(d.markers, "Cart"), r[0], r[1] - r[0], "cart_timer")
            : newMarker(uuid(), nextMarkerName(d.markers, "Cart"), wavePlayhead(), 0, "cart_timer");
          await waveEdit("Add Cart Timer Marker", { op: "add_markers", markers: [m] });
        },
      }),
      ov(waveIsOpen, {
        id: "edit.marker.range",
        run: async () => {
          const d = wave.current;
          const r = waveSel();
          if (!d || !r) return;
          const m = newMarker(uuid(), nextMarkerName(d.markers, "Range"), r[0], r[1] - r[0]);
          await waveEdit("Add Range Marker", { op: "add_markers", markers: [m] });
        },
        enabled: () => waveSel() !== null,
        reason: () => NEED_WAVE_SEL,
      }),
      ov(waveIsOpen, { id: "multitrack.play-pause", run: () => spaceToggle(waveTransport) }),
      ov(waveIsOpen, { id: "multitrack.pause", run: () => waveTransport.pause() }),
      ov(waveIsOpen, { id: "multitrack.stop", run: () => (recording() ? finishRecord() : waveTransport.stop()) }),
      ov(waveIsOpen, { id: "transport.go-start", run: () => waveTransport.seek(0) }),
      ov(waveIsOpen, { id: "view.zoom-in", run: () => waveZoom.run?.("in") }),
      ov(waveIsOpen, { id: "view.zoom-out", run: () => waveZoom.run?.("out") }),
      ov(waveIsOpen, { id: "view.zoom-session", run: () => waveZoom.run?.("full") }),
      ov(waveIsOpen, { id: "view.zoom-selection", run: () => waveZoom.run?.("selection"), enabled: () => waveSel() !== null }),
      waveHistory.subscribe(() => actions.invalidate()),
      waveFile.subscribe(() => actions.invalidate()),
      editor.subscribe(() => actions.invalidate()),
      wave.subscribe(() => actions.invalidate()),
    ];
  }


  return {
    /** What the Waveform editor's Copy holds, if anything. */
    get board() {
      return waveBoard;
    },
    waveEdit,
    waveSetSelection,
    wavePlayhead,
    confirmWaveDiscard,
    openInWaveform,
    saveWave,
    waveAmplify,
    applyWaveChain,
    register: registerWaveActions,
  };
}
