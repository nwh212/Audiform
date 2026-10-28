<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The Audiform contributors.
  This file is part of Audiform, based on Altered Audio Editor.

  The Audio Separation panel: the models to pick from, grouped by what
  they separate, each with its published quality score (SDR), size and
  whether it is downloaded; and Separate, for the file in the Waveform
  editor, the selected clips or the selected files. A model is downloaded
  the first time it is used (Audiform asks first) and kept for next time;
  Remove deletes it. Separation runs on this computer.
-->
<script lang="ts">
  import { sizeText } from "../../app/settings/pages";
  import { byCategory, sdrText } from "../../app/separation";
  import type { SeparationTools } from "../../app/separation-tools.svelte";
  import { editor } from "../../stores/editor";
  import { mediaPool } from "../../stores/media";
  import { waveFile } from "../../stores/wave";
  import { prefs } from "../../stores/prefs";
  import { settings } from "../../stores/settings";

  interface Props {
    tools: SeparationTools;
  }

  const { tools }: Props = $props();

  const KEY = "separation.model";
  let picked = $state<string | null>((prefs.raw(KEY) as string | null) ?? null);
  $effect(() => {
    void tools.refresh();
  });
  const status = $derived(tools.status);
  const groups = $derived(status ? byCategory(status.models) : []);
  const model = $derived(status?.models.find((m) => m.id === picked) ?? groups[0]?.[1][0] ?? null);
  function pick(id: string) {
    picked = id;
    prefs.set(KEY, id);
  }

  // What Separate works on follows the selection and the editor in front.
  let tick = $state(0);
  $effect(() => editor.subscribe(() => tick++));
  $effect(() => waveFile.subscribe(() => tick++));
  $effect(() => mediaPool.subscribe(() => tick++));
  const source = $derived.by(() => {
    void tick;
    return tools.source();
  });
  const job = $derived(tools.job);
  let onGpu = $state(settings.current.separationDevice === "gpu");
  $effect(() =>
    settings.subscribe((v) => {
      onGpu = v.separationDevice === "gpu";
    }),
  );
</script>

<div class="panel">
  <div class="list">
    {#if !status}
      <p class="note">Reading the model list…</p>
    {:else}
      <h3>Separation engine</h3>
      <ul>
        <li>
          <div class="pick">
            <span class="name">Engine (runs the models on this computer)</span>
          </div>
          <div class="meta">
            {#if status.engineInstalled}
              <span>{status.engineBytes ? sizeText(status.engineBytes) : ""}</span>
              <span class="ok">Downloaded</span>
              <button type="button" class="link" disabled={!!job} onclick={() => void tools.removeEngine()}>Remove</button>
            {:else}
              <span>About 10 MB</span>
              <button type="button" class="link" disabled={!!job || !status.engineAvailable} onclick={() => void tools.download("engine")}>Download</button>
            {/if}
          </div>
        </li>
      </ul>
      {#each groups as [category, models] (category)}
        <h3>{category}</h3>
        <ul>
          {#each models as m (m.id)}
            <li class:on={model?.id === m.id}>
              <label class="pick" title={`${m.author}: ${m.page}`}>
                <input type="radio" name="separation-model" checked={model?.id === m.id} onchange={() => pick(m.id)} />
                <span class="name">{m.name}</span>
                <span class="sdr" title={`Published score: ${m.sdrNote}. Higher is better.`}>{sdrText(m)}</span>
              </label>
              <div class="meta">
                <span>{m.targetName} and {m.restName}</span>
                <span>{sizeText(m.bytes)}</span>
                {#if m.installed}
                  <span class="ok">Downloaded</span>
                  <button type="button" class="link" disabled={!!job} onclick={() => void tools.remove(m)}>Remove</button>
                {:else}
                  <button type="button" class="link" disabled={!!job || !status.engineAvailable} onclick={() => void tools.download(m.id)}>Download</button>
                {/if}
              </div>
              <p class="licence">By {m.author}. {m.licence}</p>
            </li>
          {/each}
        </ul>
      {/each}
      {#if !status.engineAvailable}
        <p class="note">Audio Separation is not available for this computer yet.</p>
      {/if}
    {/if}
  </div>

  <footer>
    {#if job}
      <div class="job">
        <span class="title">{job.title}</span>
        <div class="bar" class:busy={job.progress === null}><div style:width={`${Math.round((job.progress ?? 0) * 100)}%`}></div></div>
        <span class="detail">{job.detail}</span>
        <button type="button" onclick={tools.cancel}>Cancel</button>
      </div>
    {:else}
      <p class="what">
        {#if source}Separates {source.label}{source.kind === "clips" ? " (their whole files)" : ""}.{:else}Open a file in the Waveform editor, or select clips or files, to separate them.{/if}
      </p>
      <div class="go">
        <button type="button" class="primary" disabled={!source || !model} onclick={() => model && void tools.separate(model)}>Separate</button>
        <span class="where">On the {onGpu ? "graphics card when it can be used" : "processor"}. Runs on this computer; speed and results depend on your hardware.</span>
      </div>
    {/if}
  </footer>
</div>

<style>
  .panel {
    display: grid;
    grid-template-rows: 1fr auto;
    height: 100%;
    min-height: 0;
    background: var(--surface-1);
    color: var(--text);
    font-size: var(--font-size-small);
  }

  .list {
    overflow-y: auto;
    min-height: 0;
    padding: var(--space-2) var(--space-3);
  }

  h3 {
    margin: var(--space-2) 0 var(--space-1);
    font-size: var(--font-size-small);
    font-weight: 600;
    color: var(--text-dim);
    text-transform: none;
  }

  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  li {
    padding: var(--space-1) var(--space-2);
    border: 1px solid var(--border);
    border-radius: 4px;
    margin-bottom: var(--space-1);
    background: var(--surface-0);
  }

  li.on {
    border-color: var(--accent);
  }

  .pick {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .name {
    font-weight: 600;
    flex: 1;
  }

  .sdr {
    font-family: var(--font-mono);
    color: var(--value, var(--accent));
  }

  .meta {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
    margin: 2px 0 0 22px;
    color: var(--text-dim);
  }

  .ok {
    color: var(--accent);
  }

  .licence {
    margin: 2px 0 0 22px;
    color: var(--text-faint);
  }

  .link {
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent);
    font: inherit;
    cursor: pointer;
    text-decoration: underline;
  }

  .link:disabled {
    color: var(--text-faint);
    cursor: default;
  }

  .note {
    color: var(--text-dim);
  }

  footer {
    border-top: 1px solid var(--border);
    padding: var(--space-2) var(--space-3);
    background: var(--surface-2);
  }

  .what {
    margin: 0 0 var(--space-1);
  }

  .go {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .where {
    color: var(--text-faint);
  }

  .job {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 2px var(--space-2);
    align-items: center;
  }

  .job .title {
    grid-column: 1 / -1;
  }

  .bar {
    height: 6px;
    border-radius: 3px;
    background: var(--surface-0);
    border: 1px solid var(--border);
    overflow: hidden;
  }

  .bar > div {
    height: 100%;
    background: var(--accent);
  }

  .bar.busy > div {
    width: 30% !important;
    animation: slide 1.2s linear infinite;
  }

  @keyframes slide {
    from {
      transform: translateX(-100%);
    }
    to {
      transform: translateX(340%);
    }
  }

  .detail {
    color: var(--text-dim);
    font-family: var(--font-mono);
  }

  .job button {
    grid-row: 2;
    grid-column: 2;
  }
</style>
