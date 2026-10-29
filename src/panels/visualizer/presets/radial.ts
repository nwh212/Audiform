// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/** Radial: the spectrum in a ring that turns slowly and swells on the bass. */
import { css } from "../palette";
import { fade, type Frame, type Preset, type Visual } from "./types";

class Radial implements Visual {
  private angle = 0;

  resize(): void {
    this.angle = 0;
  }

  draw(f: Frame): void {
    const { ctx, w, h, a } = f;
    fade(f, 0.25);
    this.angle += a.dt * (0.15 + a.mid * 0.8);
    const cx = w / 2;
    const cy = h / 2;
    const base = Math.min(w, h) * (0.18 + a.bass * 0.06);
    const reach = Math.min(w, h) * 0.3;
    const n = a.levels.length;
    ctx.lineCap = "round";
    for (let mirror = 0; mirror < 2; mirror++) {
      for (let i = 0; i < n; i++) {
        const t = i / n;
        const ang = this.angle + (mirror === 0 ? t : -t) * Math.PI + (mirror === 0 ? 0 : Math.PI);
        const len = (a.levels[i] ?? 0) * reach + 2;
        const c = Math.cos(ang);
        const s = Math.sin(ang);
        ctx.strokeStyle = css(f.pal(t + a.time * 0.04), 0.9);
        ctx.lineWidth = Math.max(2, (Math.PI * 2 * base) / n / 1.6);
        ctx.beginPath();
        ctx.moveTo(cx + c * base, cy + s * base);
        ctx.lineTo(cx + c * (base + len), cy + s * (base + len));
        ctx.stroke();
      }
    }
    ctx.beginPath();
    ctx.arc(cx, cy, base * (0.55 + a.bass * 0.4), 0, Math.PI * 2);
    ctx.fillStyle = css(f.pal(a.time * 0.05 + 0.5), 0.35 + a.bass * 0.5);
    ctx.fill();
  }
}

export const radial: Preset = { id: "radial", name: "Radial", create: () => new Radial() };
