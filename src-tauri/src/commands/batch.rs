// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Batch Convert: converts a list of files one after another on a worker
//! thread, reporting each file's progress as `batch-progress` events. A
//! file that fails is reported and the rest carry on.
//!
//! Tags: with `copy_tags`, each file's tags (and a WAV's markers, when
//! the output is WAV too) carry over; a job's own `tags` go on top.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::async_runtime::spawn_blocking;
use tauri::{AppHandle, Emitter, State};

use crate::codecs::cue::{read_cues, write_cues, Cue};
use crate::export::convert::{convert_with, Converted, Shaping};
use crate::export::encode::OutputFormat;
use crate::export::tags::{read_tags, write_tags, Tags};
use crate::session::model::EffectChain;

/// Set to stop the running batch after the current file.
#[derive(Default)]
pub struct BatchCancel(pub Arc<AtomicBool>);

#[derive(Debug, Clone, Deserialize)]
pub struct BatchJob {
    pub id: String,
    pub source: PathBuf,
    pub dest: PathBuf,
    /// Tags to write, over any copied from the source.
    #[serde(default)]
    pub tags: Option<Tags>,
}

fn is_wav(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("wav") || e.eq_ignore_ascii_case("wave"))
}

/// What carries over from a source file, read before it converts (with
/// Replace on, the output can take the source's place).
struct Carry {
    tags: Tags,
    cues: Vec<Cue>,
    rate: Option<u32>,
}

fn wav_rate(p: &Path) -> Option<u32> {
    hound::WavReader::open(p).ok().map(|r| r.spec().sample_rate)
}

fn carry(job: &BatchJob, copy: bool) -> Carry {
    if !copy {
        return Carry {
            tags: Tags::default(),
            cues: Vec::new(),
            rate: None,
        };
    }
    let wav = is_wav(&job.source);
    Carry {
        tags: read_tags(&job.source),
        cues: if wav {
            read_cues(&job.source)
        } else {
            Vec::new()
        },
        rate: if wav { wav_rate(&job.source) } else { None },
    }
}

/// Writes the tags and copied markers into a converted file. Says why
/// when they could not go in; the audio is fine either way.
fn finish_tags(job: &BatchJob, carried: Carry) -> Result<(), String> {
    let tags = match &job.tags {
        Some(t) => carried.tags.overlay(t),
        None => carried.tags,
    };
    let wrote = write_tags(&job.dest, &tags).map_err(|e| format!("Tags not written: {e}"))?;
    if !wrote && !tags.is_empty() {
        let ext = job
            .dest
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("this format");
        return Err(format!(
            "Converted. Tags are not written to {ext} files yet."
        ));
    }
    if !carried.cues.is_empty() && is_wav(&job.dest) {
        let (Some(from), Some(to)) = (carried.rate, wav_rate(&job.dest)) else {
            return Ok(());
        };
        let scale = |n: u64| (n as f64 * f64::from(to) / f64::from(from)).round() as u64;
        let moved: Vec<Cue> = carried
            .cues
            .into_iter()
            .map(|c| Cue {
                position: scale(c.position),
                length: scale(c.length),
                ..c
            })
            .collect();
        write_cues(&job.dest, &moved).map_err(|e| format!("Markers not copied: {e}"))?;
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct BatchProgress {
    id: String,
    /// "running", "done", "failed" or "stopped".
    status: &'static str,
    /// What is happening to the file now, for the Stage column.
    stage: &'static str,
    /// 0 to 1 while running.
    progress: f64,
    error: Option<String>,
    dest: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchSummary {
    pub done: usize,
    pub failed: usize,
    pub stopped: bool,
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn batch_convert(
    app: AppHandle,
    cancel: State<'_, BatchCancel>,
    jobs: Vec<BatchJob>,
    format: OutputFormat,
    overwrite: bool,
    copy_tags: Option<bool>,
    process: Option<EffectChain>,
    shape: Option<Shaping>,
) -> Result<BatchSummary, String> {
    let shape = shape.unwrap_or_default();
    let copy = copy_tags.unwrap_or(false);
    let flag = cancel.0.clone();
    flag.store(false, Ordering::Relaxed);
    spawn_blocking(move || {
        let mut summary = BatchSummary {
            done: 0,
            failed: 0,
            stopped: false,
        };
        for job in jobs {
            if flag.load(Ordering::Relaxed) {
                summary.stopped = true;
                break;
            }
            let working = if process.is_some() || shape != Shaping::default() {
                "Processing"
            } else {
                "Converting"
            };
            let send = |status: &'static str, progress: f64, error: Option<String>| {
                let stage = match status {
                    "running" => working,
                    "done" => "Saved",
                    "failed" => "Not saved",
                    _ => "",
                };
                let _ = app.emit(
                    "batch-progress",
                    BatchProgress {
                        id: job.id.clone(),
                        status,
                        stage,
                        progress,
                        error,
                        dest: job.dest.clone(),
                    },
                );
            };
            send("running", 0.0, None);
            let carried = carry(&job, copy);
            let mut last = 0.0;
            let r = convert_with(
                &job.source,
                &job.dest,
                &format,
                overwrite,
                process.as_ref(),
                &shape,
                &flag,
                |d, t| {
                    let p = if t > 0 { d as f64 / t as f64 } else { 0.0 };
                    if p - last >= 0.02 {
                        last = p;
                        send("running", p, None);
                    }
                },
            );
            match r {
                Ok(Converted::Done { .. }) => {
                    summary.done += 1;
                    // A tag problem is a note on a finished file, not a failure.
                    send("done", 1.0, finish_tags(&job, carried).err());
                }
                Ok(Converted::Cancelled) => {
                    summary.stopped = true;
                    send("stopped", 0.0, None);
                    break;
                }
                Err(e) => {
                    summary.failed += 1;
                    send("failed", 0.0, Some(e));
                }
            }
        }
        summary
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn batch_cancel(cancel: State<'_, BatchCancel>) {
    cancel.0.store(true, Ordering::Relaxed);
}
