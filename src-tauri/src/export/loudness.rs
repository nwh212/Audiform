// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Loudness check (EBU R128 / ITU-R BS.1770): integrated loudness,
//! loudness range and true peak of the mix, the numbers podcast
//! platforms normalize by. Uses the same render as mixdowns, so the
//! numbers match the exported file.

use std::sync::atomic::AtomicBool;

use ebur128::{EbuR128, Mode};
use serde::Serialize;

use super::mixdown::{render_mix, MixdownError, MixdownRequest};
use crate::session::model::Session;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LoudnessReport {
    /// Integrated loudness of the stereo mix, LUFS.
    pub integrated: f64,
    /// Integrated loudness of the mix folded to mono, LUFS. Compare this
    /// with mono targets.
    pub integrated_mono: f64,
    /// Loudness range, LU.
    pub range: f64,
    /// Highest true (inter-sample) peak, dBTP.
    pub true_peak: f64,
    /// Highest sample peak, dBFS.
    pub sample_peak: f64,
    /// Average (RMS) level of both channels together, dBFS: a full-scale
    /// sine reads -3.
    pub rms: f64,
    /// Quietest half second that is not digital silence, dBFS RMS: the
    /// room tone. None when the stretch has no such gap.
    pub noise_floor: Option<f64>,
    /// Seconds of digital silence (below -120 dBFS), which audiobook
    /// checks reject.
    pub silent_seconds: f64,
    pub seconds: f64,
    pub cancelled: bool,
}

/// Windows are 50 ms; the noise floor is the quietest run of this many.
const FLOOR_WINDOWS: usize = 10;
/// A window quieter than this (about -120 dBFS) is digital silence, not room tone.
const SILENT_POWER: f64 = 1e-12;

/// The noise floor and the digital silence of a stretch, from 50 ms
/// windows of its power (both channels together).
struct Floor {
    win: usize,
    in_win: usize,
    win_sum: f64,
    windows: Vec<f64>,
}

impl Floor {
    fn new(rate: u32) -> Self {
        Self {
            win: (rate as usize / 20).max(1),
            in_win: 0,
            win_sum: 0.0,
            windows: Vec::new(),
        }
    }

    /// Adds interleaved stereo frames.
    fn add(&mut self, block: &[f32]) {
        for f in block.as_chunks::<2>().0 {
            self.win_sum += (f64::from(f[0]).powi(2) + f64::from(f[1]).powi(2)) * 0.5;
            self.in_win += 1;
            if self.in_win == self.win {
                self.windows.push(self.win_sum / self.win as f64);
                self.in_win = 0;
                self.win_sum = 0.0;
            }
        }
    }

    /// (noise floor dBFS, seconds of digital silence).
    fn finish(&self, rate: u32) -> (Option<f64>, f64) {
        let silent = self.windows.iter().filter(|p| **p < SILENT_POWER).count() as f64
            * self.win as f64
            / f64::from(rate.max(1));
        let floor = self
            .windows
            .windows(FLOOR_WINDOWS)
            .filter(|run| run.iter().all(|p| *p >= SILENT_POWER))
            .map(|run| run.iter().sum::<f64>() / FLOOR_WINDOWS as f64)
            .fold(None, |m: Option<f64>, v| Some(m.map_or(v, |m| m.min(v))));
        (floor.map(|p| 10.0 * p.log10()), silent)
    }
}

fn db(x: f64) -> f64 {
    if x <= 0.0 {
        f64::NEG_INFINITY
    } else {
        20.0 * x.log10()
    }
}

fn meter_err(e: ebur128::Error) -> MixdownError {
    MixdownError::Write(format!("Loudness meter: {e:?}"))
}

/// Measures the requested part of the session.
pub fn measure(
    session: &Session,
    req: &MixdownRequest,
    cancel: &AtomicBool,
    progress: impl FnMut(u64, u64),
) -> Result<LoudnessReport, MixdownError> {
    let rate = session.sample_rate;
    let mut stereo = EbuR128::new(
        2,
        rate,
        Mode::I | Mode::LRA | Mode::TRUE_PEAK | Mode::SAMPLE_PEAK,
    )
    .map_err(meter_err)?;
    let mut mono = EbuR128::new(1, rate, Mode::I).map_err(meter_err)?;
    let mut folded: Vec<f32> = Vec::new();
    let (mut power, mut samples) = (0f64, 0u64);
    let mut floor = Floor::new(rate);
    let r = render_mix(session, req, cancel, progress, |block| {
        floor.add(block);
        power += block
            .iter()
            .map(|v| f64::from(*v) * f64::from(*v))
            .sum::<f64>();
        samples += block.len() as u64;
        stereo.add_frames_f32(block).map_err(meter_err)?;
        folded.clear();
        folded.extend(block.as_chunks::<2>().0.iter().map(|f| (f[0] + f[1]) * 0.5));
        mono.add_frames_f32(&folded).map_err(meter_err)?;
        Ok(())
    })?;
    let peak = |f: &dyn Fn(u32) -> Result<f64, ebur128::Error>| -> Result<f64, MixdownError> {
        Ok(db((0..2)
            .map(f)
            .collect::<Result<Vec<_>, _>>()
            .map_err(meter_err)?
            .into_iter()
            .fold(0.0, f64::max)))
    };
    let (noise_floor, silent_seconds) = floor.finish(rate);
    Ok(LoudnessReport {
        integrated: stereo.loudness_global().map_err(meter_err)?,
        integrated_mono: mono.loudness_global().map_err(meter_err)?,
        range: stereo.loudness_range().map_err(meter_err)?,
        true_peak: peak(&|c| stereo.true_peak(c))?,
        sample_peak: peak(&|c| stereo.sample_peak(c))?,
        rms: if samples > 0 {
            db((power / samples as f64).sqrt())
        } else {
            f64::NEG_INFINITY
        },
        noise_floor,
        silent_seconds,
        seconds: r.frames as f64 / f64::from(rate.max(1)),
        cancelled: r.cancelled,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::model::{new_id, ChannelLayout, MediaRef, Track};
    use crate::session::ops::{apply, ClipSpec, Op};
    use std::path::Path;

    /// A 997 Hz sine at the given peak, the usual meter test tone.
    fn sine(dir: &Path, peak: f32, seconds: u32) -> std::path::PathBuf {
        let path = dir.join("sine.wav");
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 48_000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut w = hound::WavWriter::create(&path, spec).unwrap();
        for i in 0..48_000 * seconds {
            let t = i as f32 / 48_000.0;
            w.write_sample(peak * (2.0 * std::f32::consts::PI * 997.0 * t).sin())
                .unwrap();
        }
        w.finalize().unwrap();
        path
    }

    #[test]
    fn measures_a_known_tone() {
        let dir = tempfile::tempdir().unwrap();
        let path = sine(dir.path(), 0.5, 10);
        let mut s = Session::new("t", 48_000);
        let m = MediaRef {
            id: new_id(),
            path,
            relative_path: None,
            sample_rate: 48_000,
            channels: 1,
            length: 480_000,
            linked: None,
        };
        let mid = m.id;
        s.media.push(m);
        s.tracks.push(Track::new("A", ChannelLayout::Mono));
        let tid = s.tracks[0].id;
        apply(
            &mut s,
            &Op::AddClips {
                track_id: tid,
                clips: vec![ClipSpec {
                    id: new_id(),
                    name: "c".into(),
                    media_id: mid,
                    start: 0,
                    source_in: 0,
                    source_out: 480_000,
                    channel_mix: None,
                }],
            },
        )
        .unwrap();
        let r = measure(
            &s,
            &MixdownRequest::default(),
            &AtomicBool::new(false),
            |_, _| {},
        )
        .unwrap();
        // A sine peaking at -6 dBFS in both channels measures about
        // -6 dB - 3 dB (sine RMS) + 3 dB (two channels) = -6 LUFS, and the
        // same signal folded to mono about -9 LUFS.
        assert!((r.integrated + 6.0).abs() < 0.5, "stereo {}", r.integrated);
        assert!(
            (r.integrated_mono + 9.0).abs() < 0.5,
            "mono {}",
            r.integrated_mono
        );
        assert!((r.sample_peak + 6.0).abs() < 0.1, "peak {}", r.sample_peak);
        assert!(r.true_peak >= r.sample_peak - 0.01);
        // A sine's RMS is 3 dB under its peak.
        assert!((r.rms + 9.0).abs() < 0.1, "rms {}", r.rms);
        assert!((r.seconds - 10.0).abs() < 1e-6);
    }
}

#[cfg(test)]
mod floor_tests {
    use super::*;

    fn tone(amp: f32, frames: usize) -> Vec<f32> {
        (0..frames)
            .flat_map(|i| {
                let v = amp * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 48_000.0).sin();
                [v, v]
            })
            .collect()
    }

    #[test]
    fn the_floor_is_the_quiet_part_between_the_loud_parts() {
        let mut s = Floor::new(48_000);
        s.add(&tone(0.5, 48_000));
        s.add(&tone(0.001, 48_000)); // -60 dB peak, so about -63 dB RMS
        s.add(&tone(0.5, 48_000));
        let (floor, silent) = s.finish(48_000);
        assert!((floor.unwrap() + 63.0).abs() < 0.5, "{floor:?}");
        assert_eq!(silent, 0.0);
    }

    #[test]
    fn digital_silence_is_counted_and_is_not_a_floor() {
        let mut s = Floor::new(48_000);
        s.add(&tone(0.5, 48_000));
        s.add(&vec![0.0; 2 * 48_000]);
        let (floor, silent) = s.finish(48_000);
        assert!(floor.is_none_or(|f| f > -20.0), "{floor:?}");
        assert!((silent - 1.0).abs() < 0.06, "{silent}");
        // Too short for half a second of anything: no floor.
        let mut short = Floor::new(48_000);
        short.add(&tone(0.5, 12_000));
        assert_eq!(short.finish(48_000).0, None);
    }
}
