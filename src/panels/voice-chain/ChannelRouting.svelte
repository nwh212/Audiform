<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The Audiform contributors.
  This file is part of Audiform, based on Altered Audio Editor.

  An effect's channel routing: for each of the effect's inputs, the
  chain channel it hears, and for each output, the chain channel it
  replaces. Chain channels no output goes to pass through untouched. By
  default channels go by speaker name, so a stereo effect works on L and
  R, and a 5.1 plug-in on a 7.1.4 master works on its 5.1 speakers.
  Plug-ins can run with their own channel layout instead of stereo.
  Routings can be saved as presets and used on any effect.
-->
<script lang="ts">
  import { effectChannels, type EffectInfo } from "../../app/core";
  import type { Preferences } from "../../stores/preferences";
  import type { EffectSlotDoc, SlotRoutingDoc } from "../../stores/session";
  import { describe, KEY_ROUTING_PRESETS, resolve, sanitizeRoutingPresets, STEREO, type RoutingPreset } from "./routing";

  interface Props {
    slot: EffectSlotDoc;
    info: EffectInfo;
    /** The chain's channels by name: L and R, or a 3D Space master's speakers. */
    chain: string[];
    prefs: Preferences;
    ask: (title: string, label: string, value: string) => Promise<string | null>;
    onChange: (routing: SlotRoutingDoc | null, label: string) => void;
    onNotice?: (text: string) => void;
  }

  const { slot, info, chain, prefs, ask, onChange, onNotice = () => {} }: Props = $props();

  let names = $state<{ inputs: string[]; outputs: string[] }>({ inputs: STEREO, outputs: STEREO });
  // The plug-in's channels follow whether it runs with its own layout.
  $effect(() => {
    const s = $state.snapshot(slot) as EffectSlotDoc;
    void s.routing?.wide;
    if (!info.plugin) {
      names = { inputs: STEREO, outputs: STEREO };
      return;
    }
    void effectChannels(s).then((n) => (names = n)).catch(() => {});
  });
  const map = $derived(resolve(slot.routing, names.inputs, names.outputs, chain));
  const custom = $derived(!!(slot.routing?.inputs?.length || slot.routing?.outputs?.length));

  let presets = $state<RoutingPreset[]>([]);
  // svelte-ignore state_referenced_locally
  void prefs.load().then(() => (presets = sanitizeRoutingPresets(prefs.raw(KEY_ROUTING_PRESETS))));

  function tidy(r: SlotRoutingDoc): SlotRoutingDoc | null {
    const out: SlotRoutingDoc = {};
    if (r.wide) out.wide = true;
    if (r.inputs?.length) out.inputs = r.inputs;
    if (r.outputs?.length) out.outputs = r.outputs;
    return Object.keys(out).length ? out : null;
  }
  function setChannel(side: "inputs" | "outputs", k: number, value: string) {
    const cur = { inputs: [...map.inputs], outputs: [...map.outputs] };
    cur[side][k] = value === "" ? null : Number(value);
    onChange(tidy({ wide: slot.routing?.wide, ...cur }), "Effect Channel Routing");
  }
  function byNames() {
    onChange(tidy({ wide: slot.routing?.wide }), "Effect Channel Routing");
  }
  function setWide(on: boolean) {
    // Its channels change, so the routing goes back to speaker names.
    onChange(tidy({ wide: on }), on ? "Plug-in Uses Its Own Channels" : "Plug-in Uses Stereo");
  }
  async function savePreset() {
    const name = (await ask("Save Routing Preset", "Preset name", ""))?.trim();
    if (!name) return;
    const p: RoutingPreset = { name, routing: { wide: !!slot.routing?.wide, inputs: [...map.inputs], outputs: [...map.outputs] } };
    presets = [...presets.filter((x) => x.name !== name), p].sort((a, b) => a.name.localeCompare(b.name));
    prefs.set(KEY_ROUTING_PRESETS, $state.snapshot(presets));
    onNotice(`Saved routing preset ${name}.`);
  }
  let picked = $state("");
  function applyPreset(name: string) {
    const p = presets.find((x) => x.name === name);
    if (!p) return;
    onChange(tidy({ ...p.routing, wide: info.plugin ? p.routing.wide : false }), `Routing Preset: ${name}`);
  }
  function deletePreset() {
    if (!picked) return;
    presets = presets.filter((x) => x.name !== picked);
    prefs.set(KEY_ROUTING_PRESETS, $state.snapshot(presets));
    onNotice(`Deleted routing preset ${picked}.`);
    picked = "";
  }
</script>

<div class="routing">
  {#if info.plugin}
    <label class="wide" title="Run the plug-in with its own channel layout (5.1, 7.1.4 and so on) instead of stereo. Plug-ins added to the master of a 3D Space session start this way.">
      <input type="checkbox" checked={!!slot.routing?.wide} onchange={(e) => setWide(e.currentTarget.checked)} />
      Use the plug-in's own channels
    </label>
  {/if}
  <div class="grid">
    {#each [["inputs", "Hears", names.inputs], ["outputs", "Writes to", names.outputs]] as const as [side, title, list] (side)}
      <div class="col">
        <span class="head">{side === "inputs" ? "Effect input" : "Effect output"} → {title}</span>
        {#each list as n, k (k)}
          <label class="row">
            <span class="name">{n}</span>
            <select value={map[side][k] === null ? "" : String(map[side][k])} onchange={(e) => setChannel(side, k, e.currentTarget.value)}>
              <option value="">{side === "inputs" ? "Silence" : "Nowhere"}</option>
              {#each chain as c, i (i)}<option value={String(i)}>{c}</option>{/each}
            </select>
          </label>
        {/each}
      </div>
    {/each}
  </div>
  <p class="hint">Channels no output goes to pass through untouched.{custom ? "" : " Channels are matched by speaker name."}</p>
  <div class="bar">
    <button type="button" disabled={!custom} title="Match the effect's channels to the chain's by speaker name" onclick={byNames}>Match by Name</button>
    <span class="spacer"></span>
    <select aria-label="Routing presets" bind:value={picked} onchange={() => applyPreset(picked)}>
      <option value="" disabled>{presets.length ? "Routing presets…" : "No routing presets"}</option>
      {#each presets as p (p.name)}<option value={p.name}>{p.name}</option>{/each}
    </select>
    <button type="button" title="Save this routing as a preset for any effect" onclick={() => void savePreset()}>Save…</button>
    <button type="button" disabled={!picked} title="Delete the picked routing preset" onclick={deletePreset}>Delete</button>
  </div>
</div>
<p class="io">{describe(map, chain)}</p>

<style>
  .routing {
    display: grid;
    gap: 6px;
    margin-bottom: 8px;
    padding: 8px;
    border: 1px solid var(--border);
    border-radius: 3px;
    background: var(--surface-1);
  }
  .wide {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }
  .col {
    display: grid;
    gap: 3px;
    align-content: start;
  }
  .head {
    color: var(--text-dim);
    font-size: var(--font-size-small);
  }
  .row {
    display: grid;
    grid-template-columns: 3.5em 1fr;
    gap: 6px;
    align-items: center;
  }
  .name {
    font-family: var(--font-mono);
    font-size: var(--font-size-small);
  }
  .bar {
    display: flex;
    gap: 4px;
    align-items: center;
  }
  .spacer {
    flex: 1;
  }
  .hint,
  .io {
    margin: 0;
    color: var(--text-dim);
    font-size: var(--font-size-small);
  }
  .io {
    margin: 4px 0 0;
  }
</style>
