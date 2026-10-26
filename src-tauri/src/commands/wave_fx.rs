// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Voice Chain in the Waveform editor: the chain plays as a preview on
//! the open file, and Apply writes it into the audio. Apply renders the
//! selection (or the whole file) through the chain into a new file; the
//! interface swaps that stretch in as one undoable edit, like any other
//! destructive edit.

use tauri::async_runtime::spawn_blocking;
use tauri::{AppHandle, State};

use super::spectral::{by_pairs, render_n, snapshot, write_n, SpectralResult};
use super::wave::OpenWave;
use crate::dsp::effects::ChainProcessor;
use crate::export::mixdown;
use crate::session::model::EffectChain;

/// Audio before the range the chain hears first, so filters and level
/// detectors have settled when the range starts.
const WARM_SECONDS: f64 = 0.25;
/// Blend from the untouched audio into the processed audio, and back,
/// at each end of the range, so a change in level there is not a step.
const EDGE_SECONDS: f64 = 0.005;

/// Runs `chain` over timeline samples `start..end` of `audio`, a stereo
/// render that starts `warm` frames before `start` and runs `latency`
/// frames past `end`. Returns the processed range, `end - start` frames.
pub fn process_range(
    chain: &mut ChainProcessor,
    audio: &[f32],
    warm: usize,
    frames: usize,
    edge: usize,
) -> Vec<f32> {
    let latency = chain.latency();
    let mut wet = audio.to_vec();
    for block in wet.chunks_mut(4096 * 2) {
        chain.process(block);
    }
    let dry = audio.as_chunks::<2>().0;
    let wet = wet.as_chunks::<2>().0;
    let edge = edge.min(frames / 2);
    let mut out = Vec::with_capacity(frames * 2);
    for i in 0..frames {
        let d = dry.get(warm + i).copied().unwrap_or([0.0; 2]);
        let w = wet.get(warm + latency + i).copied().unwrap_or([0.0; 2]);
        let k = if edge == 0 {
            1.0
        } else {
            let from_edge = i.min(frames - 1 - i);
            (from_edge as f32 / edge as f32).min(1.0)
        };
        out.push(d[0] + (w[0] - d[0]) * k);
        out.push(d[1] + (w[1] - d[1]) * k);
    }
    out
}

/// Renders `start..end` of the open file through `chain` into a new
/// file in the app's cache folder, for the interface to swap in.
#[tauri::command]
pub async fn wave_apply_chain(
    app: AppHandle,
    wave: State<'_, OpenWave>,
    chain: EffectChain,
    start: u64,
    end: u64,
) -> Result<SpectralResult, String> {
    let session = snapshot(&wave)?;
    let dir = crate::commands::cache::cache_root(&app)?.join("wave-fx");
    spawn_blocking(move || -> Result<SpectralResult, String> {
        let rate = session.sample_rate;
        let total = mixdown::session_end(&session);
        let end = end.min(total);
        if end <= start {
            return Err("Select some audio first, or apply to the whole file.".into());
        }
        let empty = || "The Audio FX chain is empty or bypassed, so there is nothing to apply.";
        let latency = ChainProcessor::new(&chain, rate).ok_or(empty())?.latency();
        let warm = (f64::from(rate) * WARM_SECONDS) as usize;
        let from = start as i64 - warm as i64;
        let to = end as i64 + latency as i64;
        let (audio, n) = render_n(&session, from, to);
        let frames = (end - start) as usize;
        let edge = (f64::from(rate) * EDGE_SECONDS) as usize;
        // A multichannel file goes through the chain a pair of channels at a time.
        let out = by_pairs(&audio, n, |st| {
            let mut c = ChainProcessor::new(&chain, rate).ok_or(empty())?;
            Ok(process_range(&mut c, st, warm, frames, edge))
        })?;
        let media = write_n(&dir, "voice-chain", &out, n, 0, frames, rate)?;
        Ok(SpectralResult { media, start, end })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Stretches `audio` (stereo interleaved) to exactly `frames` frames,
/// keeping its pitch.
pub fn stretch_to(audio: &[f32], frames: usize, rate: u32) -> Vec<f32> {
    let have = audio.len() / 2;
    if have == 0 || frames == 0 {
        return vec![0.0; frames * 2];
    }
    let ratio = frames as f64 / have as f64;
    let mut st = crate::dsp::stretch::Stretcher::new(ratio, rate);
    let mut out = Vec::with_capacity(frames * 2 + 8192);
    for block in audio.chunks(8192 * 2) {
        st.push(block);
        st.pull(&mut out);
    }
    st.finish();
    st.pull(&mut out);
    out.resize(frames * 2, 0.0);
    out
}

/// Waveform editor Stretch: makes `start..end` of the open file last
/// `frames` frames, keeping its pitch (a 31.3 second spot read in 30).
/// The interface swaps the result in and moves what follows.
#[tauri::command]
pub async fn wave_stretch(
    app: AppHandle,
    wave: State<'_, OpenWave>,
    start: u64,
    end: u64,
    frames: u64,
) -> Result<SpectralResult, String> {
    let session = snapshot(&wave)?;
    let dir = crate::commands::cache::cache_root(&app)?.join("wave-fx");
    spawn_blocking(move || -> Result<SpectralResult, String> {
        let rate = session.sample_rate;
        let end = end.min(mixdown::session_end(&session));
        if end <= start {
            return Err("Select some audio first, or stretch the whole file.".into());
        }
        let len = end - start;
        let ratio = frames as f64 / len as f64;
        if !(0.25..=4.0).contains(&ratio) {
            return Err("Stretch works from a quarter to four times the length.".into());
        }
        let (audio, n) = render_n(&session, start as i64, end as i64);
        let out = by_pairs(&audio, n, |st| Ok(stretch_to(st, frames as usize, rate)))?;
        let media = write_n(&dir, "stretch", &out, n, 0, frames as usize, rate)?;
        Ok(SpectralResult { media, start, end })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Reverse (play backwards) or invert (flip polarity) audio of `n` interleaved channels.
fn reverse_or_invert(audio: &[f32], n: usize, mode: &str) -> Result<Vec<f32>, String> {
    let n = n.max(1);
    match mode {
        "invert" => Ok(audio.iter().map(|v| -v).collect()),
        "reverse" => Ok(audio.chunks_exact(n).rev().flatten().copied().collect()),
        _ => Err("Unknown edit.".into()),
    }
}

/// Waveform editor Reverse and Invert on `start..end` (mode "reverse" or
/// "invert"). The interface swaps the result in; Undo takes it back.
#[tauri::command]
pub async fn wave_reverse_invert(
    app: AppHandle,
    wave: State<'_, OpenWave>,
    start: u64,
    end: u64,
    mode: String,
) -> Result<SpectralResult, String> {
    let session = snapshot(&wave)?;
    let dir = crate::commands::cache::cache_root(&app)?.join("wave-fx");
    spawn_blocking(move || -> Result<SpectralResult, String> {
        let rate = session.sample_rate;
        let end = end.min(mixdown::session_end(&session));
        if end <= start {
            return Err("Select some audio first, or use the whole file.".into());
        }
        let (audio, n) = render_n(&session, start as i64, end as i64);
        let out = reverse_or_invert(&audio, n, &mode)?;
        let frames = (end - start) as usize;
        let media = write_n(&dir, &mode, &out, n, 0, frames, rate)?;
        Ok(SpectralResult { media, start, end })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Where one output channel of a one-channel edit comes from: the file
/// before the edit or after it, and which of its channels (from 0).
#[derive(Debug, Clone, Copy, serde::Deserialize)]
pub struct ChannelSource {
    pub after: bool,
    pub channel: u8,
}

/// Builds `take.len()` channels from two renders of `n` interleaved
/// channels, each output channel as `take` says.
pub fn merge_channels(before: &[f32], after: &[f32], n: usize, take: &[ChannelSource]) -> Vec<f32> {
    let n = n.max(1);
    let frames = before.len() / n;
    let mut out = Vec::with_capacity(frames * take.len());
    for i in 0..frames {
        for t in take {
            let src = if t.after { after } else { before };
            let c = usize::from(t.channel).min(n - 1);
            out.push(src.get(i * n + c).copied().unwrap_or(0.0));
        }
    }
    out
}

/// An edit limited to one channel of a stereo file in the Waveform
/// editor: `op` is applied to a copy, `start..end` of the copy and of the
/// file as it is are rendered, and each output channel is taken from one
/// of them as `take` says. The interface swaps the result in, so the
/// other channel keeps exactly what it had.
#[tauri::command]
pub async fn wave_channel_edit(
    app: AppHandle,
    wave: State<'_, OpenWave>,
    op: crate::session::ops::Op,
    start: u64,
    end: u64,
    take: Vec<ChannelSource>,
) -> Result<SpectralResult, String> {
    let before = snapshot(&wave)?;
    let dir = crate::commands::cache::cache_root(&app)?.join("wave-fx");
    spawn_blocking(move || -> Result<SpectralResult, String> {
        let end = end.min(mixdown::session_end(&before));
        if end <= start {
            return Err("Select some audio first.".into());
        }
        let mut after = before.clone();
        crate::session::ops::apply(&mut after, &op).map_err(|e| e.to_string())?;
        let (a, n) = render_n(&before, start as i64, end as i64);
        let (b, _) = render_n(&after, start as i64, end as i64);
        if take.len() != n {
            return Err("The channel buttons don't match the file. Open it again.".into());
        }
        let out = merge_channels(&a, &b, n, &take);
        let frames = (end - start) as usize;
        let media = write_n(&dir, "channel-edit", &out, n, 0, frames, before.sample_rate)?;
        Ok(SpectralResult { media, start, end })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::model::EffectSlot;

    fn gain_chain(db: f64) -> EffectChain {
        let mut c = EffectChain::default();
        c.slots.push(EffectSlot {
            id: crate::session::model::new_id(),
            effect: "altaudio.gain".into(),
            enabled: true,
            params: [("gain".to_string(), db)].into_iter().collect(),
            preset: None,
            sidechain: None,
            plugin_state: None,
            data: None,
            position: None,
            routing: None,
        });
        c
    }

    #[test]
    fn multichannel_audio_runs_a_pair_at_a_time() {
        // Three channels, two frames: pairs 1-2 and 3 (with silence beside it).
        let audio = [1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0];
        let mut calls = 0;
        let out = by_pairs(&audio, 3, |st| {
            calls += 1;
            Ok(st.iter().map(|v| v * 10.0).collect())
        })
        .unwrap();
        assert_eq!(calls, 2, "a fresh run for each pair");
        assert_eq!(out, vec![10.0, 20.0, 30.0, 40.0, 50.0, 60.0]);
        // Mono goes through both sides and back.
        let mono = by_pairs(&[0.5, 0.25], 1, |st| Ok(st.to_vec())).unwrap();
        assert_eq!(mono, vec![0.5, 0.25]);
    }

    #[test]
    fn merges_channels_from_before_and_after() {
        let before = vec![1.0f32, 2.0, 3.0, 4.0];
        let after = vec![10.0f32, 20.0, 30.0, 40.0];
        let left_edit = [
            ChannelSource {
                after: true,
                channel: 0,
            },
            ChannelSource {
                after: false,
                channel: 1,
            },
        ];
        assert_eq!(
            merge_channels(&before, &after, 2, &left_edit),
            vec![10.0, 2.0, 30.0, 4.0]
        );
        // Left copied onto the right.
        let copy = [
            ChannelSource {
                after: false,
                channel: 0,
            },
            ChannelSource {
                after: true,
                channel: 0,
            },
        ];
        assert_eq!(
            merge_channels(&before, &after, 2, &copy),
            vec![1.0, 10.0, 3.0, 30.0]
        );
    }

    #[test]
    fn reverse_and_invert() {
        let audio = vec![0.1f32, -0.1, 0.2, -0.2, 0.3, -0.3];
        let r = reverse_or_invert(&audio, 2, "reverse").unwrap();
        assert_eq!(r, vec![0.3, -0.3, 0.2, -0.2, 0.1, -0.1]);
        let i = reverse_or_invert(&audio, 2, "invert").unwrap();
        assert_eq!(i, vec![-0.1, 0.1, -0.2, 0.2, -0.3, 0.3]);
        assert!(reverse_or_invert(&audio, 2, "x").is_err());
    }

    #[test]
    fn stretch_to_hits_the_length_and_keeps_the_pitch() {
        let rate = 48_000;
        let tone: Vec<f32> = (0..rate as usize)
            .flat_map(|i| {
                let v = (std::f32::consts::TAU * 220.0 * i as f32 / rate as f32).sin() * 0.5;
                [v, v]
            })
            .collect();
        let out = stretch_to(&tone, 45_600, rate);
        assert_eq!(out.len(), 45_600 * 2);
        let mid = &out[10_000 * 2..30_000 * 2];
        let crossings = mid
            .chunks(2)
            .collect::<Vec<_>>()
            .windows(2)
            .filter(|w| (w[0][0] < 0.0) != (w[1][0] < 0.0))
            .count();
        // 20,000 frames of 220 Hz cross zero about 183 times, stretched or not.
        assert!((170..=196).contains(&crossings), "{crossings}");
    }

    #[test]
    fn processes_the_range_and_blends_at_the_edges() {
        let mut c = ChainProcessor::new(&gain_chain(-6.0206), 48_000).unwrap();
        let audio = vec![1.0f32; 300 * 2];
        let out = process_range(&mut c, &audio, 100, 100, 10);
        assert_eq!(out.len(), 200);
        assert!((out[0] - 1.0).abs() < 1e-6, "starts as it was");
        assert!((out[100] - 0.5).abs() < 1e-3, "middle is processed");
        assert!((out[199] - 1.0).abs() < 0.06, "ends close to as it was");
    }

    #[test]
    fn look_ahead_is_trimmed_so_audio_stays_in_place() {
        let mut lim = EffectChain::default();
        lim.slots.push(EffectSlot {
            id: crate::session::model::new_id(),
            effect: "altaudio.limiter".into(),
            enabled: true,
            params: [("ceiling".to_string(), 0.0), ("lookahead".to_string(), 5.0)]
                .into_iter()
                .collect(),
            preset: None,
            sidechain: None,
            plugin_state: None,
            data: None,
            position: None,
            routing: None,
        });
        let mut c = ChainProcessor::new(&lim, 48_000).unwrap();
        let lat = c.latency();
        assert!(lat > 0);
        // A click 50 frames into the range.
        let warm = 1_000;
        let frames = 2_000;
        let mut audio = vec![0f32; (warm + frames + lat) * 2];
        audio[(warm + 50) * 2] = 0.5;
        let out = process_range(&mut c, &audio, warm, frames, 0);
        let at = out.iter().step_by(2).position(|v| v.abs() > 0.4);
        assert_eq!(at, Some(50));
    }
}
