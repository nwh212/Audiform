// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/** Starfield: flying through stars that stream faster as it gets louder and flash on a beat. */
import { css } from "../palette";
import { fade, type Frame, type Preset, type Visual } from "./types";

interface Star {
  x: number;
  y: number;
  z: number;
  hue: number;
}

const COUNT = 220;

class Stars implements Visual {
  private stars: Star[] = [];
  private seed = 3;
  private flash = 0;

  private rand(): number {
    this.seed = (this.seed * 16807) % 2147483647;
    return this.seed / 2147483647;
  }

  private spawn(star?: Star): Star {
    const s = star ?? { x: 0, y: 0, z: 0, hue: 0 };
    s.x = this.rand() * 2 - 1;
    s.y = this.rand() * 2 - 1;
    s.z = 0.2 + this.rand() * 0.8;
    s.hue = this.rand();
    return s;
  }

  resize(): void {
    this.stars = [];
  }

  draw(f: Frame): void {
    const { ctx, w, h, a } = f;
    if (this.stars.length === 0) this.stars = Array.from({ length: COUNT }, () => this.spawn());
    fade(f, 0.3);
    if (a.beat) this.flash = 1;
    this.flash = Math.max(0, this.flash - a.dt * 3);
    const speed = 0.12 + a.level * 1.1 + a.bass * 0.6;
    const cx = w / 2;
    const cy = h / 2;
    const scale = Math.max(w, h) * 0.6;
    for (const s of this.stars) {
      const px = (s.x / s.z) * scale + cx;
      const py = (s.y / s.z) * scale + cy;
      s.z -= speed * a.dt;
      if (s.z <= 0.02 || px < -20 || px > w + 20 || py < -20 || py > h + 20) {
        this.spawn(s);
        s.z = 1;
        continue;
      }
      const nx = (s.x / s.z) * scale + cx;
      const ny = (s.y / s.z) * scale + cy;
      const near = 1 - s.z;
      ctx.strokeStyle = css(f.pal(s.hue + a.time * 0.04), Math.min(1, 0.25 + near + this.flash * 0.4));
      ctx.lineWidth = 0.6 + near * 2.4 + this.flash;
      ctx.beginPath();
      ctx.moveTo(px, py);
      ctx.lineTo(nx, ny);
      ctx.stroke();
    }
  }
}

export const stars: Preset = { id: "stars", name: "Starfield", create: () => new Stars() };
