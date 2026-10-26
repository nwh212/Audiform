// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! The Waveform editor's file. It is held as a private one-track
//! session whose clips are the edit list, so edits use the same ops,
//! undo and renderer as the multitrack. The file on disk only changes
//! on save, which renders the edit list and replaces the file.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;

use serde::Serialize;
use tauri::async_runtime::spawn_blocking;
use tauri::{AppHandle, Emitter, State};

use super::session::Applied;
use crate::codecs;
use crate::engine::{PreviewPlayer, Transport};
use crate::export::encode::{FormatKind, OutputFormat};
use crate::export::mixdown::{self, MixdownRequest};
use crate::session::model::{new_id, Marker, MarkerKind, MediaRef, Session, SessionView};
use crate::session::ops::{self, ClipSpec, Op};

pub struct WaveState {
    pub session: Session,
    /// The file being edited.
    pub path: PathBuf,
    /// Made by Paste to New File: lives in the app's cache folder until
    /// saved somewhere with Save As.
    pub untitled: bool,
}

/// The file open in the Waveform editor, if any.
#[derive(Default)]
pub struct OpenWave(pub Mutex<Option<WaveState>>);

/// Files set aside when another file is opened in the Waveform editor,
/// with their unsaved edits, so switching files never asks to save.
#[derive(Default)]
pub struct ParkedWaves(pub Mutex<Vec<WaveState>>);

/// Which document the transport is playing, so an edit in one editor
/// never restarts playback of the other.
#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum PlayingDoc {
    #[default]
    Session,
    Wave,
}

#[derive(Default)]
pub struct Playing(pub Mutex<PlayingDoc>);

impl Playing {
    pub fn set(&self, doc: PlayingDoc) {
        if let Ok(mut d) = self.0.lock() {
            *d = doc;
        }
    }

    pub fn is(&self, doc: PlayingDoc) -> bool {
        self.0.lock().map(|d| *d == doc).unwrap_or(false)
    }
}

#[derive(Debug, Serialize)]
pub struct WaveDoc {
    pub session: Session,
    pub path: PathBuf,
    /// Container, for example "WAV" or "MP3". Only WAV files can be saved in place.
    pub format: String,
    pub bit_depth: Option<u32>,
    /// True until a new file is saved somewhere with Save As.
    pub untitled: bool,
}

/// Builds the edit-list session for a file: one track, one clip over
/// the whole file.
/// `session` with every Voice Chain taken off. In the Waveform editor a
/// chain is only ever a preview, so saving and destructive edits work
/// on the file itself; Apply is what writes a chain into the audio.
pub fn without_effects(mut session: Session) -> Session {
    session.strip_effects();
    session
}

pub fn session_for(info: &codecs::AudioInfo) -> Result<Session, String> {
    let name = Path::new(&info.name)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| info.name.clone());
    let mut s = Session::new(name.clone(), info.sample_rate.max(1));
    s.bit_depth = match info.bit_depth {
        Some(16) => 16,
        Some(24) => 24,
        _ => 32,
    };
    let (m, t) = (new_id(), new_id());
    ops::apply(
        &mut s,
        &Op::Batch {
            ops: vec![
                Op::AddMedia {
                    media: MediaRef {
                        id: m,
                        path: info.path.clone(),
                        relative_path: None,
                        sample_rate: info.sample_rate,
                        channels: info.channels,
                        length: info.length,
                    },
                },
                Op::AddTrack {
                    id: t,
                    name: name.clone(),
                    stereo: info.channels > 1,
                    index: 0,
                    midi: false,
                },
            ],
        },
    )
    .map_err(|e| e.to_string())?;
    // An empty new file has a track but nothing on it yet.
    if info.length > 0 {
        ops::apply(
            &mut s,
            &Op::AddClips {
                track_id: t,
                clips: vec![ClipSpec {
                    id: new_id(),
                    name,
                    media_id: m,
                    start: 0,
                    source_in: 0,
                    source_out: info.length,
                }],
            },
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(s)
}

fn doc_of(state: &WaveState) -> WaveDoc {
    let info = codecs::probe(&state.path).ok();
    WaveDoc {
        session: state.session.clone(),
        path: state.path.clone(),
        format: info.as_ref().map(|i| i.format.clone()).unwrap_or_default(),
        bit_depth: info.and_then(|i| i.bit_depth),
        untitled: state.untitled,
    }
}

/// Markers for a file's cue points.
fn markers_from_cues(cues: &[codecs::cue::Cue]) -> Vec<Marker> {
    cues.iter()
        .map(|c| Marker {
            id: new_id(),
            name: c.label.clone(),
            kind: MarkerKind::Cue,
            position: c.position,
            duration: c.length,
            description: String::new(),
            chapter: None,
            contracted_duration: None,
            color: None,
        })
        .collect()
}

/// Cue points for the editor's markers, at `rate` (the saved file's rate).
fn cues_from_markers(markers: &[Marker], session_rate: u32, rate: u32) -> Vec<codecs::cue::Cue> {
    let conv = |v: u64| {
        if session_rate == rate || session_rate == 0 {
            v
        } else {
            (u128::from(v) * u128::from(rate) / u128::from(session_rate)) as u64
        }
    };
    markers
        .iter()
        .map(|m| codecs::cue::Cue {
            position: conv(m.position),
            length: conv(m.duration),
            label: m.name.clone(),
        })
        .collect()
}

/// Opens `path` in the Waveform editor, replacing whatever was open.
#[tauri::command]
pub async fn wave_open(
    wave: State<'_, OpenWave>,
    transport: State<'_, Transport>,
    playing: State<'_, Playing>,
    path: PathBuf,
) -> Result<WaveDoc, String> {
    let p = path.clone();
    let info = spawn_blocking(move || codecs::probe(&p))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    if info.length == 0 {
        return Err("This file has no audio to edit.".into());
    }
    let mut session = session_for(&info)?;
    let p = path.clone();
    let cues = spawn_blocking(move || codecs::cue::read_cues(&p))
        .await
        .map_err(|e| e.to_string())?;
    session.markers = markers_from_cues(&cues);
    if playing.is(PlayingDoc::Wave) {
        transport.stop();
    }
    let mut w = wave.0.lock().map_err(|e| e.to_string())?;
    *w = Some(WaveState {
        session,
        path,
        untitled: false,
    });
    Ok(doc_of(w.as_ref().expect("just set")))
}

/// Sets the open file aside (edits kept in memory) and leaves the editor empty.
#[tauri::command]
pub fn wave_park(
    wave: State<'_, OpenWave>,
    parked: State<'_, ParkedWaves>,
    transport: State<'_, Transport>,
    playing: State<'_, Playing>,
) -> Result<(), String> {
    if playing.is(PlayingDoc::Wave) {
        transport.stop();
    }
    let Some(state) = wave.0.lock().map_err(|e| e.to_string())?.take() else {
        return Ok(());
    };
    let mut p = parked.0.lock().map_err(|e| e.to_string())?;
    p.retain(|w| w.path != state.path);
    p.push(state);
    Ok(())
}

/// Brings back a set-aside file, if `path` is one. None means open it from disk.
#[tauri::command]
pub fn wave_resume(
    wave: State<'_, OpenWave>,
    parked: State<'_, ParkedWaves>,
    path: PathBuf,
) -> Result<Option<WaveDoc>, String> {
    let mut p = parked.0.lock().map_err(|e| e.to_string())?;
    let Some(i) = p.iter().position(|w| w.path == path) else {
        return Ok(None);
    };
    let state = p.remove(i);
    let doc = doc_of(&state);
    *wave.0.lock().map_err(|e| e.to_string())? = Some(state);
    Ok(Some(doc))
}

/// Drops a set-aside file and its unsaved edits.
#[tauri::command]
pub fn wave_forget(parked: State<'_, ParkedWaves>, path: PathBuf) -> Result<(), String> {
    parked
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .retain(|w| w.path != path);
    Ok(())
}

#[tauri::command]
pub fn wave_get(wave: State<'_, OpenWave>) -> Result<Option<WaveDoc>, String> {
    let w = wave.0.lock().map_err(|e| e.to_string())?;
    Ok(w.as_ref().map(doc_of))
}

#[tauri::command]
pub fn wave_close(
    wave: State<'_, OpenWave>,
    transport: State<'_, Transport>,
    playing: State<'_, Playing>,
) -> Result<(), String> {
    if playing.is(PlayingDoc::Wave) {
        transport.stop();
    }
    *wave.0.lock().map_err(|e| e.to_string())? = None;
    Ok(())
}

#[tauri::command]
pub fn wave_apply(
    wave: State<'_, OpenWave>,
    transport: State<'_, Transport>,
    playing: State<'_, Playing>,
    op: Op,
) -> Result<Applied, String> {
    let mut w = wave.0.lock().map_err(|e| e.to_string())?;
    let state = w
        .as_mut()
        .ok_or("No file is open in the Waveform editor.")?;
    let inverse = ops::apply(&mut state.session, &op).map_err(|e| e.to_string())?;
    if playing.is(PlayingDoc::Wave) {
        if let Err(e) = transport.sync(&state.session) {
            eprintln!("playback could not follow the edit: {e}");
            transport.stop();
        }
    }
    Ok(Applied {
        session: state.session.clone(),
        inverse,
    })
}

#[tauri::command]
pub fn wave_set_view(wave: State<'_, OpenWave>, view: SessionView) -> Result<(), String> {
    let mut w = wave.0.lock().map_err(|e| e.to_string())?;
    if let Some(state) = w.as_mut() {
        ops::set_view(&mut state.session, view);
    }
    Ok(())
}

/// Plays the Waveform editor's file from `from` (samples at its rate).
#[tauri::command]
pub fn wave_play(
    wave: State<'_, OpenWave>,
    transport: State<'_, Transport>,
    preview: State<'_, PreviewPlayer>,
    playing: State<'_, Playing>,
    from: Option<u64>,
    mode: Option<super::transport::PlayMode>,
) -> Result<(), String> {
    preview.stop();
    let w = wave.0.lock().map_err(|e| e.to_string())?;
    let state = w
        .as_ref()
        .ok_or("No file is open in the Waveform editor.")?;
    playing.set(PlayingDoc::Wave);
    transport.play(
        &state.session,
        from.unwrap_or(state.session.view.playhead),
        super::transport::jump_for(&state.session, mode),
    )
}

/// File > New Audio File: an empty untitled file in the app's cache
/// folder, opened in the Waveform editor. Paste or record into it, then
/// Save As.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn wave_new(
    app: AppHandle,
    wave: State<'_, OpenWave>,
    transport: State<'_, Transport>,
    playing: State<'_, Playing>,
    name: String,
    sample_rate: u32,
    channels: u16,
    bit_depth: u16,
) -> Result<WaveDoc, String> {
    let rate = sample_rate.clamp(8_000, 384_000);
    let channels = channels.clamp(1, 2);
    let bits = if [16, 24, 32].contains(&bit_depth) {
        bit_depth
    } else {
        32
    };
    let dir = crate::commands::cache::cache_root(&app)?.join("untitled");
    let path = spawn_blocking(move || -> Result<PathBuf, String> {
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = mixdown::unique_path(&dir, &name, "wav");
        let spec = hound::WavSpec {
            channels,
            sample_rate: rate,
            bits_per_sample: bits,
            sample_format: if bits == 32 {
                hound::SampleFormat::Float
            } else {
                hound::SampleFormat::Int
            },
        };
        hound::WavWriter::create(&path, spec)
            .and_then(|w| w.finalize())
            .map_err(|e| e.to_string())?;
        Ok(path)
    })
    .await
    .map_err(|e| e.to_string())??;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let info = codecs::AudioInfo {
        path: path.clone(),
        name,
        format: "WAV".into(),
        codec: if bits == 32 {
            "PCM float".into()
        } else {
            format!("PCM {bits}-bit")
        },
        sample_rate: rate,
        channels,
        length: 0,
        bit_depth: Some(u32::from(bits)),
        file_size: 0,
    };
    let session = session_for(&info)?;
    if playing.is(PlayingDoc::Wave) {
        transport.stop();
    }
    let mut w = wave.0.lock().map_err(|e| e.to_string())?;
    *w = Some(WaveState {
        session,
        path,
        untitled: true,
    });
    let mut doc = doc_of(w.as_ref().expect("just set"));
    doc.bit_depth = Some(u32::from(bits));
    Ok(doc)
}

/// Paste to New File: renders `clips` (positions from 0, playing the
/// given `media`) with their own settings to a new untitled file in
/// the app's cache folder and opens it in the Waveform editor. It stays
/// untitled until saved with Save As.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn wave_new_from_clips(
    app: AppHandle,
    wave: State<'_, OpenWave>,
    transport: State<'_, Transport>,
    playing: State<'_, Playing>,
    name: String,
    sample_rate: u32,
    channels: u16,
    clips: Vec<crate::session::model::Clip>,
    media: Vec<MediaRef>,
) -> Result<WaveDoc, String> {
    if clips.is_empty() {
        return Err("Nothing to paste. Copy audio or clips first.".into());
    }
    let mut s = Session::new(name.clone(), sample_rate.clamp(8_000, 384_000));
    s.media = media;
    let mut track = crate::session::model::Track::new(
        name.clone(),
        if channels > 1 {
            crate::session::model::ChannelLayout::Stereo
        } else {
            crate::session::model::ChannelLayout::Mono
        },
    );
    track.clips = clips;
    s.tracks.push(track);
    let dir = crate::commands::cache::cache_root(&app)?.join("untitled");
    let channels = channels.clamp(1, 2);
    let path = spawn_blocking(move || -> Result<PathBuf, String> {
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = mixdown::unique_path(&dir, &name, "wav");
        let req = MixdownRequest {
            clip_only: true,
            channels: Some(channels),
            bit_depth: Some(32),
            ..Default::default()
        };
        mixdown::mixdown(&s, &req, &path, &AtomicBool::new(false), |_, _| {})
            .map_err(|e| e.to_string())?;
        Ok(path)
    })
    .await
    .map_err(|e| e.to_string())??;
    let p = path.clone();
    let info = spawn_blocking(move || codecs::probe(&p))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    let session = session_for(&info)?;
    if playing.is(PlayingDoc::Wave) {
        transport.stop();
    }
    let mut w = wave.0.lock().map_err(|e| e.to_string())?;
    *w = Some(WaveState {
        session,
        path,
        untitled: true,
    });
    Ok(doc_of(w.as_ref().expect("just set")))
}

/// Highest absolute sample of the edited file between `from` and `to`
/// (whole file when None), with clip settings applied. Used by Normalize.
#[tauri::command]
pub async fn wave_peak(
    wave: State<'_, OpenWave>,
    range: Option<(u64, u64)>,
) -> Result<f32, String> {
    let session = {
        let w = wave.0.lock().map_err(|e| e.to_string())?;
        w.as_ref()
            .ok_or("No file is open in the Waveform editor.")?
            .session
            .clone()
    };
    spawn_blocking(move || {
        let req = MixdownRequest {
            range,
            clip_only: true,
            ..Default::default()
        };
        mixdown::render_mix(
            &session,
            &req,
            &AtomicBool::new(false),
            |_, _| {},
            |_| Ok(()),
        )
        .map(|r| r.peak)
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Clone, Serialize)]
struct Progress {
    done: u64,
    total: u64,
}

/// Saves the edited file. With `path` it writes a new file (Save As)
/// and the editor moves to it; without, it replaces the open file. The
/// render goes to a temporary file first, so a failed save never
/// damages the original. Afterwards the editor reopens the saved file
/// with a fresh edit list.
#[tauri::command]
pub async fn wave_save(
    app: AppHandle,
    wave: State<'_, OpenWave>,
    transport: State<'_, Transport>,
    playing: State<'_, Playing>,
    path: Option<PathBuf>,
    bit_depth: Option<u16>,
) -> Result<WaveDoc, String> {
    let (session, source) = {
        let w = wave.0.lock().map_err(|e| e.to_string())?;
        let state = w
            .as_ref()
            .ok_or("No file is open in the Waveform editor.")?;
        // A previewed chain is not part of the file until it is applied.
        (without_effects(state.session.clone()), state.path.clone())
    };
    let untitled = wave
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .as_ref()
        .is_some_and(|s| s.untitled);
    if path.is_none() && untitled {
        return Err(
            "This file has not been saved yet. Use Save As to choose where it goes.".into(),
        );
    }
    let target = path.unwrap_or_else(|| source.clone());
    if playing.is(PlayingDoc::Wave) {
        transport.stop();
    }
    let channels = session
        .media
        .first()
        .map(|m| m.channels)
        .unwrap_or(2)
        .clamp(1, 2);
    // The file's extension picks the format: WAV, FLAC or MP3.
    let ext = target
        .extension()
        .map(|e| e.to_string_lossy().into_owned())
        .unwrap_or_default();
    let Some(kind) = FormatKind::from_extension(&ext) else {
        return Err(format!(
            "Audiform can open .{ext} files but can't write them. Use Save As and pick WAV, FLAC or MP3."
        ));
    };
    let depth = bit_depth.unwrap_or(session.bit_depth);
    let request = MixdownRequest {
        clip_only: true,
        channels: Some(channels),
        format: Some(OutputFormat {
            kind,
            bit_depth: match kind {
                FormatKind::Wav => Some(depth),
                FormatKind::Flac => Some(depth.min(24)),
                FormatKind::Mp3 | FormatKind::M4a => None,
            },
            channels: Some(channels),
            ..Default::default()
        }),
        ..Default::default()
    };
    let view = session.view.clone();
    let markers = session.markers.clone();
    let saved = target.clone();
    spawn_blocking(move || -> Result<(), String> {
        let dir = saved.parent().map(Path::to_path_buf).unwrap_or_default();
        let stem = saved
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "audio".into());
        let tmp = dir.join(format!(".{stem}.saving.{}", kind.extension()));
        let mut last = 0u64;
        let r = mixdown::mixdown(
            &session,
            &request,
            &tmp,
            &AtomicBool::new(false),
            |done, total| {
                if done == total || done - last >= total / 100 {
                    last = done;
                    let _ = app.emit("export-progress", Progress { done, total });
                }
            },
        );
        if let Err(e) = r {
            let _ = std::fs::remove_file(&tmp);
            return Err(e.to_string());
        }
        // WAV keeps the markers inside the file as cue points.
        if kind == FormatKind::Wav && !session.markers.is_empty() {
            let rate = codecs::probe(&tmp)
                .map(|i| i.sample_rate)
                .unwrap_or(session.sample_rate);
            let cues = cues_from_markers(&session.markers, session.sample_rate, rate);
            if let Err(e) = codecs::cue::write_cues(&tmp, &cues) {
                let _ = std::fs::remove_file(&tmp);
                return Err(format!("Could not save the markers: {e}"));
            }
        }
        std::fs::rename(&tmp, &saved).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            format!("Could not replace {}: {e}", saved.display())
        })
    })
    .await
    .map_err(|e| e.to_string())??;

    // Start over on the saved file: the old edit list pointed at audio that changed.
    let p = target.clone();
    let info = spawn_blocking(move || codecs::probe(&p))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    let mut fresh = session_for(&info)?;
    fresh.view = view;
    // Markers carry over: they sit on the same audio in the saved file.
    // (WAV also stores them in the file; FLAC and MP3 keep them while open.)
    fresh.markers = markers;
    let mut w = wave.0.lock().map_err(|e| e.to_string())?;
    *w = Some(WaveState {
        session: fresh,
        path: target,
        untitled: false,
    });
    Ok(doc_of(w.as_ref().expect("just set")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_becomes_one_track_with_one_clip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("v.wav");
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 44_100,
            bits_per_sample: 24,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(&path, spec).unwrap();
        for _ in 0..4_410 {
            w.write_sample(0i32).unwrap();
        }
        w.finalize().unwrap();
        let info = codecs::probe(&path).unwrap();
        let s = session_for(&info).unwrap();
        assert_eq!(s.sample_rate, 44_100);
        assert_eq!(s.bit_depth, 24);
        assert_eq!(s.tracks.len(), 1);
        assert_eq!(s.tracks[0].clips.len(), 1);
        assert_eq!(s.tracks[0].clips[0].source_out, 4_410);
        assert_eq!(s.media[0].channels, 1);
    }

    #[test]
    fn an_empty_file_has_a_track_and_no_clips() {
        let info = codecs::AudioInfo {
            path: "new.wav".into(),
            name: "new.wav".into(),
            format: "WAV".into(),
            codec: "PCM float".into(),
            sample_rate: 48_000,
            channels: 2,
            length: 0,
            bit_depth: Some(32),
            file_size: 0,
        };
        let s = session_for(&info).unwrap();
        assert_eq!(s.tracks.len(), 1);
        assert!(s.tracks[0].clips.is_empty());
        assert_eq!(s.media[0].channels, 2);
    }
}

/// Multitrack > Mix Down to New File: renders the session's mix (or the
/// requested range or clips) into a new untitled 32-bit float file and
/// opens it in the Waveform editor, ready to edit or Save As. Emits
/// `export-progress` and stops on `export_cancel`; None when cancelled.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn wave_new_from_mix(
    app: AppHandle,
    open: State<'_, super::session::OpenSession>,
    cancel: State<'_, super::export::ExportCancel>,
    wave: State<'_, OpenWave>,
    transport: State<'_, Transport>,
    playing: State<'_, Playing>,
    name: String,
    mut request: MixdownRequest,
) -> Result<Option<(WaveDoc, mixdown::MixdownResult)>, String> {
    let session = open.0.lock().map_err(|e| e.to_string())?.session.clone();
    let flag = cancel.0.clone();
    flag.store(false, std::sync::atomic::Ordering::Relaxed);
    request.bit_depth = Some(32);
    request.format = None;
    let dir = crate::commands::cache::cache_root(&app)?.join("untitled");
    let progress = app.clone();
    let (path, result) = spawn_blocking(
        move || -> Result<(PathBuf, mixdown::MixdownResult), String> {
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let path = mixdown::unique_path(&dir, &name, "wav");
            let mut last = 0u64;
            let r = mixdown::mixdown(&session, &request, &path, &flag, |done, total| {
                if done == total || done - last >= total / 100 {
                    last = done;
                    let _ = progress.emit(
                        "export-progress",
                        serde_json::json!({ "done": done, "total": total }),
                    );
                }
            })
            .map_err(|e| e.to_string())?;
            Ok((path, r))
        },
    )
    .await
    .map_err(|e| e.to_string())??;
    if result.cancelled {
        return Ok(None);
    }
    let p = path.clone();
    let info = spawn_blocking(move || codecs::probe(&p))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    let session = session_for(&info)?;
    if playing.is(PlayingDoc::Wave) {
        transport.stop();
    }
    let mut w = wave.0.lock().map_err(|e| e.to_string())?;
    *w = Some(WaveState {
        session,
        path,
        untitled: true,
    });
    Ok(Some((doc_of(w.as_ref().expect("just set")), result)))
}
