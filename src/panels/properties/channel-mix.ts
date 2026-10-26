// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * The Channel Routing: how much of each of a file's channels a clip sends
 * to the left and the right (from -100% to 100%; negative flips its
 * polarity). Channel names follow the standard order of multichannel
 * WAV files, the same as the core (`dsp/channels.rs`).
 */

import type { ChannelMixDoc } from "../../stores/session";

const NAMED: Record<number, string[]> = {
  1: ["M"],
  2: ["L", "R"],
  3: ["L", "R", "C"],
  4: ["L", "R", "Ls", "Rs"],
  5: ["L", "R", "C", "Ls", "Rs"],
  6: ["L", "R", "C", "LFE", "Ls", "Rs"],
  7: ["L", "R", "C", "LFE", "Cs", "Ls", "Rs"],
  8: ["L", "R", "C", "LFE", "Lb", "Rb", "Ls", "Rs"],
  10: ["L", "R", "C", "LFE", "Ls", "Rs", "Ltf", "Rtf", "Ltb", "Rtb"],
  12: ["L", "R", "C", "LFE", "Lb", "Rb", "Ls", "Rs", "Ltf", "Rtf", "Ltb", "Rtb"],
};

/** Speaker names for a file with `n` channels, in file order; numbers when the order is unknown. */
export function channelLabels(n: number): string[] {
  return NAMED[n] ?? Array.from({ length: n }, (_, k) => String(k + 1));
}

const M3 = Math.SQRT1_2;

/** The standard fold-down: fronts straight, center and surrounds 3 dB down, LFE left out. */
export function defaultMix(n: number): ChannelMixDoc {
  return {
    sends: channelLabels(n).map((name, k) => {
      if (name === "M") return { left: 1, right: 1 };
      if (name === "L") return { left: 1, right: 0 };
      if (name === "R") return { left: 0, right: 1 };
      if (name === "C") return { left: M3, right: M3 };
      if (name === "Cs") return { left: 0.5, right: 0.5 };
      if (name === "LFE") return { left: 0, right: 0 };
      if (name.startsWith("L")) return { left: M3, right: 0 };
      if (name.startsWith("R")) return { left: 0, right: M3 };
      return k === 0 ? { left: 1, right: 0 } : k === 1 ? { left: 0, right: 1 } : { left: 0, right: 0 };
    }),
  };
}

/** The mix a clip plays with now: its own, filled out to `n` channels, or the default. */
export function mixOf(own: ChannelMixDoc | null | undefined, n: number): ChannelMixDoc {
  if (!own) return defaultMix(n);
  return { sends: Array.from({ length: n }, (_, k) => own.sends[k] ?? { left: 0, right: 0 }) };
}

export interface MixPreset {
  id: string;
  label: string;
  mix: ChannelMixDoc;
}

const only = (n: number, pick: Record<number, [number, number]>): ChannelMixDoc => ({
  sends: Array.from({ length: n }, (_, k) => ({ left: pick[k]?.[0] ?? 0, right: pick[k]?.[1] ?? 0 })),
});

/** Quick settings for a file of `n` channels. */
export function presets(n: number): MixPreset[] {
  const names = channelLabels(n);
  const out: MixPreset[] = [{ id: "default", label: n > 2 ? "Standard fold-down" : "As recorded", mix: defaultMix(n) }];
  const lfe = names.indexOf("LFE");
  if (lfe >= 0) {
    const m = defaultMix(n);
    m.sends[lfe] = { left: 0.5, right: 0.5 };
    out.push({ id: "fold-lfe", label: "Fold-down with LFE", mix: m });
  }
  if (n === 2) out.push({ id: "swap", label: "Swap left and right", mix: only(2, { 0: [0, 1], 1: [1, 0] }) });
  if (n >= 2) {
    const g = 1 / n;
    out.push({ id: "mono", label: "Mono: all channels even", mix: { sends: names.map(() => ({ left: g, right: g })) } });
  }
  // Each left/right pair on its own: L and R, Ls and Rs, Ltf and Rtf...
  for (let k = 0; k < n; k++) {
    const a = names[k]!;
    if (!a.startsWith("L") || a === "LFE") continue;
    const r = names.indexOf(`R${a.slice(1)}`);
    if (r < 0) continue;
    out.push({ id: `pair-${k}`, label: `${a} and ${names[r]} only`, mix: only(n, { [k]: [1, 0], [r]: [0, 1] }) });
  }
  if (n > 2 && !names.includes("L")) out.push({ id: "pair-0", label: "Channels 1 and 2 only", mix: only(n, { 0: [1, 0], 1: [0, 1] }) });
  if (n > 1) for (let k = 0; k < n; k++) out.push({ id: `one-${k}`, label: `${names[k]} alone, to both sides`, mix: only(n, { [k]: [1, 1] }) });
  return out;
}

/** The preset that matches `mix`, if one does. */
export function presetFor(mix: ChannelMixDoc, n: number): string | null {
  const near = (a: number, b: number) => Math.abs(a - b) < 1e-4;
  const p = presets(n).find((p) => p.mix.sends.every((s, k) => near(s.left, mix.sends[k]?.left ?? 0) && near(s.right, mix.sends[k]?.right ?? 0)));
  return p?.id ?? null;
}
