// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Electric: arcs of lightning jump between points that drift around the
 * screen, forking as they go. The treble makes them jump more often and
 * jitter harder; the bass makes them thick and bright; a beat fires a
 * big one across the whole picture.
 */
import { css } from "../palette";
import { fade, type Frame, type Preset, type Visual } from "./types";

interface Node {
  x: number;
  y: number;
  vx: number;
  vy: number;
}

interface Arc {
  from: number;
  to: number;
  life: number;
  width: number;
  hue: number;
  /** A fixed wobble per arc so it does not shimmer every picture. */
  seed: number;
}

const NODES = 7;

class Electric implements Visual {
  private nodes: Node[] = [];
  private arcs: Arc[] = [];
  private seed = 5;
  private since = 0;

  private rand(): number {
    this.seed = (this.seed * 16807) % 2147483647;
    return this.seed / 2147483647;
  }

  resize(): void {
    this.nodes = [];
    this.arcs = [];
  }

  private strike(strength: number, hue: number, from?: number, to?: number): void {
    const a = from ?? Math.floor(this.rand() * NODES);
    let b = to ?? Math.floor(this.rand() * NODES);
    if (b === a) b = (a + 1) % NODES;
    this.arcs.push({ from: a, to: b, life: 1, width: 1 + strength * 4, hue, seed: this.rand() * 1000 });
    if (this.arcs.length > 24) this.arcs.shift();
  }

  /** A jagged line from one point to another, with a few forks off it. */
  private bolt(ctx: CanvasRenderingContext2D, x0: number, y0: number, x1: number, y1: number, jitter: number, seed: number, depth: number): void {
    const steps = 14;
    const dx = x1 - x0;
    const dy = y1 - y0;
    const len = Math.hypot(dx, dy) || 1;
    const nx = -dy / len;
    const ny = dx / len;
    ctx.moveTo(x0, y0);
    let px = x0;
    let py = y0;
    for (let i = 1; i < steps; i++) {
      const t = i / steps;
      // A fixed wobble per arc and step, biggest in the middle.
      const s = Math.sin(seed + i * 12.9898) * 43758.5453;
      const off = (s - Math.floor(s) - 0.5) * jitter * Math.sin(t * Math.PI) * len * 0.25;
      const x = x0 + dx * t + nx * off;
      const y = y0 + dy * t + ny * off;
      ctx.lineTo(x, y);
      if (depth > 0 && i % 5 === 3) {
        // A fork: a shorter bolt off to the side that fades out.
        const fx = x + (nx * off * 2 + dx * 0.15) * (depth === 2 ? 1 : -1);
        const fy = y + (ny * off * 2 + dy * 0.15) * (depth === 2 ? 1 : -1);
        this.bolt(ctx, x, y, fx, fy, jitter * 1.4, seed + i, depth - 1);
        ctx.moveTo(x, y);
      }
      px = x;
      py = y;
    }
    ctx.lineTo(x1, y1);
    void px;
    void py;
  }

  draw(f: Frame): void {
    const { ctx, w, h, a } = f;
    if (this.nodes.length === 0) this.nodes = Array.from({ length: NODES }, () => ({ x: w * (0.1 + this.rand() * 0.8), y: h * (0.1 + this.rand() * 0.8), vx: (this.rand() - 0.5) * 30, vy: (this.rand() - 0.5) * 30 }));
    fade(f, 0.28);
    // The points drift and stay on screen.
    for (const n of this.nodes) {
      n.x += n.vx * a.dt * (1 + a.mid);
      n.y += n.vy * a.dt * (1 + a.mid);
      if (n.x < w * 0.05 || n.x > w * 0.95) n.vx *= -1;
      if (n.y < h * 0.05 || n.y > h * 0.95) n.vy *= -1;
    }
    this.since += a.dt;
    const rate = 0.9 - a.treble * 0.7;
    if (a.beat) {
      // A big one straight across, and a couple of small ones.
      this.strike(0.8 + a.bass, this.rand());
      this.strike(a.bass * 0.6, this.rand());
      this.since = 0;
    } else if (this.since > rate && a.level > 0.03) {
      this.since = 0;
      this.strike(a.treble * 0.8 + a.level * 0.3, this.rand());
    } else if (a.level <= 0.03 && this.since > 2) {
      this.since = 0;
      this.strike(0.15, this.rand());
    }
    const jitter = 0.5 + a.treble * 1.5;
    ctx.lineCap = "round";
    ctx.lineJoin = "round";
    for (const arc of this.arcs) {
      arc.life -= a.dt * 2.2;
      if (arc.life <= 0) continue;
      const p = this.nodes[arc.from]!;
      const q = this.nodes[arc.to]!;
      // A wide soft glow, then the bright core.
      for (const [width, alpha] of [
        [arc.width * 5, 0.12],
        [arc.width * 2, 0.35],
        [arc.width * 0.8, 1],
      ] as const) {
        ctx.beginPath();
        this.bolt(ctx, p.x, p.y, q.x, q.y, jitter, arc.seed, 2);
        ctx.lineWidth = Math.max(0.5, width * (0.5 + 0.5 * arc.life));
        const c = width === arc.width * 0.8 ? [245, 250, 255] : f.pal(arc.hue + a.time * 0.03);
        ctx.strokeStyle = css(c as [number, number, number], alpha * arc.life);
        ctx.stroke();
      }
    }
    this.arcs = this.arcs.filter((x) => x.life > 0);
    // The points themselves: small orbs that brighten with the bass.
    for (const n of this.nodes) {
      ctx.beginPath();
      ctx.arc(n.x, n.y, 2 + a.bass * 5, 0, Math.PI * 2);
      ctx.fillStyle = css(f.pal(a.time * 0.05 + n.x / w), 0.5 + a.bass * 0.5);
      ctx.fill();
    }
  }
}

export const electric: Preset = { id: "electric", name: "Electric", create: () => new Electric() };
