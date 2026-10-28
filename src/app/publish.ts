// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Publish Package: the files that go with an episode when it is uploaded
 * to a podcast host by hand. Nothing here touches the network; it builds
 * the text of each file, and the app writes them next to the audio.
 * Field names follow the common podcast host episode fields, so the numbers
 * and text can be copied straight across to most hosts.
 */

import type { MarkerDoc } from "../stores/session";
import { render, type Chapter } from "../panels/markers/chapters";

export type EpisodeType = "full" | "trailer" | "bonus";

export interface EpisodeInfo {
  title: string;
  description: string;
  /** Null leaves it out. */
  season: number | null;
  episode: number | null;
  type: EpisodeType;
  explicit: boolean;
}

export const EMPTY_EPISODE: EpisodeInfo = { title: "", description: "", season: null, episode: null, type: "full", explicit: false };

/** Hosts cap titles at 250 characters and descriptions at 4000. */
export const TITLE_MAX = 250;
export const DESCRIPTION_MAX = 4000;

export interface PackageFile {
  name: string;
  content: string;
}

/** A name for the folder and files: letters, digits and single dashes. */
export function slugOf(title: string): string {
  const s = title
    .normalize("NFKD")
    .replace(/[̀-ͯ]/g, "")
    .replace(/[^A-Za-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 80)
    .replace(/-+$/g, "");
  return s || "episode";
}

/** What would stop a host from taking the episode, in plain words. Empty when fine. */
export function episodeProblems(info: EpisodeInfo): string[] {
  const out: string[] = [];
  if (info.title.trim() === "") out.push("The episode needs a title.");
  if (info.title.length > TITLE_MAX) out.push(`The title is ${info.title.length} characters; hosts take up to ${TITLE_MAX}.`);
  if (info.description.length > DESCRIPTION_MAX) out.push(`The description is ${info.description.length} characters; hosts take up to ${DESCRIPTION_MAX}.`);
  for (const [label, n] of [["season", info.season], ["episode", info.episode]] as const) {
    if (n !== null && (!Number.isInteger(n) || n < 0)) out.push(`The ${label} number must be a whole number.`);
  }
  return out;
}


const byTime = (a: MarkerDoc, b: MarkerDoc) => a.position - b.position;

/** The whole-session markers of one kind, as chapter-like entries in seconds. */
function ofKind(markers: readonly MarkerDoc[], rate: number, kind: MarkerDoc["kind"]): Chapter[] {
  return markers
    .filter((m) => m.kind === kind)
    .slice()
    .sort(byTime)
    .map((m) => ({
      title: m.name.trim() || "Untitled",
      start: m.position / rate,
      end: m.duration > 0 ? (m.position + m.duration) / rate : null,
      kind: m.kind,
      description: m.description,
      url: (m.chapter as { url?: string | null } | null)?.url ?? null,
    }));
}

/**
 * Every file of the package except the audio itself. `audioName` is the
 * audio file's name inside the folder; `checked` says how the check of the
 * finished file went, for the checklist.
 */
export function packageFiles(info: EpisodeInfo, audioName: string, durationMs: number, markers: readonly MarkerDoc[], rate: number, checked: string): PackageFile[] {
  const chapters = ofKind(markers, rate, "chapter");
  const midrolls = ofKind(markers, rate, "ad_slot");
  const clips = ofKind(markers, rate, "social_clip");

  const episode: Record<string, unknown> = {
    title: info.title.trim(),
    description: info.description.trim(),
    itunes_explicit: info.explicit,
    itunes_episode_type: info.type,
  };
  if (info.episode !== null) episode.itunes_episode = info.episode;
  if (info.season !== null) episode.itunes_season = info.season;
  episode.audio_file = audioName;
  episode.duration_ms = durationMs;
  episode.chapters = chapters.map((c) => ({
    start_time_ms: Math.round(c.start * 1000),
    duration_ms: c.end === null ? null : Math.round((c.end - c.start) * 1000),
    title: c.title,
    url: c.url,
  }));
  episode.midrolls = midrolls.map((c) => ({ start_time_ms: Math.round(c.start * 1000) }));
  episode.soundbites = clips.map((c) => ({
    start_time_ms: Math.round(c.start * 1000),
    duration_ms: c.end === null ? null : Math.round((c.end - c.start) * 1000),
    title: c.title,
  }));

  const notes = [info.description.trim()];
  if (chapters.length) notes.push("", "Chapters", render(chapters, "timestamps").trimEnd());

  const files: PackageFile[] = [{ name: "episode.json", content: JSON.stringify(episode, null, 2) + "\n" }, { name: "show-notes.txt", content: notes.join("\n").trimEnd() + "\n" }];
  if (chapters.length) files.push({ name: "chapters.json", content: render(chapters, "podcast-json") });
  if (clips.length) files.push({ name: "soundbites.json", content: JSON.stringify({ soundbites: (episode.soundbites as unknown[]) }, null, 2) + "\n" });

  const lines = [
    `${info.title.trim()}`,
    "",
    "Files in this folder",
    `  ${audioName}  the episode${chapters.length ? ", with its chapters inside the file" : ""}`,
    "  episode.json  title, description, numbers, chapters, ad positions and soundbites in one place",
    "  show-notes.txt  the description, with a timestamp list when there are chapters",
  ];
  if (chapters.length) lines.push("  chapters.json  the chapters as a Podcasting 2.0 chapters file, for hosts that take a file");
  if (clips.length) lines.push("  soundbites.json  the soundbite positions");
  lines.push(
    "",
    "Before you upload",
    `  ${checked}`,
    `  Length: ${clockOf(durationMs)}.`,
    midrolls.length ? `  Ad positions (${midrolls.length}): ${midrolls.map((m) => clockOf(Math.round(m.start * 1000))).join(", ")}` : "  No ad positions are marked.",
    "",
    "Upload steps are the same on every host: the audio file first, then the title, description and numbers from episode.json,",
    "then the chapters and ad positions if your host asks for them separately.",
    "Made with Audiform. Nothing was sent anywhere.",
  );
  files.push({ name: "README.txt", content: lines.join("\n") + "\n" });
  return files;
}

function clockOf(totalMs: number): string {
  const t = Math.floor(totalMs / 1000);
  const h = Math.floor(t / 3600);
  const m = Math.floor((t % 3600) / 60);
  const s = t % 60;
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}` : `${m}:${String(s).padStart(2, "0")}`;
}
