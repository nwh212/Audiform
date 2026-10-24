// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

//! What the editor and a plug-in host process say to each other, over the
//! host's stdin and stdout. Plain data, no plug-in types, so the editor
//! can use it without touching any unsafe code.
//!
//! Every message is one frame: a kind byte, a little-endian u32 length,
//! then the payload.
//! - `CONTROL`: JSON. Requests from the editor ([`Request`]); replies and
//!   events from the host ([`Message`]).
//! - `AUDIO`: one block. From the editor: frames (u32), the number of
//!   parameter changes (u32), each change as id (u32) and value (f64),
//!   the number of note events (u32), each as frame offset (u32), on or
//!   off (u8), pitch (u8) and velocity (f32, 0 to 1), then the frames as
//!   interleaved stereo f32. From the host: the processed frames,
//!   interleaved stereo f32.

use std::io::{self, Read, Write};

use serde::{Deserialize, Serialize};

pub const CONTROL: u8 = 1;
pub const AUDIO: u8 = 2;

/// Largest block a host processes in one call. Bigger blocks are split.
pub const MAX_BLOCK: usize = 8_192;

/// A plug-in found in a bundle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClassInfo {
    /// The class id as 32 hex digits.
    pub cid: String,
    pub name: String,
    pub vendor: String,
    pub version: String,
    /// For example "Fx|Pitch Shift".
    pub sub_categories: String,
    pub params: Vec<ParamInfo>,
    pub latency: u32,
    pub has_editor: bool,
    /// Plays notes (takes note events and makes sound) rather than
    /// processing audio.
    #[serde(default)]
    pub instrument: bool,
}

/// A note starting or ending within a block.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NoteEvent {
    /// Frames from the start of the block.
    pub offset: u32,
    pub on: bool,
    pub pitch: u8,
    /// 0 to 1.
    pub velocity: f32,
}

/// One plug-in control. Values are normalized, 0 to 1.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParamInfo {
    pub id: u32,
    pub title: String,
    pub units: String,
    /// 0 for a continuous control, otherwise the number of steps.
    pub steps: i32,
    pub default: f64,
    /// Not shown as a control (read-only meters and the like).
    pub hidden: bool,
}

/// What a bundle holds, or why it could not be read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScanResult {
    pub classes: Vec<ClassInfo>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "req", rename_all = "snake_case")]
pub enum Request {
    /// Loads one plug-in. Must come first.
    Load {
        path: String,
        cid: String,
        rate: f64,
        /// Saved settings from [`Message::State`], if any.
        #[serde(default)]
        state: Option<String>,
        /// Control values to set after loading (normalized).
        #[serde(default)]
        params: Vec<(u32, f64)>,
    },
    /// Clears the plug-in's memory (filters, envelopes), as after a jump.
    Reset,
    /// Asks for the plug-in's settings as a [`Message::State`].
    GetState,
    /// Opens the plug-in's own window, or brings it to the front.
    OpenEditor {
        title: String,
    },
    CloseEditor,
    /// The session's tempo, told to the plug-in with every block.
    Tempo {
        bpm: f64,
        beats_per_bar: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "msg", rename_all = "snake_case")]
pub enum Message {
    Loaded {
        latency: u32,
        params: Vec<ParamInfo>,
        has_editor: bool,
        #[serde(default)]
        instrument: bool,
    },
    /// Settings: base64 of the plug-in's component and controller state.
    State {
        data: String,
    },
    Ok,
    Error {
        message: String,
    },
    /// Event: a control was moved in the plug-in's own window.
    ParamEdited {
        id: u32,
        value: f64,
    },
    /// Event: the plug-in's look-ahead changed (playback must re-align).
    LatencyChanged {
        latency: u32,
    },
    /// Event: the plug-in's window was closed.
    EditorClosed,
    /// Event: the plug-in changed settings that are not controls (save
    /// its state to keep them).
    Dirty,
}

pub fn write_frame(w: &mut impl Write, kind: u8, payload: &[u8]) -> io::Result<()> {
    let mut head = [0u8; 5];
    head[0] = kind;
    head[1..].copy_from_slice(&(payload.len() as u32).to_le_bytes());
    w.write_all(&head)?;
    w.write_all(payload)?;
    w.flush()
}

/// Reads one frame into `buf`; returns its kind. `Ok(None)` at the end of the stream.
pub fn read_frame(r: &mut impl Read, buf: &mut Vec<u8>) -> io::Result<Option<u8>> {
    let mut head = [0u8; 5];
    match r.read_exact(&mut head) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let len = u32::from_le_bytes([head[1], head[2], head[3], head[4]]) as usize;
    if len > 256 * 1024 * 1024 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "frame too large",
        ));
    }
    buf.clear();
    buf.resize(len, 0);
    r.read_exact(buf)?;
    Ok(Some(head[0]))
}

/// An audio request: frames, parameter changes and interleaved samples.
pub fn encode_audio(
    out: &mut Vec<u8>,
    changes: &[(u32, f64)],
    notes: &[NoteEvent],
    samples: &[f32],
) {
    out.clear();
    out.extend_from_slice(&((samples.len() / 2) as u32).to_le_bytes());
    out.extend_from_slice(&(changes.len() as u32).to_le_bytes());
    for (id, v) in changes {
        out.extend_from_slice(&id.to_le_bytes());
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&(notes.len() as u32).to_le_bytes());
    for n in notes {
        out.extend_from_slice(&n.offset.to_le_bytes());
        out.push(u8::from(n.on));
        out.push(n.pitch);
        out.extend_from_slice(&n.velocity.to_le_bytes());
    }
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
}

/// Reads an audio request back: changes, notes and samples. None if malformed.
pub fn decode_audio(
    buf: &[u8],
    changes: &mut Vec<(u32, f64)>,
    notes: &mut Vec<NoteEvent>,
    samples: &mut Vec<f32>,
) -> Option<()> {
    let u32_at = |i: usize| {
        buf.get(i..i + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    };
    let frames = u32_at(0)? as usize;
    let n = u32_at(4)? as usize;
    let mut at = 8;
    changes.clear();
    for _ in 0..n {
        let id = u32_at(at)?;
        let v = f64::from_le_bytes(buf.get(at + 4..at + 12)?.try_into().ok()?);
        changes.push((id, v));
        at += 12;
    }
    let m = u32_at(at)? as usize;
    at += 4;
    notes.clear();
    for _ in 0..m {
        let offset = u32_at(at)?;
        let on = *buf.get(at + 4)? != 0;
        let pitch = *buf.get(at + 5)?;
        let velocity = f32::from_le_bytes(buf.get(at + 6..at + 10)?.try_into().ok()?);
        notes.push(NoteEvent {
            offset,
            on,
            pitch,
            velocity,
        });
        at += 10;
    }
    decode_samples(buf.get(at..)?, samples);
    (samples.len() == frames * 2).then_some(())
}

pub fn encode_samples(out: &mut Vec<u8>, samples: &[f32]) {
    out.clear();
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
}

pub fn decode_samples(buf: &[u8], samples: &mut Vec<f32>) {
    samples.clear();
    samples.extend(
        buf.chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])),
    );
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn base64_encode(data: &[u8]) -> String {
    let mut s = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (u32::from(c[0]) << 16)
            | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
            | u32::from(*c.get(2).unwrap_or(&0));
        for (i, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            if i <= c.len() {
                s.push(B64[((n >> shift) & 63) as usize] as char);
            } else {
                s.push('=');
            }
        }
    }
    s
}

pub fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    let mut acc = 0u32;
    let mut bits = 0;
    for ch in s.bytes().filter(|b| !b.is_ascii_whitespace()) {
        if ch == b'=' {
            break;
        }
        let v = B64.iter().position(|b| *b == ch)? as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_and_audio_round_trip() {
        let mut wire = Vec::new();
        let mut body = Vec::new();
        let note = NoteEvent {
            offset: 1,
            on: true,
            pitch: 60,
            velocity: 0.75,
        };
        encode_audio(&mut body, &[(7, 0.25)], &[note], &[0.5, -0.5, 1.0, 0.0]);
        write_frame(&mut wire, AUDIO, &body).unwrap();
        let mut r = &wire[..];
        let mut buf = Vec::new();
        assert_eq!(read_frame(&mut r, &mut buf).unwrap(), Some(AUDIO));
        let (mut ch, mut n, mut s) = (Vec::new(), Vec::new(), Vec::new());
        decode_audio(&buf, &mut ch, &mut n, &mut s).unwrap();
        assert_eq!(ch, vec![(7, 0.25)]);
        assert_eq!(n, vec![note]);
        assert_eq!(s, vec![0.5, -0.5, 1.0, 0.0]);
        assert_eq!(read_frame(&mut r, &mut buf).unwrap(), None);
    }

    #[test]
    fn base64_round_trips() {
        for data in [&b""[..], b"f", b"fo", b"foo", b"foob", &[0, 255, 7, 128, 3]] {
            assert_eq!(base64_decode(&base64_encode(data)).unwrap(), data);
        }
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
    }

    #[test]
    fn json_messages_are_tagged() {
        let r = Request::OpenEditor {
            title: "Vocal".into(),
        };
        assert_eq!(
            serde_json::to_string(&r).unwrap(),
            r#"{"req":"open_editor","title":"Vocal"}"#
        );
        let m: Message =
            serde_json::from_str(r#"{"msg":"param_edited","id":3,"value":0.5}"#).unwrap();
        assert_eq!(m, Message::ParamEdited { id: 3, value: 0.5 });
    }
}
