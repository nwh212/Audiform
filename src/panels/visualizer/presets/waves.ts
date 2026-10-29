// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/** Waves: a glowing line that ripples with the sound, with a mirrored echo. */
import { css } from "../palette";
import { fade, type Frame, type Preset, type Visual } from "./types";

class Waves implements Visual {
  private phase = 0;

  resize(): void {
    this.phase = 0;
  }

  draw(f: Frame): void {
    const { ctx, w, h, a } = f;
    fade(f, 0.22);
    this.phase += a.dt * (1.5 + a.level * 6);
    const mid = h / 2;
    const steps = Math.max(40, Math.floor(w / 3));
    for (let pass = 0; pass < 3; pass++) {
      ctx.beginPath();
      for (let s = 0; s <= steps; s++) {
        const x = s / steps;
        let y = 0;
        // A few of the spectrum's bands, each a sine of its own pitch: it moves like a scope trace.
        for (let k = 0; k < 8; k++) {
          const amp = a.levels[k * 7 + pass] ?? 0;
          y += amp * Math.sin(x * Math.PI * 2 * (k + 1) * (1 + pass * 0.5) + this.phase * (0.7 + k * 0.3) + pass);
        }
        const env = Math.sin(x * Math.PI);
        const py = mid + (y / 4) * mid * env * (0.35 + 0.65 * (pass === 0 ? 1 : 0.7));
        if (s === 0) ctx.moveTo(x * w, py);
        else ctx.lineTo(x * w, py);
      }
      ctx.lineWidth = pass === 0 ? 3 : 1.5;
      ctx.strokeStyle = css(f.pal(a.time * 0.05 + pass * 0.18), pass === 0 ? 0.95 : 0.55);
      ctx.shadowColor = css(f.pal(a.time * 0.05 + pass * 0.18));
      ctx.shadowBlur = pass === 0 ? 14 : 6;
      ctx.stroke();
    }
    ctx.shadowBlur = 0;
  }
}

export const waves: Preset = { id: "waves", name: "Waves", create: () => new Waves() };
