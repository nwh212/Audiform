<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  A track's or bus's FX Chain inside its header (the Effects view):
  the chain's power, then the numbered slots, each with its power and
  name. Click a name to open the effect, the arrow (or an empty slot) to
  add or replace one. Drag an effect to another slot to move it, or onto
  another track's slots to copy it with every setting. Every change is
  one undoable step.
-->
<script lang="ts">
  import type { EffectInfo } from "../../app/core";
  import type { EffectChainDoc, EffectSlotDoc, EffectTarget, Op, SessionDoc } from "../../stores/session";
  import { addSlotAt, chainOf, layoutSlots, newSlot, removeSlot, replaceSlot, setSlot, type MenuPlugin } from "../voice-chain/chain";
  import { dropEdit, effectMenu, ownerKey, slotOver, startSlotDrag, type MenuItem, type SlotSpot } from "../voice-chain/slot-drag";
  import { stoppedPlugins } from "../../stores/plugins";

  interface Props {
    doc: SessionDoc;
    target: EffectTarget;
    chain: EffectChainDoc | undefined;
    catalog: readonly EffectInfo[];
    plugins: readonly MenuPlugin[];
    onEdit: (label: string, op: Op) => void;
    onOpen: (slot: EffectSlotDoc) => void;
    onMenu: (x: number, y: number, items: MenuItem[]) => void;
  }

  const { doc, target, chain: raw, catalog, plugins, onEdit, onOpen, onMenu }: Props = $props();

  const chain = $derived(chainOf(raw));
  const owner = $derived(ownerKey(target));
  const rows = $derived(layoutSlots(chain));
  const byId = $derived(new Map(catalog.map((e) => [e.id, e])));

  function commit(label: string, next: EffectChainDoc | null) {
    if (next) onEdit(label, { op: "set_effects", target, chain: next });
  }

  function addAt(id: string, at: number) {
    const info = byId.get(id);
    if (!info) return;
    const s = newSlot(info, crypto.randomUUID());
    commit(`Add ${info.name}`, addSlotAt(chain, s, at));
  }

  function menuFor(e: MouseEvent, s: EffectSlotDoc | null, at: number) {
    e.preventDefault();
    e.stopPropagation();
    if (!s) {
      onMenu(e.clientX, e.clientY, effectMenu(catalog, plugins, (id) => addAt(id, at)));
      return;
    }
    const info = byId.get(s.effect);
    onMenu(e.clientX, e.clientY, [
      { kind: "item", label: info?.plugin && info.hasWindow ? "Open Plug-in Window" : "Edit Effect", run: () => onOpen(s) },
      { kind: "item", label: s.enabled ? "Bypass" : "Turn On", run: () => commit(s.enabled ? "Bypass Effect" : "Turn On Effect", setSlot(chain, s.id, { enabled: !s.enabled })) },
      { kind: "item", label: "Remove Effect", run: () => commit(`Remove ${info?.name ?? "Effect"}`, removeSlot(chain, s.id)) },
      { kind: "sep" },
      {
        kind: "sub",
        label: "Replace With",
        items: effectMenu(catalog, plugins, (id) => {
          const ni = byId.get(id);
          if (ni) commit(`Replace with ${ni.name}`, replaceSlot(chain, s.id, { ...newSlot(ni, crypto.randomUUID()), position: s.position ?? null }));
        }),
      },
    ]);
  }

  function drop(s: EffectSlotDoc, to: SlotSpot) {
    const ed = dropEdit(doc, target, s, to, () => crypto.randomUUID());
    if (ed) onEdit(ed.label, ed.op);
  }

  const stop = (e: Event) => e.stopPropagation();
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="hfx" onclick={stop} ondblclick={stop}>
  <div class="top">
    <span class="fx" aria-hidden="true">fx</span>
    <button
      type="button"
      class="pow"
      class:on={chain.enabled}
      title={chain.enabled ? "Effects on: click to bypass the whole chain" : "Effects bypassed: click to turn them on"}
      aria-pressed={chain.enabled}
      onclick={() => commit(chain.enabled ? "Bypass FX Chain" : "Turn On FX Chain", { ...chain, enabled: !chain.enabled })}
    >
      <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M8 2v6" stroke="currentColor" stroke-width="1.8" /><path d="M4.6 4.4a5 5 0 1 0 6.8 0" fill="none" stroke="currentColor" stroke-width="1.6" /></svg>
    </button>
    <span class="count">{chain.slots.length ? `${chain.slots.length} ${chain.slots.length === 1 ? "effect" : "effects"}` : "No effects"}</span>
  </div>
  <!-- The wheel here scrolls the tracks, as it does over the rest of the
       header; this list scrolls by its own scrollbar. -->
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <ol
    class="slots"
    class:off={!chain.enabled}
    onwheel={(e) => {
      if (e.ctrlKey || e.altKey || e.shiftKey) return;
      const tracks = (e.currentTarget as HTMLElement).closest<HTMLElement>(".scroller");
      if (!tracks) return;
      e.preventDefault();
      // Handled here: over the rest of a header the wheel zooms the tracks.
      e.stopPropagation();
      tracks.scrollTop += e.deltaMode === 1 ? e.deltaY * 16 : e.deltaY;
    }}
  >
    {#each rows as s, i (i)}
      {@const info = s ? byId.get(s.effect) : undefined}
      <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
      <li
        class:bypassed={!!s && !s.enabled}
        class:over={$slotOver?.owner === owner && $slotOver.index === i}
        class:filled={!!s}
        data-slot-owner={owner}
        data-slot-index={i}
        title={s ? `${info?.name ?? s.effect}. Click to open; drag to move, or onto another track to copy.` : `Slot ${i + 1}: click to add an effect`}
        onpointerdown={(e) => {
          e.stopPropagation();
          if (s) startSlotDrag(e, (to) => drop(s, to), () => onOpen(s));
        }}
        onclick={(e) => !s && menuFor(e, null, i)}
        oncontextmenu={(e) => menuFor(e, s, i)}
      >
        <span class="n">{i + 1}</span>
        {#if s}
          <button
            type="button"
            class="pow"
            class:on={s.enabled}
            title={s.enabled ? "On: click to bypass" : "Off: click to turn on"}
            aria-pressed={s.enabled}
            onclick={() => commit(s.enabled ? "Bypass Effect" : "Turn On Effect", setSlot(chain, s.id, { enabled: !s.enabled }))}
          >
            <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M8 2v6" stroke="currentColor" stroke-width="1.8" /><path d="M4.6 4.4a5 5 0 1 0 6.8 0" fill="none" stroke="currentColor" stroke-width="1.6" /></svg>
          </button>
          <span class="name" class:stopped={$stoppedPlugins.has(s.id)}>{info?.name ?? (s.effect.startsWith("vst3:") ? "Missing plug-in" : s.effect)}</span>
        {:else}
          <span class="name empty"></span>
        {/if}
        <button type="button" class="arrow" title={s ? "Replace, bypass or remove" : "Add an effect here"} aria-label={s ? "Effect menu" : "Add an effect"} onclick={(e) => menuFor(e, s, i)}>&rsaquo;</button>
      </li>
    {/each}
  </ol>
</div>

<style>
  .hfx {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-height: 0;
    margin: 2px 22px 4px 6px;
    border: 1px solid var(--border);
    border-radius: 2px;
    background: var(--surface-0, rgba(0, 0, 0, 0.2));
    font-size: var(--font-size-small);
  }
  .top {
    display: flex;
    flex: none;
    gap: 4px;
    align-items: center;
    height: 20px;
    padding: 0 4px;
    border-bottom: 1px solid var(--border);
  }
  .fx {
    color: var(--text-dim);
    font-family: serif;
    font-style: italic;
  }
  .count {
    margin-left: auto;
    color: var(--text-faint);
  }
  .slots {
    flex: 1;
    min-height: 0;
    margin: 0;
    padding: 0;
    overflow-y: auto;
    list-style: none;
  }
  .slots.off {
    opacity: 0.55;
  }
  li {
    display: flex;
    gap: 2px;
    align-items: center;
    height: 18px;
    padding: 0 2px 0 4px;
    border-bottom: 1px solid color-mix(in srgb, var(--border) 50%, transparent);
    cursor: pointer;
  }
  li.filled {
    cursor: grab;
  }
  li.over {
    box-shadow: inset 0 2px 0 var(--accent);
  }
  li.bypassed .name {
    color: var(--text-faint);
    text-decoration: line-through;
  }
  .n {
    width: 14px;
    color: var(--text-faint);
    font-family: var(--font-mono);
    text-align: right;
  }
  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    color: var(--text);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .name.stopped {
    color: var(--danger);
  }
  .pow {
    display: grid;
    flex: none;
    place-items: center;
    width: 16px;
    height: 16px;
    padding: 0;
    border: none;
    background: none;
    color: var(--text-faint);
    cursor: pointer;
  }
  .pow svg {
    width: 12px;
    height: 12px;
  }
  .pow.on {
    color: #5fd38d;
  }
  .arrow {
    flex: none;
    width: 14px;
    height: 16px;
    padding: 0;
    border: none;
    background: none;
    color: var(--text-dim);
    cursor: pointer;
  }
</style>
