// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import {
  BUILTIN_PRESETS,
  clipRuns,
  cutRange,
  DEFAULTS,
  findAll,
  noiseRuns,
  scanInfo,
  findSilences,
  sanitizeSettings,
  shiftFindings,
  silentRuns,
  suggestLevels,
  type Finding,
} from "./quickpass";

const W = 0.1; // seconds per window
const s = {
  ...DEFAULTS,
  quietDb: -45,
  contentDb: -35,
  minQuietSec: 0.3,
  minContentSec: 0.2,
};
const quiet = (n: number) => Array<number>(n).fill(-60);
const loud = (n: number) => Array<number>(n).fill(-20);

describe("silentRuns", () => {
  it("finds quiet stretches that last long enough", () => {
    expect(silentRuns([...loud(5), ...quiet(4), ...loud(5)], W, s)).toEqual([
      [5, 9],
    ]);
  });
  it("ignores quiet that is too short", () => {
    expect(silentRuns([...loud(5), ...quiet(2), ...loud(5)], W, s)).toEqual([]);
  });
  it("keeps a silence going through a very short burst of sound", () => {
    expect(silentRuns([...quiet(4), ...loud(1), ...quiet(4)], W, s)).toEqual([
      [0, 9],
    ]);
  });
  it("splits at speech that lasts long enough", () => {
    expect(silentRuns([...quiet(4), ...loud(3), ...quiet(4)], W, s)).toEqual([
      [0, 4],
      [7, 11],
    ]);
  });
  it("a silence at the very end counts", () => {
    expect(silentRuns([...loud(3), ...quiet(5)], W, s)).toEqual([[3, 8]]);
  });
  it("sound between the two levels does not start a silence", () => {
    expect(silentRuns(Array<number>(10).fill(-40), W, s)).toEqual([]);
  });
});

describe("findSilences", () => {
  it("places findings on the timeline", () => {
    const f = findSilences(
      [
        {
          clipId: "c",
          start: 1000,
          window: 100,
          db: [...loud(2), ...quiet(5), ...loud(1)],
          peak: [],
        },
      ],
      new Map([["c", { trackId: "t", name: "Host" }]]),
      s,
      1000,
    );
    expect(f).toEqual([
      {
        id: "c:1200",
        kind: "silence",
        trackId: "t",
        clipId: "c",
        clipName: "Host",
        start: 1200,
        end: 1700,
      },
    ]);
  });
});

describe("suggestLevels", () => {
  it("sets quiet above the noise floor and below speech", () => {
    const db = [...Array<number>(30).fill(-62), ...Array<number>(70).fill(-22)];
    const r = suggestLevels(db)!;
    expect(r.quietDb).toBeGreaterThan(-62);
    expect(r.contentDb).toBeLessThan(-22);
    expect(r.contentDb).toBeGreaterThanOrEqual(r.quietDb);
  });
  it("gives up on too little or flat audio", () => {
    expect(suggestLevels([-30, -30])).toBeNull();
    expect(suggestLevels(Array<number>(100).fill(-30))).toBeNull();
  });
});

describe("sanitizeSettings", () => {
  it("fills in and clamps", () => {
    expect(sanitizeSettings(null)).toEqual(DEFAULTS);
    expect(sanitizeSettings({ action: "bogus", keepMs: -5 })).toMatchObject({
      action: "remove",
      keepMs: 0,
    });
    expect(
      sanitizeSettings({ quietDb: -30, contentDb: -50, ripple: "all" }),
    ).toMatchObject({ quietDb: -30, contentDb: -30, ripple: "all" });
  });
});

describe("shiftFindings", () => {
  const f = (
    id: string,
    trackId: string,
    start: number,
    end: number,
  ): Finding => ({ id, kind: "silence", trackId, clipName: "", start, end });
  it("moves later ones left on the same track only", () => {
    const gone = f("a", "t1", 100, 200);
    const out = shiftFindings(
      [gone, f("b", "t1", 300, 400), f("c", "t2", 300, 400)],
      gone,
      "track",
    );
    expect(out.map((x) => [x.id, x.start])).toEqual([
      ["b", 200],
      ["c", 300],
    ]);
  });
  it("moves every track when the gap closes on all", () => {
    const gone = f("a", "t1", 100, 200);
    expect(shiftFindings([f("c", "t2", 300, 400)], gone, "all")[0]!.start).toBe(
      200,
    );
  });
});

import {
  applyLocal,
  clipsOn,
  duckOp,
  markersFor,
  removeOp,
  soundRuns,
  splitOp,
  spotFindings,
  spotsRequest,
} from "./quickpass";
import { levelAt, volumePoints } from "../editor/envelope";
import type { ClipDoc, SessionDoc, TrackDoc } from "../../stores/session";

const fd = () => ({ length: 0, shape: "linear" as const, curve: 0 });
const mkClip = (id: string, start: number, out = 48_000): ClipDoc => ({
  id,
  name: id,
  media_id: "m",
  start,
  source_in: 0,
  source_out: out,
  gain_db: 0,
  muted: false,
  locked: false,
  color: null,
  fade_in: fd(),
  fade_out: fd(),
});
const mkDoc = (clips: ClipDoc[]): SessionDoc =>
  ({
    format: "altaudio.session",
    schema_version: 1,
    id: "s",
    name: "s",
    sample_rate: 48_000,
    bit_depth: 32,
    media: [
      {
        id: "m",
        path: "a.wav",
        relative_path: null,
        sample_rate: 48_000,
        channels: 1,
        length: 480_000,
      },
    ],
    tracks: [
      { id: "t", name: "t", clips } as unknown as TrackDoc,
      {
        id: "u",
        name: "u",
        clips: [mkClip("bed", 0, 48_000)],
      } as unknown as TrackDoc,
    ],
    markers: [],
    view: {
      samples_per_pixel: 100,
      scroll: 0,
      scroll_y: 0,
      playhead: 0,
      selection: null,
    },
    libraries: [],
  }) as unknown as SessionDoc;
let n = 0;
const nid = () => `n${++n}`;
const span = (d: SessionDoc, id: string) => {
  const c = d.tracks.flatMap((t) => t.clips).find((x) => x.id === id)!;
  return [c.start, c.source_out - c.source_in] as const;
};
const fnd = (start: number, end: number) => ({
  id: `f${start}`,
  kind: "silence" as const,
  trackId: "t",
  clipName: "a",
  start,
  end,
});

describe("acting on findings", () => {
  it("removes two silences from one clip in a single edit", () => {
    const doc = mkDoc([mkClip("a", 0, 48_000)]);
    const op = removeOp(
      doc,
      [fnd(6_000, 12_000), fnd(24_000, 30_000)],
      { ripple: "track", keepMs: 0 },
      nid,
    )!;
    const after = applyLocal(doc, op);
    const t = after.tracks[0]!;
    const total = t.clips.reduce((s, c) => s + (c.source_out - c.source_in), 0);
    expect(total).toBe(48_000 - 12_000);
    expect(span(after, "bed")).toEqual([0, 48_000]);
    const starts = t.clips.map((c) => c.start).sort((a, b) => a - b);
    expect(starts).toEqual([0, 6_000, 18_000]);
  });

  it("splits out a silence and finds the piece again", () => {
    const doc = mkDoc([mkClip("a", 0, 48_000)]);
    const f = fnd(10_000, 20_000);
    const after = applyLocal(doc, splitOp(doc, [f], nid)!);
    expect(after.tracks[0]!.clips).toHaveLength(3);
    expect(clipsOn(after, [f])).toHaveLength(1);
  });

  it("names markers after the ones already there", () => {
    const doc = mkDoc([]);
    const m = markersFor(
      { ...doc, markers: [{ ...markersFor(doc, [fnd(0, 10)], nid)[0]! }] },
      [fnd(20, 30), fnd(40, 50)],
      nid,
    );
    expect(m.map((x) => x.name)).toEqual(["Silence 2", "Silence 3"]);
    expect(m[0]!.duration).toBe(10);
  });
});

describe("keeping some pause", () => {
  it("cuts only the middle of a silence", () => {
    expect(cutRange({ start: 0, end: 48_000 }, 100, 48_000)).toEqual([
      2_400, 45_600,
    ]);
  });
  it("leaves a silence that is shorter than the pause alone", () => {
    expect(cutRange({ start: 0, end: 4_000 }, 100, 48_000)).toBeNull();
  });
  it("removing with a kept pause shortens the silence", () => {
    const doc = mkDoc([mkClip("a", 0, 48_000)]);
    const op = removeOp(
      doc,
      [fnd(10_000, 20_000)],
      { ripple: "track", keepMs: 100 },
      nid,
    )!;
    const t = applyLocal(doc, op).tracks[0]!;
    expect(t.clips.reduce((n, c) => n + (c.source_out - c.source_in), 0)).toBe(
      48_000 - (10_000 - 4_800),
    );
  });
});

describe("presets", () => {
  it("are all valid settings with unique names", () => {
    const names = BUILTIN_PRESETS.map((p) => p.name);
    expect(new Set(names).size).toBe(names.length);
    for (const p of BUILTIN_PRESETS)
      expect(sanitizeSettings(p.settings)).toEqual(p.settings);
  });
});

describe("turning silences down", () => {
  it("dips the volume over the silence and keeps the clip whole", () => {
    const doc = mkDoc([mkClip("a", 0, 48_000)]);
    const after = applyLocal(doc, duckOp(doc, [fnd(10_000, 20_000)], -15)!);
    const c = after.tracks[0]!.clips;
    expect(c).toHaveLength(1);
    const pts = volumePoints(c[0]!);
    expect(levelAt(pts, 15_000)).toBeCloseTo(-15);
    expect(levelAt(pts, 5_000)).toBeCloseTo(0);
    expect(levelAt(pts, 30_000)).toBeCloseTo(0);
  });
  it("keeps an existing envelope level either side", () => {
    const base = mkClip("a", 0, 48_000);
    const withEnv = {
      ...base,
      envelopes: [
        {
          id: "e",
          target: { type: "volume" as const },
          enabled: true,
          points: [
            { position: 0, value: -6, curve: "linear" as const },
            { position: 48_000, value: -6, curve: "linear" as const },
          ],
        },
      ],
    };
    const doc = mkDoc([withEnv]);
    const after = applyLocal(doc, duckOp(doc, [fnd(10_000, 20_000)], -10)!);
    const pts = volumePoints(after.tracks[0]!.clips[0]!);
    expect(levelAt(pts, 15_000)).toBeCloseTo(-16);
    expect(levelAt(pts, 40_000)).toBeCloseTo(-6);
  });
  it("leaves locked clips alone", () => {
    const doc = mkDoc([{ ...mkClip("a", 0, 48_000), locked: true }]);
    expect(duckOp(doc, [fnd(10_000, 20_000)], -10)).toBeNull();
  });
});

describe("short noises", () => {
  const n = {
    ...DEFAULTS,
    scan: "noise" as const,
    quietDb: -45,
    maxBurstSec: 0.3,
    guardSec: 0.3,
  };
  it("finds a cough between sentences", () => {
    // speech, quiet, 0.2 s cough, quiet, speech
    const db = [...loud(10), ...quiet(5), ...loud(2), ...quiet(5), ...loud(10)];
    expect(noiseRuns(db, W, n)).toEqual([[14, 18]]);
  });
  it("leaves short words that sit close to other speech", () => {
    const db = [...loud(10), ...quiet(2), ...loud(2), ...quiet(2), ...loud(10)];
    expect(noiseRuns(db, W, n)).toEqual([]);
  });
  it("ignores sounds longer than the limit", () => {
    const db = [...quiet(5), ...loud(5), ...quiet(5)];
    expect(noiseRuns(db, W, n)).toEqual([]);
  });
  it("places noise findings on the timeline with their kind", () => {
    const f = findAll(
      [
        {
          clipId: "c",
          start: 0,
          window: 100,
          db: [...quiet(5), ...loud(2), ...quiet(5)],
          peak: [],
        },
      ],
      new Map([["c", { trackId: "t", name: "Host" }]]),
      n,
      1000,
    );
    expect(f.map((x) => [x.kind, x.start, x.end])).toEqual([
      ["noise", 400, 800],
    ]);
  });
});

describe("clipping", () => {
  it("joins clipped windows that are close together", () => {
    const peak = [-6, -0.1, -6, 0, -6, -6, -6, -6, -0.2, -6];
    expect(clipRuns(peak, { ...DEFAULTS, clipDb: -0.3 })).toEqual([
      [1, 4],
      [8, 9],
    ]);
  });
  it("offers repair, mark and split", () => {
    expect(scanInfo("clipping").actions).toEqual(["repair", "mark", "split"]);
    expect(
      sanitizeSettings({ scan: "clipping", action: "remove" }).action,
    ).toBe("repair");
  });
});

describe("clicks, clipping and sound", () => {
  it("turns the core's spots into findings with their clip", () => {
    const f = spotFindings(
      [
        { clipId: "b", spans: [[5000, 5004]] },
        {
          clipId: "a",
          spans: [
            [900, 902],
            [3000, 3001],
          ],
        },
        { clipId: "gone", spans: [[1, 2]] },
      ],
      new Map([
        ["a", { trackId: "t1", name: "Host" }],
        ["b", { trackId: "t2", name: "Guest" }],
      ]),
      "clicks",
    );
    expect(f.map((x) => [x.clipId, x.start, x.end])).toEqual([
      ["a", 900, 903],
      ["a", 3000, 3002],
      ["b", 5000, 5005],
    ]);
    expect(f[0]!.kind).toBe("clicks");
  });

  it("asks the core for what the settings say", () => {
    expect(
      spotsRequest({
        ...DEFAULTS,
        scan: "clicks",
        clickSensitivity: 70,
        clickWidthMs: 0.5,
      }),
    ).toEqual({ kind: "clicks", sensitivity: 70, widthMs: 0.5 });
    expect(
      spotsRequest({
        ...DEFAULTS,
        scan: "clipping",
        clipDb: -0.5,
        clipLongestMs: 4,
      }),
    ).toEqual({ kind: "clipping", levelDb: -0.5, longestMs: 4 });
  });

  it("marks the parts with sound between silences", () => {
    const st = {
      ...DEFAULTS,
      quietDb: -50,
      contentDb: -40,
      minQuietSec: 0.3,
      minContentSec: 0.1,
    };
    const db = [
      ...Array<number>(10).fill(-20),
      ...Array<number>(5).fill(-70),
      ...Array<number>(8).fill(-20),
      ...Array<number>(4).fill(-70),
    ];
    expect(soundRuns(db, 0.1, st)).toEqual([
      [0, 10],
      [15, 23],
    ]);
  });

  it("ships repair presets that keep their scan type and action", () => {
    for (const name of ["Remove Clicks", "Repair Clipping"]) {
      const p = BUILTIN_PRESETS.find((x) => x.name === name)!;
      expect(sanitizeSettings(p.settings).action).toBe("repair");
    }
    expect(
      BUILTIN_PRESETS.find((x) => x.name === "Mark Non-Silent Areas")!.settings
        .scan,
    ).toBe("audio");
    expect(
      sanitizeSettings({ scan: "clicks", clickSensitivity: 500 })
        .clickSensitivity,
    ).toBe(100);
  });
});

describe("Strip leaves gaps open", () => {
  it("defaults to not closing gaps", () => {
    expect(sanitizeSettings({}).ripple).toBe("none");
    expect(sanitizeSettings({ ripple: "track" }).ripple).toBe("track");
  });
  it("keeps later findings where they are when the gap stays open", () => {
    const a = {
      id: "a",
      trackId: "t",
      clipName: "c",
      start: 1000,
      end: 2000,
    } as never;
    const b = {
      id: "b",
      trackId: "t",
      clipName: "c",
      start: 5000,
      end: 6000,
    } as never;
    const rest = shiftFindings([b], a, "none", 0, 48000);
    expect(rest).toEqual([b]);
  });
});
