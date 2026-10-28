// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

//! Pre-render: a track's clips through its Audio FX, rendered ahead.
//!
//! With Pre-render on, a track's clips (or its MIDI notes through its
//! instrument) and its Audio FX are rendered in the background to a file
//! in the app's cache folder. Once that file is ready, playback and
//! export play it in place of the clips and effects, so heavy plug-ins
//! rest. What comes after the effects stays live: the fader, pan, mono,
//! polarity, sends, output and the room spot.
//!
//! Each track's render is named after a fingerprint of everything that
//! shapes its sound before the fader (clips, notes, instrument, effects
//! and their automation, the files used, the tempo, the sample rate).
//! Any change gives a new fingerprint: the live track plays again while
//! the new render is made, and takes over when it is ready. Going back to
//! an earlier state reuses its render if it is still in the cache.
//!
//! Plug-in settings changed in a plug-in's own window reach the session
//! when they are captured (on save, or when the window closes), and a new
//! render starts then.

use std::borrow::Cow;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{LazyLock, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use super::session::OpenSession;
use super::wave::{Playing, PlayingDoc};
use crate::engine::Transport;
use crate::export::mixdown::{self, MixdownRequest};
use crate::session::model::{
    new_id, EffectChain, EnvelopeTarget, Id, MediaRef, Output, Session, TrackKind,
};
use crate::session::ops::ClipSpec;

/// Where each pre-rendered track stands.
#[derive(Debug, Clone, Default)]
struct Entry {
    /// The fingerprint being rendered now, if any.
    rendering: Option<u64>,
    /// A finished render: its fingerprint, file and length (samples).
    ready: Option<(u64, PathBuf, u64)>,
}

static STATE: LazyLock<Mutex<HashMap<Id, Entry>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// The track alone as its render hears it: clips, notes, instrument and
/// effects (with their automation), through nothing after the effects.
fn alone(s: &Session, id: Id) -> Session {
    let mut out = s.clone();
    out.tracks.retain(|t| t.id == id);
    for t in &mut out.tracks {
        t.muted = false;
        t.solo = false;
        t.volume_db = 0.0;
        t.pan = 0.0;
        t.mono = false;
        t.polarity_inverted = false;
        t.prerender = false;
        t.output = Output::Master;
        t.sends.clear();
        t.space = None;
        t.envelopes
            .retain(|e| matches!(e.target, EnvelopeTarget::EffectParam { .. }));
    }
    out.buses.clear();
    out.master.volume_db = 0.0;
    out.master.effects = EffectChain::default();
    out.master.envelopes.clear();
    // Rendered as plain stereo; the room spot is applied live after it.
    out.space = None;
    out
}

/// The fingerprint of what shapes the track's sound before its fader.
fn fingerprint(s: &Session, id: Id) -> Option<u64> {
    let a = alone(s, id);
    let t = a.tracks.first()?;
    let used: Vec<&MediaRef> = a
        .media
        .iter()
        .filter(|m| t.clips.iter().any(|c| c.media_id == m.id))
        .collect();
    let text = serde_json::to_string(&(
        &t.kind,
        &t.channels,
        &t.clips,
        &t.midi_clips,
        &t.instrument,
        &t.effects,
        &t.envelopes,
        used.iter()
            .map(|m| (&m.path, m.length, m.channels, m.sample_rate))
            .collect::<Vec<_>>(),
        a.sample_rate,
        serde_json::to_string(&a.tempo).unwrap_or_default(),
    ))
    .ok()?;
    let mut h = DefaultHasher::new();
    text.hash(&mut h);
    Some(h.finish())
}

/// The session with every pre-rendered track whose render is ready
/// playing that render in place of its clips and effects.
pub fn heard(s: &Session) -> Cow<'_, Session> {
    if !s.tracks.iter().any(|t| t.prerender) {
        return Cow::Borrowed(s);
    }
    let state = STATE.lock().map(|m| m.clone()).unwrap_or_default();
    let ready: Vec<(Id, PathBuf, u64)> = s
        .tracks
        .iter()
        .filter(|t| t.prerender)
        .filter_map(|t| {
            let (key, path, len) = state.get(&t.id)?.ready.clone()?;
            (Some(key) == fingerprint(s, t.id)).then_some((t.id, path, len))
        })
        .collect();
    if ready.is_empty() {
        return Cow::Borrowed(s);
    }
    let mut out = s.clone();
    for (id, path, len) in ready {
        let media = MediaRef {
            id: new_id(),
            path,
            relative_path: None,
            sample_rate: s.sample_rate,
            channels: 2,
            length: len,
        };
        let Some(t) = out.tracks.iter_mut().find(|t| t.id == id) else {
            continue;
        };
        t.clips = vec![ClipSpec {
            id: new_id(),
            name: format!("{} (pre-rendered)", t.name),
            media_id: media.id,
            start: 0,
            source_in: 0,
            source_out: len,
            channel_mix: None,
        }
        .build()];
        t.midi_clips.clear();
        t.kind = TrackKind::Audio;
        t.instrument = None;
        t.effects = EffectChain::default();
        t.envelopes
            .retain(|e| !matches!(e.target, EnvelopeTarget::EffectParam { .. }));
        out.media.push(media);
    }
    Cow::Owned(out)
}

/// Keeps the session's playback in step after a render came or went.
fn resync(app: &AppHandle) {
    let playing = app.state::<Playing>();
    if !playing.is(PlayingDoc::Session) {
        return;
    }
    let open = app.state::<OpenSession>();
    let transport = app.state::<Transport>();
    let heard = match open.0.lock() {
        Ok(s) => super::live::heard(&s.session).into_owned(),
        Err(_) => return,
    };
    if let Err(e) = transport.sync(&heard) {
        eprintln!("playback could not follow a pre-render: {e}");
    }
}

/// Starts a render for each pre-rendered track whose sound changed, and
/// picks up renders already in the cache. Cheap when nothing changed.
pub fn refresh(app: &AppHandle, s: &Session) {
    let Ok(root) = super::cache::cache_root(app) else {
        return;
    };
    let dir = root.join("prerender");
    let mut state = match STATE.lock() {
        Ok(m) => m,
        Err(_) => return,
    };
    // Nothing is pre-rendered and nothing was: every edit comes through
    // here, so leave before copying the session.
    if state.is_empty() && !s.tracks.iter().any(|t| t.prerender) {
        return;
    }
    // The session as it sounds, unsaved Waveform editor edits included.
    let base = super::live::files_heard(s);
    let before = snapshot(&state);
    state.retain(|id, _| base.tracks.iter().any(|t| t.id == *id && t.prerender));
    for t in base.tracks.iter().filter(|t| t.prerender) {
        let Some(key) = fingerprint(&base, t.id) else {
            continue;
        };
        let e = state.entry(t.id).or_default();
        if e.ready.as_ref().is_some_and(|r| r.0 == key) || e.rendering == Some(key) {
            continue;
        }
        let file = dir.join(format!("{key:016x}.wav"));
        if let Ok(info) = crate::codecs::probe(&file) {
            e.ready = Some((key, file, info.length));
            continue;
        }
        e.rendering = Some(key);
        let job = alone(&base, t.id);
        let (app, id, dir) = (app.clone(), t.id, dir.clone());
        std::thread::spawn(move || {
            let done = render(&job, &dir, &file);
            prune(&dir);
            if let Ok(mut st) = STATE.lock() {
                if let Some(e) = st.get_mut(&id) {
                    if e.rendering == Some(key) {
                        e.rendering = None;
                        match done {
                            Ok(len) => e.ready = Some((key, file, len)),
                            Err(err) => eprintln!("pre-render failed: {err}"),
                        }
                    }
                }
            }
            resync(&app);
            let _ = app.emit("prerender-changed", ());
        });
    }
    // Only said when something changed: every edit calls this, and the
    // interface re-reads the state each time it hears.
    let changed = snapshot(&state) != before;
    drop(state);
    if changed {
        let _ = app.emit("prerender-changed", ());
    }
}

/// Where each pre-rendered track stands, for noticing a change.
fn snapshot(state: &HashMap<Id, Entry>) -> Vec<(Id, bool, bool)> {
    let mut v: Vec<(Id, bool, bool)> = state
        .iter()
        .map(|(id, e)| (*id, e.ready.is_some(), e.rendering.is_some()))
        .collect();
    v.sort();
    v
}

/// How many renders the cache keeps, newest first, so undoing to an
/// earlier state can reuse one without the folder growing forever.
const KEEP: usize = 24;

/// Removes all but the newest [`KEEP`] renders, never one in use.
fn prune(dir: &Path) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    let in_use: Vec<PathBuf> = STATE
        .lock()
        .map(|m| {
            m.values()
                .filter_map(|e| e.ready.as_ref().map(|r| r.1.clone()))
                .collect()
        })
        .unwrap_or_default();
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = read
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "wav"))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    files.sort_by_key(|a| std::cmp::Reverse(a.0));
    for (_, path) in files.into_iter().skip(KEEP) {
        if !in_use.contains(&path) && !path.to_string_lossy().ends_with(".part.wav") {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Renders one track alone to `file`: 32-bit float stereo at the session
/// rate, from the session's start. Returns its length in samples.
fn render(job: &Session, dir: &Path, file: &Path) -> Result<u64, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let tmp = file.with_extension("part.wav");
    let request = MixdownRequest {
        bit_depth: Some(32),
        ..Default::default()
    };
    mixdown::mixdown(job, &request, &tmp, &AtomicBool::new(false), |_, _| {})
        .map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, file).map_err(|e| e.to_string())?;
    Ok(crate::codecs::probe(file)
        .map_err(|e| e.to_string())?
        .length)
}

/// Where a pre-rendered track stands, for the interface.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrerenderStatus {
    pub track_id: Id,
    /// "rendering", "ready", or "waiting" (on, but nothing rendered yet).
    pub state: &'static str,
}

/// Each pre-rendered track's state.
#[tauri::command]
pub fn prerender_status(open: State<'_, OpenSession>) -> Result<Vec<PrerenderStatus>, String> {
    let s = open.0.lock().map_err(|e| e.to_string())?;
    let base = super::live::files_heard(&s.session).into_owned();
    let state = STATE.lock().map_err(|e| e.to_string())?;
    Ok(base
        .tracks
        .iter()
        .filter(|t| t.prerender)
        .map(|t| {
            let e = state.get(&t.id);
            let key = fingerprint(&base, t.id);
            let state = match e {
                Some(e) if e.ready.as_ref().is_some_and(|r| Some(r.0) == key) => "ready",
                Some(e) if e.rendering.is_some() => "rendering",
                _ => "waiting",
            };
            PrerenderStatus {
                track_id: t.id,
                state,
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::model::{Envelope, Track};

    fn session() -> (Session, Id) {
        let mut s = Session::new("s", 48_000);
        let mut t = Track::new("Synth", crate::session::model::ChannelLayout::Stereo);
        t.prerender = true;
        t.volume_db = -6.0;
        t.pan = 0.5;
        let id = t.id;
        s.tracks.push(t);
        (s, id)
    }

    #[test]
    fn the_fingerprint_follows_the_sound_not_the_fader() {
        let (mut s, id) = session();
        let a = fingerprint(&s, id).unwrap();
        // Fader, pan and mute come after the render: same fingerprint.
        s.tracks[0].volume_db = 3.0;
        s.tracks[0].pan = -1.0;
        s.tracks[0].muted = true;
        assert_eq!(fingerprint(&s, id), Some(a));
        // The effects shape the sound: a new fingerprint.
        s.tracks[0].effects.enabled = !s.tracks[0].effects.enabled;
        assert_ne!(fingerprint(&s, id), Some(a));
    }

    #[test]
    fn a_ready_render_stands_in_for_the_clips_and_effects() {
        let (mut s, id) = session();
        s.tracks[0].envelopes.push(Envelope {
            id: new_id(),
            target: EnvelopeTarget::Volume,
            enabled: true,
            points: Vec::new(),
            height: None,
        });
        let key = fingerprint(&s, id).unwrap();
        STATE.lock().unwrap().insert(
            id,
            Entry {
                rendering: None,
                ready: Some((key, PathBuf::from("/cache/prerender/x.wav"), 96_000)),
            },
        );
        let h = heard(&s);
        let t = &h.tracks[0];
        assert_eq!(t.clips.len(), 1);
        assert_eq!(t.clips[0].source_out, 96_000);
        assert!(t.effects.slots.is_empty());
        // What comes after the effects stays.
        assert_eq!((t.volume_db, t.pan), (-6.0, 0.5));
        assert_eq!(t.envelopes.len(), 1);
        assert!(h
            .media
            .iter()
            .any(|m| m.path == Path::new("/cache/prerender/x.wav")));
        // A change to the sound: the live track plays again.
        s.tracks[0].effects.enabled = !s.tracks[0].effects.enabled;
        assert!(matches!(heard(&s), Cow::Borrowed(_)));
        STATE.lock().unwrap().clear();
    }
}
