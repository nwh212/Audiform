<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  Properties: the selected clip's details and its stretch and pitch.
  Stretch makes a clip play longer or shorter; pitch moves it up or down
  in semitones without changing its length. Source Channels sets how
  much of each of the file's channels goes to the left and right. The type picks how: a
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
  import { channelLabels, mixOf, presetFor, presets, toSpeakers } from "./channel-mix";
  import type { ChannelMixDoc } from "../../stores/session";

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
  // Source Channels: one setting for every selected clip whose file has the same channels.
  const chans = $derived(media?.channels ?? 0);
  const sameChans = $derived(picked.every((p) => doc?.media.find((m) => m.id === p.clip.media_id)?.channels === chans));
  const mix = $derived(first && chans > 0 ? mixOf(first.clip.channel_mix, chans) : null);
  const names = $derived(channelLabels(chans));
  const quick = $derived(presets(chans));
  const current = $derived(mix ? presetFor(mix, chans) : null);
  const custom = $derived(picked.some((p) => p.clip.channel_mix));
  // 3D Space sessions: each channel from its own speaker (the default for files of more than two channels).
  const in3d = $derived(!!doc?.space);
  const speakers = $derived(in3d && first !== null && chans > 1 && toSpeakers(first.clip.channel_mix, chans));

  function setMix(next: ChannelMixDoc | null, label = "Source Channels") {
    const changes: ClipChange[] = picked.map(({ clip }) => (next ? { id: clip.id, channel_mix: next } : { id: clip.id, clear_channel_mix: true }));
    if (changes.length) void onEdit(label, changes).catch(onError);
  }
  function setSend(k: number, side: "left" | "right", pct: number) {
    if (!mix || !Number.isFinite(pct)) return;
    const sends = mix.sends.map((s) => ({ ...s }));
    sends[k]![side] = Math.max(-100, Math.min(100, pct)) / 100;
    setMix({ sends });
  }
  function invert(k: number) {
    if (!mix) return;
    setMix({ sends: mix.sends.map((s, i) => (i === k ? { left: -s.left, right: -s.right } : { ...s })) });
  }
  const pct = (v: number) => Math.round(v * 1000) / 10;

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

    <h2>Source Channels</h2>
    {#if !mix}
      <p class="note">The clip's file is missing.</p>
    {:else if !sameChans}
      <p class="note">The selected clips' files have different channels. Select clips of one kind to set their Source Channels.</p>
    {:else}
      <p class="note">{chans} channel{chans === 1 ? "" : "s"}. How much of each goes to the track's left and right. Negative flips polarity.</p>
      {#if in3d && chans > 1}
        <label class="check" title="Each channel plays straight from the speaker of the same name (L to L, C to C, LFE, Ls...), with the track's fader, mute and Audio FX. A channel the layout lacks (heights in a 5.1 session, say) folds into the speakers nearest where it would stand. Also on the clip's right-click menu.">
          <input type="checkbox" checked={speakers} onchange={(e) => mix && setMix({ sends: mix.sends, to_speakers: e.currentTarget.checked }, e.currentTarget.checked ? "Pass Through to Speakers" : "Place Clip in the Room")} />
          Pass through to speakers (each channel from its own speaker)
        </label>
      {/if}
      <div class="grid" class:dim={speakers}>
        <label for="pp-mixp">Preset</label>
        <select id="pp-mixp" value={current ?? ""} onchange={(e) => { const p = quick.find((x) => x.id === e.currentTarget.value); if (p) setMix(p.id === "default" && !in3d ? null : p.mix, `Source Channels: ${p.label}`); }}>
          {#if !current}<option value="" disabled>Custom</option>{/if}
          {#each quick as p (p.id)}<option value={p.id}>{p.label}</option>{/each}
        </select>
      </div>
      <table class="mixer" class:dim={speakers}>
        <thead><tr><th>Channel</th><th>Left</th><th>Right</th><th></th></tr></thead>
        <tbody>
          {#each mix.sends as s, k (k)}
            <tr>
              <th scope="row">{names[k]}</th>
              {#each ["left", "right"] as const as side (side)}
                <td>
                  <span class="send">
                    <span class="bar" class:neg={s[side] < 0} style:width="{Math.abs(s[side]) * 100}%"></span>
                    <input type="number" min="-100" max="100" step="1" aria-label="{names[k]} to {side}" value={pct(s[side])} onchange={(e) => setSend(k, side, num(e))} />
                  </span>
                </td>
              {/each}
              <td><button type="button" class="inv" class:on={s.left < 0 || s.right < 0} title="Flip this channel's polarity" aria-label="Flip {names[k]} polarity" onclick={() => invert(k)}>ø</button></td>
            </tr>
          {/each}
        </tbody>
      </table>
      <div class="quick">
        <button type="button" disabled={!custom} onclick={() => setMix(null, "Source Channels: Reset")}>Reset</button>
      </div>
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
  .dim {
    opacity: 0.4;
  }
  .mixer {
    border-collapse: collapse;
    width: 100%;
  }
  .mixer th,
  .mixer td {
    padding: 1px 4px 1px 0;
    font-weight: normal;
    text-align: left;
  }
  .mixer thead th {
    color: var(--text-faint);
  }
  .mixer tbody th {
    color: var(--text-dim);
    font-family: var(--font-mono);
  }
  .send {
    position: relative;
    display: block;
  }
  .send input {
    position: relative;
    width: 100%;
    max-width: 72px;
    background: transparent;
  }
  .send .bar {
    position: absolute;
    top: 1px;
    bottom: 1px;
    left: 1px;
    max-width: calc(min(100%, 72px) - 2px);
    background: color-mix(in srgb, var(--accent) 30%, transparent);
    pointer-events: none;
  }
  .send .bar.neg {
    background: color-mix(in srgb, var(--danger) 35%, transparent);
  }
  .inv {
    width: 22px;
    padding: 0;
  }
  .inv.on {
    border-color: var(--danger);
    color: var(--danger);
  }
  details summary {
    color: var(--text-dim);
    cursor: pointer;
  }
  details .grid {
    margin-top: 6px;
  }
</style>
