<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Altered Audio Editor.

  Phase Wheel: how two signals of what is playing line up, frequency by
  frequency. Each dot is one frequency: its angle is the phase between
  the two (straight up when they move together, straight down when they
  are opposite and would cancel in mono), its distance from the center
  how loud it is, its color its frequency from 20 Hz to 20 kHz (red to
  magenta by default; right-click to pick the two ends). The bar
  under the wheel is the overall correlation: +1 the same, 0 unrelated,
  -1 opposite. Compares left and right, or mid and side. Smaller sample
  counts are lighter on slower computers and react faster; larger ones
  resolve low frequencies better. It redraws up to 60 times a second;
  15 or 30 is lighter.
-->
<script lang="ts">
  import { onDestroy } from "svelte";
  import { scopePhase, type ScopeSignal } from "../../app/core";
  import ColorPairPopover from "../../app/ColorPairPopover.svelte";
  import { hexToRgb, PHASE_HIGH, PHASE_LOW, ramp } from "../../app/colors";
  import { settings } from "../../stores/settings";
  import type { TransportState, TransportStore } from "../../stores/transport";

  interface Props {
    transport: TransportStore;
    /** The Waveform editor's transport. */
    wave?: TransportStore | null;
  }

  const { transport, wave = null }: Props = $props();

  const SIZES = [1024, 2048, 4096, 8192, 16384];
  const SIGNALS: { id: ScopeSignal; label: string }[] = [
    { id: "left", label: "Left" },
    { id: "right", label: "Right" },
    { id: "mid", label: "Mid (L+R)" },
    { id: "side", label: "Side (L-R)" },
  ];

  let size = $state(4096);
  /** Pictures a second, at most. */
  let fps = $state(60);
  const RATES = [15, 30, 60];
  let appColors = $state(settings.current.colors);
  $effect(() => settings.subscribe((v) => (appColors = v.colors)));
  /** 64 colors from the low end to the high end, so drawing never works one out per dot. */
  const palette = $derived.by(() => {
    const low = hexToRgb(appColors.phaseLow || PHASE_LOW)!;
    const high = hexToRgb(appColors.phaseHigh || PHASE_HIGH)!;
    return Array.from({ length: 64 }, (_, i) => ramp(low, high, i / 63).map(Math.round) as [number, number, number]);
  });
  const legend = $derived(`linear-gradient(to right, ${palette.filter((_, i) => i % 8 === 0 || i === 63).map(([r, g, b]) => `rgb(${r} ${g} ${b})`).join(", ")})`);
  let colorPop = $state<{ x: number; y: number } | null>(null);
  function setColors(low: string, high: string) {
    const c = settings.current;
    settings.set({ ...c, colors: { ...c.colors, phaseLow: low, phaseHigh: high } });
  }
  let a = $state<ScopeSignal>("left");
  let b = $state<ScopeSignal>("right");
  let correlation = $state(0);
  let canvas = $state<HTMLCanvasElement | null>(null);
  let box = $state({ w: 0, h: 0 });

  let multi = $state<TransportState | null>(null);
  let waveS = $state<TransportState | null>(null);
  $effect(() => transport.subscribe((v) => (multi = v)));
  $effect(() => wave?.subscribe((v) => (waveS = v)));
  const playing = $derived(!!multi?.playing || !!waveS?.playing);

  /**
   * A new picture on each screen refresh while playing, up to `fps` a
   * second, and never more than one request out at a time.
   */
  let frame: number | null = null;
  let busy = false;
  let last = 0;
  async function fetchAndDraw() {
    busy = true;
    try {
      const r = await scopePhase(size, a, b);
      correlation = r.correlation;
      draw(r.bins);
    } catch {
      // Nothing playing yet, or the core is busy: the next frame tries again.
    } finally {
      busy = false;
    }
  }
  function loop(now: number) {
    frame = null;
    if (!busy && !document.hidden && now - last >= 1000 / fps - 2) {
      last = now;
      void fetchAndDraw();
    }
    if (playing) frame = requestAnimationFrame(loop);
  }
  $effect(() => {
    // Start while something plays (or after a setting changes); draw once otherwise.
    void size;
    void a;
    void b;
    void fps;
    void palette;
    if (frame !== null) cancelAnimationFrame(frame);
    frame = null;
    if (playing) frame = requestAnimationFrame(loop);
    else void fetchAndDraw();
  });
  onDestroy(() => {
    if (frame !== null) cancelAnimationFrame(frame);
  });

  function css(name: string, fallback: string): string {
    return getComputedStyle(document.documentElement).getPropertyValue(name).trim() || fallback;
  }

  function draw(bins: Float32Array) {
    const c = canvas;
    if (!c) return;
    const side = Math.max(60, Math.min(box.w, box.h));
    const dpr = window.devicePixelRatio || 1;
    if (c.width !== Math.round(side * dpr)) {
      c.width = Math.round(side * dpr);
      c.height = Math.round(side * dpr);
      c.style.width = `${side}px`;
      c.style.height = `${side}px`;
    }
    const g = c.getContext("2d");
    if (!g) return;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    const mid = side / 2;
    const R = mid - 18;
    // Fade the last picture a little, so dots leave a short trail.
    g.globalCompositeOperation = "source-over";
    g.fillStyle = "rgba(10, 10, 12, 0.55)";
    g.fillRect(0, 0, side, side);
    // Rings and axes.
    g.strokeStyle = "rgba(255, 255, 255, 0.10)";
    g.lineWidth = 1;
    for (const f of [0.25, 0.5, 0.75]) {
      g.beginPath();
      g.arc(mid, mid, R * f, 0, Math.PI * 2);
      g.stroke();
    }
    g.beginPath();
    g.moveTo(mid - R, mid);
    g.lineTo(mid + R, mid);
    g.moveTo(mid, mid - R);
    g.lineTo(mid, mid + R);
    g.stroke();
    g.strokeStyle = css("--accent", "#3fa9f5");
    g.lineWidth = 1.5;
    g.beginPath();
    g.arc(mid, mid, R, 0, Math.PI * 2);
    g.stroke();
    g.fillStyle = "rgba(255, 255, 255, 0.55)";
    g.font = "10px system-ui, sans-serif";
    g.textAlign = "center";
    g.fillText("in phase", mid, 11);
    g.fillText("opposite", mid, side - 4);
    // Inside the ring, just above the axis, so they never fall off the edge.
    g.textAlign = "right";
    g.fillText("+90°", mid + R - 4, mid - 4);
    g.textAlign = "left";
    g.fillText("-90°", mid - R + 4, mid - 4);
    // The dots: angle from straight up is the phase, distance the level, color the pitch.
    g.globalCompositeOperation = "lighter";
    for (let i = 0; i + 2 < bins.length; i += 3) {
      const ph = bins[i]!;
      const level = bins[i + 1]!;
      const pitch = bins[i + 2]!;
      const r = level * R;
      const x = mid + r * Math.sin(ph);
      const y = mid - r * Math.cos(ph);
      const [cr, cg, cb] = palette[Math.max(0, Math.min(63, Math.round(pitch * 63)))]!;
      g.fillStyle = `rgba(${cr}, ${cg}, ${cb}, ${0.2 + 0.55 * level})`;
      g.beginPath();
      g.arc(x, y, 1.6 + 1.6 * level, 0, Math.PI * 2);
      g.fill();
    }
    g.globalCompositeOperation = "source-over";
  }

  const corrText = $derived(`${correlation >= 0 ? "+" : ""}${correlation.toFixed(2)}`);
</script>

<section class="phase" aria-label="Phase Wheel">
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="wheel"
    bind:clientWidth={box.w}
    bind:clientHeight={box.h}
    oncontextmenu={(e) => {
      e.preventDefault();
      colorPop = { x: e.clientX, y: e.clientY };
    }}
    title="Right-click for the colors"
  >
    <canvas bind:this={canvas} aria-label="Phase of each frequency between the two signals"></canvas>
    {#if !playing}<span class="idle">Shows what is playing. Press play.</span>{/if}
  </div>
  <div class="legend" title="Dot colors by frequency. Right-click the wheel to change them.">
    <span class="lab">20 Hz</span>
    <span class="bar" style:background={legend}></span>
    <span class="lab">20 kHz</span>
  </div>
  <div class="corr" title="Correlation: +1 the same in both, 0 unrelated, -1 opposite (cancels in mono)">
    <span class="lab">-1</span>
    <div class="track">
      <div class="zero"></div>
      <div class="mark" class:neg={correlation < 0} style:left="{((correlation + 1) / 2) * 100}%"></div>
    </div>
    <span class="lab">+1</span>
    <span class="val" class:neg={correlation < 0}>{corrText}</span>
  </div>
  <div class="controls">
    <label>Signal <select bind:value={a}>{#each SIGNALS as s (s.id)}<option value={s.id}>{s.label}</option>{/each}</select></label>
    <label>Compare to <select bind:value={b}>{#each SIGNALS as s (s.id)}<option value={s.id}>{s.label}</option>{/each}</select></label>
    <label title="Frames analysed each time. Fewer is lighter on slower computers and reacts faster; more shows low frequencies in finer detail.">
      Samples
      <select bind:value={size}>{#each SIZES as n (n)}<option value={n}>{n}</option>{/each}</select>
    </label>
    <label title="Pictures a second, at most. Lower is lighter on slower computers.">
      Refresh
      <select bind:value={fps}>{#each RATES as r (r)}<option value={r}>{r} fps</option>{/each}</select>
    </label>
  </div>
  <p class="note">The app plays in stereo, so the choices are left, right, mid and side.</p>
</section>

{#if colorPop}
  <ColorPairPopover
    x={colorPop.x}
    y={colorPop.y}
    title="Phase Wheel colors"
    lowLabel="Low notes (20 Hz)"
    highLabel="High notes (20 kHz)"
    low={appColors.phaseLow}
    high={appColors.phaseHigh}
    lowDefault={PHASE_LOW}
    highDefault={PHASE_HIGH}
    onChange={setColors}
    onClose={() => (colorPop = null)}
  />
{/if}

<style>
  .phase {
    display: flex;
    flex-direction: column;
    gap: 6px;
    height: 100%;
    min-height: 0;
    padding: 6px;
    box-sizing: border-box;
    background: var(--surface-1);
    font-size: var(--font-size-small);
  }
  .wheel {
    position: relative;
    display: grid;
    flex: 1;
    place-items: center;
    min-height: 60px;
    overflow: hidden;
  }
  canvas {
    border-radius: 4px;
    background: #0a0a0c;
  }
  .idle {
    position: absolute;
    top: 2px;
    left: 4px;
    color: var(--text-faint);
    pointer-events: none;
  }
  .legend {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .legend .bar {
    flex: 1;
    height: 6px;
    border-radius: 3px;
  }
  .corr {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .lab {
    color: var(--text-faint);
  }
  .track {
    position: relative;
    flex: 1;
    height: 8px;
    border-radius: 4px;
    background: linear-gradient(to right, #b8443c, #3a3a40 50%, #3d8f6a);
  }
  .zero {
    position: absolute;
    top: -2px;
    bottom: -2px;
    left: 50%;
    width: 1px;
    background: var(--text-dim);
  }
  .mark {
    position: absolute;
    top: -3px;
    width: 4px;
    height: 14px;
    margin-left: -2px;
    border-radius: 2px;
    background: var(--text);
    transition: left 60ms linear;
  }
  .mark.neg,
  .val.neg {
    background: #e0574d;
  }
  .val {
    min-width: 40px;
    padding: 0 4px;
    border-radius: 2px;
    font-family: var(--font-mono);
    text-align: right;
  }
  .val.neg {
    color: #fff;
  }
  .controls {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  label {
    display: flex;
    gap: 4px;
    align-items: center;
    color: var(--text-dim);
  }
  select {
    height: 22px;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    background: var(--surface-2);
    color: var(--text);
    font: inherit;
  }
  .note {
    margin: 0;
    color: var(--text-faint);
  }
</style>
