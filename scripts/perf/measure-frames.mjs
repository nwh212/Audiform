// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

// Parts of this file run inside the browser page, so it is not type checked as Node code.
// @ts-nocheck

/**
 * Frame-rate measurement for the interface, in headless Chromium.
 *
 * It serves a built copy of the frontend (`npx vite build`, then `dist`),
 * fakes the Tauri core with a stub that returns plausible values, and
 * measures each theme in each scenario:
 *
 *   idle    the shell with nothing open
 *   phase   the Phase Wheel panel open and "playing" (a synthetic status
 *           that moves the playhead, synthetic wheel data)
 *   wave    the Waveform editor with a synthetic file, playing
 *   multi   the Multitrack editor with three tracks of clips, playing
 *   resize  dragging the sash between two panels
 *
 * For each run it prints: frames a second, median and 95th percentile
 * frame time, long tasks, the share of the main thread that was busy, and
 * the time spent in style, layout, paint and raster, from Chrome tracing.
 *
 * Headless Chromium has no real GPU, so compare numbers between themes and
 * between builds, not against what a real computer shows.
 *
 * Usage:
 *   node scripts/perf/measure-frames.mjs [--dist dist] [--themes dark,meadow]
 *     [--scenarios idle,multi,wave,phase,resize] [--seconds 4] [--json out.json]
 *     [--phase-ms 0] [--inject "css"] [--threads 1] [--shot prefix]
 *
 *   --phase-ms   pretend the core takes this long to answer a Phase Wheel request
 *   --inject     extra CSS, to try a fix without rebuilding
 *   --top        also print the events that took the most main thread time
 *   --threads    also print the busy time of each browser thread
 *   --shot       save a screenshot per run (prefix is a path start)
 *
 * Needs Playwright (`require("playwright")` must work) and its Chromium.
 */

import { createRequire } from "node:module";
import http from "node:http";
import fs from "node:fs";
import path from "node:path";

const { chromium } = createRequire(import.meta.url)("playwright");

const args = Object.fromEntries(
  process.argv.slice(2).reduce((acc, a, i, all) => (a.startsWith("--") ? [...acc, [a.slice(2), all[i + 1] ?? "true"]] : acc), []),
);
const dist = path.resolve(args.dist ?? "dist");
const themes = (args.themes ?? "dark,meadow,aqua,moonlight,prism,phosphor,analog,mallpastel,mallnight").split(",");
const scenarios = (args.scenarios ?? "idle,phase,resize").split(",");
const seconds = Number(args.seconds ?? 4);
const phaseMs = Number(args["phase-ms"] ?? 0);

const TYPES = { ".js": "text/javascript", ".css": "text/css", ".html": "text/html", ".svg": "image/svg+xml", ".webp": "image/webp", ".png": "image/png", ".woff2": "font/woff2", ".woff": "font/woff" };
const server = http
  .createServer((q, r) => {
    let p = path.join(dist, decodeURIComponent(q.url.split("?")[0]));
    if (!fs.existsSync(p) || fs.statSync(p).isDirectory()) p = path.join(dist, "index.html");
    r.setHeader("content-type", TYPES[path.extname(p)] || "application/octet-stream");
    fs.createReadStream(p).pipe(r);
  })
  .listen(0);
const port = server.address().port;

/** The stand-in for the Tauri core. Runs in the page before the app. */
function installStub({ theme, phaseMs }) {
  let cb = 1;
  let t0 = 0;
  const bins = (() => {
    const out = new Float32Array(1 + 1024 * 4);
    out[0] = 0.8;
    for (let i = 0; i < 1024; i++) {
      out[1 + i * 4] = Math.sin(i * 0.01) * 0.6;
      out[2 + i * 4] = 0.2 + 0.7 * ((i * 7) % 100) / 100;
      out[3 + i * 4] = i / 1024;
      out[4 + i * 4] = 0.8;
    }
    return out.buffer;
  })();
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const RATE = 48000;
  const LEN = RATE * 120;
  const chain = () => ({ enabled: true, input_gain_db: 0, output_gain_db: 0, mix: 1, slots: [] });
  const fade = () => ({ length: 0, shape: "linear", curve: 0 });
  const clip = (id, start) => ({ id, name: id, media_id: "m1", start, source_in: 0, source_out: LEN, gain_db: 0, muted: false, locked: false, color: null, fade_in: fade(), fade_out: fade(), effects: chain() });
  const track = (i) => ({
    id: `t${i}`, name: `Track ${i + 1}`, role: "standard", channels: "stereo", color: null, height: 90, volume_db: 0, pan: 0,
    muted: false, solo: false, record_armed: false, effects: chain(), clips: [clip(`c${i}`, 0), clip(`d${i}`, LEN + RATE)],
  });
  const session = (n, id) => ({
    format: "altaudio.session", schema_version: 1, id, name: "Perf session", sample_rate: RATE, bit_depth: 24,
    media: [{ id: "m1", path: "/perf/voice.flac", relative_path: null, sample_rate: RATE, channels: 2, length: LEN }],
    tracks: Array.from({ length: n }, (_, i) => track(i)), markers: [], view: { samples_per_pixel: 512, scroll: 0, scroll_y: 0, playhead: 0, selection: null },
    libraries: [], master: { volume_db: 0, channels: "stereo", height: 60 },
  });
  // Peaks: three levels of i16 min/max pairs per channel per block.
  const LEVELS = [{ block: 256, count: Math.ceil(LEN / 256) }, { block: 4096, count: Math.ceil(LEN / 4096) }, { block: 65536, count: Math.ceil(LEN / 65536) }];
  const peakLevel = (i) => {
    const n = LEVELS[i].count * 2 * 2;
    const d = new Int16Array(n);
    for (let k = 0; k < n; k += 2) {
      const amp = 9000 + 6000 * Math.sin(k / 400);
      d[k] = -amp;
      d[k + 1] = amp;
    }
    return d.buffer;
  };
  const wavePath = "/perf/voice.flac";
  const handlers = {
    preferences_load: () => ({ "app.settings": { theme } }),
    app_info: () => ({ version: "0.0.0", sessionSchemaVersion: 1 }),
    session_get: () => ({ session: session(3, "s1"), path: "/perf/perf.audfrm" }),
    keymap_load: () => ({}),
    workspace_list: () => [],
    effects_catalog: () => [],
    plugins_registry: () => ({ plugins: [], scanning: false }),
    prerender_status: () => ({}),
    preview_status: () => ({}),
    peaks_ensure: () => ({ sampleRate: RATE, channels: 2, length: LEN, levels: LEVELS }),
    peaks_level: (a) => peakLevel(a.level),
    wave_get: () => ({ session: session(1, "w1"), path: wavePath, format: "FLAC", bit_depth: 24, untitled: false }),
    wave_open: () => ({ session: session(1, "w1"), path: wavePath, format: "FLAC", bit_depth: 24, untitled: false }),
    transport_play: () => {
      t0 = performance.now();
      return null;
    },
    wave_play: () => {
      t0 = performance.now();
      return null;
    },
    transport_stop: () => 0,
    transport_status: () => ({
      playing: true,
      position: Math.round(((performance.now() - t0) / 1000) * RATE),
      trackIds: ["t0", "t1", "t2"],
      meters: [[0.5, 0.4], [0.3, 0.35], [0.2, 0.25]],
      master: [0.4 + 0.3 * Math.sin(performance.now() / 90), 0.4 + 0.3 * Math.cos(performance.now() / 110)],
      underruns: 0,
    }),
    scope_phase: async () => {
      if (phaseMs) await sleep(phaseMs);
      return bins.slice(0);
    },
    scope_phase_at: async () => bins.slice(0),
  };
  window.__TAURI_INTERNALS__ = {
    metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main", windowLabel: "main" } },
    transformCallback: () => cb++,
    unregisterCallback() {},
    convertFileSrc: (x) => x,
    invoke: async (cmd, a) => {
      const h = handlers[cmd];
      return h ? h(a ?? {}) : null;
    },
  };
}

/** Frame timer and long-task counter, inside the page. */
function installProbe() {
  window.__frames = [];
  window.__long = [];
  let last = 0;
  const tick = (now) => {
    if (last) window.__frames.push(now - last);
    last = now;
    requestAnimationFrame(tick);
  };
  requestAnimationFrame(tick);
  try {
    new PerformanceObserver((l) => {
      for (const e of l.getEntries()) window.__long.push(e.duration);
    }).observe({ entryTypes: ["longtask"] });
  } catch {
    // Long task reporting is not available.
  }
}

const pct = (a, p) => (a.length ? [...a].sort((x, y) => x - y)[Math.min(a.length - 1, Math.floor(a.length * p))] : 0);

/**
 * Busy time per thread kind (milliseconds): the top-level events of each
 * thread added up, so work on the compositor, raster and GPU threads shows
 * too (the main thread numbers miss it).
 */
function busyByThread(events) {
  const names = new Map();
  for (const e of events) if (e.ph === "M" && e.name === "thread_name") names.set(`${e.pid}:${e.tid}`, e.args.name);
  const byThread = new Map();
  for (const e of events) if (e.ph === "X" && e.dur) (byThread.get(`${e.pid}:${e.tid}`) ?? byThread.set(`${e.pid}:${e.tid}`, []).get(`${e.pid}:${e.tid}`)).push(e);
  const out = {};
  for (const [k, list] of byThread) {
    list.sort((a, b) => a.ts - b.ts || b.dur - a.dur);
    let end = 0;
    let sum = 0;
    for (const e of list) {
      if (e.ts >= end) {
        sum += e.dur;
        end = e.ts + e.dur;
      } else if (e.ts + e.dur > end) {
        sum += e.ts + e.dur - end;
        end = e.ts + e.dur;
      }
    }
    const kind = (names.get(k) ?? "other").replace(/\d+$/, "").replace(/ \d+$/, "");
    out[kind] = (out[kind] ?? 0) + sum / 1000;
  }
  return out;
}

/** The events that took the most time on the renderer's main thread (parents include their children). */
function topMainEvents(events, n = 12) {
  const main = new Set();
  for (const e of events) if (e.ph === "M" && e.name === "thread_name" && e.args.name === "CrRendererMain") main.add(`${e.pid}:${e.tid}`);
  const sums = {};
  for (const e of events) if (e.ph === "X" && e.dur && main.has(`${e.pid}:${e.tid}`)) sums[e.name] = (sums[e.name] ?? 0) + e.dur / 1000;
  return Object.entries(sums)
    .sort((a, b) => b[1] - a[1])
    .slice(0, n)
    .map(([k, v]) => `${k} ${v.toFixed(0)}`);
}

/** Sums trace event durations by name (microseconds to milliseconds). */
function sumTrace(events) {
  const want = ["UpdateLayoutTree", "Layout", "Paint", "PrePaint", "Layerize", "Commit", "RasterTask", "ImageDecodeTask", "FunctionCall", "EvaluateScript", "HitTest", "CompositeLayers", "UpdateLayer"];
  const sums = {};
  for (const e of events) if (e.ph === "X" && want.includes(e.name)) sums[e.name] = (sums[e.name] ?? 0) + (e.dur ?? 0) / 1000;
  return sums;
}

async function metrics(client) {
  const { metrics } = await client.send("Performance.getMetrics");
  return Object.fromEntries(metrics.map((m) => [m.name, m.value]));
}

async function openPhaseWheel(page) {
  await page.getByText("Window", { exact: true }).first().click();
  await page.getByText("Phase Wheel", { exact: false }).first().click();
  await page.waitForSelector("section.phase", { timeout: 5000 });
  await page.keyboard.press("Escape");
}

async function run(browser, theme, scenario) {
  const ctx = await browser.newContext({ viewport: { width: 1600, height: 900 } });
  const page = await ctx.newPage();
  page.on("pageerror", (e) => console.error("page error:", e.message));
  await page.addInitScript(installStub, { theme, phaseMs });
  await page.addInitScript(installProbe);
  if (args.inject) {
    // Extra CSS for trying a fix without rebuilding (for example "* { backdrop-filter: none !important }").
    await page.addInitScript((css) => {
      document.addEventListener("DOMContentLoaded", () => {
        const st = document.createElement("style");
        st.textContent = css;
        document.head.append(st);
      });
    }, args.inject);
  }
  await page.goto(`http://localhost:${port}/`);
  await page.waitForSelector("header, .shell", { timeout: 15000 });
  await page.waitForTimeout(1500);
  if (scenario === "phase") {
    await openPhaseWheel(page);
    await page.locator('button[aria-label="Play"]').first().click();
    await page.waitForTimeout(1000);
  }
  if (scenario === "multi") {
    await page.locator('button[aria-label="Play"]').first().click();
    await page.waitForTimeout(1000);
  }
  if (scenario === "wave") {
    // Double-click a clip: the core's wave_open stub gives back the file.
    await page.mouse.dblclick(900, 235);
    await page.waitForTimeout(1500);
    await page.keyboard.press("Escape");
    await page.locator('button[aria-label="Play"]').first().click();
    await page.waitForTimeout(1000);
  }
  if (args.shot) await page.screenshot({ path: `${args.shot}-${scenario}-${theme}.png` });
  const client = await ctx.newCDPSession(page);
  await client.send("Performance.enable");
  await page.evaluate(() => {
    window.__frames.length = 0;
    window.__long.length = 0;
  });
  await browser.startTracing(page, { categories: ["devtools.timeline", "disabled-by-default-devtools.timeline", "cc", "gpu", "viz"] });
  const m0 = await metrics(client);
  const start = Date.now();
  if (scenario === "resize") {
    // Drag a sash back and forth.
    const sash = page.locator(".dv-sash").first();
    const box = await sash.boundingBox();
    if (box) {
      const x = box.x + box.width / 2;
      const y = box.y + box.height / 2;
      await page.mouse.move(x, y);
      await page.mouse.down();
      const end = Date.now() + seconds * 1000;
      let k = 0;
      while (Date.now() < end) {
        await page.mouse.move(x + 60 * Math.sin(k / 8), y);
        k++;
        await page.waitForTimeout(8);
      }
      await page.mouse.up();
    }
  } else {
    await page.waitForTimeout(seconds * 1000);
  }
  const wall = (Date.now() - start) / 1000;
  const m1 = await metrics(client);
  const trace = JSON.parse((await browser.stopTracing()).toString());
  const frames = await page.evaluate(() => window.__frames.slice());
  const long = await page.evaluate(() => window.__long.slice());
  const events = Array.isArray(trace) ? trace : trace.traceEvents;
  const d = (k) => ((m1[k] ?? 0) - (m0[k] ?? 0)) * 1000;
  const result = {
    theme,
    scenario,
    fps: +(frames.length / wall).toFixed(1),
    medianMs: +pct(frames, 0.5).toFixed(1),
    p95Ms: +pct(frames, 0.95).toFixed(1),
    worstMs: +Math.max(0, ...frames).toFixed(1),
    longTasks: long.length,
    longTaskMs: +long.reduce((a, b) => a + b, 0).toFixed(0),
    mainBusyPct: +((d("TaskDuration") / (wall * 1000)) * 100).toFixed(1),
    scriptMs: +d("ScriptDuration").toFixed(0),
    styleMs: +d("RecalcStyleDuration").toFixed(0),
    layoutMs: +d("LayoutDuration").toFixed(0),
    top: topMainEvents(events),
    themeApplied: await page.evaluate(() => document.documentElement.dataset.theme ?? "dark"),
    threads: Object.fromEntries(Object.entries(busyByThread(events)).filter(([, v]) => v >= 20).map(([k, v]) => [k, +v.toFixed(0)])),
    trace: Object.fromEntries(Object.entries(sumTrace(events)).map(([k, v]) => [k, +v.toFixed(0)])),
  };
  await ctx.close();
  return result;
}

const browser = await chromium.launch();
const results = [];
for (const scenario of scenarios) {
  for (const theme of themes) {
    const r = await run(browser, theme, scenario);
    results.push(r);
    const tr = r.trace;
    console.log(
      `${scenario.padEnd(7)} ${theme.padEnd(10)} fps ${String(r.fps).padStart(5)}  med ${String(r.medianMs).padStart(5)}ms  p95 ${String(r.p95Ms).padStart(5)}ms  worst ${String(r.worstMs).padStart(6)}ms  long ${String(r.longTasks).padStart(3)} (${r.longTaskMs}ms)  busy ${r.mainBusyPct}%  script ${r.scriptMs}  style ${r.styleMs}  layout ${r.layoutMs}  paint ${tr.Paint ?? 0}  raster ${tr.RasterTask ?? 0}`,
    );
    if (args.top) console.log("        main thread top events (ms):", r.top.join(", "));
    if (args.threads) console.log("        threads(ms):", JSON.stringify(r.threads), "theme", r.themeApplied);
  }
}
await browser.close();
server.close();
if (args.json) fs.writeFileSync(args.json, JSON.stringify(results, null, 2));
