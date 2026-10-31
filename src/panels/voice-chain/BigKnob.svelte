<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  A large dial with a numbered scale, for the hardware-style effect
  windows. Drag up or down to turn it (Ctrl for fine steps, Shift for
  coarse), turn the mouse wheel over it, double-click for the default,
  or use the number box under it.
-->
<script lang="ts">
  import type { EffectParamInfo } from "../../app/core";
  import { dragValue, fromTurn, keyValue, toTurn } from "../../app/knob";
  import { formatParam, fromSlider, toSlider } from "./chain";
  import ValueField from "./ValueField.svelte";

  interface Props {
    param: EffectParamInfo;
    value: number;
    /** Outer size in pixels. */
    size?: number;
    /** Scale numbers around the dial; none when empty. */
    marks?: number[];
    label?: string | undefined;
    oninput: (v: number) => void;
    oncommit: (v: number) => void;
  }

  const { param, value, size = 110, marks = [], label, oninput, oncommit }: Props = $props();

  // The default points straight up; each half of the turn covers its side of the range.
  const home = $derived(Math.min(1, Math.max(0, toSlider(param, param.default))));
  const turn = (v: number) => toTurn(toSlider(param, v), home);
  const fromT = (u: number) => fromSlider(param, fromTurn(u, home));
  const t = $derived(turn(value));
  const ang = (u: number) => -135 + u * 270;
  const R = 50;
  const polar = (deg: number, r: number) => {
    const a = ((deg - 90) * Math.PI) / 180;
    return [60 + r * Math.cos(a), 60 + r * Math.sin(a)] as const;
  };

  const tip = $derived([...polar(ang(t), R - 6), ...polar(ang(t), 10)]);

  let drag: { y: number; start: number; last: number } | null = null;
  function down(e: PointerEvent) {
    if (e.button !== 0) return;
    (e.currentTarget as Element).setPointerCapture(e.pointerId);
    drag = { y: e.clientY, start: t, last: value };
    e.preventDefault();
  }
  function move(e: PointerEvent) {
    if (!drag) return;
    const v = fromT(dragValue(drag.start, (drag.y - e.clientY) * 0.8, { ctrl: e.ctrlKey, shift: e.shiftKey }));
    if (v === drag.last) return;
    drag.last = v;
    oninput(v);
  }
  function up() {
    if (!drag) return;
    const d = drag;
    drag = null;
    if (d.last !== value || fromT(d.start) !== d.last) oncommit(d.last);
  }
  let wheelTimer: ReturnType<typeof setTimeout> | undefined;
  let wheelValue: number | null = null;
  function wheel(e: WheelEvent) {
    e.preventDefault();
    const base = wheelValue ?? value;
    const v = fromT(turn(base) + (e.ctrlKey ? 0.002 : e.shiftKey ? 0.05 : 0.01) * (e.deltaY < 0 ? 1 : -1));
    wheelValue = v;
    oninput(v);
    clearTimeout(wheelTimer);
    wheelTimer = setTimeout(() => {
      const w = wheelValue;
      wheelValue = null;
      if (w !== null) oncommit(w);
    }, 350);
  }
  $effect(() => () => clearTimeout(wheelTimer));
</script>

<div class="bk" style:width="{size}px">
  <svg
    viewBox="0 0 120 120"
    width={size}
    height={size}
    role="slider"
    tabindex="0"
    aria-label={label ?? param.name}
    aria-valuemin={param.min}
    aria-valuemax={param.max}
    aria-valuenow={value}
    aria-valuetext={formatParam(param, value)}
    onpointerdown={down}
    onpointermove={move}
    onpointerup={up}
    onpointercancel={() => (drag = null)}
    onwheel={wheel}
    ondblclick={() => oncommit(param.default)}
    onkeydown={(e) => {
      const n = keyValue(t, e.key, 0.5);
      if (n === null) return;
      e.preventDefault();
      oncommit(fromT(n));
    }}
  >
    {#each Array.from({ length: 21 }, (_, i) => i / 20) as u (u)}
      {@const [x1, y1] = polar(ang(u), R + 4)}
      {@const [x2, y2] = polar(ang(u), Math.round(u * 20) % 2 === 0 ? R + 9 : R + 7)}
      <line class="tick" x1={x1} y1={y1} x2={x2} y2={y2} />
    {/each}
    {#each marks as m (m)}
      {@const [x, y] = polar(ang(turn(m)), R + 15)}
      <text class="mk" {x} y={y + 3} text-anchor="middle">{m}</text>
    {/each}
    <circle class="skirt" cx="60" cy="60" r={R - 4} />
    <circle class="cap" cx="60" cy="60" r={R - 14} />
    {#each Array.from({ length: 24 }, (_, i) => i * 15) as g (g)}
      {@const [x1, y1] = polar(g, R - 14)}
      {@const [x2, y2] = polar(g, R - 5)}
      <line class="grip" x1={x1} y1={y1} x2={x2} y2={y2} />
    {/each}
    <line class="pointer" x1={tip[2]} y1={tip[3]} x2={tip[0]} y2={tip[1]} />
  </svg>
  <span class="name">{label ?? param.name}</span>
  <div class="box"><ValueField {param} {value} {oninput} {oncommit} /></div>
</div>

<style>
  .bk {
    display: grid;
    justify-items: center;
    gap: 2px;
  }
  svg {
    cursor: ns-resize;
    touch-action: none;
    overflow: visible;
  }
  svg:focus-visible {
    outline: 1px solid var(--accent);
    border-radius: 50%;
  }
  .tick {
    stroke: var(--panel-ink, #e9dcc0);
    stroke-width: 1.2;
  }
  .mk {
    fill: var(--panel-ink, #e9dcc0);
    font-size: 9px;
    font-weight: 600;
  }
  .skirt {
    fill: #1b1a19;
    stroke: #000;
    stroke-width: 1.5;
  }
  .cap {
    fill: #2c2a28;
    stroke: #3c3936;
  }
  .grip {
    stroke: #0c0b0b;
    stroke-width: 2.5;
  }
  .pointer {
    stroke: var(--panel-accent, #f2c14e);
    stroke-width: 3.5;
    stroke-linecap: round;
  }
  .name {
    color: var(--panel-ink, #e9dcc0);
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }
  .box {
    width: 72px;
  }
</style>
