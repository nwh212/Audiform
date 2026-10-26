// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Clip > Phase Align: how far to move one clip so it lines up with
//! another recording of the same sound. The interface makes the move
//! (and any polarity flip) as one undoable edit.

use tauri::async_runtime::spawn_blocking;
use tauri::State;

use super::effects::{isolate, render};
use super::session::OpenSession;
use crate::dsp::align::{align, Alignment};
use crate::session::model::{Id, Session};

/// Longest stretch compared, from where the two clips start to overlap.
const MAX_COMPARE_SECONDS: u64 = 30;

/// The clip's timeline span.
fn span(session: &Session, id: Id) -> Option<(u64, u64)> {
    let c = session
        .tracks
        .iter()
        .flat_map(|t| &t.clips)
        .find(|c| c.id == id)?;
    let rate = session
        .media
        .iter()
        .find(|m| m.id == c.media_id)
        .map_or(session.sample_rate, |m| m.sample_rate);
    Some((
        c.start,
        c.start + c.timeline_length(rate, session.sample_rate),
    ))
}

fn mono(stereo: &[f32]) -> Vec<f32> {
    stereo
        .as_chunks::<2>()
        .0
        .iter()
        .map(|f| (f[0] + f[1]) * 0.5)
        .collect()
}

/// Where `other` sits against `reference`, looking up to `max_ms` either way.
#[tauri::command]
pub async fn phase_align(
    open: State<'_, OpenSession>,
    reference: Id,
    other: Id,
    max_ms: f64,
) -> Result<Alignment, String> {
    let session = open.0.lock().map_err(|e| e.to_string())?.session.clone();
    spawn_blocking(move || {
        let rate = session.sample_rate;
        let (a0, a1) = span(&session, reference).ok_or("The reference clip is gone.")?;
        let (b0, b1) = span(&session, other).ok_or("The clip to move is gone.")?;
        let max_lag = (max_ms.clamp(1.0, 2_000.0) / 1000.0 * f64::from(rate)) as u64;
        // Compare where both play, widened by the search so a late clip
        // still has its matching part inside.
        let start = a0.max(b0).saturating_sub(max_lag);
        let end = a1.min(b1) + max_lag;
        let end = end.min(start + MAX_COMPARE_SECONDS * u64::from(rate));
        if a0.max(b0) >= a1.min(b1) {
            return Err(
                "The two clips do not play at the same time, so there is nothing to line up."
                    .into(),
            );
        }
        let ra = mono(&render(
            &isolate(&session, None, Some(reference)),
            start,
            end,
        )?);
        let rb = mono(&render(&isolate(&session, None, Some(other)), start, end)?);
        let found = align(&ra, &rb, max_lag as usize).ok_or("One of the clips is silent there.")?;
        Ok(found)
    })
    .await
    .map_err(|e| e.to_string())?
}
