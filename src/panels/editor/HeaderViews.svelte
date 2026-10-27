<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  Copyright (c) The Audiform contributors.
  This file is part of Audiform, based on Altered Audio Editor.

  The corner above the track headers: how many clips are selected, what
  the headers show, and the timeline toggles (metronome, follow playhead,
  stretch handles, snapping) with Add Cue Marker.
-->
<script lang="ts">
  import { editor, type EditorState } from "../../stores/editor";
  import { setMetronomePrefs } from "../../stores/metronome";

  interface Props {
    ed: EditorState;
    /** What the track headers show. */
    headerView?: "mix" | "fx";
    /** The metronome is on. */
    metOn: boolean;
    hasDoc: boolean;
    run: (id: string) => void;
    /** A button's tooltip with its shortcut. */
    tip: (label: string, id: string) => string;
  }

  let { ed, headerView = $bindable("mix"), metOn, hasDoc, run, tip }: Props = $props();
</script>

<!-- What the track headers show. Only the mix controls exist so far; the
     other views arrive with recording (P4) and effects (P5). -->
{#if ed.selectedClips.size > 1}<span class="selcount" aria-live="polite">{ed.selectedClips.size} clips selected</span>{/if}
<div class="views" role="radiogroup" aria-label="Track header view">
  <button type="button" class="view" class:on={headerView === "mix"} role="radio" aria-checked={headerView === "mix"} title="Mix: volume, pan, mute, solo and arm, then input, output and sends as the track gets taller" onclick={() => (headerView = "mix")}>
    <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4 2v12M8 2v12M12 2v12" stroke="currentColor" stroke-width="1" opacity="0.6" /><rect x="2.5" y="8" width="3" height="2" fill="currentColor" /><rect x="6.5" y="4.5" width="3" height="2" fill="currentColor" /><rect x="10.5" y="10" width="3" height="2" fill="currentColor" /></svg>
  </button>
  <button type="button" class="view" role="radio" aria-checked="false" disabled title="Inputs and Outputs: arrives with recording (P4)">
    <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M1.5 8h9M7.5 5l3 3-3 3" fill="none" stroke="currentColor" stroke-width="1.4" /><path d="M13 3v10" stroke="currentColor" stroke-width="1.4" /></svg>
  </button>
  <button type="button" class="view" class:on={headerView === "fx"} role="radio" aria-checked={headerView === "fx"} title="Effects: each track's Audio FX chain in its header. Drag an effect to move it, or onto another track to copy it." onclick={() => (headerView = "fx")}>
    <svg viewBox="0 0 16 16" aria-hidden="true"><text x="1.5" y="12" font-size="10" font-style="italic" font-family="serif" fill="currentColor">fx</text></svg>
  </button>
  <button type="button" class="view" role="radio" aria-checked="false" disabled title="EQ: arrives with the Audio FX chain (P5)">
    <svg viewBox="0 0 16 16" aria-hidden="true"><rect x="2" y="7" width="2.5" height="7" fill="currentColor" /><rect x="6.75" y="3" width="2.5" height="11" fill="currentColor" /><rect x="11.5" y="9" width="2.5" height="5" fill="currentColor" /></svg>
  </button>
</div>
<div class="toggles" role="group" aria-label="Timeline options">
  <button
    type="button"
    class="view"
    class:on={metOn}
    title="Metronome: a click on every beat while playing and recording, on its own track at the top (never in an export)"
    aria-label="Metronome"
    aria-pressed={metOn}
    onclick={() => setMetronomePrefs({ on: !metOn })}
  >
    <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M6 2h4l3 12H3z" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round" /><path d="M8 11l3.5-6" stroke="currentColor" stroke-width="1.3" /></svg>
  </button>
  <button
    type="button"
    class="view"
    class:on={ed.followPlayhead}
    title="Follow Playhead: keep the playhead in view during playback"
    aria-label="Follow playhead"
    aria-pressed={ed.followPlayhead}
    onclick={() => editor.setFollowPlayhead(!ed.followPlayhead)}
  >
    <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M8 2v12" stroke="currentColor" stroke-width="1.3" /><path d="M1.5 8h4M3.5 6l2 2-2 2M14.5 8h-4M12.5 6l-2 2 2 2" fill="none" stroke="currentColor" stroke-width="1.2" /></svg>
  </button>
  <button
    type="button"
    class="view"
    class:on={ed.stretchHandles}
    title="Stretch Handles: each clip shows a triangle in its top corners; drag one to stretch the clip (no Alt needed)"
    aria-label="Stretch handles"
    aria-pressed={ed.stretchHandles}
    onclick={() => editor.setStretchHandles(!ed.stretchHandles)}
  >
    <svg viewBox="0 0 16 16" aria-hidden="true"><circle cx="8" cy="9" r="5" fill="none" stroke="currentColor" stroke-width="1.3" /><path d="M8 9V6.2M8 9l2 1.4M6.5 2h3" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" /></svg>
  </button>
  <button
    type="button"
    class="view"
    class:on={ed.snap.on}
    title="Snapping: clips and selections snap to clips, the playhead and markers"
    aria-label="Snapping"
    aria-pressed={ed.snap.on}
    onclick={() => editor.setSnap({ on: !ed.snap.on })}
  >
    <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4 2.5v6a4 4 0 0 0 8 0v-6" fill="none" stroke="currentColor" stroke-width="2.4" /><path d="M2.8 3.5h2.4M10.8 3.5h2.4" stroke="var(--surface-2)" stroke-width="1.2" /></svg>
  </button>
  <button type="button" class="view" title={tip("Add Cue Marker", "edit.marker.cue")} aria-label="Add cue marker" disabled={!hasDoc} onclick={() => run("edit.marker.cue")}>
    <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M5 2h6v6l-3 3-3-3z" fill="currentColor" /><path d="M8 11v3.5" stroke="currentColor" stroke-width="1.3" /></svg>
  </button>
</div>

<style>
  .views {
    display: flex;
    gap: 1px;
  }

  .view {
    display: grid;
    place-items: center;
    width: 22px;
    height: 18px;
    padding: 0;
    border: 1px solid transparent;
    border-radius: 2px;
    background: none;
    color: var(--text-dim);
  }

  .view svg {
    width: 13px;
    height: 13px;
  }

  .view.on {
    border-color: var(--accent);
    color: var(--text);
  }

  .view:disabled {
    opacity: 0.35;
  }

  .toggles {
    display: flex;
    gap: 1px;
    margin-left: auto;
  }

  .selcount {
    font-size: 11px;
    color: var(--text-dim, #a0a0a8);
    white-space: nowrap;
    padding: 0 6px;
  }
</style>
