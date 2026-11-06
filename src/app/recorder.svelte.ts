// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Recording: Record and Stop, the count-in, the armed tracks' input
 * meters and the live waveform, and placing the takes when recording
 * stops. Multitrack: every armed track records its input into its own
 * file, from the playhead, while the session plays; tracks armed or
 * disarmed while recording start or end their takes then. Waveform
 * editor: the take goes into the file at the playhead.
 */

import { midiLiveSet, midiRecordAdd, midiRecordStart, midiRecordStop, recordAdd, recordMonitor, recordRemove, recordStart, recordStatus, recordStop, type LiveMidiTrack, type MidiTake, type RecordTarget } from "./core";
import { midiTakeClip } from "./midi-take";
import { placeTake, punchStopAfter, recordPlan, sanitizeRecordOptions, type RecordOptions, type RecordPlan } from "./recording";
import { isolate, LockedError } from "../panels/editor/clip-edit";
import { insertFile } from "../panels/editor/wave-edit";
import { sessionHistory } from "../stores/history";
import { editor } from "../stores/editor";
import { metronomeRecording } from "../stores/metronome";
import { mediaPool } from "../stores/media";
import { peaks, session, transport } from "../stores/open-session";
import { prefs } from "../stores/prefs";
import { inputLevels, liveTakes, LiveWave, recording } from "../stores/record";
import { isMidi, type Op, type SessionDoc, type TrackDoc } from "../stores/session";
import { DEFAULT_TEMPO } from "../stores/tempo";
import { settings } from "../stores/settings";
import { noteTakeHome, wave, waveFile, waveTransport } from "../stores/wave";

/** What recording needs from the rest of the app. */
export interface RecorderDeps {
  showError: (e: unknown) => void;
  showNotice: (text: string) => void;
  /** The Waveform editor is the one in front. */
  inWave: () => boolean;
  /** The Multitrack playhead (where playback is, while playing). */
  playheadNow: () => number;
  timeSel: () => SessionDoc["view"]["selection"];
  /** An edit to the Waveform editor's file, as one undo step. */
  waveEdit: (label: string, op: Op | null) => Promise<boolean>;
  waveSetSelection: (sel: [number, number] | null, playhead?: number) => void;
  wavePlayhead: () => number;
  uuid: () => string;
}

/** Settings key of the Multitrack's recording options. */
export const KEY_RECORD = "record.options";

export function createRecorder(deps: RecorderDeps) {
  const { showError, showNotice, inWave, playheadNow, timeSel, waveEdit, waveSetSelection, wavePlayhead, uuid } = deps;
    // Recording. Multitrack: every armed track records its input into
    // its own file, from the playhead, while the session plays. Waveform
    // editor: the take goes into the file at the playhead.
    /** Loudest input peak seen during the current recording (0 to 1). */
    let recPeakMax = 0;
    let take: { mode: "multitrack" | "wave"; at: number; plan?: RecordPlan; stopAfter?: number | null } | null = null;
    /** While recording, playback keeps going past the end of the old audio. */
    function holdEnd(on: boolean) {
      transport.holdAtEnd = on;
      waveTransport.holdAtEnd = on;
    }
    const recordOptions = (): RecordOptions => sanitizeRecordOptions(prefs.raw(KEY_RECORD));
    let recordOptionsOpen = $state(false);
    let recPoll: ReturnType<typeof setInterval> | undefined;

    function stamp(): string {
      const d = new Date();
      const p = (n: number) => String(n).padStart(2, "0");
      return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}${p(d.getMinutes())}${p(d.getSeconds())}`;
    }

    // Settings > Playback and Recording: a count before recording starts.
    let countdown = $state<number | null>(null);
    async function countIn(): Promise<boolean> {
      const secs = settings.current.countdownSec;
      if (secs <= 0) return true;
      for (let n = secs; n > 0; n--) {
        countdown = n;
        await new Promise((r) => setTimeout(r, 1000));
        if (countdown === null) return false;
      }
      countdown = null;
      return true;
    }

    async function toggleRecord() {
      if (countdown !== null) {
        // Pressing Record again during the count cancels it.
        countdown = null;
        return;
      }
      if (take) {
        await finishRecord();
        return;
      }
      // Already playing in the Multitrack: recording starts at once, no count.
      const rolling = !inWave() && transport.current.playing;
      if (!rolling && !(await countIn())) return;
      try {
        if (inWave()) {
          const d = wave.current;
          const f = waveFile.current();
          if (!d || !f) {
            showNotice("Open or create a file in the Waveform editor first.");
            return;
          }
          await waveTransport.stop();
          const base = f.path.split(/[\\/]/).pop()?.replace(/\.[^.]+$/, "") ?? "Take";
          const channels = Math.min(2, Math.max(1, d.media[0]?.channels ?? 1));
          const at = wavePlayhead();
          const heard = settings.current.waveMonitor;
          await recordStart([{ key: "wave", input: settings.current.waveInput, channels, name: `${base} take ${stamp()}`, monitor: heard, chain: heard ? (d.tracks[0]?.effects ?? null) : null }]);
          take = { mode: "wave", at };
          liveTakes.set({ at, rate: 0, waves: {}, tick: 0 });
          holdEnd(true);
        } else {
          const doc = session.current;
          if (!doc) return;
          // Nothing armed is fine: the session plays and records nothing
          // until a track is armed. Each armed track gets its own take.
          const armed = doc.tracks.filter((t) => t.record_armed);
          // Armed MIDI tracks record what their MIDI input plays; the others record audio.
          const targets: RecordTarget[] = armed.filter((t) => !isMidi(t)).map(recordTarget);
          const midiIds = armed.filter(isMidi).map((t) => t.id);
          let plan = recordPlan(playheadNow(), timeSel(), recordOptions(), doc.sample_rate);
          // Playing and not punching in: keep playing and record from where
          // the playhead is when the inputs start, with no stop and restart.
          const live = rolling && !plan.punch;
          if (live) {
            const before = playheadNow();
            await recordStart(targets);
            await midiRecordStart(midiIds);
            plan = { ...plan, at: Math.round((before + playheadNow()) / 2) };
            // Playing in Loop or Skip: carry on straight from here instead,
            // so the take never jumps back to the start of the loop.
            if (editor.current().playMode !== "normal") {
              holdEnd(true);
              void transport.play(playheadNow());
            }
          } else {
            await transport.stop();
            await recordStart(targets);
            await midiRecordStart(midiIds);
          }
          const at = plan.at;
          recordingKeys = new Set(armed.map((t) => t.id));
          take = { mode: "multitrack", at, plan, stopAfter: punchStopAfter(plan, doc.sample_rate) };
          holdEnd(true);
          if (plan.punch) showNotice(`Punch-in: recording stops at the end of the time selection.`);
          else if (!armed.length) showNotice("Recording. Arm a track to record on it.");
          liveTakes.set({ at, rate: 0, waves: {}, tick: 0 });
          metronomeRecording(true);
          if (!live) void transport.play(at);
        }
        recPeakMax = 0;
        recording.set({ active: true, seconds: 0, peak: 0 });
        startInputPoll();
      } catch (e) {
        take = null;
        holdEnd(false);
        metronomeRecording(false);
        liveTakes.set(null);
        showError(e);
      }
    }

    // Input meters: armed tracks' inputs stay open for their meters while
    // not recording, and the same poll feeds the live waveform while recording.
    let lastMonitorNote = "";
    function startInputPoll() {
      if (recPoll) return;
      recPoll = setInterval(async () => {
        const st = await recordStatus().catch(() => null);
        if (!st || (!st.recording && !st.monitoring)) {
          inputLevels.set({});
          return;
        }
        if (st.recording) {
          recSeconds = st.seconds;
          recPeakMax = Math.max(recPeakMax, st.peak);
          recording.set({ active: true, seconds: st.seconds, peak: st.peak });
        }
        if (st.monitorNote && st.monitorNote !== lastMonitorNote) showNotice(st.monitorNote);
        lastMonitorNote = st.monitorNote ?? "";
        // A punch-in stops by itself at the end of the selection.
        if (st.recording && take?.stopAfter != null && st.seconds >= take.stopAfter) {
          take = { ...take, stopAfter: null };
          void finishRecord();
          return;
        }
        inputLevels.update((old) => {
          const next: Record<string, number[]> = {};
          for (const k of new Set([...Object.keys(old), ...Object.keys(st.live)])) {
            const l = st.live[k];
            const was = old[k] ?? [];
            const n = Math.max(l ? Math.min(2, Math.max(1, l.channels)) : 0, was.length);
            next[k] = Array.from({ length: n }, (_, c) => {
              const fall = (was[c] ?? 0) * 0.8;
              return Math.max(l?.peaks?.[c] ?? 0, fall < 0.002 ? 0 : fall);
            });
          }
          return next;
        });
        if (st.recording) {
          liveTakes.update((lt) => {
            if (!lt) return lt;
            const waves = { ...lt.waves };
            for (const [k, v] of Object.entries(st.live)) {
              if (!v.blocks.length) continue;
              (waves[k] ??= new LiveWave()).push(v.channels, v.blocks, st.rate);
            }
            return { ...lt, rate: st.rate, waves, tick: lt.tick + 1 };
          });
        }
      }, 80);
    }

    function stopInputPoll() {
      clearInterval(recPoll);
      recPoll = undefined;
      inputLevels.set({});
    }

    function recordTarget(t: TrackDoc): RecordTarget {
      return { key: t.id, input: t.input ?? null, channels: t.channels === "mono" ? 1 : 2, name: `${t.name} ${stamp()}`, monitor: !!t.monitor_input, chain: t.monitor_input ? (t.effects ?? null) : null, gainDb: t.volume_db, pan: t.pan, plugins: !t.monitor_dry };
    }

    /** Tracks recording in the current multitrack take, and seconds recorded at the last poll. */
    let recordingKeys = new Set<string>();
    let recSeconds = 0;
    /**
     * While recording the multitrack, arming a track starts its take from
     * that moment and disarming ends it. The takes are placed when
     * recording stops.
     */
    function syncRecordingTracks() {
      if (take?.mode !== "multitrack") return;
      const doc = session.current;
      const armed = new Set((doc?.tracks ?? []).filter((t) => t.record_armed).map((t) => t.id));
      for (const id of recordingKeys) {
        if (armed.has(id)) continue;
        recordingKeys.delete(id);
        // A MIDI track disarmed keeps what it recorded until recording stops.
        if (doc?.tracks.some((t) => t.id === id && isMidi(t))) continue;
        void recordRemove(id).catch(showError);
      }
      for (const t of doc?.tracks ?? []) {
        if (!armed.has(t.id) || recordingKeys.has(t.id)) continue;
        recordingKeys.add(t.id);
        if (isMidi(t)) {
          void midiRecordAdd(t.id).catch(showError);
          continue;
        }
        const begin = take.at + Math.round(recSeconds * (doc?.sample_rate ?? 48_000));
        liveTakes.update((lt) => {
          if (!lt) return lt;
          // A fresh live drawing: a track armed again starts a new take.
          const waves = { ...lt.waves };
          delete waves[t.id];
          return { ...lt, waves, starts: { ...lt.starts, [t.id]: begin } };
        });
        void recordAdd(recordTarget(t)).catch((e) => {
          recordingKeys.delete(t.id);
          showError(e);
        });
      }
    }

    /**
     * Opens or closes the armed tracks' inputs for their meters. A
     * monitored track is heard through its built-in effects at its fader
     * and pan; a change to those while monitoring opens the input again
     * after a moment's rest, so a dragged fader does not reopen it on
     * every step.
     */
    let monitorKey = "";
    let monitorTune = "";
    let monitorRetune: ReturnType<typeof setTimeout> | undefined;
    function syncMonitor(force = false) {
      syncMidi(force);
      if (take) {
        syncRecordingTracks();
        return;
      }
      const armed = (session.current?.tracks ?? []).filter((t) => t.record_armed && !isMidi(t));
      const key = armed.map((t) => `${t.id}:${t.input ?? ""}:${t.channels}:${t.monitor_input ? (t.monitor_dry ? 2 : 1) : 0}`).join("|");
      const tune = armed
        .filter((t) => t.monitor_input)
        .map((t) => `${t.id}:${t.volume_db}:${t.pan}:${JSON.stringify(t.effects ?? null)}`)
        .join("|");
      const open = () => {
        clearTimeout(monitorRetune);
        monitorRetune = undefined;
        monitorKey = key;
        monitorTune = tune;
        const targets: RecordTarget[] = armed.map((t) => ({ key: t.id, input: t.input ?? null, channels: t.channels === "mono" ? 1 : 2, name: t.name, monitor: !!t.monitor_input, chain: t.monitor_input ? (t.effects ?? null) : null, gainDb: t.volume_db, pan: t.pan, plugins: !t.monitor_dry }));
        void recordMonitor(targets).catch(() => {});
        if (targets.length) startInputPoll();
        else stopInputPoll();
      };
      if (force || key !== monitorKey) {
        open();
        return;
      }
      if (tune === monitorTune) return;
      clearTimeout(monitorRetune);
      monitorRetune = setTimeout(open, 350);
    }

    /**
     * Armed MIDI tracks listen to their MIDI input and play their
     * instrument live (and record while recording). Changes to an armed
     * track's sound reach it after a moment's rest, so a dragged fader
     * does not rebuild it on every step.
     */
    let midiKey = "";
    let midiRetune: ReturnType<typeof setTimeout> | undefined;
    function syncMidi(force = false) {
      // A track set to no input plays and records nothing.
      const listening: LiveMidiTrack[] = (session.current?.tracks ?? [])
        .filter((t) => t.record_armed && isMidi(t) && t.input !== "none")
        .map((t) => ({ trackId: t.id, input: t.input ?? null, instrument: t.instrument ?? null, chain: t.effects ?? null, gainDb: t.volume_db, pan: t.pan }));
      const key = JSON.stringify(listening);
      if (!force && key === midiKey) return;
      const send = () => {
        midiRetune = undefined;
        midiKey = key;
        void midiLiveSet(listening).catch((e) => showNotice(`MIDI input: ${e instanceof Error ? e.message : String(e)}`));
      };
      clearTimeout(midiRetune);
      // Arming, disarming or a new input: at once. Anything else: after a rest.
      const shape = (k: string) => JSON.stringify((JSON.parse(k || "[]") as LiveMidiTrack[]).map((t) => [t.trackId, t.input, t.instrument?.effect ?? null]));
      if (force || shape(key) !== shape(midiKey)) send();
      else midiRetune = setTimeout(send, 400);
    }

    /** Stops recording and places the take. The live waveform stays until the finished clip is in, so nothing is redrawn in between. */
    async function finishRecord() {
      takePaths = [];
      try {
        await finishRecordInner();
        // Keep drawing the live waveform until the finished take's own drawing is ready (up to 4 seconds).
        for (let i = 0; i < 40 && takePaths.some((p) => peaks.get(p).status === "building"); i++) await new Promise((r) => setTimeout(r, 100));
      } finally {
        liveTakes.set(null);
      }
    }
    let takePaths: string[] = [];

    /** Each MIDI track's recorded notes as a new clip on it, from the bar the playing started in. */
    function midiTakeOps(midiTakes: MidiTake[], plan: RecordPlan): Op[] {
      const doc = session.current;
      if (!doc) return [];
      const ops: Op[] = [];
      for (const k of midiTakes) {
        const track = doc.tracks.find((x) => x.id === k.trackId);
        if (!track) continue;
        const clip = midiTakeClip(k.notes, plan.at, doc.sample_rate, doc.tempo ?? DEFAULT_TEMPO, plan.punch, uuid(), `${track.name} ${stamp()}`, k.controls ?? []);
        if (!clip) continue;
        const clips = [...(track.midi_clips ?? []), clip].sort((a, b) => a.start - b.start);
        ops.push({ op: "set_midi_clips", track_id: track.id, clips });
      }
      return ops;
    }

    async function finishRecordInner() {
      const t = take;
      take = null;
      recordingKeys = new Set();
      recSeconds = 0;
      holdEnd(false);
      stopInputPoll();
      metronomeRecording(false);
      recording.set({ active: false, seconds: 0, peak: 0 });
      let midiTakes: MidiTake[] = [];
      try {
        midiTakes = await midiRecordStop();
      } catch (e) {
        showError(e);
      }
      let takes;
      try {
        takes = await recordStop();
      } catch (e) {
        showError(e);
        syncMonitor(true);
        return;
      } finally {
        // Back to metering the armed inputs.
        setTimeout(() => syncMonitor(true), 0);
      }
      if (!t) return;
      const silent = recPeakMax < 0.0005; // about -66 dBFS: nothing is arriving
      if (t.mode === "multitrack") await transport.stop();
      const usable = takes.filter((k) => k.length > 0);
      takePaths = usable.map((k) => k.path);
      const midiOps = t.mode === "multitrack" ? midiTakeOps(midiTakes, t.plan ?? { at: t.at, punch: null }) : [];
      if (usable.length === 0) {
        if (midiOps.length) {
          await session.edit(midiOps.length === 1 ? "Record MIDI" : `Record MIDI on ${midiOps.length} Tracks`, { op: "batch", ops: midiOps }, sessionHistory).catch(showError);
          return;
        }
        showNotice("Nothing was recorded.");
        return;
      }
      if (silent && usable[0]!.length / Math.max(1, usable[0]!.sample_rate) >= 1) {
        showNotice(
          "No sound arrived while recording. On Mac, check System Settings > Privacy & Security > Microphone. On Windows, check Settings > Privacy > Microphone. Also check the input choice and its cable or gain.",
        );
      }
      // Every take shows in the Files panel, whatever happens to its clip.
      // When the Files panel copies it into the session folder, the clip
      // uses the copy too.
      const listed = new Map<string, string>();
      try {
        const r = await mediaPool.importPaths(
          usable.map((k) => k.path),
          sessionHistory,
        );
        const items = mediaPool.current.items;
        usable.forEach((k, i) => {
          const at = items.find((it) => it.id === r.ids[i])?.path;
          if (at) listed.set(k.path, at);
        });
      } catch (e) {
        showError(e);
      }
      const media = (k: (typeof usable)[number]) => ({
        id: uuid(),
        path: listed.get(k.path) ?? k.path,
        relative_path: null,
        sample_rate: k.sample_rate,
        channels: k.channels,
        length: k.length,
      });
      if (t.mode === "wave") {
        const d = wave.current;
        if (!d) return;
        const m = media(usable[0]!);
        const r = insertFile(d, t.at, m, "Take", uuid);
        if (r && (await waveEdit("Record", r.op))) {
          waveSetSelection([t.at, t.at + r.length], t.at);
          // The take is listed in Files on its own; opening it from there offers this file, edits and all.
          const home = waveFile.current();
          if (home) noteTakeHome(m.path, home.path);
        }
        return;
      }
      const ops: Op[] = [...midiOps];
      const doc = session.current;
      const opts = recordOptions();
      const plan: RecordPlan = t.plan ?? { at: t.at, punch: null };
      // A punch-in replaces what was in its range on each recorded track.
      if (doc && plan.punch && opts.replace) {
        try {
          const { ops: cut, inside } = isolate(doc, usable.map((k) => k.key), plan.punch.start, plan.punch.end, uuid);
          ops.push(...cut);
          if (inside.length) ops.push({ op: "remove_clips", ids: inside });
        } catch (e) {
          if (e instanceof LockedError) showNotice("A locked clip is under the punch-in, so the new take was laid on top of it.");
          else throw e;
        }
      }
      for (const k of usable) {
        const placed = placeTake(plan, { length: k.length, sampleRate: k.sample_rate, startFrames: k.start ?? 0 }, doc?.sample_rate ?? k.sample_rate, opts.offsetMs);
        if (!placed) continue;
        const m = media(k);
        ops.push({ op: "add_media", media: m });
        const name = k.path.split(/[\\/]/).pop()?.replace(/\.wav$/i, "") ?? "Take";
        ops.push({ op: "add_clips", track_id: k.key, clips: [{ id: uuid(), name, media_id: m.id, start: placed.start, source_in: placed.sourceIn, source_out: placed.sourceOut }] });
      }
      if (!ops.some((o) => o.op === "add_clips")) {
        showNotice("The take was shorter than the recording offset, so nothing was added.");
        return;
      }
      try {
        const n = usable.length + midiOps.length;
        await session.edit(n === 1 ? "Record" : `Record ${n} Tracks`, { op: "batch", ops }, sessionHistory);
      } catch (e) {
        showError(e);
      }
    }



  return {
    /** Recording now (or counting in: see `countdown`). */
    get active() {
      return take !== null;
    },
    /** Seconds left in the count-in, or null. */
    get countdown() {
      return countdown;
    },
    get optionsOpen() {
      return recordOptionsOpen;
    },
    set optionsOpen(v: boolean) {
      recordOptionsOpen = v;
    },
    options: recordOptions,
    toggle: toggleRecord,
    finish: finishRecord,
    syncMonitor,
  };
}
