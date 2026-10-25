<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Altered Audio Editor.

  Edit > Settings: every app setting on one dialog, page by page. The
  whole dialog is one draft: OK applies it, Cancel or Escape drops it.
  Settings the app cannot act on yet are shown greyed out with the
  reason, never faked.
-->
<script lang="ts">
  import { cacheInfo, cacheClearPeaks, cacheSetFolder, libraryPickFolder, openSoundSettings, type CacheInfo } from "../core";
  import { audioDriver, audioInputs, audioOutputs, type AudioDriver, type AudioInput } from "../core";
  import { keepLast } from "./device-cache";
  import { BROADCAST_REGIONS, targetsFor } from "../loudness";
  import { DEFAULT_SETTINGS, NO_COLORS, TIMECODE_RATES, type AppColors } from "../../stores/settings";
  import { cleanDraft, LATER_PAGES, PAGES, sizeText, type Page, type SettingsDraft } from "./pages";

  interface Props {
    initial: SettingsDraft;
    page?: Page;
    onApply: (d: SettingsDraft) => void;
    onNotice?: (m: string) => void;
    onClose: () => void;
  }

  const { initial, page: startPage = "General", onApply, onNotice = () => {}, onClose }: Props = $props();

  // svelte-ignore state_referenced_locally
  let d = $state<SettingsDraft>($state.snapshot(initial) as SettingsDraft);
  // svelte-ignore state_referenced_locally
  let page = $state<Page>(startPage);

  let outputs = $state<{ name: string; isDefault: boolean }[] | null>(null);
  let inputs = $state<AudioInput[] | null>(null);
  let driver = $state<AudioDriver | null>(null);
  const systemDriverName = () => (navigator.userAgent.includes("Windows") ? "WASAPI" : navigator.userAgent.includes("Mac") ? "Core Audio" : "ALSA");
  let cache = $state<CacheInfo | null>(null);
  let busy = $state(false);
  // Device lookups run one after another, never together: a driver that
  // misbehaves under parallel calls can no longer take the app down.
  let devicesLoading = false;
  let devicesStale = $state(false);
  async function loadDevices() {
    if (devicesLoading) return;
    devicesLoading = true;
    try {
      const asio = d.app.driver === "asio";
      driver = await audioDriver().catch(() => ({ asioBuilt: false, asio: false, drivers: [] }));
      const o = keepLast(`out:${asio}`, await audioOutputs(asio).catch(() => []));
      const i = keepLast(`in:${asio}`, await audioInputs(asio).catch(() => []));
      outputs = o.list;
      inputs = i.list;
      devicesStale = o.stale || i.stale;
    } finally {
      devicesLoading = false;
    }
  }
  $effect(() => {
    if (page === "Audio Devices" && (outputs === null || inputs === null || driver === null)) void loadDevices();
    if (page === "Cache and Files" && cache === null) void refreshCache();
  });
  async function refreshCache() {
    cache = await cacheInfo().catch(() => null);
  }
  async function chooseCacheFolder(reset: boolean) {
    busy = true;
    try {
      const dir = reset ? null : await libraryPickFolder("Choose a folder for working files");
      if (!reset && !dir) return;
      await cacheSetFolder(dir);
      await refreshCache();
      onNotice("Working files will go to the new folder from now on. Waveform drawings are rebuilt as files are shown.");
    } catch (e) {
      onNotice(String(e));
    } finally {
      busy = false;
    }
  }
  async function clearPeaks() {
    busy = true;
    try {
      const freed = await cacheClearPeaks();
      onNotice(`Cleared ${sizeText(freed)} of waveform drawings. They are rebuilt as files are shown.`);
      await refreshCache();
    } finally {
      busy = false;
    }
  }

  const COLOR_ROWS: { key: keyof AppColors; label: string; theme: string }[] = [
    { key: "wave", label: "Waveform (Waveform editor)", theme: "#53c28b" },
    { key: "playhead", label: "Playhead", theme: "#f2c14e" },
    { key: "accent", label: "Accent (focus and selection)", theme: "#3fa9f5" },
    { key: "laneSelected", label: "Selected track lane", theme: "#34343b" },
    { key: "spectrumLow", label: "Spectrogram, quiet", theme: "#2a1a8a" },
    { key: "spectrumHigh", label: "Spectrogram, loud", theme: "#ffe066" },
    { key: "phaseLow", label: "Phase Wheel, low notes", theme: "#ff2a2a" },
    { key: "phaseHigh", label: "Phase Wheel, high notes", theme: "#e02aff" },
  ];
  const CACHE_NOTES: Record<string, string> = {
    peaks: "Waveform drawings",
    spectral: "Spectrum edit results",
    "wave-fx": "Voice Chain apply results",
    untitled: "Untitled Waveform files",
    recordings: "Recordings not yet saved with a session",
  };

  function ok() {
    onApply(cleanDraft($state.snapshot(d) as SettingsDraft));
  }
  function resetPage() {
    const def = DEFAULT_SETTINGS;
    if (page === "General") Object.assign(d.app, { zoomStep: def.zoomStep, wheelAnchor: def.wheelAnchor, wheelZooms: def.wheelZooms, fadeShape: def.fadeShape, dropMany: def.dropMany });
    else if (page === "Appearance") d.app.colors = { ...NO_COLORS };
    else if (page === "Autosave") Object.assign(d.app, { autosave: def.autosave, autosaveMinutes: def.autosaveMinutes, backups: def.backups, backupCount: def.backupCount });
    else if (page === "Session Media") Object.assign(d.app, { copyMedia: def.copyMedia, mediaFolder: def.mediaFolder, collectOnSave: def.collectOnSave, leaveLibraryFiles: def.leaveLibraryFiles });
    else if (page === "Playback and Recording") Object.assign(d.app, { meterYellowDb: def.meterYellowDb, meterRedDb: def.meterRedDb, meterSpeed: def.meterSpeed });
  }
  const resettable = $derived(["General", "Appearance", "Autosave", "Session Media", "Playback and Recording"].includes(page));
</script>

<svelte:window
  onkeydown={(e) => {
    if (e.key === "Escape") onClose();
  }}
/>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={onClose}>
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="st-title" tabindex="-1" onclick={(e) => e.stopPropagation()}>
    <header><h1 id="st-title">Settings</h1></header>
    <div class="body">
      <nav aria-label="Settings pages">
        {#each PAGES as p (p)}
          <button type="button" class:on={page === p} class:later={LATER_PAGES[p] !== undefined} aria-current={page === p ? "page" : undefined} onclick={() => (page = p)}>{p}</button>
        {/each}
      </nav>

      <section class="page" aria-label={page}>
        <h2>{page}</h2>

        {#if LATER_PAGES[page]}
          <p class="later-note">{LATER_PAGES[page]}</p>
        {/if}

        {#if page === "General"}
          <label class="row" title="Off: the wheel scrolls the tracks or the file, and Ctrl+wheel zooms. Over the track headers the wheel always makes the tracks taller or shorter.">
            <input type="checkbox" bind:checked={d.app.wheelZooms} /> The mouse wheel over the waveform zooms in time (Waveform editor; in the Multitrack the wheel zooms over the ruler and scrolls over the tracks)
          </label>
          <label class="row">Each wheel step zooms by <input type="number" min="5" max="100" step="5" bind:value={d.app.zoomStep} /> % a step</label>
          <fieldset>
            <legend>Wheel zoom centers on</legend>
            <label><input type="radio" bind:group={d.app.wheelAnchor} value="mouse" /> The mouse pointer</label>
            <label><input type="radio" bind:group={d.app.wheelAnchor} value="between" /> Between the pointer and the middle of the view (drifts less)</label>
            <label><input type="radio" bind:group={d.app.wheelAnchor} value="playhead" /> The playhead</label>
          </fieldset>
          <fieldset>
            <legend>Several files dropped on the Editor go</legend>
            <label><input type="radio" bind:group={d.app.dropMany} value="sequence" /> One after another on one track</label>
            <label><input type="radio" bind:group={d.app.dropMany} value="tracks" /> Side by side, each on its own track from the same time</label>
          </fieldset>
          <fieldset>
            <legend>New fades start as</legend>
            <label><input type="radio" bind:group={d.app.fadeShape} value="cosine" /> S curve (smooth at both ends)</label>
            <label><input type="radio" bind:group={d.app.fadeShape} value="linear" /> Straight line</label>
            <label><input type="radio" bind:group={d.app.fadeShape} value="logarithmic" /> Logarithmic (fast start)</label>
          </fieldset>
          <p class="note">The Space bar plays, and stops when playing or recording.</p>
          <label class="off" title="Warnings with a 'don't ask again' box arrive with the first such warning."><input type="checkbox" disabled /> Show warnings I turned off again <span class="why">No warnings can be turned off yet.</span></label>
        {:else if page === "Appearance"}
          <p class="note">Leave a color on Theme to keep the built-in one. Changes show the next time each view redraws.</p>
          <div class="colors">
            {#each COLOR_ROWS as r (r.key)}
              <span>{r.label}</span>
              <input type="color" value={d.app.colors[r.key] || r.theme} oninput={(e) => (d.app.colors[r.key] = e.currentTarget.value)} aria-label={r.label} />
              <button type="button" disabled={!d.app.colors[r.key]} onclick={() => (d.app.colors[r.key] = "")}>Theme</button>
            {/each}
          </div>
          <label class="off"><select disabled><option>Dark</option></select> Theme <span class="why">One dark theme for now; a light theme is planned.</span></label>
        {:else if page === "Audio Devices"}
          <label class="row">
            Play through
            <select class="grow" value={d.app.output ?? ""} onchange={(e) => (d.app.output = e.currentTarget.value || null)}>
              <option value="">System default{outputs?.find((o) => o.isDefault) ? ` (${outputs.find((o) => o.isDefault)?.name})` : ""}</option>
              {#each outputs ?? [] as o (o.name)}<option value={o.name}>{o.name}</option>{/each}
              {#if d.app.output && outputs && !outputs.some((o) => o.name === d.app.output)}<option value={d.app.output}>{d.app.output} (not connected)</option>{/if}
            </select>
          </label>
          <p class="note">Used for playback, file previews and input monitoring from the next time they start. If it is unplugged, the system default plays instead.</p>
          <label class="row">
            Waveform editor records from
            <select class="grow" value={d.app.waveInput ?? ""} onchange={(e) => (d.app.waveInput = e.currentTarget.value || null)}>
              <option value="">System default input</option>
              {#each inputs ?? [] as dev (dev.name)}
                <optgroup label={dev.name}>
                  {#each Array.from({ length: dev.channels }, (_, i) => i + 1) as c (c)}<option value={`Mono: ${dev.name} ${c}`}>Mono, channel {c}</option>{/each}
                  {#each Array.from({ length: Math.floor(dev.channels / 2) }, (_, i) => i * 2 + 1) as c (c)}<option value={`Stereo: ${dev.name} ${c}-${c + 1}`}>Stereo, channels {c}-{c + 1}</option>{/each}
                </optgroup>
              {/each}
              {#if d.app.waveInput && inputs && !inputs.some((dev) => d.app.waveInput?.includes(dev.name))}<option value={d.app.waveInput}>{d.app.waveInput} (not connected)</option>{/if}
            </select>
          </label>
          <p class="note">In the multitrack view, each track picks its own input in its track header.</p>
          <label class="row" class:off={!driver?.asioBuilt}>
            Driver
            <select
              class="grow"
              value={d.app.driver}
              disabled={!driver?.asioBuilt}
              onchange={(e) => {
                d.app.driver = e.currentTarget.value === "asio" ? "asio" : "system";
                // Device names differ between drivers, so the output goes back to the default.
                d.app.output = null;
                d.app.waveInput = null;
                outputs = null;
                inputs = null;
              }}
            >
              <option value="system">System ({systemDriverName()})</option>
              <option value="asio">ASIO</option>
            </select>
          </label>
          {#if devicesStale}<p class="note">The driver is busy (for example while it records), so the last known device lists are shown.</p>{/if}
          {#if !driver?.asioBuilt}
            <p class="note">ASIO is in the Windows release builds. This build uses the system driver.</p>
          {:else if d.app.driver === "asio"}
            <p class="note">
              {driver.drivers.length ? `ASIO drivers found: ${driver.drivers.join(", ")}.` : "No ASIO drivers found. Install your interface's ASIO driver."}
              Sample rate and buffer size are set in the driver's own control panel. Takes effect from the next playback or recording. If the ASIO driver can't open, the system driver plays instead.
            </p>
          {/if}
          <p class="note">ASIO is a trademark of Steinberg Media Technologies GmbH.</p>
          <div><button type="button" onclick={() => void openSoundSettings()}>Open System Sound Settings</button></div>
        {:else if page === "Channel Routing"}
          <label class="off"><select disabled><option>1 and 2</option></select> Input channels for stereo tracks</label>
          <label class="off"><select disabled><option>1 and 2</option></select> Output channels</label>
        {:else if page === "Autosave"}
          <label><input type="checkbox" bind:checked={d.app.autosave} /> Save a recovery copy of the session</label>
          <label class="row" class:dim={!d.app.autosave}>Every <input type="number" min="1" max="60" step="1" bind:value={d.app.autosaveMinutes} disabled={!d.app.autosave} /> minutes</label>
          <p class="note">The recovery copy is offered back if the app closes without saving. It never replaces your session file.</p>
          <fieldset>
            <legend>Backups of the session file</legend>
            <label><input type="checkbox" bind:checked={d.app.backups} /> Keep older copies when saving</label>
            <label class="row" class:dim={!d.app.backups}>Keep the newest <input type="number" min="1" max="50" step="1" bind:value={d.app.backupCount} disabled={!d.app.backups} /> copies</label>
            <p class="note">Each save first copies the previous file into a Backups folder beside the session, named with the date and time.</p>
          </fieldset>
        {:else if page === "Session Media"}
          <p class="note">A session can keep every audio file it uses in its own folder, so the folder can be moved, backed up or handed to someone else whole. Your original files are never moved or changed.</p>
          <fieldset>
            <legend>When importing audio into a saved session</legend>
            <label><input type="radio" name="copy-media" value="always" bind:group={d.app.copyMedia} /> Copy it into the session folder</label>
            <label><input type="radio" name="copy-media" value="ask" bind:group={d.app.copyMedia} /> Ask each time</label>
            <label><input type="radio" name="copy-media" value="never" bind:group={d.app.copyMedia} /> Leave it where it is</label>
          </fieldset>
          <label class="row">Copies go into <input type="text" maxlength="60" bind:value={d.app.mediaFolder} /> in the session folder</label>
          <label class:dim={d.app.copyMedia === "never"}><input type="checkbox" bind:checked={d.app.collectOnSave} disabled={d.app.copyMedia === "never"} /> When saving, copy in audio still outside the session folder</label>
          <p class="note">This catches files imported before the session was first saved, and working files such as untitled mixdowns. Recordings already go into the session folder.</p>
          <label><input type="checkbox" bind:checked={d.app.leaveLibraryFiles} /> Leave library files where they are</label>
          <p class="note">Libraries keep their own copies. Untick this to copy library audio in too, so the session folder works without the library. File &gt; Collect Session Media copies everything in at any time.</p>
        {:else if page === "Editing"}
          <label class="off"><input type="checkbox" disabled /> Crossfade the edges of cuts and deletes in the Waveform editor <span class="why">Planned with the next Waveform editor batch.</span></label>
          <label class="off"><input type="number" disabled value="1" /> Files processed at once in Batch Convert <span class="why">One at a time for now.</span></label>
          <label class="off"><select disabled><option>High</option></select> Sample rate conversion quality <span class="why">Always the high-quality converter.</span></label>
        {:else if page === "Effects"}
          <label><input type="checkbox" bind:checked={d.app.openChainOnAdd} /> Show the Voice Chain when adding an effect from the Effects menu</label>
          <label><input type="checkbox" bind:checked={d.app.realtimePlugins} /> Mix down sessions that use VST3 plug-ins in real time</label>
          <p class="note">Some plug-ins only work while audio plays at normal speed: pitch correction, immersive panners and other tools that follow the clock. With this on, any mixdown, stem export or Mix Down to New File of a session with VST3 plug-ins runs at playback speed, so an hour takes an hour. Built-in effects never need it.</p>
          <label class="off"><input type="checkbox" disabled checked /> Bypassed effects are skipped on playback <span class="why">Always on: a bypassed effect costs nothing.</span></label>
          <label class="off"><select disabled><option>By category</option></select> Effects menu order <span class="why">Grouped by category.</span></label>
        {:else if page === "Cache and Files"}
          {#if cache}
            <p class="note">Working files live in <code>{cache.path}</code></p>
            <table>
              <tbody>
                {#each cache.folders as f (f.name)}
                  <tr><td>{CACHE_NOTES[f.name] ?? f.name}</td><td class="num">{sizeText(f.bytes)}</td></tr>
                {/each}
              </tbody>
            </table>
            <div class="row">
              <button type="button" disabled={busy} onclick={() => void clearPeaks()}>Clear Waveform Drawings</button>
              <span class="why">Edit results, untitled files and unsaved recordings stay: an open file or session may use them.</span>
            </div>
          {:else}
            <p class="note">Reading the cache folder…</p>
          {/if}
          <div class="row">
            <button type="button" disabled={busy} onclick={() => void chooseCacheFolder(false)}>Change Folder...</button>
            <button type="button" disabled={busy} onclick={() => void chooseCacheFolder(true)}>Use Default</button>
            <span class="why">The default is Documents\Altered Audio Editor\Cache. If Documents syncs to a cloud service, pick a local folder instead.</span>
          </div>
        {:else if page === "Markers"}
          <label class="off"><input type="checkbox" disabled checked /> Marker changes can be undone <span class="why">Always on.</span></label>
          <label class="off"><input type="checkbox" disabled checked /> Markers move with cuts, pastes and crops <span class="why">Always on.</span></label>
          <label class="off"><input type="checkbox" disabled checked /> Write chapters into exported MP3 and FLAC files <span class="why">Chosen in each export.</span></label>
          <label class="row">Name for new markers <input type="text" maxlength="40" bind:value={d.app.markerName} /> (numbered after it)</label>
        {:else if page === "Multitrack"}
          <fieldset>
            <legend>Solo</legend>
            <label><input type="radio" bind:group={d.app.soloExclusive} value={false} /> Several tracks can be soloed at once</label>
            <label><input type="radio" bind:group={d.app.soloExclusive} value={true} /> Soloing a track unsolos the others</label>
          </fieldset>
          <label class="off"><select disabled><option>Balance (centre at full level)</option></select> Pan law <span class="why">Planned with buses.</span></label>
          <label class="off"><input type="checkbox" disabled checked /> New tracks get a color in turn <span class="why">Always on.</span></label>
        {:else if page === "Clips"}
          <label><input type="checkbox" bind:checked={d.editor.autoCrossfade} /> Crossfade clips that overlap on a track</label>
          <label class="off"><input type="checkbox" disabled /> Files dropped together go on their own tracks <span class="why">Use Insert Files as Tracks for that for now.</span></label>
          <label class="off"><select disabled><option>Keep pitch</option></select> Time Stretch starts as <span class="why">Picked in the Time Stretch dialog each time.</span></label>
        {:else if page === "Playback and Recording"}
          <fieldset>
            <legend>Playback</legend>
            <label><input type="checkbox" bind:checked={d.editor.followPlayhead} /> Scroll to follow the playhead</label>
            <label class="off"><input type="checkbox" disabled /> Play audio while scrubbing <span class="why">Planned.</span></label>
          </fieldset>
          <fieldset>
            <legend>Meters turn yellow and red at</legend>
            <label class="row">Yellow from <input type="number" min="-40" max="-1" step="1" bind:value={d.app.meterYellowDb} /> dBFS</label>
            <label class="row">Red from <input type="number" min="-39" max="0" step="1" bind:value={d.app.meterRedDb} /> dBFS</label>
          </fieldset>
          <label class="row">
            Meter response
            <select bind:value={d.app.meterSpeed}>
              <option value="fast">Fast (every peak)</option>
              <option value="medium">Medium</option>
              <option value="slow">Slow (steady, easier to read)</option>
            </select>
          </label>
          <fieldset>
            <legend>Recording</legend>
            <label class="row">Trim from each take <input type="number" min="0" max="1000" step="1" bind:value={d.record.offsetMs} /> ms (latency offset)</label>
            <label class="row">Play before a punch-in <input type="number" min="0" max="10" step="0.5" bind:value={d.record.prerollSec} /> seconds</label>
            <label><input type="checkbox" bind:checked={d.record.punch} /> With a time selection, record only into the selection</label>
            <label class="row">
              Count down before recording
              <select value={String(d.app.countdownSec)} onchange={(e) => (d.app.countdownSec = Number(e.currentTarget.value))}>
                <option value="0">Off</option>
                <option value="3">3 seconds</option>
                <option value="5">5 seconds</option>
              </select>
            </label>
          </fieldset>
        {:else if page === "Spectrum Views"}
          <label class="off"><select disabled><option>2048</option></select> Spectrum detail <span class="why">Fixed for now, tuned for speech.</span></label>
          <label class="off"><input type="number" disabled value="100" /> Spectrum range (dB) <span class="why">Fixed for now.</span></label>
          <label class="off"><input type="checkbox" disabled /> Gridlines on the spectrum <span class="why">Planned.</span></label>
        {:else if page === "Time Display"}
          <label class="row">
            Show times as
            <select bind:value={d.editor.timeFormat}>
              <option value="time">Minutes and seconds (1:05.250)</option>
              <option value="timecode">Timecode (hh:mm:ss:ff)</option>
              <option value="samples">Samples</option>
            </select>
          </label>
          <label class="row">
            Timecode frame rate
            <select value={String(d.app.timecodeFps)} onchange={(e) => (d.app.timecodeFps = Number(e.currentTarget.value))}>
              {#each TIMECODE_RATES as r (r)}<option value={String(r)}>{r} frames a second</option>{/each}
            </select>
          </label>
          <label class="off"><input type="number" disabled value="120" /> Tempo for bars and beats <span class="why">Arrives with MIDI tracks.</span></label>
        {:else if page === "Loudness"}
          <label class="row">
            Broadcast region
            <select bind:value={d.loudness.region}>
              {#each BROADCAST_REGIONS as r (r.id)}<option value={r.id}>{r.name} ({r.standard})</option>{/each}
            </select>
          </label>
          <label class="row">
            Target in Check Loudness
            <select bind:value={d.loudness.target}>
              {#each targetsFor(d.loudness.region, { lufs: d.app.customLufs, peak: d.app.customPeak }) as t (t.id)}<option value={t.id}>{t.name}</option>{/each}
            </select>
          </label>
          <fieldset>
            <legend>Custom target (pick Custom above, or in Check Loudness and Loudness Match)</legend>
            <label class="row">Loudness <input type="number" min="-40" max="-5" step="0.5" bind:value={d.app.customLufs} /> LUFS</label>
            <label class="row">True peak at or below <input type="number" min="-10" max="0" step="0.1" bind:value={d.app.customPeak} /> dBTP</label>
          </fieldset>
          <p class="note">Podcast targets follow published podcast guidance (stereo -16 LUFS, mono -19 LUFS, true peak at or below -1 dBTP). Broadcast targets follow the region.</p>
        {:else if page === "Privacy"}
          <p class="note">Altered Audio Editor never goes online by itself. No account, no sign-in, no tracking, no usage data, no crash reports, no update checks. Everything stays on this computer.</p>
          <p class="note">The only web pages it opens are the ones you pick in the Help menu, in your own browser.</p>
          <label class="off"><input type="checkbox" disabled /> Check for updates once a week <span class="why">Arrives after the first public release, and stays off unless you turn it on.</span></label>
        {/if}
      </section>
    </div>
    <footer>
      {#if resettable}<button type="button" class="left" onclick={resetPage}>Reset This Page</button>{/if}
      <button type="button" onclick={onClose}>Cancel</button>
      <button type="button" class="primary" onclick={ok}>OK</button>
    </footer>
  </div>
</div>

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
    grid-template-rows: auto minmax(0, 1fr) auto;
    width: min(780px, 96vw);
    height: min(560px, 92vh);
    border: 1px solid var(--border-strong);
    background: var(--surface-2);
    box-shadow: 0 16px 40px rgba(0, 0, 0, 0.6);
    font-size: 12px;
  }
  header {
    padding: 10px 14px 6px;
  }
  h1 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
  }
  .body {
    display: grid;
    grid-template-columns: 170px minmax(0, 1fr);
    min-height: 0;
    gap: 0;
    border-top: 1px solid var(--border);
    border-bottom: 1px solid var(--border);
  }
  nav {
    display: flex;
    flex-direction: column;
    overflow: auto;
    padding: 6px 0;
    background: var(--surface-1);
    border-right: 1px solid var(--border);
  }
  nav button {
    padding: 4px 12px;
    border: 0;
    border-left: 2px solid transparent;
    background: transparent;
    color: var(--text-dim);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  nav button:hover {
    color: var(--text);
  }
  nav button.on {
    border-left-color: var(--accent);
    background: var(--surface-3);
    color: var(--text);
  }
  nav button.later {
    color: var(--text-faint);
  }
  .page {
    display: flex;
    flex-direction: column;
    gap: 8px;
    overflow: auto;
    padding: 10px 16px 14px;
  }
  h2 {
    margin: 0 0 2px;
    padding-bottom: 4px;
    border-bottom: 1px solid var(--border);
    font-size: 13px;
    font-weight: 600;
  }
  fieldset {
    display: grid;
    gap: 4px;
    margin: 0;
    padding: 4px 10px 8px;
    border: 1px solid var(--border);
    border-radius: 3px;
  }
  legend {
    padding: 0 4px;
    color: var(--text-dim);
  }
  label {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    align-items: center;
  }
  .row {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .grow {
    flex: 1;
    min-width: 0;
  }
  .off,
  .dim {
    color: var(--text-faint);
  }
  .why {
    color: var(--text-faint);
    font-style: italic;
  }
  .note {
    margin: 0;
    color: var(--text-dim);
  }
  .later-note {
    margin: 0;
    padding: 6px 8px;
    border-left: 2px solid var(--accent-dim);
    background: var(--surface-1);
    color: var(--text-dim);
  }
  .colors {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 48px auto;
    gap: 6px 10px;
    align-items: center;
    max-width: 420px;
  }
  input[type="color"] {
    width: 44px;
    height: 20px;
    padding: 0;
    border: 1px solid var(--border-strong);
    background: transparent;
  }
  input[type="number"],
  input[type="text"],
  select,
  button {
    height: 22px;
    padding: 0 6px;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    background: var(--surface-1);
    color: var(--text);
    font: inherit;
  }
  input[type="number"] {
    width: 64px;
  }
  button {
    flex-shrink: 0;
    white-space: nowrap;
  }
  button:disabled,
  select:disabled,
  input:disabled {
    opacity: 0.5;
  }
  table {
    border-collapse: collapse;
    max-width: 420px;
  }
  td {
    padding: 2px 8px 2px 0;
    color: var(--text-dim);
  }
  td.num {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  code {
    font-family: var(--font-mono);
    word-break: break-all;
  }
  footer {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
    padding: 10px 14px;
  }
  footer .left {
    margin-right: auto;
  }
  footer button {
    min-width: 72px;
  }
  .primary {
    border-color: var(--accent);
    background: var(--accent);
    color: var(--on-accent, #fff);
  }
</style>
