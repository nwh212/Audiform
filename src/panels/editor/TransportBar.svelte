<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Altered Audio Editor.

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
  import type { TempoDoc } from "../../stores/session";
  import { addChange, bpmText, editSegment, meterText, nearestBar, parseMeter, secondsToBeats, segmentAtSeconds, BPM_MAX, BPM_MIN } from "../../stores/tempo";
  import { METRONOME_SOUNDS, METRONOME_SUBDIVIDE, settings, type MetronomePrefs } from "../../stores/settings";
  import { setMetronomePrefs } from "../../stores/metronome";

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
    /** The session's tempo map, for the metronome group (Multitrack editor). */
    tempo?: TempoDoc | null;
    onTempo?: (t: TempoDoc, label: string) => void;
  }

  const { transport, rate, playhead, end, hasSelection, shortcutFor = () => undefined, zoom, amplitude = false, tracks = false, tempo = null, onTempo = () => {} }: Props = $props();

  // The metronome, with the tempo and time signature where the playhead is.
  let met = $state<MetronomePrefs>(settings.current.metronome);
  $effect(() =>
    settings.subscribe((s) => {
      met = s.metronome;
    }),
  );
  const seg = $derived(tempo ? segmentAtSeconds(tempo, Math.max(0, playhead) / Math.max(1, rate)) : null);
  let metMenu = $state<{ x: number; y: number } | null>(null);

  function commitBpm(el: HTMLInputElement) {
    if (!tempo || !seg) return;
    const b = Number(el.value.replace(",", "."));
    if (!Number.isFinite(b) || b <= 0) {
      el.value = bpmText(seg.bpm);
      return;
    }
    const bpm = Math.min(BPM_MAX, Math.max(BPM_MIN, Math.round(b * 100) / 100));
    el.value = bpmText(bpm);
    if (bpm !== seg.bpm) onTempo(editSegment(tempo, seg, { bpm }), "Tempo");
  }

  function commitMeter(el: HTMLInputElement) {
    if (!tempo || !seg) return;
    const m = parseMeter(el.value);
    if (!m) {
      el.value = meterText(seg);
      return;
    }
    el.value = meterText(m);
    if (m.beats_per_bar !== seg.beats_per_bar || m.beat_unit !== seg.beat_unit) onTempo(editSegment(tempo, seg, m), "Time Signature");
  }

  function addChangeAtPlayhead() {
    if (!tempo) return;
    const bar = Math.max(1, nearestBar(tempo, secondsToBeats(tempo, playhead / rate)));
    const r = addChange(tempo, bar, crypto.randomUUID());
    if (r.tempo !== tempo) onTempo(r.tempo, "Add Tempo Change");
  }

  const fieldKeys = (e: KeyboardEvent) => {
    e.stopPropagation();
    if (e.key === "Enter") (e.currentTarget as HTMLInputElement).blur();
    if (e.key === "Escape") {
      const el = e.currentTarget as HTMLInputElement;
      el.value = el.defaultValue;
      el.blur();
    }
  };

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
  {#if tempo && seg}
    <div class="metronome" role="group" aria-label="Metronome, tempo and time signature">
      <button
        type="button"
        class="tool"
        class:on={met.on}
        title="Metronome: click on every beat {met.when === 'recording' ? 'while recording' : 'during playback and recording'}{met.on ? ' (on)' : ''}. Right-click for its sound, level and more. Never in an export."
        aria-label="Metronome"
        aria-pressed={met.on}
        onclick={() => setMetronomePrefs({ on: !met.on })}
        oncontextmenu={(e) => {
          e.preventDefault();
          metMenu = { x: e.clientX, y: e.clientY };
        }}
      >
        <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M5.8 2.2h4.4l2.8 11.3H3z" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round" /><path d="M3.6 11h8.8" stroke="currentColor" stroke-width="1" /><path d="M8 11l3.2-6.4" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" /><circle cx="10.3" cy="6.4" r="1" fill="currentColor" /></svg>
      </button>
      <button type="button" class="tool more" title="Metronome settings" aria-label="Metronome settings" onclick={(e) => (metMenu = { x: e.clientX, y: e.clientY })}>
        <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4.5 6.5L8 10l3.5-3.5" fill="none" stroke="currentColor" stroke-width="1.3" /></svg>
      </button>
      {#key `${seg.id}:${seg.bpm}`}
        <input
          class="field bpm"
          value={bpmText(seg.bpm)}
          aria-label="Tempo in beats per minute"
          title={seg.id ? `Tempo from bar ${seg.bar + 1}, where the playhead is. Changes later in the song are on the Tempo strip under the ruler.` : "Tempo where the playhead is. Changes later in the song are on the Tempo strip under the ruler (right-click the ruler to add one)."}
          onchange={(e) => commitBpm(e.currentTarget)}
          onkeydown={fieldKeys}
        />
      {/key}
      <span class="unit">BPM</span>
      {#key `${seg.id}:${meterText(seg)}`}
        <input
          class="field meter"
          value={meterText(seg)}
          aria-label="Time signature"
          title="Time signature where the playhead is, such as 4/4, 3/4 or 6/8. Bars and beats, the metronome and MIDI clips follow it."
          onchange={(e) => commitMeter(e.currentTarget)}
          onkeydown={fieldKeys}
        />
      {/key}
    </div>
  {/if}
  </div>

  <div class="transport" role="group" aria-label="Transport">
    <button type="button" class="tool" title={tip("Stop", "multitrack.stop")} aria-label="Stop" onclick={stopAll}>
      <svg viewBox="0 0 16 16" aria-hidden="true"><rect x="4" y="4" width="8" height="8" rx="1" fill="currentColor" /></svg>
    </button>
    <button
      type="button"
      class="tool play"
      class:on={tp?.playing}
      title={tip(tp?.playing ? "Pause" : "Play", "multitrack.play-pause")}
      aria-label={tp?.playing ? "Pause" : "Play"}
      onclick={() => void transport.toggle()}
    >
      {#if tp?.playing}
        <svg viewBox="0 0 16 16" aria-hidden="true"><rect x="4" y="3.5" width="3" height="9" rx="0.5" fill="currentColor" /><rect x="9" y="3.5" width="3" height="9" rx="0.5" fill="currentColor" /></svg>
      {:else}
        <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M5 3v10l8-5z" fill="currentColor" /></svg>
      {/if}
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

{#if metMenu}
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div class="scrim" onclick={() => (metMenu = null)} oncontextmenu={(e) => { e.preventDefault(); metMenu = null; }}></div>
  <div
    class="met-pop"
    role="dialog"
    aria-label="Metronome settings"
    tabindex="-1"
    style:left="{Math.min(metMenu.x, window.innerWidth - 270)}px"
    style:bottom="{Math.max(8, window.innerHeight - metMenu.y + 8)}px"
    onkeydown={(e) => {
      e.stopPropagation();
      if (e.key === "Escape") metMenu = null;
    }}
  >
    <div class="pop-title">Metronome</div>
    <label class="row"><input type="checkbox" checked={met.on} onchange={(e) => setMetronomePrefs({ on: e.currentTarget.checked })} /> Metronome on</label>
    <label class="row">Click
      <select value={met.when} onchange={(e) => setMetronomePrefs({ when: e.currentTarget.value === "recording" ? "recording" : "always" })}>
        <option value="always">Playback and recording</option>
        <option value="recording">Recording only</option>
      </select>
    </label>
    <label class="row">Sound
      <select value={met.sound} onchange={(e) => setMetronomePrefs({ sound: e.currentTarget.value as MetronomePrefs["sound"] })}>
        {#each METRONOME_SOUNDS as o (o.value)}<option value={o.value}>{o.label}</option>{/each}
      </select>
    </label>
    <label class="row">Level
      <input type="range" min="-40" max="6" step="0.5" value={met.levelDb} oninput={(e) => setMetronomePrefs({ levelDb: Number(e.currentTarget.value) })} />
      <span class="num">{met.levelDb.toFixed(1)} dB</span>
    </label>
    <label class="row">Between beats
      <select value={met.subdivide} onchange={(e) => setMetronomePrefs({ subdivide: e.currentTarget.value as MetronomePrefs["subdivide"] })}>
        {#each METRONOME_SUBDIVIDE as o (o.value)}<option value={o.value}>{o.label}</option>{/each}
      </select>
    </label>
    <label class="row"><input type="checkbox" checked={met.accent} onchange={(e) => setMetronomePrefs({ accent: e.currentTarget.checked })} /> Higher click on the first beat of each bar</label>
    <div class="pop-sep"></div>
    <button type="button" class="pop-btn" onclick={() => { addChangeAtPlayhead(); metMenu = null; }}>Add Tempo Change at Nearest Bar</button>
    <p class="pop-note">The click plays through your speakers or headphones only. It is never in a mixdown or export.</p>
  </div>
{/if}

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

  .metronome {
    display: flex;
    gap: 3px;
    align-items: center;
    color: var(--text-faint);
    font-size: var(--font-size-small);
  }

  .metronome .tool.more {
    width: 14px;
    margin-left: -3px;
  }

  .metronome .tool.more svg {
    width: 10px;
    height: 10px;
  }

  .field {
    height: 20px;
    box-sizing: border-box;
    padding: 0 4px;
    border: 1px solid var(--border);
    border-radius: 2px;
    background: var(--surface-2);
    color: var(--value);
    font-family: var(--font-mono);
    font-size: var(--font-size-small);
    text-align: right;
  }

  .field.bpm {
    width: 54px;
  }

  .field.meter {
    width: 40px;
    margin-left: 4px;
    text-align: center;
  }

  .scrim {
    position: fixed;
    inset: 0;
    z-index: 90;
  }

  .met-pop {
    position: fixed;
    z-index: 91;
    display: grid;
    gap: 6px;
    width: 260px;
    padding: 10px 12px;
    border: 1px solid var(--border-strong);
    border-radius: 4px;
    background: var(--surface-2);
    box-shadow: 0 6px 20px rgba(0, 0, 0, 0.5);
    font-size: var(--font-size-small);
  }

  .met-pop .pop-title {
    color: var(--text);
    font-weight: 600;
  }

  .met-pop .row {
    display: flex;
    gap: 6px;
    align-items: center;
    color: var(--text-dim);
  }

  .met-pop select,
  .met-pop input[type="range"] {
    flex: 1;
    min-width: 0;
  }

  .met-pop .num {
    width: 52px;
    color: var(--value);
    font-family: var(--font-mono);
    text-align: right;
  }

  .met-pop .pop-sep {
    height: 1px;
    background: var(--border);
  }

  .met-pop .pop-btn {
    justify-self: start;
  }

  .met-pop .pop-note {
    margin: 0;
    color: var(--text-faint);
    font-size: 10px;
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
