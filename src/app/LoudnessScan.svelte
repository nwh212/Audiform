<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  Copyright (c) The Audiform contributors.
  This file is part of Audiform, based on Altered Audio Editor.

  A scan of the mix before it is exported: how loud it is, where its
  peaks reach, and what the file will come out as with the chosen
  loudness target. The scan runs when asked and is dropped when the
  target, the mix or the format changes, so it never shows stale numbers.
-->
<script lang="ts">
  import { predictExport, targetsFor, type LoudnessReport } from "./loudness";

  interface Props {
    /** The loudness target id chosen in the dialog, or "". */
    target: string;
    /** True when the file is written as mono. */
    mono?: boolean;
    /** Anything that makes an old scan stale when it changes. */
    stamp: string;
    scan: () => Promise<LoudnessReport | null>;
  }

  const { target, mono = false, stamp, scan }: Props = $props();

  let report = $state<LoudnessReport | null>(null);
  let seen = $state("");
  let busy = $state(false);

  $effect(() => {
    if (stamp !== seen) {
      report = null;
      seen = stamp;
    }
  });

  const goal = $derived(target ? (targetsFor().find((t) => t.id === target) ?? null) : null);
  const prediction = $derived(report ? predictExport(report, goal, mono) : null);

  async function run() {
    if (busy) return;
    busy = true;
    try {
      const r = await scan();
      if (r && !r.cancelled) {
        seen = stamp;
        report = r;
      }
    } finally {
      busy = false;
    }
  }
</script>

<div class="scan">
  <div class="row">
    <button type="button" onclick={run} disabled={busy} title="Plays the mix through the loudness meter without writing a file, then says what the export will come out as.">
      {busy ? "Scanning…" : report ? "Scan again" : "Scan loudness"}
    </button>
    {#if report}
      <span class="nums">
        <span>Loudness <b>{(mono ? report.integratedMono : report.integrated).toFixed(1)}</b> LUFS</span>
        <span>Peak <b>{report.truePeak.toFixed(1)}</b> dBTP</span>
        <span>Range <b>{report.range.toFixed(1)}</b> LU</span>
      </span>
    {/if}
  </div>
  {#if prediction}
    <p class="verdict {prediction.grade}">{prediction.summary}</p>
  {/if}
</div>

<style>
  .scan {
    display: grid;
    gap: 6px;
  }

  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 8px 14px;
    align-items: center;
  }

  .nums {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 14px;
    color: var(--text-dim);
  }

  b {
    color: var(--value);
    font-weight: 600;
  }

  .verdict {
    margin: 0;
    padding: 6px 8px;
    border-left: 3px solid var(--border-strong);
    background: var(--surface-0);
    color: var(--text);
  }

  .verdict.pass {
    border-left-color: #4caf50;
  }

  .verdict.close {
    border-left-color: #e0a030;
  }

  .verdict.fail {
    border-left-color: var(--danger);
  }

  button {
    height: 24px;
    padding: 0 12px;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    background: var(--surface-3);
    color: var(--text);
    font: inherit;
  }

  button:disabled {
    opacity: 0.6;
  }
</style>
