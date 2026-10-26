<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  Markers panel: every marker in time order. Click a row to jump there,
  double-click the name to rename, change the type from the list, and
  delete with the row's button. Every change is undoable. Follows the
  active editor: the Waveform editor's file markers when it is in front,
  the session's otherwise.
-->
<script lang="ts">
  import type { Readable } from "svelte/store";
  import type { ActionRegistry } from "../../app/actions";
  import type { CommandRunner } from "../../stores/media-pool";
  import { editor, type EditorState } from "../../stores/editor";
  import type { MarkerChange, MarkerDoc, MarkerKind, SessionDoc, SessionStore } from "../../stores/session";
  import type { TransportStore } from "../../stores/transport";
  import { formatIn } from "../editor/timeline";
  import { KIND_LABELS, markerCss } from "./markers";

  interface Props {
    session: SessionStore;
    transport: TransportStore;
    runner: CommandRunner;
    actions: ActionRegistry;
    onError?: (e: unknown) => void;
    /** The Waveform editor's store, history and transport, when present. */
    wave?: SessionStore;
    waveRunner?: CommandRunner;
    waveTransport?: TransportStore;
    /** The open Waveform editor file; null when it is empty. */
    waveFile?: Readable<unknown>;
  }

  const { session, transport, runner, actions, onError = console.error, wave, waveRunner, waveTransport, waveFile }: Props = $props();

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
  let waveHasFile = $state(false);
  $effect(() =>
    waveFile?.subscribe((f) => {
      waveHasFile = f !== null && f !== undefined;
    }),
  );
  let ed = $state<EditorState>(editor.current());
  $effect(() =>
    editor.subscribe((s) => {
      ed = s;
    }),
  );

  // The Waveform editor wins while it is in front with a file open.
  const onWave = $derived(ed.mode === "waveform" && !!wave && !!waveDoc && waveHasFile);
  const doc = $derived(onWave ? waveDoc : sessionDoc);
  const store = $derived(onWave && wave ? wave : session);
  const run = $derived(onWave && waveRunner ? waveRunner : runner);
  const tp = $derived(onWave && waveTransport ? waveTransport : transport);

  let renaming = $state<string | null>(null);
  const rate = $derived(doc?.sample_rate ?? 48000);
  const t = (samples: number) => formatIn(samples, rate, ed.timeFormat);
  // Kinds that can be chosen now. The others arrive with their phases.
  const KINDS: MarkerKind[] = ["cue", "subclip", "chapter", "ad_slot", "social_clip", "cd_track", "cart_timer"];

  function edit(label: string, m: MarkerDoc, change: Omit<MarkerChange, "id">) {
    void store.edit(label, { op: "set_markers", changes: [{ id: m.id, ...change }] }, run).catch(onError);
  }

  function commitRename(m: MarkerDoc, value: string) {
    renaming = null;
    const v = value.trim();
    if (v && v !== m.name) edit("Rename Marker", m, { name: v });
  }
</script>

<section class="markers" aria-label="Markers">
  <div class="toolbar">
    <button type="button" onclick={() => void actions.run("edit.marker.cue")}>+ Cue</button>
    <button type="button" disabled={!doc?.view.selection} title="Needs a time selection" onclick={() => void actions.run("edit.marker.range")}>+ Range</button>
  </div>
  {#if !doc || doc.markers.length === 0}
    <p class="empty">No markers yet. Add one at the playhead with Edit &gt; Markers, or right-click the ruler.</p>
  {:else}
    <div class="table" role="table">
      <div class="row head" role="row">
        <span></span>
        <span role="columnheader">Name</span>
        <span role="columnheader">Start</span>
        <span role="columnheader" class="dur">Duration</span>
        <span role="columnheader">Type</span>
        <span></span>
      </div>
      {#each doc.markers as m (m.id)}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div class="row" role="row" tabindex="-1" onclick={() => void tp.seek(m.position)}>
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
          <label class="swatch" title={m.color ? "Change color (right-click to go back to the type's color)" : "Pick a color for this marker"} style:background={markerCss(m)} onclick={(e) => e.stopPropagation()} oncontextmenu={(e) => { e.preventDefault(); if (m.color) edit("Marker Color", m, { color: "" }); }}>
            <input type="color" value={m.color ?? "#c8b040"} aria-label="Marker color" onchange={(e) => edit("Marker Color", m, { color: e.currentTarget.value })} />
          </label>
          {#if renaming === m.id}
            <!-- svelte-ignore a11y_autofocus -->
            <input
              type="text"
              value={m.name}
              autofocus
              onclick={(e) => e.stopPropagation()}
              onblur={(e) => commitRename(m, e.currentTarget.value)}
              onkeydown={(e) => {
                e.stopPropagation();
                if (e.key === "Enter") commitRename(m, e.currentTarget.value);
                if (e.key === "Escape") renaming = null;
              }}
            />
          {:else}
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <span class="name" title="Double-click to rename" ondblclick={() => (renaming = m.id)}>
              {m.name}
            </span>
          {/if}
          <span class="num">{t(m.position)}</span>
          <span class="num dur" class:off={m.kind === "ad_slot" && m.contracted_duration !== null && Math.abs(m.duration - m.contracted_duration) > 5 * rate} title={m.kind === "ad_slot" && m.contracted_duration !== null ? `Contracted ${t(m.contracted_duration)}. Off by more than 5 seconds shows red.` : ""}>{m.duration > 0 ? t(m.duration) : "-"}</span>
          <select
            value={m.kind}
            onclick={(e) => e.stopPropagation()}
            onchange={(e) => {
              const kind = e.currentTarget.value as MarkerKind;
              // Only ad slots carry a contracted length.
              edit("Marker Type", m, kind === "ad_slot" ? { kind, contracted_duration: m.contracted_duration ?? (m.duration || 30 * rate) } : m.contracted_duration !== null ? { kind, clear_contracted: true } : { kind });
            }}
          >
            {#each KINDS.includes(m.kind) ? KINDS : [...KINDS, m.kind] as k (k)}
              <option value={k}>{KIND_LABELS[k]}</option>
            {/each}
          </select>
          <button
            type="button"
            class="del"
            title="Delete marker"
            aria-label="Delete marker"
            onclick={(e) => {
              e.stopPropagation();
              void store.edit("Delete Marker", { op: "remove_markers", ids: [m.id] }, run).catch(onError);
            }}>×</button
          >
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
          <div class="notes" onclick={(e) => e.stopPropagation()}>
            <input
              type="text"
              placeholder="Notes"
              aria-label="Marker notes"
              value={m.description}
              onkeydown={(e) => {
                e.stopPropagation();
                if (e.key === "Enter") e.currentTarget.blur();
              }}
              onblur={(e) => {
                const v = e.currentTarget.value;
                if (v !== m.description) edit("Marker Notes", m, { description: v });
              }}
            />
          </div>
        </div>
      {/each}
    </div>
  {/if}
</section>

<style>
  .markers {
    container-type: inline-size;
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    height: 100%;
    background: var(--surface-1);
  }

  .toolbar {
    display: flex;
    gap: 4px;
    padding: 4px 6px;
    border-bottom: 1px solid var(--border);
  }

  .toolbar button {
    height: 20px;
    padding: 0 8px;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    background: var(--surface-3);
    color: var(--text);
    font: inherit;
    font-size: var(--font-size-small);
  }

  .toolbar button:disabled {
    color: var(--text-faint);
  }

  .empty {
    margin: 0;
    padding: 16px;
    color: var(--text-faint);
    text-align: center;
  }

  .table {
    overflow: auto;
  }

  .row {
    display: grid;
    grid-template-columns: 10px minmax(70px, 1fr) 66px 58px 76px 18px;
    gap: 2px 6px;
    align-items: center;
    min-height: 22px;
    padding: 0 6px;
    border-bottom: 1px solid var(--surface-0);
    font-size: var(--font-size-small);
    cursor: pointer;
  }

  @container (max-width: 320px) {
    .row {
      grid-template-columns: 10px minmax(60px, 1fr) 62px 70px 18px;
    }

    .dur {
      display: none;
    }
  }

  .row:hover:not(.head) {
    background: var(--surface-2);
  }

  .head {
    position: sticky;
    top: 0;
    background: var(--surface-2);
    color: var(--text-dim);
    cursor: default;
  }

  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .swatch {
    position: relative;
    width: 10px;
    height: 10px;
    border-radius: 2px;
    cursor: pointer;
  }

  .swatch input {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    opacity: 0;
    cursor: pointer;
  }

  .dur.off {
    color: var(--danger);
  }

  .notes {
    grid-column: 2 / -1;
    padding-bottom: 3px;
  }

  .notes input {
    width: 100%;
    height: 18px;
    padding: 0 4px;
    border: 1px solid transparent;
    background: transparent;
    color: var(--text-dim);
    font: inherit;
  }

  .notes input:hover,
  .notes input:focus {
    border-color: var(--border);
    background: var(--surface-0);
    color: var(--text);
  }

  .num {
    color: var(--value);
    font-family: var(--font-mono);
    text-align: right;
  }

  input,
  select {
    height: 18px;
    min-width: 0;
    border: 1px solid var(--border-strong);
    background: var(--surface-0);
    color: var(--text);
    font: inherit;
  }

  .del {
    width: 18px;
    height: 18px;
    padding: 0;
    border: 0;
    background: none;
    color: var(--text-dim);
    font-size: 14px;
  }

  .del:hover {
    color: var(--danger);
  }
</style>
