// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

import type { Analysis } from "../analysis";
import type { Palette } from "../palette";

export interface Frame {
  ctx: CanvasRenderingContext2D;
  w: number;
  h: number;
  a: Analysis;
  pal: Palette;
}

export interface Visual {
  /** The drawing area changed size. */
  resize(w: number, h: number): void;
  draw(f: Frame): void;
}

export interface Preset {
  id: string;
  name: string;
  create(): Visual;
}

export const BACKDROP = "4,7,18";

/** Fades what is on the canvas toward the backdrop, leaving trails; 1 clears it. */
export function fade(f: Frame, amount: number): void {
  f.ctx.fillStyle = `rgba(${BACKDROP},${amount})`;
  f.ctx.fillRect(0, 0, f.w, f.h);
}

/** A small canvas drawn pixel by pixel, then scaled up smoothly: cheap soft pictures. */
export class Field {
  private canvas: HTMLCanvasElement | null = null;
  private img: ImageData | null = null;
  w = 0;
  h = 0;
  data: Uint8ClampedArray = new Uint8ClampedArray(0);

  fit(width: number, height: number, cell: number): void {
    const w = Math.max(8, Math.round(width / cell));
    const h = Math.max(8, Math.round(height / cell));
    if (w === this.w && h === this.h && this.canvas) return;
    this.w = w;
    this.h = h;
    this.canvas = document.createElement("canvas");
    this.canvas.width = w;
    this.canvas.height = h;
    this.img = this.canvas.getContext("2d")!.createImageData(w, h);
    this.data = this.img.data;
  }

  /** Puts the pixels on screen, stretched over the whole area. */
  show(f: Frame): void {
    if (!this.canvas || !this.img) return;
    this.canvas.getContext("2d")!.putImageData(this.img, 0, 0);
    f.ctx.imageSmoothingEnabled = true;
    f.ctx.imageSmoothingQuality = "high";
    f.ctx.drawImage(this.canvas, 0, 0, f.w, f.h);
  }
}
