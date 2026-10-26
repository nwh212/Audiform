<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  The strip under the menu bar: which editor shows (Waveform or
  Multitrack) and the editing tools. Tools that only make sense in the
  multitrack are disabled while the Waveform editor shows. Every button
  runs a named action, so the menus and shortcuts do the same thing.
-->
<script lang="ts">
  import type { ActionRegistry } from "./actions";
  import { editor, type EditorState, type EditorTool } from "../stores/editor";

  interface Props {
    actions: ActionRegistry;
    shortcutFor?: (id: string) => string | undefined;
  }

  const { actions, shortcutFor = () => undefined }: Props = $props();

  let ed = $state<EditorState>(editor.current());
  $effect(() =>
    editor.subscribe((s) => {
      ed = s;
    }),
  );

  const tip = (label: string, id: string, more = "") => {
    const k = shortcutFor(id);
    return `${label}${k ? ` (${k})` : ""}${more ? `. ${more}` : ""}`;
  };

  const TOOLS: { id: EditorTool; label: string; help: string; multitrackOnly: boolean }[] = [
    { id: "move", label: "Move", help: "Select and drag clips; drag empty space to select time", multitrackOnly: true },
    { id: "razor", label: "Razor", help: "Click a clip to split it there; Shift splits every track", multitrackOnly: true },
    { id: "slip", label: "Slip", help: "Drag inside a clip to move its audio without moving the clip", multitrackOnly: true },
    { id: "time", label: "Time Selection", help: "Drag anywhere, even over clips, to select time", multitrackOnly: false },
  ];

  const isWave = $derived(ed.mode === "waveform");
  const activeTool = $derived(isWave ? ed.spectralTool : ed.tool);
  const spectral = $derived(isWave && (ed.showSpectrum || ed.showPitch));

  const SPECTRAL: { id: "box" | "lasso" | "brush" | "spot"; label: string; help: string }[] = [
    { id: "box", label: "Box Select", help: "Drag a box on the spectrum to select an area of time and frequency" },
    { id: "lasso", label: "Lasso Select", help: "Draw around a sound on the spectrum to select it" },
    { id: "brush", label: "Brush Select", help: "Paint over a sound on the spectrum to select it; Shift adds to the selection" },
    { id: "spot", label: "Spot Repair", help: "Paint over a short sound (click, cough, chirp) to fill it from the audio around it" },
  ];
</script>

<div class="strip" role="toolbar" aria-label="Editor and tools">
  <div class="modes" role="radiogroup" aria-label="Editor">
    <button
      type="button"
      role="radio"
      aria-checked={isWave}
      class:on={isWave}
      title={tip("Waveform Editor", "view.editor.waveform", "Edit one audio file")}
      onclick={() => void actions.run("view.editor.waveform")}
    >
      <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M1 8h2l1.5-4 2 8 2-10 2 9 1.5-3H15" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" /></svg>
      Waveform
    </button>
    <button
      type="button"
      role="radio"
      aria-checked={!isWave}
      class:on={!isWave}
      title={tip("Multitrack Editor", "view.editor.multitrack", "Arrange and mix the session")}
      onclick={() => void actions.run("view.editor.multitrack")}
    >
      <svg viewBox="0 0 16 16" aria-hidden="true"><rect x="1.5" y="2.5" width="9" height="3" rx="1" fill="currentColor" /><rect x="5.5" y="6.5" width="9" height="3" rx="1" fill="currentColor" opacity="0.7" /><rect x="2.5" y="10.5" width="7" height="3" rx="1" fill="currentColor" opacity="0.5" /></svg>
      Multitrack
    </button>
  </div>

  <span class="sep"></span>

  <div class="views" role="group" aria-label="Waveform editor views">
    <button
      type="button"
      class="tool"
      class:on={isWave && ed.showSpectrum}
      aria-pressed={ed.showSpectrum}
      aria-label="Spectrum"
      disabled={!isWave}
      title={tip("Spectrum", "view.spectrum", "Show frequencies over time under the waveform")}
      onclick={() => void actions.run("view.spectrum")}
    >
      <svg viewBox="0 0 16 16" aria-hidden="true"><rect x="1.5" y="2" width="13" height="12" rx="1" fill="none" stroke="currentColor" stroke-width="1.1" /><path d="M3.5 11.5h2M3.5 9h4M6.5 6.5h5M8 4.5h3M9.5 11.5h3" stroke="currentColor" stroke-width="1.4" /></svg>
    </button>
    <button
      type="button"
      class="tool"
      class:on={isWave && ed.showPitch}
      aria-pressed={ed.showPitch}
      aria-label="Pitch"
      disabled={!isWave}
      title={tip("Pitch", "view.pitch", "Draw the voice's pitch over the spectrum")}
      onclick={() => void actions.run("view.pitch")}
    >
      <svg viewBox="0 0 16 16" aria-hidden="true"><rect x="1.5" y="2" width="13" height="12" rx="1" fill="none" stroke="currentColor" stroke-width="1.1" /><path d="M3 10.5c1.5 0 1.8-4 3.4-4s1.6 3 3.2 3 1.5-3.5 3.4-3.5" fill="none" stroke="currentColor" stroke-width="1.4" /></svg>
    </button>
  </div>

  <span class="sep"></span>

  <div class="tools" role="radiogroup" aria-label="Tool">
    {#each TOOLS as t (t.id)}
      <button
        type="button"
        role="radio"
        aria-checked={activeTool === t.id}
        aria-label={t.label}
        class="tool"
        class:on={activeTool === t.id}
        disabled={isWave && t.multitrackOnly}
        title={tip(t.label, `tool.${t.id}`, t.help)}
        onclick={() => void actions.run(`tool.${t.id}`)}
      >
        {#if t.id === "move"}
          <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3 2l8 5.5-3.6.6 2.2 4.4-1.6.8-2.2-4.4L3 11.6z" fill="currentColor" /><path d="M12 10v5M9.5 12.5h5" stroke="currentColor" stroke-width="1.2" /></svg>
        {:else if t.id === "razor"}
          <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4 14L11.5 2.5l2 1.3L6 15.2z" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round" /><path d="M8.2 7.5l2 1.3" stroke="currentColor" stroke-width="1.2" /><path d="M1 9h4" stroke="currentColor" stroke-width="1.2" stroke-dasharray="1.5 1.5" /></svg>
        {:else if t.id === "slip"}
          <svg viewBox="0 0 16 16" aria-hidden="true"><rect x="1.5" y="4" width="13" height="8" rx="1" fill="none" stroke="currentColor" stroke-width="1.2" /><path d="M3 8c1-2 2 2 3 0s2 2 3 0 2 2 3 0" fill="none" stroke="currentColor" stroke-width="1" opacity="0.7" /><path d="M5 1.8h6M5 1.8l1.4-1.2M5 1.8l1.4 1.2M11 1.8l-1.4-1.2M11 1.8l-1.4 1.2" fill="none" stroke="currentColor" stroke-width="1" /></svg>
        {:else}
          <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M6 2.5h4M6 13.5h4M8 2.5v11" stroke="currentColor" stroke-width="1.4" /></svg>
        {/if}
      </button>
    {/each}
    <span class="sep thin"></span>
    {#each SPECTRAL as t (t.id)}
      <button
        type="button"
        role="radio"
        aria-checked={activeTool === t.id}
        aria-label={t.label}
        class="tool"
        class:on={isWave && activeTool === t.id}
        disabled={!isWave}
        title={tip(t.label, `tool.${t.id}`, isWave ? t.help : `${t.help}. Open a file in the Waveform editor first.`)}
        onclick={() => void actions.run(`tool.${t.id}`)}
      >
        {#if t.id === "box"}
          <svg viewBox="0 0 16 16" aria-hidden="true"><rect x="2.5" y="3.5" width="11" height="9" fill="none" stroke="currentColor" stroke-width="1.3" stroke-dasharray="2 1.5" /></svg>
        {:else if t.id === "lasso"}
          <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M8 3c3.5 0 5.5 1.6 5.5 3.4S11 9.8 8 9.8 2.5 8.3 2.5 6.4 4.5 3 8 3z" fill="none" stroke="currentColor" stroke-width="1.2" stroke-dasharray="2 1.4" /><path d="M5 9.4c-.5 1.2.2 2.2 1.2 2.4M6.2 11.8l-.8 2.4" fill="none" stroke="currentColor" stroke-width="1.2" /></svg>
        {:else if t.id === "brush"}
          <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M13.5 2.5L7.5 8.5" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" /><path d="M7 9c-2 0-3 1.2-3 2.8 0 .9-.6 1.5-1.5 1.7 2.8 1 5.6-.4 5.6-3.2z" fill="currentColor" /></svg>
        {:else}
          <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M12.5 2.5L8 7" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" /><path d="M7.5 7.5c-1.6 0-2.4 1-2.4 2.3" fill="none" stroke="currentColor" stroke-width="1.6" /><path d="M2 12.5h3M3.5 11v3M10.5 11.5l1.5 1.5M12 11.5l-1.5 1.5" stroke="currentColor" stroke-width="1.1" /></svg>
        {/if}
      </button>
    {/each}
  </div>

  {#if spectral && (ed.spectralTool === "brush" || ed.spectralTool === "spot")}
    <label class="size" title="Brush size in pixels">
      <span>Size</span>
      <input type="range" min="2" max="80" value={ed.brushSize} oninput={(e) => editor.setBrushSize(Number(e.currentTarget.value))} />
      <span class="num">{ed.brushSize} px</span>
    </label>
  {/if}
</div>

<style>
  .strip {
    display: flex;
    gap: 8px;
    align-items: center;
    height: 30px;
    padding: 0 8px;
    border-bottom: 1px solid var(--border);
    background: var(--surface-1);
  }

  .modes {
    display: flex;
    overflow: hidden;
    border: 1px solid var(--border-strong);
    border-radius: 4px;
  }

  .modes button {
    display: flex;
    gap: 6px;
    align-items: center;
    height: 22px;
    padding: 0 10px;
    border: 0;
    background: var(--surface-2);
    color: var(--text-dim);
    font: inherit;
    font-size: var(--font-size-small);
  }

  .modes button + button {
    border-left: 1px solid var(--border-strong);
  }

  .modes button.on {
    background: var(--surface-3);
    color: var(--text);
    box-shadow: inset 0 -2px 0 var(--accent);
  }

  .modes svg {
    width: 14px;
    height: 14px;
  }

  .sep {
    width: 1px;
    height: 18px;
    background: var(--border);
  }

  .tools,
  .views {
    display: flex;
    gap: 2px;
    align-items: center;
  }

  .sep.thin {
    margin: 0 3px;
  }

  .size {
    display: flex;
    gap: 6px;
    align-items: center;
    color: var(--text-dim);
    font-size: var(--font-size-small);
  }

  .size input {
    width: 110px;
    accent-color: var(--accent);
  }

  .size .num {
    min-width: 36px;
    color: var(--value);
    font-family: var(--font-mono);
  }

  .tool {
    display: grid;
    place-items: center;
    width: 28px;
    height: 24px;
    border: 1px solid transparent;
    border-radius: 3px;
    background: none;
    color: var(--text-dim);
  }

  .tool svg {
    width: 15px;
    height: 15px;
  }

  .tool:hover:not(:disabled) {
    border-color: var(--border-strong);
    color: var(--text);
  }

  .tool.on {
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 18%, transparent);
    color: var(--text);
  }

  .tool:disabled {
    opacity: 0.35;
  }
</style>
