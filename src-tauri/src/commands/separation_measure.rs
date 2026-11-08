// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform.

//! Measure Scores: runs the downloaded separation models over a test set
//! on this computer and writes what they scored (see
//! `separation::score`). Each run gets a folder under the separation
//! folder's `scores` with:
//!
//! - `scores.json`: every number, per model, per result, per song, with
//!   the app, engine, device and test set it was measured with;
//! - `scores.txt`: the same as a table to read;
//! - `run.log`: each step with its time, and anything that went wrong.
//!
//! Both score files are written again after every song, so a run that is
//! cancelled or stops still leaves what it measured.
//!
//! Progress goes out as `separation-measure` events: `{ done, total, line }`.

use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use serde::Serialize;
use tauri::async_runtime::spawn_blocking;
use tauri::{AppHandle, Emitter, State};

use super::separation::{data_dir, engine_version, run_engine, write_resampled, SeparationCancel};
use crate::codecs::AudioReader;
use crate::separation::score::{self, Made, Plan, Reference, Song, Stats, Test, RATE};
use crate::separation::{self, ModelEntry};

/// How each model would be measured, for the dialog.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeasurePlan {
    pub id: String,
    pub name: String,
    pub category: String,
    pub installed: bool,
    /// What it is fed, when it can be scored.
    pub test: Option<String>,
    /// Why it cannot be scored.
    pub reason: Option<String>,
}

#[tauri::command]
pub fn separation_measure_plan(app: AppHandle) -> Result<Vec<MeasurePlan>, String> {
    let dir = data_dir(&app)?;
    Ok(separation::catalog()
        .into_iter()
        .map(|m| {
            let p = score::plan(&m);
            MeasurePlan {
                id: m.id.into(),
                name: m.name.into(),
                category: m.category.into(),
                installed: separation::model_path(&dir, &m).is_file(),
                test: p.as_ref().ok().map(|p| p.test.describe()),
                reason: p.err(),
            }
        })
        .collect())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeasureResult {
    pub folder: PathBuf,
    pub json: PathBuf,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
struct Event<'a> {
    done: usize,
    total: usize,
    line: &'a str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SongScore {
    song: String,
    /// The model's result against the reference.
    sdr: Option<f64>,
    /// Doing nothing (the input itself) against the reference.
    input_sdr: Option<f64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CheckReport {
    output: String,
    reference: Reference,
    reference_name: &'static str,
    stats: Option<Stats>,
    input_stats: Option<Stats>,
    songs: Vec<SongScore>,
}

impl CheckReport {
    fn update(&mut self) {
        let got: Vec<f64> = self.songs.iter().filter_map(|s| s.sdr).collect();
        let base: Vec<f64> = self.songs.iter().filter_map(|s| s.input_sdr).collect();
        self.stats = score::stats(&got);
        self.input_stats = score::stats(&base);
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ModelReport {
    id: String,
    name: String,
    category: String,
    author: String,
    /// The pinned checksum of the weights measured.
    sha256: String,
    published_sdr: Option<f32>,
    published_note: String,
    published_stems: Vec<(String, f32)>,
    /// "measured", "not measurable", "not downloaded", "not picked", "failed".
    status: String,
    reason: Option<String>,
    test: Option<Test>,
    test_description: Option<String>,
    caveat: Option<String>,
    /// Seconds the engine took, all songs together.
    engine_seconds: f64,
    checks: Vec<CheckReport>,
    errors: Vec<String>,
    #[serde(skip)]
    plan: Option<Plan>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TestSet {
    name: &'static str,
    folder: PathBuf,
    songs_found: usize,
    songs_used: Vec<String>,
    seconds_per_song: Option<u32>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Report {
    format: &'static str,
    version: u32,
    app_version: String,
    engine_version: String,
    os: &'static str,
    arch: &'static str,
    device: &'static str,
    overlap: Option<u32>,
    metric: &'static str,
    test_set: TestSet,
    started: String,
    finished: Option<String>,
    /// "running", "done", "cancelled" or "failed".
    status: String,
    error: Option<String>,
    models: Vec<ModelReport>,
}

/// The run's folder and log.
struct Run {
    folder: PathBuf,
    log: std::fs::File,
    started: Instant,
}

impl Run {
    fn say(&mut self, app: &AppHandle, done: usize, total: usize, line: &str) {
        let t = self.started.elapsed().as_secs();
        let _ = writeln!(
            self.log,
            "[{:02}:{:02}:{:02}] {line}",
            t / 3600,
            t / 60 % 60,
            t % 60
        );
        let _ = self.log.flush();
        let _ = app.emit("separation-measure", Event { done, total, line });
    }

    fn save(&self, report: &Report) -> Result<PathBuf, String> {
        let json = self.folder.join("scores.json");
        let text = serde_json::to_vec_pretty(report).map_err(|e| e.to_string())?;
        std::fs::write(&json, text)
            .map_err(|e| format!("Could not write {}: {e}", json.display()))?;
        let _ = std::fs::write(self.folder.join("scores.txt"), summary(report));
        Ok(json)
    }
}

fn fmt(v: Option<f64>) -> String {
    v.map_or("-".into(), |v| format!("{v:.2}"))
}

/// The report as a table to read.
fn summary(r: &Report) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "Audiform separation scores ({})", r.status);
    let _ = writeln!(
        s,
        "Audiform {}, engine {}, {} {}, on the {}, overlap {}",
        r.app_version,
        r.engine_version,
        r.os,
        r.arch,
        r.device,
        r.overlap
            .map_or("as each model sets".into(), |o| o.to_string())
    );
    let _ = writeln!(
        s,
        "Test set: {} at {}, {} of {} songs{}",
        r.test_set.name,
        r.test_set.folder.display(),
        r.test_set.songs_used.len(),
        r.test_set.songs_found,
        r.test_set
            .seconds_per_song
            .map_or(String::new(), |n| format!(", first {n} s of each"))
    );
    let _ = writeln!(
        s,
        "Started {}, finished {}",
        r.started,
        r.finished.as_deref().unwrap_or("-")
    );
    let _ = writeln!(s, "Score: {}", r.metric);
    if let Some(e) = &r.error {
        let _ = writeln!(s, "Stopped: {e}");
    }
    let _ = writeln!(s);
    let _ = writeln!(
        s,
        "{:<48} {:<28} {:<30} {:>7} {:>7} {:>7} {:>7} {:>9} {:>5}",
        "Model", "Result", "Against", "Mean", "Median", "Min", "Max", "Do-nothing", "Songs"
    );
    for m in r
        .models
        .iter()
        .filter(|m| m.status == "measured" || m.status == "failed")
    {
        for c in &m.checks {
            let st = c.stats;
            let _ = writeln!(
                s,
                "{:<48} {:<28} {:<30} {:>7} {:>7} {:>7} {:>7} {:>9} {:>5}",
                m.name.chars().take(48).collect::<String>(),
                c.output.chars().take(28).collect::<String>(),
                c.reference_name,
                fmt(st.map(|x| x.mean)),
                fmt(st.map(|x| x.median)),
                fmt(st.map(|x| x.min)),
                fmt(st.map(|x| x.max)),
                fmt(c.input_stats.map(|x| x.mean)),
                st.map_or(0, |x| x.count)
            );
        }
        let published = m.published_sdr.map_or("none".into(), |v| format!("{v}"));
        let _ = writeln!(s, "    published: {published} ({})", m.published_note);
        if let Some(c) = &m.caveat {
            let _ = writeln!(s, "    note: {c}");
        }
        for e in m.errors.iter().take(5) {
            let _ = writeln!(s, "    error: {e}");
        }
    }
    let _ = writeln!(s);
    let _ = writeln!(s, "Not measured:");
    for m in r
        .models
        .iter()
        .filter(|m| m.status != "measured" && m.status != "failed")
    {
        let _ = writeln!(
            s,
            "  {} ({}): {}",
            m.name,
            m.status,
            m.reason.as_deref().unwrap_or("")
        );
    }
    s
}

/// Reads a file as interleaved stereo at the test rate.
fn load(path: &Path) -> Result<Vec<f32>, String> {
    let mut reader = AudioReader::open(path, 2, Some(RATE))
        .map_err(|e| format!("Could not read {}: {e}", path.display()))?;
    let mut all = Vec::new();
    let mut buf = Vec::new();
    while reader.read_into(&mut buf).map_err(|e| e.to_string())? {
        all.extend_from_slice(&buf);
        buf.clear();
    }
    Ok(all)
}

fn write_wav(path: &Path, samples: &[f32]) -> Result<(), String> {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: RATE,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut w = hound::WavWriter::create(path, spec)
        .map_err(|e| format!("Could not write {}: {e}", path.display()))?;
    for s in samples {
        w.write_sample(*s).map_err(|e| e.to_string())?;
    }
    w.finalize().map_err(|e| e.to_string())
}

fn clear(dir: &Path) {
    if let Ok(files) = std::fs::read_dir(dir) {
        for f in files.flatten() {
            let _ = std::fs::remove_file(f.path());
        }
    }
}

/// One model on one song: its results, cut or padded to the song's length.
#[allow(clippy::too_many_arguments)]
fn separate(
    app: &AppHandle,
    engine: &Path,
    m: &ModelEntry,
    checkpoint: &Path,
    input: &[f32],
    work: &Path,
    (gpu, overlap): (bool, Option<u32>),
    cancel: &AtomicBool,
) -> Result<Vec<Vec<f32>>, String> {
    clear(work);
    let raw_in = work.join("song.wav");
    write_wav(&raw_in, input)?;
    let model_rate = (m.spec)()["sample_rate"].as_u64().unwrap_or(44_100) as u32;
    let input_path = if model_rate == RATE {
        raw_in
    } else {
        let p = work.join("input.wav");
        write_resampled(&raw_in, &p, 2, model_rate, None)?;
        p
    };
    let outs: Vec<PathBuf> = (0..m.output_names().len())
        .map(|k| work.join(format!("out{k}.wav")))
        .collect();
    let job = separation::job(m, checkpoint, &input_path, &outs, gpu, overlap);
    let job_path = work.join("job.json");
    std::fs::write(
        &job_path,
        serde_json::to_vec(&job).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    run_engine(app, engine, &job_path, m.id, cancel)?;
    outs.iter()
        .map(|p| {
            let mut v = load(p)?;
            v.resize(input.len(), 0.0);
            Ok(v)
        })
        .collect()
}

/// Measures the picked models (all downloaded ones when empty) on the
/// test songs in `folder`: the first `songs` of them, the first `seconds`
/// of each when given.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn separation_measure(
    app: AppHandle,
    cancel: State<'_, SeparationCancel>,
    folder: PathBuf,
    models: Vec<String>,
    songs: Option<usize>,
    seconds: Option<u32>,
    gpu: bool,
    overlap: Option<u32>,
) -> Result<MeasureResult, String> {
    let dir = data_dir(&app)?;
    let flag = cancel.0.clone();
    flag.store(false, Ordering::Relaxed);
    spawn_blocking(move || {
        measure(
            &app,
            &dir,
            &folder,
            &models,
            songs,
            seconds,
            (gpu, overlap),
            &flag,
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

#[allow(clippy::too_many_arguments)]
fn measure(
    app: &AppHandle,
    dir: &Path,
    folder: &Path,
    picked: &[String],
    songs: Option<usize>,
    seconds: Option<u32>,
    (gpu, overlap): (bool, Option<u32>),
    cancel: &AtomicBool,
) -> Result<MeasureResult, String> {
    let engine = separation::engine_path(dir);
    if !engine.is_file() {
        return Err("The separation engine is not installed.".into());
    }
    let version = engine_version(&engine);
    if !separation::engine_is_new_enough(&version) {
        return Err(
            "The separation engine is out of date. Press Update next to the engine first.".into(),
        );
    }
    let all_songs = score::songs(folder)?;
    let use_songs: Vec<PathBuf> = all_songs
        .iter()
        .take(songs.unwrap_or(usize::MAX).max(1))
        .cloned()
        .collect();

    let stamp = chrono::Local::now().format("%Y-%m-%d_%H%M%S").to_string();
    let out = separation::root(dir).join("scores").join(&stamp);
    let work = out.join("work");
    std::fs::create_dir_all(&work)
        .map_err(|e| format!("Could not create {}: {e}", work.display()))?;
    let log = std::fs::File::create(out.join("run.log")).map_err(|e| e.to_string())?;
    let mut run = Run {
        folder: out.clone(),
        log,
        started: Instant::now(),
    };

    let mut report = Report {
        format: "audiform-separation-scores",
        version: 1,
        app_version: app.package_info().version.to_string(),
        engine_version: version.clone(),
        os: std::env::consts::OS,
        arch: std::env::consts::ARCH,
        device: if gpu { "graphics card" } else { "processor" },
        overlap,
        metric: "SDR in dB of each song taken whole (reference energy over error energy, all channels), then averaged over the songs; higher is better. Do-nothing is the input itself scored the same way.",
        test_set: TestSet {
            name: "MUSDB18-HQ test",
            folder: folder.to_path_buf(),
            songs_found: all_songs.len(),
            songs_used: use_songs
                .iter()
                .map(|p| p.file_name().unwrap_or_default().to_string_lossy().into_owned())
                .collect(),
            seconds_per_song: seconds,
        },
        started: chrono::Local::now().to_rfc3339(),
        finished: None,
        status: "running".into(),
        error: None,
        models: Vec::new(),
    };

    let mut entries: Vec<ModelEntry> = Vec::new();
    for m in separation::catalog() {
        let installed = separation::model_path(dir, &m).is_file();
        let plan = score::plan(&m);
        let wanted = picked.is_empty() || picked.iter().any(|p| p == m.id);
        let (status, reason) = match (&plan, installed, wanted) {
            (Err(why), _, _) => ("not measurable", Some(why.clone())),
            (Ok(_), false, _) => (
                "not downloaded",
                Some("download it in Audio Separation to measure it".into()),
            ),
            (Ok(_), true, false) => ("not picked", None),
            (Ok(_), true, true) => ("measured", None),
        };
        let plan = plan.ok();
        report.models.push(ModelReport {
            id: m.id.into(),
            name: m.name.into(),
            category: m.category.into(),
            author: m.author.into(),
            sha256: m.sha256.into(),
            published_sdr: m.sdr,
            published_note: m.sdr_note.into(),
            published_stems: m.stems.iter().map(|(n, v)| ((*n).into(), *v)).collect(),
            status: status.into(),
            reason,
            test: plan.as_ref().map(|p| p.test),
            test_description: plan.as_ref().map(|p| p.test.describe()),
            caveat: plan.as_ref().and_then(|p| p.caveat.clone()),
            engine_seconds: 0.0,
            checks: plan
                .as_ref()
                .map(|p| {
                    p.checks
                        .iter()
                        .map(|c| CheckReport {
                            output: c.label.clone(),
                            reference: c.reference,
                            reference_name: c.reference.name(),
                            stats: None,
                            input_stats: None,
                            songs: Vec::new(),
                        })
                        .collect()
                })
                .unwrap_or_default(),
            errors: Vec::new(),
            plan,
        });
        entries.push(m);
    }
    let todo: Vec<usize> = (0..report.models.len())
        .filter(|&k| report.models[k].status == "measured")
        .collect();
    let total = todo.len() * use_songs.len();
    run.say(
        app,
        0,
        total,
        &format!(
            "Measuring {} models on {} songs ({}), engine {}, on the {}.",
            todo.len(),
            use_songs.len(),
            seconds.map_or("whole songs".into(), |s| format!("first {s} s")),
            version,
            report.device
        ),
    );
    if todo.is_empty() {
        report.status = "done".into();
        report.error = Some("No downloaded model can be measured.".into());
        report.finished = Some(chrono::Local::now().to_rfc3339());
        let json = run.save(&report)?;
        let _ = std::fs::remove_dir_all(&work);
        return Ok(MeasureResult {
            folder: out,
            json,
            status: report.status,
        });
    }
    let _ = run.save(&report);

    let mut done = 0usize;
    let mut stop: Option<(String, String)> = None;
    'songs: for (si, path) in use_songs.iter().enumerate() {
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let mut parts: [Vec<f32>; 5] = Default::default();
        let mut failed = None;
        for (k, part) in score::PARTS.iter().enumerate() {
            match load(&path.join(format!("{part}.wav"))) {
                Ok(v) => parts[k] = v,
                Err(e) => {
                    failed = Some(e);
                    break;
                }
            }
        }
        if let Some(e) = failed {
            run.say(
                app,
                done,
                total,
                &format!("Song {} ({name}) skipped: {e}", si + 1),
            );
            done += todo.len();
            continue;
        }
        let limit = seconds.map(|s| s as usize * RATE as usize * 2);
        let song = Song::new(name.clone(), parts, limit);
        run.say(
            app,
            done,
            total,
            &format!(
                "Song {} of {}: {name} ({:.0} s)",
                si + 1,
                use_songs.len(),
                song.mixture.len() as f64 / 2.0 / f64::from(RATE)
            ),
        );
        // Each made-up input once per song, shared by the models that use it.
        let mut made: Vec<(Test, Made)> = Vec::new();
        for &k in &todo {
            if cancel.load(Ordering::Relaxed) {
                stop = Some(("cancelled".into(), "Cancelled.".into()));
                break 'songs;
            }
            let Some(plan) = report.models[k].plan.clone() else {
                continue;
            };
            if !made.iter().any(|(t, _)| *t == plan.test) {
                made.push((plan.test, song.input(plan.test, si as u32 + 1)));
            }
            let Some((_, input)) = made.iter().find(|(t, _)| *t == plan.test) else {
                continue;
            };
            let m = &entries[k];
            let checkpoint = separation::model_path(dir, m);
            let t0 = Instant::now();
            let result = separate(
                app,
                &engine,
                m,
                &checkpoint,
                &input.input,
                &work,
                (gpu, overlap),
                cancel,
            );
            let took = t0.elapsed().as_secs_f64();
            done += 1;
            match result {
                Ok(outputs) => {
                    let mr = &mut report.models[k];
                    mr.engine_seconds += took;
                    let mut said = Vec::new();
                    for (c, cr) in plan.checks.iter().zip(mr.checks.iter_mut()) {
                        let reference = song.reference(c.reference, input);
                        let est = score::sum_outputs(&outputs, &c.outputs, reference.len());
                        let got = score::sdr(&reference, &est);
                        let base = if c.reference == Reference::Added {
                            None
                        } else {
                            score::sdr(&reference, &input.input)
                        };
                        cr.songs.push(SongScore {
                            song: name.clone(),
                            sdr: got,
                            input_sdr: base,
                        });
                        cr.update();
                        said.push(format!("{} {}", cr.output, fmt(got)));
                    }
                    let line = format!("  {} in {took:.1} s: {}", m.name, said.join(", "));
                    run.say(app, done, total, &line);
                }
                Err(e) => {
                    let gpu_failed = e.starts_with("GPU:");
                    let cancelled = cancel.load(Ordering::Relaxed);
                    let mr = &mut report.models[k];
                    if !cancelled {
                        mr.errors.push(format!("{name}: {e}"));
                    }
                    run.say(app, done, total, &format!("  {} failed: {e}", m.name));
                    if cancelled {
                        stop = Some(("cancelled".into(), "Cancelled.".into()));
                        break 'songs;
                    }
                    if gpu_failed {
                        stop = Some(("failed".into(), format!("The graphics card could not be used: {e}. Untick the graphics card to measure on the processor.")));
                        break 'songs;
                    }
                }
            }
        }
        let _ = run.save(&report);
    }

    for k in &todo {
        let mr = &mut report.models[*k];
        if mr.checks.iter().all(|c| c.stats.is_none()) && !mr.errors.is_empty() {
            mr.status = "failed".into();
        }
    }
    let (status, error) = stop.map_or(("done".to_string(), None), |(s, e)| (s, Some(e)));
    report.status = status;
    report.error = error;
    report.finished = Some(chrono::Local::now().to_rfc3339());
    let json = run.save(&report)?;
    let t = run.started.elapsed().as_secs();
    let line = format!(
        "Finished ({}) after {}h {:02}m. Results: {}",
        report.status,
        t / 3600,
        t / 60 % 60,
        out.display()
    );
    run.say(app, done, total, &line);
    let _ = std::fs::remove_dir_all(&work);
    Ok(MeasureResult {
        folder: out,
        json,
        status: report.status,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_summary_lists_scores_and_reasons() {
        let mut check = CheckReport {
            output: "Vocals".into(),
            reference: Reference::Vocals,
            reference_name: Reference::Vocals.name(),
            stats: None,
            input_stats: None,
            songs: vec![
                SongScore {
                    song: "a".into(),
                    sdr: Some(10.0),
                    input_sdr: Some(-5.0),
                },
                SongScore {
                    song: "b".into(),
                    sdr: Some(12.0),
                    input_sdr: None,
                },
            ],
        };
        check.update();
        let model = |status: &str, checks| ModelReport {
            id: "x".into(),
            name: "Model X".into(),
            category: "Vocals".into(),
            author: "a".into(),
            sha256: "0".into(),
            published_sdr: Some(11.0),
            published_note: "vocals".into(),
            published_stems: vec![],
            status: status.into(),
            reason: (status != "measured").then(|| "no part".into()),
            test: None,
            test_description: None,
            caveat: None,
            engine_seconds: 1.0,
            checks,
            errors: vec![],
            plan: None,
        };
        let r = Report {
            format: "audiform-separation-scores",
            version: 1,
            app_version: "0.8.0".into(),
            engine_version: "0.2.0".into(),
            os: "windows",
            arch: "x86_64",
            device: "graphics card",
            overlap: None,
            metric: "SDR",
            test_set: TestSet {
                name: "MUSDB18-HQ test",
                folder: "m".into(),
                songs_found: 50,
                songs_used: vec!["a".into(), "b".into()],
                seconds_per_song: None,
            },
            started: "s".into(),
            finished: None,
            status: "done".into(),
            error: None,
            models: vec![
                model("measured", vec![check]),
                model("not measurable", vec![]),
            ],
        };
        let s = summary(&r);
        assert!(s.contains("11.00"), "{s}");
        assert!(s.contains("-5.00"));
        assert!(s.contains("Model X (not measurable): no part"));
        let j: serde_json::Value =
            serde_json::from_slice(&serde_json::to_vec(&r).unwrap()).unwrap();
        assert_eq!(j["models"][0]["checks"][0]["stats"]["mean"], 11.0);
    }
}
