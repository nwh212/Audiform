// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Split Channels to Tracks: a clip of a file with two or more channels
 * becomes one track per channel, each playing only that channel the way
 * the original clip played it (its Source Channels, or the default
 * fold-down). With every new track on, the mix sounds the same as the
 * original did. The LFE track starts muted, because the default fold-down
 * leaves it out. The original clips are muted, not removed.
 */

import type { ClipChange, ClipSpec, Op, SessionDoc, TrackChanges } from "../../stores/session";
import type { Located } from "../editor/clip-edit";
import { channelLabels, mixOf } from "./channel-mix";

/** Clips among `found` whose file has two or more channels. */
export function splittable(doc: SessionDoc, found: readonly Located[]): Located[] {
  const channels = new Map(doc.media.map((m) => [m.id, m.channels]));
  return found.filter((f) => (channels.get(f.clip.media_id) ?? 0) >= 2);
}

/**
 * The edit that splits `found` clips onto new tracks, or null when none
 * of them has two or more channels. One new track per channel of the
 * widest file on each source track, placed right under it.
 */
export function splitChannelsOp(doc: SessionDoc, found: readonly Located[], uuid: () => string): Op | null {
  const media = new Map(doc.media.map((m) => [m.id, m]));
  const usable = splittable(doc, found);
  if (usable.length === 0) return null;

  // Group by source track, in track order.
  const byTrack = new Map<number, Located[]>();
  for (const f of usable) byTrack.set(f.trackIndex, [...(byTrack.get(f.trackIndex) ?? []), f]);
  const trackOrder = [...byTrack.keys()].sort((a, b) => a - b);

  const ops: Op[] = [];
  let added = 0;
  for (const ti of trackOrder) {
    const clips = byTrack.get(ti)!;
    const source = clips[0]!.track;
    const widest = Math.max(...clips.map((f) => media.get(f.clip.media_id)?.channels ?? 0));
    const labels = channelLabels(widest);
    const trackIds = labels.map(() => uuid());

    labels.forEach((label, k) => {
      ops.push({ op: "add_track", id: trackIds[k]!, name: `${source.name} ${label}`, stereo: true, index: ti + added + 1 + k });
    });
    added += widest;

    const specs: ClipSpec[][] = labels.map(() => []);
    const changes: ClipChange[] = [];
    for (const { clip } of clips) {
      const n = media.get(clip.media_id)?.channels ?? 0;
      const mix = mixOf(clip.channel_mix, n);
      for (let k = 0; k < n; k++) {
        const id = uuid();
        specs[k]!.push({
          id,
          name: `${clip.name} ${channelLabels(n)[k]}`,
          media_id: clip.media_id,
          start: clip.start,
          source_in: clip.source_in,
          source_out: clip.source_out,
        });
        // Only this channel's amounts; every other channel is silent.
        const only = { sends: mix.sends.map((s, j) => (j === k ? { left: s.left, right: s.right } : { left: 0, right: 0 })) };
        const change: ClipChange = { id, gain_db: clip.gain_db, fade_in: clip.fade_in, fade_out: clip.fade_out, channel_mix: only };
        if (clip.muted) change.muted = true;
        if (clip.audio_type) change.audio_type = clip.audio_type;
        changes.push(change);
      }
    }
    specs.forEach((list, k) => {
      if (list.length > 0) ops.push({ op: "add_clips", track_id: trackIds[k]!, clips: list });
    });
    if (changes.length > 0) ops.push({ op: "set_clips", changes });

    labels.forEach((label, k) => {
      const t: TrackChanges = {};
      if (source.color) t.color = source.color;
      if (label === "LFE") t.muted = true;
      if (Object.keys(t).length > 0) ops.push({ op: "set_track", id: trackIds[k]!, changes: t });
    });

    // The originals stay, muted, so nothing is lost and nothing plays twice.
    ops.push({ op: "set_clips", changes: clips.map((f) => ({ id: f.clip.id, muted: true })) });
  }
  return ops.length === 1 ? ops[0]! : { op: "batch", ops };
}
