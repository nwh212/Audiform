<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The Audiform contributors.
  This file is part of Audiform, based on Altered Audio Editor.

  Visualizer: moving pictures that follow what is playing. Pick a look
  (Lava Lamp, Bars, Waves, Plasma, Ripples, Starfield or Radial), a set of
  colors, how strongly the picture reacts, and how many pictures a second
  it draws (15 and 30 are lighter on slower computers). "Change every 30
  seconds" steps through the looks on its own. While stopped it keeps
  moving gently and shows the audio just before the playhead. It only
  draws while its tab is in front. Its choices are kept for every file.
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
  import type { Visual } from "./presets/types";

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
    transport.session.subscribe((d) => {
      const at = d?.view.playhead;
      if (at !== undefined && (follow.wave || at !== follow.at)) follow = { wave: false, at };
    }),
  );
  $effect(() =>
    wave?.session.subscribe((d) => {
      const at = d?.view.playhead;
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
    if (ctx) visual.draw({ ctx, w: canvas.width, h: canvas.height, a, pal: palette });
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
    <label>
      <span>Look</span>
      <select value={presetId} onchange={(e) => { presetId = e.currentTarget.value; save("preset", presetId); }}>
        {#each PRESETS as p (p.id)}<option value={p.id}>{p.name}</option>{/each}
      </select>
    </label>
    <label>
      <span>Colors</span>
      <select value={scheme} onchange={(e) => { scheme = e.currentTarget.value as Scheme; save("scheme", scheme); }}>
        {#each SCHEMES as s (s.id)}<option value={s.id}>{s.label}</option>{/each}
      </select>
    </label>
    <label>
      <span>Reaction</span>
      <input type="range" min="0.4" max="2.5" step="0.1" value={sens} oninput={(e) => { sens = Number(e.currentTarget.value); save("sens", sens); }} />
    </label>
    <label>
      <span>Pictures a second</span>
      <select value={fps} onchange={(e) => { fps = Number(e.currentTarget.value); save("fps", fps); }}>
        {#each FPS as f (f)}<option value={f}>{f}</option>{/each}
      </select>
    </label>
    <label class="check">
      <input type="checkbox" checked={auto} onchange={(e) => { auto = e.currentTarget.checked; save("auto", auto); }} />
      <span>Change every 30 seconds</span>
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
  .bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px var(--space-3);
    padding: var(--space-1) var(--space-2);
    border-bottom: 1px solid var(--border);
  }
  label {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    color: var(--text-dim);
  }
  .check {
    gap: 4px;
  }
  input[type="range"] {
    width: 80px;
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
