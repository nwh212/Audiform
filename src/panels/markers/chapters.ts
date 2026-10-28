// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Marker lists as files people paste or upload: a timestamp list for
 * video descriptions, a spreadsheet (CSV), and the open podcast chapters
 * JSON format that podcast apps read. Pure text, so it is all tested.
 */

import type { MarkerDoc } from "../../stores/session";

export type ChapterFormat = "timestamps" | "csv" | "podcast-json";

export const CHAPTER_FORMATS: { id: ChapterFormat; label: string; ext: string; filter: string }[] = [
  { id: "timestamps", label: "Timestamp list (video descriptions)", ext: "txt", filter: "Text" },
  { id: "csv", label: "Spreadsheet (CSV)", ext: "csv", filter: "CSV" },
  { id: "podcast-json", label: "Podcast chapters (JSON)", ext: "json", filter: "Chapters JSON" },
];

export interface Chapter {
  title: string;
  /** Seconds. */
  start: number;
  /** Seconds. Null for a point marker. */
  end: number | null;
  kind: string;
  description: string;
  url: string | null;
}

/** The markers to export, in time order: chapter markers only, or every marker. */
export function chaptersOf(markers: readonly MarkerDoc[], rate: number, which: "chapters" | "all"): Chapter[] {
  return markers
    .filter((m) => which === "all" || m.kind === "chapter")
    .slice()
    .sort((a, b) => a.position - b.position)
    .map((m) => {
      const info = m.chapter as { url?: string | null } | null;
      return {
        title: m.name.trim() || "Untitled",
        start: m.position / rate,
        end: m.duration > 0 ? (m.position + m.duration) / rate : null,
        kind: m.kind,
        description: m.description,
        url: info?.url ?? null,
      };
    });
}

/** 0:00, 4:05, 1:02:03. */
export function clock(seconds: number): string {
  const t = Math.max(0, Math.floor(seconds));
  const h = Math.floor(t / 3600);
  const m = Math.floor((t % 3600) / 60);
  const s = String(t % 60).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${s}` : `${m}:${s}`;
}

/** 00:01:02.345 */
function precise(seconds: number): string {
  const ms = Math.max(0, Math.round(seconds * 1000));
  const h = Math.floor(ms / 3_600_000);
  const m = Math.floor((ms % 3_600_000) / 60_000);
  const s = Math.floor((ms % 60_000) / 1000);
  return `${String(h).padStart(2, "0")}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}.${String(ms % 1000).padStart(3, "0")}`;
}

const csvCell = (v: string) => (/[",\r\n]/.test(v) ? `"${v.replace(/"/g, '""')}"` : v);

export function render(list: readonly Chapter[], format: ChapterFormat): string {
  if (format === "timestamps") return list.map((c) => `${clock(c.start)} ${c.title}`).join("\n") + "\n";
  if (format === "csv") {
    const rows = list.map((c) =>
      [c.title, precise(c.start), c.end === null ? "" : precise(c.end), c.end === null ? "" : precise(c.end - c.start), c.kind, c.description].map(csvCell).join(","),
    );
    return ["Name,Start,End,Duration,Type,Description", ...rows].join("\r\n") + "\r\n";
  }
  const chapters = list.map((c) => {
    const o: Record<string, unknown> = { startTime: Math.round(c.start * 1000) / 1000, title: c.title };
    if (c.end !== null) o.endTime = Math.round(c.end * 1000) / 1000;
    if (c.url) o.url = c.url;
    return o;
  });
  return JSON.stringify({ version: "1.2.0", chapters }, null, 2) + "\n";
}

/** Problems a site or app would reject, as plain warnings. Empty when fine. */
export function warnings(list: readonly Chapter[], format: ChapterFormat): string[] {
  const out: string[] = [];
  if (list.length === 0) return ["There are no markers to export."];
  if (format === "timestamps") {
    if (list[0]!.start >= 1) out.push("Video chapters must start at 0:00. Add a marker at the very start.");
    if (list.length < 3) out.push("Video sites need at least three chapters before they show them.");
    for (let i = 1; i < list.length; i++) {
      if (list[i]!.start - list[i - 1]!.start < 10) {
        out.push(`"${list[i - 1]!.title}" is shorter than 10 seconds; video sites ignore chapters that short.`);
        break;
      }
    }
  }
  return out;
}

/**
 * Chapter markers to write into an exported file, in milliseconds from
 * the start of the export. With a range (a time-selection export), only
 * chapters inside it go, moved so the range starts at 0.
 */
export function embedList(markers: readonly MarkerDoc[], rate: number, range: [number, number] | null): { title: string; startMs: number; endMs: number | null; url?: string | null }[] {
  const [a, b] = range ?? [0, Number.POSITIVE_INFINITY];
  return markers
    .filter((m) => m.kind === "chapter" && m.position >= a && m.position < b)
    .slice()
    .sort((x, y) => x.position - y.position)
    .map((m) => ({
      title: m.name.trim() || "Untitled",
      startMs: Math.round(((m.position - a) / rate) * 1000),
      endMs: m.duration > 0 ? Math.round(((Math.min(m.position + m.duration, b) - a) / rate) * 1000) : null,
      url: chapterUrl(m) || null,
    }));
}

/** A chapter marker's link, or "". */
export function chapterUrl(m: Pick<MarkerDoc, "chapter">): string {
  const info = m.chapter as { url?: string | null } | null;
  return info?.url ?? "";
}

/** A link a chapter can carry: http or https, no spaces. */
export const isChapterUrl = (text: string) => /^https?:\/\/[^\s]+$/i.test(text.trim());

/**
 * Plain tips for a chapter title, from Apple's guidance for chapter titles:
 * 45 characters or fewer, no more than five words, no repeated words.
 * Empty when the title is fine. Tips only; nothing is blocked.
 */
export function titleTips(title: string): string[] {
  const t = title.trim();
  if (!t) return [];
  const out: string[] = [];
  const words = t.split(/\s+/).filter(Boolean);
  if (t.length > 45) out.push(`${t.length} characters; apps show about 45.`);
  if (words.length > 5) out.push(`${words.length} words; five or fewer reads best.`);
  const seen = new Set<string>();
  for (const w of words.map((x) => x.toLowerCase().replace(/[^\p{L}\p{N}]/gu, "")).filter((x) => x.length > 3)) {
    if (seen.has(w)) {
      out.push(`"${w}" appears twice.`);
      break;
    }
    seen.add(w);
  }
  return out;
}
