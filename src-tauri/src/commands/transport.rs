// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

//! Play, pause and stop for the open session.

use tauri::State;

use super::session::OpenSession;
use crate::engine::{Jump, PreviewPlayer, Transport, TransportStatus};
use crate::session::model::Session;

/// How playback treats the time selection.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PlayMode {
    /// Play straight through.
    #[default]
    Normal,
    /// Loop Playback: repeat the selection.
    Loop,
    /// Skip Selection: play around the selection, to hear a cut before making it.
    Skip,
}

/// The jump `mode` asks for, from the session's time selection.
/// Loop Playback with no time selection loops the whole session, so it
/// stops at the end of the audio instead of playing on into silence.
pub fn jump_for(session: &Session, mode: Option<PlayMode>) -> Option<Jump> {
    let mode = mode.unwrap_or_default();
    let sel = match session.view.selection.as_ref() {
        Some(sel) if sel.end > sel.start => *sel,
        _ if mode == PlayMode::Loop => {
            let end = crate::export::mixdown::session_end(session);
            return (end > 0).then_some(Jump::Loop { start: 0, end });
        }
        _ => return None,
    };
    match mode {
        PlayMode::Normal => None,
        PlayMode::Loop => Some(Jump::Loop {
            start: sel.start,
            end: sel.end,
        }),
        PlayMode::Skip => Some(Jump::Skip {
            start: sel.start,
            end: sel.end,
        }),
    }
}

/// Plays from `from` (session samples), or from the playhead.
#[tauri::command]
pub fn transport_play(
    transport: State<'_, Transport>,
    preview: State<'_, PreviewPlayer>,
    open: State<'_, OpenSession>,
    playing: State<'_, super::wave::Playing>,
    from: Option<u64>,
    mode: Option<PlayMode>,
) -> Result<(), String> {
    preview.stop();
    playing.set(super::wave::PlayingDoc::Session);
    let s = open.0.lock().map_err(|e| e.to_string())?;
    let at = from.unwrap_or(s.session.view.playhead);
    transport.play(&s.session, at, jump_for(&s.session, mode))
}

/// Stops and returns where playback was, in session samples.
#[tauri::command]
pub fn transport_stop(transport: State<'_, Transport>) -> Option<u64> {
    transport.stop()
}

/// Mono check: hear everything folded to mono (listening only).
#[tauri::command]
pub fn transport_set_mono_check(on: bool) {
    crate::engine::transport::set_mono_check(on);
}

#[tauri::command]
pub fn transport_status(transport: State<'_, Transport>) -> TransportStatus {
    transport.status()
}
