<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  Properties: the selected clip's details and its stretch and pitch.
  Stretch makes a clip play longer or shorter; pitch moves it up or down
  in semitones without changing its length. The type picks how: a
  single voice or instrument, music with several notes at once, or
  varispeed, where the pitch follows the speed like tape. Live and
  Rendered sound the same here; Rendered is kept so files from other
  editors keep their setting. With several clips selected, a change
  applies to all of them. Every change is one undoable step.
-->
<script lang="ts">
  import { editor, type EditorState } from "../../stores/editor";
  import { clipLength, type ClipChange, type ClipDoc, type SessionDoc, type SessionStore } from "../../stores/session";
  import { formatIn } from "../editor/timeline";
  import { naturalLength } from "../editor/stretch";
  import { changeFor, settingsOf, varispeedSemitones, type StretchSettings } from "./clip-props";

  interface Props {
    session: SessionStore;
    onEdit: (label: string, changes: ClipChange[]) => Promise<void>;
    onError?: (e: unknown) => void;
  }

  const { session, onEdit, onError = console.error }: Props = $props();

  let doc = $state<SessionDoc | null>(null);
  $effect(() => session.subscribe((d) => (doc = d)));
  let ed = $state<EditorState>(editor.current());
  $effect(() => editor.subscribe((s) => (ed = s)));

  const picked = $derived.by(() => {
    const out: { clip: ClipDoc; track: string }[] = [];
    for (const t of doc?.tracks ?? []) for (const c of t.clips) if (ed.selectedClips.has(c.id)) out.push({ clip: c, track: t.name });
    return out;
  });
  const first = $derived(picked[0] ?? null);
  const rate = $derived(doc?.sample_rate ?? 48_000);
  const media = $derived(first ? doc?.media.find((m) => m.id === first.clip.media_id) : undefined);
  const t = (s: number) => formatIn(s, rate, ed.timeFormat);
  const st = $derived<StretchSettings | null>(first ? settingsOf(first.clip) : null);
  const looped = $derived(picked.some((p) => p.clip.looped));

  async function apply(patch: Partial<StretchSettings>, label = "Clip Stretch and Pitch") {
    const d = doc;
    if (!d || !st) return;
    const next: StretchSettings = { ...st, ...patch, advanced: { ...st.advanced, ...(patch.advanced ?? {}) } };
    // Turning on a pitch or a length from Off starts a live stretch.
    if (st.mode === "off" && patch.mode === undefined && (patch.percent !== undefined || patch.pitch !== undefined || patch.type !== undefined)) next.mode = "live";
    const changes = picked.flatMap(({ clip }) => {
      const m = d.media.find((x) => x.id === clip.media_id);
      const ch = changeFor(clip, m, d.sample_rate, { ...settingsOf(clip), ...next, percent: next.percent, advanced: next.advanced });
      return ch ? [ch] : [];
    });
    if (changes.length) await onEdit(label, changes).catch(onError);
  }

  const num = (e: Event) => Number((e.currentTarget as HTMLInputElement).value);
  function setPercent(e: Event) {
    const v = num(e);
    if (Number.isFinite(v) && v > 0) void apply({ percent: Math.max(25, Math.min(400, v)) });
  }
  function setPitch(e: Event) {
    const v = num(e);
    if (Number.isFinite(v)) void apply({ pitch: Math.max(-24, Math.min(24, v)) });
  }
  const tone = (v: number) => `${v > 0 ? "+" : ""}${(Math.round(v * 100) / 100).toFixed(2)}`;
</script>

<section class="props" aria-label="Properties">
  {#if !first || !st}
    <p class="empty">Select a clip in the Multitrack editor to see its properties.</p>
  {:else}
    <header>
      <div class="name" title={first.clip.name}>{first.clip.name}</div>
      <div class="sub">{picked.length > 1 ? `${picked.length} clips selected; changes apply to all of them` : first.track}</div>
    </header>
    <dl class="facts">
      <dt>Start</dt><dd>{t(first.clip.start)}</dd>
      <dt>Length</dt><dd>{t(clipLength(first.clip, media, rate))}</dd>
      <dt>Natural length</dt><dd>{t(naturalLength(first.clip, media, rate))}</dd>
      <dt>File</dt><dd title={media?.path ?? ""}>{media?.path.split(/[\\/]/).pop() ?? "Missing"}</dd>
    </dl>

    <h2>Stretch and Pitch</h2>
    {#if looped}
      <p class="note">Looped clips cannot stretch. Turn off Loop Clip first.</p>
    {:else}
      <div class="grid">
        <label for="pp-mode">Mode</label>
        <select id="pp-mode" value={st.mode} onchange={(e) => void apply({ mode: e.currentTarget.value as StretchSettings["mode"] })}>
          <option value="off">Off</option>
          <option value="live">Live</option>
          <option value="rendered">Rendered</option>
        </select>

        <label for="pp-type">Type</label>
        <select id="pp-type" value={st.type} onchange={(e) => void apply({ type: e.currentTarget.value as StretchSettings["type"] })}>
          <option value="mono" title="One voice or instrument at a time: speech, a lead vocal">Single voice</option>
          <option value="poly" title="Several notes at once: music, chords, a full mix">Several notes</option>
          <option value="varispeed" title="Pitch follows speed, like tape running faster or slower">Varispeed</option>
        </select>

        <label for="pp-pct">Stretch</label>
        <span class="field">
          <input id="pp-pct" type="number" min="25" max="400" step="0.1" value={st.percent} onchange={setPercent} />
          <span class="unit">%</span>
        </span>

        <label for="pp-pitch">Pitch</label>
        {#if st.type === "varispeed"}
          <span class="note" title="Varispeed sets the pitch by the speed">{tone(varispeedSemitones(st.percent / 100))} semitones (follows speed)</span>
        {:else}
          <span class="field">
            <input id="pp-pitch" type="number" min="-24" max="24" step="0.1" value={st.pitch} onchange={setPitch} />
            <span class="unit">semitones</span>
          </span>
        {/if}
      </div>
      {#if st.mode !== "off"}
        <div class="quick">
          <button type="button" onclick={() => void apply({ pitch: st.pitch - 1 })} disabled={st.type === "varispeed"}>-1 st</button>
          <button type="button" onclick={() => void apply({ pitch: st.pitch + 1 })} disabled={st.type === "varispeed"}>+1 st</button>
          <button type="button" onclick={() => void apply({ percent: 100, pitch: 0 })}>Reset</button>
        </div>
      {/if}

      <details>
        <summary>Advanced</summary>
        <div class="grid">
          <label for="pp-win" title="Length of each analysis frame. Longer suits music, shorter a single voice.">Window</label>
          <span class="field">
            <input id="pp-win" type="number" min="5" max="200" step="1" value={st.advanced.window_ms} onchange={(e) => { const v = num(e); if (Number.isFinite(v)) void apply({ advanced: { ...st.advanced, window_ms: Math.max(5, Math.min(200, v)) } }); }} />
            <span class="unit">ms</span>
          </span>
          <label for="pp-trans" title="How readily a sharp attack starts a new frame. Stored for files from other editors.">Transients</label>
          <input id="pp-trans" type="number" min="0" max="100" step="1" value={st.advanced.transient_sensitivity} onchange={(e) => { const v = num(e); if (Number.isFinite(v)) void apply({ advanced: { ...st.advanced, transient_sensitivity: Math.max(0, Math.min(100, v)) } }); }} />
          <span></span>
          <label class="check"><input type="checkbox" checked={st.advanced.preserve_formants} onchange={(e) => void apply({ advanced: { ...st.advanced, preserve_formants: e.currentTarget.checked } })} /> Keep voice character</label>
          <span></span>
          <label class="check"><input type="checkbox" checked={st.advanced.high_precision} onchange={(e) => void apply({ advanced: { ...st.advanced, high_precision: e.currentTarget.checked } })} /> High precision</label>
        </div>
        <p class="note">The window changes the sound now; the rest are kept with the clip for files that use them.</p>
      </details>
    {/if}
  {/if}
</section>

<style>
  .props {
    display: flex;
    flex-direction: column;
    gap: 8px;
    height: 100%;
    padding: 8px;
    overflow: auto;
    box-sizing: border-box;
    background: var(--surface-1);
    font-size: var(--font-size-small);
  }
  .empty,
  .note {
    margin: 0;
    color: var(--text-faint);
  }
  header .name {
    overflow: hidden;
    color: var(--text);
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  header .sub {
    color: var(--text-dim);
  }
  .facts {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 2px 10px;
    margin: 0;
  }
  dt {
    color: var(--text-dim);
  }
  dd {
    margin: 0;
    overflow: hidden;
    font-family: var(--font-mono);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  h2 {
    margin: 4px 0 0;
    padding-top: 6px;
    border-top: 1px solid var(--border);
    color: var(--text);
    font-size: inherit;
    font-weight: 600;
  }
  .grid {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 6px 10px;
    align-items: center;
  }
  label {
    color: var(--text-dim);
  }
  .check {
    display: flex;
    gap: 4px;
    align-items: center;
  }
  .field {
    display: flex;
    gap: 4px;
    align-items: center;
  }
  .unit {
    color: var(--text-faint);
  }
  select,
  input[type="number"] {
    min-width: 0;
    height: 22px;
    padding: 0 4px;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    background: var(--surface-2);
    color: var(--text);
    font: inherit;
  }
  input[type="number"] {
    width: 72px;
  }
  .quick {
    display: flex;
    gap: 6px;
  }
  button {
    height: 22px;
    padding: 0 8px;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    background: var(--surface-2);
    color: var(--text);
    font: inherit;
    cursor: pointer;
  }
  button:disabled {
    opacity: 0.5;
    cursor: default;
  }
  details summary {
    color: var(--text-dim);
    cursor: pointer;
  }
  details .grid {
    margin-top: 6px;
  }
</style>
