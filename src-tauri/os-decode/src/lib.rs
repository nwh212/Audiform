// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

//! Audio decoding through the decoders built into the operating system,
//! for what the app's own decoders cannot read: HE-AAC, Dolby Digital
//! (AC-3) and Dolby Digital Plus (E-AC-3, which carries Dolby Atmos music
//! in M4A and MP4 files), and whatever else the system can decode.
//!
//! This is its own small crate because the system decoders need `unsafe`
//! calls, and the app crate forbids `unsafe` everywhere.
//!
//! Windows: Media Foundation. Each open file gets its own worker thread,
//! which owns the system objects, so an `OsReader` can move between
//! threads freely. Other systems: `OsReader::open` returns an error that
//! says so.

use std::path::Path;

/// True where this build can decode through the system.
pub const AVAILABLE: bool = cfg!(windows);

/// Why system decoding is not available here, for the error message.
pub const UNAVAILABLE: &str = "This file needs a decoder built into Windows.";

/// What an open file holds.
#[derive(Debug, Clone, PartialEq)]
pub struct OsInfo {
    pub sample_rate: u32,
    pub channels: usize,
    /// WAVE-style speaker mask (bit 0 front left, 1 front right, 2 center,
    /// 3 LFE, 4 back left, 5 back right, ... 9 side left, 10 side right),
    /// 0 when the decoder does not say.
    pub channel_mask: u32,
    /// Length in frames, when the file states it.
    pub frames: Option<u64>,
    /// A short codec name, such as "AAC" or "Dolby Digital Plus".
    pub codec: String,
}

/// Streams a file's audio as interleaved 32-bit float.
pub struct OsReader {
    info: OsInfo,
    #[cfg(windows)]
    worker: windows_impl::Worker,
}

impl OsReader {
    pub fn open(path: &Path) -> Result<Self, String> {
        #[cfg(windows)]
        {
            let (worker, info) = windows_impl::Worker::open(path)?;
            Ok(Self { info, worker })
        }
        #[cfg(not(windows))]
        {
            let _ = path;
            Err(UNAVAILABLE.into())
        }
    }

    pub fn info(&self) -> &OsInfo {
        &self.info
    }

    /// Appends the next block of interleaved samples to `out`. Returns
    /// false at the end of the file.
    pub fn read(&mut self, out: &mut Vec<f32>) -> Result<bool, String> {
        #[cfg(windows)]
        {
            self.worker.read(out)
        }
        #[cfg(not(windows))]
        {
            let _ = out;
            Err(UNAVAILABLE.into())
        }
    }

    /// Moves to `frame`; the next read starts exactly there.
    pub fn seek(&mut self, frame: u64) -> Result<(), String> {
        #[cfg(windows)]
        {
            self.worker.seek(frame)
        }
        #[cfg(not(windows))]
        {
            let _ = frame;
            Err(UNAVAILABLE.into())
        }
    }
}

#[cfg(windows)]
mod windows_impl {
    use super::OsInfo;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use std::sync::mpsc::{channel, Receiver, Sender};

    use windows::core::{GUID, PCWSTR};
    use windows::Win32::Media::MediaFoundation::*;
    use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};

    const FIRST_AUDIO: u32 = MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32;
    const ALL_STREAMS: u32 = MF_SOURCE_READER_ALL_STREAMS.0 as u32;
    const MEDIA_SOURCE: u32 = MF_SOURCE_READER_MEDIASOURCE.0 as u32;

    fn err(what: &str, e: windows::core::Error) -> String {
        format!("system decoder ({what}): {e}")
    }

    enum Request {
        Read,
        Seek(u64),
    }

    enum Reply {
        Samples(Vec<f32>),
        End,
        Done,
        Failed(String),
    }

    pub struct Worker {
        requests: Sender<Request>,
        replies: Receiver<Reply>,
    }

    impl Worker {
        pub fn open(path: &Path) -> Result<(Self, OsInfo), String> {
            let path = path.to_path_buf();
            let (req_tx, req_rx) = channel::<Request>();
            let (rep_tx, rep_rx) = channel::<Reply>();
            let (info_tx, info_rx) = channel::<Result<OsInfo, String>>();
            std::thread::Builder::new()
                .name("system decoder".into())
                .spawn(move || run(&path, info_tx, req_rx, rep_tx))
                .map_err(|e| format!("system decoder: {e}"))?;
            let info = info_rx
                .recv()
                .map_err(|_| "system decoder stopped".to_string())??;
            Ok((
                Self {
                    requests: req_tx,
                    replies: rep_rx,
                },
                info,
            ))
        }

        fn ask(&self, r: Request) -> Result<Reply, String> {
            self.requests
                .send(r)
                .map_err(|_| "system decoder stopped".to_string())?;
            self.replies
                .recv()
                .map_err(|_| "system decoder stopped".to_string())
        }

        pub fn read(&mut self, out: &mut Vec<f32>) -> Result<bool, String> {
            match self.ask(Request::Read)? {
                Reply::Samples(s) => {
                    out.extend_from_slice(&s);
                    Ok(true)
                }
                Reply::End => Ok(false),
                Reply::Done => Ok(true),
                Reply::Failed(e) => Err(e),
            }
        }

        pub fn seek(&mut self, frame: u64) -> Result<(), String> {
            match self.ask(Request::Seek(frame))? {
                Reply::Failed(e) => Err(e),
                _ => Ok(()),
            }
        }
    }

    /// The worker thread: owns COM, Media Foundation and the reader.
    fn run(
        path: &Path,
        info_tx: Sender<Result<OsInfo, String>>,
        requests: Receiver<Request>,
        replies: Sender<Reply>,
    ) {
        unsafe {
            let com = CoInitializeEx(None, COINIT_MULTITHREADED).is_ok();
            if let Err(e) = MFStartup(MF_VERSION, MFSTARTUP_LITE) {
                let _ = info_tx.send(Err(err("start", e)));
                if com {
                    CoUninitialize();
                }
                return;
            }
            match Decoder::open(path) {
                Ok(mut d) => {
                    let _ = info_tx.send(Ok(d.info.clone()));
                    while let Ok(r) = requests.recv() {
                        let reply = match r {
                            Request::Read => match d.read() {
                                Ok(Some(s)) => Reply::Samples(s),
                                Ok(None) => Reply::End,
                                Err(e) => Reply::Failed(e),
                            },
                            Request::Seek(f) => match d.seek(f) {
                                Ok(()) => Reply::Done,
                                Err(e) => Reply::Failed(e),
                            },
                        };
                        if replies.send(reply).is_err() {
                            break;
                        }
                    }
                    drop(d);
                }
                Err(e) => {
                    let _ = info_tx.send(Err(e));
                }
            }
            let _ = MFShutdown();
            if com {
                CoUninitialize();
            }
        }
    }

    struct Decoder {
        reader: IMFSourceReader,
        info: OsInfo,
        /// Frames still to drop after a seek, which lands at or before
        /// the frame asked for.
        skip_to: Option<u64>,
    }

    fn codec_name(subtype: &GUID) -> String {
        let names: [(&GUID, &str); 9] = [
            (&MFAudioFormat_AAC, "AAC"),
            (&MFAudioFormat_Dolby_AC3, "Dolby Digital"),
            (&MFAudioFormat_Dolby_DDPlus, "Dolby Digital Plus"),
            (&MFAudioFormat_Dolby_AC4, "Dolby AC-4"),
            (&MFAudioFormat_ALAC, "ALAC"),
            (&MFAudioFormat_FLAC, "FLAC"),
            (&MFAudioFormat_MP3, "MP3"),
            (&MFAudioFormat_Opus, "Opus"),
            (&MFAudioFormat_WMAudioV9, "WMA"),
        ];
        names
            .iter()
            .find(|(g, _)| **g == *subtype)
            .map(|(_, n)| n.to_string())
            .unwrap_or_else(|| "Audio".into())
    }

    impl Decoder {
        unsafe fn open(path: &Path) -> Result<Self, String> {
            let wide: Vec<u16> = path
                .as_os_str()
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            let reader = MFCreateSourceReaderFromURL(PCWSTR(wide.as_ptr()), None)
                .map_err(|e| err("open", e))?;
            let _ = reader.SetStreamSelection(ALL_STREAMS, false);
            reader
                .SetStreamSelection(FIRST_AUDIO, true)
                .map_err(|e| err("no audio", e))?;

            let native = reader
                .GetNativeMediaType(FIRST_AUDIO, 0)
                .map_err(|e| err("format", e))?;
            let subtype = native.GetGUID(&MF_MT_SUBTYPE).unwrap_or_default();
            let native_channels = native.GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS).unwrap_or(0);

            // Ask for float samples with every channel the file has, so a
            // decoder that would mix down to stereo by default does not.
            let want = |channels: u32| -> windows::core::Result<()> {
                let t = MFCreateMediaType()?;
                t.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
                t.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_Float)?;
                if channels > 0 {
                    t.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, channels)?;
                }
                reader.SetCurrentMediaType(FIRST_AUDIO, None, &t)
            };
            if native_channels == 0 || want(native_channels).is_err() {
                want(0).map_err(|e| err("float output", e))?;
            }
            let _ = reader.SetStreamSelection(FIRST_AUDIO, true);

            let mut d = Self {
                reader,
                info: OsInfo {
                    sample_rate: 0,
                    channels: 0,
                    channel_mask: 0,
                    frames: None,
                    codec: codec_name(&subtype),
                },
                skip_to: None,
            };
            d.read_type()?;

            if let Ok(v) = d
                .reader
                .GetPresentationAttribute(MEDIA_SOURCE, &MF_PD_DURATION)
            {
                if let Ok(t) = u64::try_from(&v) {
                    let frames =
                        (u128::from(t) * u128::from(d.info.sample_rate) / 10_000_000) as u64;
                    d.info.frames = (frames > 0).then_some(frames);
                }
            }
            Ok(d)
        }

        /// Reads the output format the decoder settled on.
        unsafe fn read_type(&mut self) -> Result<(), String> {
            let t = self
                .reader
                .GetCurrentMediaType(FIRST_AUDIO)
                .map_err(|e| err("format", e))?;
            let bits = t.GetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE).unwrap_or(32);
            let sub = t.GetGUID(&MF_MT_SUBTYPE).unwrap_or_default();
            if sub != MFAudioFormat_Float || bits != 32 {
                return Err("system decoder: no float output".into());
            }
            self.info.sample_rate = t
                .GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND)
                .map_err(|e| err("format", e))?;
            self.info.channels = t
                .GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS)
                .map_err(|e| err("format", e))? as usize;
            self.info.channel_mask = t.GetUINT32(&MF_MT_AUDIO_CHANNEL_MASK).unwrap_or(0);
            if self.info.sample_rate == 0 || self.info.channels == 0 {
                return Err("system decoder: no audio".into());
            }
            Ok(())
        }

        /// The next block of samples, None at the end.
        unsafe fn read(&mut self) -> Result<Option<Vec<f32>>, String> {
            loop {
                let mut flags = 0u32;
                let mut time = 0i64;
                let mut sample: Option<IMFSample> = None;
                self.reader
                    .ReadSample(
                        FIRST_AUDIO,
                        0,
                        None,
                        Some(&mut flags),
                        Some(&mut time),
                        Some(&mut sample),
                    )
                    .map_err(|e| err("read", e))?;
                if flags & MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED.0 as u32 != 0 {
                    let (rate, ch) = (self.info.sample_rate, self.info.channels);
                    self.read_type()?;
                    if (rate, ch) != (self.info.sample_rate, self.info.channels) {
                        return Err("system decoder: the format changed mid-file".into());
                    }
                }
                let Some(sample) = sample else {
                    if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                        return Ok(None);
                    }
                    continue;
                };
                let buf = sample
                    .ConvertToContiguousBuffer()
                    .map_err(|e| err("read", e))?;
                let mut ptr: *mut u8 = std::ptr::null_mut();
                let mut len = 0u32;
                buf.Lock(&mut ptr, None, Some(&mut len))
                    .map_err(|e| err("read", e))?;
                let n = len as usize / 4;
                let mut out = vec![0f32; n];
                std::ptr::copy_nonoverlapping(ptr as *const f32, out.as_mut_ptr(), n);
                let _ = buf.Unlock();

                // After a seek, drop what comes before the frame asked for.
                if let Some(target) = self.skip_to {
                    let ch = self.info.channels;
                    let start = (i128::from(time.max(0)) * i128::from(self.info.sample_rate)
                        / 10_000_000) as u64;
                    let frames = (out.len() / ch) as u64;
                    if start + frames <= target {
                        continue;
                    }
                    self.skip_to = None;
                    if start < target {
                        out.drain(..(target - start) as usize * ch);
                    }
                }
                if out.is_empty() {
                    continue;
                }
                return Ok(Some(out));
            }
        }

        unsafe fn seek(&mut self, frame: u64) -> Result<(), String> {
            let t = (u128::from(frame) * 10_000_000 / u128::from(self.info.sample_rate)) as i64;
            let pos = PROPVARIANT::from(t);
            self.reader
                .SetCurrentPosition(&GUID::zeroed(), &pos)
                .map_err(|e| err("seek", e))?;
            self.skip_to = Some(frame);
            Ok(())
        }
    }
}
