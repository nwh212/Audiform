// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/** Plasma: drifting color clouds that churn faster and stretch with the bass. */
import { Field, type Frame, type Preset, type Visual } from "./types";

class Plasma implements Visual {
  private field = new Field();
  private t = 0;

  resize(): void {
    // Sized when drawn.
  }

  draw(f: Frame): void {
    const { a } = f;
    this.t += a.dt * (0.4 + a.level * 2.4);
    const fld = this.field;
    fld.fit(f.w, f.h, 7);
    const W = fld.w;
    const H = fld.h;
    const d = fld.data;
    const zoom = 3 + a.bass * 2.2;
    const t = this.t;
    for (let py = 0; py < H; py++) {
      const y = (py / H) * zoom;
      for (let px = 0; px < W; px++) {
        const x = (px / W) * zoom * (f.w / f.h);
        const v =
          Math.sin(x * 2.1 + t) +
          Math.sin(y * 2.7 - t * 1.3) +
          Math.sin((x + y) * 1.7 + t * 0.7) +
          Math.sin(Math.hypot(x - 2 + Math.sin(t * 0.5), y - 1.5 + Math.cos(t * 0.4)) * 3.1 - t * 1.1);
        const c = f.pal((v + 4) / 8 + a.time * 0.03 + a.mid * 0.2);
        const k = 0.45 + 0.75 * Math.min(1, a.level * 1.3 + 0.15) * (0.6 + 0.4 * Math.sin(v * 1.2));
        const i = (py * W + px) * 4;
        d[i] = c[0] * k;
        d[i + 1] = c[1] * k;
        d[i + 2] = c[2] * k;
        d[i + 3] = 255;
      }
    }
    fld.show(f);
  }
}

export const plasma: Preset = { id: "plasma", name: "Plasma", create: () => new Plasma() };
