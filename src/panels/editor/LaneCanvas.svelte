<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  One track lane: clips with their waveforms, drawn on a canvas sized to
  the visible width. Redraws when the view, the track, or peaks change,
  never as the playhead moves (that line is the editor's, over every lane).
-->
<script lang="ts">
  import { untrack } from "svelte";
  import { drawGainHandle, gainBox } from "./clip-gain";
  import { hiddenEdges } from "./lane-hit";
  import type { PeakEntry, PeakStore } from "../../stores/peaks";
  import { clipLength, stretchOf, type ClipDoc, type FadeDoc, type MediaRefDoc, type TrackDoc } from "../../stores/session";
  import type { LiveWave } from "../../stores/record";
  import { channelLabels } from "../properties/channel-mix";
  import { defaultTrackColor, trackHeight, peakRange, pickLevel, xOf, type Viewport, TRACK_GAP } from "./timeline";
  import { chipBox, chipLabel, fxLaneOf, pointsOf, rangeOf, restValue, valueToY, type EnvKind, type FxLane } from "./envelope";
  import { exactSamples, SAMPLE_VIEW } from "../../stores/exact-samples";
  import { appearanceTick } from "../../stores/appearance";

  interface Props {
    track: TrackDoc;
    media: ReadonlyMap<string, MediaRefDoc>;
    sessionRate: number;
    view: Viewport;
    width: number;
    selectedClips: ReadonlySet<string>;
    trackSelected: boolean;
    /** Row number, for alternating lane shades. */
    index?: number;
    /** Waveform display scale. */
    ampZoom?: number;
    /** A take being recorded on this track: where it starts, the input rate, and min/max blocks. */
    liveTake?: { at: number; rate: number; wave: LiveWave; tick: number } | null;
    peaks: PeakStore;
    /** Changes whenever any peak data changes. */
    peakTick: number;
    /** Draw clip volume envelopes. */
    showVolume?: boolean;
    /** Draw clip pan envelopes. */
    showPan?: boolean;
    /** Effect-setting envelopes, and which setting each clip shows. */
    showFx?: boolean;
    fxLanes?: Record<string, FxLane>;
    /** Draw stretch handles in each clip's top corners. */
    stretchHandles?: boolean;
    /** Not heard (the track is muted, or others are soloed): every clip greyed out. */
    silent?: boolean;
  }

  const { track, media, sessionRate, view, width, selectedClips, trackSelected, index = 0, ampZoom = 1, liveTake = null, peaks, peakTick, showVolume = false, showPan = false, showFx = false, fxLanes = {}, stretchHandles = false, silent = false }: Props = $props();

  let canvas = $state<HTMLCanvasElement | null>(null);

  function css(name: string): string {
    return getComputedStyle(document.documentElement).getPropertyValue(name).trim() || "#888";
  }

  /** Fade gain for t from 0 to 1. Matches the playback engine. */
  function fadeGain(shape: FadeDoc["shape"], t: number, bend = 0): number {
    const b = Math.max(-1, Math.min(1, bend || 0));
    const u = Math.pow(Math.min(1, Math.max(0, t)), b === 0 ? 1 : Math.pow(4, -b));
    if (shape === "logarithmic") return 1 - (1 - u) * (1 - u);
    if (shape === "cosine") return 0.5 - 0.5 * Math.cos(Math.PI * u);
    if (shape === "exponential") return u * u;
    if (shape === "equal_power") return Math.sin((Math.PI / 2) * u);
    return u;
  }

  /** The clip's fade gain `tl` session frames into a clip `len` long, so the drawn waveform follows its fades. */
  function fadeAt(clip: ClipDoc, tl: number, len: number): number {
    let g = 1;
    const fi = clip.fade_in.length;
    const fo = clip.fade_out.length;
    if (fi > 0 && tl < fi) g *= fadeGain(clip.fade_in.shape, Math.max(0, tl) / fi, clip.fade_in.curve);
    if (fo > 0 && tl > len - fo) g *= fadeGain(clip.fade_out.shape, Math.max(0, len - tl) / fo, clip.fade_out.curve);
    return g;
  }

  /** Shades the silent side of a fade and draws its curve. */
  function drawFade(g: CanvasRenderingContext2D, fade: FadeDoc, from: number, to: number, top: number, height: number, rising: boolean) {
    if (fade.length <= 0 || to - from < 1) return;
    const bottom = top + height;
    const steps = Math.max(2, Math.ceil(to - from));
    g.beginPath();
    g.moveTo(from, top);
    for (let i = 0; i <= steps; i++) {
      const x = from + ((to - from) * i) / steps;
      const t = i / steps;
      const gain = fadeGain(fade.shape, rising ? t : 1 - t, fade.curve);
      g.lineTo(x, bottom - gain * height);
    }
    g.lineTo(to, top);
    g.closePath();
    g.fillStyle = "rgba(0, 0, 0, 0.2)";
    g.fill();
    g.beginPath();
    for (let i = 0; i <= steps; i++) {
      const x = from + ((to - from) * i) / steps;
      const t = i / steps;
      const y = bottom - fadeGain(fade.shape, rising ? t : 1 - t, fade.curve) * height;
      if (i === 0) g.moveTo(x, y);
      else g.lineTo(x, y);
    }
    g.strokeStyle = "rgba(255, 255, 255, 0.7)";
    g.lineWidth = 1;
    g.stroke();
  }

  /** Mixes a color toward white by `amount` (0 to 1), or toward black when negative. Unknown formats come back unchanged. */
  function lighten(c: string, amount: number): string {
    const m = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(c.trim());
    let r: number, gr: number, b: number;
    if (m) {
      const h = m[1]!.length === 3 ? m[1]!.replace(/./g, (x) => x + x) : m[1]!;
      [r, gr, b] = [0, 2, 4].map((i) => parseInt(h.slice(i, i + 2), 16)) as [number, number, number];
    } else {
      const n = /^rgba?\(\s*(\d+)[\s,]+(\d+)[\s,]+(\d+)/i.exec(c.trim());
      if (!n) return c;
      [r, gr, b] = [Number(n[1]), Number(n[2]), Number(n[3])];
    }
    const mix = (v: number) => Math.round(amount >= 0 ? v + (255 - v) * amount : v * (1 + amount));
    return `rgb(${mix(r)}, ${mix(gr)}, ${mix(b)})`;
  }

  function clipColor(clip: ClipDoc): string {
    const c = clip.color ?? track.color ?? defaultTrackColor(index);
    return c.startsWith("--") ? css(c) : c;
  }

  // Redraw when exact samples (for close zoom) arrive.
  let exactTick = $state(0);
  $effect(() => exactSamples.subscribe(() => exactTick++));
  // Redraw when the theme or colors change.
  let looks = $state(0);
  $effect(() => appearanceTick.subscribe((v) => (looks = v)));

  // Waveform data for this lane's own files only, so a file that finishes
  // loading redraws the lanes that use it, not every lane in the session.
  const peakKey = $derived.by(() => {
    void peakTick;
    let key = "";
    for (const c of track.clips) {
      const m = media.get(c.media_id);
      const e = m ? peaks.get(m.path) : null;
      key += e ? `${e.status}${Math.round(e.progress * 20)}.${e.levels.length}|` : "-|";
    }
    return key;
  });

  // Worked out only when the clips or the view change, not on every playhead move.
  const hidden = $derived.by(() => hiddenEdges({ view, media, rate: sessionRate }, track));

  $effect(() => {
    void peakKey;
    void exactTick;
    void looks;
    void liveTake?.tick;
    const c = canvas;
    if (!c) return;
    const dpr = window.devicePixelRatio || 1;
    const h = trackHeight(track);
    const bw = Math.max(1, Math.round(width * dpr));
    const bh = Math.max(1, Math.round(h * dpr));
    c.width = bw;
    c.height = bh;
    c.style.width = `${width}px`;
    c.style.height = `${h}px`;
    const g = c.getContext("2d");
    if (!g) return;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    g.clearRect(0, 0, width, h);
    // The bottom TRACK_GAP pixels are the dark break between tracks.
    const lh = h - TRACK_GAP;
    const laneFill = trackSelected ? css("--lane-selected") : css(index % 2 ? "--lane-b" : "--lane-a");
    g.fillStyle = laneFill;
    g.fillRect(0, 0, width, lh);

    const titleH = 14;
    // A take being recorded is drawn as a clip of its own, from its live
    // peak data, by the same code as every finished clip.
    const lw = liveTake && liveTake.rate > 0 && liveTake.wave.blocks > 0 ? liveTake : null;
    const fadeNone = { length: 0, shape: "linear", curve: 0 };
    const liveClip: ClipDoc | null = lw
      ? { id: "__live__", name: "Recording", media_id: "__live__", start: lw.at, source_in: 0, source_out: lw.wave.frames, gain_db: 0, muted: false, locked: false, color: null, fade_in: fadeNone, fade_out: fadeNone }
      : null;
    const liveMedia = lw ? ({ id: "__live__", path: "__live__", sample_rate: lw.rate, channels: lw.wave.channels, length: lw.wave.frames } as unknown as MediaRefDoc) : null;
    const liveEntry = lw
      ? ({
          status: "ready",
          progress: 1,
          error: null,
          summary: { sampleRate: lw.rate, channels: lw.wave.channels, levels: [], length: lw.wave.frames },
          levels: [lw.wave.levelFor((view.spp * lw.rate) / sessionRate)],
        } as unknown as PeakEntry)
      : null;
    // Theme: how strongly a clip's color fills its body, whether it shines,
    // and whether its waveform is drawn lighter (dark theme) or deeper.
    const bodyAlpha = Number(css("--clip-body-alpha")) || 0.2;
    const waveTone = Number(css("--clip-wave-tone")) || 0.55;
    const gloss = css("--clip-gloss") === "1";
    // Muted clips: drawn through the theme's muted look (greyscale unless the theme says otherwise).
    const mutedFilter = css("--clip-muted-filter") || "grayscale(1) brightness(0.8)";
    for (const clip of liveClip ? [...track.clips, liveClip] : track.clips) {
      g.filter = clip.muted ? mutedFilter : "none";
      const m = clip === liveClip ? liveMedia! : media.get(clip.media_id);
      const len = clipLength(clip, m, sessionRate);
      const x0 = xOf(clip.start, view);
      const x1 = xOf(clip.start + len, view);
      if (x1 < 0 || x0 > width) continue;
      const left = Math.max(0, x0);
      const right = Math.min(width, x1);
      const selected = selectedClips.has(clip.id);
      const color = clipColor(clip);

      // Body and title bar. The lane color goes down first, so a clip
      // covers any clip under it instead of letting it show through.
      // Clearing first keeps see-through themes looking the same.
      g.globalAlpha = 1;
      g.clearRect(left, 1, right - left, lh - 2);
      g.fillStyle = laneFill;
      g.fillRect(left, 1, right - left, lh - 2);
      g.globalAlpha = clip.muted ? 0.6 : 1;
      g.fillStyle = color;
      g.globalAlpha *= bodyAlpha;
      g.fillRect(left, 1, right - left, lh - 2);
      if (gloss) {
        // Aero themes: a shine across the top half of the clip.
        const sh = g.createLinearGradient(0, 1, 0, lh / 2);
        sh.addColorStop(0, "rgba(255, 255, 255, 0.55)");
        sh.addColorStop(1, "rgba(255, 255, 255, 0)");
        g.globalAlpha = clip.muted ? 0.6 : 1;
        g.fillStyle = sh;
        g.fillRect(left, 1, right - left, lh / 2);
      }
      g.globalAlpha = clip.muted ? 0.6 : 1;
      g.fillStyle = color;
      g.fillRect(left, 1, right - left, titleH);
      g.fillStyle = "#0e0e10";
      g.font = `11px ${css("--font-ui")}`;
      g.textBaseline = "middle";
      g.save();
      g.beginPath();
      g.rect(left, 1, right - left, titleH);
      g.clip();
      const st = stretchOf(clip);
      // A stretched clip says so in its title, with the pitch mode.
      const semis = clip.looped || !clip.stretch?.keep_pitch ? 0 : (clip.stretch.pitch ?? 0);
      const pitchText = semis ? `  ${semis > 0 ? "+" : ""}${Math.round(semis * 100) / 100} st` : "";
      const label = st === 1 && !semis ? clip.name : `${clip.name}  ${st === 1 ? "" : `${Math.round(st * 1000) / 10}%`}${clip.stretch?.keep_pitch === false ? " tape" : ""}${pitchText}`;
      // Clear of the stretch handle in the corner, when shown.
      const indent = stretchHandles && !clip.looped && !clip.locked && x1 - x0 > 26 ? 14 : 4;
      g.fillText(label, Math.max(x0, 0) + indent, 1 + titleH / 2);
      g.restore();

      // Waveform.
      const body = { top: 1 + titleH, height: lh - 2 - titleH };
      const entry = clip === liveClip ? liveEntry : m ? peaks.get(m.path) : null;
      if (entry?.status === "ready" && entry.summary) {
        const ch = entry.summary.channels;
        const srcRate = m!.sample_rate || entry.summary.sampleRate;
        // A stretched clip covers less (or more) of its file per pixel.
        const ratio = stretchOf(clip);
        const framesPerPx = (view.spp * srcRate) / sessionRate / ratio;
        const level = pickLevel(entry.levels, framesPerPx);
        if (!level || level.data.length === 0) {
          // Ready but nothing to draw: say so instead of leaving the clip blank.
          g.save();
          g.fillStyle = css("--danger");
          g.font = `11px ${css("--font-ui")}`;
          g.fillText(`No waveform data (${entry.levels.length} levels)`, left + 6, body.top + 12);
          g.restore();
        }
        // Zoomed in close: the file's exact samples, a line through them
        // with a dot on each once they are 6 pixels apart.
        const exact =
          framesPerPx <= SAMPLE_VIEW && clip !== liveClip && !clip.looped && !clip.reversed && ratio === 1 && m
            ? exactSamples.get(
                m.path,
                Math.max(clip.source_in, Math.floor(clip.source_in + ((view.scroll + left * view.spp - clip.start) * srcRate) / sessionRate) - 1),
                Math.min(clip.source_out, Math.ceil(clip.source_in + ((view.scroll + right * view.spp - clip.start) * srcRate) / sessionRate) + 1),
              )
            : null;
        // The waveform grows and shrinks with the clip's level.
        const clipLevel = Math.pow(10, (clip.gain_db ?? 0) / 20);
        if (exact) {
          const tone = lighten(color, waveTone);
          g.strokeStyle = tone;
          g.fillStyle = tone;
          g.lineWidth = 1.25;
          g.globalAlpha = clip.muted ? 0.6 : 1;
          const laneH = body.height / ch;
          const fA = Math.max(clip.source_in, exact.start, Math.floor(clip.source_in + ((view.scroll + left * view.spp - clip.start) * srcRate) / sessionRate) - 1);
          const fB = Math.min(clip.source_out, exact.start + exact.count, Math.ceil(clip.source_in + ((view.scroll + right * view.spp - clip.start) * srcRate) / sessionRate) + 1);
          const pxPerFrame = 1 / framesPerPx;
          g.save();
          g.beginPath();
          g.rect(left, body.top, right - left, body.height);
          g.clip();
          for (let k = 0; k < ch; k++) {
            const mid = body.top + laneH * (k + 0.5);
            const hc = Math.min(k, exact.channels - 1);
            const at = (f: number): [number, number] => {
              const tl = ((f - clip.source_in) * sessionRate) / srcRate;
              const v = Math.max(-1, Math.min(1, (exact.data[(f - exact.start) * exact.channels + hc] ?? 0) * ampZoom * clipLevel * fadeAt(clip, tl, len)));
              return [xOf(clip.start + tl, view), mid - v * (laneH / 2 - 1)];
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
          g.restore();
          g.globalAlpha = 1;
        } else if (level) {
          // Much lighter than the clip body so the waveform stands out.
          g.fillStyle = lighten(color, waveTone);
          g.globalAlpha = clip.muted ? 0.6 : 1;
          const laneH = body.height / ch;
          // A looped clip wraps back to the start of its region.
          const region = clip.source_out - clip.source_in;
          const phase = clip.looped ? clip.looped.phase : 0;
          for (let px = Math.floor(left); px < Math.ceil(right); px++) {
            const tl0 = view.scroll + px * view.spp - clip.start;
            let into = (tl0 * srcRate) / sessionRate / ratio;
            if (clip.looped && region > 0) into = (((into + phase) % region) + region) % region;
            const s0 = clip.source_in + into;
            const s1 = s0 + framesPerPx;
            if (s1 < clip.source_in || s0 >= clip.source_out) continue;
            const faded = fadeAt(clip, tl0 + view.spp / 2, len);
            for (let k = 0; k < ch; k++) {
              // Reversed: positions count from the file's end.
              const fileLen = m?.length ?? clip.source_out;
              const [a0, a1] = [Math.max(s0, clip.source_in), Math.min(s1, clip.source_out)];
              const [lo0, hi0] = clip.reversed ? peakRange(level, ch, k, fileLen - a1, fileLen - a0) : peakRange(level, ch, k, a0, a1);
              // Inverted: drawn upside down.
              const flipped = clip.inverted && (!clip.invert_channel || clip.invert_channel === "both" || (ch === 2 && (clip.invert_channel === "left" ? k === 0 : k === 1)));
              const [lo, hi] = flipped ? [-hi0, -lo0] : [lo0, hi0];
              const mid = body.top + laneH * (k + 0.5);
              const amp = (v: number) => Math.max(-1, Math.min(1, v * ampZoom * clipLevel * faded));
              const y0 = mid - amp(hi) * (laneH / 2 - 1);
              const y1 = mid - amp(lo) * (laneH / 2 - 1);
              g.fillRect(px, y0, 1, Math.max(1, y1 - y0));
            }
          }
          g.globalAlpha = 1;
        }
        // Multichannel files: each row's channel name, when rows are tall enough.
        if (ch > 2 && body.height / ch >= 12) {
          const names = channelLabels(ch);
          const rowH = body.height / ch;
          g.save();
          g.font = `9px ${css("--font-mono")}`;
          g.fillStyle = "rgba(255, 255, 255, 0.6)";
          g.textBaseline = "top";
          for (let k = 0; k < ch; k++) g.fillText(names[k]!, Math.max(x0, 0) + 16, body.top + rowH * k + 2);
          g.restore();
        }
        // Looped clips: a dashed line where each repeat begins.
        if (clip.looped && clip.source_out > clip.source_in) {
          const regionTl = ((clip.source_out - clip.source_in) * sessionRate) / srcRate;
          const firstTl = regionTl - (clip.looped.phase * sessionRate) / srcRate;
          if (regionTl / view.spp >= 4) {
            g.save();
            g.strokeStyle = "rgba(255, 255, 255, 0.55)";
            g.setLineDash([3, 3]);
            g.lineWidth = 1;
            for (let t = firstTl; t < clip.looped.length; t += regionTl) {
              const lx = Math.round(xOf(clip.start + t, view)) + 0.5;
              if (lx < left || lx > right) continue;
              g.beginPath();
              g.moveTo(lx, body.top);
              g.lineTo(lx, body.top + body.height);
              g.stroke();
            }
            g.restore();
          }
        }
      } else {
        // Status text, kept inside the clip so neighbors never overlap.
        g.save();
        g.beginPath();
        g.rect(left, body.top, right - left, body.height);
        g.clip();
        g.font = `11px ${css("--font-ui")}`;
        if (!entry || entry.status === "error") {
          g.fillStyle = css("--danger");
          const why = entry?.error === "File not found" || !entry ? "Media offline" : `No waveform: ${entry.error ?? ""}`;
          g.fillText(why, left + 6, body.top + 12);
        } else {
          g.fillStyle = css("--text-dim");
          g.fillText(`Building waveform ${Math.round(entry.progress * 100)}%`, left + 6, body.top + 12);
        }
        g.restore();
      }

      // Fades, and their handles in the title bar.
      g.globalAlpha = 1;
      const fi = x0 + clip.fade_in.length / view.spp;
      const fo = x1 - clip.fade_out.length / view.spp;
      g.save();
      g.beginPath();
      g.rect(left, body.top, right - left, body.height);
      g.clip();
      drawFade(g, clip.fade_in, x0, fi, body.top, body.height, true);
      drawFade(g, clip.fade_out, fo, x1, body.top, body.height, false);
      g.restore();
      if (x1 - x0 > 30) {
        // Fade handles sit just under the title bar, large and light so they are easy to see and grab.
        const hy = 1 + titleH + 3;
        for (const hx of [fi, fo]) {
          if (hx < -8 || hx > width + 8) continue;
          const cx = Math.min(Math.max(hx, x0 + 8), x1 - 8);
          g.fillStyle = "#ffffff";
          g.fillRect(Math.round(cx) - 6, hy, 13, 13);
          g.strokeStyle = "#0e0e10";
          g.lineWidth = 1.5;
          g.strokeRect(Math.round(cx) - 5.25, hy + 0.75, 11.5, 11.5);
          g.lineWidth = 1;
        }
      }
      // Stretch handles: a white triangle filling each top corner of the
      // title bar (11 pixels, matching the Editor's hit test). Brighter on
      // a clip that is already stretched.
      if (stretchHandles && !clip.looped && !clip.locked && x1 - x0 > 26) {
        const s = 11;
        g.fillStyle = clip.stretch ? "#ffffff" : "rgba(255, 255, 255, 0.8)";
        g.strokeStyle = "#0e0e10";
        g.lineWidth = 1;
        for (const [cx, dir] of [[x0 + 1, 1], [x1 - 1, -1]] as const) {
          if (cx < -s || cx > width + s) continue;
          g.beginPath();
          g.moveTo(cx, 1);
          g.lineTo(cx + dir * s, 1);
          g.lineTo(cx, 1 + s);
          g.closePath();
          g.fill();
          g.stroke();
        }
      }
      // Envelope chip: which envelope lines show; click it to change.
      const chip = chipBox(x0, x1, chipLabel(showVolume, showPan, showFx));
      let tagRight = right - 14;
      if (chip && chip.x >= left && chip.x + chip.w <= right) {
        g.fillStyle = "rgba(14, 14, 16, 0.55)";
        g.beginPath();
        g.roundRect(chip.x, 2, chip.w, titleH - 2, 3);
        g.fill();
        g.fillStyle = "#f2f2f2";
        g.font = `10px ${css("--font-ui")}`;
        g.textBaseline = "middle";
        g.fillText(`${chipLabel(showVolume, showPan, showFx)} \u25BE`, chip.x + 6, 1 + titleH / 2);
        tagRight = chip.x - 6;
      }
      if (clip.locked || clip.group || clip.looped) {
        const tag = [
          clip.looped ? "Looped" : "",
          clip.group ? "Grouped" : "",
          clip.locked ? "Locked" : "",
        ]
          .filter(Boolean)
          .join("  ");
        g.font = `10px ${css("--font-ui")}`;
        const w = g.measureText(tag).width;
        if (tagRight - left > w + 40) {
          g.fillStyle = "#0e0e10";
          g.textBaseline = "middle";
          g.fillText(tag, tagRight - w, 1 + titleH / 2);
        }
      }

      // Envelopes: a line across the body with a handle per point.
      // Volume is yellow, pan is blue (full left at the top).
      // An effect setting is violet, its top the control's highest value.
      const fxLane = showFx ? fxLaneOf(clip, fxLanes) : null;
      const shown: [EnvKind, string][] = [];
      if (showVolume) shown.push(["volume", "255, 215, 94"]);
      if (showPan) shown.push(["pan", "120, 235, 255"]);
      if (fxLane) shown.push(["fx", "205, 150, 255"]);
      for (const [kind, rgb] of shown) {
        const pts = pointsOf(clip, kind, fxLane);
        const range = rangeOf(kind, clip, fxLane);
        const rest = restValue(kind, clip, fxLane);
        const yAt = (v: number) => valueToY(kind, v, body.top + 2, body.height - 4, range);
        g.save();
        g.beginPath();
        g.rect(left, body.top, right - left, body.height);
        g.clip();
        g.strokeStyle = pts.length ? `rgb(${rgb})` : `rgba(${rgb}, 0.8)`;
        g.lineWidth = 1.5;
        g.beginPath();
        if (pts.length === 0) {
          g.moveTo(x0, yAt(rest));
          g.lineTo(x1, yAt(rest));
        } else {
          g.moveTo(x0, yAt(pts[0]!.value));
          for (const p of pts) g.lineTo(xOf(clip.start + p.position, view), yAt(p.value));
          g.lineTo(x1, yAt(pts[pts.length - 1]!.value));
        }
        g.stroke();
        g.fillStyle = `rgb(${rgb})`;
        for (const p of pts) {
          const px = xOf(clip.start + p.position, view);
          g.fillRect(Math.round(px) - 3, Math.round(yAt(p.value)) - 3, 6, 6);
        }
        g.restore();
      }

      // Raised edge: a light top and left, a dark bottom and right, so clips stand off the lane.
      g.globalAlpha = clip.muted ? 0.6 : 1;
      g.fillStyle = "rgba(255, 255, 255, 0.28)";
      g.fillRect(left, 1, right - left, 1);
      if (x0 >= 0) g.fillRect(x0 + 1, 1, 1, lh - 3);
      g.fillStyle = "rgba(0, 0, 0, 0.55)";
      g.fillRect(left, lh - 2, right - left, 1);
      if (x1 <= width) g.fillRect(x1 - 2, 1, 1, lh - 3);

      // Outline.
      g.globalAlpha = 1;
      if (selected) {
        // Clear selection: a light wash over the whole clip, a dark halo and a thick white outline.
        g.fillStyle = "rgba(255, 255, 255, 0.14)";
        g.fillRect(left, 1, right - left, lh - 2);
        g.strokeStyle = "rgba(0, 0, 0, 0.7)";
        g.lineWidth = 4;
        g.strokeRect(x0 + 0.5, 1.5, x1 - x0 - 1, lh - 3);
        g.strokeStyle = "#ffffff";
        g.lineWidth = 2;
        g.strokeRect(x0 + 0.5, 1.5, x1 - x0 - 1, lh - 3);
      } else {
        g.strokeStyle = color;
        g.lineWidth = 1;
        g.strokeRect(x0 + 0.5, 1.5, x1 - x0 - 1, lh - 3);
      }
      // Level handle in the bottom-left corner (see clip-gain.ts), with the level when it is not 0.
      const gb = clip === liveClip ? null : gainBox(x0, x1, lh - 2, 1 + titleH);
      if (gb) drawGainHandle(g, gb, clip.gain_db, `10px ${css("--font-ui")}`);
    }
    g.filter = "none";

    // Clip edges under a clip on top (overlaps and crossfades): a strip of
    // the hidden clip's color along the bottom of the overlap, and a line
    // where it starts or ends, so it can be seen and grabbed.
    for (const e of hidden) {
      const a = Math.min(e.x, e.to);
      const b = Math.max(e.x, e.to);
      if (b < 0 || a > width) continue;
      const color = clipColor(e.clip);
      g.globalAlpha = 0.85;
      g.fillStyle = color;
      g.fillRect(a, lh - 7, b - a, 4);
      g.globalAlpha = 1;
      g.fillStyle = "rgba(0, 0, 0, 0.75)";
      g.fillRect(Math.round(e.x) - 1, titleH + 2, 3, lh - titleH - 4);
      g.fillStyle = color;
      g.fillRect(Math.round(e.x), titleH + 2, 1, lh - titleH - 4);
      // The hidden clip's own fade under the overlap (a crossfade's other half), dashed.
      {
        const hc = e.clip;
        const hm = media.get(hc.media_id);
        const hlen = clipLength(hc, hm, sessionRate);
        const fade = e.side === "end" ? hc.fade_out : hc.fade_in;
        if (fade.length > 0) {
          const hx0 = xOf(hc.start, view);
          const hx1 = xOf(hc.start + hlen, view);
          const from = e.side === "end" ? hx1 - fade.length / view.spp : hx0;
          const to = e.side === "end" ? hx1 : hx0 + fade.length / view.spp;
          const top = titleH + 2;
          const height = lh - titleH - 4;
          const steps = Math.max(2, Math.ceil(to - from));
          g.save();
          g.beginPath();
          for (let i = 0; i <= steps; i++) {
            const t = i / steps;
            const x = from + (to - from) * t;
            const y = top + height - fadeGain(fade.shape, e.side === "end" ? 1 - t : t, fade.curve) * height;
            if (i === 0) g.moveTo(x, y);
            else g.lineTo(x, y);
          }
          g.setLineDash([4, 3]);
          g.strokeStyle = color;
          g.lineWidth = 1.5;
          g.stroke();
          g.restore();
        }
      }
      // A small tab on the hidden clip's side of the line.
      const dir = e.side === "start" ? 1 : -1;
      g.beginPath();
      g.moveTo(e.x, lh - 3);
      g.lineTo(e.x + dir * 6, lh - 3);
      g.lineTo(e.x, lh - 11);
      g.closePath();
      g.fill();
    }

    // Break between tracks. (The playhead and the start marker are one
    // line over every lane, drawn by the editor, so a lane never repaints
    // as they move.)
    g.fillStyle = css("--track-gap");
    g.fillRect(0, lh, width, TRACK_GAP);
  });
</script>

<canvas bind:this={canvas} class:silent aria-hidden="true"></canvas>

<style>
  /* A track not heard: its clips take the theme's muted look. */
  canvas.silent {
    filter: var(--clip-muted-filter, grayscale(1) brightness(0.8));
  }
  canvas {
    display: block;
  }
</style>
