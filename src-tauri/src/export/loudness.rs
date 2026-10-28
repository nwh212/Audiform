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
    pub seconds: f64,
    pub cancelled: bool,
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
    let r = render_mix(session, req, cancel, progress, |block| {
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
    Ok(LoudnessReport {
        integrated: stereo.loudness_global().map_err(meter_err)?,
        integrated_mono: mono.loudness_global().map_err(meter_err)?,
        range: stereo.loudness_range().map_err(meter_err)?,
        true_peak: peak(&|c| stereo.true_peak(c))?,
        sample_peak: peak(&|c| stereo.sample_peak(c))?,
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
        assert!((r.seconds - 10.0).abs() < 1e-6);
    }
}
