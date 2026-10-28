// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import type { MarkerDoc } from "../stores/session";
import { EMPTY_EPISODE, episodeProblems, packageFiles, slugOf } from "./publish";

const m = (name: string, kind: MarkerDoc["kind"], position: number, duration = 0): MarkerDoc => ({ id: name, name, kind, position, duration, description: "", chapter: null, contracted_duration: null });

describe("slugOf", () => {
  it("makes a safe folder name", () => {
    expect(slugOf("Episode 12: Café & Co.")).toBe("Episode-12-Cafe-Co");
    expect(slugOf("???")).toBe("episode");
    expect(slugOf("a".repeat(200)).length).toBeLessThanOrEqual(80);
  });
});

describe("episodeProblems", () => {
  it("wants a title and respects host limits", () => {
    expect(episodeProblems(EMPTY_EPISODE)).toEqual(["The episode needs a title."]);
    expect(episodeProblems({ ...EMPTY_EPISODE, title: "x".repeat(251) })).toHaveLength(1);
    expect(episodeProblems({ ...EMPTY_EPISODE, title: "ok", description: "x".repeat(4001) })).toHaveLength(1);
    expect(episodeProblems({ ...EMPTY_EPISODE, title: "ok", season: 1.5 })).toHaveLength(1);
    expect(episodeProblems({ ...EMPTY_EPISODE, title: "ok", season: 2, episode: 0 })).toEqual([]);
  });
});

describe("packageFiles", () => {
  const rate = 48_000;
  const markers = [m("Wrap", "chapter", 90 * rate), m("Intro", "chapter", 0), m("Break", "ad_slot", 30 * rate), m("Funny bit", "social_clip", 40 * rate, 15 * rate), m("note", "cue", 5)];
  const files = packageFiles({ ...EMPTY_EPISODE, title: " Ep 1 ", description: "About it.", season: 2, episode: 5, explicit: true }, "Ep-1.mp3", 125_000, markers, rate, "Checked: ok.");
  const get = (n: string) => files.find((f) => f.name === n)?.content ?? "";

  it("writes the episode fields hosts ask for, with times in milliseconds", () => {
    const e = JSON.parse(get("episode.json"));
    expect(e).toMatchObject({ title: "Ep 1", itunes_explicit: true, itunes_episode: 5, itunes_season: 2, itunes_episode_type: "full", audio_file: "Ep-1.mp3", duration_ms: 125_000 });
    expect(e.chapters.map((c: { title: string; start_time_ms: number }) => [c.title, c.start_time_ms])).toEqual([["Intro", 0], ["Wrap", 90_000]]);
    expect(e.midrolls).toEqual([{ start_time_ms: 30_000 }]);
    expect(e.soundbites).toEqual([{ start_time_ms: 40_000, duration_ms: 15_000, title: "Funny bit" }]);
  });

  it("leaves out numbers that were not given and files with nothing in them", () => {
    const bare = packageFiles({ ...EMPTY_EPISODE, title: "T" }, "T.mp3", 1000, [], rate, "");
    expect(bare.map((f) => f.name)).toEqual(["episode.json", "show-notes.txt", "README.txt"]);
    const e = JSON.parse(bare[0]!.content);
    expect("itunes_season" in e || "itunes_episode" in e).toBe(false);
  });

  it("puts a timestamp list under the description and a chapters file beside it", () => {
    expect(get("show-notes.txt")).toBe("About it.\n\nChapters\n0:00 Intro\n1:30 Wrap\n");
    expect(JSON.parse(get("chapters.json")).chapters).toHaveLength(2);
    expect(get("README.txt")).toContain("Ad positions (1): 0:30");
    expect(files.map((f) => f.name)).toContain("soundbites.json");
  });
});
