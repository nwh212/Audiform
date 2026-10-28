// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { chapterUrl, chaptersOf, clock, embedList, isChapterUrl, render, titleTips, warnings } from "./chapters";
import { newMarker } from "./markers";

const R = 48_000;
const marks = [
  { ...newMarker("b", "Main topic", 95 * R, 0, "chapter") },
  { ...newMarker("a", "Intro", 0, 0, "chapter") },
  { ...newMarker("c", "Breath here", 30 * R, 0, "cue") },
  { ...newMarker("d", 'Ad, "midroll"', 3_725 * R, 60 * R, "chapter") },
];

describe("chapter export", () => {
  it("formats clock times like video sites expect", () => {
    expect(clock(0)).toBe("0:00");
    expect(clock(245.9)).toBe("4:05");
    expect(clock(3_723)).toBe("1:02:03");
  });
  it("keeps chapter markers in time order, or every marker", () => {
    expect(chaptersOf(marks, R, "chapters").map((c) => c.title)).toEqual(["Intro", "Main topic", 'Ad, "midroll"']);
    expect(chaptersOf(marks, R, "all")).toHaveLength(4);
  });
  it("writes a timestamp list", () => {
    expect(render(chaptersOf(marks, R, "chapters"), "timestamps")).toBe('0:00 Intro\n1:35 Main topic\n1:02:05 Ad, "midroll"\n');
  });
  it("writes CSV with quoting", () => {
    const csv = render(chaptersOf(marks, R, "chapters"), "csv").split("\r\n");
    expect(csv[0]).toBe("Name,Start,End,Duration,Type,Description");
    expect(csv[3]).toBe('"Ad, ""midroll""",01:02:05.000,01:03:05.000,00:01:00.000,chapter,');
  });
  it("writes podcast chapters JSON", () => {
    const j = JSON.parse(render(chaptersOf(marks, R, "chapters"), "podcast-json"));
    expect(j.version).toBe("1.2.0");
    expect(j.chapters[0]).toEqual({ startTime: 0, title: "Intro" });
    expect(j.chapters[2]).toEqual({ startTime: 3725, endTime: 3785, title: 'Ad, "midroll"' });
  });
  it("warns about lists video sites reject", () => {
    const late = chaptersOf([newMarker("x", "Late", 5 * R, 0, "chapter")], R, "chapters");
    const w = warnings(late, "timestamps");
    expect(w.some((s) => s.includes("0:00"))).toBe(true);
    expect(w.some((s) => s.includes("three"))).toBe(true);
    expect(warnings(chaptersOf(marks, R, "chapters"), "podcast-json")).toEqual([]);
  });
});

describe("chapters inside an export", () => {
  it("keeps chapter markers in the range and starts them at 0", () => {
    const list = embedList(marks, R, [60 * R, 4_000 * R]);
    expect(list.map((c) => [c.title, c.startMs])).toEqual([
      ["Main topic", 35_000],
      ['Ad, "midroll"', 3_665_000],
    ]);
    expect(embedList(marks, R, null)[0]).toEqual({ title: "Intro", startMs: 0, endMs: null, url: null });
  });
});

describe("chapter links and title tips", () => {
  it("reads a link and accepts only web links", () => {
    expect(chapterUrl({ chapter: null })).toBe("");
    expect(chapterUrl({ chapter: { url: "https://a.example/x", image: null } })).toBe("https://a.example/x");
    expect(isChapterUrl(" https://a.example/x ")).toBe(true);
    expect(isChapterUrl("ftp://a.example")).toBe(false);
    expect(isChapterUrl("https://a b")).toBe(false);
  });

  it("carries the link into the embed list", () => {
    const m = { id: "1", name: "Intro", kind: "chapter" as const, position: 0, duration: 0, description: "", chapter: { url: "https://a.example", image: null }, contracted_duration: null };
    expect(embedList([m], 48_000, null)[0]?.url).toBe("https://a.example");
    expect(embedList([{ ...m, chapter: null }], 48_000, null)[0]?.url).toBeNull();
  });

  it("flags long, wordy and repetitive titles and leaves good ones alone", () => {
    expect(titleTips("Intro")).toEqual([]);
    expect(titleTips("")).toEqual([]);
    expect(titleTips("A very long chapter title that goes on and on and on")).toHaveLength(2);
    expect(titleTips("Movie review: movie night")).toEqual(['"movie" appears twice.']);
  });
});
