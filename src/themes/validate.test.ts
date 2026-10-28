// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import example from "../../docs/examples/sunset.audtheme?raw";
import { validateTheme, validateThemeObject } from "./validate";

const PIXEL = "data:image/png;base64,iVBORw0KGgo=";

function theme(over: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    format: "audiform-theme",
    version: 1,
    id: "test-theme",
    name: "Test Theme",
    base: "dark",
    css: ':root[data-theme="test-theme"] { --accent: red; }',
    assets: {},
    ...over,
  };
}

function bad(over: Record<string, unknown>): string {
  const r = validateThemeObject(theme(over));
  if (r.ok) throw new Error("expected the theme to be refused");
  return r.error;
}

describe("theme file check", () => {
  it("accepts a small theme", () => {
    const r = validateThemeObject(theme({ description: "Red.", author: "Someone", secret: true }));
    expect(r.ok).toBe(true);
    if (r.ok) {
      expect(r.theme.id).toBe("test-theme");
      expect(r.theme.secret).toBe(true);
      expect(r.theme.description).toBe("Red.");
    }
  });

  it("explains text that is not JSON or not a theme, without throwing", () => {
    expect(validateTheme("{nope")).toEqual({ ok: false, error: expect.stringContaining("not valid JSON") });
    expect(validateTheme("[]")).toMatchObject({ ok: false });
    expect(validateTheme("null")).toMatchObject({ ok: false });
    expect(bad({ format: "something-else" })).toContain("not a theme file");
  });

  it("refuses a newer format version and bad versions", () => {
    expect(bad({ version: 2 })).toContain("newer Audiform");
    expect(bad({ version: 0 })).toContain("version");
    expect(bad({ version: "1" })).toContain("version");
  });

  it("checks the id, the name and the base", () => {
    expect(bad({ id: "Has Caps" })).toContain("id");
    expect(bad({ id: "double--dash" })).toContain("id");
    expect(bad({ id: "" })).toContain("id");
    expect(bad({ id: "dark" })).toContain("built-in");
    expect(bad({ id: "a".repeat(60) })).toContain("too long");
    expect(bad({ name: "  " })).toContain("name");
    expect(bad({ base: "blue" })).toContain("base");
    expect(bad({ secret: "yes" })).toContain("secret");
  });

  it("refuses css that could reach the network or other files", () => {
    expect(bad({ css: '@import "https://example.com/x.css";' })).toContain("@import");
    expect(bad({ css: "a { background: url(https://example.com/x.png); }" })).toContain("url(asset:name)");
    expect(bad({ css: 'a { background: url("//example.com/x.png"); }' })).toContain("url(asset:name)");
    expect(bad({ css: 'a { background: image-set("x.png" 1x); }' })).toContain("image-set");
    expect(bad({ css: "a{}</style><script>" })).toContain("style tag");
    expect(bad({ css: "a{}".repeat(300000) })).toContain("too large");
    expect(bad({ css: 5 })).toContain("css");
  });

  it("allows asset and fragment urls", () => {
    const r = validateThemeObject(theme({ css: "a { background: url(asset:bg); fill: url(#glow); }", assets: { bg: PIXEL } }));
    expect(r.ok).toBe(true);
  });

  it("needs every picture the css or a pointer uses", () => {
    expect(bad({ css: "a { background: url(asset:missing); }" })).toContain('"missing"');
    expect(bad({ assets: { bg: PIXEL }, pointers: { text: { asset: "nope", hot: [1, 1] } } })).toContain("nope");
  });

  it("checks assets", () => {
    expect(bad({ assets: { "bad name": PIXEL } })).toContain("not allowed");
    expect(bad({ assets: { x: "https://example.com/x.png" } })).toContain("data:");
    expect(bad({ assets: { x: "data:text/html;base64,PGI+" } })).toContain("data:");
    expect(bad({ assets: { x: 5 } })).toContain("data:");
    expect(bad({ assets: [] })).toContain("assets");
  });

  it("checks pointers and keeps good ones", () => {
    expect(bad({ assets: { p: PIXEL }, pointers: { teleport: { asset: "p", hot: [1, 1] } } })).toContain("pointer kind");
    expect(bad({ assets: { p: PIXEL }, pointers: { text: { asset: "p", hot: [1] } } })).toContain("hot");
    expect(bad({ assets: { p: PIXEL }, pointers: { text: { asset: "p", hot: [1.5, 2] } } })).toContain("hot");
    const r = validateThemeObject(theme({ assets: { p: PIXEL }, pointers: { ew: { asset: "p", hot: [8, 8] } } }));
    expect(r.ok && r.theme.pointers).toEqual({ ew: { asset: "p", hot: [8, 8] } });
  });

  it("checks canvas look flags and ignores unknown ones", () => {
    expect(bad({ canvasLook: { pixel: "yes" } })).toContain("pixel");
    const r = validateThemeObject(theme({ canvasLook: { pixel: true, marble: false, future: 1 } }));
    expect(r.ok && r.theme.canvasLook).toEqual({ pixel: true, marble: false });
  });
});

describe("documented example", () => {
  it("passes the check", () => {
    expect(validateTheme(example).ok).toBe(true);
  });
});
