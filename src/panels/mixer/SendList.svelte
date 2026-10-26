<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  A track's or bus's output picker and aux sends, used by the Mixer
  strips and the Editor's track headers. Each send has a level (drag
  sideways, Shift for fine steps, double-click for 0 dB), pre or post
  fader, on and off, and remove. Choices that would loop are left out.
-->
<script lang="ts">
  import { busChoices, busOp, MASTER_ID, newSend, removeSend, setSend } from "./routing";
  import { dbToFader, formatDb } from "./fader";
  import type { CommandRunner } from "../../stores/media-pool";
  import type { Op, OutputDoc, SendDoc, SessionDoc, SessionStore } from "../../stores/session";
  import { trackOp } from "./solo";

  interface Props {
    doc: SessionDoc;
    session: SessionStore;
    runner: CommandRunner;
    kind: "track" | "bus";
    id: string;
    name: string;
    output: OutputDoc;
    sends: SendDoc[];
    /** Leave out the output picker (shown elsewhere). */
    noOutput?: boolean;
    onError?: (e: unknown) => void;
  }

  const { doc, session, runner, kind, id, name, output, sends, noOutput = false, onError = console.error }: Props = $props();

  const choices = $derived(busChoices(doc, id));
  const busName = (b: string) => doc.buses?.find((x) => x.id === b)?.name ?? "Missing bus";

  function edit(label: string, op: Op) {
    void session.edit(label, op, runner).catch(onError);
  }

  function setSends(next: SendDoc[], label: string) {
    edit(label, { op: "set_sends", owner: id, sends: next });
  }

  function setOutput(value: string) {
    const o: OutputDoc = value === MASTER_ID ? { type: "master" } : { type: "bus", bus_id: value };
    const changes = { output: o };
    const label = kind === "bus" ? "Route Bus Output" : "Route Track Output";
    edit(label, kind === "bus" ? busOp(doc, id, changes) : trackOp(doc, id, changes));
  }

  function drag(e: PointerEvent, x: SendDoc) {
    if (e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    const startX = e.clientX;
    const start = x.level_db;
    let last = start;
    const move = (ev: PointerEvent) => {
      const v = Math.min(12, Math.max(-60, Math.round((start + (ev.clientX - startX) * (ev.shiftKey ? 0.05 : 0.25)) * 10) / 10));
      if (v !== last) {
        last = v;
        setSends(setSend(sends, x.id, { level_db: v }), "Send Level");
      }
    };
    const up = () => {
      el.removeEventListener("pointermove", move);
      el.removeEventListener("pointerup", up);
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up);
  }

  const stop = (e: Event) => e.stopPropagation();
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="routing" onclick={stop} onpointerdown={stop}>
  {#if !noOutput}
    <select class="out" aria-label="{name} output" title="Where this {kind}'s sound goes after its fader" value={output.type === "bus" ? output.bus_id : MASTER_ID} onchange={(e) => setOutput(e.currentTarget.value)}>
      <option value={MASTER_ID}>To Master</option>
      {#each choices as b (b.id)}<option value={b.id}>To {b.name}</option>{/each}
      {#if output.type === "bus" && !choices.some((b) => output.type === "bus" && b.id === output.bus_id)}
        <option value={output.bus_id}>To {busName(output.bus_id)}</option>
      {/if}
    </select>
  {/if}
  {#each sends as x (x.id)}
    <div class="send" class:off={!x.enabled}>
      <div class="srow">
        <button type="button" class="sbtn" class:on={x.enabled} title={x.enabled ? "Send on: click to turn off" : "Send off: click to turn on"} aria-pressed={x.enabled} onclick={() => setSends(setSend(sends, x.id, { enabled: !x.enabled }), x.enabled ? "Turn Off Send" : "Turn On Send")}>●</button>
        <span class="sname" title="Send to {busName(x.bus_id)}">{busName(x.bus_id)}</span>
        <button type="button" class="sbtn" class:on={x.pre_fader} title={x.pre_fader ? "Pre-fader: ignores this fader and pan. Click for post-fader." : "Post-fader: follows this fader. Click for pre-fader."} aria-pressed={x.pre_fader} onclick={() => setSends(setSend(sends, x.id, { pre_fader: !x.pre_fader }), "Send Pre or Post Fader")}>P</button>
        <button type="button" class="sbtn" title="Remove send" aria-label="Remove send" onclick={() => setSends(removeSend(sends, x.id), "Remove Send")}>✕</button>
      </div>
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="slevel"
        role="slider"
        tabindex="-1"
        aria-label="Send level to {busName(x.bus_id)}"
        aria-valuemin={-60}
        aria-valuemax={12}
        aria-valuenow={x.level_db}
        title="Send level. Drag sideways, Shift for fine steps, double-click for 0 dB."
        onpointerdown={(e) => drag(e, x)}
        ondblclick={() => setSends(setSend(sends, x.id, { level_db: 0 }), "Send Level")}
      >
        <div class="sfill" style:width="{dbToFader(x.level_db) * 100}%"></div>
        <span class="sdb">{formatDb(x.level_db)}</span>
      </div>
    </div>
  {/each}
  {#if choices.length > 0}
    <select
      class="out add"
      aria-label="Add a send"
      title="Send a copy of this {kind}'s sound to a bus, at its own level"
      value=""
      onchange={(e) => {
        const b = e.currentTarget.value;
        e.currentTarget.value = "";
        if (b) setSends([...sends, newSend(b, crypto.randomUUID())], "Add Send");
      }}
    >
      <option value="" disabled>+ Send</option>
      {#each choices as b (b.id)}<option value={b.id}>{b.name}</option>{/each}
    </select>
  {:else if (doc.buses?.length ?? 0) === 0}
    <span class="hint" title="Add a bus first: Multitrack > Add > Bus Track (Alt+B)">No buses yet</span>
  {/if}
</div>

<style>
  .routing {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    cursor: default;
  }

  .out {
    width: 100%;
    min-width: 0;
    height: 18px;
    padding: 0 2px;
    font: inherit;
    font-size: 10px;
  }

  .out.add,
  .hint {
    color: var(--text-dim);
  }

  .hint {
    font-size: 10px;
  }

  .send {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
    font-size: 10px;
  }

  .send.off .slevel,
  .send.off .sname {
    opacity: 0.5;
  }

  .srow {
    display: flex;
    gap: 1px;
    align-items: center;
    min-width: 0;
  }

  .sname {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .slevel {
    position: relative;
    height: 12px;
    overflow: hidden;
    border-radius: 2px;
    background: var(--surface-0);
    cursor: ew-resize;
  }

  .sfill {
    position: absolute;
    top: 0;
    bottom: 0;
    left: 0;
    background: var(--accent-dim);
  }

  .sdb {
    position: relative;
    display: block;
    color: var(--text);
    font-family: var(--font-mono);
    font-size: 9px;
    line-height: 12px;
    text-align: center;
  }

  .sbtn {
    flex: none;
    width: 13px;
    height: 14px;
    padding: 0;
    border: none;
    background: none;
    color: var(--text-faint);
    font: inherit;
    font-size: 9px;
    line-height: 1;
  }

  .sbtn.on {
    color: var(--accent);
  }
</style>
