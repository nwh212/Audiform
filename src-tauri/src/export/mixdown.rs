// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Session mixdown to WAV, FLAC or MP3.
//!
//! Uses the same plan, renderer and mix graph as playback, at the
//! session sample rate, so a mixdown sounds exactly like what plays:
//! clip gain, fades, every Voice Chain, track and bus volume, pan, mute,
//! solo, outputs, sends, sidechains and master volume all apply. Runs on a
//! worker thread, reports progress and can be cancelled.

use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::encode::{EncodeError, OutputFormat, Writer};
use crate::engine::graph::{Graph, Mix};
use crate::engine::plan;
use crate::engine::render::Renderer;
use crate::session::model::{Id, Session};

/// Frames rendered per step.
const BLOCK: usize = 8_192;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct MixdownRequest {
    /// Start and end in session samples. None mixes the whole session.
    #[serde(default)]
    pub range: Option<(u64, u64)>,
    /// Only these clips. None or empty uses every clip.
    #[serde(default)]
    pub clips: Option<Vec<Id>>,
    /// 16, 24 or 32 (float). None uses the session bit depth.
    #[serde(default)]
    pub bit_depth: Option<u16>,
    /// A stem: only this track, played even if muted or not soloed.
    /// Without a range, the stem runs the length of the whole session
    /// so every stem lines up.
    #[serde(default)]
    pub track: Option<Id>,
    /// Bakes only what belongs to the clips (gain, fades, envelopes):
    /// track volume, pan, mute, routing, buses and the Master are left
    /// out. Used by Merge Clips, where the track still applies its own
    /// settings.
    #[serde(default)]
    pub clip_only: bool,
    /// Leaves every Voice Chain out, for measuring the raw audio
    /// (Speaker Balance).
    #[serde(default)]
    pub dry: bool,
    /// 1 writes a mono file (left and right averaged). None or 2 writes stereo.
    #[serde(default)]
    pub channels: Option<u16>,
    /// File format, rate and quality. None writes a WAV at the session
    /// rate, using `bit_depth` and `channels` above.
    #[serde(default)]
    pub format: Option<OutputFormat>,
    /// Renders at playback speed rather than as fast as possible, for
    /// plug-ins that only work in real time (pitch correction, immersive
    /// panners and other tools that follow the clock).
    #[serde(default)]
    pub realtime: bool,
    /// 3D Space sessions: the speaker layout to write (None uses the
    /// session's). 2.0 writes stereo.
    #[serde(default)]
    pub space_layout: Option<crate::session::model::SpaceLayout>,
    /// 3D Space sessions: a stereo file for headphones, the session's
    /// layout heard around a head.
    #[serde(default)]
    pub binaural: bool,
}

/// What a 3D Space export writes: every speaker of `layout`, or stereo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SpaceOut {
    Bed(crate::session::model::SpaceLayout),
    Stereo,
}

/// How a request renders a 3D Space session, and the session to render
/// (with the layout and listening the export asked for). None for a
/// session that is not one, or a request that is not a whole mix.
fn space_out(
    session: &Session,
    req: &MixdownRequest,
) -> Option<(SpaceOut, crate::session::model::Space)> {
    use crate::session::model::{SpaceLayout, SpaceListen};
    let sp = session.space?;
    if req.track.is_some() || req.clip_only || req.dry || req.channels == Some(1) {
        return None;
    }
    if req.binaural {
        return Some((
            SpaceOut::Stereo,
            crate::session::model::Space {
                listen: SpaceListen::Binaural,
                ..sp
            },
        ));
    }
    let layout = req.space_layout.unwrap_or(sp.layout);
    let space = crate::session::model::Space {
        layout,
        listen: SpaceListen::Stereo,
    };
    Some((
        if layout == SpaceLayout::Stereo20 {
            SpaceOut::Stereo
        } else {
            SpaceOut::Bed(layout)
        },
        space,
    ))
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MixdownResult {
    pub frames: u64,
    pub sample_rate: u32,
    pub bit_depth: u16,
    /// Highest absolute sample value before conversion. Above 1.0 means
    /// the mix clipped.
    pub peak: f32,
    pub cancelled: bool,
}

#[derive(Debug, Error)]
pub enum MixdownError {
    #[error("There is nothing to mix down in that range.")]
    Empty,
    #[error("Bit depth must be 16, 24 or 32.")]
    BadBitDepth,
    #[error("Could not write the file: {0}")]
    Write(String),
}

/// Session samples where the last clip (audio or MIDI) ends.
pub fn session_end(session: &Session) -> u64 {
    plan::build(session, session.sample_rate)
        .tracks
        .iter()
        .flat_map(|t| t.clips.iter().map(|c| c.start + c.len))
        .max()
        .unwrap_or(0)
        .max(plan::midi_end(session))
}

/// What a render pass produced.
pub struct Rendered {
    pub frames: u64,
    pub sample_rate: u32,
    pub peak: f32,
    pub cancelled: bool,
}

/// Renders the requested part of the session as stereo blocks and hands
/// each block (interleaved, at the session rate) to `sink`. Shared by
/// mixdowns and loudness checks so both hear exactly what plays.
pub fn render_mix(
    session: &Session,
    req: &MixdownRequest,
    cancel: &AtomicBool,
    progress: impl FnMut(u64, u64),
    sink: impl FnMut(&[f32]) -> Result<(), MixdownError>,
) -> Result<Rendered, MixdownError> {
    // Plug-ins get hosts of their own, so an export never disturbs playback.
    crate::plugins::instance::fresh(|| render_mix_inner(session, req, cancel, progress, sink))
}

fn render_mix_inner(
    session: &Session,
    req: &MixdownRequest,
    cancel: &AtomicBool,
    mut progress: impl FnMut(u64, u64),
    mut sink: impl FnMut(&[f32]) -> Result<(), MixdownError>,
) -> Result<Rendered, MixdownError> {
    let full_end = session_end(session);
    let mut s = session.clone();
    if let Some(id) = req.track {
        s.tracks.retain(|t| t.id == id);
        for t in &mut s.tracks {
            t.muted = false;
            t.solo = false;
        }
        for b in &mut s.buses {
            b.muted = false;
            b.solo = false;
        }
    }
    if let Some(ids) = req.clips.as_ref().filter(|v| !v.is_empty()) {
        let keep: HashSet<Id> = ids.iter().copied().collect();
        for t in &mut s.tracks {
            t.clips.retain(|c| keep.contains(&c.id));
            t.midi_clips.retain(|c| keep.contains(&c.id));
        }
    }
    if req.dry {
        s.strip_effects();
    }
    if req.clip_only {
        for t in &mut s.tracks {
            t.muted = false;
            t.solo = false;
            t.volume_db = 0.0;
            t.pan = 0.0;
            t.output = crate::session::model::Output::Master;
            t.sends.clear();
        }
        s.buses.clear();
        s.master.volume_db = 0.0;
        s.master.effects = Default::default();
    }
    // 3D Space sessions: the layout and listening the export asked for.
    // Stems and clip mixes are placed as an ordinary stereo mix would be.
    let bed = match space_out(session, req) {
        Some((out, sp)) => {
            s.space = Some(sp);
            match out {
                SpaceOut::Bed(l) => crate::dsp::space::speakers(l).len(),
                SpaceOut::Stereo => 0,
            }
        }
        None => {
            if s.space.is_some() {
                s.space = Some(crate::session::model::Space {
                    listen: crate::session::model::SpaceListen::Stereo,
                    ..s.space.expect("checked")
                });
            }
            0
        }
    };
    let rate = s.sample_rate;
    let (start, end) = match req.range {
        Some((a, b)) => (a.min(b), a.max(b)),
        None if req.track.is_some() => (0, full_end),
        None => (0, session_end(&s)),
    };
    if end <= start
        || s.tracks
            .iter()
            .all(|t| t.clips.is_empty() && t.midi_clips.is_empty())
    {
        return Err(MixdownError::Empty);
    }
    let total = end - start;

    let master = plan::master_gain(&s);
    // 3D Space: multichannel clips' channels on their speakers.
    let s = crate::engine::speaker_feed::expanded(&s);
    let renderer = Renderer::new(plan::build(&s, rate), start);
    let mut graph = Graph::new(&s, rate, true);
    graph.set_want_bed(bed > 0);
    let mut mix = Mix::new(renderer, graph);
    let mut out = vec![Vec::with_capacity(BLOCK * 2)];
    let mut bed_buf: Vec<f32> = Vec::new();
    let mut peak = 0f32;
    let mut done = 0u64;
    let clock = std::time::Instant::now();
    while done < total {
        if req.realtime {
            // Stay no more than a block ahead of the wall clock, as playback does.
            let due = std::time::Duration::from_secs_f64(done as f64 / f64::from(rate));
            let ahead = due.saturating_sub(clock.elapsed());
            if ahead > std::time::Duration::from_millis(20) {
                std::thread::sleep(ahead - std::time::Duration::from_millis(10));
            }
        }
        if cancel.load(Ordering::Relaxed) {
            return Ok(Rendered {
                frames: done,
                sample_rate: rate,
                peak,
                cancelled: true,
            });
        }
        let n = BLOCK.min((total - done) as usize);
        let got = mix.render(n, &mut out);
        if bed > 0 {
            // Every speaker channel, after the master fader.
            bed_buf.clear();
            bed_buf.extend_from_slice(&mix.graph().bed()[..got * bed]);
            for v in bed_buf.iter_mut() {
                *v *= master;
                peak = peak.max(v.abs());
            }
            sink(&bed_buf)?;
        } else {
            let out = &mut out[0][..got * 2];
            for v in out.iter_mut() {
                *v *= master;
                peak = peak.max(v.abs());
            }
            sink(out)?;
        }
        done += got as u64;
        progress(done, total);
    }
    Ok(Rendered {
        frames: total,
        sample_rate: rate,
        peak,
        cancelled: false,
    })
}

/// Renders the session to `path`. `progress` gets (done, total) frames.
/// Setting `cancel` stops early and deletes the partial file.
pub fn mixdown(
    session: &Session,
    req: &MixdownRequest,
    path: &Path,
    cancel: &AtomicBool,
    progress: impl FnMut(u64, u64),
) -> Result<MixdownResult, MixdownError> {
    if let Some(b) = req.bit_depth {
        if ![16, 24, 32].contains(&b) {
            return Err(MixdownError::BadBitDepth);
        }
    }
    // A session mixed to mono exports mono unless the request says otherwise.
    let mono = req.channels == Some(1)
        || (req.channels.is_none()
            && req.track.is_none()
            && session.master.channels == crate::session::model::ChannelLayout::Mono);
    let mut fmt = req.format.clone().unwrap_or_default();
    if fmt.bit_depth.is_none() {
        fmt.bit_depth = req.bit_depth;
    }
    if fmt.channels.is_none() {
        fmt.channels = Some(if mono { 1 } else { 2 });
    }
    let fmt = fmt
        .resolve(session.sample_rate, 2, session.bit_depth)
        .map_err(|e| MixdownError::Write(e.to_string()))?;
    let enc_err = |e: EncodeError| MixdownError::Write(e.to_string());
    // Check the request before creating a file, so a bad one leaves nothing behind.
    if render_mix(session, req, &AtomicBool::new(true), |_, _| {}, |_| Ok(())).is_err() {
        return Err(MixdownError::Empty);
    }
    if let Some((SpaceOut::Bed(layout), _)) = space_out(session, req) {
        return mixdown_bed(session, req, path, cancel, progress, layout, fmt.bits);
    }
    let mut w = Writer::create(path, &fmt, session.sample_rate, 2).map_err(enc_err)?;
    let r = render_mix(session, req, cancel, progress, |stereo| {
        w.write(stereo).map_err(enc_err)
    })?;
    if r.cancelled {
        drop(w);
        let _ = std::fs::remove_file(path);
    } else {
        w.finish().map_err(enc_err)?;
    }
    Ok(MixdownResult {
        frames: r.frames,
        sample_rate: r.sample_rate,
        bit_depth: fmt.bits,
        peak: r.peak,
        cancelled: r.cancelled,
    })
}

/// A 3D Space session to a multichannel WAV, one channel per speaker of
/// `layout`, labeled so other software knows which is which.
fn mixdown_bed(
    session: &Session,
    req: &MixdownRequest,
    path: &Path,
    cancel: &AtomicBool,
    progress: impl FnMut(u64, u64),
    layout: crate::session::model::SpaceLayout,
    bits: u16,
) -> Result<MixdownResult, MixdownError> {
    if req
        .format
        .as_ref()
        .is_some_and(|f| f.kind != super::encode::FormatKind::Wav)
    {
        return Err(MixdownError::Write(
            "Speaker layouts are written as WAV. Pick WAV, or 2.0 or binaural for other formats."
                .into(),
        ));
    }
    let werr = |e: std::io::Error| MixdownError::Write(e.to_string());
    let ch = crate::dsp::space::speakers(layout).len() as u16;
    let mask = crate::dsp::space::channel_mask(layout);
    let mut w = super::wav_multi::MultiWav::create(path, session.sample_rate, ch, mask, bits)
        .map_err(werr)?;
    let r = render_mix(session, req, cancel, progress, |bed| {
        w.write(bed).map_err(werr)
    })?;
    if r.cancelled {
        drop(w);
        let _ = std::fs::remove_file(path);
    } else {
        w.finish().map_err(werr)?;
    }
    Ok(MixdownResult {
        frames: r.frames,
        sample_rate: r.sample_rate,
        bit_depth: if [16, 24, 32].contains(&bits) {
            bits
        } else {
            24
        },
        peak: r.peak,
        cancelled: r.cancelled,
    })
}

/// A path in `dir` named `stem.ext` that does not exist yet: "stem 2",
/// "stem 3" and so on when it does. Characters that are not allowed in
/// file names on Windows are replaced.
pub fn unique_path(dir: &Path, stem: &str, ext: &str) -> std::path::PathBuf {
    let clean: String = stem
        .chars()
        .map(|c| {
            if "<>:\"/\\|?*".contains(c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let clean = clean.trim().trim_end_matches('.');
    let clean = if clean.is_empty() { "Merged" } else { clean };
    let mut path = dir.join(format!("{clean}.{ext}"));
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("{clean} {n}.{ext}"));
        n += 1;
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::model::{new_id, ChannelLayout, MediaRef, Track};
    use crate::session::ops::{apply, ClipSpec, Op};

    fn tone(dir: &Path, name: &str, frames: u32, value: i16) -> std::path::PathBuf {
        let path = dir.join(name);
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 48_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(&path, spec).unwrap();
        for _ in 0..frames {
            w.write_sample(value).unwrap();
        }
        w.finalize().unwrap();
        path
    }

    #[test]
    fn a_3d_session_writes_every_speaker_or_stereo_as_asked() {
        use crate::session::model::{Space, SpaceLayout, SpaceListen, SpacePos};
        let dir = tempfile::tempdir().unwrap();
        let (mut s, _, _) = session_with(dir.path());
        s.space = Some(Space {
            layout: SpaceLayout::Surround51,
            listen: SpaceListen::Stereo,
        });
        // A at the front left speaker, B at the back center.
        s.tracks[0].space = Some(SpacePos {
            x: -1.0,
            y: 1.0,
            lfe: 1.0,
            ..SpacePos::default()
        });
        s.tracks[1].space = Some(SpacePos {
            x: 0.0,
            y: -1.0,
            ..SpacePos::default()
        });
        let path = dir.path().join("bed.wav");
        let req = MixdownRequest {
            bit_depth: Some(32),
            ..MixdownRequest::default()
        };
        mixdown(&s, &req, &path, &AtomicBool::new(false), |_, _| {}).unwrap();
        let r = hound::WavReader::open(&path).unwrap();
        assert_eq!(r.spec().channels, 6);
        let x: Vec<f32> = r.into_samples::<f32>().map(Result::unwrap).collect();
        let at = |frame: usize, ch: usize| (x[frame * 6 + ch] * 1_000.0).round() / 1_000.0;
        assert_eq!((at(1_000, 0), at(1_000, 1), at(1_000, 2)), (0.25, 0.0, 0.0));
        // A's level is steady (no frequency at all), and the low-frequency
        // channel's 20 Hz high pass keeps such offsets out of the subwoofer.
        assert_eq!(at(10_000, 3), 0.0);
        assert_eq!(at(100_000, 3), 0.0);
        // B shares the two surrounds at -3 dB each.
        assert_eq!((at(100_000, 4), at(100_000, 5)), (0.177, 0.177));
        assert_eq!(at(100_000, 0), 0.0);
        // The same session to 7.1.4, then to stereo, then for headphones.
        for (layout, binaural, ch) in [
            (Some(SpaceLayout::Surround714), false, 12),
            (Some(SpaceLayout::Stereo20), false, 2),
            (None, true, 2),
        ] {
            let p = dir.path().join(format!("o{ch}{binaural}.wav"));
            let req = MixdownRequest {
                space_layout: layout,
                binaural,
                ..MixdownRequest::default()
            };
            mixdown(&s, &req, &p, &AtomicBool::new(false), |_, _| {}).unwrap();
            let r = hound::WavReader::open(&p).unwrap();
            assert_eq!(r.spec().channels, ch);
        }
    }

    #[test]
    fn a_surround_file_plays_each_channel_from_its_speaker() {
        use crate::session::model::{Space, SpaceLayout, SpaceListen, Track};
        let dir = tempfile::tempdir().unwrap();
        // Six channels, each its own steady level.
        let path = dir.path().join("film.wav");
        let spec = hound::WavSpec {
            channels: 6,
            sample_rate: 48_000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut w = hound::WavWriter::create(&path, spec).unwrap();
        for _ in 0..48_000 {
            for c in 0..6 {
                w.write_sample(0.1 * (c + 1) as f32).unwrap();
            }
        }
        w.finalize().unwrap();
        let mut s = Session::new("t", 48_000);
        s.space = Some(Space {
            layout: SpaceLayout::Surround51,
            listen: SpaceListen::Stereo,
        });
        let m = new_id();
        s.media.push(MediaRef {
            id: m,
            path,
            relative_path: None,
            sample_rate: 48_000,
            channels: 6,
            length: 48_000,
        });
        let mut t = Track::new("Film", ChannelLayout::Stereo);
        t.clips.push(
            crate::session::ops::ClipSpec {
                id: new_id(),
                name: "film".into(),
                media_id: m,
                start: 0,
                source_in: 0,
                source_out: 48_000,
            }
            .build(),
        );
        s.tracks.push(t);
        let out = dir.path().join("bed.wav");
        let req = MixdownRequest {
            bit_depth: Some(32),
            ..MixdownRequest::default()
        };
        mixdown(&s, &req, &out, &AtomicBool::new(false), |_, _| {}).unwrap();
        let r = hound::WavReader::open(&out).unwrap();
        assert_eq!(r.spec().channels, 6);
        let x: Vec<f32> = r.into_samples::<f32>().map(Result::unwrap).collect();
        let at = |ch: usize| (x[20_000 * 6 + ch] * 1_000.0).round() / 1_000.0;
        // Each speaker holds its own channel and nothing else (the steady
        // LFE level is kept out by the subwoofer's 20 Hz high pass).
        assert_eq!(
            (at(0), at(1), at(2), at(3), at(4), at(5)),
            (0.1, 0.2, 0.3, 0.0, 0.5, 0.6)
        );
    }

    fn session_with(dir: &Path) -> (Session, Id, Id) {
        let mut s = Session::new("t", 48_000);
        s.bit_depth = 24;
        let path = tone(dir, "a.wav", 48_000, 8_192);
        let m = MediaRef {
            id: new_id(),
            path,
            relative_path: None,
            sample_rate: 48_000,
            channels: 1,
            length: 48_000,
        };
        let mid = m.id;
        s.media.push(m);
        s.tracks.push(Track::new("A", ChannelLayout::Mono));
        s.tracks.push(Track::new("B", ChannelLayout::Mono));
        let (a, b) = (new_id(), new_id());
        for (ti, (id, start)) in [(a, 0u64), (b, 96_000u64)].into_iter().enumerate() {
            let tid = s.tracks[ti].id;
            apply(
                &mut s,
                &Op::AddClips {
                    track_id: tid,
                    clips: vec![ClipSpec {
                        id,
                        name: "c".into(),
                        media_id: mid,
                        start,
                        source_in: 0,
                        source_out: 48_000,
                    }],
                },
            )
            .unwrap();
        }
        (s, a, b)
    }

    #[test]
    fn mixes_the_whole_session_at_the_session_depth() {
        let dir = tempfile::tempdir().unwrap();
        let (mut s, _, _) = session_with(dir.path());
        s.tracks[1].volume_db = -6.0;
        let out = dir.path().join("mix.wav");
        let mut last = (0, 0);
        let r = mixdown(
            &s,
            &MixdownRequest::default(),
            &out,
            &AtomicBool::new(false),
            |d, t| last = (d, t),
        )
        .unwrap();
        assert_eq!(r.frames, 144_000, "ends where the last clip ends");
        assert_eq!(last, (144_000, 144_000));
        let mut rd = hound::WavReader::open(&out).unwrap();
        assert_eq!(rd.spec().bits_per_sample, 24);
        assert_eq!(rd.spec().channels, 2);
        let v: Vec<i32> = rd.samples::<i32>().map(|x| x.unwrap()).collect();
        let at = |frame: usize| v[frame * 2] as f32 / 8_388_607.0;
        assert!((at(1_000) - 0.25).abs() < 0.01, "track A at full level");
        assert!(at(60_000).abs() < 0.001, "silence between clips");
        assert!((at(100_000) - 0.125).abs() < 0.01, "track B at -6 dB");
    }

    #[test]
    fn a_realtime_mixdown_takes_as_long_as_the_audio() {
        let dir = tempfile::tempdir().unwrap();
        let (s, _a, _b) = session_with(dir.path());
        let req = MixdownRequest {
            range: Some((0, 24_000)),
            realtime: true,
            ..Default::default()
        };
        let at = std::time::Instant::now();
        let r = render_mix(&s, &req, &AtomicBool::new(false), |_, _| {}, |_| Ok(())).unwrap();
        assert_eq!(r.frames, 24_000);
        // Paced a block at a time: done no sooner than the audio, less the last block.
        assert!(
            at.elapsed() >= std::time::Duration::from_millis(250),
            "{:?}",
            at.elapsed()
        );
    }

    #[test]
    fn mixes_a_range_or_selected_clips_and_can_cancel() {
        let dir = tempfile::tempdir().unwrap();
        let (s, _a, b) = session_with(dir.path());
        let out = dir.path().join("part.wav");
        let req = MixdownRequest {
            range: Some((90_000, 100_000)),
            clips: Some(vec![b]),
            bit_depth: Some(32),
            track: None,
            clip_only: false,
            dry: false,
            channels: None,
            format: None,
            realtime: false,
            space_layout: None,
            binaural: false,
        };
        let r = mixdown(&s, &req, &out, &AtomicBool::new(false), |_, _| {}).unwrap();
        assert_eq!(r.frames, 10_000);
        let v: Vec<f32> = hound::WavReader::open(&out)
            .unwrap()
            .samples::<f32>()
            .map(|x| x.unwrap())
            .collect();
        assert!(v[0].abs() < 1e-6 && (v[2 * 9_000] - 0.25).abs() < 0.01);

        let gone = dir.path().join("gone.wav");
        let r = mixdown(
            &s,
            &MixdownRequest::default(),
            &gone,
            &AtomicBool::new(true),
            |_, _| {},
        )
        .unwrap();
        assert!(r.cancelled);
        assert!(!gone.exists());
        assert!(matches!(
            mixdown(
                &s,
                &MixdownRequest {
                    range: Some((5, 5)),
                    ..Default::default()
                },
                &gone,
                &AtomicBool::new(false),
                |_, _| {}
            ),
            Err(MixdownError::Empty)
        ));
    }

    #[test]
    fn a_stem_is_one_track_for_the_whole_session() {
        let dir = tempfile::tempdir().unwrap();
        let (mut s, _, _) = session_with(dir.path());
        s.tracks[0].muted = true;
        s.tracks[1].solo = true;
        let out = dir.path().join("stem.wav");
        let req = MixdownRequest {
            track: Some(s.tracks[0].id),
            bit_depth: Some(32),
            ..Default::default()
        };
        let r = mixdown(&s, &req, &out, &AtomicBool::new(false), |_, _| {}).unwrap();
        assert_eq!(r.frames, 144_000, "as long as the whole session");
        let v: Vec<f32> = hound::WavReader::open(&out)
            .unwrap()
            .samples::<f32>()
            .map(|x| x.unwrap())
            .collect();
        assert!((v[2 * 1_000] - 0.25).abs() < 0.01, "plays though muted");
        assert!(v[2 * 100_000].abs() < 1e-6, "other tracks left out");
    }

    #[test]
    fn mixes_through_buses_and_their_chains() {
        let dir = tempfile::tempdir().unwrap();
        let (mut s, _, _) = session_with(dir.path());
        let mut bus = crate::session::model::Bus::new("Voices", ChannelLayout::Stereo);
        bus.volume_db = -6.0206;
        bus.effects.output_gain_db = -6.0206;
        let bid = bus.id;
        s.buses.push(bus);
        s.tracks[0].output = crate::session::model::Output::Bus { bus_id: bid };
        let out = dir.path().join("bus.wav");
        let req = MixdownRequest {
            range: Some((0, 48_000)),
            bit_depth: Some(32),
            ..MixdownRequest::default()
        };
        mixdown(&s, &req, &out, &AtomicBool::new(false), |_, _| {}).unwrap();
        let v: Vec<f32> = hound::WavReader::open(&out)
            .unwrap()
            .into_samples::<f32>()
            .map(|x| x.unwrap())
            .collect();
        // 0.25 through the bus chain (-6 dB) and the bus fader (-6 dB).
        assert!((v[2_000] - 0.0625).abs() < 0.002, "got {}", v[2_000]);
        // A stem of the routed track still goes through its bus.
        let req = MixdownRequest {
            range: Some((0, 48_000)),
            bit_depth: Some(32),
            track: Some(s.tracks[0].id),
            ..MixdownRequest::default()
        };
        s.buses[0].muted = true;
        mixdown(&s, &req, &out, &AtomicBool::new(false), |_, _| {}).unwrap();
        let v: Vec<f32> = hound::WavReader::open(&out)
            .unwrap()
            .into_samples::<f32>()
            .map(|x| x.unwrap())
            .collect();
        assert!((v[2_000] - 0.0625).abs() < 0.002, "stem got {}", v[2_000]);
    }

    #[test]
    fn clip_only_leaves_out_track_and_master_settings() {
        let dir = tempfile::tempdir().unwrap();
        let (mut s, a, _) = session_with(dir.path());
        s.tracks[0].volume_db = -12.0;
        s.tracks[0].muted = true;
        s.master.volume_db = -6.0;
        s.tracks[0].clips[0].gain_db = -6.0;
        let out = dir.path().join("merged.wav");
        let req = MixdownRequest {
            range: Some((0, 48_000)),
            clips: Some(vec![a]),
            bit_depth: Some(32),
            track: Some(s.tracks[0].id),
            clip_only: true,
            dry: false,
            channels: Some(1),
            format: None,
            realtime: false,
            space_layout: None,
            binaural: false,
        };
        mixdown(&s, &req, &out, &AtomicBool::new(false), |_, _| {}).unwrap();
        let rd = hound::WavReader::open(&out).unwrap();
        assert_eq!(rd.spec().channels, 1, "mono on request");
        let v: Vec<f32> = rd.into_samples::<f32>().map(|x| x.unwrap()).collect();
        assert_eq!(v.len(), 48_000);
        // 0.25 with the clip's -6 dB only: about 0.125.
        assert!((v[1_000] - 0.125).abs() < 0.005, "got {}", v[1_000]);
    }

    #[test]
    fn unique_path_counts_up_and_cleans_names() {
        let dir = tempfile::tempdir().unwrap();
        let first = unique_path(dir.path(), "Intro: Take/1", "wav");
        assert_eq!(first.file_name().unwrap(), "Intro_ Take_1.wav");
        std::fs::write(&first, b"x").unwrap();
        let second = unique_path(dir.path(), "Intro: Take/1", "wav");
        assert_eq!(second.file_name().unwrap(), "Intro_ Take_1 2.wav");
        assert_eq!(
            unique_path(dir.path(), " . ", "wav").file_name().unwrap(),
            "Merged.wav"
        );
    }

    #[test]
    fn mixes_to_flac_and_mp3_at_another_rate() {
        use crate::export::encode::{FormatKind, OutputFormat};
        let dir = tempfile::tempdir().unwrap();
        let (s, _, _) = session_with(dir.path());
        for kind in [FormatKind::Flac, FormatKind::Mp3] {
            let out = dir.path().join(format!("mix.{}", kind.extension()));
            let req = MixdownRequest {
                format: Some(OutputFormat {
                    kind,
                    sample_rate: Some(44_100),
                    ..Default::default()
                }),
                ..Default::default()
            };
            let r = mixdown(&s, &req, &out, &AtomicBool::new(false), |_, _| {}).unwrap();
            assert_eq!(r.frames, 144_000, "frames rendered at the session rate");
            let info = crate::codecs::probe(&out).unwrap();
            assert_eq!(info.sample_rate, 44_100);
            let secs = info.length as f64 / 44_100.0;
            assert!((secs - 3.0).abs() < 0.1, "{kind:?}: {secs} s");
        }
    }
}
