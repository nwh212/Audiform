// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Help > Report a Problem: turns what the user wrote, and the details
 * they agree to include, into a ready-filled problem report. Nothing is
 * sent from the app: the report opens in the browser on the project's
 * issue page (or is copied), and the user sends it from there.
 */

export interface ReportInput {
  what: string;
  steps: string;
  expected: string;
  /** Version, system and theme, when the user leaves them in. */
  details: string | null;
}

/** Where reports go: a new issue on the project's page. */
export const ISSUE_URL = "https://github.com/nwh212/audiform/issues/new";

/** Longest address browsers and the site reliably take. */
const MAX_URL = 7000;

/** The report as text: a title, then sections. */
export function reportText(r: ReportInput): { title: string; body: string } {
  const what = r.what.trim();
  const first = what.split(/\r?\n/)[0] ?? "";
  const title = first.length > 80 ? `${first.slice(0, 77)}...` : first || "Problem report";
  const parts = [`## What happened\n\n${what || "(not given)"}`];
  if (r.steps.trim()) parts.push(`## Steps to see it\n\n${r.steps.trim()}`);
  if (r.expected.trim()) parts.push(`## What I expected\n\n${r.expected.trim()}`);
  if (r.details) parts.push(`## Details\n\n${r.details}`);
  parts.push("_Sent from Audiform's Report a Problem window._");
  return { title, body: parts.join("\n\n") };
}

/** The issue page with the report filled in, shortened to fit when it is very long. */
export function reportUrl(r: ReportInput): string {
  const { title, body } = reportText(r);
  const make = (b: string) => `${ISSUE_URL}?title=${encodeURIComponent(title)}&body=${encodeURIComponent(b)}`;
  let url = make(body);
  if (url.length <= MAX_URL) return url;
  // Too long for an address: cut the body and say so.
  let cut = body.length;
  while (url.length > MAX_URL && cut > 200) {
    cut = Math.floor(cut * 0.85);
    url = make(`${body.slice(0, cut)}\n\n(Shortened to fit. Paste the rest from Copy Report.)`);
  }
  return url;
}
