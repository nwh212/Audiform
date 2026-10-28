<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  The Waveform editor: one audio file drawn as one continuous waveform
  per channel, with a time selection, a level scale and a quick level
  control. The file is an edit list in the core (see wave-edit.ts); this
  view draws the result and turns gestures into actions.
-->
<script lang="ts">
  import { pickedChannel } from "../../stores/file-channels";
  import { settings, wheelAnchorX, zoomRatios } from "../../stores/settings";
  import ColorPairPopover from "../../app/ColorPairPopover.svelte";
  import { SPECTRUM_HIGH, SPECTRUM_LOW, themeColor } from "../../app/colors";
  import { KIND_COLORS } from "../markers/markers";
  import { drawStartLine, drawStartTip } from "./ruler-draw";
  import type { ActionRegistry } from "../../app/actions";
  import ContextMenu, { type ContextItem } from "../../app/ContextMenu.svelte";
  import { editor, type EditorState } from "../../stores/editor";
  import type { PeakStore } from "../../stores/peaks";
  import { liveTakes, type LiveTakes } from "../../stores/record";
  import { clipLength, sessionEnd, stretchOf, type ClipDoc, type SessionDoc, type SessionStore } from "../../stores/session";
  import type { TransportState, TransportStore } from "../../stores/transport";
  import { onDestroy, untrack } from "svelte";
  import { waveZoom, type WaveFile } from "../../stores/wave";
  import { allOn, toggleChannel, waveChannels } from "./wave-channels";
  import { appearanceTick } from "../../stores/appearance";
  import { channelLabels } from "../properties/channel-mix";
  import { laneWidths } from "../time-info/time-info";
  import { timeDisplayItems } from "./time-menu";
  import { levelAt, pointsOf } from "./envelope";
  import { fit, formatIn, peakRange, pickLevel, rulerTicks, ticksFor, sampleAt, SAMPLE_SPP, xOf, zoomAround, type Viewport } from "./timeline";
  import { mediaFrames, transportAuditionLevel } from "../../app/core";
  import { dragGain, gainText } from "./clip-gain";
  import TransportBar from "./TransportBar.svelte";
  import SpectrumView from "./SpectrumView.svelte";

  interface Props {
    wave: SessionStore;
    file: WaveFile | null;
    transport: TransportStore;
    peaks: PeakStore;
    actions: ActionRegistry;
    shortcutFor?: (id: string) => string | undefined;
    /** Applies a level change in dB to the selection, or the whole file. */
    onLevel: (db: number) => void | Promise<void>;
  }

  const { wave, file, transport, peaks, actions, shortcutFor = () => undefined, onLevel }: Props = $props();

  const RULER_H = 22;
  const SCALE_W = 46;
  $effect(() => {
    const w = width;
    laneWidths.update((l) => (l.waveform === w ? l : { ...l, waveform: w }));
  });
  // Each file opens with every channel on, or only the one picked in the Files panel.
  let channelFile: string | null = null;
  $effect(() => {
    const key = `${file?.path ?? ""}:${channels}`;
    if (key !== channelFile) {
      channelFile = key;
      const k = file ? pickedChannel(file.path, channels) : null;
      waveChannels.set(k === null ? allOn(channels) : allOn(channels).map((_, i) => i === k));
    }
  });

  let doc = $state<SessionDoc | null>(null);
  $effect(() =>
    wave.subscribe((d) => {
      doc = d;
    }),
  );
  let tp = $state<TransportState | null>(null);
  $effect(() =>
    transport.subscribe((t) => {
      tp = t;
    }),
  );
  let ed = $state<EditorState>(editor.current());
  $effect(() =>
    editor.subscribe((s) => {
      ed = s;
    }),
  );
  let peakTick = $state(0);
  $effect(() => peaks.subscribe(() => peakTick++));
  // The theme or colors changed: draw again.
  $effect(() => appearanceTick.subscribe(() => untrack(() => peakTick++)));
  /** The take being recorded into this file, drawn as it arrives. */
  let live = $state<LiveTakes | null>(null);
  $effect(() =>
    liveTakes.subscribe((v) => {
      live = v;
    }),
  );
  let tick = $state(0);
  $effect(() =>
    actions.changed.subscribe((n) => {
      tick = n;
    }),
  );

  const run = (id: string) => void actions.run(id);
  const can = (id: string) => {
    void tick;
    return actions.isEnabled(id);
  };
  const tip = (label: string, id: string) => {
    const k = shortcutFor(id);
    return k ? `${label} (${k})` : label;
  };

  let width = $state(800);
  let height = $state(300);
  let canvas = $state<HTMLCanvasElement | null>(null);
  let ruler = $state<HTMLCanvasElement | null>(null);
  let area = $state<HTMLElement | null>(null);
  let menu = $state<{ x: number; y: number; items: ContextItem[] } | null>(null);
  /** Amplitude zoom: 1 shows full scale; higher makes quiet audio bigger. */
  let ampZoom = $state(1);

  const rate = $derived(doc?.sample_rate ?? 48_000);
  const total = $derived(doc ? sessionEnd(doc) : 0);
  const view = $derived<Viewport>({ spp: doc?.view.samples_per_pixel ?? 1024, scroll: doc?.view.scroll ?? 0 });
  const playhead = $derived(tp?.playing ? tp.position : (doc?.view.playhead ?? 0));
  /** The start marker: shown while playing or paused, where playback began. */
  const startMark = $derived(tp && (tp.playing || tp.paused) ? tp.anchor : null);
  const selection = $derived(doc?.view.selection ?? null);
  const channels = $derived(Math.max(1, Math.min(64, doc?.media[0]?.channels ?? 1)));
  const chNames = $derived(channelLabels(channels));
  const clips = $derived((doc?.tracks[0]?.clips ?? []).slice().sort((a, b) => a.start - b.start));

  function setView(v: Partial<{ spp: number; scroll: number; playhead: number; selection: { start: number; end: number } | null }>, auto = false) {
    if (!doc) return;
    // Scrolling or zooming by hand while playing stops the following until playback starts again or the playhead is moved.
    if (!auto && tp?.playing) {
      if (v.playhead !== undefined) followHeld = false;
      else if ((v.scroll !== undefined && v.scroll !== doc.view.scroll) || (v.spp !== undefined && v.spp !== doc.view.samples_per_pixel)) followHeld = true;
    }
    // The view never runs past the end of the file.
    const maxScroll = Math.max(0, total - width * (v.spp ?? view.spp));
    wave.setView({
      ...doc.view,
      samples_per_pixel: v.spp ?? doc.view.samples_per_pixel,
      scroll: Math.max(0, Math.min(maxScroll, Math.round(v.scroll ?? doc.view.scroll))),
      playhead: Math.max(0, Math.round(v.playhead ?? doc.view.playhead)),
      selection: v.selection === undefined ? doc.view.selection : v.selection,
    });
  }

  // A newly opened file fits the window. The panel can still be growing
  // when the file opens (switching from the Multitrack view), so it fits
  // again while the width changes, until the user zooms or scrolls.
  let fittedFor = "";
  let fittedSpp = 0;
  let fittedWidth = 0;
  $effect(() => {
    if (!doc || !file || width <= 0) return;
    const key = file.path + doc.id;
    const untouched = fittedFor === key && doc.view.samples_per_pixel === fittedSpp && doc.view.scroll === 0;
    if (fittedFor === key && !(untouched && width !== fittedWidth)) return;
    fittedFor = key;
    const f = fit(total, width, 1);
    fittedSpp = f.spp;
    fittedWidth = width;
    setView({ spp: f.spp, scroll: 0 });
  });

  // Keep the playhead in view while playing, unless the view was scrolled
  // or zoomed by hand: then leave it be until playback starts again or the
  // playhead is moved.
  let followHeld = false;
  let wasPlaying = false;
  $effect(() => {
    const playingNow = !!tp?.playing;
    if (playingNow && !wasPlaying) followHeld = false;
    wasPlaying = playingNow;
    if (!playingNow || !tp || width <= 0) return;
    const x = xOf(tp.position, view);
    if (followHeld) return;
    if (x > width - 20 || x < 0) setView({ scroll: tp.position - 20 * view.spp }, true);
  });

  /** Farthest zoom out: the whole file fills the view, with no empty time after it. */
  const outermost = () => fit(total, width, 1).spp;

  /** The view's zoom commands, used by the Zoom actions while this editor shows. */
  export function zoom(kind: "in" | "out" | "full" | "selection" | "amp-in" | "amp-out"): void {
    if (!doc) return;
    if (kind === "amp-in") ampZoom = Math.min(16, ampZoom * 2);
    else if (kind === "amp-out") ampZoom = Math.max(1, ampZoom / 2);
    else if (kind === "full") {
      const f = fit(total, width, 1);
      setView({ spp: f.spp, scroll: 0 });
    } else if (kind === "selection" && selection) {
      const f = fit(selection.end - selection.start, width);
      setView({ spp: f.spp, scroll: Math.max(0, selection.start - (selection.end - selection.start) * 0.05) });
    } else if (kind === "in" || kind === "out") {
      const anchor = Math.max(0, Math.min(width, xOf(playhead, view)));
      const z = zoomAround(view, kind === "in" ? 0.5 : 2, anchor, outermost(), SAMPLE_SPP);
      setView({ spp: z.spp, scroll: z.scroll });
    }
  }

  waveZoom.run = (k) => zoom(k);
  onDestroy(() => {
    if (waveZoom.run) waveZoom.run = null;
  });

  /** Level of a piece at `rel` samples from its start, as a linear factor. */
  function pieceGain(c: ClipDoc, rel: number, len: number): number {
    if (c.muted) return 0;
    let g = 10 ** (c.gain_db / 20);
    const vol = pointsOf(c, "volume");
    if (vol.length) {
      const db = levelAt(vol, rel);
      g *= db <= -59.9 ? 0 : 10 ** (db / 20);
    }
    if (rel < c.fade_in.length) g *= rel / Math.max(1, c.fade_in.length);
    if (len - rel < c.fade_out.length) g *= (len - rel) / Math.max(1, c.fade_out.length);
    return g;
  }

  /**
   * Zoomed in past this many source frames per pixel, the waveform is
   * drawn from the file's exact samples (a line through them, with a dot
   * on each once they are far enough apart) instead of the peak cache.
   */
  const SAMPLE_VIEW = 8;
  /** Exact samples read for drawing, per file: a stretch around what is shown. */
  let exact = $state<Record<string, { start: number; count: number; channels: number; data: Float32Array }>>({});
  const asking = new Set<string>();
  const failed = new Set<string>();
  function needFrames(path: string, from: number, to: number) {
    const have = exact[path];
    if (have && from >= have.start && to <= have.start + have.count) return;
    const span = to - from;
    const start = Math.max(0, Math.floor(from - span));
    const count = Math.ceil(span * 3) + 2;
    const key = `${path}:${start}:${count}`;
    if (asking.has(key)) return;
    asking.add(key);
    void mediaFrames(path, start, count)
      .then((r) => (exact = { ...exact, [path]: { start, count: r.data.length / r.channels, channels: r.channels, data: r.data } }))
      .catch((e: unknown) => {
        // The peak drawing stays up; say why once per file.
        if (!failed.has(path)) console.warn(`Exact samples could not be read from ${path}:`, e);
        failed.add(path);
      })
      .finally(() => asking.delete(key));
  }

  function css(name: string): string {
    return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  }

  /** dB marks on the level scale. */
  const MARKS = [0, -3, -6, -9, -12, -18, -24];

  // Waveform. The picture without the start marker and playhead is drawn
  // off screen; the playhead moves about thirty times a second while playing,
  // and moving it copies that picture and draws two lines instead of
  // working out every column of the waveform again.
  let waveBase: HTMLCanvasElement | null = null;
  let headColor = "#53c28b";
  function paintWaveHead() {
    const c = canvas;
    if (!c || !waveBase || c.width !== waveBase.width || c.height !== waveBase.height) return;
    const g = c.getContext("2d");
    if (!g) return;
    const dpr = window.devicePixelRatio || 1;
    g.setTransform(1, 0, 0, 1, 0, 0);
    g.drawImage(waveBase, 0, 0);
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    const w = width;
    if (startMark !== null) {
      const sx = xOf(startMark, view);
      if (sx >= 0 && sx <= w) drawStartLine(g, sx, height, headColor);
    }
    const px = xOf(playhead, view);
    if (px >= 0 && px <= w) {
      g.fillStyle = headColor;
      g.fillRect(Math.round(px), 0, 1, height);
    }
  }
  $effect(() => {
    void playhead;
    void startMark;
    untrack(paintWaveHead);
  });
  $effect(() => {
    void peakTick;
    void live?.tick;
    const c = canvas;
    if (!c || !doc) return;
    const dpr = window.devicePixelRatio || 1;
    const w = width;
    const h = height;
    c.width = Math.round((w + SCALE_W) * dpr);
    c.height = Math.round(h * dpr);
    c.style.width = `${w + SCALE_W}px`;
    c.style.height = `${h}px`;
    waveBase ??= document.createElement("canvas");
    waveBase.width = c.width;
    waveBase.height = c.height;
    const g = waveBase.getContext("2d");
    if (!g) return;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    g.fillStyle = css("--surface-0");
    g.fillRect(0, 0, w + SCALE_W, h);
    const laneH = h / channels;
    const media = new Map(doc.media.map((m) => [m.id, m]));
    const waveColor = css("--wave") || css("--clip-1") || "#53c28b";

    for (let ch = 0; ch < channels; ch++) {
      const top = ch * laneH;
      const mid = top + laneH / 2;
      const half = laneH / 2 - 2;
      // Level grid.
      g.strokeStyle = "rgba(255, 255, 255, 0.06)";
      g.lineWidth = 1;
      for (const db of MARKS) {
        const a = 10 ** (db / 20) * ampZoom;
        if (a > 1.0001) continue;
        for (const y of [mid - a * half, mid + a * half]) {
          g.beginPath();
          g.moveTo(0, Math.round(y) + 0.5);
          g.lineTo(w, Math.round(y) + 0.5);
          g.stroke();
        }
      }
      g.strokeStyle = "rgba(255, 255, 255, 0.18)";
      g.beginPath();
      g.moveTo(0, Math.round(mid) + 0.5);
      g.lineTo(w, Math.round(mid) + 0.5);
      g.stroke();
      if (ch > 0) {
        g.fillStyle = css("--border-strong");
        g.fillRect(0, Math.round(top), w + SCALE_W, 1);
      }
    }

    // Time grid at the ruler's labeled ticks.
    g.strokeStyle = "rgba(255, 255, 255, 0.05)";
    g.lineWidth = 1;
    for (const t of ticksFor(view, w, rate, ed.timeFormat)) {
      if (!t.label) continue;
      const x = Math.round(t.x) + 0.5;
      g.beginPath();
      g.moveTo(x, 0);
      g.lineTo(x, h);
      g.stroke();
    }

    // Each piece of the edit list, drawn where it now sits.
    g.fillStyle = waveColor;
    for (const clip of clips) {
      const m = media.get(clip.media_id);
      if (!m) continue;
      const len = clipLength(clip, m, rate);
      const x0 = Math.max(0, Math.floor(xOf(clip.start, view)));
      const x1 = Math.min(w, Math.ceil(xOf(clip.start + len, view)));
      if (x1 <= 0 || x0 >= w) continue;
      const entry = peaks.get(m.path);
      if (entry.status !== "ready" || !entry.summary) continue;
      const srcRate = m.sample_rate || entry.summary.sampleRate;
      const framesPerPx = (view.spp * srcRate) / rate;
      const plain = !clip.looped && (clip.stretch == null || stretchOf(clip) === 1);
      if (framesPerPx <= SAMPLE_VIEW && plain) {
        // Exact samples: map every frame in view to its spot, through the piece's level.
        const relA = Math.max(0, view.scroll + x0 * view.spp - clip.start);
        const relB = Math.min(len, view.scroll + x1 * view.spp - clip.start);
        const fA = Math.max(clip.source_in, Math.floor(clip.source_in + (relA * srcRate) / rate) - 1);
        const fB = Math.min(clip.source_out, Math.ceil(clip.source_in + (relB * srcRate) / rate) + 1);
        needFrames(m.path, fA, fB);
        const have = exact[m.path];
        const ready = have && fA >= have.start && fB <= have.start + Math.max(have.count, 0) + 1;
        if (ready) {
        const pxPerFrame = 1 / framesPerPx;
        g.strokeStyle = waveColor;
        g.lineWidth = 1.25;
        for (let ch = 0; ch < channels; ch++) {
          const top = ch * laneH;
          const mid = top + laneH / 2;
          const half = laneH / 2 - 2;
          const hc = Math.min(ch, have.channels - 1);
          const at = (f: number) => {
            const rel = ((f - clip.source_in) * rate) / srcRate;
            const v = (have.data[(f - have.start) * have.channels + hc] ?? 0) * pieceGain(clip, rel, len) * ampZoom * hud(clip.start + rel);
            return [xOf(clip.start + rel, view), mid - Math.max(-1, Math.min(1, v)) * half] as const;
          };
          g.beginPath();
          for (let f = fA; f < fB; f++) {
            const [x, y] = at(f);
            if (f === fA) g.moveTo(x, y);
            else g.lineTo(x, y);
          }
          g.stroke();
          if (pxPerFrame >= 6) {
            for (let f = fA; f < fB; f++) {
              const [x, y] = at(f);
              g.beginPath();
              g.arc(x, y, 2.25, 0, Math.PI * 2);
              g.fill();
            }
          }
        }
        continue;
        }
        // Not read yet (or could not be): the finest peaks below, until it is.
      }
      const level = pickLevel(entry.levels, framesPerPx);
      if (!level) continue;
      const pch = entry.summary.channels;
      const region = clip.source_out - clip.source_in;
      for (let px = x0; px < x1; px++) {
        const rel = view.scroll + px * view.spp - clip.start;
        if (rel < 0 || rel >= len) continue;
        let into = (rel * srcRate) / rate;
        if (clip.looped && region > 0) into = (((into + clip.looped.phase) % region) + region) % region;
        const s0 = clip.source_in + into;
        const s1 = Math.min(clip.source_out, s0 + framesPerPx);
        const k = pieceGain(clip, rel, len) * ampZoom * hud(clip.start + rel);
        for (let ch = 0; ch < channels; ch++) {
          const top = ch * laneH;
          const mid = top + laneH / 2;
          const half = laneH / 2 - 2;
          const [lo, hi] = peakRange(level, pch, Math.min(ch, pch - 1), s0, Math.max(s0 + 1, s1));
          const y0 = mid - Math.min(1, hi * k) * half;
          const y1 = mid - Math.max(-1, lo * k) * half;
          g.fillRect(px, y0, 1, Math.max(1, y1 - y0));
        }
      }
    }

    // The take being recorded: its live peak data, drawn by the same code as the clips above.
    const lw = live && live.rate > 0 ? live.waves["wave"] : undefined;
    if (live && lw && lw.blocks > 0) {
      const srcRate = live.rate;
      const len = Math.round((lw.frames * rate) / srcRate);
      const x0 = Math.max(0, Math.floor(xOf(live.at, view)));
      const x1 = Math.min(w, Math.ceil(xOf(live.at + len, view)));
      if (x1 > x0) {
        // The new take lies over what is there until Stop; hide it so the take reads alone.
        g.fillStyle = css("--surface-0") || "#111";
        g.fillRect(x0, 0, x1 - x0, h);
        const framesPerPx = (view.spp * srcRate) / rate;
        const level = lw.levelFor(framesPerPx);
        g.fillStyle = waveColor;
        for (let px = x0; px < x1; px++) {
          const rel = view.scroll + px * view.spp - live.at;
          if (rel < 0 || rel >= len) continue;
          const s0 = (rel * srcRate) / rate;
          const s1 = Math.min(lw.frames, s0 + framesPerPx);
          for (let ch = 0; ch < channels; ch++) {
            const top = ch * laneH;
            const mid = top + laneH / 2;
            const half = laneH / 2 - 2;
            const [lo, hi] = peakRange(level, lw.channels, Math.min(ch, lw.channels - 1), s0, Math.max(s0 + 1, s1));
            const y0 = mid - Math.min(1, hi * ampZoom) * half;
            const y1 = mid - Math.max(-1, lo * ampZoom) * half;
            g.fillRect(px, y0, 1, Math.max(1, y1 - y0));
          }
        }
      }
    }

    // Past the end of the file.
    const endX = xOf(total, view);
    if (endX < w) {
      g.fillStyle = "rgba(0, 0, 0, 0.35)";
      g.fillRect(Math.max(0, endX), 0, w - Math.max(0, endX), h);
    }

    // Selection.
    if (selection) {
      const a = Math.max(0, xOf(selection.start, view));
      const b = Math.min(w, xOf(selection.end, view));
      if (b > a) {
        g.fillStyle = "rgba(255, 255, 255, 0.14)";
        g.fillRect(a, 0, b - a, h);
      }
    }

    // Playhead.
    // Markers: a flag on the ruler, a band for ranges, the name beside it.
    for (const m of doc.markers) {
      const mx = Math.round(xOf(m.position, view));
      const color = m.color || css(KIND_COLORS[m.kind]) || "#c8b040";
      if (m.duration > 0) {
        const ex = xOf(m.position + m.duration, view);
        if (ex >= 0 && mx <= w) {
          g.fillStyle = color;
          g.globalAlpha = 0.35;
          g.fillRect(Math.max(0, mx), RULER_H - 6, Math.min(w, ex) - Math.max(0, mx), 6);
          g.globalAlpha = 1;
        }
      }
      if (mx < -60 || mx > w) continue;
      g.fillStyle = color;
      g.fillRect(mx, 0, 2, RULER_H);
      g.fillText(m.name, mx + 4, 11);
    }
    headColor = css("--playhead");

    // Level scale on the right.
    g.fillStyle = css("--surface-2");
    g.fillRect(w, 0, SCALE_W, h);
    g.fillStyle = css("--border");
    g.fillRect(w, 0, 1, h);
    g.font = `10px ${css("--font-mono")}`;
    g.textBaseline = "middle";
    g.fillStyle = css("--text-dim");
    for (let ch = 0; ch < channels; ch++) {
      const top = ch * laneH;
      const mid = top + laneH / 2;
      const half = laneH / 2 - 2;
      let lastY = -100;
      for (const db of MARKS) {
        const a = 10 ** (db / 20) * ampZoom;
        if (a > 1.0001) continue;
        const y = mid - a * half;
        if (Math.abs(y - lastY) < 11 || y < top + 5 || mid - y < 11) continue;
        lastY = y;
        g.fillText(db === 0 ? " 0" : String(db), w + 8, y);
        g.fillText(db === 0 ? " 0" : String(db), w + 8, mid + a * half);
      }
      g.fillText("-∞", w + 8, mid);
    }
    untrack(paintWaveHead);
  });

  // Ruler.
  $effect(() => {
    void peakTick;
    const c = ruler;
    if (!c || !doc) return;
    const dpr = window.devicePixelRatio || 1;
    const w = width;
    c.width = Math.round((w + SCALE_W) * dpr);
    c.height = Math.round(RULER_H * dpr);
    c.style.width = `${w + SCALE_W}px`;
    c.style.height = `${RULER_H}px`;
    const g = c.getContext("2d");
    if (!g) return;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    g.fillStyle = css("--surface-2");
    g.fillRect(0, 0, w + SCALE_W, RULER_H);
    g.fillStyle = css("--text-dim");
    g.strokeStyle = css("--border-strong");
    g.font = `10px ${css("--font-ui")}`;
    g.textBaseline = "top";
    for (const t of ticksFor(view, w, rate, ed.timeFormat)) {
      const x = Math.round(t.x) + 0.5;
      g.beginPath();
      g.moveTo(x, t.label ? 8 : 15);
      g.lineTo(x, RULER_H);
      g.stroke();
      if (t.label) g.fillText(ed.timeFormat === "time" || ed.timeFormat === "bars" ? t.label : formatIn(sampleAt(t.x, view), rate, ed.timeFormat), x + 3, 2);
    }
    if (selection) {
      const a = Math.max(0, xOf(selection.start, view));
      const b = Math.min(w, xOf(selection.end, view));
      if (b > a) {
        g.fillStyle = "rgba(255, 255, 255, 0.16)";
        g.fillRect(a, 0, b - a, RULER_H);
      }
    }
    if (startMark !== null) {
      const sx = xOf(startMark, view);
      if (sx >= 0 && sx <= w) drawStartTip(g, sx, RULER_H, css("--playhead"));
    }
    const px = xOf(playhead, view);
    if (px >= 0 && px <= w) {
      g.fillStyle = css("--playhead");
      const cx = Math.round(px) + 0.5;
      g.beginPath();
      g.moveTo(cx - 5, RULER_H - 7);
      g.lineTo(cx + 5, RULER_H - 7);
      g.lineTo(cx, RULER_H);
      g.fill();
    }
    g.fillStyle = css("--text-faint");
    g.fillText("dB", w + 8, 6);
  });

  // Gestures.
  function xIn(e: { clientX: number }, el: HTMLElement): number {
    return e.clientX - el.getBoundingClientRect().left;
  }

  function onPointer(e: PointerEvent) {
    if (!doc || e.button !== 0) return;
    const el = e.currentTarget as HTMLElement;
    if (xIn(e, el) > width) return;
    e.preventDefault();
    const clamp = (s: number) => Math.max(0, Math.min(total, s));
    const v = view;
    const at = (cx: number) => clamp(Math.round(sampleAt(cx - el.getBoundingClientRect().left, v)));
    const anchor = e.shiftKey && selection ? (Math.abs(at(e.clientX) - selection.start) < Math.abs(at(e.clientX) - selection.end) ? selection.end : selection.start) : at(e.clientX);
    let dragged = e.shiftKey && !!selection;
    const move = (ev: PointerEvent) => {
      const s = at(ev.clientX);
      if (!dragged && Math.abs(s - anchor) / v.spp < 3) return;
      dragged = true;
      setView({ selection: { start: Math.min(anchor, s), end: Math.max(anchor, s) } });
    };
    const up = (ev: PointerEvent) => {
      window.removeEventListener("pointermove", move, true);
      window.removeEventListener("pointerup", up, true);
      if (dragged) {
        move(ev);
        const sel = doc?.view.selection;
        if (sel && !tp?.playing) setView({ playhead: sel.start });
        return;
      }
      // A click moves the playhead and clears the selection.
      setView({ selection: null });
      void transport.seek(anchor);
    };
    window.addEventListener("pointermove", move, true);
    window.addEventListener("pointerup", up, true);
  }

  function onRulerPointer(e: PointerEvent) {
    if (!doc || e.button !== 0) return;
    const x = xIn(e, e.currentTarget as HTMLElement);
    if (x > width) return;
    // Click or drag: the playhead follows the mouse. While playing,
    // playback moves there when the button is let go.
    e.preventDefault();
    const el = e.currentTarget as HTMLElement;
    const at = (ev: PointerEvent) => Math.max(0, Math.min(total, Math.round(sampleAt(Math.max(0, Math.min(width, xIn(ev, el))), view))));
    const wasPlaying = !!tp?.playing;
    let last = at(e);
    if (!wasPlaying) void transport.seek(last);
    const move = (ev: PointerEvent) => {
      last = at(ev);
      if (!wasPlaying) void transport.seek(last);
    };
    const up = () => {
      window.removeEventListener("pointermove", move, true);
      window.removeEventListener("pointerup", up, true);
      void transport.seek(last);
    };
    window.addEventListener("pointermove", move, true);
    window.addEventListener("pointerup", up, true);
  }

  function onWheel(e: WheelEvent) {
    if (!area) return;
    const x = Math.max(0, Math.min(width, xIn(e, area)));
    // Over the time ruler the wheel always zooms, toward the pointer.
    const overRuler = e.currentTarget === ruler;
    if (e.altKey) {
      e.preventDefault();
      zoom(e.deltaY < 0 ? "amp-in" : "amp-out");
    } else if (overRuler || e.ctrlKey || (settings.current.wheelZooms && !e.shiftKey && Math.abs(e.deltaY) >= Math.abs(e.deltaX))) {
      e.preventDefault();
      const r = zoomRatios();
      const at = wheelAnchorX(x, width, xOf(playhead, view), overRuler);
      const z = zoomAround(view, e.deltaY > 0 ? r.zoomOut : r.zoomIn, at, outermost(), SAMPLE_SPP);
      setView({ spp: z.spp, scroll: z.scroll });
    } else {
      e.preventDefault();
      const d = Math.abs(e.deltaX) > Math.abs(e.deltaY) ? e.deltaX : e.deltaY;
      setView({ scroll: view.scroll + d * view.spp });
    }
  }

  function onMenu(e: MouseEvent) {
    e.preventDefault();
    const act = (label: string, id: string): ContextItem => ({ kind: "item", label, shortcut: shortcutFor(id), disabled: !can(id), run: () => run(id) });
    menu = {
      x: e.clientX,
      y: e.clientY,
      items: [
        act("Cut", "edit.cut"),
        act("Copy", "edit.copy"),
        act("Paste", "edit.paste"),
        act("Delete", "edit.delete"),
        act("Crop", "edit.crop"),
        { kind: "sep" },
        act("Silence", "fx.silence"),
        act("Fade In", "fx.fade-in"),
        act("Fade Out", "fx.fade-out"),
        act("Amplify...", "fx.amplify"),
        act("Normalize...", "fx.normalize"),
        { kind: "sep" },
        act("Select All", "edit.select-all"),
        act("Deselect All", "edit.deselect"),
        ...(ed.showSpectrum
          ? ([{ kind: "sep" }, { kind: "item", label: "Spectrogram Colors...", run: () => (colorPop = { x: e.clientX, y: e.clientY }) }] as ContextItem[])
          : []),
      ],
    };
  }

  /** The spectrogram's color picker, opened from the right-click menu. */
  let colorPop = $state<{ x: number; y: number } | null>(null);
  let appColors = $state(settings.current.colors);
  $effect(() => settings.subscribe((v) => (appColors = v.colors)));
  function setSpectrumColors(low: string, high: string) {
    const c = settings.current;
    settings.set({ ...c, colors: { ...c.colors, spectrumLow: low, spectrumHigh: high } });
  }

  // Horizontal scroll bar.
  const thumbW = $derived(total > 0 ? Math.max(24, Math.min(width, (width * width * view.spp) / Math.max(total, width * view.spp))) : width);
  const thumbX = $derived(total > 0 ? Math.min(width - thumbW, (view.scroll / Math.max(1, total)) * width) : 0);
  function onThumbPointer(e: PointerEvent) {
    e.preventDefault();
    const x0 = e.clientX;
    const s0 = view.scroll;
    const per = total / Math.max(1, width);
    const move = (ev: PointerEvent) => setView({ scroll: s0 + (ev.clientX - x0) * per });
    const up = () => {
      window.removeEventListener("pointermove", move, true);
      window.removeEventListener("pointerup", up, true);
    };
    window.addEventListener("pointermove", move, true);
    window.addEventListener("pointerup", up, true);
  }

  // Quick level: drag left or right, release to apply. While dragging, the
  // waveform shows the new level and playback plays it at once.
  let level = $state(0);
  let levelDrag = $state(false);
  let levelTip = $state<{ x: number; y: number; text: string } | null>(null);
  /** The quick level's gain at file sample `t` while it is being dragged (1 elsewhere). */
  function hud(t: number): number {
    if (!levelDrag || level === 0) return 1;
    if (selection && (t < selection.start || t >= selection.end)) return 1;
    return Math.pow(10, level / 20);
  }
  let auditionFrame = 0;
  function audition(db: number) {
    cancelAnimationFrame(auditionFrame);
    auditionFrame = requestAnimationFrame(() => {
      const sel = doc?.view.selection;
      void transportAuditionLevel(db, sel ? sel.start : 0, sel ? sel.end : 0).catch(() => {});
    });
  }
  function onLevelPointer(e: PointerEvent) {
    e.preventDefault();
    const x0 = e.clientX;
    const y0 = e.clientY;
    levelDrag = true;
    // The same feel as a clip's level handle in the Multitrack: right or
    // up turns it up, Shift for fine steps, the level beside the pointer.
    const move = (ev: PointerEvent) => {
      level = dragGain(0, ev.clientX - x0, ev.clientY - y0, ev.shiftKey);
      levelTip = { x: ev.clientX, y: ev.clientY, text: level === 0 ? "0.0 dB" : gainText(level) };
      audition(level);
    };
    const up = () => {
      window.removeEventListener("pointermove", move, true);
      window.removeEventListener("pointerup", up, true);
      levelDrag = false;
      levelTip = null;
      const db = level;
      level = 0;
      cancelAnimationFrame(auditionFrame);
      // The edit takes over from the audition once it is made.
      void Promise.resolve(db !== 0 ? onLevel(db) : undefined).finally(() => void transportAuditionLevel(0, 0, 0).catch(() => {}));
    };
    window.addEventListener("pointermove", move, true);
    window.addEventListener("pointerup", up, true);
  }

  // Share of the height the waveform gets when the spectrum shows.
  let split = $state(0.4);
  let section = $state<HTMLElement | null>(null);
  function onSplitPointer(e: PointerEvent) {
    e.preventDefault();
    const box = area?.getBoundingClientRect();
    const total = section?.getBoundingClientRect();
    if (!box || !total) return;
    const top = box.top;
    const spectrumBottom = total.bottom - 60;
    const move = (ev: PointerEvent) => {
      const h = spectrumBottom - top;
      if (h > 0) split = Math.max(0.12, Math.min(0.88, (ev.clientY - top) / h));
    };
    const up = () => {
      window.removeEventListener("pointermove", move, true);
      window.removeEventListener("pointerup", up, true);
    };
    window.addEventListener("pointermove", move, true);
    window.addEventListener("pointerup", up, true);
  }

  const fileName = $derived(file ? file.path.split(/[\\/]/).pop() : "");
  const info = $derived(
    doc && file
      ? [`${(rate / 1000).toLocaleString("en-US")} kHz`, channels === 1 ? "Mono" : channels === 2 ? "Stereo" : `${channels} channels`, file.bitDepth ? `${file.bitDepth}-bit` : file.format]
          .filter(Boolean)
          .join("  ")
      : "",
  );
</script>

<section
  class="wave"
  style:grid-template-rows={(ed.showSpectrum || ed.showPitch) && doc && file ? `auto auto minmax(0, ${split}fr) 5px minmax(0, ${1 - split}fr) auto auto` : undefined}
  aria-label="Waveform"
  data-area="waveform"
  bind:this={section}
>
  <div class="head">
    {#if file}
      <span class="name" title={file.path}>{fileName}</span>
      <span class="info">{info}</span>
      {#if file.untitled}<span class="unsaved">Not saved yet</span>{/if}
    {:else}
      <span class="info">Waveform editor</span>
    {/if}
  </div>

  {#if doc && file}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <canvas
      class="ruler"
      bind:this={ruler}
      onpointerdown={onRulerPointer}
      onwheel={onWheel}
      oncontextmenu={(e) => {
        e.preventDefault();
        menu = { x: e.clientX, y: e.clientY, items: timeDisplayItems() };
      }}
    ></canvas>
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="area" bind:this={area} bind:clientHeight={height} onwheel={onWheel} oncontextmenu={onMenu}>
      <div class="measure" bind:clientWidth={width} style:right="{SCALE_W}px"></div>
      <canvas bind:this={canvas} onpointerdown={onPointer} ondblclick={() => run("edit.select-all")}></canvas>
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="hud" class:active={levelDrag} onpointerdown={onLevelPointer} title="Drag right or up to turn the selection (or the whole file) up, left or down to turn it down. Release to apply. Shift for fine steps.">
        <span class="knob">
          <svg viewBox="0 0 16 16" aria-hidden="true"><circle cx="8" cy="8" r="6" fill="none" stroke="currentColor" stroke-width="1.4" /><path d="M8 8l{3.5 * Math.sin(Math.max(-1, Math.min(1, level / 24)) * 2.4)} {-3.5 * Math.cos(Math.max(-1, Math.min(1, level / 24)) * 2.4)}" stroke="currentColor" stroke-width="1.6" /></svg>
        </span>
        <span class="val">{level > 0 ? "+" : ""}{level.toFixed(1)} dB</span>
      </div>
      {#if levelTip}
        <div class="level-tip" style:left="{levelTip.x}px" style:top="{levelTip.y}px">{levelTip.text}</div>
      {/if}
      {#if channels > 1}
        {#each chNames as label, ch (ch)}
          {@const name = channels === 2 ? (ch === 0 ? "left" : "right") : label}
          {@const on = $waveChannels[ch] ?? true}
          {#if !on}<div class="chdim" style:top="{(ch * height) / channels}px" style:height="{height / channels}px" style:right="{SCALE_W}px"></div>{/if}
          {#if height / channels >= 14}
            <button
              type="button"
              class="chbtn"
              class:on
              style:top="{(ch * height) / channels + (height / channels >= 24 ? 4 : 1)}px"
              aria-pressed={on}
              title={on
                ? `Switch the ${name} channel off: edits, effects, copy and paste then reach only the channels still on${channels === 2 ? ` (${ch === 0 ? "Up" : "Down"} picks this channel alone)` : ""}`
                : `Switch the ${name} channel back on`}
              onclick={() => waveChannels.update((c) => toggleChannel(c, ch))}>{label}</button
            >
          {/if}
        {/each}
      {/if}
    </div>
    {#if ed.showSpectrum || ed.showPitch}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="divider" title="Drag to share the space between the waveform and the spectrum" onpointerdown={onSplitPointer}></div>
      <SpectrumView
        {doc}
        {view}
        {width}
        scaleWidth={SCALE_W}
        {total}
        {playhead}
        {selection}
        tool={ed.spectralTool}
        brushSize={ed.brushSize}
        showSpectrum={ed.showSpectrum}
        showPitch={ed.showPitch}
        onSeek={(s) => void transport.seek(s)}
        onSelectTime={(sel) => setView({ selection: sel })}
        {run}
        {tip}
        onTimeWheel={onWheel}
        onColors={(x, y) => (colorPop = { x, y })}
      />
    {/if}
    <div class="hbar">
      <div class="track" style:margin-right="{SCALE_W}px">
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="thumb" style:width="{thumbW}px" style:left="{thumbX}px" onpointerdown={onThumbPointer}></div>
      </div>
    </div>
    <TransportBar {transport} {rate} {playhead} end={total} hasSelection={!!selection} {shortcutFor} zoom={(k) => { if (k !== "tracks-in" && k !== "tracks-out") zoom(k); }} amplitude />
  {:else}
    <div class="empty">
      <p>No file is open in the Waveform editor.</p>
      <p class="hint">Double-click a file in the Files panel, or select a clip and choose Clip &gt; Open in Waveform Editor.</p>
    </div>
  {/if}
</section>

{#if colorPop}
  <ColorPairPopover
    x={colorPop.x}
    y={colorPop.y}
    title="Spectrogram colors"
    lowLabel="Quiet"
    highLabel="Loud"
    low={appColors.spectrumLow}
    high={appColors.spectrumHigh}
    lowDefault={themeColor("--spectrum-low") || SPECTRUM_LOW}
    highDefault={themeColor("--spectrum-high") || SPECTRUM_HIGH}
    onChange={setSpectrumColors}
    onClose={() => (colorPop = null)}
  />
{/if}
{#if menu}
  <ContextMenu x={menu.x} y={menu.y} items={menu.items} onClose={() => (menu = null)} />
{/if}

<style>
  .wave {
    display: grid;
    grid-template-rows: auto auto 1fr auto auto;
    grid-template-columns: minmax(0, 1fr);
    height: 100%;
    overflow: hidden;
    background: var(--surface-0);
  }

  .divider {
    cursor: row-resize;
    background: var(--track-gap);
  }

  .divider:hover {
    background: var(--accent-dim);
  }


  .head {
    display: flex;
    gap: 8px;
    align-items: center;
    min-height: 26px;
    padding: 2px 8px;
    border-bottom: 1px solid var(--border);
    background: var(--surface-1);
  }

  .name {
    overflow: hidden;
    color: var(--text);
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .info {
    color: var(--text-faint);
    font-size: var(--font-size-small);
    white-space: pre;
  }

  .unsaved {
    color: var(--clip-5);
    font-size: var(--font-size-small);
  }

  .ruler {
    display: block;
    cursor: text;
    border-bottom: 1px solid var(--border);
  }

  .area {
    position: relative;
    min-height: 0;
    overflow: hidden;
  }
  /* A channel switched off: edits pass it by. */
  .chdim {
    position: absolute;
    left: 0;
    background: rgba(0, 0, 0, 0.55);
    pointer-events: none;
  }
  .chbtn {
    position: absolute;
    right: 2px;
    min-width: 16px;
    height: 16px;
    padding: 0 2px;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    background: var(--surface-1, #111);
    color: var(--text-faint);
    font: inherit;
    font-size: 10px;
    line-height: 14px;
    cursor: pointer;
  }
  .chbtn.on {
    border-color: var(--accent);
    color: var(--accent);
  }

  .measure {
    position: absolute;
    top: 0;
    left: 0;
    height: 0;
  }

  .area canvas {
    display: block;
    cursor: text;
  }

  .hud {
    position: absolute;
    top: 10px;
    left: 50%;
    display: flex;
    gap: 6px;
    align-items: center;
    padding: 3px 8px;
    border: 1px solid var(--border-strong);
    border-radius: 3px;
    background: color-mix(in srgb, var(--surface-2) 88%, transparent);
    color: var(--text-dim);
    font-size: var(--font-size-small);
    transform: translateX(-50%);
    user-select: none;
  }

  /* The level beside the pointer while dragging, as on a clip's level handle. */
  .level-tip {
    position: fixed;
    z-index: 4000;
    padding: 2px 6px;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    background: var(--surface-3);
    color: var(--value);
    font-family: var(--font-mono);
    font-size: 11px;
    pointer-events: none;
    white-space: nowrap;
  }
  .hud.active {
    border-color: var(--accent);
    color: var(--text);
  }

  .hud {
    cursor: ew-resize;
  }

  .knob {
    display: grid;
    place-items: center;
    cursor: ew-resize;
    color: var(--text);
  }

  .knob svg {
    width: 16px;
    height: 16px;
  }

  .val {
    min-width: 56px;
    color: var(--value);
    font-family: var(--font-mono);
  }

  .hbar {
    height: 10px;
    padding: 1px 0;
    background: var(--surface-1);
  }

  .hbar .track {
    position: relative;
    height: 100%;
  }

  .thumb {
    position: absolute;
    top: 0;
    bottom: 0;
    border-radius: 4px;
    background: var(--surface-3);
  }

  .thumb:hover {
    background: var(--border-strong);
  }

  .empty {
    display: grid;
    align-content: center;
    justify-items: center;
    gap: 4px;
    grid-row: 2 / -1;
    color: var(--text-dim);
  }

  .empty p {
    margin: 0;
  }

  .hint {
    color: var(--text-faint);
    font-size: var(--font-size-small);
  }
</style>
