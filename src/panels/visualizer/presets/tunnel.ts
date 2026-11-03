// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Tunnel: shapes come at you out of a point and fly past, faster as it
 * gets louder. The beat throws a new one in; the treble spins them; the
 * bass bends the path they come down.
 */
import { css } from "../palette";
import { fade, type Frame, type Preset, type Visual } from "./types";

interface Shape {
  /** Depth: 1 far away, 0 at the viewer. */
  z: number;
  sides: number;
  spin: number;
  hue: number;
  /** Offset of the vanishing point this shape is drawn around, so the tunnel bends. */
  ox: number;
  oy: number;
}

const COUNT = 18;

class Tunnel implements Visual {
  private shapes: Shape[] = [];
  private seed = 11;
  private angle = 0;
  private bend = { x: 0, y: 0 };
  private flash = 0;

  private rand(): number {
    this.seed = (this.seed * 16807) % 2147483647;
    return this.seed / 2147483647;
  }

  private spawn(z: number): Shape {
    return { z, sides: 3 + Math.floor(this.rand() * 5), spin: (this.rand() - 0.5) * 2, hue: this.rand(), ox: this.bend.x, oy: this.bend.y };
  }

  resize(): void {
    this.shapes = [];
  }

  draw(f: Frame): void {
    const { ctx, w, h, a } = f;
    if (this.shapes.length === 0) this.shapes = Array.from({ length: COUNT }, (_, i) => this.spawn((i + 1) / COUNT));
    fade(f, 0.32);
    this.angle += a.dt * (0.2 + a.treble * 1.6);
    // The path bends slowly on its own and harder with the bass.
    this.bend.x = Math.sin(a.time * 0.37) * 0.25 * (1 + a.bass);
    this.bend.y = Math.cos(a.time * 0.29) * 0.18 * (1 + a.bass);
    if (a.beat) this.flash = 1;
    this.flash = Math.max(0, this.flash - a.dt * 4);
    const speed = 0.25 + a.level * 1.4 + a.bass * 0.8;
    const cx = w / 2;
    const cy = h / 2;
    const reach = Math.max(w, h) * 0.9;
    // Far shapes first, so the near ones draw over them.
    this.shapes.sort((p, q) => q.z - p.z);
    for (const s of this.shapes) {
      s.z -= speed * a.dt * 0.35;
      if (s.z <= 0.02) {
        Object.assign(s, this.spawn(1));
        continue;
      }
      const near = 1 - s.z;
      const r = (reach * near * near) / (0.15 + near) + 6;
      const x = cx + (s.ox * (1 - near) + this.bend.x * near) * w * 0.3;
      const y = cy + (s.oy * (1 - near) + this.bend.y * near) * h * 0.3;
      const rot = this.angle * s.spin + near * 2;
      ctx.beginPath();
      for (let k = 0; k <= s.sides; k++) {
        const t = rot + (k / s.sides) * Math.PI * 2;
        const px = x + Math.cos(t) * r;
        const py = y + Math.sin(t) * r * (0.75 + 0.25 * Math.sin(a.time + s.hue * 6));
        if (k === 0) ctx.moveTo(px, py);
        else ctx.lineTo(px, py);
      }
      const bright = Math.min(1, 0.2 + near * 1.1 + this.flash * 0.5);
      ctx.strokeStyle = css(f.pal(s.hue + a.time * 0.05 + near * 0.15), bright);
      ctx.lineWidth = 1 + near * 5 + this.flash * 2;
      ctx.stroke();
      if (near > 0.6) {
        ctx.fillStyle = css(f.pal(s.hue + 0.5), (near - 0.6) * 0.12);
        ctx.fill();
      }
    }
    // The point everything comes from, glowing with the level.
    const g = ctx.createRadialGradient(cx + this.bend.x * w * 0.3, cy + this.bend.y * h * 0.3, 0, cx + this.bend.x * w * 0.3, cy + this.bend.y * h * 0.3, 40 + a.level * 80);
    g.addColorStop(0, css(f.pal(a.time * 0.05), 0.5 + a.level * 0.5));
    g.addColorStop(1, css(f.pal(a.time * 0.05), 0));
    ctx.fillStyle = g;
    ctx.fillRect(0, 0, w, h);
  }
}

export const tunnel: Preset = { id: "tunnel", name: "Tunnel", create: () => new Tunnel() };
