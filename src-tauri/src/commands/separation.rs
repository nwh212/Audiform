// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

//! Audio Separation commands: what is installed, downloading and removing
//! the engine and models, and separating a file.
//!
//! Progress goes out as `separation-progress` events: `{ id, stage,
//! done, total }`, where `stage` is "download" or "separate" and `id` is
//! the model (or "engine").

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Duration;

use serde::Serialize;
use tauri::async_runtime::spawn_blocking;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::codecs::{probe, AudioReader};
use crate::export::mixdown::unique_path;
use crate::separation::{self, download, EngineLine, ModelEntry};

/// Set to stop the running download or separation.
#[derive(Default)]
pub struct SeparationCancel(pub Arc<AtomicBool>);

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelStatus {
    #[serde(flatten)]
    pub model: ModelEntry,
    pub installed: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeparationStatus {
    pub engine_installed: bool,
    /// Size of the installed engine on disk, in bytes.
    pub engine_bytes: Option<u64>,
    /// None when no engine is published for this computer.
    pub engine_available: bool,
    pub models: Vec<ModelStatus>,
    pub folder: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
struct Progress<'a> {
    id: &'a str,
    stage: &'a str,
    done: f64,
    total: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeparationResult {
    /// The result files with their names, in the model's order (a target
    /// and the rest, or one per stem).
    pub outputs: Vec<SeparatedFile>,
}

#[derive(Debug, Serialize)]
pub struct SeparatedFile {
    pub name: String,
    pub path: PathBuf,
}

fn data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path().app_data_dir().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn separation_status(app: AppHandle) -> Result<SeparationStatus, String> {
    let dir = data_dir(&app)?;
    let models = separation::catalog()
        .into_iter()
        .map(|m| {
            let installed = separation::model_path(&dir, &m).is_file();
            ModelStatus {
                model: m,
                installed,
            }
        })
        .collect();
    Ok(SeparationStatus {
        engine_installed: separation::engine_path(&dir).is_file(),
        engine_bytes: std::fs::metadata(separation::engine_path(&dir))
            .ok()
            .map(|m| m.len()),
        engine_available: separation::engine_asset().is_some(),
        models,
        folder: separation::root(&dir),
    })
}

#[tauri::command]
pub fn separation_cancel(cancel: State<'_, SeparationCancel>) {
    cancel.0.store(true, Ordering::Relaxed);
}

/// Downloads the engine (`id` "engine") or a model, and checks it.
#[tauri::command]
pub async fn separation_download(
    app: AppHandle,
    cancel: State<'_, SeparationCancel>,
    id: String,
) -> Result<(), String> {
    let dir = data_dir(&app)?;
    let flag = cancel.0.clone();
    flag.store(false, Ordering::Relaxed);
    spawn_blocking(move || {
        if id == "engine" {
            install_engine(&app, &dir, &flag)
        } else {
            let m = separation::model(&id).ok_or("Unknown model.")?;
            let dest = separation::model_path(&dir, &m);
            let total = m.bytes as f64;
            download::fetch(m.url, &dest, &flag, |done| {
                let _ = app.emit(
                    "separation-progress",
                    Progress {
                        id: &id,
                        stage: "download",
                        done: done as f64,
                        total,
                    },
                );
            })?;
            download::verify(&dest, m.sha256)
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

fn install_engine(app: &AppHandle, dir: &Path, cancel: &AtomicBool) -> Result<(), String> {
    let asset = separation::engine_asset()
        .ok_or("Audio Separation is not available for this computer yet.")?;
    let url = separation::engine_url(asset);
    let engine_dir = separation::root(dir).join("engine");
    let zip_path = engine_dir.join(asset);
    let sum_path = engine_dir.join(format!("{asset}.sha256"));
    download::fetch(&format!("{url}.sha256"), &sum_path, cancel, |_| {})?;
    let expected = std::fs::read_to_string(&sum_path).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&sum_path);
    let expected = expected
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_string();
    download::fetch(&url, &zip_path, cancel, |done| {
        // The engine's size is not known ahead; show the bytes so far.
        let _ = app.emit(
            "separation-progress",
            Progress {
                id: "engine",
                stage: "download",
                done: done as f64,
                total: 0.0,
            },
        );
    })?;
    download::verify(&zip_path, &expected)?;
    let result = unpack_engine(&zip_path, &separation::engine_path(dir));
    if result.is_ok() {
        // What was installed, so "check for update" can tell when it changes.
        let _ = std::fs::write(engine_dir.join("engine.sha256"), &expected);
    }
    let _ = std::fs::remove_file(&zip_path);
    result
}

fn unpack_engine(zip_path: &Path, dest: &Path) -> Result<(), String> {
    let file = std::fs::File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| format!("The engine download is damaged: {e}"))?;
    let name = dest
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_string();
    let mut entry = archive
        .by_name(&name)
        .map_err(|_| "The engine download does not hold the engine.".to_string())?;
    let tmp = dest.with_extension("new");
    {
        let mut out = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
        std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
    }
    std::fs::rename(&tmp, dest).map_err(|e| e.to_string())
}

/// Whether the published engine differs from the installed one. Fetches only
/// the small checksum file, and only when the user asks. An engine installed
/// before Audiform kept a record of it counts as out of date.
#[tauri::command]
pub async fn separation_check_engine(
    app: AppHandle,
    cancel: State<'_, SeparationCancel>,
) -> Result<bool, String> {
    let dir = data_dir(&app)?;
    let flag = cancel.0.clone();
    flag.store(false, Ordering::Relaxed);
    spawn_blocking(move || {
        let asset = separation::engine_asset()
            .ok_or("Audio Separation is not available for this computer yet.")?;
        let engine_dir = separation::root(&dir).join("engine");
        let latest_path = engine_dir.join("latest.sha256");
        download::fetch(
            &format!("{}.sha256", separation::engine_url(asset)),
            &latest_path,
            &flag,
            |_| {},
        )?;
        let latest = std::fs::read_to_string(&latest_path).map_err(|e| e.to_string())?;
        let _ = std::fs::remove_file(&latest_path);
        let latest = latest
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        let mine = std::fs::read_to_string(engine_dir.join("engine.sha256")).unwrap_or_default();
        Ok(mine
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase()
            != latest)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Removes a downloaded model, or the engine with `id` "engine".
#[tauri::command]
pub fn separation_remove(app: AppHandle, id: String) -> Result<(), String> {
    let dir = data_dir(&app)?;
    let path = if id == "engine" {
        separation::root(&dir).join("engine").join(
            separation::engine_path(&dir)
                .file_name()
                .unwrap_or_default(),
        )
    } else {
        separation::model_path(&dir, &separation::model(&id).ok_or("Unknown model.")?)
    };
    if id == "engine" {
        let _ = std::fs::remove_file(path.with_file_name("engine.sha256"));
    }
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("Could not remove {}: {e}", path.display())),
    }
}

/// Separates `source` with `model` into two WAVs in `folder`, at the
/// source's own sample rate and channel count so they line up with it.
/// An error starting with "GPU:" means the graphics card could not be
/// used; the caller can offer the processor instead.
#[tauri::command]
pub async fn separation_run(
    app: AppHandle,
    cancel: State<'_, SeparationCancel>,
    source: PathBuf,
    model: String,
    gpu: bool,
    overlap: Option<u32>,
    folder: PathBuf,
) -> Result<SeparationResult, String> {
    let dir = data_dir(&app)?;
    let flag = cancel.0.clone();
    flag.store(false, Ordering::Relaxed);
    spawn_blocking(move || {
        let m = separation::model(&model).ok_or("Unknown model.")?;
        let engine = separation::engine_path(&dir);
        if !engine.is_file() {
            return Err("The separation engine is not installed.".into());
        }
        let checkpoint = separation::model_path(&dir, &m);
        if !checkpoint.is_file() {
            return Err(format!("{} is not downloaded.", m.name));
        }
        let work = separation::root(&dir)
            .join("work")
            .join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
        let result = separate_in(
            &app,
            &engine,
            &m,
            &checkpoint,
            &source,
            &folder,
            &work,
            (gpu, overlap),
            &flag,
        );
        // The work folder only ever holds the files named here.
        if let Ok(files) = std::fs::read_dir(&work) {
            for f in files.flatten() {
                let _ = std::fs::remove_file(f.path());
            }
        }
        let _ = std::fs::remove_dir(&work);
        result
    })
    .await
    .map_err(|e| e.to_string())?
}

#[allow(clippy::too_many_arguments)]
fn separate_in(
    app: &AppHandle,
    engine: &Path,
    m: &ModelEntry,
    checkpoint: &Path,
    source: &Path,
    folder: &Path,
    work: &Path,
    (gpu, overlap): (bool, Option<u32>),
    cancel: &AtomicBool,
) -> Result<SeparationResult, String> {
    let info = probe(source).map_err(|e| e.to_string())?;
    let model_rate = (m.spec)()["sample_rate"].as_u64().unwrap_or(44_100) as u32;
    // Stereo models take mono as is; more than two channels are folded to stereo.
    let channels = usize::from(info.channels.clamp(1, 2));
    let input = work.join("input.wav");
    write_resampled(source, &input, channels, model_rate, None)?;
    let names = m.output_names();
    let raw: Vec<PathBuf> = (0..names.len())
        .map(|k| work.join(format!("out{k}.wav")))
        .collect();
    let job = separation::job(m, checkpoint, &input, &raw, gpu, overlap);
    let job_path = work.join("job.json");
    std::fs::write(
        &job_path,
        serde_json::to_vec(&job).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    run_engine(app, engine, &job_path, m.id, cancel)?;

    std::fs::create_dir_all(folder)
        .map_err(|e| format!("Could not create {}: {e}", folder.display()))?;
    let stem = source
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Audio");
    let mut outputs = Vec::new();
    for (name, wav) in names.iter().zip(&raw) {
        let path = unique_path(folder, &format!("{stem} ({name})"), "wav");
        // Back to the source's own rate and exact length, so clips line up.
        write_resampled(wav, &path, channels, info.sample_rate, Some(info.length))?;
        outputs.push(SeparatedFile {
            name: (*name).into(),
            path,
        });
    }
    Ok(SeparationResult { outputs })
}

fn run_engine(
    app: &AppHandle,
    engine: &Path,
    job: &Path,
    id: &str,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let mut cmd = Command::new(engine);
    cmd.arg(job)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Could not start the separation engine: {e}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or("The separation engine gave no output.")?;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if tx.send(separation::parse_line(&line)).is_err() {
                break;
            }
        }
    });
    let mut outcome: Option<Result<(), String>> = None;
    // Once the engine has said it is done, it gets a few seconds to exit
    // on its own, then it is stopped: it never lingers after a job.
    let mut finished_at: Option<std::time::Instant> = None;
    loop {
        if finished_at.is_some_and(|t| t.elapsed() > Duration::from_secs(5)) {
            let _ = child.kill();
        }
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Separation cancelled.".into());
        }
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(EngineLine::Progress(p)) => {
                let _ = app.emit(
                    "separation-progress",
                    Progress {
                        id,
                        stage: "separate",
                        done: f64::from(p),
                        total: 1.0,
                    },
                );
            }
            Ok(EngineLine::Done) => {
                outcome = Some(Ok(()));
                finished_at = Some(std::time::Instant::now());
            }
            Ok(EngineLine::Error { gpu, message }) => {
                outcome = Some(Err(if gpu {
                    format!("GPU: {message}")
                } else {
                    message
                }));
                finished_at = Some(std::time::Instant::now());
            }
            Ok(EngineLine::Other) | Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                let status = child.wait().map_err(|e| e.to_string())?;
                return match outcome {
                    Some(r) => r,
                    None if status.success() => {
                        Err("The separation engine stopped without finishing.".into())
                    }
                    None => Err(format!(
                        "The separation engine stopped unexpectedly ({status})."
                    )),
                };
            }
        }
    }
}

/// Decodes `src` and writes it as a 32-bit float WAV at `rate` with
/// `channels`, cut or padded to `frames` when given.
fn write_resampled(
    src: &Path,
    dest: &Path,
    channels: usize,
    rate: u32,
    frames: Option<u64>,
) -> Result<(), String> {
    let mut reader = AudioReader::open(src, channels, Some(rate)).map_err(|e| e.to_string())?;
    let spec = hound::WavSpec {
        channels: channels as u16,
        sample_rate: rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut w = hound::WavWriter::create(dest, spec)
        .map_err(|e| format!("Could not write {}: {e}", dest.display()))?;
    let limit = frames.map(|f| f * channels as u64);
    let mut written = 0u64;
    let mut buf = Vec::new();
    while reader.read_into(&mut buf).map_err(|e| e.to_string())? {
        for &s in &buf {
            if limit.is_some_and(|l| written >= l) {
                break;
            }
            w.write_sample(s).map_err(|e| e.to_string())?;
            written += 1;
        }
        buf.clear();
    }
    if let Some(l) = limit {
        while written < l {
            w.write_sample(0.0f32).map_err(|e| e.to_string())?;
            written += 1;
        }
    }
    w.finalize().map_err(|e| e.to_string())
}
