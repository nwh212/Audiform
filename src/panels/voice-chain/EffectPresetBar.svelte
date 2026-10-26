<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The Audiform contributors.
  This file is part of Audiform, based on Altered Audio Editor.

  An effect window's presets: pick one to apply it (Default puts every
  setting back), Save As keeps the current settings under a name, and
  Delete removes the picked one. Built-in effects keep their presets in
  the preferences. VST3 plug-ins use preset files in the standard VST3
  preset folders, the same ones other VST3 programs use, so presets saved
  for a plug-in elsewhere show up here too. Applying one is an undoable
  edit like any other.
-->
<script lang="ts">
  import { pluginPresetDelete, pluginPresetRead, pluginPresets, pluginPresetSave, type EffectInfo, type PluginPresetFile } from "../../app/core";
  import type { Preferences } from "../../stores/preferences";
  import type { EffectSlotDoc } from "../../stores/session";
  import { deletePreset, KEY_EFFECT_PRESETS, presetChange, sanitizeEffectPresets, savePreset, type EffectPresets } from "./effect-presets";

  interface Props {
    slot: EffectSlotDoc;
    info: EffectInfo;
    prefs: Preferences;
    ask: (title: string, label: string, value: string) => Promise<string | null>;
    /** Applies a change to the slot as one undoable edit. */
    onChange: (change: Partial<EffectSlotDoc>, label: string) => void;
    onNotice?: (text: string) => void;
    onError?: (e: unknown) => void;
  }

  const { slot, info, prefs, ask, onChange, onNotice = () => {}, onError = console.error }: Props = $props();

  const plugin = $derived(!!info.plugin);
  let mine = $state<EffectPresets>({});
  // svelte-ignore state_referenced_locally
  void prefs.load().then(() => (mine = sanitizeEffectPresets(prefs.raw(KEY_EFFECT_PRESETS))));
  let files = $state<PluginPresetFile[]>([]);
  async function loadFiles() {
    files = plugin ? await pluginPresets(info.id).catch(() => []) : [];
  }
  $effect(() => {
    void info.id;
    void loadFiles();
  });

  const names = $derived(plugin ? files.map((f) => f.name) : (mine[info.id] ?? []).map((p) => p.name));
  const current = $derived(slot.preset && names.includes(slot.preset) ? slot.preset : "");
  const canDelete = $derived(!!current && (!plugin || files.some((f) => f.name === current && f.own)));
  const defaults = $derived(Object.fromEntries(info.params.map((p) => [p.key, p.default])));

  async function choose(name: string) {
    if (name === "\u0000default") {
      onChange({ params: { ...defaults }, data: null, preset: null }, `${info.name}: Default Settings`);
      return;
    }
    if (plugin) {
      const f = files.find((x) => x.name === name);
      if (!f) return;
      try {
        const state = await pluginPresetRead(f.path);
        // The preset's settings replace the plug-in's; its controls follow from them.
        onChange({ plugin_state: state, params: {}, preset: f.name }, `${info.name} Preset: ${f.name}`);
      } catch (e) {
        onError(e);
      }
      return;
    }
    const p = (mine[info.id] ?? []).find((x) => x.name === name);
    if (p) onChange(presetChange(p, defaults), `${info.name} Preset: ${p.name}`);
  }

  async function saveAs() {
    const name = (await ask(`Save ${info.name} Preset`, "Preset name", current || ""))?.trim();
    if (!name) return;
    try {
      if (plugin) {
        await pluginPresetSave($state.snapshot(slot) as EffectSlotDoc, name);
        await loadFiles();
      } else {
        mine = savePreset(mine, $state.snapshot(slot) as EffectSlotDoc, name);
        prefs.set(KEY_EFFECT_PRESETS, $state.snapshot(mine));
      }
      onChange({ preset: name }, `Save ${info.name} Preset`);
      onNotice(`Saved preset ${name}.`);
    } catch (e) {
      onError(e);
    }
  }

  async function remove() {
    if (!canDelete) return;
    const name = current;
    try {
      if (plugin) {
        const f = files.find((x) => x.name === name && x.own);
        if (!f) return;
        await pluginPresetDelete(f.path);
        await loadFiles();
      } else {
        mine = deletePreset(mine, info.id, name);
        prefs.set(KEY_EFFECT_PRESETS, $state.snapshot(mine));
      }
      onNotice(`Deleted preset ${name}.`);
    } catch (e) {
      onError(e);
    }
  }
</script>

<div class="presets">
  <select
    aria-label="Presets"
    title={plugin ? "Presets from the standard VST3 preset folders, shared with other VST3 programs" : "This effect's saved presets"}
    value={current}
    onchange={(e) => {
      const v = e.currentTarget.value;
      e.currentTarget.value = current;
      void choose(v);
    }}
  >
    <option value="" disabled>{names.length ? "Presets…" : "No presets yet"}</option>
    {#if !plugin}<option value={"\u0000default"}>Default settings</option>{/if}
    {#if names.length}
      <optgroup label={plugin ? "Preset files" : "Saved"}>
        {#each names as n (n)}<option value={n}>{n}</option>{/each}
      </optgroup>
    {/if}
  </select>
  <button type="button" title="Save the current settings as a new preset (or replace one of the same name)" onclick={() => void saveAs()}>Save As…</button>
  <button type="button" disabled={!canDelete} title={canDelete ? `Delete preset ${current}` : plugin && current ? "Only presets in your own preset folder can be deleted here" : "Pick a preset to delete it"} onclick={() => void remove()}>Delete</button>
</div>

<style>
  .presets {
    display: flex;
    gap: 4px;
    align-items: center;
    margin-bottom: 6px;
  }
  select {
    flex: 1;
    min-width: 0;
  }
</style>
