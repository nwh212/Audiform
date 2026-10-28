<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  Time Info: where the playhead is, the time selection (or the span of the
  selected clips), the visible range, and the session's length, format and
  mixdown size. In the Waveform editor it shows the open file and its
  playback instead. The playhead, selection and view times can be dragged
  or typed: see TimeField.
-->
<script lang="ts">
  import { editor, type EditorState } from "../../stores/editor";
  import { clipLength, sessionEnd, type SessionDoc, type SessionStore } from "../../stores/session";
  import type { TransportState, TransportStore } from "../../stores/transport";
  import { formatIn } from "../editor/timeline";
  import { formatSize } from "../files/format";
  import { mixdownBytes } from "../../app/session-properties";
  import { settings } from "../../stores/settings";
  import TimeField from "./TimeField.svelte";
  import { editSelection, laneWidths, parseTime, viewFor, viewRange } from "./time-info";

  interface Props {
    session: SessionStore;
    transport: TransportStore;
    /** The Waveform editor's file and transport, shown while that editor is in front. */
    wave?: SessionStore;
    waveTransport?: TransportStore;
  }

  const { session, transport, wave, waveTransport }: Props = $props();

  let sessionDoc = $state<SessionDoc | null>(null);
  $effect(() =>
    session.subscribe((d) => {
      sessionDoc = d;
    }),
  );
  let sessionTp = $state<TransportState | null>(null);
  $effect(() =>
    transport.subscribe((t) => {
      sessionTp = t;
    }),
  );
  let waveDoc = $state<SessionDoc | null>(null);
  $effect(() =>
    wave?.subscribe((d) => {
      waveDoc = d;
    }),
  );
  let waveTp = $state<TransportState | null>(null);
  $effect(() =>
    waveTransport?.subscribe((t) => {
      waveTp = t;
    }),
  );
  let ed = $state<EditorState>(editor.current());
  $effect(() =>
    editor.subscribe((e) => {
      ed = e;
    }),
  );
  const inWave = $derived(ed.mode === "waveform" && waveDoc !== null);
  const doc = $derived(inWave ? waveDoc : sessionDoc);
  const tp = $derived(inWave ? waveTp : sessionTp);

  const rate = $derived(doc?.sample_rate ?? 48000);
  const t = (samples: number) => formatIn(samples, rate, ed.timeFormat);

  /** The time selection if there is one, otherwise the span of the selected clips. */
  const span = $derived.by(() => {
    if (doc?.view.selection) return doc.view.selection;
    if (!doc || inWave || ed.selectedClips.size === 0) return null;
    const media = new Map(doc.media.map((m) => [m.id, m]));
    let start = Infinity;
    let end = 0;
    for (const tr of doc.tracks)
      for (const c of tr.clips)
        if (ed.selectedClips.has(c.id)) {
          start = Math.min(start, c.start);
          end = Math.max(end, c.start + clipLength(c, media.get(c.media_id), rate));
        }
    return start === Infinity ? null : { start, end };
  });

  const length = $derived(doc ? sessionEnd(doc) : 0);
  const playhead = $derived(tp?.playing ? tp.position : (doc?.view.playhead ?? 0));
  const store = $derived(inWave ? wave! : session);
  const player = $derived(inWave ? waveTransport! : transport);
  let widths = $state({ multitrack: 0, waveform: 0 });
  $effect(() =>
    laneWidths.subscribe((w) => {
      widths = w;
    }),
  );
  const laneW = $derived(inWave ? widths.waveform : widths.multitrack);
  const step = $derived(doc?.view.samples_per_pixel ?? 1);
  const parse = (text: string) => parseTime(text, rate, ed.timeFormat, settings.current.timecodeFps);
  const vis = $derived(doc && laneW > 0 ? viewRange(doc.view, laneW) : null);
  /** Only a time selection can be edited here; the span of selected clips is shown as it is. */
  const timeSel = $derived(doc?.view.selection ?? null);

  function setSel(part: "start" | "end" | "duration", v: number) {
    const d = store.current;
    if (!d) return;
    const sel = d.view.selection ?? { start: d.view.playhead, end: d.view.playhead };
    store.setView({ ...d.view, selection: editSelection(sel, part, v) });
  }
  function setVis(part: "start" | "end" | "duration", v: number) {
    const d = store.current;
    if (!d || laneW <= 0) return;
    const r = viewRange(d.view, laneW);
    const [a, b] = part === "start" ? [v, v + (r.end - r.start)] : part === "end" ? [r.start, Math.max(r.start + 1, v)] : [r.start, r.start + Math.max(1, v)];
    store.setView(viewFor(d.view, a, b, laneW));
  }

  /** A stereo WAV mixdown of the whole session at the session bit depth. */
  const size = $derived(mixdownBytes(length, doc?.bit_depth ?? 32));
</script>

<section class="info" aria-label="Time Info">
  <div class="head">
    <span class="k">Playhead</span>
    <TimeField big value={doc ? playhead : null} text={t(playhead)} label="Playhead" {step} {parse} onset={doc ? (v) => void player.seek(v) : null} />
  </div>
  <div class="grid">
    <span></span>
    <span class="h">Start</span>
    <span class="h">End</span>
    <span class="h">Duration</span>

    <span class="k" title={timeSel ? "Time selection" : span ? "Selected clips" : "Time selection"}>{timeSel || !span ? "Selection" : "Clips"}</span>
    <TimeField value={span?.start ?? null} text={span ? t(span.start) : "-"} label="Selection start" {step} {parse} onset={doc && (timeSel || !span) ? (v) => setSel("start", v) : null} />
    <TimeField value={span?.end ?? null} text={span ? t(span.end) : "-"} label="Selection end" {step} {parse} onset={doc && (timeSel || !span) ? (v) => setSel("end", v) : null} />
    <TimeField value={span ? span.end - span.start : null} text={span ? t(span.end - span.start) : "-"} label="Selection duration" {step} {parse} onset={doc && timeSel ? (v) => setSel("duration", v) : null} />

    <span class="k" title="The part of the timeline in view">View</span>
    <TimeField value={vis?.start ?? null} text={vis ? t(vis.start) : "-"} label="View start" {step} {parse} onset={vis ? (v) => setVis("start", v) : null} />
    <TimeField value={vis?.end ?? null} text={vis ? t(vis.end) : "-"} label="View end" {step} {parse} onset={vis ? (v) => setVis("end", v) : null} />
    <TimeField value={vis ? vis.end - vis.start : null} text={vis ? t(vis.end - vis.start) : "-"} label="View duration" {step} {parse} onset={vis ? (v) => setVis("duration", v) : null} />
  </div>
  <div class="foot">
    <span title="Session length">{t(length)}</span>
    <span>{rate.toLocaleString("en-US")} Hz</span>
    <span>{doc?.bit_depth === 32 ? "32-bit float" : `${doc?.bit_depth ?? 32}-bit`}</span>
    <span title="Size of a stereo WAV mixdown at the session bit depth">{formatSize(size)}</span>
  </div>
</section>

<style>
  .info {
    display: grid;
    grid-template-rows: auto auto;
    align-content: start;
    gap: 2px;
    height: 100%;
    overflow: auto;
    padding: 3px 10px;
    line-height: 1.2;
    box-sizing: border-box;
    background: var(--surface-1);
    container-type: inline-size;
  }

  .head {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 0 10px;
    align-items: center;
  }
  .grid {
    display: grid;
    grid-template-columns: auto repeat(3, minmax(max-content, 1fr));
    gap: 1px 6px;
    align-items: center;
  }
  .h {
    color: var(--text-faint);
    font-size: 10px;
    text-align: right;
    padding-right: 4px;
  }

  .k {
    color: var(--text-dim);
    font-size: var(--font-size-small);
  }


  /* In a narrow panel the grid turns on its side: Start, End and Duration down the
     left, Selection and View across the top, so the times never run into each other. */
  @container (max-width: 340px) {
    .grid {
      grid-auto-flow: column;
      grid-template-rows: repeat(4, auto);
      grid-template-columns: auto repeat(2, minmax(max-content, 1fr));
    }
    .h {
      text-align: left;
      padding-right: 0;
    }
    .grid > .k {
      text-align: right;
      padding-right: 4px;
    }
  }

  .foot {
    display: flex;
    flex-wrap: wrap;
    gap: 2px 10px;
    padding-top: 4px;
    border-top: 1px solid var(--border);
    color: var(--text-faint);
    font-size: 10px;
  }
</style>
