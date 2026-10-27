<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  The bar under an editor: the position clock, transport buttons, and
  zoom buttons. Shared by the Multitrack and Waveform editors; each
  passes its own transport and zoom.
-->
<script lang="ts" module>
  export type ZoomKind = "in" | "out" | "full" | "selection" | "amp-in" | "amp-out" | "tracks-in" | "tracks-out";
</script>

<script lang="ts">
  import { editor, type EditorState } from "../../stores/editor";
  import type { TransportState, TransportStore } from "../../stores/transport";
  import { formatIn } from "./timeline";
  import { recordControl, recording, type RecordingState } from "../../stores/record";

  interface Props {
    transport: TransportStore;
    rate: number;
    playhead: number;
    /** Where the audio ends, for Go to End. */
    end: number;
    /** A time selection, for Zoom to Selection. */
    hasSelection: boolean;
    shortcutFor?: (id: string) => string | undefined;
    zoom: (kind: ZoomKind) => void;
    /** Show amplitude zoom. */
    amplitude?: boolean;
    /** Show track height zoom (Multitrack editor). */
    tracks?: boolean;
  }

  const { transport, rate, playhead, end, hasSelection, shortcutFor = () => undefined, zoom, amplitude = false, tracks = false }: Props = $props();


  /** Loop Playback and Skip Selection act on the time selection. A change while playing restarts from the same spot. */
  function togglePlayMode(mode: "loop" | "skip") {
    editor.togglePlayMode(mode);
    if (tp?.playing) void transport.play(tp.position);
  }
  const modeTip = (mode: "loop" | "skip") => {
    const on = ed.playMode === mode;
    const what = mode === "loop" ? "Loop Playback: repeat the time selection" : "Skip Selection: play around the time selection to hear a cut before you make it";
    return `${what}${on ? " (on)" : ""}${hasSelection ? "" : ". Make a time selection to use it."}`;
  };

  let tp = $state<TransportState | null>(null);
  $effect(() =>
    transport.subscribe((t) => {
      tp = t;
    }),
  );
  let rec = $state<RecordingState>({ active: false, seconds: 0, peak: 0 });
  $effect(() =>
    recording.subscribe((r) => {
      rec = r;
    }),
  );
  let ed = $state<EditorState>(editor.current());
  $effect(() =>
    editor.subscribe((s) => {
      ed = s;
    }),
  );

  /** Stop ends a recording in progress as well as playback. */
  function stopAll() {
    if (rec.active) recordControl.toggle?.();
    else void transport.stop();
  }

  const tip = (label: string, id?: string) => {
    const k = id ? shortcutFor(id) : undefined;
    return k ? `${label} (${k})` : label;
  };
  const NEXT_FORMAT = { time: "seconds", seconds: "bars", bars: "samples", samples: "timecode", timecode: "time" } as const;
  const clock = $derived(formatIn(playhead, rate, ed.timeFormat));

  /** Rewind and fast forward step 5 seconds, 1 with Shift. */
  function nudge(e: MouseEvent, dir: -1 | 1) {
    const step = (e.shiftKey ? 1 : 5) * rate;
    void transport.seek(Math.max(0, Math.min(end, playhead + dir * step)));
  }
</script>

<div class="bar">
  <div class="left">
  <button
    type="button"
    class="clock"
    title="Playhead position. Click to switch between minutes and seconds, seconds, bars and beats, samples and timecode (or right-click the time ruler)."
    onclick={() => editor.setTimeFormat(NEXT_FORMAT[ed.timeFormat])}
  >
    {clock}
  </button>
  </div>

  <div class="transport" role="group" aria-label="Transport">
    <button type="button" class="tool" title={tip("Stop", "multitrack.stop")} aria-label="Stop" onclick={stopAll}>
      <svg viewBox="0 0 16 16" aria-hidden="true"><rect x="4" y="4" width="8" height="8" rx="1" fill="currentColor" /></svg>
    </button>
    <button
      type="button"
      class="tool play"
      class:on={tp?.playing}
      title={tip(tp?.paused ? "Play: plays on from where Pause stopped" : "Play", "multitrack.play-pause")}
      aria-label="Play"
      onclick={() => {
        if (!tp?.playing) void transport.play();
      }}
    >
      <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M5 3v10l8-5z" fill="currentColor" /></svg>
    </button>
    <button
      type="button"
      class="tool"
      class:on={tp?.paused}
      title={tip("Pause: stops where the playhead is and keeps the start marker; press again to play on", "multitrack.pause")}
      aria-label="Pause"
      aria-pressed={tp?.paused ?? false}
      disabled={!tp?.playing && !tp?.paused}
      onclick={() => void transport.pause()}
    >
      <svg viewBox="0 0 16 16" aria-hidden="true"><rect x="4" y="3.5" width="3" height="9" rx="0.5" fill="currentColor" /><rect x="9" y="3.5" width="3" height="9" rx="0.5" fill="currentColor" /></svg>
    </button>
    <span class="sep"></span>
    <button type="button" class="tool" title="Go to Start" aria-label="Go to start" onclick={() => void transport.seek(0)}>
      <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3.5 3v10" stroke="currentColor" stroke-width="1.6" /><path d="M13 3.5v9L6 8z" fill="currentColor" /></svg>
    </button>
    <button type="button" class="tool" title="Rewind 5 seconds (Shift: 1 second)" aria-label="Rewind" onclick={(e) => nudge(e, -1)}>
      <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M8 4v8L2.5 8zM14 4v8L8.5 8z" fill="currentColor" /></svg>
    </button>
    <button type="button" class="tool" title="Fast Forward 5 seconds (Shift: 1 second)" aria-label="Fast forward" onclick={(e) => nudge(e, 1)}>
      <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M2 4v8l5.5-4zM8 4v8l5.5-4z" fill="currentColor" /></svg>
    </button>
    <button type="button" class="tool" title="Go to End" aria-label="Go to end" onclick={() => void transport.seek(end)}>
      <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3 3.5v9L10 8z" fill="currentColor" /><path d="M12.5 3v10" stroke="currentColor" stroke-width="1.6" /></svg>
    </button>
    <span class="sep"></span>
    <button
      type="button"
      class="tool rec"
      class:on={rec.active}
      title={rec.active ? "Stop Recording" : tip("Record: armed tracks record their inputs (in the Waveform editor, records into the file at the playhead)", "multitrack.record")}
      aria-label={rec.active ? "Stop recording" : "Record"}
      aria-pressed={rec.active}
      onclick={() => recordControl.toggle?.()}
    >
      <svg viewBox="0 0 16 16" aria-hidden="true"><circle cx="8" cy="8" r="4.5" fill="currentColor" /></svg>
    </button>
    {#if rec.active}
      <span class="rec-time" aria-live="polite">REC {Math.floor(rec.seconds / 60)}:{String(Math.floor(rec.seconds % 60)).padStart(2, "0")}</span>
      <span class="rec-meter" aria-hidden="true"><span style:width="{Math.min(100, rec.peak * 100)}%" class:hot={rec.peak > 0.9}></span></span>
    {/if}
    <button
      type="button"
      class="tool"
      class:on={ed.playMode === "loop"}
      title={modeTip("loop")}
      aria-label="Loop playback"
      aria-pressed={ed.playMode === "loop"}
      onclick={() => togglePlayMode("loop")}
    >
      <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4 6.5h7.5l-2-2M12 9.5H4.5l2 2" fill="none" stroke="currentColor" stroke-width="1.4" /></svg>
    </button>
    <button
      type="button"
      class="tool"
      class:on={ed.playMode === "skip"}
      title={modeTip("skip")}
      aria-label="Skip selection"
      aria-pressed={ed.playMode === "skip"}
      onclick={() => togglePlayMode("skip")}
    >
      <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M6 3.5v9M10 3.5v9" stroke="currentColor" stroke-width="1" stroke-dasharray="1.5 1.5" /><path d="M2 11c1.5-6 10.5-6 12 0" fill="none" stroke="currentColor" stroke-width="1.4" /><path d="M14 11l-2.2-.6M14 11l.5-2.2" stroke="currentColor" stroke-width="1.3" /></svg>
    </button>
  </div>

  <div class="zoom" role="group" aria-label="Zoom">
    {#if amplitude}
      <button type="button" class="tool" title="Zoom In Amplitude (Alt + mouse wheel)" aria-label="Zoom in amplitude" onclick={() => zoom("amp-in")}>
        <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M8 2v12M5 5l3-3 3 3M5 11l3 3 3-3" fill="none" stroke="currentColor" stroke-width="1.3" /></svg>
      </button>
      <button type="button" class="tool" title="Zoom Out Amplitude (Alt + mouse wheel)" aria-label="Zoom out amplitude" onclick={() => zoom("amp-out")}>
        <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M8 2v12M5 2l3 3 3-3M5 14l3-3 3 3" fill="none" stroke="currentColor" stroke-width="1.3" /></svg>
      </button>
      <span class="sep"></span>
    {/if}
    {#if tracks}
      <button type="button" class="tool" title="Taller Tracks" aria-label="Taller tracks" onclick={() => zoom("tracks-in")}>
        <svg viewBox="0 0 16 16" aria-hidden="true"><rect x="2" y="2.5" width="12" height="4.5" rx="1" fill="none" stroke="currentColor" stroke-width="1.2" /><rect x="2" y="9" width="12" height="4.5" rx="1" fill="none" stroke="currentColor" stroke-width="1.2" /><path d="M8 3.8v2M7 4.8h2" stroke="currentColor" stroke-width="1.1" /></svg>
      </button>
      <button type="button" class="tool" title="Shorter Tracks" aria-label="Shorter tracks" onclick={() => zoom("tracks-out")}>
        <svg viewBox="0 0 16 16" aria-hidden="true"><rect x="2" y="4" width="12" height="3" rx="1" fill="none" stroke="currentColor" stroke-width="1.2" /><rect x="2" y="9" width="12" height="3" rx="1" fill="none" stroke="currentColor" stroke-width="1.2" /></svg>
      </button>
      <span class="sep"></span>
    {/if}
    <button type="button" class="tool" title={tip("Zoom In", "view.zoom-in")} aria-label="Zoom in" onclick={() => zoom("in")}>
      <svg viewBox="0 0 16 16" aria-hidden="true"><circle cx="7" cy="7" r="4.5" fill="none" stroke="currentColor" stroke-width="1.4" /><path d="M5 7h4M7 5v4M10.5 10.5l3.5 3.5" stroke="currentColor" stroke-width="1.4" /></svg>
    </button>
    <button type="button" class="tool" title={tip("Zoom Out", "view.zoom-out")} aria-label="Zoom out" onclick={() => zoom("out")}>
      <svg viewBox="0 0 16 16" aria-hidden="true"><circle cx="7" cy="7" r="4.5" fill="none" stroke="currentColor" stroke-width="1.4" /><path d="M5 7h4M10.5 10.5l3.5 3.5" stroke="currentColor" stroke-width="1.4" /></svg>
    </button>
    <button type="button" class="tool" title={tip("Zoom to Selection", "view.zoom-selection")} aria-label="Zoom to selection" disabled={!hasSelection} onclick={() => zoom("selection")}>
      <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M2 3v10M14 3v10" stroke="currentColor" stroke-width="1.4" /><path d="M5 8h6M5 8l2-2M5 8l2 2M11 8l-2-2M11 8l-2 2" fill="none" stroke="currentColor" stroke-width="1.2" /></svg>
    </button>
    <button type="button" class="tool" title={tip("Zoom Out Full", "view.zoom-session")} aria-label="Zoom out full" onclick={() => zoom("full")}>
      <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M2 5V2h3M11 2h3v3M14 11v3h-3M5 14H2v-3" fill="none" stroke="currentColor" stroke-width="1.4" /></svg>
    </button>
  </div>
</div>

<style>
  .bar {
    display: grid;
    grid-template-columns: 1fr auto 1fr;
    gap: 6px;
    align-items: center;
    min-height: 32px;
    padding: 2px 8px;
    border-top: 1px solid var(--border);
    background: var(--surface-1);
  }

  .clock {
    min-width: 128px;
    padding: 0 4px;
    border: 0;
    background: none;
    color: var(--value);
    font-family: var(--font-mono);
    font-size: 18px;
    font-variant-numeric: tabular-nums;
    letter-spacing: 0.02em;
    text-align: left;
  }

  .transport,
  .zoom {
    display: flex;
    gap: 2px;
    align-items: center;
  }

  .bar > :first-child {
    justify-self: start;
  }

  .left {
    display: flex;
    gap: 8px;
    align-items: center;
    min-width: 0;
  }


  .zoom {
    justify-self: end;
  }

  .sep {
    width: 1px;
    height: 16px;
    margin: 0 4px;
    background: var(--border);
  }

  .tool {
    display: grid;
    place-items: center;
    width: 26px;
    height: 24px;
    border: 1px solid transparent;
    border-radius: 3px;
    background: none;
    color: var(--text-dim);
  }

  .tool svg {
    width: 14px;
    height: 14px;
  }

  .tool:hover:not(:disabled) {
    border-color: var(--border-strong);
    color: var(--text);
  }

  .tool:disabled {
    opacity: 0.35;
  }

  .tool.play.on {
    color: var(--clip-1);
  }

  .tool.on:not(.play) {
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 18%, transparent);
    color: var(--text);
  }

  .tool.rec {
    color: var(--danger);
  }

  .tool.rec.on {
    border-color: var(--danger);
    background: color-mix(in srgb, var(--danger) 25%, transparent);
    animation: rec-blink 1s steps(2, start) infinite;
  }

  @keyframes rec-blink {
    to {
      background: transparent;
    }
  }

  .rec-time {
    margin-left: 4px;
    color: var(--danger);
    font-family: var(--font-mono);
    font-size: var(--font-size-small);
    white-space: nowrap;
  }

  .rec-meter {
    position: relative;
    width: 60px;
    height: 6px;
    margin-left: 4px;
    border-radius: 2px;
    background: var(--surface-0);
    overflow: hidden;
  }

  .rec-meter span {
    position: absolute;
    inset: 0 auto 0 0;
    background: var(--clip-1);
  }

  .rec-meter span.hot {
    background: var(--danger);
  }
</style>
