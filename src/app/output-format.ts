// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Output file settings shared by Export Mixdown, Batch Convert and Save
 * As: format, sample rate, channels, bit depth and MP3 quality. Field
 * names match the core's `OutputFormat`, so a value goes over as is.
 */

export type FormatKind = "wav" | "flac" | "mp3" | "m4a";
export type Mp3Mode = "cbr" | "vbr";

export interface OutputFormat {
  kind: FormatKind;
  /** WAV 4 (IMA ADPCM), 8, 16, 24 or 32 (float); FLAC 16 or 24. Null keeps the source's. */
  bitDepth: number | null;
  /** Null keeps the source's rate. */
  sampleRate: number | null;
  /** Null keeps the source's channels. */
  channels: 1 | 2 | null;
  mp3Mode: Mp3Mode;
  mp3Bitrate: number;
  /** 0 (best) to 9 (smallest). */
  mp3Quality: number;
  /** AAC kbps: 96, 128, 160 or 192. */
  aacBitrate: number;
  /** Dither when writing fewer bits. On by default. */
  dither: boolean;
  /** Stereo to mono: how much of each side goes in, 0 to 1. */
  mixLeft: number;
  mixRight: number;
}

export const FORMAT_KINDS: { value: FormatKind; label: string; lossy: boolean }[] = [
  { value: "wav", label: "WAV", lossy: false },
  { value: "flac", label: "FLAC (lossless, smaller)", lossy: false },
  { value: "mp3", label: "MP3", lossy: true },
  { value: "m4a", label: "AAC (M4A)", lossy: true },
];

/** AAC uses the encoder built into Windows; other systems get it later. */
export function formatAvailable(kind: FormatKind, platform: string): boolean {
  return kind !== "m4a" || platform === "windows";
}

export const AAC_BITRATES = [96, 128, 160, 192] as const;
/** Rates the system AAC encoder takes. */
export const OUTPUT_RATES = [8_000, 11_025, 16_000, 22_050, 32_000, 44_100, 48_000, 88_200, 96_000] as const;
export const MP3_BITRATES = [32, 48, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320] as const;
/** Highest rate an MP3 carries. */
export const MP3_MAX_RATE = 48_000;

export const DEFAULT_FORMAT: OutputFormat = {
  kind: "wav",
  bitDepth: null,
  sampleRate: null,
  channels: null,
  mp3Mode: "cbr",
  mp3Bitrate: 192,
  mp3Quality: 2,
  aacBitrate: 128,
  dither: true,
  mixLeft: 0.5,
  mixRight: 0.5,
};

/** Bit depths a format offers. MP3 has none. */
export function depthsFor(kind: FormatKind): { value: number; label: string }[] {
  if (kind === "mp3" || kind === "m4a") return [];
  const d = [
    { value: 16, label: "16-bit" },
    { value: 24, label: "24-bit" },
  ];
  return kind === "wav"
    ? [{ value: 4, label: "4-bit ADPCM (smallest, lossy)" }, { value: 8, label: "8-bit" }, ...d, { value: 32, label: "32-bit float" }]
    : d;
}

export const extensionOf = (kind: FormatKind): string => kind;

/** The format a file name saves as, or null if it isn't one we write. */
export function kindOfPath(path: string): FormatKind | null {
  const ext = path.slice(path.lastIndexOf(".") + 1).toLowerCase();
  if (ext === "wav" || ext === "wave") return "wav";
  if (ext === "flac") return "flac";
  if (ext === "mp3") return "mp3";
  if (ext === "m4a") return "m4a";
  return null;
}

/** Keeps only valid values, so a stale or hand-edited preference can't break an export. */
export function sanitizeFormat(raw: unknown): OutputFormat {
  const v = (raw && typeof raw === "object" ? raw : {}) as Partial<Record<keyof OutputFormat, unknown>>;
  const kind: FormatKind = v.kind === "flac" || v.kind === "mp3" || v.kind === "m4a" ? v.kind : "wav";
  const depth = typeof v.bitDepth === "number" && depthsFor(kind).some((d) => d.value === v.bitDepth) ? v.bitDepth : null;
  const rate = typeof v.sampleRate === "number" && (OUTPUT_RATES as readonly number[]).includes(v.sampleRate) ? v.sampleRate : null;
  return {
    kind,
    bitDepth: depth,
    sampleRate: rate,
    channels: v.channels === 1 || v.channels === 2 ? v.channels : null,
    mp3Mode: v.mp3Mode === "vbr" ? "vbr" : "cbr",
    mp3Bitrate: typeof v.mp3Bitrate === "number" && (MP3_BITRATES as readonly number[]).includes(v.mp3Bitrate) ? v.mp3Bitrate : 192,
    mp3Quality: typeof v.mp3Quality === "number" ? Math.min(9, Math.max(0, Math.round(v.mp3Quality))) : 2,
    aacBitrate: typeof v.aacBitrate === "number" && (AAC_BITRATES as readonly number[]).includes(v.aacBitrate) ? v.aacBitrate : 128,
    dither: v.dither !== false,
    mixLeft: unit(v.mixLeft),
    mixRight: unit(v.mixRight),
  };
}

const unit = (x: unknown) => (typeof x === "number" && Number.isFinite(x) ? Math.min(1, Math.max(0, x)) : 0.5);

/** What the file will be, given the source it comes from. Matches the core's choices. */
export function describeFormat(f: OutputFormat, src: { sampleRate: number; channels: number; bitDepth: number }): string {
  let rate = f.sampleRate ?? src.sampleRate;
  let ch = f.channels ?? src.channels;
  if (f.kind === "mp3") {
    ch = Math.min(ch, 2);
    if (rate > MP3_MAX_RATE) rate = MP3_MAX_RATE;
  }
  if (f.kind === "m4a") {
    ch = Math.min(ch, 2);
    rate = rate > 44_100 ? 48_000 : 44_100;
  }
  const chs = ch === 1 ? "mono" : ch === 2 ? "stereo" : `${ch} ch`;
  const hz = `${rate} Hz`;
  if (f.kind === "m4a") return `AAC ${f.aacBitrate} kbps, ${hz} ${chs}`;
  if (f.kind === "mp3") {
    return f.mp3Mode === "cbr" ? `MP3 ${f.mp3Bitrate} kbps constant, ${hz} ${chs}` : `MP3 variable, quality ${f.mp3Quality}, ${hz} ${chs}`;
  }
  const want = f.bitDepth ?? src.bitDepth;
  // 4 and 8 are only ever chosen outright; a source's depth rounds up to 16, 24 or 32 like the core does.
  const picked = f.kind === "wav" && (f.bitDepth === 4 || f.bitDepth === 8) ? f.bitDepth : null;
  const bits = picked ?? (f.kind === "flac" ? (want <= 16 ? 16 : 24) : want <= 16 ? 16 : want <= 24 ? 24 : 32);
  const depth = bits === 32 ? "32-bit float" : bits === 4 ? "4-bit IMA ADPCM" : `${bits}-bit`;
  return `${f.kind === "wav" ? "WAV" : "FLAC"} ${depth}, ${hz} ${chs}`;
}

/** Rough size in bytes of `seconds` of audio in this format. */
export function estimateBytes(f: OutputFormat, seconds: number, src: { sampleRate: number; channels: number; bitDepth: number }): number {
  const rate = Math.min(f.sampleRate ?? src.sampleRate, f.kind === "mp3" ? MP3_MAX_RATE : Infinity);
  const ch = f.channels ?? src.channels;
  if (f.kind === "m4a") return (f.aacBitrate * 1000 * seconds) / 8;
  if (f.kind === "mp3") {
    // Variable quality 0..9 runs roughly 245 down to 65 kbps.
    const kbps = f.mp3Mode === "cbr" ? f.mp3Bitrate : 245 - f.mp3Quality * 20;
    return (kbps * 1000 * seconds) / 8;
  }
  const want = f.bitDepth ?? src.bitDepth;
  if (f.kind === "wav" && f.bitDepth === 4) return rate * ch * 0.5 * seconds;
  if (f.kind === "wav" && f.bitDepth === 8) return rate * ch * seconds;
  const bytes = f.kind === "flac" ? (want <= 16 ? 2 : 3) * 0.6 : want <= 16 ? 2 : want <= 24 ? 3 : 4;
  return rate * ch * bytes * seconds;
}
