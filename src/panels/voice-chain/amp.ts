// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

/**
 * What an Amp Model slot holds, for its window: the capture's name, kind,
 * rate and gear, and the cabinet's name and length. The slot's data is
 * the capture file's JSON with the cabinet beside it (see amp.rs).
 */

export interface AmpSummary {
  model: { name: string; kind: string; rate: number | null; gear: string; detail: string } | null;
  ir: { name: string; rate: number; ms: number } | null;
  /** Why a loaded capture will not play, if it will not. */
  problem: string | null;
}

const PLAYS = new Set(["WaveNet", "LSTM", "SlimmableContainer", "Container"]);
let last: { data: string | null; sum: AmpSummary } | null = null;

export function ampSummary(data: string | null): AmpSummary {
  if (last && last.data === data) return last.sum;
  const sum: AmpSummary = { model: null, ir: null, problem: null };
  if (data) {
    try {
      const d = JSON.parse(data) as {
        model?: { architecture?: string; sample_rate?: number; metadata?: Record<string, unknown> };
        model_name?: string;
        ir?: { name?: string; rate?: number; samples?: number[] };
      };
      if (d.model) {
        const m = d.model.metadata ?? {};
        const text = (k: string) => (typeof m[k] === "string" ? (m[k] as string) : "");
        const gear = [text("gear_make"), text("gear_model")].filter(Boolean).join(" ");
        const arch = d.model.architecture ?? "Unknown";
        // A container holds one capture at several sizes; the full one plays.
        const kind = arch === "SlimmableContainer" || arch === "Container" ? "WaveNet (sized)" : arch;
        const by = text("modeled_by");
        sum.model = {
          name: text("name") || d.model_name || "Capture",
          kind,
          rate: typeof d.model.sample_rate === "number" ? d.model.sample_rate : null,
          gear,
          detail: [d.model_name, gear, by && `captured by ${by}`].filter(Boolean).join(", "),
        };
        if (!PLAYS.has(arch)) sum.problem = `${kind} captures are not supported yet, so sound passes through without the amp. WaveNet and LSTM captures play.`;
      }
      if (d.ir && d.ir.rate) {
        sum.ir = { name: d.ir.name || "Cabinet", rate: d.ir.rate, ms: Math.round(((d.ir.samples?.length ?? 0) / d.ir.rate) * 1000) };
      }
    } catch {
      sum.problem = "The amp's saved data could not be read.";
    }
  }
  last = { data, sum };
  return sum;
}
