// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Clip > Fit Music to Length: plans where to cut or repeat a music
//! clip so it plays for an exact time. The interface turns the plan into
//! clips with crossfades, so the result stays editable and undoable.

use std::path::PathBuf;

use tauri::async_runtime::spawn_blocking;

use crate::codecs::AudioReader;
use crate::dsp::musicfit::{analyze, FitPlan, Segment, RATE};

/// Plans `target` seconds from the part of `path` between `start` and
/// `end` seconds: up to three plans, best first, joining in different
/// places. Segment times come back in seconds into the file.
#[tauri::command]
pub async fn music_fit(
    path: PathBuf,
    start: f64,
    end: f64,
    target: f64,
    xfade: f64,
) -> Result<Vec<FitPlan>, String> {
    spawn_blocking(move || {
        let mut r = AudioReader::open(&path, 1, Some(RATE)).map_err(|e| e.to_string())?;
        if start > 0.0 {
            r.seek(start).map_err(|e| e.to_string())?;
        }
        let want = ((end - start).max(0.0) * f64::from(RATE)) as usize;
        let mut mono = Vec::with_capacity(want);
        let mut buf = Vec::new();
        while mono.len() < want {
            buf.clear();
            if !r.read_into(&mut buf).map_err(|e| e.to_string())? {
                break;
            }
            mono.extend_from_slice(&buf);
        }
        mono.truncate(want);
        let a = analyze(&mono).ok_or("No steady beat found in this clip. Music Fit needs music with a pulse; use Time Stretch for speech or ambience.")?;
        let mut plans = a.plans(target, xfade.clamp(0.0, 0.5), 3);
        if plans.is_empty() {
            return Err("Could not find joins that reach that length. Try a length nearer the clip's own, or Time Stretch.".into());
        }
        for plan in &mut plans {
            for s in &mut plan.segments {
                *s = Segment {
                    start: (s.start + start).max(start),
                    end: (s.end + start).min(end),
                };
            }
        }
        Ok(plans)
    })
    .await
    .map_err(|e| e.to_string())?
}
