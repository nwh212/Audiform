// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/** Bars: the spectrum as columns with a falling cap on each. */
import { css } from "../palette";
import { BACKDROP, type Frame, type Preset, type Visual } from "./types";

class Bars implements Visual {
  private caps: number[] = [];
  private speed: number[] = [];

  resize(): void {
    this.caps = [];
    this.speed = [];
  }

  draw(f: Frame): void {
    const { ctx, w, h, a } = f;
    ctx.fillStyle = `rgb(${BACKDROP})`;
    ctx.fillRect(0, 0, w, h);
    const n = Math.max(12, Math.min(64, Math.floor(w / 9)));
    const gap = Math.max(1, Math.floor(w / n / 6));
    const bw = w / n;
    for (let i = 0; i < n; i++) {
      const from = Math.floor((i * a.levels.length) / n);
      const to = Math.max(from + 1, Math.floor(((i + 1) * a.levels.length) / n));
      let v = 0;
      for (let k = from; k < to; k++) v = Math.max(v, a.levels[k] ?? 0);
      const bh = v * (h - 6);
      const x = i * bw;
      const grad = ctx.createLinearGradient(0, h, 0, h - Math.max(2, bh));
      grad.addColorStop(0, css(f.pal(0.0)));
      grad.addColorStop(1, css(f.pal(0.35 + v * 0.65)));
      ctx.fillStyle = grad;
      ctx.fillRect(x + gap / 2, h - bh, bw - gap, bh);
      const cap = this.caps[i] ?? 0;
      let sp = this.speed[i] ?? 0;
      if (bh >= cap) {
        this.caps[i] = bh;
        sp = 0;
      } else {
        sp += a.dt * 260;
        this.caps[i] = Math.max(bh, cap - sp * a.dt);
      }
      this.speed[i] = sp;
      ctx.fillStyle = css(f.pal(0.9));
      ctx.fillRect(x + gap / 2, h - (this.caps[i] ?? 0) - 4, bw - gap, 2);
    }
  }
}

export const bars: Preset = { id: "bars", name: "Bars", create: () => new Bars() };
