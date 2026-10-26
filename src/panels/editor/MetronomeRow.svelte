<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  The Metronome track, shown above the session's tracks while the
  metronome is on (the metronome button by the ruler). Its header has
  mute, level, sound, and the tempo and time signature where the
  playhead is; its lane shows the beats it clicks on. It is not a real
  track: it is never saved in the session and never heard in a mixdown
  or export, only through the speakers while playing or recording.
-->
<script lang="ts">
  import type { TempoDoc } from "../../stores/session";
  import { addChange, bpmText, editSegment, meterText, nearestBar, parseMeter, secondsToBeats, segmentAtSeconds, BPM_MAX, BPM_MIN } from "../../stores/tempo";
  import { METRONOME_SOUNDS, METRONOME_SUBDIVIDE, settings, type MetronomePrefs } from "../../stores/settings";
  import { setMetronomePrefs } from "../../stores/metronome";
  import { gridLines, type Viewport } from "./timeline";
  import Dial from "./Dial.svelte";

  interface Props {
    tempo: TempoDoc;
    view: Viewport;
    /** Lane width in pixels. */
    width: number;
    rate: number;
    playhead: number;
    headerW: number;
    onTempo: (t: TempoDoc, label: string) => void;
  }

  const { tempo, view, width, rate, playhead, headerW, onTempo }: Props = $props();

  const ROW_H = 62;

  let met = $state<MetronomePrefs>(settings.current.metronome);
  $effect(() =>
    settings.subscribe((s) => {
      met = s.metronome;
    }),
  );
  const seg = $derived(segmentAtSeconds(tempo, Math.max(0, playhead) / Math.max(1, rate)));
  const lines = $derived(gridLines(view, width, rate, tempo, 5));
  let pop = $state<{ x: number; y: number } | null>(null);

  function commitBpm(el: HTMLInputElement) {
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
    const m = parseMeter(el.value);
    if (!m) {
      el.value = meterText(seg);
      return;
    }
    el.value = meterText(m);
    if (m.beats_per_bar !== seg.beats_per_bar || m.beat_unit !== seg.beat_unit) onTempo(editSegment(tempo, seg, m), "Time Signature");
  }

  function addChangeAtPlayhead() {
    const bar = Math.max(1, nearestBar(tempo, secondsToBeats(tempo, playhead / rate)));
    const r = addChange(tempo, bar, crypto.randomUUID());
    if (r.tempo !== tempo) onTempo(r.tempo, "Add Tempo Change");
  }

  const fieldKeys = (e: KeyboardEvent) => {
    e.stopPropagation();
    const el = e.currentTarget as HTMLInputElement;
    if (e.key === "Enter") el.blur();
    if (e.key === "Escape") {
      el.value = el.defaultValue;
      el.blur();
    }
  };
  const openPop = (e: MouseEvent) => {
    e.preventDefault();
    pop = { x: e.clientX, y: e.clientY };
  };
</script>

<div class="row" style:height="{ROW_H}px">
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="head" style:width="{headerW}px" oncontextmenu={openPop}>
    <div class="line">
      <svg class="icon" viewBox="0 0 16 16" aria-hidden="true"><path d="M5.8 2.2h4.4l2.8 11.3H3z" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round" /><path d="M3.6 11h8.8" stroke="currentColor" stroke-width="1" /><path d="M8 11l3.2-6.4" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" /><circle cx="10.3" cy="6.4" r="1" fill="currentColor" /></svg>
      <span class="name">Metronome</span>
      <button type="button" class="ms" class:on={met.muted} title="Mute the click (the track stays)" aria-pressed={met.muted} onclick={() => setMetronomePrefs({ muted: !met.muted })}>M</button>
      <button type="button" class="ms more" title="Metronome settings (or right-click the track)" aria-label="Metronome settings" onclick={openPop}>
        <svg viewBox="0 0 16 16" aria-hidden="true"><circle cx="4" cy="8" r="1.3" fill="currentColor" /><circle cx="8" cy="8" r="1.3" fill="currentColor" /><circle cx="12" cy="8" r="1.3" fill="currentColor" /></svg>
      </button>
    </div>
    <div class="line">
      <Dial value={met.levelDb} min={-40} max={6} step={0.2} origin={-6} label="Click level" icon="volume" format={(v) => v.toFixed(1)} onChange={(v) => setMetronomePrefs({ levelDb: Math.round(v * 10) / 10 })} />
      <select class="sound" aria-label="Click sound" title="Click sound" value={met.sound} onchange={(e) => setMetronomePrefs({ sound: e.currentTarget.value as MetronomePrefs["sound"] })}>
        {#each METRONOME_SOUNDS as o (o.value)}<option value={o.value}>{o.label}</option>{/each}
      </select>
    </div>
    <div class="line">
      <span class="lbl">Tempo</span>
      {#key `${seg.id}:${seg.bpm}`}
        <input
          class="field bpm"
          value={bpmText(seg.bpm)}
          aria-label="Tempo in beats per minute"
          title={seg.id ? `Tempo from bar ${seg.bar + 1}, where the playhead is. Changes later in the song are on the Tempo strip.` : "Tempo where the playhead is. To change it later in the song, add a tempo change (right-click the ruler)."}
          onchange={(e) => commitBpm(e.currentTarget)}
          onkeydown={fieldKeys}
        />
      {/key}
      {#key `${seg.id}:${meterText(seg)}`}
        <input
          class="field meter"
          value={meterText(seg)}
          aria-label="Time signature"
          title="Time signature where the playhead is, such as 4/4, 3/4 or 6/8. Bars and beats, the click and MIDI clips follow it."
          onchange={(e) => commitMeter(e.currentTarget)}
          onkeydown={fieldKeys}
        />
      {/key}
    </div>
  </div>
  <div class="lane" class:muted={met.muted} style:width="{width}px">
    <svg width={width} height={ROW_H} aria-hidden="true">
      {#each lines as l, i (i)}
        <line x1={Math.round(l.x) + 0.5} x2={Math.round(l.x) + 0.5} y1={l.bar ? 8 : 26} y2={ROW_H - 8} class:bar={l.bar} />
      {/each}
    </svg>
  </div>
</div>

{#if pop}
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div class="scrim" onclick={() => (pop = null)} oncontextmenu={(e) => { e.preventDefault(); pop = null; }}></div>
  <div
    class="met-pop"
    role="dialog"
    aria-label="Metronome settings"
    tabindex="-1"
    style:left="{Math.min(pop.x, window.innerWidth - 280)}px"
    style:top="{Math.min(pop.y, window.innerHeight - 300)}px"
    onkeydown={(e) => {
      e.stopPropagation();
      if (e.key === "Escape") pop = null;
    }}
  >
    <div class="pop-title">Metronome</div>
    <label class="prow">Click
      <select value={met.when} onchange={(e) => setMetronomePrefs({ when: e.currentTarget.value === "recording" ? "recording" : "always" })}>
        <option value="always">Playback and recording</option>
        <option value="recording">Recording only</option>
      </select>
    </label>
    <label class="prow">Between beats
      <select value={met.subdivide} onchange={(e) => setMetronomePrefs({ subdivide: e.currentTarget.value as MetronomePrefs["subdivide"] })}>
        {#each METRONOME_SUBDIVIDE as o (o.value)}<option value={o.value}>{o.label}</option>{/each}
      </select>
    </label>
    <label class="prow"><input type="checkbox" checked={met.accent} onchange={(e) => setMetronomePrefs({ accent: e.currentTarget.checked })} /> Higher click on the first beat of each bar</label>
    <div class="pop-sep"></div>
    <button type="button" onclick={() => { addChangeAtPlayhead(); pop = null; }}>Add Tempo Change at Nearest Bar</button>
    <button type="button" onclick={() => { setMetronomePrefs({ on: false }); pop = null; }}>Turn Off Metronome</button>
    <p class="pop-note">The click plays through your speakers or headphones only. It is never in a mixdown or export.</p>
  </div>
{/if}

<style>
  .row {
    display: flex;
    flex: none;
    min-width: 0;
    border-bottom: 3px solid var(--track-gap);
  }
  .head {
    flex: none;
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 3px;
    box-sizing: border-box;
    padding: 2px 8px 2px 14px;
    border-right: 1px solid var(--border);
    border-left: 4px solid color-mix(in srgb, var(--text) 35%, transparent);
    background: var(--surface-2);
    font-size: var(--font-size-small);
  }
  .line {
    display: flex;
    gap: 6px;
    align-items: center;
    min-width: 0;
  }
  .icon {
    width: 13px;
    height: 13px;
    color: var(--text-dim);
  }
  .name {
    flex: 1;
    color: var(--text);
  }
  .ms {
    display: grid;
    place-items: center;
    width: 20px;
    height: 18px;
    padding: 0;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    background: var(--surface-3);
    color: var(--text-dim);
    font: inherit;
    font-size: 10px;
  }
  .ms.on {
    background: var(--accent-dim);
    color: #fff;
  }
  .ms svg {
    width: 12px;
    height: 12px;
  }
  .sound {
    height: 18px;
    padding: 0 2px;
    border: 1px solid var(--border);
    border-radius: 2px;
    background: var(--surface-3);
    color: var(--text);
    font: inherit;
    font-size: 11px;
  }
  .lbl {
    color: var(--text-dim);
  }
  .field {
    height: 18px;
    box-sizing: border-box;
    padding: 0 4px;
    border: 1px solid var(--border);
    border-radius: 2px;
    background: var(--surface-1);
    color: var(--value);
    font-family: var(--font-mono);
    font-size: 11px;
  }
  .field.bpm {
    width: 54px;
    text-align: right;
  }
  .field.meter {
    width: 38px;
    text-align: center;
  }
  .lane {
    flex: none;
    background: color-mix(in srgb, var(--text) 3%, var(--surface-1));
    overflow: hidden;
  }
  .lane line {
    stroke: color-mix(in srgb, var(--text) 45%, transparent);
    stroke-width: 1;
  }
  .lane line.bar {
    stroke: color-mix(in srgb, var(--accent) 70%, transparent);
    stroke-width: 2;
  }
  .lane.muted {
    opacity: 0.4;
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
  .pop-title {
    font-weight: 600;
  }
  .prow {
    display: flex;
    gap: 6px;
    align-items: center;
    color: var(--text-dim);
  }
  .prow select {
    flex: 1;
    min-width: 0;
  }
  .pop-sep {
    height: 1px;
    background: var(--border);
  }
  .met-pop button {
    justify-self: start;
  }
  .pop-note {
    margin: 0;
    color: var(--text-faint);
    font-size: 10px;
  }
</style>
