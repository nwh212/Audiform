<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  The spectrum under the waveform in the Waveform editor: time across,
  frequency up, loudness as color, with an optional pitch line. The
  spectral tools select areas here (box, lasso, brush) or repair a
  painted spot straight away. The mouse wheel over the frequency scale
  zooms the frequencies around the pointer (never past what the sample
  rate holds); over the spectrum it zooms time, as over the waveform.
  Right-click for the frequency zoom, Hertz or note names, a more linear
  or more logarithmic scale, the analysis resolution and the colors.
-->
<script lang="ts">
  import { wavePitch, waveSpectrogram } from "../../app/core";
  import type { SpectralTool } from "../../stores/editor";
  import type { SessionDoc } from "../../stores/session";
  import { spectralSelection } from "../../stores/wave";
  import { sampleAt, xOf, type Viewport } from "./timeline";
  import { freqAt, fracOf, freqTicks, FULL_VIEW, rowEdges, SPECTRUM_LUT, viewFrac, viewFreq, viewRange, zoomFreq, type SpectralShape } from "./spectrum";
  import { FFT_SIZES, spectrumView, type SpectrumViewState } from "../../stores/spectrum-view";
  import { noteName } from "./midi";
  import ContextMenu, { type ContextItem } from "../../app/ContextMenu.svelte";
  import { settings } from "../../stores/settings";
  import { SPECTRUM_HIGH, SPECTRUM_LOW, spectrumLut } from "../../app/colors";

  interface Props {
    doc: SessionDoc;
    view: Viewport;
    /** Width of the time area, without the scale. */
    width: number;
    scaleWidth: number;
    total: number;
    playhead: number;
    selection: { start: number; end: number } | null;
    tool: SpectralTool;
    brushSize: number;
    showSpectrum: boolean;
    showPitch: boolean;
    onSeek: (sample: number) => void;
    onSelectTime: (sel: { start: number; end: number } | null) => void;
    /** Runs a named action (the spectral edits). */
    run: (id: string) => void;
    tip: (label: string, id: string) => string;
    /** The wheel over the spectrum itself: time zoom and scroll, as over the waveform. */
    onTimeWheel?: (e: WheelEvent) => void;
    /** Opens the spectrogram color picker at a point. */
    onColors?: (x: number, y: number) => void;
  }

  const { doc, view, width, scaleWidth, total, playhead, selection, tool, brushSize, showSpectrum, showPitch, onSeek, onSelectTime, run, tip, onTimeWheel = () => {}, onColors = () => {} }: Props = $props();

  let sv = $state<SpectrumViewState>({ ...FULL_VIEW, notes: false, fft: 2048 });
  $effect(() => spectrumView.subscribe((v) => (sv = v)));
  const setSv = (patch: Partial<SpectrumViewState>) => spectrumView.update((v) => ({ ...v, ...patch }));

  let height = $state(200);
  let canvas = $state<HTMLCanvasElement | null>(null);
  let image = $state<{ data: Uint8Array; columns: number; rows: number; start: number; end: number } | null>(null);
  let pitch = $state<{ hz: number[]; start: number; end: number } | null>(null);
  let note = $state<string | null>(null);
  let shapes = $state<readonly SpectralShape[] | null>(null);
  $effect(() =>
    spectralSelection.subscribe((r) => {
      shapes = r;
    }),
  );
  /** The shape being drawn now, before it joins the selection. */
  let drawing = $state<SpectralShape | null>(null);
  let hover = $state<{ x: number; y: number } | null>(null);

  const rate = $derived(doc.sample_rate);
  const nyquist = $derived(rate / 2);
  const range = $derived({
    start: Math.max(0, Math.round(view.scroll)),
    end: Math.min(total, Math.round(view.scroll + width * view.spp)),
  });

  // Fetch the spectrogram for what is in view, a moment after it settles.
  let seq = 0;
  $effect(() => {
    const { start, end } = range;
    const columns = Math.max(1, Math.min(2048, Math.round(width)));
    const rows = Math.max(16, Math.min(512, Math.round(height)));
    const edges = rowEdges(rows, sv, nyquist);
    const fft = sv.fft;
    void doc;
    const my = ++seq;
    if (end <= start || width <= 0) {
      image = null;
      return;
    }
    const t = setTimeout(() => {
      waveSpectrogram(start, end, columns, rows, edges, fft)
        .then((data) => {
          if (my !== seq) return;
          image = { data, columns, rows, start, end };
          note = null;
        })
        .catch((e) => {
          if (my !== seq) return;
          image = null;
          note = String(e);
        });
    }, 90);
    return () => clearTimeout(t);
  });

  // The pitch line, when shown.
  let pseq = 0;
  $effect(() => {
    const { start, end } = range;
    const on = showPitch;
    void doc;
    const my = ++pseq;
    if (!on || end <= start || width <= 0) {
      pitch = null;
      return;
    }
    const columns = Math.max(1, Math.min(1024, Math.round(width / 2)));
    const t = setTimeout(() => {
      wavePitch(start, end, columns)
        .then((hz) => {
          if (my === pseq) pitch = { hz, start, end };
        })
        .catch(() => {
          if (my === pseq) pitch = null;
        });
    }, 120);
    return () => clearTimeout(t);
  });

  // Places on screen for frequencies, through the view's scale. Selection
  // shapes keep the edits' own log axis (`frac` below), so they stay on the
  // same sounds whatever the view shows.
  const yOf = (hz: number) => (1 - viewFrac(hz, sv, nyquist)) * height;
  const yOfFrac = (f: number) => yOf(freqAt(f, nyquist));
  const fracAtY = (y: number) => fracOf(viewFreq(1 - y / height, sv, nyquist), nyquist);

  function css(name: string): string {
    return getComputedStyle(document.documentElement).getPropertyValue(name).trim() || "#888";
  }

  /** The scale's marks: frequencies in Hz, or note names at their pitches. */
  function scaleMarks(h: number): [number, string][] {
    if (!sv.notes) return freqTicks(sv, nyquist, h).map((f) => [f, label(f)]);
    const [lo, hi] = viewRange(sv, nyquist);
    const out: [number, string][] = [];
    let lastY = Infinity;
    // Every C, or every note when zoomed in far enough to read them.
    const every = yOf(493.88) - yOf(523.25) >= 14 ? 1 : 12;
    for (let p = 0; p <= 135; p++) {
      const f = 440 * Math.pow(2, (p - 69) / 12);
      if (f <= lo || f >= hi) continue;
      const y = yOf(f);
      if (lastY - y < 14) continue;
      if (every === 12 && p % 12 !== 0) continue;
      out.push([f, noteName(p)]);
      lastY = y;
    }
    return out;
  }

  /** The wheel: over the scale it zooms the frequencies around the pointer; over the spectrum, time. */
  function onWheel(e: WheelEvent) {
    const box = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const x = e.clientX - box.left;
    if (x <= width) {
      onTimeWheel(e);
      return;
    }
    e.preventDefault();
    const hz = viewFreq(1 - (e.clientY - box.top) / height, sv, nyquist);
    setSv(zoomFreq(sv, hz, e.deltaY > 0 ? 1.25 : 0.8, nyquist));
  }

  let menu = $state<{ x: number; y: number; items: ContextItem[] } | null>(null);
  const zoomed = $derived(sv.hi !== null || sv.lo > (sv.log >= 0.5 ? 30 : 0) + 1);
  /** Right-click: frequency zoom, labels, scale, resolution and colors. */
  function onMenu(e: MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
    const box = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const hz = viewFreq(1 - (e.clientY - box.top) / height, sv, nyquist);
    const fi = FFT_SIZES.indexOf(sv.fft as (typeof FFT_SIZES)[number]);
    const logSteps = [0, 0.25, 0.5, 0.75, 1];
    const li = logSteps.findIndex((k) => Math.abs(k - sv.log) < 0.01);
    const at = (k: number) => {
      // A new scale keeps the range in view, fit to that scale's floor.
      const next = { ...sv, log: k };
      setSv(k >= 0.5 && sv.lo < 30 && sv.hi === null ? { ...next, lo: 30 } : k < 0.5 && sv.lo === 30 && sv.hi === null ? { ...next, lo: 0 } : next);
    };
    menu = {
      x: e.clientX,
      y: e.clientY,
      items: [
        { kind: "item", label: "Zoom In (Frequency)", run: () => setSv(zoomFreq(sv, hz, 0.5, nyquist)) },
        { kind: "item", label: "Zoom Out (Frequency)", disabled: !zoomed, reason: "Showing every frequency already.", run: () => setSv(zoomFreq(sv, hz, 2, nyquist)) },
        { kind: "item", label: "Zoom Reset (Frequency)", disabled: !zoomed, reason: "Showing every frequency already.", run: () => setSv({ lo: sv.log >= 0.5 ? 30 : 0, hi: null }) },
        { kind: "sep" },
        { kind: "item", label: "Hertz", checked: !sv.notes, run: () => setSv({ notes: false }) },
        { kind: "item", label: "Notes", checked: sv.notes, run: () => setSv({ notes: true }) },
        { kind: "sep" },
        { kind: "item", label: "More Logarithmic", disabled: li === logSteps.length - 1, reason: "Fully logarithmic already.", run: () => at(logSteps[Math.min(logSteps.length - 1, (li < 0 ? 2 : li) + 1)]!) },
        { kind: "item", label: "More Linear", disabled: li === 0, reason: "Fully linear already.", run: () => at(logSteps[Math.max(0, (li < 0 ? 2 : li) - 1)]!) },
        { kind: "item", label: "Full Logarithmic", checked: sv.log === 1, run: () => at(1) },
        { kind: "item", label: "Full Linear", checked: sv.log === 0, run: () => at(0) },
        { kind: "sep" },
        { kind: "item", label: `Increase Spectral Resolution (${FFT_SIZES[Math.min(FFT_SIZES.length - 1, fi + 1)]})`, disabled: fi === FFT_SIZES.length - 1, reason: "At the finest resolution.", run: () => setSv({ fft: FFT_SIZES[Math.min(FFT_SIZES.length - 1, fi + 1)]! }) },
        { kind: "item", label: `Decrease Spectral Resolution (${FFT_SIZES[Math.max(0, fi - 1)]})`, disabled: fi === 0, reason: "At the coarsest resolution.", run: () => setSv({ fft: FFT_SIZES[Math.max(0, fi - 1)]! }) },
        { kind: "sep" },
        { kind: "item", label: "Spectrogram Colors...", run: () => onColors(e.clientX, e.clientY) },
      ],
    };
  }

  function label(hz: number): string {
    return hz >= 1000 ? `${+(hz / 1000).toFixed(2)}k` : `${+hz.toFixed(hz < 100 ? 1 : 0)}`;
  }

  function drawShape(g: CanvasRenderingContext2D, s: SpectralShape, live: boolean) {
    g.save();
    g.fillStyle = live ? "rgba(255, 255, 255, 0.16)" : "rgba(255, 255, 255, 0.12)";
    g.strokeStyle = "#ffffff";
    g.lineWidth = 1;
    g.setLineDash([4, 3]);
    if (s.kind === "box") {
      const a = xOf(s.start, view);
      const b = xOf(s.end, view);
      const top = yOfFrac(s.top);
      const bottom = yOfFrac(s.bottom);
      g.fillRect(a, top, b - a, bottom - top);
      g.strokeRect(Math.round(a) + 0.5, Math.round(top) + 0.5, Math.round(b - a), Math.round(bottom - top));
    } else if (s.kind === "lasso") {
      if (s.points.length > 1) {
        g.beginPath();
        s.points.forEach(([t, f], i) => (i === 0 ? g.moveTo(xOf(t, view), yOfFrac(f)) : g.lineTo(xOf(t, view), yOfFrac(f))));
        if (!live) g.closePath();
        if (!live) g.fill();
        g.stroke();
      }
    } else {
      // Brush strokes: a thick round line, scaled so it matches the brush shape.
      const rx = Math.max(0.5, s.rx / view.spp);
      const ry = Math.max(0.5, s.ry * height);
      g.setLineDash([]);
      g.strokeStyle = "rgba(255, 255, 255, 0.28)";
      g.lineCap = "round";
      g.lineJoin = "round";
      g.lineWidth = rx * 2;
      g.scale(1, ry / rx);
      g.beginPath();
      s.points.forEach(([t, f], i) => {
        const x = xOf(t, view);
        const y = (yOfFrac(f) * rx) / ry;
        if (i === 0) g.moveTo(x, y);
        else g.lineTo(x, y);
      });
      if (s.points.length === 1) g.lineTo(xOf(s.points[0]![0], view) + 0.01, (yOfFrac(s.points[0]![1]) * rx) / ry);
      g.stroke();
    }
    g.restore();
  }

  $effect(() => {
    const c = canvas;
    if (!c) return;
    const w = width + scaleWidth;
    const h = height;
    const dpr = window.devicePixelRatio || 1;
    c.width = Math.max(1, Math.round(w * dpr));
    c.height = Math.max(1, Math.round(h * dpr));
    c.style.width = `${w}px`;
    c.style.height = `${h}px`;
    const g = c.getContext("2d");
    if (!g) return;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    g.fillStyle = "#08080d";
    g.fillRect(0, 0, w, h);

    // The image, placed by sample so it lines up while a new one loads.
    if (image) {
      const { data, columns, rows } = image;
      const off = new ImageData(columns, rows);
      for (let i = 0; i < data.length; i++) {
        const k = data[i]! * 3;
        off.data[i * 4] = lut[k]!;
        off.data[i * 4 + 1] = lut[k + 1]!;
        off.data[i * 4 + 2] = lut[k + 2]!;
        off.data[i * 4 + 3] = 255;
      }
      const tmp = document.createElement("canvas");
      tmp.width = columns;
      tmp.height = rows;
      tmp.getContext("2d")?.putImageData(off, 0, 0);
      const x0 = xOf(image.start, view);
      const x1 = xOf(image.end, view);
      g.save();
      g.beginPath();
      g.rect(0, 0, width, h);
      g.clip();
      // With only the pitch line on, the spectrum stays as a dim backdrop.
      g.globalAlpha = showSpectrum ? 1 : 0.35;
      g.imageSmoothingEnabled = true;
      g.drawImage(tmp, x0, 0, x1 - x0, h);
      g.restore();
    }

    // Pitch line.
    if (showPitch && pitch && pitch.hz.length) {
      const n = pitch.hz.length;
      const step = (pitch.end - pitch.start) / n;
      g.save();
      g.strokeStyle = "#a6ff4d";
      g.lineWidth = 2;
      g.lineJoin = "round";
      g.beginPath();
      let last = 0;
      for (let i = 0; i < n; i++) {
        const hz = pitch.hz[i]!;
        if (hz <= 0) {
          last = 0;
          continue;
        }
        const x = xOf(pitch.start + (i + 0.5) * step, view);
        const y = yOf(hz);
        // Break the line at big jumps: those are octave slips, not glides.
        if (last > 0 && Math.abs(Math.log2(hz / last)) < 0.35) g.lineTo(x, y);
        else g.moveTo(x, y);
        last = hz;
      }
      g.stroke();
      g.restore();
    }

    // Time selection, faint, so it matches the waveform above.
    if (selection) {
      const a = Math.max(0, xOf(selection.start, view));
      const b = Math.min(width, xOf(selection.end, view));
      if (b > a) {
        g.fillStyle = "rgba(255, 255, 255, 0.08)";
        g.fillRect(a, 0, b - a, h);
      }
    }

    for (const s of shapes ?? []) drawShape(g, s, false);
    if (drawing) drawShape(g, drawing, true);

    // Brush outline under the pointer.
    if (hover && (tool === "brush" || tool === "spot")) {
      g.save();
      g.strokeStyle = tool === "spot" ? "#ffd27a" : "#ffffff";
      g.lineWidth = 1;
      g.beginPath();
      g.arc(hover.x, hover.y, brushSize / 2, 0, Math.PI * 2);
      g.stroke();
      g.restore();
    }

    // Playhead.
    const px = xOf(playhead, view);
    if (px >= 0 && px <= width) {
      g.fillStyle = css("--playhead");
      g.fillRect(Math.round(px), 0, 1, h);
    }

    // Frequency scale.
    g.fillStyle = css("--surface-1");
    g.fillRect(width, 0, scaleWidth, h);
    g.fillStyle = css("--text-faint");
    g.font = `10px ${css("--font-ui")}`;
    g.textBaseline = "middle";
    for (const [hz, text] of scaleMarks(h)) {
      const y = yOf(hz);
      if (y < 8 || y > h - 6) continue;
      g.fillRect(width, Math.round(y), 4, 1);
      g.fillText(text, width + 7, y);
    }
    g.fillText(sv.notes ? "Note" : "Hz", width + 7, 7);
  });

  const cursor = $derived(tool === "time" ? "text" : tool === "brush" || tool === "spot" ? "none" : "crosshair");

  function onPointer(e: PointerEvent) {
    if (e.button !== 0) return;
    const el = e.currentTarget as HTMLElement;
    const box = el.getBoundingClientRect();
    const x0 = e.clientX - box.left;
    if (x0 > width) return;
    e.preventDefault();
    const v = view;
    const sAt = (cx: number) => Math.max(0, Math.min(total, Math.round(sampleAt(cx - box.left, v))));
    const fAt = (cy: number) => Math.max(0, Math.min(1, fracAtY(cy - box.top)));
    const s0 = sAt(e.clientX);
    const f0 = fAt(e.clientY);
    const add = e.shiftKey;
    let dragged = false;

    if (tool === "brush" || tool === "spot") {
      const r = brushSize / 2;
      drawing = { kind: "brush", points: [[s0, f0]], rx: r * v.spp, ry: r / height };
      dragged = true;
    } else if (tool === "lasso") {
      drawing = { kind: "lasso", points: [[s0, f0]] };
    }

    const move = (ev: PointerEvent) => {
      if (!dragged && Math.abs(ev.clientX - e.clientX) < 3 && Math.abs(ev.clientY - e.clientY) < 3) return;
      dragged = true;
      const s1 = sAt(ev.clientX);
      const f1 = fAt(ev.clientY);
      hover = { x: ev.clientX - box.left, y: ev.clientY - box.top };
      if (tool === "time") {
        onSelectTime({ start: Math.min(s0, s1), end: Math.max(s0, s1) });
      } else if (tool === "box") {
        drawing = { kind: "box", start: Math.min(s0, s1), end: Math.max(s0, s1), bottom: Math.min(f0, f1), top: Math.max(f0, f1) };
      } else if (drawing && drawing.kind !== "box") {
        drawing = { ...drawing, points: [...drawing.points, [s1, f1]] };
      }
    };
    const up = () => {
      window.removeEventListener("pointermove", move, true);
      window.removeEventListener("pointerup", up, true);
      const shape = drawing;
      drawing = null;
      if (!dragged) {
        // A click: clear selections and move the playhead.
        if (!add) spectralSelection.set(null);
        if (tool === "time") onSelectTime(null);
        onSeek(s0);
        return;
      }
      if (tool === "time" || !shape) return;
      if (shape.kind === "lasso" && shape.points.length < 3) return;
      if (tool === "spot") {
        // Repair straight away, then forget the stroke.
        spectralSelection.set([shape]);
        run("fx.spectrum.repair");
        return;
      }
      spectralSelection.set(add && shapes ? [...shapes, shape] : [shape]);
    };
    window.addEventListener("pointermove", move, true);
    window.addEventListener("pointerup", up, true);
  }

  // The bar of spectral edits sits over the selection.
  const bar = $derived.by(() => {
    if (!shapes || shapes.length === 0 || tool === "spot") return null;
    let left = Infinity;
    let top = Infinity;
    for (const s of shapes) {
      if (s.kind === "box") {
        left = Math.min(left, xOf(s.start, view));
        top = Math.min(top, yOfFrac(s.top));
      } else {
        for (const [t, f] of s.points) {
          left = Math.min(left, xOf(t, view));
          top = Math.min(top, yOfFrac(f) - (s.kind === "brush" ? s.ry * height : 0));
        }
      }
    }
    return { x: Math.max(4, Math.min(width - 330, left)), y: Math.max(4, top - 30) };
  });

  // The palette: the built-in one, or the quiet and loud colors picked in Settings or the right-click menu.
  let lut = $state<Uint8Array>(SPECTRUM_LUT);
  $effect(() =>
    settings.subscribe((v) => {
      const c = v.colors;
      lut = c.spectrumLow || c.spectrumHigh ? spectrumLut(c.spectrumLow || SPECTRUM_LOW, c.spectrumHigh || SPECTRUM_HIGH) : SPECTRUM_LUT;
    }),
  );
</script>

<div class="spectrum" bind:clientHeight={height}>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <canvas
    bind:this={canvas}
    style:cursor={cursor}
    onpointerdown={onPointer}
    onpointermove={(e) => {
      const b = (e.currentTarget as HTMLElement).getBoundingClientRect();
      hover = { x: e.clientX - b.left, y: e.clientY - b.top };
    }}
    onpointerleave={() => (hover = null)}
    onwheel={onWheel}
    oncontextmenu={onMenu}
  ></canvas>
  {#if note}
    <p class="note">{note}</p>
  {/if}
  {#if bar}
    <div class="bar" style:left="{bar.x}px" style:top="{bar.y}px" role="toolbar" aria-label="Spectral edit">
      <button type="button" title={tip("Reduce the selected area by 12 dB", "fx.spectrum.reduce-12")} onclick={() => run("fx.spectrum.reduce-12")}>-12 dB</button>
      <button type="button" title={tip("Reduce the selected area by 24 dB", "fx.spectrum.reduce-24")} onclick={() => run("fx.spectrum.reduce-24")}>-24 dB</button>
      <button type="button" title={tip("Remove the selected area", "fx.spectrum.remove")} onclick={() => run("fx.spectrum.remove")}>Remove</button>
      <button type="button" title={tip("Fill the selected area from the audio around it", "fx.spectrum.repair")} onclick={() => run("fx.spectrum.repair")}>Repair</button>
      <button type="button" class="x" title="Clear the spectral selection" aria-label="Clear" onclick={() => spectralSelection.set(null)}>&times;</button>
    </div>
  {/if}
</div>

{#if menu}
  <ContextMenu x={menu.x} y={menu.y} items={menu.items} onClose={() => (menu = null)} />
{/if}

<style>
  .spectrum {
    position: relative;
    min-height: 0;
    overflow: hidden;
    border-top: 3px solid var(--track-gap);
  }

  canvas {
    display: block;
  }

  .note {
    position: absolute;
    top: 50%;
    left: 50%;
    margin: 0;
    color: var(--text-dim);
    transform: translate(-50%, -50%);
    pointer-events: none;
  }

  .bar {
    position: absolute;
    display: flex;
    gap: 4px;
    align-items: center;
    padding: 2px 4px;
    border: 1px solid var(--border-strong);
    border-radius: 3px;
    background: color-mix(in srgb, var(--surface-2) 92%, transparent);
    font-size: var(--font-size-small);
  }

  .bar button {
    height: 20px;
    padding: 0 6px;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    background: var(--surface-3);
    color: var(--text);
    font: inherit;
  }

  .bar button:hover {
    border-color: var(--accent);
  }

  .bar .x {
    padding: 0 5px;
    color: var(--text-dim);
  }
</style>
