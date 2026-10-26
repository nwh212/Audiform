<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  A track's level meter: left and right bars (or one bar per speaker
  channel, for the master of a 3D Space session) on a -60 to 0 dB scale,
  green to -12 dB, yellow to -3 dB, red above, with a held peak line and
  an over light per channel that stays lit until clicked. While a track
  is armed it shows the input level instead, on both bars.
-->
<script lang="ts">
  import { meterFraction, type MeterLevel } from "../../stores/transport";

  interface Props {
    levels: readonly MeterLevel[] | null;
    armed: boolean;
    /** Channel names, shown on hover, when there are more than two bars. */
    names?: readonly string[];
    /** Clears the over lights. */
    onClear: () => void;
  }

  const { levels, armed, onClear, names }: Props = $props();
  const chans = $derived(Array.from({ length: Math.max(2, levels?.length ?? 2) }, (_, k) => k));
</script>

<div
  class="meter"
  class:armed
  style:--n={chans.length}
  style:width={chans.length > 2 ? `${chans.length * 3 + 5}px` : undefined}
  title={armed ? "Input level" : chans.length > 2 && names ? `Speaker levels: ${names.join(" ")}. Click a red light to clear it.` : "Track level. Click a red light to clear it."}
>
  <div class="overs">
    {#each chans as ch (ch)}
      <button
        type="button"
        class="over"
        class:on={levels?.[ch]?.over}
        aria-label="Clear over lights"
        tabindex="-1"
        onclick={(e) => {
          e.stopPropagation();
          onClear();
        }}
      ></button>
    {/each}
  </div>
  <div class="bars" aria-hidden="true">
    {#each chans as ch (ch)}
      {@const m = levels?.[ch]}
      <div class="bar" title={names?.[ch]}>
        <div class="cover" style:height="{(1 - meterFraction(m?.level ?? 0)) * 100}%"></div>
        {#if m && m.peak > 0.001}<div class="peak" style:bottom="{meterFraction(m.peak) * 100}%"></div>{/if}
      </div>
    {/each}
  </div>
</div>

<style>
  .meter {
    position: absolute;
    top: 5px;
    right: 4px;
    bottom: 8px;
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: 15px;
  }

  .overs,
  .bars {
    display: grid;
    grid-template-columns: repeat(var(--n, 2), 1fr);
    gap: 1px;
  }

  .bars {
    flex: 1;
    min-height: 0;
    padding: 1px;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    background: var(--surface-0);
  }

  .meter.armed .bars {
    border-color: color-mix(in srgb, var(--danger) 60%, var(--border-strong));
  }

  /* -60 to 0 dB: -12 dB sits at 80%, -3 dB at 95%. The cover hides the
     part of the scale above the current level. */
  .bar {
    position: relative;
    overflow: hidden;
    background: linear-gradient(to top, #3d8f6a 0 var(--meter-yellow, 80%), #c9b33f var(--meter-yellow, 80%) var(--meter-red, 95%), #e5534b var(--meter-red, 95%) 100%);
  }

  .cover {
    position: absolute;
    top: 0;
    right: 0;
    left: 0;
    background: var(--surface-0);
  }

  .peak {
    position: absolute;
    right: 0;
    left: 0;
    height: 2px;
    background: var(--text);
    transform: translateY(1px);
  }

  .over {
    height: 6px;
    padding: 0;
    border: 1px solid var(--border-strong);
    border-radius: 1px;
    background: var(--surface-0);
    cursor: pointer;
  }

  .over.on {
    border-color: var(--danger);
    background: var(--danger);
  }
</style>
