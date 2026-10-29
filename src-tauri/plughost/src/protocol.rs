// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! What the editor and a plug-in host process say to each other, over the
//! host's stdin and stdout. Plain data, no plug-in types, so the editor
//! can use it without touching any unsafe code.
//!
//! Every message is one frame: a kind byte, a little-endian u32 length,
//! then the payload.
//! - `CONTROL`: JSON. Requests from the editor ([`Request`]); replies and
//!   events from the host ([`Message`]).
//! - `AUDIO`: one block. From the editor: frames (u32), channels (u32), the number of
//!   parameter changes (u32), each change as id (u32) and value (f64),
//!   the number of note events (u32), each as frame offset (u32), on or
//!   off (u8), pitch (u8), velocity (f32, 0 to 1), MIDI channel (u8,
//!   0 to 15) and pan (f32, -1 left to 1 right), then the frames as
//!   interleaved f32 (stereo, or the plug-in's own channels when it was
//!   loaded with them). From the host: the processed frames, the same way.

use std::io::{self, Read, Write};

use serde::{Deserialize, Serialize};

pub const CONTROL: u8 = 1;
pub const AUDIO: u8 = 2;
/// Which plug-in in the process the frames after it are for (a little-
/// endian u32), in either direction. A host process can run several
/// plug-ins (from one maker, so they can find each other); frames before
/// any SELECT are for plug-in 0, so a process with one plug-in never
/// needs one.
pub const SELECT: u8 = 3;
/// A sidechain key for the next AUDIO block of the selected plug-in: the
/// same number of frames as that block, as interleaved stereo f32.
pub const KEY: u8 = 4;

fn quarter() -> u32 {
    4
}

fn is_zero(v: &u64) -> bool {
    *v == 0
}

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
    /// Has a second audio input (a sidechain) it can listen to.
    #[serde(default)]
    pub sidechain: bool,
    /// Why it was not fully read, when the scan only got its name (its
    /// controls and look-ahead are then learned when it runs).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// A note starting or ending within a block.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct NoteEvent {
    /// Frames from the start of the block.
    pub offset: u32,
    pub on: bool,
    pub pitch: u8,
    /// 0 to 1.
    pub velocity: f32,
    /// MIDI channel, 0 to 15 (shown as 1 to 16).
    pub channel: u8,
    /// The note's own pan, -1 (left) to 1 (right); 0 sends none.
    pub pan: f32,
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

/// Scan mode prints one line per step, each this prefix and a
/// [`ScanLine`] as JSON, so whatever it got through before a plug-in
/// crashes it is kept, and anything else a plug-in prints is ignored.
pub const SCAN_PREFIX: &str = "@@altered-scan ";

/// Lines on stderr naming the step a plug-in is at, for a crash report.
pub const STAGE_PREFIX: &str = "@@altered-stage ";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanLine {
    /// The bundle's plug-ins by name, before any is started.
    Found(Vec<ClassInfo>),
    /// One plug-in read in full.
    Read(ClassInfo),
    /// One plug-in that loaded but refused to start.
    Failed {
        cid: String,
        error: String,
    },
    /// The bundle could not be opened at all.
    Error(String),
    Done,
}

/// What a crash exit code means, in words, if it is a known one.
pub fn crash_reason(code: i32) -> Option<&'static str> {
    Some(match code as u32 {
        0xC000_0005 => "a memory access violation",
        0xC000_0409 => "it stopped itself after a failed safety check",
        0xC000_0374 => "it damaged its own memory",
        0xC000_00FD => "a stack overflow",
        0xC000_0135 => "a file it needs is missing (DLL not found)",
        0xC000_0139 => "a file it needs is the wrong version (entry point not found)",
        0xC000_007B => "it is not a 64-bit plug-in",
        0xC000_001D => "an instruction this processor does not have",
        0x8000_0003 => "a debug breakpoint left in the plug-in",
        0xC000_0602 | 0xE06D_7363 => "an error it did not catch",
        _ => return None,
    })
}

/// How a scan process ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanExit {
    /// It exited with this code.
    Code(i32),
    /// The system stopped it (a crash, on Linux and macOS).
    Killed,
    /// It took too long and was stopped.
    TimedOut,
}

/// Puts a scan process's output back together. `stdout` and `stderr`
/// are what it printed. Plug-ins found by name are listed even when the
/// process died reading them, with a note saying so.
pub fn parse_scan_output(stdout: &str, stderr: &str, exit: ScanExit) -> ScanResult {
    let mut found: Vec<ClassInfo> = Vec::new();
    let mut read: Vec<ClassInfo> = Vec::new();
    let mut failed: Vec<(String, String)> = Vec::new();
    let mut error = None;
    let mut done = false;
    for line in stdout.lines() {
        let Some(json) = line.trim().strip_prefix(SCAN_PREFIX.trim_end()) else {
            continue;
        };
        match serde_json::from_str::<ScanLine>(json.trim()) {
            Ok(ScanLine::Found(c)) => found = c,
            Ok(ScanLine::Read(c)) => read.push(c),
            Ok(ScanLine::Failed { cid, error }) => failed.push((cid, error)),
            Ok(ScanLine::Error(e)) => error = Some(e),
            Ok(ScanLine::Done) => done = true,
            Err(_) => {}
        }
    }
    let stage = stderr
        .lines()
        .rev()
        .find_map(|l| l.trim().strip_prefix(STAGE_PREFIX.trim_end()))
        .map(str::trim);
    // Why the process ended early, if it did.
    let died = (!done).then(|| {
        let at = stage.map(|s| format!(" while {s}")).unwrap_or_default();
        match exit {
            ScanExit::TimedOut => format!("It took too long{at} and was skipped."),
            ScanExit::Killed => format!("It crashed{at}."),
            ScanExit::Code(c) => match crash_reason(c) {
                Some(why) => format!("It crashed{at}: {why} (code {:08X}).", c as u32),
                None if c == 0 => format!("It closed without answering{at}."),
                None => format!("It crashed{at} (code {:08X}).", c as u32),
            },
        }
    });
    let mut classes: Vec<ClassInfo> = Vec::new();
    for c in found {
        if let Some(r) = read.iter().find(|r| r.cid == c.cid) {
            classes.push(r.clone());
        } else if failed.iter().any(|f| f.0 == c.cid) {
            continue;
        } else if let Some(why) = &died {
            // Never got to it, or died reading it: listed by name.
            classes.push(ClassInfo {
                note: Some(format!(
                    "Not fully read. {why} It may still work; if it crashes when used, it is bypassed."
                )),
                ..c
            });
        }
    }
    for r in read {
        if !classes.iter().any(|c| c.cid == r.cid) {
            classes.push(r);
        }
    }
    let mut errors: Vec<String> = failed.into_iter().map(|(_, e)| e).collect();
    if classes.is_empty() {
        if let Some(why) = died {
            errors.push(why);
        }
    }
    ScanResult {
        error: error
            .or_else(|| (classes.is_empty() && !errors.is_empty()).then(|| errors.join("; "))),
        classes,
    }
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
        /// The main input and output: missing for stereo, 0 for the
        /// plug-in's own layout (5.1, 7.1.4 and so on), or a VST3 speaker
        /// arrangement to ask for.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        main: Option<u64>,
    },
    /// Clears the plug-in's memory (filters, envelopes), as after a jump.
    Reset,
    /// Asks for the plug-in's settings as a [`Message::State`].
    GetState,
    /// Replaces the plug-in's settings with saved ones (a preset), from
    /// [`Message::State`] or a preset file. Answered with Ok.
    SetState {
        data: String,
    },
    /// Opens the plug-in's own window, or brings it to the front.
    /// `owner` is the editor's main window (a Windows window handle; 0
    /// for none): the plug-in window stays above it.
    OpenEditor {
        title: String,
        #[serde(default, skip_serializing_if = "is_zero")]
        owner: u64,
        /// Whether the effect is bypassed, for the window's power button.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        bypassed: bool,
    },
    CloseEditor,
    /// The effect was bypassed or turned on in the editor: the window's
    /// power button follows.
    EditorBypassed {
        bypassed: bool,
    },
    /// Hides the plug-in's window for a while (so the editor's close
    /// prompt is seen), or shows it again.
    HideEditor {
        hidden: bool,
    },
    /// Removes the selected plug-in from the process (others carry on).
    Unload,
    /// The session's tempo and time signature, told to the plug-in with
    /// every block, from timeline frame `start` (at `start_music` quarter
    /// notes) until the next one.
    Tempo {
        bpm: f64,
        beats_per_bar: u32,
        #[serde(default = "quarter")]
        beat_unit: u32,
        #[serde(default)]
        start: i64,
        #[serde(default)]
        start_music: f64,
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
        /// The main input's and output's speakers, by name (L, R, C, LFE...).
        #[serde(default)]
        inputs: Vec<String>,
        #[serde(default)]
        outputs: Vec<String>,
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
    /// Event: the power button on the plug-in's window was clicked.
    BypassPicked {
        bypassed: bool,
    },
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

/// An audio request: the timeline position of its first frame (-1 when
/// unknown: the plug-in keeps counting on), frames, parameter changes and
/// interleaved samples.
pub fn encode_audio(
    out: &mut Vec<u8>,
    position: i64,
    changes: &[(u32, f64)],
    notes: &[NoteEvent],
    samples: &[f32],
) {
    encode_audio_ch(out, position, changes, notes, samples, 2);
}

/// [`encode_audio`] for `channels` interleaved channels.
pub fn encode_audio_ch(
    out: &mut Vec<u8>,
    position: i64,
    changes: &[(u32, f64)],
    notes: &[NoteEvent],
    samples: &[f32],
    channels: usize,
) {
    let channels = channels.max(1);
    out.clear();
    out.extend_from_slice(&position.to_le_bytes());
    out.extend_from_slice(&((samples.len() / channels) as u32).to_le_bytes());
    out.extend_from_slice(&(channels as u32).to_le_bytes());
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
        out.push(n.channel.min(15));
        out.extend_from_slice(&n.pan.to_le_bytes());
    }
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
}

/// Reads an audio request back: changes, notes and samples. Returns the
/// channels per frame; None if malformed.
pub fn decode_audio(
    buf: &[u8],
    position: &mut i64,
    changes: &mut Vec<(u32, f64)>,
    notes: &mut Vec<NoteEvent>,
    samples: &mut Vec<f32>,
) -> Option<usize> {
    *position = i64::from_le_bytes(buf.get(..8)?.try_into().ok()?);
    let buf = &buf[8..];
    let u32_at = |i: usize| {
        buf.get(i..i + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    };
    let frames = u32_at(0)? as usize;
    let channels = (u32_at(4)? as usize).clamp(1, 64);
    let n = u32_at(8)? as usize;
    let mut at = 12;
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
        let channel = *buf.get(at + 10)?;
        let pan = f32::from_le_bytes(buf.get(at + 11..at + 15)?.try_into().ok()?);
        notes.push(NoteEvent {
            offset,
            on,
            pitch,
            velocity,
            channel,
            pan,
        });
        at += 15;
    }
    decode_samples(buf.get(at..)?, samples);
    (samples.len() == frames * channels).then_some(channels)
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
        buf.as_chunks::<4>()
            .0
            .iter()
            .map(|b| f32::from_le_bytes(*b)),
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

    fn class(cid: &str) -> ClassInfo {
        ClassInfo {
            cid: cid.into(),
            name: cid.into(),
            vendor: String::new(),
            version: String::new(),
            sub_categories: String::new(),
            params: Vec::new(),
            latency: 0,
            has_editor: true,
            instrument: false,
            sidechain: false,
            note: None,
        }
    }

    fn line(l: &ScanLine) -> String {
        format!("{SCAN_PREFIX}{}\n", serde_json::to_string(l).unwrap())
    }

    #[test]
    fn scan_output_survives_noise_and_crashes() {
        let a = ClassInfo {
            latency: 64,
            ..class("A")
        };
        // Everything read, with a plug-in's own chatter around the lines.
        let out = format!(
            "Plugin v1.0 starting\n{}{}garbage \u{fffd}\n{}",
            line(&ScanLine::Found(vec![class("A")])),
            line(&ScanLine::Read(a.clone())),
            line(&ScanLine::Done)
        );
        let r = parse_scan_output(&out, "", ScanExit::Code(0));
        assert_eq!((r.classes, r.error), (vec![a.clone()], None));

        // Died on the second: the first is kept, the second listed by name with why.
        let out = format!(
            "{}{}",
            line(&ScanLine::Found(vec![class("A"), class("B")])),
            line(&ScanLine::Read(a.clone()))
        );
        let err = format!("{STAGE_PREFIX}loading B\n{STAGE_PREFIX}switching it on\n");
        let r = parse_scan_output(&out, &err, ScanExit::Code(0xC000_0005_u32 as i32));
        assert_eq!(r.classes.len(), 2);
        assert_eq!(r.classes[0], a);
        let note = r.classes[1].note.clone().unwrap();
        assert!(
            note.contains(
                "crashed while switching it on: a memory access violation (code C0000005)"
            ),
            "{note}"
        );
        assert_eq!(r.error, None);

        // Died before naming anything: the bundle fails, with where.
        let r = parse_scan_output(
            "",
            &format!("{STAGE_PREFIX}loading its file\n"),
            ScanExit::Killed,
        );
        assert!(r.classes.is_empty());
        assert_eq!(
            r.error.as_deref(),
            Some("It crashed while loading its file.")
        );
        let r = parse_scan_output("", "", ScanExit::Code(0xC000_0135_u32 as i32));
        assert!(r.error.unwrap().contains("DLL not found"));
        let r = parse_scan_output("", "", ScanExit::TimedOut);
        assert_eq!(
            r.error.as_deref(),
            Some("It took too long and was skipped.")
        );
    }

    #[test]
    fn frames_and_audio_round_trip() {
        let mut wire = Vec::new();
        let mut body = Vec::new();
        let note = NoteEvent {
            offset: 1,
            on: true,
            pitch: 60,
            velocity: 0.75,
            channel: 3,
            pan: -0.5,
        };
        encode_audio(
            &mut body,
            4800,
            &[(7, 0.25)],
            &[note],
            &[0.5, -0.5, 1.0, 0.0],
        );
        write_frame(&mut wire, AUDIO, &body).unwrap();
        let mut r = &wire[..];
        let mut buf = Vec::new();
        assert_eq!(read_frame(&mut r, &mut buf).unwrap(), Some(AUDIO));
        let (mut ch, mut n, mut s) = (Vec::new(), Vec::new(), Vec::new());
        let mut pos = 0;
        decode_audio(&buf, &mut pos, &mut ch, &mut n, &mut s).unwrap();
        assert_eq!(pos, 4800);
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
            owner: 0,
            bypassed: false,
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
