// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Batch Convert's list and naming rules. The list lives here rather than
 * in the panel so it survives the panel being closed and reopened.
 */

import { writable } from "svelte/store";
import { DEFAULT_FORMAT, extensionOf, sanitizeFormat, type FormatKind, type OutputFormat } from "../../app/output-format";

export type RowStatus = "waiting" | "running" | "done" | "failed" | "stopped";

export interface BatchRow {
  id: string;
  source: string;
  status: RowStatus;
  /** What is happening to the file now: Processing, Converting, Saved. */
  stage?: string;
  /** 0 to 1 while running. */
  progress: number;
  error: string | null;
}

export type Naming = "affix" | "template";

export interface BatchSettings {
  format: OutputFormat;
  /** Before-and-after text around the name, or a full template. */
  naming: Naming;
  prefix: string;
  suffix: string;
  /** How output files are named: text plus {name}, {number}, {date} and {format}. */
  template: string;
  /** FX Chain preset run on each file first; null converts as is. */
  process: string | null;
  /** After the preset: peak normalize and fades. */
  level: LevelSettings;
  /** Null writes next to each source file. */
  folder: string | null;
  overwrite: boolean;
  removeDone: boolean;
  /** Carry each file's tags (and a WAV's markers) into the output. */
  copyTags: boolean;
  /** Tags to set on every output; blank leaves the copied value. Text can use the name tokens. */
  tags: TagFields;
}

export interface LevelSettings {
  normalize: boolean;
  /** Peak level to normalize to, dBFS, -30 to 0. */
  normalizeDb: number;
  /** 0 for none, up to 60 s. */
  fadeInMs: number;
  fadeOutMs: number;
}

export const DEFAULT_LEVEL: LevelSettings = { normalize: false, normalizeDb: -1, fadeInMs: 0, fadeOutMs: 0 };

export function sanitizeLevel(raw: unknown): LevelSettings {
  const v = (raw && typeof raw === "object" ? raw : {}) as Record<string, unknown>;
  const num = (x: unknown, lo: number, hi: number, d: number) => (typeof x === "number" && Number.isFinite(x) ? Math.min(hi, Math.max(lo, x)) : d);
  return {
    normalize: v.normalize === true,
    normalizeDb: num(v.normalizeDb, -30, 0, DEFAULT_LEVEL.normalizeDb),
    fadeInMs: num(v.fadeInMs, 0, 60_000, 0),
    fadeOutMs: num(v.fadeOutMs, 0, 60_000, 0),
  };
}

/** What the core needs for the level work, or null when it is all off. */
export function shapingFor(l: LevelSettings): { normalizeDb: number | null; fadeInMs: number; fadeOutMs: number } | null {
  if (!l.normalize && l.fadeInMs <= 0 && l.fadeOutMs <= 0) return null;
  return { normalizeDb: l.normalize ? l.normalizeDb : null, fadeInMs: l.fadeInMs, fadeOutMs: l.fadeOutMs };
}

/** Tag fields as typed in the panel. */
export interface TagFields {
  title: string;
  artist: string;
  album: string;
  year: string;
  genre: string;
  comment: string;
}

export const TAG_KEYS = ["title", "artist", "album", "year", "genre", "comment"] as const satisfies readonly (keyof TagFields)[];

export const EMPTY_TAGS: TagFields = { title: "", artist: "", album: "", year: "", genre: "", comment: "" };

export const DEFAULT_TEMPLATE = "{name}";

export const DEFAULT_BATCH: BatchSettings = {
  format: { ...DEFAULT_FORMAT },
  naming: "affix",
  prefix: "",
  suffix: "",
  template: DEFAULT_TEMPLATE,
  process: null,
  level: { ...DEFAULT_LEVEL },
  folder: null,
  overwrite: false,
  removeDone: false,
  copyTags: true,
  tags: { ...EMPTY_TAGS },
};

export const batchRows = writable<BatchRow[]>([]);

export function sanitizeBatch(raw: unknown): BatchSettings {
  const v = (raw && typeof raw === "object" ? raw : {}) as Partial<Record<keyof BatchSettings | "prefix" | "suffix", unknown>>;
  const text = (x: unknown) => (typeof x === "string" ? x : "");
  // Settings saved before templates had a Before and After box.
  const old = `${text(v.prefix)}{name}${text(v.suffix)}`;
  const v2 = v as Record<string, unknown>;
  // Older settings had only a template (or Before and After boxes saved as prefix and suffix).
  const naming: Naming = v2.naming === "template" || (v2.naming === undefined && text(v.template).trim() !== "" && text(v.template) !== DEFAULT_TEMPLATE) ? "template" : "affix";
  return {
    format: sanitizeFormat(v.format),
    naming,
    prefix: text(v.prefix),
    suffix: text(v.suffix),
    process: typeof v2.process === "string" && v2.process ? v2.process : null,
    level: sanitizeLevel(v2.level),
    template: text(v.template).trim() !== "" ? text(v.template) : old,
    folder: typeof v.folder === "string" && v.folder ? v.folder : null,
    overwrite: v.overwrite === true,
    removeDone: v.removeDone === true,
    copyTags: v.copyTags !== false,
    tags: sanitizeTags(v.tags),
  };
}

function sanitizeTags(raw: unknown): TagFields {
  const v = (raw && typeof raw === "object" ? raw : {}) as Record<string, unknown>;
  const out = { ...EMPTY_TAGS };
  for (const k of TAG_KEYS) if (typeof v[k] === "string") out[k] = v[k];
  return out;
}

/**
 * The tags one file gets: the typed fields with their tokens filled in.
 * Blank fields are left out so the copied value stays. Null when nothing is set.
 */
export function tagsFor(fields: TagFields, source: string, kind: FormatKind, ctx: NameContext): Partial<TagFields> | null {
  const name = fileName(source);
  const dot = name.lastIndexOf(".");
  const stem = dot > 0 ? name.slice(0, dot) : name;
  const out: Partial<TagFields> = {};
  for (const k of TAG_KEYS) {
    const t = fields[k].trim();
    if (t) out[k] = fillTokens(t, stem, kind, ctx);
  }
  return Object.keys(out).length ? out : null;
}

const lastSep = (p: string) => Math.max(p.lastIndexOf("/"), p.lastIndexOf("\\"));

export function fileName(path: string): string {
  return path.slice(lastSep(path) + 1);
}

/** Characters Windows won't take in a file name. */
const cleanPart = (s: string) => s.replace(/[<>:"/\\|?*\u0000-\u001f]/g, "_");

/** What the tokens in a name template can use. */
export interface NameContext {
  /** Position in the list, starting at 1. */
  number: number;
  /** How many files are in the list; sets how many digits {number} has. */
  count: number;
  date: Date;
}

const pad = (n: number, w: number) => String(n).padStart(w, "0");

/** The template that naming settings stand for. */
export function effectiveTemplate(s: Pick<BatchSettings, "naming" | "prefix" | "suffix" | "template">): string {
  return s.naming === "template" ? s.template : `${s.prefix}{name}${s.suffix}`;
}

/** Fills the tokens {name}, {number}, {date} (2026-10-15) and {format} (the output extension). */
function fillTokens(text: string, stem: string, kind: FormatKind, ctx: NameContext): string {
  const date = `${ctx.date.getFullYear()}-${pad(ctx.date.getMonth() + 1, 2)}-${pad(ctx.date.getDate(), 2)}`;
  return text
    .replace(/\{name\}/gi, stem)
    .replace(/\{number\}/gi, pad(ctx.number, Math.max(2, String(ctx.count).length)))
    .replace(/\{date\}/gi, date)
    .replace(/\{format\}/gi, extensionOf(kind));
}

/** Fills a name template and makes it safe as a file name. */
export function expandTemplate(template: string, stem: string, kind: FormatKind, ctx: NameContext): string {
  const out = fillTokens(template.trim() === "" ? DEFAULT_TEMPLATE : template, stem, kind, ctx);
  return cleanPart(out).trim() || cleanPart(stem);
}

/** Where a source file converts to. */
export function destFor(
  source: string,
  s: { template: string; folder: string | null; kind: FormatKind },
  ctx: NameContext = { number: 1, count: 1, date: new Date() },
): string {
  const cut = lastSep(source);
  const dir = (s.folder ?? (cut >= 0 ? source.slice(0, cut) : ".")).replace(/[\\/]+$/, "");
  const name = source.slice(cut + 1);
  const dot = name.lastIndexOf(".");
  const stem = dot > 0 ? name.slice(0, dot) : name;
  const sep = dir.includes("\\") || (s.folder === null && source.includes("\\")) ? "\\" : "/";
  return `${dir}${sep}${expandTemplate(s.template, stem, s.kind, ctx)}.${extensionOf(s.kind)}`;
}

const same = (a: string, b: string) => a.replace(/\\/g, "/").toLowerCase() === b.replace(/\\/g, "/").toLowerCase();

/**
 * Problems that stop a row from converting: two rows writing the same
 * file, or a row writing over its own source without Replace on.
 * Keyed by row id.
 */
export function conflicts(rows: readonly BatchRow[], dests: ReadonlyMap<string, string>, overwrite: boolean): Map<string, string> {
  const out = new Map<string, string>();
  const seen = new Map<string, string>();
  for (const r of rows) {
    const d = dests.get(r.id);
    if (!d) continue;
    const key = d.replace(/\\/g, "/").toLowerCase();
    const first = seen.get(key);
    if (first) out.set(r.id, `Writes the same file as ${fileName(first)}. Add a prefix or suffix.`);
    else seen.set(key, r.source);
    if (!out.has(r.id) && same(d, r.source) && !overwrite) out.set(r.id, "Would replace the original. Turn on Replace existing files, or add a prefix or suffix.");
  }
  return out;
}

/** Adds files not already listed; returns how many were new. */
export function addRows(rows: readonly BatchRow[], paths: readonly string[], newId: () => string): { rows: BatchRow[]; added: number } {
  const have = new Set(rows.map((r) => r.source.replace(/\\/g, "/").toLowerCase()));
  const next = [...rows];
  for (const p of paths) {
    const k = p.replace(/\\/g, "/").toLowerCase();
    if (have.has(k)) continue;
    have.add(k);
    next.push({ id: newId(), source: p, status: "waiting", progress: 0, error: null });
  }
  return { rows: next, added: next.length - rows.length };
}
