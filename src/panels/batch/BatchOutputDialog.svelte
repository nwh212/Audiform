<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  Batch Convert's Output Settings: how new files are named, where they
  go, the format, the sample settings (with their own dialog), encoder
  settings for MP3 and AAC, and what happens to tags and the list.
  Nothing changes until OK.
-->
<script lang="ts">
  import { PLATFORM } from "../../lib/platform";
  import { AAC_BITRATES, extensionOf, formatAvailable, FORMAT_KINDS, MP3_BITRATES, type OutputFormat } from "../../app/output-format";
  import { effectiveTemplate, expandTemplate, type BatchSettings, type TagFields } from "./batch";
  import SampleSettingsDialog from "./SampleSettingsDialog.svelte";

  interface Props {
    settings: BatchSettings;
    /** The first listed file's name, for the example. */
    exampleStem: string;
    pickFolder: (current: string | null) => Promise<string | null>;
    onApply: (s: BatchSettings) => void;
    onClose: () => void;
  }

  const { settings, exampleStem, pickFolder, onApply, onClose }: Props = $props();
  // svelte-ignore state_referenced_locally
  let s = $state<BatchSettings>(structuredClone($state.snapshot(settings)) as BatchSettings);
  let sampleOpen = $state(false);
  let tagsOpen = $state(false);
  let templateInput = $state<HTMLInputElement | null>(null);

  const TOKENS = [
    { token: "{name}", hint: "The original file name, without its extension." },
    { token: "{number}", hint: "The file's place in the list: 01, 02, 03 and so on." },
    { token: "{date}", hint: "Today's date, like 2026-10-15." },
    { token: "{format}", hint: "The new format, like mp3." },
  ] as const;
  const TAG_FIELDS: { key: keyof TagFields; label: string }[] = [
    { key: "title", label: "Title" },
    { key: "artist", label: "Artist" },
    { key: "album", label: "Album" },
    { key: "year", label: "Year" },
    { key: "genre", label: "Genre" },
    { key: "comment", label: "Comment" },
  ];

  const example = $derived(`${expandTemplate(effectiveTemplate(s), exampleStem, s.format.kind, { number: 1, count: 1, date: new Date() })}.${extensionOf(s.format.kind)}`);
  const sampleSummary = (f: OutputFormat) => {
    const rate = f.sampleRate ? `${f.sampleRate} Hz` : "same rate";
    const ch = f.channels === 1 ? `mono${f.mixLeft !== 0.5 || f.mixRight !== 0.5 ? ` (L ${Math.round(f.mixLeft * 100)}%, R ${Math.round(f.mixRight * 100)}%)` : ""}` : f.channels === 2 ? "stereo" : "same channels";
    const depth = f.kind === "mp3" || f.kind === "m4a" ? "" : f.bitDepth === 32 ? ", 32-bit float" : f.bitDepth ? `, ${f.bitDepth}-bit${f.dither ? "" : ", no dither"}` : ", same depth";
    return `${rate}, ${ch}${depth}`;
  };
  const encoderSummary = $derived(
    s.format.kind === "mp3"
      ? s.format.mp3Mode === "cbr"
        ? `MP3 ${s.format.mp3Bitrate} kbps constant`
        : `MP3 variable, quality ${s.format.mp3Quality} (0 is best)`
      : s.format.kind === "m4a"
        ? `AAC ${s.format.aacBitrate} kbps`
        : s.format.kind === "flac"
          ? "FLAC, lossless"
          : "WAV, uncompressed",
  );

  function insertToken(token: string) {
    const el = templateInput;
    const at = el?.selectionStart ?? s.template.length;
    const to = el?.selectionEnd ?? at;
    s.template = s.template.slice(0, at) + token + s.template.slice(to);
    requestAnimationFrame(() => {
      el?.focus();
      el?.setSelectionRange(at + token.length, at + token.length);
    });
  }

  async function browse() {
    const f = await pickFolder(s.folder);
    if (f) s.folder = f;
  }
  const available = (k: (typeof FORMAT_KINDS)[number]["value"]) => formatAvailable(k, PLATFORM);
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && !sampleOpen && onClose()} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={onClose}>
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="bo-title" tabindex="-1" onclick={(e) => e.stopPropagation()}>
    <h1 id="bo-title">Output Settings</h1>

    <section>
      <div class="grid">
        <span class="lbl">Naming</span>
        <div class="line">
          <label class="check"><input type="radio" name="naming" checked={s.naming === "affix"} onchange={() => (s.naming = "affix")} /> Add before and after</label>
          <label class="check"><input type="radio" name="naming" checked={s.naming === "template"} onchange={() => (s.naming = "template")} /> Template</label>
        </div>
        {#if s.naming === "affix"}
          <span class="lbl">Before</span>
          <div class="line">
            <input type="text" spellcheck="false" bind:value={s.prefix} placeholder="Nothing" />
            <span class="lbl small">After</span>
            <input type="text" spellcheck="false" bind:value={s.suffix} placeholder="Nothing" />
          </div>
        {:else}
          <span class="lbl">Template</span>
          <div>
            <input class="wide" type="text" spellcheck="false" placeholder={"{name}"} bind:value={s.template} bind:this={templateInput} />
            <div class="tokens" role="group" aria-label="Insert into the template">
              {#each TOKENS as t (t.token)}<button type="button" title={t.hint} onclick={() => insertToken(t.token)}>{t.token}</button>{/each}
            </div>
          </div>
        {/if}
        <span class="lbl">Example</span>
        <span class="mono">{example}</span>
      </div>
    </section>

    <section>
      <div class="grid">
        <span class="lbl">Save to</span>
        <div class="line">
          <input class="wide" type="text" readonly value={s.folder ?? ""} placeholder="Next to each original" disabled={s.folder === null} title={s.folder ?? ""} />
          <button type="button" disabled={s.folder === null} onclick={() => void browse()}>Browse…</button>
        </div>
        <span></span>
        <div class="stack">
          <label class="check">
            <input
              type="checkbox"
              checked={s.folder === null}
              onchange={async (e) => {
                if (e.currentTarget.checked) s.folder = null;
                else await browse();
              }}
            />
            Next to each original
          </label>
          <label class="check"><input type="checkbox" bind:checked={s.overwrite} /> Replace existing files</label>
        </div>
      </div>
    </section>

    <section>
      <div class="grid">
        <span class="lbl">Format</span>
        <select bind:value={s.format.kind}>
          {#each FORMAT_KINDS as k (k.value)}
            <option value={k.value} disabled={!available(k.value)}>{k.label}{available(k.value) ? "" : " (Windows only for now)"}</option>
          {/each}
        </select>
        <span class="lbl">Sample settings</span>
        <div class="line">
          <span class="box">{sampleSummary(s.format)}</span>
          <button type="button" onclick={() => (sampleOpen = true)}>Change…</button>
        </div>
        <span class="lbl">Encoder</span>
        <div class="stack">
          <span class="box">{encoderSummary}</span>
          {#if s.format.kind === "mp3"}
            <div class="line">
              <select bind:value={s.format.mp3Mode} aria-label="Bit rate mode">
                <option value="cbr">Constant bit rate</option>
                <option value="vbr">Variable bit rate</option>
              </select>
              {#if s.format.mp3Mode === "cbr"}
                <select bind:value={s.format.mp3Bitrate} aria-label="Kbps">
                  {#each MP3_BITRATES as b (b)}<option value={b}>{b} kbps</option>{/each}
                </select>
              {:else}
                <select bind:value={s.format.mp3Quality} aria-label="Quality">
                  {#each [0, 1, 2, 3, 4, 5, 6, 7, 8, 9] as q (q)}<option value={q}>Quality {q}{q === 0 ? " (best)" : q === 9 ? " (smallest)" : ""}</option>{/each}
                </select>
              {/if}
            </div>
          {:else if s.format.kind === "m4a"}
            <select bind:value={s.format.aacBitrate} aria-label="Kbps">
              {#each AAC_BITRATES as b (b)}<option value={b}>{b} kbps{b === 128 ? " (spoken word)" : b === 192 ? " (music)" : ""}</option>{/each}
            </select>
          {/if}
        </div>
      </div>
    </section>

    <section class="stack">
      <label class="check"><input type="checkbox" bind:checked={s.removeDone} /> Take finished files off the list</label>
      <label class="check" title="Title, artist, album and so on. WAV markers carry over when the output is WAV too."><input type="checkbox" bind:checked={s.copyTags} /> Keep each file's tags and markers</label>
      <button type="button" class="more" aria-expanded={tagsOpen} onclick={() => (tagsOpen = !tagsOpen)}>{tagsOpen ? "▾" : "▸"} Set tags on every file</button>
      {#if tagsOpen}
        <div class="grid">
          {#each TAG_FIELDS as t (t.key)}
            <span class="lbl">{t.label}</span>
            <input type="text" spellcheck="false" placeholder={s.copyTags ? "Keep" : ""} bind:value={s.tags[t.key]} />
          {/each}
        </div>
        <p class="note">Filled fields replace the file's own and can use the name tokens. Tags go into WAV, FLAC, MP3 and M4A.</p>
      {/if}
    </section>

    <div class="buttons">
      <button type="button" onclick={onClose}>Cancel</button>
      <button type="button" class="primary" onclick={() => onApply($state.snapshot(s) as BatchSettings)}>OK</button>
    </div>
  </div>
</div>

{#if sampleOpen}
  <SampleSettingsDialog
    format={s.format}
    onApply={(f) => {
      s.format = f;
      sampleOpen = false;
    }}
    onClose={() => (sampleOpen = false)}
  />
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 5000;
    display: grid;
    place-items: center;
    background: rgba(0, 0, 0, 0.5);
  }
  .dialog {
    display: grid;
    gap: 10px;
    width: min(520px, 94vw);
    max-height: 92vh;
    overflow: auto;
    padding: 14px;
    border: 1px solid var(--border-strong);
    background: var(--surface-2);
    box-shadow: 0 16px 40px rgba(0, 0, 0, 0.6);
    font-size: 12px;
  }
  h1 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
  }
  section {
    padding-bottom: 10px;
    border-bottom: 1px solid var(--border);
  }
  .grid {
    display: grid;
    grid-template-columns: 104px minmax(0, 1fr);
    gap: 6px 10px;
    align-items: center;
  }
  .lbl {
    color: var(--text-dim);
    text-align: right;
  }
  .lbl.small {
    flex: none;
  }
  .line {
    display: flex;
    gap: 6px;
    align-items: center;
    min-width: 0;
  }
  .line input,
  .wide {
    flex: 1;
    width: 100%;
    min-width: 0;
  }
  .stack {
    display: grid;
    gap: 5px;
  }
  .check {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .box {
    flex: 1;
    padding: 3px 6px;
    border: 1px solid var(--border);
    border-radius: 2px;
    background: var(--surface-1);
    color: var(--text-dim);
  }
  .mono {
    overflow: hidden;
    color: var(--text-dim);
    font-family: var(--font-mono, monospace);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .tokens {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin-top: 4px;
  }
  input[type="text"],
  select,
  button {
    height: 24px;
    padding: 0 6px;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    background: var(--surface-1);
    color: var(--text);
    font: inherit;
  }
  input:disabled {
    opacity: 0.55;
  }
  .more {
    justify-self: start;
    border: 0;
    background: none;
    color: var(--text-dim);
    padding: 0;
  }
  .note {
    margin: 0;
    color: var(--text-faint);
  }
  .buttons {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
  }
  .buttons button {
    min-width: 72px;
  }
  .primary {
    border-color: var(--accent);
    background: var(--accent);
    color: var(--on-accent, #fff);
  }
</style>
