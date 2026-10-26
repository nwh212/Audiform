<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  Playlist panel: range markers in an order of your choosing. Play runs
  them back to back so you can hear an arrangement (a highlight reel, a
  set of promos) without moving any clips. Export writes each one as its
  own file. Changes to the order are undoable.
-->
<script lang="ts">
  import { onDestroy } from "svelte";
  import type { CommandRunner } from "../../stores/media-pool";
  import { editor, type EditorState } from "../../stores/editor";
  import type { MarkerDoc, SessionDoc, SessionStore } from "../../stores/session";
  import type { TransportState, TransportStore } from "../../stores/transport";
  import { formatIn } from "../editor/timeline";
  import { markerCss } from "../markers/markers";
  import { available, nextStart, playlistItems, withAdded, withMoved, withRemoved } from "./playlist";

  interface Props {
    session: SessionStore;
    transport: TransportStore;
    runner: CommandRunner;
    onExport: (items: MarkerDoc[]) => void;
    onError?: (e: unknown) => void;
  }

  const { session, transport, runner, onExport, onError = console.error }: Props = $props();

  let doc = $state<SessionDoc | null>(null);
  $effect(() =>
    session.subscribe((d) => {
      doc = d;
    }),
  );
  let ed = $state<EditorState>(editor.current());
  $effect(() =>
    editor.subscribe((s) => {
      ed = s;
    }),
  );
  let tp = $state<TransportState | null>(null);
  $effect(() =>
    transport.subscribe((t) => {
      tp = t;
    }),
  );

  const items = $derived(doc ? playlistItems(doc) : []);
  const spare = $derived(doc ? available(doc) : []);
  const rate = $derived(doc?.sample_rate ?? 48000);
  const t = (s: number) => formatIn(s, rate, ed.timeFormat);
  const total = $derived(items.reduce((n, m) => n + m.duration, 0));
  let adding = $state("");

  function save(label: string, ids: string[]) {
    void session.edit(label, { op: "set_playlist", ids }, runner).catch(onError);
  }

  // Preview: play item `playing`, and when the playhead passes its end jump to the next.
  let playing = $state<number | null>(null);
  let timer: ReturnType<typeof setInterval> | undefined;
  function stopWatch() {
    clearInterval(timer);
    timer = undefined;
  }
  onDestroy(stopWatch);

  async function playFrom(index: number) {
    const m = items[index];
    if (!m) return;
    playing = index;
    await transport.play(m.position);
    stopWatch();
    timer = setInterval(() => {
      const cur = transport.current;
      const at = playing;
      if (at === null || !cur.playing) {
        playing = null;
        stopWatch();
        return;
      }
      const item = items[at];
      if (!item || cur.position < item.position + item.duration) return;
      const next = nextStart(items, at);
      if (next) {
        playing = next.index;
        void transport.play(next.at);
      } else {
        playing = null;
        stopWatch();
        void transport.pause();
      }
    }, 20);
  }

  async function stop() {
    playing = null;
    stopWatch();
    await transport.pause();
  }
</script>

<section class="pl" aria-label="Playlist">
  <div class="bar">
    <button type="button" disabled={items.length === 0} onclick={() => void (playing === null || !tp?.playing ? playFrom(0) : stop())}>
      {playing !== null && tp?.playing ? "Stop" : "Play All"}
    </button>
    <button type="button" disabled={items.length === 0} onclick={() => onExport(items)} title="Write each range as its own file, numbered in playlist order">Export Each...</button>
    <span class="grow"></span>
    <select
      aria-label="Add a range marker"
      value={adding}
      disabled={spare.length === 0}
      onchange={(e) => {
        const id = e.currentTarget.value;
        if (doc && id === "*") save("Add All to Playlist", withAdded(doc, spare.map((m) => m.id)));
        else if (doc && id) save("Add to Playlist", withAdded(doc, [id]));
        adding = "";
        e.currentTarget.value = "";
      }}
    >
      <option value="">{spare.length ? "Add a range..." : "No more range markers"}</option>
      {#if spare.length > 1}<option value="*">All range markers ({spare.length})</option>{/if}
      {#each spare as m (m.id)}<option value={m.id}>{m.name} ({t(m.duration)})</option>{/each}
    </select>
  </div>

  {#if items.length === 0}
    <p class="empty">
      Add range markers to build a playlist: a highlight reel, a set of promos, or the parts to send a guest. Make a range marker by selecting time and choosing Edit &gt; Markers &gt; Add Range Marker.
    </p>
  {:else}
    <ol class="list">
      {#each items as m, i (m.id)}
        <li class:on={playing === i && tp?.playing}>
          <button type="button" class="play" title="Play from here" aria-label="Play from {m.name}" onclick={() => void playFrom(i)}>
            <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M5 3v10l8-5z" fill="currentColor" /></svg>
          </button>
          <i style:background={markerCss(m)}></i>
          <span class="nm" title={m.description || m.name}>{i + 1}. {m.name}</span>
          <span class="num">{t(m.duration)}</span>
          <button type="button" class="tiny" disabled={i === 0} title="Move up" aria-label="Move up" onclick={() => doc && save("Move in Playlist", withMoved(doc, m.id, -1))}>↑</button>
          <button type="button" class="tiny" disabled={i === items.length - 1} title="Move down" aria-label="Move down" onclick={() => doc && save("Move in Playlist", withMoved(doc, m.id, 1))}>↓</button>
          <button type="button" class="tiny" title="Take out of the playlist (the marker stays)" aria-label="Remove from playlist" onclick={() => doc && save("Remove from Playlist", withRemoved(doc, m.id))}>×</button>
        </li>
      {/each}
    </ol>
    <p class="status">{items.length} {items.length === 1 ? "range" : "ranges"}, {t(total)} in all</p>
  {/if}
</section>

<style>
  .pl {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    background: var(--surface-1);
    font-size: 12px;
  }
  .bar {
    display: flex;
    gap: 4px;
    align-items: center;
    padding: 4px 6px;
    border-bottom: 1px solid var(--border);
  }
  .grow {
    flex: 1;
  }
  select {
    max-width: 50%;
    height: 22px;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    background: var(--surface-0);
    color: var(--text);
    font: inherit;
  }
  .empty,
  .status {
    margin: 0;
    padding: 8px;
    color: var(--text-dim);
  }
  .status {
    padding: 3px 8px;
    border-top: 1px solid var(--border);
  }
  .list {
    flex: 1;
    min-height: 0;
    margin: 0;
    padding: 0;
    overflow: auto;
    list-style: none;
    background: var(--surface-0);
  }
  li {
    display: grid;
    grid-template-columns: 20px 8px minmax(0, 1fr) auto 18px 18px 18px;
    gap: 6px;
    align-items: center;
    padding: 2px 6px;
    border-bottom: 1px solid var(--surface-1);
  }
  li.on {
    background: var(--surface-2);
  }
  li i {
    width: 8px;
    height: 8px;
    border-radius: 1px;
  }
  .nm {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .num {
    color: var(--value);
    font-family: var(--font-mono);
  }
  .play,
  .tiny {
    display: grid;
    place-items: center;
    width: 18px;
    height: 18px;
    padding: 0;
    border: 1px solid transparent;
    background: none;
    color: var(--text-dim);
  }
  .play svg {
    width: 11px;
    height: 11px;
  }
  .play:hover,
  .tiny:hover:not(:disabled) {
    border-color: var(--border-strong);
    color: var(--text);
  }
  .tiny:disabled {
    opacity: 0.35;
  }
</style>
