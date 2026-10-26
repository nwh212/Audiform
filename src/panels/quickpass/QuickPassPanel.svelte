<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  QuickPass: scans the selected clips (or every clip on a track that
  plays when none are selected) for silences. The Scan view lists what
  was found; click a row to select it and jump to it, then fix the
  selected rows or all of them. The Settings view holds the two levels,
  what fixing does, and saved presets. Nothing changes until you press
  Fix, and every change can be undone.
-->
<script lang="ts">
  import { quickpassLevels, quickpassSpots, type ClipLevels, type SpotsRequest } from "../../app/core";
  import { editor } from "../../stores/editor";
  import type { CommandRunner } from "../../stores/media-pool";
  import type { Preferences } from "../../stores/preferences";
  import type { SessionDoc, SessionStore } from "../../stores/session";
  import type { TransportStore } from "../../stores/transport";
  import { formatIn } from "../editor/timeline";
  import {
    BUILTIN_PRESETS,
    clipsOn,
    cutRange,
    DEFAULTS,
    duckOp,
    findAll,
    scanInfo,
    SCAN_TYPES,
    markersFor,
    removeOp,
    sanitizeSettings,
    shiftFindings,
    splitOp,
    spotFindings,
    spotsRequest,
    suggestLevels,
    type Finding,
    type Preset,
    type QuickPassSettings,
    type ScanType,
  } from "./quickpass";

  interface Props {
    session: SessionStore;
    transport: TransportStore;
    runner: CommandRunner;
    prefs: Preferences;
    ask: (title: string, label: string, value: string) => Promise<string | null>;
    /** Writes repaired copies for the findings' clips; returns the ids of findings it fixed. */
    onRepair: (spots: SpotsRequest, list: Finding[]) => Promise<string[]>;
    onNotice: (text: string) => void;
    onError: (e: unknown) => void;
    /** The Waveform editor's file, its transport and its undo history: scanned while that editor is in front. */
    wave?: SessionStore;
    waveTransport?: TransportStore;
    waveRunner?: CommandRunner;
  }

  const { session, transport, runner, prefs, ask, onRepair, onNotice, onError, wave, waveTransport, waveRunner }: Props = $props();

  const KEY = "quickpass.settings";
  const PRESETS_KEY = "quickpass.presets";
  const WINDOW_MS = 10;
  const VERB = { repair: "Repair", remove: "Strip", duck: "Turn Down", split: "Split", mark: "Mark" } as const;

  let s = $state<QuickPassSettings>({ ...DEFAULTS });
  let mine = $state<Preset[]>([]);
  let preset = $state("Default");
  let loaded = $state(false);
  // svelte-ignore state_referenced_locally
  void prefs.load().then(() => {
    s = sanitizeSettings(prefs.raw(KEY));
    const raw = prefs.raw(PRESETS_KEY);
    mine = Array.isArray(raw)
      ? raw.flatMap((p: unknown) => {
          const r = p as { name?: unknown; settings?: unknown };
          return typeof r?.name === "string" && r.name.trim() ? [{ name: r.name, settings: sanitizeSettings(r.settings) }] : [];
        })
      : [];
    const saved = prefs.raw("quickpass.preset");
    if (typeof saved === "string") preset = saved;
    loaded = true;
  });
  $effect(() => {
    const snap = $state.snapshot(s);
    const list = $state.snapshot(mine);
    const name = preset;
    if (!loaded) return;
    prefs.set(KEY, snap);
    prefs.set(PRESETS_KEY, list);
    prefs.set("quickpass.preset", name);
  });

  const allPresets = $derived([...BUILTIN_PRESETS, ...mine]);
  // The preset list shows the presets for the chosen scan type.
  const shownPresets = $derived(allPresets.filter((p) => p.settings.scan === s.scan));
  const info = $derived(scanInfo(s.scan));
  const one = $derived(info.noun[0]);
  const many = $derived(info.noun[1]);
  /** Short names for the Fix by menu. */
  const FIX_BY = { repair: "Repair", remove: "Strip (leave a gap)", duck: "Turn Down", split: "Split", mark: "Mark" } as const;
  const ACTION_TEXT = {
    repair: "Repairing it (the clip gets a repaired copy of its audio)",
    remove: "Stripping it (the audio goes, the gap stays so tracks stay in step)",
    duck: "Turning it down (clips stay whole)",
    split: "Splitting at it",
    mark: "Marking it",
  } as const;

  function chooseScan(scan: ScanType) {
    if (scan === s.scan) return;
    // Switch to that type's first preset, so the settings make sense for it.
    const first = BUILTIN_PRESETS.find((p) => p.settings.scan === scan);
    if (first) choosePreset(first.name);
    else s = sanitizeSettings({ ...s, scan });
    found = [];
    done = {};
    scanned = false;
  }
  const isBuiltin = $derived(BUILTIN_PRESETS.some((p) => p.name === preset));
  const known = $derived(allPresets.some((p) => p.name === preset && p.settings.scan === s.scan));

  let sessionDoc = $state<SessionDoc | null>(null);
  $effect(() =>
    session.subscribe((d) => {
      sessionDoc = d;
    }),
  );
  let waveDoc = $state<SessionDoc | null>(null);
  $effect(() =>
    wave?.subscribe((d) => {
      waveDoc = d;
    }),
  );
  let selectedClips = $state<ReadonlySet<string>>(new Set());
  let waveMode = $state(false);
  $effect(() =>
    editor.subscribe((e) => {
      selectedClips = e.selectedClips;
      waveMode = e.mode === "waveform";
    }),
  );
  // In the Waveform editor, QuickPass scans the open file.
  const inWave = $derived(waveMode && !!wave && waveDoc !== null);
  const doc = $derived(inWave ? waveDoc : sessionDoc);
  const chosen = $derived<ReadonlySet<string>>(inWave ? new Set() : selectedClips);
  const store = () => (inWave && wave ? wave : session);
  const run = () => (inWave && waveRunner ? waveRunner : runner);
  const tport = () => (inWave && waveTransport ? waveTransport : transport);

  type Done = "repaired" | "removed" | "ducked" | "split" | "marked";
  let view = $state<"scan" | "settings">("scan");
  let found = $state<Finding[]>([]);
  let done = $state<Record<string, Done>>({});
  let picked = $state<ReadonlySet<string>>(new Set());
  let scanned = $state(false);
  let busy = $state(false);
  const rate = $derived(doc?.sample_rate ?? 48000);
  const waiting = $derived(found.filter((f) => !done[f.id]));
  const target = $derived(picked.size > 0 ? waiting.filter((f) => picked.has(f.id)) : []);

  function choosePreset(name: string) {
    const p = allPresets.find((x) => x.name === name);
    if (!p) return;
    preset = name;
    s = { ...p.settings };
  }

  async function savePreset() {
    const name = (await ask("Save Preset", "Preset name", known && !isBuiltin ? preset : ""))?.trim();
    if (!name) return;
    if (BUILTIN_PRESETS.some((p) => p.name === name)) return onNotice("That name is used by a built-in preset. Pick another.");
    mine = [...mine.filter((p) => p.name !== name), { name, settings: { ...s } }];
    preset = name;
  }

  function deletePreset() {
    if (isBuiltin || !known) return;
    mine = mine.filter((p) => p.name !== preset);
    preset = "Default";
    s = { ...DEFAULTS };
  }

  function targets(): { ids: string[]; info: Map<string, { trackId: string; name: string }> } {
    const info = new Map<string, { trackId: string; name: string }>();
    for (const t of doc?.tracks ?? []) {
      for (const c of t.clips) {
        if (chosen.size > 0 ? chosen.has(c.id) : !t.muted && !c.muted) info.set(c.id, { trackId: t.id, name: c.name });
      }
    }
    return { ids: [...info.keys()], info };
  }
  const scope = $derived(inWave ? "the open file" : chosen.size > 0 ? `${chosen.size} selected ${chosen.size === 1 ? "clip" : "clips"}` : "all clips on tracks that play");

  async function measure(ids: string[]): Promise<ClipLevels[]> {
    busy = true;
    try {
      return await quickpassLevels(ids, WINDOW_MS, inWave);
    } finally {
      busy = false;
    }
  }

  async function scan() {
    const { ids, info } = targets();
    if (ids.length === 0) return onNotice("There are no clips to scan.");
    try {
      if (s.scan === "clicks" || s.scan === "clipping") {
        busy = true;
        try {
          found = spotFindings(await quickpassSpots(ids, spotsRequest(s), inWave), info, s.scan);
        } finally {
          busy = false;
        }
      } else found = findAll(await measure(ids), info, s, rate);
      done = {};
      picked = new Set();
      scanned = true;
      view = "scan";
    } catch (e) {
      onError(e);
    }
  }

  async function findLevels() {
    const { ids } = targets();
    if (ids.length === 0) return onNotice("There are no clips to measure.");
    try {
      const r = suggestLevels((await measure(ids)).flatMap((l) => l.db));
      if (!r) return onNotice("There is not enough sound to suggest levels. Set them by hand.");
      s = { ...s, ...r };
    } catch (e) {
      onError(e);
    }
  }

  const uuid = () => crypto.randomUUID();
  const t = (samples: number) => formatIn(samples, rate, "time");

  async function fix(list: Finding[]) {
    if (!doc || list.length === 0) return;
    try {
      if (s.action === "repair" && inWave) {
        return onNotice("Repair in QuickPass works in the Multitrack view. In the Waveform editor, use Effects > Noise and Repair > Click Repair or Clip Repair.");
      } else if (s.action === "repair") {
        busy = true;
        let fixed: string[] = [];
        try {
          fixed = await onRepair(spotsRequest(s), list);
        } finally {
          busy = false;
        }
        done = { ...done, ...Object.fromEntries(fixed.map((id) => [id, "repaired" as const])) };
      } else if (s.action === "remove") {
        const op = removeOp(doc, list, s, uuid);
        if (!op) return onNotice(`Nothing to remove there. The ${many} are shorter than the pause you keep.`);
        await store().edit(`Strip ${cap(list.length === 1 ? one : many)}`, op, run());
        let rest = found.filter((f) => !list.includes(f));
        for (const g of [...list].sort((a, b) => b.start - a.start)) rest = shiftFindings(rest, g, s.ripple, s.keepMs, rate);
        const next: Record<string, Done> = {};
        for (const f of list) next[f.id] = "removed";
        found = [...rest, ...list].sort((a, b) => a.start - b.start);
        done = { ...done, ...next };
      } else if (s.action === "duck") {
        const op = duckOp(doc, list, s.duckDb);
        if (!op) return onNotice("Nothing to turn down there. Locked clips are left alone.");
        await store().edit(`Turn Down ${cap(list.length === 1 ? one : many)}`, op, run());
        done = { ...done, ...Object.fromEntries(list.map((f) => [f.id, "ducked" as const])) };
      } else if (s.action === "split") {
        const op = splitOp(doc, list, uuid);
        if (!op) return onNotice("Nothing to split there. Locked clips are left alone.");
        await store().edit(`Split at ${cap(many)}`, op, run());
        if (!inWave && session.current) editor.selectClips(clipsOn(session.current, list));
        done = { ...done, ...Object.fromEntries(list.map((f) => [f.id, "split" as const])) };
      } else {
        await store().edit(`Mark ${cap(list.length === 1 ? one : many)}`, { op: "add_markers", markers: markersFor(doc, list, uuid, cap(one)) }, run());
        done = { ...done, ...Object.fromEntries(list.map((f) => [f.id, "marked" as const])) };
      }
      picked = new Set();
    } catch (e) {
      onError(e);
    }
  }

  function clearFixed() {
    found = found.filter((f) => !done[f.id]);
    done = {};
  }

  function pick(f: Finding, e: MouseEvent) {
    const next = new Set(e.ctrlKey || e.metaKey ? picked : []);
    if (e.ctrlKey || e.metaKey) {
      if (next.has(f.id)) next.delete(f.id);
      else next.add(f.id);
    } else next.add(f.id);
    picked = next;
    void tport().seek(f.start);
  }

  const cap = (w: string) => w.replace(/(^|\s)\S/g, (c) => c.toUpperCase());
  const field = (e: Event) => Number((e.currentTarget as HTMLInputElement).value);
  const set = (patch: Partial<QuickPassSettings>) => (s = sanitizeSettings({ ...s, ...patch }));
  const doneLabel = { repaired: "Repaired", removed: "Removed", ducked: "Lowered", split: "Split", marked: "Marked" } as const;
</script>

<section class="qp" aria-label="QuickPass">
  <div class="top">
    <label class="line">Scan for
      <select aria-label="What to scan for" value={s.scan} onchange={(e) => chooseScan(e.currentTarget.value as ScanType)}>
        {#each SCAN_TYPES as t (t.id)}<option value={t.id}>{t.label}</option>{/each}
      </select>
    </label>
    <div class="line">
      <label class="grow" for="qp-preset">Preset</label>
      <select id="qp-preset" value={known ? preset : ""} onchange={(e) => choosePreset(e.currentTarget.value)}>
        {#if !known}<option value="" disabled>Custom</option>{/if}
        {#each shownPresets as p (p.name)}<option value={p.name}>{p.name}</option>{/each}
      </select>
      <button type="button" class="icon" title="Save the current settings as a preset" aria-label="Save preset" onclick={() => void savePreset()}>
        <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3 2.5h8l2.5 2.5v8.5h-10.5zM5.5 2.5v3.5h4v-3.5M5.5 13.5v-4h5v4" fill="none" stroke="currentColor" stroke-width="1.2" /></svg>
      </button>
      <button type="button" class="icon" disabled={isBuiltin || !known} title={isBuiltin ? "Built-in presets cannot be deleted" : "Delete this preset"} aria-label="Delete preset" onclick={deletePreset}>
        <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3.5 4.5h9M6 4.5v-2h4v2M4.5 4.5l.6 9h5.8l.6-9M7 7v4M9 7v4" fill="none" stroke="currentColor" stroke-width="1.2" /></svg>
      </button>
    </div>
    <div class="tabs" role="tablist" aria-label="QuickPass view">
      <button type="button" role="tab" aria-selected={view === "scan"} class:on={view === "scan"} onclick={() => (view = "scan")}>Scan</button>
      <button type="button" role="tab" aria-selected={view === "settings"} class:on={view === "settings"} onclick={() => (view = "settings")}>Settings</button>
    </div>
  </div>

  {#if view === "settings"}
    <div class="body settings">
      {#if s.scan === "clicks"}
        <fieldset>
          <legend>Define a click as</legend>
          <label title="Higher finds fainter clicks. Too high can catch sharp consonants; listen to a few before fixing all.">Sensitivity <input type="number" step="5" min="0" max="100" value={s.clickSensitivity} onchange={(e) => set({ clickSensitivity: field(e) })} /> %</label>
          <label>No longer than <input type="number" step="0.1" min="0.1" max="3" value={s.clickWidthMs} onchange={(e) => set({ clickWidthMs: field(e) })} /> ms</label>
        </fieldset>
        <p class="hint">Each click is filled from the sound on either side. Repair writes a copy of the clip's audio with the chosen clicks fixed; the original file is never changed, and undo puts the clip back.</p>
      {:else if s.scan === "silence" || s.scan === "audio"}
        <fieldset>
          <legend>Define silence as</legend>
          <label>Level below <input type="number" step="1" min="-100" max="-1" value={s.quietDb} onchange={(e) => set({ quietDb: field(e) })} /> dB</label>
          <label>For more than <input type="number" step="10" min="5" max="60000" value={Math.round(s.minQuietSec * 1000)} onchange={(e) => set({ minQuietSec: field(e) / 1000 })} /> ms</label>
        </fieldset>
        <fieldset>
          <legend>Define sound as</legend>
          <label>Level above <input type="number" step="1" min="-100" max="0" value={s.contentDb} onchange={(e) => set({ contentDb: field(e) })} /> dB</label>
          <label title="Sound shorter than this inside a silence, such as a cough, bump or click, is treated as part of the silence and goes with it.">For more than <input type="number" step="10" min="10" max="10000" value={Math.round(s.minContentSec * 1000)} onchange={(e) => set({ minContentSec: field(e) / 1000 })} /> ms</label>
        </fieldset>
        <button type="button" class="wide" disabled={busy} onclick={findLevels} title="Measure the clips and set both levels to suit them.">Find Levels</button>
      {:else if s.scan === "noise"}
        <fieldset>
          <legend>Define a short noise as</legend>
          <label>Louder than <input type="number" step="1" min="-100" max="-1" value={s.quietDb} onchange={(e) => set({ quietDb: field(e) })} /> dB</label>
          <label>Lasting no more than <input type="number" step="10" min="20" max="2000" value={Math.round(s.maxBurstSec * 1000)} onchange={(e) => set({ maxBurstSec: field(e) / 1000 })} /> ms</label>
          <label title="Quiet needed before and after, so short words next to other speech are never caught.">With quiet either side of <input type="number" step="10" min="50" max="5000" value={Math.round(s.guardSec * 1000)} onchange={(e) => set({ guardSec: field(e) / 1000 })} /> ms</label>
        </fieldset>
        <button type="button" class="wide" disabled={busy} onclick={findLevels} title="Measure the clips and set the level to sit just above their background noise.">Find Levels</button>
      {:else}
        <fieldset>
          <legend>Define clipping as</legend>
          <label>Samples at or above <input type="number" step="0.1" min="-12" max="0" value={s.clipDb} onchange={(e) => set({ clipDb: field(e) })} /> dBFS</label>
          <label title="Longer flat runs are left alone: they are usually loud sound, not clipping.">Flat for no more than <input type="number" step="0.5" min="0.1" max="10" value={s.clipLongestMs} onchange={(e) => set({ clipLongestMs: field(e) })} /> ms</label>
        </fieldset>
        <p class="hint">Repair rebuilds each flattened peak from the shape on either side, in a copy of the clip's audio. Rebuilt peaks can go above full scale, so lower the clip gain a little or add a Limiter afterward.</p>
      {/if}
      <fieldset>
        <legend>Fix by</legend>
        <label>Action
          <select value={s.action} onchange={(e) => set({ action: e.currentTarget.value as QuickPassSettings["action"] })}>
            {#each info.actions as a (a)}<option value={a}>{ACTION_TEXT[a]}</option>{/each}
          </select>
        </label>
        {#if s.action === "remove"}
          <label title="Leave this much in place so speech does not run together. Use 0 for noises.">Keep a pause of <input type="number" step="10" min="0" max="5000" value={s.keepMs} onchange={(e) => set({ keepMs: field(e) })} /> ms</label>
          <label title="Leaving the gap open keeps every track in step. Closing it moves later clips left and can throw other tracks out of line.">Close gaps after fixing
            <select value={s.ripple} onchange={(e) => set({ ripple: e.currentTarget.value === "all" ? "all" : e.currentTarget.value === "track" ? "track" : "none" })}>
              <option value="none">no, leave them open</option>
              <option value="track">this track only</option>
              <option value="all">all tracks</option>
            </select>
          </label>
        {/if}
        {#if s.action === "duck"}
          <label title="How far each one is lowered, using the clip's volume envelope. Adjust any dip afterward like any envelope point.">Lower by <input type="number" step="1" min="-60" max="-1" value={s.duckDb} onchange={(e) => set({ duckDb: field(e) })} /> dB</label>
        {/if}
      </fieldset>
    </div>
  {:else}
    <label class="fixby">Fix by
      <select value={s.action} onchange={(e) => set({ action: e.currentTarget.value as QuickPassSettings["action"] })}>
        {#each info.actions as a (a)}<option value={a}>{FIX_BY[a]}</option>{/each}
      </select>
    </label>
    <div class="bar">
      <button type="button" class="go" disabled={busy} onclick={scan}>{busy ? "Scanning" : "Scan"}</button>
      <span class="grow"></span>
      <button type="button" disabled={target.length === 0} onclick={() => void fix(target)}>{VERB[s.action]}</button>
      <button type="button" disabled={waiting.length === 0} onclick={() => void fix(waiting)}>{VERB[s.action]} All</button>
      <button type="button" disabled={Object.keys(done).length === 0} onclick={clearFixed}>Clear Fixed</button>
    </div>
    <div class="body table" role="table" aria-label="Findings">
      <div class="head" role="row"><span>Fixed</span><span>Start</span><span>Length</span><span>Clip</span></div>
      {#if !scanned}
        <p class="empty">Scans {scope}. Press Scan to list the {many}. Nothing changes until you fix them.</p>
      {:else if found.length === 0}
        <p class="empty">No {many} found. Try adjusting the levels or times in Settings.</p>
      {:else}
        {#each found as f (f.id)}
          <button type="button" role="row" class="row" class:picked={picked.has(f.id)} class:fixed={!!done[f.id]} disabled={!!done[f.id]} onclick={(e) => pick(f, e)}>
            <span>{done[f.id] ? doneLabel[done[f.id]!] : ""}</span>
            <span class="num">{t(f.start)}</span>
            <span class="num">{t(f.end - f.start)}</span>
            <span class="nm">{f.clipName}</span>
          </button>
        {/each}
      {/if}
    </div>
    <p class="status" aria-live="polite">
      {#if !scanned}Ready.{:else}{waiting.length} {waiting.length === 1 ? one : many} to fix{#if Object.keys(done).length > 0}, {Object.keys(done).length} fixed{/if}.{/if}
    </p>
  {/if}
</section>

<style>
  .qp {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    background: var(--surface-1);
    font-size: 12px;
  }
  .top {
    display: grid;
    gap: 5px;
    padding: 6px 8px;
    border-bottom: 1px solid var(--border);
  }
  .fixby {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 8px 0;
    color: var(--text-dim);
  }
  .fixby select {
    flex: 1;
    min-width: 0;
  }
  .line {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .line select,
  .grow {
    flex: 1;
    min-width: 0;
  }
  .line > label.grow {
    flex: 0 0 auto;
  }
  select,
  input[type="number"] {
    background: var(--surface-0);
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    color: var(--text);
    font: inherit;
    padding: 1px 4px;
  }
  input[type="number"] {
    width: 58px;
  }
  .icon {
    display: grid;
    place-items: center;
    width: 22px;
    height: 20px;
    padding: 0;
    border: 1px solid transparent;
    background: none;
    color: var(--text-dim);
  }
  .icon svg {
    width: 14px;
    height: 14px;
  }
  .icon:hover:not(:disabled) {
    border-color: var(--border-strong);
    color: var(--text);
  }
  .icon:disabled {
    opacity: 0.4;
  }
  .tabs {
    display: grid;
    grid-template-columns: 1fr 1fr;
    height: 20px;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    overflow: hidden;
  }
  .tabs button {
    padding: 0;
    border: none;
    background: var(--surface-0);
    color: var(--text-dim);
    font: inherit;
    font-size: 11px;
    line-height: 18px;
  }
  .tabs button.on {
    background: var(--accent);
    color: var(--on-accent, #fff);
  }
  .body {
    flex: 1;
    min-height: 0;
    min-width: 0;
    overflow-x: hidden;
    overflow-y: auto;
  }
  .settings {
    display: grid;
    align-content: start;
    gap: 8px;
    padding: 8px;
  }
  fieldset {
    display: grid;
    gap: 5px;
    margin: 0;
    padding: 6px 8px 8px;
    border: 1px solid var(--border);
    border-radius: 3px;
  }
  legend {
    padding: 0 4px;
    color: var(--text-dim);
  }
  fieldset label {
    display: flex;
    gap: 6px;
    align-items: center;
    justify-content: space-between;
  }
  .wide {
    justify-self: start;
  }
  .hint {
    margin: 0;
    color: var(--text-dim);
  }
  .bar {
    display: flex;
    flex-wrap: wrap;
    gap: 3px;
    align-items: center;
    padding: 3px 6px;
    border-bottom: 1px solid var(--border);
  }
  .bar button {
    height: 20px;
    padding: 0 6px;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    background: var(--surface-0);
    color: var(--text);
    font: inherit;
    font-size: 11px;
    line-height: 18px;
    white-space: nowrap;
  }
  .bar button:disabled {
    color: var(--text-faint);
    opacity: 0.6;
  }
  .bar button:hover:not(:disabled) {
    background: var(--surface-2);
  }
  .bar .go {
    border-color: var(--accent);
  }
  .table {
    background: var(--surface-0);
  }
  .head,
  .row {
    display: grid;
    grid-template-columns: minmax(0, 0.8fr) minmax(0, 1fr) minmax(0, 1fr) minmax(0, 1.2fr);
    gap: 6px;
    padding: 2px 8px;
    text-align: left;
  }
  .head {
    position: sticky;
    top: 0;
    background: var(--surface-1);
    color: var(--text-dim);
    border-bottom: 1px solid var(--border);
  }
  .row {
    width: 100%;
    border: none;
    background: none;
    color: var(--text);
    font: inherit;
  }
  .row:hover:not(:disabled) {
    background: var(--surface-1);
  }
  .row.picked {
    background: var(--accent);
    color: var(--on-accent, #fff);
  }
  .row.fixed {
    color: var(--text-faint);
  }
  .num {
    font-variant-numeric: tabular-nums;
  }
  .nm {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .empty {
    margin: 0;
    padding: 10px 8px;
    color: var(--text-dim);
  }
  .status {
    margin: 0;
    padding: 3px 8px;
    border-top: 1px solid var(--border);
    color: var(--text-dim);
  }
</style>
