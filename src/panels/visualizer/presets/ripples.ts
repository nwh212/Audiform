// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/** Ripples: rings spread from a point on each beat, like drops on water. */
import { css } from "../palette";
import { fade, type Frame, type Preset, type Visual } from "./types";

interface Ring {
  x: number;
  y: number;
  r: number;
  speed: number;
  life: number;
  hue: number;
  width: number;
}

class Ripples implements Visual {
  private rings: Ring[] = [];
  private seed = 7;
  private since = 0;

  private rand(): number {
    this.seed = (this.seed * 16807) % 2147483647;
    return this.seed / 2147483647;
  }

  resize(): void {
    this.rings = [];
  }

  private drop(w: number, h: number, strength: number, hue: number): void {
    this.rings.push({
      x: w * (0.15 + this.rand() * 0.7),
      y: h * (0.15 + this.rand() * 0.7),
      r: 2,
      speed: Math.min(w, h) * (0.25 + strength * 0.5),
      life: 1,
      hue,
      width: 1.5 + strength * 4,
    });
    if (this.rings.length > 40) this.rings.shift();
  }

  draw(f: Frame): void {
    const { ctx, w, h, a } = f;
    fade(f, 0.18);
    this.since += a.dt;
    if (a.beat) this.drop(w, h, a.bass, this.rand());
    else if (a.treble > 0.3 && this.since > 0.35) {
      this.since = 0;
      this.drop(w, h, a.treble * 0.5, this.rand());
    } else if (a.level < 0.02 && this.since > 2.5) {
      // Quiet: a slow drop now and then keeps it alive.
      this.since = 0;
      this.drop(w, h, 0.1, this.rand());
    }
    for (const ring of this.rings) {
      ring.r += ring.speed * a.dt;
      ring.life -= a.dt * 0.45;
      if (ring.life <= 0) continue;
      for (let k = 0; k < 3; k++) {
        ctx.beginPath();
        ctx.arc(ring.x, ring.y, ring.r - k * ring.width * 1.6, 0, Math.PI * 2);
        ctx.lineWidth = Math.max(0.5, ring.width * (1 - k * 0.25));
        ctx.strokeStyle = css(f.pal(ring.hue + k * 0.05), Math.max(0, ring.life) * (1 - k * 0.3));
        if (ring.r - k * ring.width * 1.6 > 0) ctx.stroke();
      }
    }
    this.rings = this.rings.filter((r) => r.life > 0);
  }
}

export const ripples: Preset = { id: "ripples", name: "Ripples", create: () => new Ripples() };
