<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The Audiform contributors.
  This file is part of Audiform, based on Altered Audio Editor.

  An add-on's window. The add-on's page fills it, in a frame of its own
  that cannot reach the app, the network or the computer. The page and
  Audiform talk through messages: Audiform tells it what is selected and
  what the theme looks like; the page asks Audiform to run the add-on's
  tool, to bring result files into the Files panel, to open a file or
  folder dialog, to open one of its own folders, to show a notice or to keep its settings. While the tool
  runs, a Stop button sits under the page, so a run can always be ended.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { addonSelection, parseAddonRequest, type AddonSelection } from "../../app/addons";
  import { addonCancel, addonOpenFolder, addonPageUrl, addonPick, addonRun, onAddonOutput, type AddonInfo } from "../../app/core";
  import type { SeparationSource } from "../../app/separation-tools.svelte";
  import { editor } from "../../stores/editor";
  import { mediaPool } from "../../stores/media";
  import { session } from "../../stores/open-session";
  import { prefs } from "../../stores/prefs";
  import { waveFile } from "../../stores/wave";

  interface Props {
    addon: AddonInfo;
    /** What the add-on would work on: the Waveform file, the selected clips or the selected files. */
    source: () => SeparationSource | null;
    /** Brings result files into the Files panel. */
    onImport: (paths: string[]) => Promise<void>;
    onNotice: (text: string) => void;
    onError: (e: unknown) => void;
  }

  const { addon, source, onImport, onNotice, onError }: Props = $props();

  const key = () => `addon.${addon.id}`;
  let frame = $state<HTMLIFrameElement | undefined>();
  /** The id the page gave the run in progress, if any. */
  let running = $state<string | null>(null);
  let selection: AddonSelection | null = null;

  const post = (msg: unknown) => frame?.contentWindow?.postMessage(msg, "*");

  /** The theme's colors and type, so a page can match the app. */
  const THEME_VARS = ["surface-0", "surface-1", "surface-2", "surface-3", "border", "border-strong", "text", "text-dim", "text-faint", "accent", "value", "danger", "font-ui", "font-mono", "font-size"] as const;
  function themeOf(): Record<string, string> {
    const cs = getComputedStyle(document.documentElement);
    return Object.fromEntries(THEME_VARS.map((v) => [v, cs.getPropertyValue(`--${v}`).trim()]));
  }

  function hello() {
    selection ??= addonSelection(source(), session.current);
    post({
      type: "hello",
      addon: { id: addon.id, name: addon.name, version: addon.version, hasTool: addon.hasTool },
      selection,
      theme: themeOf(),
      data: prefs.raw(key()) ?? null,
      running,
    });
  }

  function refreshSelection() {
    const next = addonSelection(source(), session.current);
    if (selection && JSON.stringify(next) === JSON.stringify(selection)) return;
    selection = next;
    post({ type: "selection", selection });
  }

  async function run(id: string, args: string[], input: string) {
    if (running) {
      post({ type: "failed", run: id, error: "The tool is already running." });
      return;
    }
    running = id;
    const off = await onAddonOutput((p) => {
      if (p.id === addon.id) post({ type: "output", run: id, line: p.line, err: p.err });
    });
    try {
      const r = await addonRun(addon.id, args, input);
      post({ type: "done", run: id, code: r.code, lines: r.lines });
    } catch (e) {
      post({ type: "failed", run: id, error: String(e) });
    } finally {
      off();
      running = null;
    }
  }

  async function onMessage(e: MessageEvent) {
    if (!frame || e.source !== frame.contentWindow) return;
    const req = parseAddonRequest(e.data);
    if (!req) return;
    switch (req.type) {
      case "ready":
        hello();
        break;
      case "run":
        void run(req.run, req.args, req.input);
        break;
      case "cancel":
        void addonCancel().catch(onError);
        break;
      case "import":
        try {
          await onImport(req.paths);
          post({ type: "imported", request: req.request, ok: true });
        } catch (err) {
          post({ type: "imported", request: req.request, ok: false, error: String(err) });
        }
        break;
      case "pick":
        try {
          post({ type: "picked", request: req.request, path: await addonPick(req.folder, req.title) });
        } catch (err) {
          post({ type: "picked", request: req.request, path: null, error: String(err) });
        }
        break;
      case "openFolder":
        void addonOpenFolder(addon.id, req.path).catch(onError);
        break;
      case "notice":
        onNotice(req.text);
        break;
      case "save":
        prefs.set(key(), req.data);
        break;
    }
  }

  onMount(() => {
    window.addEventListener("message", onMessage);
    const offs = [editor.subscribe(refreshSelection), mediaPool.subscribe(refreshSelection), session.subscribe(refreshSelection), waveFile.subscribe(refreshSelection)];
    // The theme can change while the window is open.
    const watch = new MutationObserver(() => post({ type: "theme", theme: themeOf() }));
    watch.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme", "class", "style"] });
    return () => {
      window.removeEventListener("message", onMessage);
      for (const off of offs) off();
      watch.disconnect();
      if (running) void addonCancel().catch(() => {});
    };
  });
</script>

<div class="addon">
  <iframe bind:this={frame} title={addon.title} src={addonPageUrl(addon.id, addon.entry)} sandbox="allow-scripts" referrerpolicy="no-referrer"></iframe>
  {#if running}
    <div class="bar">
      <span>Running {addon.name}...</span>
      <button type="button" onclick={() => void addonCancel().catch(onError)}>Stop</button>
    </div>
  {/if}
</div>

<style>
  .addon {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--surface-1);
  }
  iframe {
    flex: 1;
    width: 100%;
    min-height: 0;
    border: 0;
    background: var(--surface-1);
  }
  .bar {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: 4px 8px;
    border-top: 1px solid var(--border);
    color: var(--text-dim);
    font-size: var(--font-size-small);
  }
  .bar span {
    flex: 1;
  }
</style>
