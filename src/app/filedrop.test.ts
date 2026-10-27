// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { dropScale, FileDropRouter, isSessionFile } from "./filedrop";

function box(left: number, top: number, width: number, height: number): HTMLElement {
  return {
    getBoundingClientRect: () => ({ left, top, width, height, right: left + width, bottom: top + height }),
  } as unknown as HTMLElement;
}

describe("FileDropRouter", () => {
  it("drops on the panel under the pointer and highlights while hovering", () => {
    const fallback: string[][] = [];
    const dropped: string[][] = [];
    const hover: boolean[] = [];
    const router = new FileDropRouter((p) => fallback.push(p));
    router.register({ element: box(0, 0, 250, 400), onDrop: (p) => dropped.push(p), onHover: (o) => hover.push(o) });

    router.handle({ type: "enter", paths: ["a.wav"], x: 600, y: 10 });
    router.handle({ type: "over", x: 100, y: 100 });
    router.handle({ type: "over", x: 120, y: 110 });
    router.handle({ type: "drop", paths: ["a.wav"], x: 120, y: 110 });

    expect(dropped).toEqual([["a.wav"]]);
    expect(fallback).toEqual([]);
    expect(hover).toEqual([true, false]);
  });

  it("tells the panel under the files where they are, with the files, so it can preview the drop", () => {
    const moves: (string | null)[] = [];
    const router = new FileDropRouter(() => {});
    router.register({ element: box(0, 0, 250, 400), onDrop: () => {}, onMove: (p, at) => moves.push(at ? `${p.join()}@${at.x},${at.y}` : null) });
    router.handle({ type: "enter", paths: ["a.wav", "b.wav"], x: 10, y: 10 });
    router.handle({ type: "over", x: 20, y: 30 });
    router.handle({ type: "over", x: 600, y: 30 });
    expect(moves).toEqual(["a.wav,b.wav@10,10", "a.wav,b.wav@20,30", null]);
    // In-app drags pass their files along too.
    router.dragOver(5, 6, undefined, ["c.wav"]);
    router.dragCancel();
    expect(moves.slice(3)).toEqual(["c.wav@5,6", null]);
  });

  it("sends drops outside every panel to the fallback", () => {
    const fallback: string[][] = [];
    const router = new FileDropRouter((p) => fallback.push(p));
    router.register({ element: box(0, 0, 250, 400), onDrop: () => {} });
    router.handle({ type: "drop", paths: ["b.mp3"], x: 800, y: 300 });
    expect(fallback).toEqual([["b.mp3"]]);
  });

  it("ignores hidden panels and clears the highlight on leave or unregister", () => {
    const hover: boolean[] = [];
    const router = new FileDropRouter(() => {});
    router.register({ element: box(0, 0, 0, 0), onDrop: () => {}, onHover: () => hover.push(true) });
    const off = router.register({ element: box(0, 0, 100, 100), onDrop: () => {}, onHover: (o) => hover.push(o) });
    router.handle({ type: "over", x: 10, y: 10 });
    router.handle({ type: "leave" });
    router.handle({ type: "over", x: 10, y: 10 });
    off();
    expect(hover).toEqual([true, false, true, false]);
  });
});

describe("FileDropRouter in-app drags", () => {
  it("delivers to the panel under the pointer and never falls back", () => {
    const fallback: string[][] = [];
    const dropped: string[][] = [];
    const router = new FileDropRouter((p) => fallback.push(p));
    router.register({ element: box(0, 0, 250, 400), onDrop: (p) => dropped.push(p) });
    expect(router.dragEnd(["x.wav"], 900, 900)).toBe(false);
    expect(router.dragEnd(["y.wav"], 10, 10)).toBe(true);
    expect(dropped).toEqual([["y.wav"]]);
    expect(fallback).toEqual([]);
  });
});

describe("session files", () => {
  it("opens a dropped session wherever it lands", () => {
    const opened: string[][] = [];
    const dropped: string[][] = [];
    const router = new FileDropRouter(() => {}, (p) => opened.push(p));
    router.register({ element: box(0, 0, 250, 400), onDrop: (p) => dropped.push(p) });
    router.handle({ type: "drop", paths: ["C:\\Ep 1.SESX", "a.wav"], x: 10, y: 10 });
    router.handle({ type: "drop", paths: ["b.wav"], x: 10, y: 10 });
    expect(opened).toEqual([["C:\\Ep 1.SESX", "a.wav"]]);
    expect(dropped).toEqual([["b.wav"]]);
    expect(isSessionFile("x.altaudio")).toBe(true);
    expect(isSessionFile("x.audfrm")).toBe(true);
    expect(isSessionFile("x.wav")).toBe(false);
  });
});

describe("dropScale", () => {
  it("uses the measured ratio, not devicePixelRatio, when they disagree", () => {
    expect(dropScale(2400, 1600, 1.25)).toBe(1.5);
    expect(dropScale(1600, 1600, 2)).toBe(1);
  });

  it("falls back to devicePixelRatio when the size is unknown or odd", () => {
    expect(dropScale(null, 1600, 1.25)).toBe(1.25);
    expect(dropScale(1, 1600, 1.5)).toBe(1.5);
    expect(dropScale(null, 0, 0)).toBe(1);
  });
});

