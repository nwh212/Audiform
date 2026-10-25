<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Altered Audio Editor.

  Levels panel: the output level, left and right, on a dB scale from
  -60 to 0. It follows whatever is playing: the multitrack session, the
  Waveform editor's file, or a preview from Files, Drive Manager or
  Libraries. Click a red over light to clear it.
-->
<script lang="ts">
  import type { PreviewMeter, PreviewMeterState } from "../../stores/preview-meter";
  import { meterFraction, type MeterLevel, type TransportState, type TransportStore } from "../../stores/transport";
  import type { SessionStore, SpaceLayout } from "../../stores/session";
  import { channelNames } from "../space/space";

  interface Props {
    transport: TransportStore;
    /** The Waveform editor's transport. */
    wave?: TransportStore | null;
    preview?: PreviewMeter | null;
    /** The open session: in 3D Space sessions every speaker channel shows. */
    session?: SessionStore | null;
  }

  const { transport, wave = null, preview = null, session = null }: Props = $props();

  let space = $state<SpaceLayout | null>(null);
  $effect(() =>
    session?.subscribe((d) => {
      space = d?.space?.layout ?? null;
    }),
  );

  type Source = "multi" | "wave" | "preview";
  let multiS = $state<TransportState | null>(null);
  let waveS = $state<TransportState | null>(null);
  let previewS = $state<PreviewMeterState | null>(null);
  // Whatever played last stays on screen (with its over lights) after it stops.
  let lastSource = $state<Source>("multi");
  $effect(() =>
    transport.subscribe((v) => {
      multiS = v;
      if (v.playing) lastSource = "multi";
    }),
  );
  $effect(() =>
    wave?.subscribe((v) => {
      waveS = v;
      if (v.playing) lastSource = "wave";
    }),
  );
  $effect(() =>
    preview?.subscribe((v) => {
      previewS = v;
      if (v.playing) lastSource = "preview";
    }),
  );
  const source = $derived<Source>(waveS?.playing ? "wave" : multiS?.playing ? "multi" : previewS?.playing ? "preview" : lastSource);
  const s = $derived<{ master: [MeterLevel, MeterLevel] } | null>(source === "wave" ? waveS : source === "preview" ? previewS : multiS);
  /** 3D Space sessions: one row per speaker channel of the master. */
  const rows = $derived.by((): { label: string; m: MeterLevel | undefined; name: string }[] => {
    // A 3D Space session shows every speaker channel, stopped or playing (unless another editor is playing).
    const names = space ? channelNames(space) : [];
    if (names.length > 2 && source === "multi") {
      const bed = multiS?.bed ?? [];
      return names.map((n, k) => ({ label: n, m: bed.length === names.length ? bed[k] : undefined, name: `${n} level` }));
    }
    return [0, 1].map((ch) => ({ label: ch === 0 ? "L" : "R", m: s?.master[ch], name: ch === 0 ? "Left level" : "Right level" }));
  });
  function clearOver() {
    if (source === "wave") wave?.clearOver();
    else if (source === "preview") preview?.clearOver();
    else transport.clearOver();
  }

  const TICKS = [-60, -48, -36, -24, -18, -12, -9, -6, -3, 0];
  const pos = (db: number) => ((db + 60) / 60) * 100;

  function db(m: MeterLevel | undefined): string {
    if (!m || m.peak <= 0.00001) return "-inf";
    const v = 20 * Math.log10(m.peak);
    return v > 0 ? `+${v.toFixed(1)}` : v.toFixed(1);
  }
</script>

<section class="levels" class:many={rows.length > 2} style:--rows={rows.length} aria-label="Levels">
  {#each rows as row (row.label)}
    {@const m = row.m}
    <div class="row">
      <span class="ch">{row.label}</span>
      <div
        class="bar"
        role="meter"
        aria-label={row.name}
        aria-valuemin={-60}
        aria-valuemax={0}
        aria-valuenow={m && m.peak > 0 ? Math.max(-60, Math.round(20 * Math.log10(m.peak))) : -60}
      >
        <div class="cover" style:width="{(1 - meterFraction(m?.level ?? 0)) * 100}%"></div>
        {#if m && m.peak > 0}<div class="peak" style:left="{meterFraction(m.peak) * 100}%"></div>{/if}
      </div>
      <button
        type="button"
        class="over"
        class:on={m?.over}
        title="Went over full scale. Click to clear."
        aria-label="Clear over light"
        onclick={clearOver}
      ></button>
      <span class="val">{db(m)}</span>
    </div>
  {/each}
  <div class="scale" aria-hidden="true">
    <span></span>
    <div class="ticks">
      {#each TICKS as t (t)}
        <span style:left="{pos(t)}%">{t}</span>
      {/each}
    </div>
  </div>
</section>

<style>
  .levels {
    display: grid;
    align-content: center;
    gap: 4px;
    height: 100%;
    padding: 6px 12px;
    box-sizing: border-box;
    background: var(--surface-1);
  }

  /* Every speaker channel shares the panel's height, so all of them fit. */
  .levels.many {
    grid-template-rows: repeat(var(--rows), minmax(0, 1fr)) auto;
    align-content: stretch;
    gap: 1px;
    padding-top: 3px;
    padding-bottom: 3px;
  }

  .levels.many .row {
    min-height: 0;
    overflow: hidden;
  }

  .levels.many .bar,
  .levels.many .over {
    height: auto;
    min-height: 2px;
    max-height: 9px;
    align-self: stretch;
  }

  .levels.many .ch,
  .levels.many .val {
    font-size: 9px;
    line-height: 1;
  }

  .row,
  .scale {
    display: grid;
    grid-template-columns: 26px 1fr 12px 44px;
    gap: 6px;
    align-items: center;
  }

  .ch {
    color: var(--text-dim);
    font-size: var(--font-size-small);
  }

  /* Green to -12 dB, yellow to -3 dB, red above. The cover hides the
     part of the scale above the current level. */
  .bar {
    position: relative;
    height: 9px;
    overflow: hidden;
    background: linear-gradient(to right, #3d8f6a 0 var(--meter-yellow, 80%), #c9b33f var(--meter-yellow, 80%) var(--meter-red, 95%), #e5534b var(--meter-red, 95%) 100%);
  }

  .cover {
    position: absolute;
    top: 0;
    right: 0;
    height: 100%;
    background: var(--surface-0);
  }

  .peak {
    position: absolute;
    top: 0;
    width: 2px;
    height: 100%;
    background: var(--text);
    transform: translateX(-1px);
  }

  .over {
    width: 12px;
    height: 9px;
    padding: 0;
    border: 1px solid var(--border-strong);
    background: var(--surface-0);
  }

  .over.on {
    border-color: var(--danger);
    background: var(--danger);
  }

  .val {
    color: var(--value);
    font-family: var(--font-mono);
    font-size: var(--font-size-small);
    text-align: right;
  }

  .ticks {
    position: relative;
    height: 12px;
  }

  .ticks span {
    position: absolute;
    color: var(--text-faint);
    font-size: 9px;
    transform: translateX(-50%);
  }
</style>
