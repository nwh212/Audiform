<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The Audiform contributors.
  This file is part of Audiform, based on Altered Audio Editor.

  Visualizer: moving pictures that follow what is playing. Pick a look
  (Lava Lamp, Plasma, Tunnel, Electric, Bars, Waves, Ripples or
  Starfield), a set of colors, how strongly the picture reacts, how fine
  the soft looks are drawn (Fine and Finest cost more), and how many
  pictures a second it draws (15 and 30 are lighter on slower computers).
  "Change every 30 seconds" steps through the looks on its own. While
  stopped it keeps moving gently and shows the audio just before the
  playhead. It only draws while its tab is in front. Its choices are kept
  for every file.
-->
<script lang="ts">
  import { onDestroy } from "svelte";
  import { scopeSpectrum } from "../../app/core";
  import { themeColor } from "../../app/colors";
  import { appearanceTick } from "../../stores/appearance";
  import { prefs } from "../../stores/prefs";
  import { settings } from "../../stores/settings";
  import type { TransportState, TransportStore } from "../../stores/transport";
  import { Analyser, BANDS } from "./analysis";
  import { paletteFor, SCHEMES, type Scheme } from "./palette";
  import { PRESETS, presetById } from "./presets";
  import { DETAILS, type Visual } from "./presets/types";

  interface Props {
    transport: TransportStore;
    /** The Waveform editor's transport. */
    wave?: TransportStore | null;
  }

  const { transport, wave = null }: Props = $props();

  const FPS = [15, 30, 60];
  const KEY = "visualizer.";
  const stored = <T,>(k: string, ok: (v: unknown) => v is T, d: T): T => {
    const v = prefs.raw(KEY + k);
    return ok(v) ? v : d;
  };
  let presetId = $state(stored("preset", (v): v is string => typeof v === "string", PRESETS[0]!.id));
  let scheme = $state<Scheme>(stored("scheme", (v): v is Scheme => SCHEMES.some((s) => s.id === v), "theme"));
  let sens = $state(stored("sens", (v): v is number => typeof v === "number" && v >= 0.4 && v <= 2.5, 1));
  let fps = $state(stored("fps", (v): v is number => FPS.includes(v as number), 30));
  let detail = $state(stored("detail", (v): v is number => DETAILS.some((d) => d.value === v), 1));
  let auto = $state(stored("auto", (v): v is boolean => typeof v === "boolean", false));
  function save(k: string, v: unknown) {
    prefs.set(KEY + k, v);
  }

  let appColors = $state(settings.current.colors);
  $effect(() =>
    settings.subscribe((v) => {
      appColors = v.colors;
    }),
  );
  let looks = $state(0);
  $effect(() => appearanceTick.subscribe((v) => (looks = v)));
  const palette = $derived((void looks, paletteFor(scheme, appColors.phaseLow || themeColor("--phase-low"), appColors.phaseHigh || themeColor("--phase-high"))));

  let canvas = $state<HTMLCanvasElement | null>(null);
  let box = $state({ w: 0, h: 0 });
  let visual: Visual | null = null;
  let visualId = "";

  let multi = $state<TransportState | null>(null);
  let waveS = $state<TransportState | null>(null);
  $effect(() => transport.subscribe((v) => (multi = v)));
  $effect(() => wave?.subscribe((v) => (waveS = v)));
  const playing = $derived(!!multi?.playing || !!waveS?.playing);

  /** While stopped: the playhead of whichever editor moved it last. */
  let follow = $state<{ wave: boolean; at: number }>({ wave: false, at: -1 });
  $effect(() =>
    transport.session.subscribeView((v) => {
      const at = v?.playhead;
      if (at !== undefined && (follow.wave || at !== follow.at)) follow = { wave: false, at };
    }),
  );
  $effect(() =>
    wave?.session.subscribeView((v) => {
      const at = v?.playhead;
      if (at !== undefined && (!follow.wave || at !== follow.at)) follow = { wave: true, at };
    }),
  );

  // The newest spectrum, asked for in the background: pictures never wait for the core.
  let spectrum: Float32Array = new Float32Array(BANDS).fill(-120);
  let asking = false;
  async function ask() {
    if (asking) return;
    asking = true;
    try {
      const s = playing ? await scopeSpectrum(2048, BANDS, null, false) : follow.at >= 0 ? await scopeSpectrum(2048, BANDS, follow.at, follow.wave) : new Float32Array(0);
      spectrum = s.length ? s : new Float32Array(BANDS).fill(-120);
    } catch {
      spectrum = new Float32Array(BANDS).fill(-120);
    } finally {
      asking = false;
    }
  }

  const analyser = new Analyser();
  let frame: number | null = null;
  let last = 0;
  let switched = 0;
  function loop(now: number) {
    frame = requestAnimationFrame(loop);
    if (document.hidden || box.w < 4 || box.h < 4 || !canvas) return;
    const step = 1000 / fps;
    if (now - last < step - 2) return;
    const dt = last === 0 ? 1 / fps : Math.min(0.2, (now - last) / 1000);
    last = now;
    void ask();
    if (visualId !== presetId || !visual) {
      visualId = presetId;
      visual = presetById(presetId).create();
      visual.resize(box.w, box.h);
    }
    const a = analyser.feed(spectrum, sens, dt, playing ? 1 : 0.28);
    if (auto && now - switched > 30000) {
      switched = now;
      const i = PRESETS.findIndex((p) => p.id === presetId);
      presetId = PRESETS[(i + 1) % PRESETS.length]!.id;
      save("preset", presetId);
    }
    const ctx = canvas.getContext("2d");
    if (ctx) visual.draw({ ctx, w: canvas.width, h: canvas.height, a, pal: palette, detail });
  }
  $effect(() => {
    // Runs while the tab is in front (an inactive tab has no size) and stops otherwise.
    const run = box.w >= 4 && box.h >= 4;
    if (run && frame === null) {
      last = 0;
      frame = requestAnimationFrame(loop);
    } else if (!run && frame !== null) {
      cancelAnimationFrame(frame);
      frame = null;
    }
  });
  $effect(() => {
    // A new size or look starts the picture afresh.
    void box.w;
    void box.h;
    void presetId;
    visual = null;
  });
  onDestroy(() => {
    if (frame !== null) cancelAnimationFrame(frame);
  });

  let area = $state<HTMLDivElement | null>(null);
  $effect(() => {
    if (!area) return;
    const dpr = Math.min(window.devicePixelRatio || 1, 1.5);
    const ro = new ResizeObserver(() => {
      const w = Math.floor((area?.clientWidth ?? 0) * dpr);
      const h = Math.floor((area?.clientHeight ?? 0) * dpr);
      if (canvas && (canvas.width !== w || canvas.height !== h)) {
        canvas.width = w;
        canvas.height = h;
      }
      box = { w, h };
    });
    ro.observe(area);
    return () => ro.disconnect();
  });
</script>

<div class="panel">
  <div class="bar">
    <div class="group">
      <label class="field">
        <span>Look</span>
        <select value={presetId} onchange={(e) => { presetId = e.currentTarget.value; save("preset", presetId); }}>
          {#each PRESETS as p (p.id)}<option value={p.id}>{p.name}</option>{/each}
        </select>
      </label>
      <label class="field">
        <span>Colors</span>
        <select value={scheme} onchange={(e) => { scheme = e.currentTarget.value as Scheme; save("scheme", scheme); }}>
          {#each SCHEMES as s (s.id)}<option value={s.id}>{s.label}</option>{/each}
        </select>
      </label>
    </div>
    <div class="group">
      <label class="field" title="How strongly the picture reacts to the music">
        <span>Reaction</span>
        <input type="range" min="0.4" max="2.5" step="0.1" value={sens} oninput={(e) => { sens = Number(e.currentTarget.value); save("sens", sens); }} />
      </label>
    </div>
    <div class="group">
      <label class="field" title="How finely the soft looks (Lava Lamp, Plasma) are drawn. Fine and Finest take more of the computer.">
        <span>Detail</span>
        <select value={detail} onchange={(e) => { detail = Number(e.currentTarget.value); save("detail", detail); }}>
          {#each DETAILS as d (d.value)}<option value={d.value}>{d.label}</option>{/each}
        </select>
      </label>
      <label class="field" title="Pictures a second. Lower is lighter on slower computers.">
        <span>Rate</span>
        <select value={fps} onchange={(e) => { fps = Number(e.currentTarget.value); save("fps", fps); }}>
          {#each FPS as f (f)}<option value={f}>{f}</option>{/each}
        </select>
      </label>
    </div>
    <label class="field check" title="Step through the looks on its own">
      <input type="checkbox" checked={auto} onchange={(e) => { auto = e.currentTarget.checked; save("auto", auto); }} />
      <span>Change every 30 s</span>
    </label>
  </div>
  <div class="area" bind:this={area}>
    <canvas bind:this={canvas}></canvas>
  </div>
</div>

<style>
  .panel {
    display: grid;
    grid-template-rows: auto 1fr;
    height: 100%;
    min-height: 0;
    background: var(--surface-1);
    color: var(--text);
    font-size: var(--font-size-small);
  }
  /* A strip of grouped controls over the picture: each group a rounded
     well, the labels small and quiet, the picture's own glow under it. */
  .bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px var(--space-2);
    padding: 5px var(--space-2);
    border-bottom: 1px solid var(--border);
    background:
      linear-gradient(180deg, color-mix(in srgb, var(--surface-2) 70%, transparent), transparent),
      var(--surface-1);
  }
  .group {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 2px 4px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: color-mix(in srgb, var(--surface-0) 70%, transparent);
  }
  .field {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 0 4px;
    color: var(--text-dim);
  }
  .field > span {
    font-size: 10px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-faint);
  }
  .field + .field {
    border-left: 1px solid var(--border);
  }
  .field select {
    height: 20px;
    padding: 0 4px;
    border: 0;
    border-radius: 999px;
    background: transparent;
    color: var(--text);
    font: inherit;
  }
  .field select:hover,
  .field select:focus-visible {
    background: color-mix(in srgb, var(--accent) 18%, transparent);
    outline: none;
  }
  .check {
    gap: 5px;
    padding: 2px 8px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: color-mix(in srgb, var(--surface-0) 70%, transparent);
  }
  .check > span {
    text-transform: none;
    letter-spacing: 0;
    font-size: var(--font-size-small);
    color: var(--text-dim);
  }
  .check input {
    margin: 0;
    accent-color: var(--accent);
  }
  input[type="range"] {
    width: 86px;
    height: 14px;
    accent-color: var(--accent);
  }
  .area {
    position: relative;
    min-height: 0;
    overflow: hidden;
    background: #04071a;
  }
  canvas {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    display: block;
  }
</style>
