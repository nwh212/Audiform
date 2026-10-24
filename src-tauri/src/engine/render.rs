// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

//! Renders each track's clips into a stereo block at the output rate.
//! Runs on a worker thread ahead of the audio device, never on it: it
//! opens files, seeks and allocates.
//!
//! Clip Voice Chains run here: each clip's chain on that clip. A chain
//! with look-ahead delays its output, so its input is read that many
//! frames early (and the chain is primed with the frames in between),
//! which keeps every clip in time with the others. Track, bus and
//! master chains run after this, in the mix graph (`graph.rs`).
//!
//! A chain edited during playback swaps in without stopping: the new
//! chain is warmed up on the last stretch of input the old one heard,
//! so it picks up exactly where the old one was. That works while the
//! look-ahead stays the same; a change in look-ahead moves where the
//! input is read from, so the transport restarts instead.

use std::collections::HashMap;

use super::fx::{swap_fx, Fx};
use super::plan::{ClipPlan, ParamLane, Plan, TrackPlan};
use crate::codecs::AudioReader;
use crate::dsp::stretch::Stretcher;
use crate::session::model::{EffectChain, Id};

impl Fx {
    /// Runs the chain over `buf`, whose first frame is `at` frames into
    /// the clip, moving automated settings every 256 frames.
    fn run_at(&mut self, buf: &mut [f32], at: u64, lanes: &[ParamLane]) {
        if lanes.is_empty() {
            self.run(buf);
            return;
        }
        const STEP: usize = 256;
        let mut done = 0;
        for chunk in buf.chunks_mut(STEP * 2) {
            let pos = at + done as u64;
            for l in lanes {
                self.chain
                    .set_param(l.slot, &l.param, f64::from(l.value_at(pos)));
            }
            self.run(chunk);
            done += chunk.len() / 2;
        }
    }
}

/// An open clip being streamed.
struct Active {
    reader: Option<AudioReader>,
    /// Decoded samples not yet used (stereo interleaved).
    pending: Vec<f32>,
    used: usize,
    /// Looped clips: frames left before the region starts over.
    lap_left: u64,
    /// Pitch-keeping stretch, fed from the reader.
    stretcher: Option<Stretcher>,
    scratch: Vec<f32>,
    fx: Option<Fx>,
}

/// Chains edited during playback, by owner.
#[derive(Debug, Clone, Default)]
pub struct ChainUpdate {
    pub tracks: Vec<(Id, EffectChain)>,
    pub clips: Vec<(Id, EffectChain)>,
    pub buses: Vec<(Id, EffectChain)>,
    pub master: Option<EffectChain>,
}

impl Active {
    fn open(clip: &ClipPlan, offset_frames: u64, out_rate: u32) -> Self {
        // A looped clip starts wherever its phase and the offset land in the region.
        let (into, lap_left) = if clip.loop_len > 0 {
            let p = (clip.loop_phase + offset_frames) % clip.loop_len;
            (p, clip.loop_len - p)
        } else {
            (offset_frames, 0)
        };
        let stretched = clip.loop_len == 0 && (clip.stretch - 1.0).abs() > 1e-9;
        // Tape-style stretch reads the file as if its rate were scaled, so it
        // plays slower and lower (or faster and higher). Pitch-keeping stretch
        // reads normally and reshapes time afterwards.
        let read_rate = if stretched && !clip.keep_pitch {
            (f64::from(out_rate) * clip.stretch).round().max(1_000.0) as u32
        } else {
            out_rate
        };
        // Timeline frames into the clip, as seconds of source.
        let source_secs = if stretched {
            into as f64 / f64::from(out_rate) / clip.stretch
        } else {
            into as f64 / f64::from(out_rate)
        };
        let reader = AudioReader::open(&clip.path, 2, Some(read_rate))
            .ok()
            .and_then(|mut r| {
                let secs = clip.source_start + source_secs;
                if secs > 0.0 {
                    r.seek(secs).ok()?;
                }
                Some(r)
            });
        Self {
            reader,
            pending: Vec::new(),
            used: 0,
            lap_left,
            stretcher: (stretched && clip.keep_pitch)
                .then(|| Stretcher::new(clip.stretch, out_rate)),
            scratch: Vec::new(),
            fx: Fx::new(&clip.effects, out_rate),
        }
    }

    /// Adds `frames` frames of the clip from `rel` into `out`, through
    /// the clip's Voice Chain when it has one.
    fn play_into(&mut self, out: &mut [f32], frames: usize, clip: &ClipPlan, rel: u64) {
        let Some(mut fx) = self.fx.take() else {
            self.mix_into(out, frames, clip, rel);
            return;
        };
        let lat = fx.latency;
        if !fx.primed {
            // Feed the frames the look-ahead skips, and drop the output.
            fx.buf.clear();
            fx.buf.resize(lat as usize * 2, 0.0);
            let n = lat.min(clip.len.saturating_sub(rel)) as usize;
            if n > 0 {
                self.mix_into(&mut fx.buf, n, clip, rel);
            }
            let mut b = std::mem::take(&mut fx.buf);
            fx.run_at(&mut b, rel, &clip.automation);
            fx.buf = b;
            fx.primed = true;
        }
        let mut b = std::mem::take(&mut fx.buf);
        b.clear();
        b.resize(frames * 2, 0.0);
        let from = rel + lat;
        let n = (frames as u64).min(clip.len.saturating_sub(from)) as usize;
        if n > 0 {
            self.mix_into(&mut b, n, clip, from);
        }
        fx.run_at(&mut b, from, &clip.automation);
        for (o, s) in out.iter_mut().zip(&b) {
            *o += *s;
        }
        fx.buf = b;
        self.fx = Some(fx);
    }

    /// Looped clips: back to the start of the region.
    fn rewind(&mut self, clip: &ClipPlan) {
        self.pending.clear();
        self.used = 0;
        self.lap_left = clip.loop_len;
        if let Some(r) = self.reader.as_mut() {
            if r.seek(clip.source_start).is_err() {
                self.reader = None;
            }
        }
    }

    /// Fills `pending` with the next decoded (and, for a stretched clip,
    /// reshaped) block. False at the end of the file.
    fn refill(&mut self) -> bool {
        let Some(st) = self.stretcher.as_mut() else {
            let more = match self.reader.as_mut() {
                Some(r) => r.read_into(&mut self.pending).unwrap_or(false),
                None => false,
            };
            return more && !self.pending.is_empty();
        };
        loop {
            st.pull(&mut self.pending);
            if !self.pending.is_empty() {
                return true;
            }
            self.scratch.clear();
            let more = match self.reader.as_mut() {
                Some(r) => r.read_into(&mut self.scratch).unwrap_or(false),
                None => false,
            };
            if more && !self.scratch.is_empty() {
                st.push(&self.scratch);
            } else {
                self.reader = None;
                st.finish();
                st.pull(&mut self.pending);
                return !self.pending.is_empty();
            }
        }
    }

    /// Adds `frames` frames into `out` (stereo), shaped by the clip's
    /// gain and fades. `rel` is the clip-relative frame of the first one.
    /// Silence if the file ends early or could not be opened.
    fn mix_into(&mut self, out: &mut [f32], frames: usize, clip: &ClipPlan, rel: u64) {
        let mut done = 0;
        while done < frames {
            if self.used >= self.pending.len() {
                self.pending.clear();
                self.used = 0;
                if !self.refill() {
                    // File ended or failed: the rest stays silent.
                    self.reader = None;
                    return;
                }
            }
            let avail = (self.pending.len() - self.used) / 2;
            let mut n = avail.min(frames - done);
            if clip.loop_len > 0 {
                n = n.min(self.lap_left as usize);
            }
            let src = &self.pending[self.used..self.used + n * 2];
            let dst = &mut out[done * 2..(done + n) * 2];
            if clip.is_flat() {
                let g = clip.gain;
                if g != 0.0 {
                    for (o, s) in dst.iter_mut().zip(src) {
                        *o += *s * g;
                    }
                }
            } else {
                let at = rel + done as u64;
                for (i, (o, s)) in dst
                    .as_chunks_mut::<2>()
                    .0
                    .iter_mut()
                    .zip(src.as_chunks::<2>().0)
                    .enumerate()
                {
                    let [l, r] = clip.gains_at(at + i as u64);
                    o[0] += s[0] * l;
                    o[1] += s[1] * r;
                }
            }
            self.used += n * 2;
            done += n;
            if clip.loop_len > 0 {
                self.lap_left -= n as u64;
                if self.lap_left == 0 {
                    self.rewind(clip);
                }
            }
        }
    }
}

pub struct Renderer {
    plan: Plan,
    /// Next output frame to render.
    pos: u64,
    active: Vec<HashMap<Id, Active>>,
    /// On reaching frame `.0`, continue from frame `.1`: back to the
    /// start for Loop Playback, past the selection for Skip Selection.
    jump: Option<(u64, u64)>,
    /// Frames rendered since the start, jumps included.
    rendered: u64,
}

impl Renderer {
    pub fn new(plan: Plan, start_frame: u64) -> Self {
        let n = plan.tracks.len();
        Self {
            plan,
            pos: start_frame,
            active: (0..n).map(|_| HashMap::new()).collect(),
            jump: None,
            rendered: 0,
        }
    }

    /// Frames rendered since the start, counting across jumps, so it
    /// lines up with the frames the device has played.
    pub fn rendered(&self) -> u64 {
        self.rendered
    }

    /// Swaps in clip chains edited during playback. False if any chain's
    /// look-ahead changed; those keep the old chain, and the caller
    /// restarts playback to pick them up.
    pub fn set_chains(&mut self, update: &ChainUpdate) -> bool {
        let rate = self.plan.out_rate;
        let mut ok = true;
        for (id, chain) in &update.clips {
            for (t, track) in self.plan.tracks.iter_mut().enumerate() {
                let Some(clip) = track.clips.iter_mut().find(|c| c.id == *id) else {
                    continue;
                };
                let swapped = match self.active[t].get_mut(id) {
                    Some(a) => swap_fx(&mut a.fx, chain, rate),
                    // Not playing yet: it opens with the new chain.
                    None => true,
                };
                if swapped {
                    clip.effects = chain.clone();
                } else {
                    ok = false;
                }
            }
        }
        ok
    }

    /// Jumps from output frame `at` to `to` every time playback reaches
    /// `at`. Only applies while playback is before `at`.
    pub fn with_jump(mut self, at: u64, to: u64) -> Self {
        if at > self.pos && at != to {
            self.jump = Some((at, to));
        }
        self
    }

    pub fn track_count(&self) -> usize {
        self.plan.tracks.len()
    }

    pub fn position(&self) -> u64 {
        self.pos
    }

    /// Renders the next `frames` frames. `out[t]` receives track `t`
    /// (its clips, before the track's chain and fader) as stereo
    /// interleaved samples and is resized to the frames rendered.
    /// Stops short at a jump point, so it may render fewer frames.
    pub fn render(&mut self, frames: usize, out: &mut [Vec<f32>]) {
        let frames = match self.jump {
            Some((at, _)) if self.pos < at => frames.min((at - self.pos) as usize),
            _ => frames,
        };
        let start = self.pos;
        let end = start + frames as u64;
        let rate = self.plan.out_rate;
        for (t, track) in self.plan.tracks.iter().enumerate() {
            let buf = &mut out[t];
            buf.clear();
            buf.resize(frames * 2, 0.0);
            render_track(track, &mut self.active[t], rate, start, end, buf);
        }
        self.pos = end;
        self.rendered += frames as u64;
        if let Some((at, to)) = self.jump {
            if end == at {
                self.pos = to;
                // Clips reopen at the new spot, and their chains start clean.
                for a in &mut self.active {
                    a.clear();
                }
                if to > at {
                    // Skips happen once.
                    self.jump = None;
                }
            }
        }
    }
}

/// Mixes a track's clips for output frames `start..end` into `buf`
/// (stereo, already sized and zeroed).
fn render_track(
    track: &TrackPlan,
    active: &mut HashMap<Id, Active>,
    rate: u32,
    start: u64,
    end: u64,
    buf: &mut [f32],
) {
    // Forget clips that are behind us.
    active.retain(|id, _| {
        track
            .clips
            .iter()
            .any(|c| c.id == *id && c.start + c.len > start)
    });
    for clip in &track.clips {
        let c_end = clip.start + clip.len;
        if c_end <= start || clip.start >= end {
            continue;
        }
        let from = clip.start.max(start);
        let to = c_end.min(end);
        let a = active
            .entry(clip.id)
            .or_insert_with(|| Active::open(clip, from - clip.start, rate));
        let offset = (from - start) as usize;
        let n = (to - from) as usize;
        a.play_into(&mut buf[offset * 2..], n, clip, from - clip.start);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::plan::TrackPlan;
    use crate::session::model::new_id;
    use std::path::PathBuf;

    fn click_file(dir: &std::path::Path) -> PathBuf {
        let path = dir.join("click.wav");
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 48_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(&path, spec).unwrap();
        for i in 0..48_000 {
            w.write_sample(if i == 1_000 { i16::MAX } else { 0 })
                .unwrap();
        }
        w.finalize().unwrap();
        path
    }

    fn first_click(buf: &[f32]) -> Option<usize> {
        buf.as_chunks::<2>().0.iter().position(|f| f[0] > 0.5)
    }

    #[test]
    fn plays_clips_at_their_positions_across_blocks() {
        let dir = tempfile::tempdir().unwrap();
        let path = click_file(dir.path());
        // Clip starts at frame 5,000 and plays from source frame 500, so
        // the click (source 1,000) lands at timeline frame 5,500.
        let plan = Plan {
            out_rate: 48_000,
            session_rate: 48_000,
            tracks: vec![TrackPlan {
                id: new_id(),
                clips: vec![ClipPlan::plain(
                    new_id(),
                    path,
                    5_000,
                    10_000,
                    500.0 / 48_000.0,
                )],
            }],
        };
        let mut r = Renderer::new(plan.clone(), 0);
        let mut out = vec![Vec::new()];
        let mut all = Vec::new();
        for _ in 0..12 {
            r.render(1_024, &mut out);
            all.extend_from_slice(&out[0]);
        }
        assert_eq!(first_click(&all), Some(5_500));

        // Starting playback in the middle of the clip seeks correctly.
        let mut r = Renderer::new(plan, 5_200);
        r.render(1_024, &mut out);
        assert_eq!(first_click(&out[0]), Some(300));
    }

    fn chain(effect: &str, params: &[(&str, f64)]) -> crate::session::model::EffectChain {
        let mut c = crate::session::model::EffectChain::default();
        c.slots.push(crate::session::model::EffectSlot {
            id: new_id(),
            effect: effect.into(),
            enabled: true,
            params: params.iter().map(|(k, v)| ((*k).to_string(), *v)).collect(),
            preset: None,
            sidechain: None,
            plugin_state: None,
        });
        c
    }
    fn clicks_in(r: &mut Renderer, total: usize) -> Vec<usize> {
        let mut out = vec![Vec::new()];
        let mut all = Vec::new();
        while all.len() < total * 2 {
            r.render(1_000, &mut out);
            all.extend_from_slice(&out[0]);
        }
        all.as_chunks::<2>()
            .0
            .iter()
            .enumerate()
            .filter(|(_, f)| f[0] > 0.5)
            .map(|(i, _)| i)
            .collect()
    }

    fn click_plan(path: PathBuf) -> Plan {
        // Click at timeline frame 5,500 (clip at 5,000, source from 500).
        Plan {
            out_rate: 48_000,
            session_rate: 48_000,
            tracks: vec![TrackPlan {
                id: new_id(),
                clips: vec![ClipPlan::plain(
                    new_id(),
                    path,
                    5_000,
                    10_000,
                    500.0 / 48_000.0,
                )],
            }],
        }
    }

    #[test]
    fn loop_jump_repeats_the_range() {
        let dir = tempfile::tempdir().unwrap();
        let plan = click_plan(click_file(dir.path()));
        // Loop 5,000..6,000 from 4,000: click at 1,500 rendered frames, then every 1,000.
        let mut r = Renderer::new(plan, 4_000).with_jump(6_000, 5_000);
        assert_eq!(clicks_in(&mut r, 4_600), vec![1_500, 2_500, 3_500, 4_500]);
    }

    #[test]
    fn skip_jump_plays_around_the_range_once() {
        let dir = tempfile::tempdir().unwrap();
        let plan = click_plan(click_file(dir.path()));
        // Skipping 5,400..5,600 from 5,000 leaves the click out.
        let mut r = Renderer::new(plan.clone(), 5_000).with_jump(5_400, 5_600);
        assert!(clicks_in(&mut r, 3_000).is_empty());
        // Skipping 5,000..5,200 keeps it, 200 frames sooner.
        let mut r = Renderer::new(plan, 4_000).with_jump(5_000, 5_200);
        assert_eq!(clicks_in(&mut r, 3_000), vec![1_300]);
    }

    #[test]
    fn a_looped_clip_repeats_its_region() {
        let dir = tempfile::tempdir().unwrap();
        let path = click_file(dir.path());
        // Region is source 500..2_500 (2,000 frames, click at 500 into it).
        // The clip starts 300 frames into the region and runs 7,000 frames.
        let mut clip = ClipPlan::plain(new_id(), path, 0, 7_000, 500.0 / 48_000.0);
        clip.loop_len = 2_000;
        clip.loop_phase = 300;
        let plan = Plan {
            out_rate: 48_000,
            session_rate: 48_000,
            tracks: vec![TrackPlan {
                id: new_id(),
                clips: vec![clip],
            }],
        };
        let clicks = |buf: &[f32]| -> Vec<usize> {
            buf.as_chunks::<2>()
                .0
                .iter()
                .enumerate()
                .filter(|(_, f)| f[0] > 0.5)
                .map(|(i, _)| i)
                .collect()
        };
        let mut r = Renderer::new(plan.clone(), 0);
        let mut out = vec![Vec::new()];
        let mut all = Vec::new();
        for _ in 0..7 {
            r.render(1_000, &mut out);
            all.extend_from_slice(&out[0]);
        }
        assert_eq!(clicks(&all), vec![200, 2_200, 4_200, 6_200], "once per lap");

        // Starting mid-clip lands on the right lap position.
        let mut r = Renderer::new(plan, 3_000);
        r.render(2_000, &mut out);
        assert_eq!(clicks(&out[0]), vec![1_200]);
    }

    #[test]
    fn missing_files_are_silent() {
        let plan = Plan {
            out_rate: 48_000,
            session_rate: 48_000,
            tracks: vec![TrackPlan {
                id: new_id(),
                clips: vec![ClipPlan::plain(
                    new_id(),
                    "definitely/missing.wav".into(),
                    0,
                    4_800,
                    0.0,
                )],
            }],
        };
        let mut r = Renderer::new(plan, 0);
        let mut out = vec![Vec::new()];
        r.render(512, &mut out);
        assert!(out[0].iter().all(|v| *v == 0.0));
        assert_eq!(r.position(), 512);
    }

    #[test]
    fn applies_clip_gain_and_fades() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dc.wav");
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 48_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(&path, spec).unwrap();
        for _ in 0..10_000 {
            w.write_sample(16_384i16).unwrap();
        }
        w.finalize().unwrap();
        let mut clip = ClipPlan::plain(new_id(), path, 0, 10_000, 0.0);
        clip.gain = 0.5;
        clip.fade_in = 1_000;
        clip.fade_out = 2_000;
        let plan = Plan {
            out_rate: 48_000,
            session_rate: 48_000,
            tracks: vec![TrackPlan {
                id: new_id(),
                clips: vec![clip],
            }],
        };
        let mut r = Renderer::new(plan, 0);
        let mut out = vec![Vec::new()];
        r.render(10_000, &mut out);
        let at = |f: usize| out[0][f * 2];
        assert!(at(0).abs() < 1e-6, "fade in starts silent");
        assert!((at(500) - 0.125).abs() < 0.01, "halfway up the fade in");
        assert!(
            (at(5_000) - 0.25).abs() < 0.01,
            "full level times clip gain"
        );
        assert!(
            (at(9_000) - 0.125).abs() < 0.01,
            "halfway down the fade out"
        );
    }

    fn tone_file(dir: &std::path::Path) -> PathBuf {
        let path = dir.join("tone.wav");
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 48_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(&path, spec).unwrap();
        for i in 0..48_000 {
            let v = (i as f32 * 220.0 * std::f32::consts::TAU / 48_000.0).sin() * 0.5;
            w.write_sample((v * 32_767.0) as i16).unwrap();
        }
        w.finalize().unwrap();
        path
    }
    /// A clip's Gain effect automated from 0 dB down to -24 dB over the
    /// clip turns the tone down smoothly, without restarting anything.
    #[test]
    fn automation_moves_an_effect_setting_over_time() {
        let dir = tempfile::tempdir().unwrap();
        let path = tone_file(dir.path());
        let gain = chain("altaudio.gain", &[("gain", 0.0)]);
        let slot = gain.slots[0].id;
        let mut clip = ClipPlan::plain(new_id(), path, 0, 48_000, 0.0);
        clip.effects = gain;
        clip.automation = vec![ParamLane {
            slot,
            param: "gain".into(),
            points: vec![(0, 0.0), (48_000, -24.0)],
        }];
        let plan = Plan {
            out_rate: 48_000,
            session_rate: 48_000,
            tracks: vec![TrackPlan {
                id: new_id(),
                clips: vec![clip],
            }],
        };
        let all = render_all(plan, 48_000);
        let peak = |from: usize| {
            all[from * 2..(from + 2_400) * 2]
                .iter()
                .fold(0f32, |m, s| m.max(s.abs()))
        };
        assert!((peak(0) - 0.5).abs() < 0.03, "start {}", peak(0));
        // Halfway: -12 dB, about a quarter of the level.
        let mid = peak(22_800);
        assert!((mid - 0.5 * 0.251).abs() < 0.02, "middle {mid}");
        assert!(peak(45_000) < 0.05, "end {}", peak(45_000));
    }

    fn render_all(plan: Plan, frames: usize) -> Vec<f32> {
        let mut r = Renderer::new(plan, 0);
        let mut out = vec![Vec::new()];
        let mut all = Vec::new();
        while all.len() < frames * 2 {
            r.render(1_024, &mut out);
            all.extend_from_slice(&out[0]);
        }
        all.truncate(frames * 2);
        all
    }

    fn stretched(path: PathBuf, len: u64, ratio: f64, keep_pitch: bool) -> Plan {
        let mut c = ClipPlan::plain(new_id(), path, 0, len, 0.0);
        c.stretch = ratio;
        c.keep_pitch = keep_pitch;
        Plan {
            out_rate: 48_000,
            session_rate: 48_000,
            tracks: vec![TrackPlan {
                id: new_id(),
                clips: vec![c],
            }],
        }
    }

    #[test]
    fn tape_stretch_moves_events_by_the_ratio() {
        let dir = tempfile::tempdir().unwrap();
        let path = click_file(dir.path());
        // Twice as long: the click at source frame 1,000 plays at 2,000.
        let all = render_all(stretched(path, 96_000, 2.0, false), 4_000);
        let at = first_click(&all).expect("click plays");
        assert!((1_990..=2_010).contains(&at), "click at {at}");
    }

    #[test]
    fn pitch_keeping_stretch_fills_the_longer_clip() {
        let dir = tempfile::tempdir().unwrap();
        let path = tone_file(dir.path());
        // One second of tone stretched to 1.25 seconds.
        let all = render_all(stretched(path, 60_000, 1.25, true), 60_000);
        let left: Vec<f32> = all.iter().step_by(2).copied().collect();
        let rms = |s: &[f32]| (s.iter().map(|v| v * v).sum::<f32>() / s.len() as f32).sqrt();
        // Sound right up to near the new end, at the tone's level.
        assert!(
            (rms(&left[54_000..58_000]) - 0.3536).abs() < 0.06,
            "end rms {}",
            rms(&left[54_000..58_000])
        );
        // Pitch unchanged: about 220 rising zero crossings per second.
        let body = &left[6_000..54_000];
        let ups = body
            .windows(2)
            .filter(|w| w[0] < 0.0 && w[1] >= 0.0)
            .count() as f32;
        let hz = ups * 48_000.0 / body.len() as f32;
        assert!((hz - 220.0).abs() < 5.0, "pitch {hz}");
    }
}
