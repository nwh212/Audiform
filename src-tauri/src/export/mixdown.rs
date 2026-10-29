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
use super::loudness::LoudnessReport;
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
    /// Brings the mix to a loudness before it is written (podcast
    /// targets): measured first, then one fixed gain, never past the peak
    /// limit. Stereo and mono files only; a 3D Space speaker bed is left
    /// as mixed.
    #[serde(default)]
    pub normalize: Option<Normalize>,
}

/// A loudness to export at.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct Normalize {
    /// Integrated loudness to reach, LUFS.
    pub lufs: f64,
    /// Highest true peak allowed after the gain, dBTP.
    pub peak: f64,
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

/// The session as the request renders it, with the span and the speaker
/// count. Cheap: nothing is started, so it also serves to check a request.
struct Prepared {
    s: Session,
    bed: usize,
    start: u64,
    end: u64,
}

fn prepare(session: &Session, req: &MixdownRequest) -> Result<Prepared, MixdownError> {
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
    Ok(Prepared { s, bed, start, end })
}

fn render_mix_inner(
    session: &Session,
    req: &MixdownRequest,
    cancel: &AtomicBool,
    mut progress: impl FnMut(u64, u64),
    mut sink: impl FnMut(&[f32]) -> Result<(), MixdownError>,
) -> Result<Rendered, MixdownError> {
    let Prepared { s, bed, start, end } = prepare(session, req)?;
    let rate = s.sample_rate;
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
    // Check the request before creating a file, so a bad one leaves nothing
    // behind. Without rendering: that would start every plug-in twice.
    prepare(session, req)?;
    if let Some((SpaceOut::Bed(layout), _)) = space_out(session, req) {
        return mixdown_bed(session, req, path, cancel, progress, layout, fmt.bits);
    }
    let mut progress = progress;
    let mut gain = 1.0f32;
    let mut passes = 1u64;
    if let Some(n) = req.normalize {
        // Measure first (the same render), then write with one fixed gain.
        passes = 2;
        let report = super::loudness::measure(session, req, cancel, |d, t| progress(d, t * 2))?;
        if report.cancelled {
            return Ok(MixdownResult {
                frames: 0,
                sample_rate: session.sample_rate,
                bit_depth: fmt.bits,
                peak: 0.0,
                cancelled: true,
            });
        }
        gain = normalize_gain(&report, n, mono);
    }
    let mut w = Writer::create(path, &fmt, session.sample_rate, 2).map_err(enc_err)?;
    let mut scaled: Vec<f32> = Vec::new();
    let r = render_mix(
        session,
        req,
        cancel,
        |d, t| progress(d + (passes - 1) * t, t * passes),
        |stereo| {
            if gain == 1.0 {
                w.write(stereo).map_err(enc_err)
            } else {
                scaled.clear();
                scaled.extend(stereo.iter().map(|v| v * gain));
                w.write(&scaled).map_err(enc_err)
            }
        },
    )?;
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
        peak: r.peak * gain,
        cancelled: r.cancelled,
    })
}

/// The gain that brings a mix to the target loudness, held back so its
/// true peak stays under the target's limit. Silence is left alone.
fn normalize_gain(report: &LoudnessReport, n: Normalize, mono: bool) -> f32 {
    let measured = if mono {
        report.integrated_mono
    } else {
        report.integrated
    };
    if !measured.is_finite() || !report.true_peak.is_finite() {
        return 1.0;
    }
    let want = n.lufs - measured;
    let room = n.peak - report.true_peak;
    10f64.powf(want.min(room).clamp(-60.0, 60.0) / 20.0) as f32
}

/// Layouts a FLAC file can carry with every channel in its standard place:
/// stereo, 5.1 and 7.1 (FLAC stops at 8 channels and has no height speakers).
pub fn bed_flac_ok(layout: crate::session::model::SpaceLayout) -> bool {
    use crate::session::model::SpaceLayout::{Stereo20, Surround51, Surround71};
    matches!(layout, Stereo20 | Surround51 | Surround71)
}

/// A 3D Space session to a multichannel FLAC, one channel per speaker, in
/// the order FLAC defines for 2.0, 5.1 and 7.1.
fn mixdown_bed_flac(
    session: &Session,
    req: &MixdownRequest,
    path: &Path,
    cancel: &AtomicBool,
    progress: impl FnMut(u64, u64),
    layout: crate::session::model::SpaceLayout,
) -> Result<MixdownResult, MixdownError> {
    let enc_err = |e: EncodeError| MixdownError::Write(e.to_string());
    let ch = crate::dsp::space::speakers(layout).len() as u16;
    let mut want = req.format.clone().unwrap_or_default();
    want.channels = Some(ch);
    let fmt = want
        .resolve(session.sample_rate, ch, session.bit_depth)
        .map_err(enc_err)?;
    let mut w =
        super::encode::Writer::create(path, &fmt, session.sample_rate, ch).map_err(enc_err)?;
    let r = render_mix(session, req, cancel, progress, |bed| {
        w.write(bed).map_err(enc_err)
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
    let kind = req
        .format
        .as_ref()
        .map_or(super::encode::FormatKind::Wav, |f| f.kind);
    match kind {
        super::encode::FormatKind::Wav => {}
        super::encode::FormatKind::Flac if bed_flac_ok(layout) => {
            return mixdown_bed_flac(session, req, path, cancel, progress, layout);
        }
        super::encode::FormatKind::Flac => {
            return Err(MixdownError::Write(
                "FLAC carries up to 8 channels, in the standard 5.1 and 7.1 orders. Pick WAV for this layout, or 2.0 or binaural.".into(),
            ));
        }
        _ => {
            return Err(MixdownError::Write(
                "This format can't carry a speaker layout. Pick WAV (or FLAC for 5.1 and 7.1), or 2.0 or binaural.".into(),
            ));
        }
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
mod tests;
