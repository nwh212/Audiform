// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Lava Lamp: warm blobs of wax that heat at the bottom, rise, cool at the
 * top and sink, merging and parting like the real thing. The bass pushes
 * them up and swells them; the colors drift and follow the music.
 */
import { Field, type Frame, type Preset, type Visual } from "./types";

interface Blob {
  x: number;
  y: number;
  r: number;
  vy: number;
  heat: number;
  hue: number;
  phase: number;
}

const COUNT = 10;

function smooth(a: number, b: number, x: number): number {
  const t = Math.max(0, Math.min(1, (x - a) / (b - a)));
  return t * t * (3 - 2 * t);
}

class Lava implements Visual {
  private field = new Field();
  private blobs: Blob[] = [];
  private seed = 1;

  private rand(): number {
    this.seed = (this.seed * 16807) % 2147483647;
    return this.seed / 2147483647;
  }

  private make(): void {
    this.blobs = Array.from({ length: COUNT }, (_, i) => ({
      x: 0.2 + this.rand() * 0.6,
      y: this.rand(),
      r: 0.1 + this.rand() * 0.07,
      vy: 0,
      heat: this.rand(),
      hue: i / COUNT,
      phase: this.rand() * 6.28,
    }));
  }

  resize(): void {
    // The field is sized when drawn.
  }

  draw(f: Frame): void {
    if (this.blobs.length === 0) this.make();
    const { a } = f;
    const dt = Math.min(0.1, a.dt);
    // Heat from the bottom, cooling at the top; the music adds warmth and, on a beat, a push.
    for (const b of this.blobs) {
      const bottom = smooth(0.7, 1, b.y);
      const top = smooth(0.3, 0, b.y);
      b.heat += (bottom * (0.55 + a.level) - top * 0.5 - 0.06) * dt * 0.9;
      b.heat = Math.max(0, Math.min(1, b.heat));
      const lift = (b.heat - 0.5) * 0.22 + a.bass * 0.12;
      b.vy += (-lift - b.vy) * Math.min(1, dt * 1.6);
      if (a.beat) b.vy -= 0.1 + this.rand() * 0.12;
      b.y += b.vy * dt;
      if (b.y < -0.05) {
        b.y = -0.05;
        b.vy = Math.abs(b.vy) * 0.3;
      }
      if (b.y > 1.05) {
        b.y = 1.05;
        b.vy = -Math.abs(b.vy) * 0.3;
      }
      b.x += Math.sin(a.time * 0.4 + b.phase) * 0.03 * dt;
      b.x = Math.max(0.12, Math.min(0.88, b.x));
    }
    const fld = this.field;
    fld.fit(f.w, f.h, 6);
    const W = fld.w;
    const H = fld.h;
    const aspect = f.w / f.h;
    const sw = 1 + a.bass * 0.35;
    const d = fld.data;
    for (let py = 0; py < H; py++) {
      const y = py / H;
      for (let px = 0; px < W; px++) {
        const x = px / W;
        let sum = 0;
        let hue = 0;
        let wsum = 0;
        for (const b of this.blobs) {
          const dx = (x - b.x) * aspect;
          const dy = y - b.y;
          const r = b.r * sw * (0.8 + b.heat * 0.5);
          const v = (r * r) / (dx * dx + dy * dy + 1e-4);
          sum += v;
          hue += v * b.hue;
          wsum += v;
        }
        const c = f.pal(((wsum > 0 ? hue / wsum : 0) + a.time * 0.02 + a.mid * 0.15) % 1);
        const body = smooth(0.85, 1.25, sum);
        const glow = smooth(0.25, 0.95, sum) * 0.55;
        // Lit from below: the glass glows warm at the bottom, dark wax shows against it.
        const lamp = 0.08 + 0.2 * y;
        const rim = smooth(1.25, 0.9, sum) * body;
        const m = Math.min(1, body * 0.85 + glow * 0.5);
        const i = (py * W + px) * 4;
        d[i] = Math.min(255, 14 + lamp * 90 + (c[0] * m + 90 * rim) * (0.6 + 0.5 * a.level + 0.3 * y));
        d[i + 1] = Math.min(255, 8 + lamp * 45 + (c[1] * m + 60 * rim) * (0.6 + 0.5 * a.level + 0.3 * y));
        d[i + 2] = Math.min(255, 20 + lamp * 20 + c[2] * m * (0.6 + 0.5 * a.level + 0.3 * y));
        d[i + 3] = 255;
      }
    }
    fld.show(f);
  }
}

export const lava: Preset = { id: "lava", name: "Lava Lamp", create: () => new Lava() };
