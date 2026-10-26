// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Dragging effects between numbered slots: within a chain to move one,
 * onto another track's (or bus's) chain to copy it with every setting,
 * plug-in settings included. Built on pointer events rather than the
 * browser's drag and drop, which the window's file drop turns off on
 * Windows. Slot rows mark themselves with `data-slot-owner` (an owner
 * key, see `ownerKey`) and `data-slot-index`.
 *
 * Also the Add effect menu, shared by the Audio FX chain and the Editor's
 * track headers.
 */

import { writable } from "svelte/store";
import type { EffectInfo } from "../../app/core";
import type { EffectChainDoc, EffectSlotDoc, EffectTarget, Op, SessionDoc } from "../../stores/session";
import { MASTER_ID } from "../mixer/routing";
import { addSlotAt, chainOf, moveSlotToIndex, pluginMenuGroups, type MenuPlugin } from "./chain";

/** The same shape as the Editor's context menu items. */
export type MenuItem =
  | { kind: "item"; label: string; run: () => void; disabled?: boolean; reason?: string | null }
  | { kind: "sub"; label: string; items: readonly MenuItem[]; disabled?: boolean }
  | { kind: "sep" };

/** One string per chain, for the `data-slot-owner` attribute. */
export function ownerKey(t: EffectTarget): string {
  return t.kind === "master" ? "master" : `${t.kind}:${t.id}`;
}

export function parseOwner(key: string): EffectTarget | null {
  if (key === "master") return { kind: "master" };
  const [kind, id] = key.split(":", 2) as [string, string | undefined];
  if (!id || (kind !== "track" && kind !== "bus" && kind !== "clip")) return null;
  return { kind, id };
}

/** The chain a target names in `doc`. */
export function chainAt(doc: SessionDoc, t: EffectTarget): EffectChainDoc | null {
  if (t.kind === "master") return chainOf(doc.master?.effects);
  if (t.kind === "track") {
    const tr = doc.tracks.find((x) => x.id === t.id);
    return tr ? chainOf(tr.effects) : null;
  }
  if (t.kind === "bus") {
    const b = doc.buses?.find((x) => x.id === t.id);
    return b ? chainOf(b.effects) : null;
  }
  for (const tr of doc.tracks) for (const c of tr.clips) if (c.id === t.id) return chainOf(c.effects);
  return null;
}

/** A drop: where it lands. */
export interface SlotSpot {
  owner: string;
  index: number;
}

/** The slot under the pointer during a drag, for highlighting. */
export const slotOver = writable<SlotSpot | null>(null);

/**
 * The edit a drop makes: within the same chain the effect moves; onto
 * another chain a copy (a new id, the same settings) goes into that slot.
 * Null when nothing changes or the target chain is full.
 */
export function dropEdit(doc: SessionDoc, from: EffectTarget, slot: EffectSlotDoc, to: SlotSpot, newId: () => string): { label: string; op: Op } | null {
  const target = parseOwner(to.owner);
  if (!target) return null;
  const chain = chainAt(doc, target);
  if (!chain) return null;
  if (ownerKey(from) === to.owner) {
    const next = moveSlotToIndex(chain, slot.id, to.index);
    if (JSON.stringify(next.slots) === JSON.stringify(chain.slots)) return null;
    return { label: "Move Effect", op: { op: "set_effects", target, chain: next } };
  }
  const next = addSlotAt(chain, { ...(JSON.parse(JSON.stringify(slot)) as typeof slot), id: newId(), sidechain: target.kind === "track" && slot.sidechain === target.id ? null : (slot.sidechain ?? null) }, to.index);
  if (!next) return null;
  return { label: "Copy Effect", op: { op: "set_effects", target, chain: next } };
}

/** The numbered slot under a point, if any. */
function spotAt(x: number, y: number): SlotSpot | null {
  const el = document.elementFromPoint(x, y)?.closest<HTMLElement>("[data-slot-owner][data-slot-index]");
  if (!el) return null;
  const index = Number(el.dataset.slotIndex);
  return Number.isFinite(index) ? { owner: el.dataset.slotOwner!, index } : null;
}

/**
 * Starts dragging an effect from a slot row. A press that does not move
 * calls `onClick` instead. `onDrop` gets the slot it was let go over.
 */
export function startSlotDrag(e: PointerEvent, onDrop: (to: SlotSpot) => void, onClick?: () => void): void {
  if (e.button !== 0) return;
  if ((e.target as HTMLElement).closest("button, select, input")) return;
  e.preventDefault();
  const x0 = e.clientX;
  const y0 = e.clientY;
  let moving = false;
  const move = (ev: PointerEvent) => {
    if (!moving && Math.abs(ev.clientX - x0) < 4 && Math.abs(ev.clientY - y0) < 4) return;
    if (!moving) document.body.style.cursor = "grabbing";
    moving = true;
    slotOver.set(spotAt(ev.clientX, ev.clientY));
  };
  const up = (ev: PointerEvent) => {
    window.removeEventListener("pointermove", move, true);
    window.removeEventListener("pointerup", up, true);
    document.body.style.cursor = "";
    slotOver.set(null);
    if (!moving) {
      onClick?.();
      return;
    }
    const spot = spotAt(ev.clientX, ev.clientY);
    if (spot) onDrop(spot);
  };
  window.addEventListener("pointermove", move, true);
  window.addEventListener("pointerup", up, true);
}

/** The Add effect menu: built-in effects by family, then VST3 plug-ins by type and maker. */
export function effectMenu(catalog: readonly EffectInfo[], plugins: readonly MenuPlugin[], pick: (id: string) => void): MenuItem[] {
  catalog = catalog.filter((e) => !e.retired);
  const categories = [...new Set(catalog.map((e) => e.category))];
  const byId = new Set(catalog.map((e) => e.id));
  const installed = plugins.filter((p) => byId.has(p.effect));
  const list = (ps: MenuPlugin[]): MenuItem[] => ps.map((p) => ({ kind: "item", label: p.name, run: () => pick(p.effect) }));
  let vst: MenuItem;
  if (!installed.length) {
    vst = { kind: "item", label: "VST3", disabled: true, reason: "No VST3 plug-ins found yet. Scan for them in Effects > Plug-in Manager.", run: () => {} };
  } else {
    const { types, makers } = pluginMenuGroups(installed);
    vst = {
      kind: "sub",
      label: "VST3",
      items: [...types.map(([t, ps]): MenuItem => ({ kind: "sub", label: t, items: list(ps) })), { kind: "sep" }, ...makers.map(([m, ps]): MenuItem => ({ kind: "sub", label: m, items: list(ps) }))],
    };
  }
  return [
    ...categories
      .filter((c) => !catalog.some((e) => e.category === c && e.plugin))
      .map((c): MenuItem => ({ kind: "sub", label: c, items: catalog.filter((e) => e.category === c).map((e): MenuItem => ({ kind: "item", label: e.name, run: () => pick(e.id) })) })),
    { kind: "sep" },
    { kind: "item", label: "VST", disabled: true, reason: "Not supported: Steinberg no longer licenses VST2. Most plug-ins also come as VST3.", run: () => {} },
    vst,
  ];
}

/** The effect target of a header row: a track or bus by id, or the master. */
export function rowTarget(id: string, bus: boolean): EffectTarget {
  return id === MASTER_ID ? { kind: "master" } : bus ? { kind: "bus", id } : { kind: "track", id };
}
