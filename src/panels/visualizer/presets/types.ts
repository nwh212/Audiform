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
  /** How fine the soft pictures are drawn: 1 standard, 2 twice as fine each way (four times the work), 0.5 coarser. */
  detail: number;
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

/** The Detail choices, as the panel lists them. */
export const DETAILS: { value: number; label: string }[] = [
  { value: 0.5, label: "Soft" },
  { value: 1, label: "Standard" },
  { value: 2, label: "Fine" },
  { value: 3, label: "Finest" },
];

/** The field's size in cells for a drawing area, a cell size and a detail. */
export function fieldSize(width: number, height: number, cell: number, detail = 1): [number, number] {
  const size = Math.max(1, cell / Math.max(0.25, detail));
  return [Math.max(8, Math.round(width / size)), Math.max(8, Math.round(height / size))];
}

/** A small canvas drawn pixel by pixel, then scaled up smoothly: cheap soft pictures. */
export class Field {
  private canvas: HTMLCanvasElement | null = null;
  private img: ImageData | null = null;
  w = 0;
  h = 0;
  data: Uint8ClampedArray = new Uint8ClampedArray(0);

  /** Sizes the field: one cell of `cell` screen pixels per field pixel at standard detail, smaller cells at finer detail. */
  fit(width: number, height: number, cell: number, detail = 1): void {
    const [w, h] = fieldSize(width, height, cell, detail);
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
