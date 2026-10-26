<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  A needle meter for the hardware-style effect windows. It reads the
  effect's own meter about 30 times a second while its window is open,
  and moves with a slow, smooth needle. In gain reduction mode the needle
  rests at 0 and falls back as the effect turns the sound down; in output
  mode it shows the level against a 0 mark at -18 dBFS.
-->
<script lang="ts">
  import { effectMeter } from "../../app/core";

  interface Props {
    slotId: string;
    /** "gr": gain reduction; "out": output level. */
    mode: "gr" | "out";
    width?: number;
  }

  const { slotId, mode, width = 230 }: Props = $props();

  /** Scale marks in the meter's own units (dB from its 0 mark). */
  const MARKS = [-20, -10, -7, -5, -3, -2, -1, 0, 1, 2, 3];
  const LOW = -20;
  const HIGH = 3;
  // An even spread of the scale that gives the top more room, as needle meters do.
  const pos = (db: number) => {
    const g = Math.pow(10, Math.max(LOW, Math.min(HIGH, db)) / 20);
    return (g - Math.pow(10, LOW / 20)) / (Math.pow(10, HIGH / 20) - Math.pow(10, LOW / 20));
  };
  const ang = (u: number) => -48 + u * 96;

  let reading = $state(0);
  let shown = $state(0);
  $effect(() => {
    const id = slotId;
    let alive = true;
    let last = performance.now();
    let frame = 0;
    let busy = false;
    const step = (now: number) => {
      if (!alive) return;
      if (!busy) {
        busy = true;
        effectMeter(id)
          .then((m) => {
            if (m) reading = mode === "gr" ? -m[0] : m[1] + 18;
          })
          .catch(() => {})
          .finally(() => (busy = false));
      }
      // About 300 ms to settle, like a needle with some weight.
      const dt = Math.min(0.1, (now - last) / 1000);
      last = now;
      shown += (reading - shown) * (1 - Math.exp(-dt / 0.12));
      frame = requestAnimationFrame(step);
    };
    frame = requestAnimationFrame(step);
    return () => {
      alive = false;
      cancelAnimationFrame(frame);
    };
  });

  const H = $derived(width * 0.5);
  const pivot = $derived({ x: width / 2, y: H * 1.55 });
  const radius = $derived(H * 1.02);
  const at = (deg: number, r: number) => {
    const a = ((deg - 90) * Math.PI) / 180;
    return [pivot.x + r * Math.cos(a), pivot.y + r * Math.sin(a)] as const;
  };
  const needle = $derived(ang(pos(shown)));
  const tipAt = $derived(at(needle, radius + 4));
  const arc = $derived.by(() => {
    const [x1, y1] = at(ang(0), radius);
    const [x2, y2] = at(ang(1), radius);
    return `M${x1},${y1} A${radius},${radius} 0 0 1 ${x2},${y2}`;
  });
  const hot = $derived.by(() => {
    const [x1, y1] = at(ang(pos(0)), radius);
    const [x2, y2] = at(ang(1), radius);
    return `M${x1},${y1} A${radius},${radius} 0 0 1 ${x2},${y2}`;
  });
</script>

<div class="nm" style:width="{width}px">
  <svg {width} height={H} role="meter" aria-label={mode === "gr" ? "Gain reduction" : "Output level"} aria-valuemin={LOW} aria-valuemax={HIGH} aria-valuenow={Math.round(shown * 10) / 10}>
    <defs>
      <radialGradient id="nm-glow" cx="50%" cy="85%" r="80%">
        <stop offset="0%" stop-color="#f6e7c1" />
        <stop offset="100%" stop-color="#d9bf86" />
      </radialGradient>
    </defs>
    <rect x="0" y="0" {width} height={H} rx="4" fill="url(#nm-glow)" />
    <path d={arc} class="arc" />
    <path d={hot} class="hot" />
    {#each MARKS as m (m)}
      {@const [x1, y1] = at(ang(pos(m)), radius)}
      {@const [x2, y2] = at(ang(pos(m)), radius + 6)}
      {@const [tx, ty] = at(ang(pos(m)), radius + 14)}
      <line class="tk" class:red={m > 0} x1={x1} y1={y1} x2={x2} y2={y2} />
      <text class="num" class:red={m > 0} x={tx} y={ty + 3} text-anchor="middle">{m > 0 ? `+${m}` : m}</text>
    {/each}
    <text class="unit" x={width / 2} y={H - 24} text-anchor="middle">{mode === "gr" ? "GAIN REDUCTION" : "OUTPUT"}</text>
    <line class="needle" x1={pivot.x} y1={pivot.y} x2={tipAt[0]} y2={tipAt[1]} />
    <rect x="0" y={H - 14} {width} height="14" class="hood" />
  </svg>
</div>

<style>
  .nm {
    padding: 6px;
    border-radius: 6px;
    background: #141312;
    box-shadow: inset 0 0 0 1px #000, inset 0 2px 6px rgba(0, 0, 0, 0.8);
  }
  svg {
    display: block;
  }
  .arc {
    fill: none;
    stroke: #2a2420;
    stroke-width: 1.4;
  }
  .hot {
    fill: none;
    stroke: #b3261e;
    stroke-width: 4;
  }
  .tk {
    stroke: #2a2420;
    stroke-width: 1.2;
  }
  .tk.red,
  .num.red {
    stroke: #b3261e;
    fill: #b3261e;
  }
  .num {
    fill: #2a2420;
    font-size: 9px;
    font-weight: 600;
  }
  .unit {
    fill: #5a4c3c;
    font-size: 8px;
    letter-spacing: 0.15em;
  }
  .needle {
    stroke: #1a1512;
    stroke-width: 1.6;
  }
  .hood {
    fill: #1c1a18;
  }
</style>
