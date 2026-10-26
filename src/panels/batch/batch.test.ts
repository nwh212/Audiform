// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { addRows, conflicts, destFor, effectiveTemplate, EMPTY_TAGS, expandTemplate, sanitizeBatch, sanitizeLevel, shapingFor, tagsFor, type BatchRow } from "./batch";

const row = (id: string, source: string): BatchRow => ({ id, source, status: "waiting", progress: 0, error: null });

describe("Batch Convert", () => {
  it("names files from a template", () => {
    const base = { template: "{name}", folder: null, kind: "wav" as const };
    expect(destFor("C:\\Calls\\1-host.flac", base)).toBe("C:\\Calls\\1-host.wav");
    expect(destFor("/home/a/ep.v2.mp3", { ...base, template: "Show_{name} (clean)", kind: "flac" })).toBe("/home/a/Show_ep.v2 (clean).flac");
    expect(destFor("C:\\in\\a.wav", { ...base, folder: "D:\\out\\", kind: "mp3" })).toBe("D:\\out\\a.mp3");
    expect(destFor("/x/a.wav", { ...base, template: "{name}a/b:c" })).toBe("/x/aa_b_c.wav");
  });

  it("fills number, date and format tokens", () => {
    const ctx = { number: 3, count: 12, date: new Date(2026, 9, 5) };
    expect(expandTemplate("Ep{number}-{name}", "intro", "mp3", ctx)).toBe("Ep03-intro");
    expect(expandTemplate("{date}_{name}.{format}", "intro", "flac", { ...ctx, number: 1 })).toBe("2026-10-05_intro.flac");
    expect(expandTemplate("{NUMBER}", "x", "wav", { number: 7, count: 120, date: ctx.date })).toBe("007");
    expect(expandTemplate("   ", "keep", "wav", ctx)).toBe("keep");
    expect(expandTemplate("{unknown}", "keep", "wav", ctx)).toBe("{unknown}");
  });

  it("finds rows that would collide or replace their source", () => {
    const rows = [row("1", "/a/ep.flac"), row("2", "/a/ep.wav"), row("3", "/a/other.wav")];
    const dests = new Map(rows.map((r) => [r.id, destFor(r.source, { template: "{name}", folder: null, kind: "wav" })]));
    const c = conflicts(rows, dests, false);
    expect(c.get("1")).toBeUndefined();
    expect(c.get("2")).toMatch(/same file as ep\.flac/);
    expect(c.get("3")).toMatch(/replace the original/);
    expect(conflicts(rows, dests, true).has("3")).toBe(false);
  });

  it("adds each file once and keeps saved settings sane", () => {
    let n = 0;
    const { rows, added } = addRows([row("x", "C:\\a\\One.wav")], ["c:/a/one.wav", "C:\\a\\Two.wav", "C:\\a\\Two.wav"], () => `n${n++}`);
    expect(added).toBe(1);
    expect(rows.map((r) => r.source)).toEqual(["C:\\a\\One.wav", "C:\\a\\Two.wav"]);
    expect(sanitizeBatch({ prefix: 5, folder: "", overwrite: "yes", format: { kind: "mp3" } })).toMatchObject({
      template: "{name}",
      folder: null,
      overwrite: false,
      format: { kind: "mp3" },
    });
    // Settings saved when the panel had Before and After boxes still work.
    expect(sanitizeBatch({ prefix: "Show_", suffix: "_v2" }).template).toBe("Show_{name}_v2");
  });

  it("fills tag fields per file and leaves blank ones out", () => {
    const ctx = { number: 3, count: 12, date: new Date(2026, 9, 18) };
    const t = tagsFor({ ...EMPTY_TAGS, title: "Ep {number}: {name}", artist: "  ", album: "Season 2" }, "C:\\a\\Pilot.wav", "mp3", ctx);
    expect(t).toEqual({ title: "Ep 03: Pilot", album: "Season 2" });
    expect(tagsFor(EMPTY_TAGS, "a.wav", "wav", ctx)).toBeNull();
    expect(sanitizeBatch({}).copyTags).toBe(true);
    expect(sanitizeBatch({ copyTags: false, tags: { title: 5, artist: "Me" } })).toMatchObject({ copyTags: false, tags: { title: "", artist: "Me" } });
  });

  it("names files by before-and-after text or a template", () => {
    expect(effectiveTemplate({ naming: "affix", prefix: "GCRN_", suffix: "_final", template: "x" })).toBe("GCRN_{name}_final");
    expect(effectiveTemplate({ naming: "template", prefix: "a", suffix: "b", template: "{number} {name}" })).toBe("{number} {name}");
    expect(sanitizeBatch({}).naming).toBe("affix");
    expect(sanitizeBatch({ template: "{date} {name}" }).naming).toBe("template");
    expect(sanitizeBatch({ naming: "affix", template: "{date} {name}", process: "Podcast Voice" })).toMatchObject({ naming: "affix", process: "Podcast Voice" });
  });
  it("keeps level settings in range and sends them only when on", () => {
    expect(sanitizeLevel({ normalize: true, normalizeDb: 6, fadeInMs: -5, fadeOutMs: "x" })).toEqual({ normalize: true, normalizeDb: 0, fadeInMs: 0, fadeOutMs: 0 });
    expect(sanitizeBatch({}).level.normalize).toBe(false);
    expect(shapingFor(sanitizeLevel({}))).toBeNull();
    expect(shapingFor({ normalize: false, normalizeDb: -1, fadeInMs: 0, fadeOutMs: 500 })).toEqual({ normalizeDb: null, fadeInMs: 0, fadeOutMs: 500 });
    expect(shapingFor({ normalize: true, normalizeDb: -3, fadeInMs: 0, fadeOutMs: 0 })?.normalizeDb).toBe(-3);
  });
});
