// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! AAC (M4A) writing through the encoder built into the operating system.
//!
//! This is its own small crate because talking to the system encoder
//! needs `unsafe` calls, and the app crate forbids `unsafe` everywhere.
//! Keeping it here means that rule still holds for the app itself.
//!
//! Windows: Media Foundation. Other systems: `AacWriter::create` returns
//! an error that says so.

/// True where this build can write AAC.
pub const AVAILABLE: bool = cfg!(windows);

/// Why AAC is not available here, for the error message.
pub const UNAVAILABLE: &str =
    "AAC export uses the encoder built into Windows. On this system, export MP3 or FLAC instead.";

#[cfg(windows)]
pub use windows_impl::AacWriter;

#[cfg(not(windows))]
pub struct AacWriter;

#[cfg(not(windows))]
impl AacWriter {
    pub fn create(
        _path: &std::path::Path,
        _rate: u32,
        _channels: u16,
        _kbps: u16,
    ) -> Result<Self, String> {
        Err(UNAVAILABLE.into())
    }
    pub fn write(&mut self, _samples: &[i16]) -> Result<(), String> {
        Err(UNAVAILABLE.into())
    }
    pub fn finish(self) -> Result<(), String> {
        Err(UNAVAILABLE.into())
    }
}

#[cfg(windows)]
mod windows_impl {
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    use windows::core::{GUID, PCWSTR};
    use windows::Win32::Media::MediaFoundation::{
        IMFAttributes, IMFByteStream, IMFMediaType, IMFSinkWriter, MFAudioFormat_AAC,
        MFAudioFormat_PCM, MFCreateAttributes, MFCreateMediaType, MFCreateMemoryBuffer,
        MFCreateSample, MFCreateSinkWriterFromURL, MFMediaType_Audio, MFShutdown, MFStartup,
        MFTranscodeContainerType_MPEG4, MFSTARTUP_LITE, MF_MT_AUDIO_AVG_BYTES_PER_SECOND,
        MF_MT_AUDIO_BITS_PER_SAMPLE, MF_MT_AUDIO_BLOCK_ALIGNMENT, MF_MT_AUDIO_NUM_CHANNELS,
        MF_MT_AUDIO_SAMPLES_PER_SECOND, MF_MT_MAJOR_TYPE, MF_MT_SUBTYPE,
        MF_TRANSCODE_CONTAINERTYPE, MF_VERSION,
    };
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};

    fn err(what: &str, e: windows::core::Error) -> String {
        format!("AAC encoder ({what}): {e}")
    }

    pub struct AacWriter {
        writer: IMFSinkWriter,
        stream: u32,
        rate: u32,
        channels: u16,
        frames: u64,
        com: bool,
        started: bool,
    }

    unsafe fn audio_type(
        subtype: &GUID,
        rate: u32,
        channels: u16,
        avg_bytes: u32,
    ) -> Result<IMFMediaType, String> {
        let t = MFCreateMediaType().map_err(|e| err("media type", e))?;
        let a: &IMFAttributes = &t;
        let set = |k: &GUID, v: u32| unsafe { a.SetUINT32(k, v) }.map_err(|e| err("media type", e));
        a.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)
            .map_err(|e| err("media type", e))?;
        a.SetGUID(&MF_MT_SUBTYPE, subtype)
            .map_err(|e| err("media type", e))?;
        set(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
        set(&MF_MT_AUDIO_SAMPLES_PER_SECOND, rate)?;
        set(&MF_MT_AUDIO_NUM_CHANNELS, u32::from(channels))?;
        set(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, avg_bytes)?;
        if *subtype == MFAudioFormat_PCM {
            set(&MF_MT_AUDIO_BLOCK_ALIGNMENT, u32::from(channels) * 2)?;
        }
        Ok(t)
    }

    impl AacWriter {
        pub fn create(path: &Path, rate: u32, channels: u16, kbps: u16) -> Result<Self, String> {
            unsafe {
                // Fine if COM is already set up on this thread in either mode.
                let com = CoInitializeEx(None, COINIT_MULTITHREADED).is_ok();
                let mut w = Self {
                    // Filled below; on failure Drop cleans up what started.
                    writer: {
                        if let Err(e) = MFStartup(MF_VERSION, MFSTARTUP_LITE) {
                            if com {
                                CoUninitialize();
                            }
                            return Err(err("start", e));
                        }
                        let wide: Vec<u16> = path
                            .as_os_str()
                            .encode_wide()
                            .chain(std::iter::once(0))
                            .collect();
                        // Ask for an MPEG-4 (M4A) file outright rather than relying on the extension.
                        let mut attrs: Option<IMFAttributes> = None;
                        let made = MFCreateAttributes(&mut attrs, 1);
                        if let (Ok(()), Some(a)) = (made, attrs.as_ref()) {
                            let _ = a.SetGUID(
                                &MF_TRANSCODE_CONTAINERTYPE,
                                &MFTranscodeContainerType_MPEG4,
                            );
                        }
                        match MFCreateSinkWriterFromURL(
                            PCWSTR(wide.as_ptr()),
                            None::<&IMFByteStream>,
                            attrs.as_ref(),
                        ) {
                            Ok(w) => w,
                            Err(e) => {
                                let _ = MFShutdown();
                                if com {
                                    CoUninitialize();
                                }
                                return Err(err("create file", e));
                            }
                        }
                    },
                    stream: 0,
                    rate,
                    channels,
                    frames: 0,
                    com,
                    started: true,
                };
                let out = audio_type(
                    &MFAudioFormat_AAC,
                    rate,
                    channels,
                    u32::from(kbps) * 1000 / 8,
                )?;
                w.stream = w
                    .writer
                    .AddStream(&out)
                    .map_err(|e| err("output format", e))?;
                let input = audio_type(
                    &MFAudioFormat_PCM,
                    rate,
                    channels,
                    rate * u32::from(channels) * 2,
                )?;
                w.writer
                    .SetInputMediaType(w.stream, &input, None::<&IMFAttributes>)
                    .map_err(|e| err("input format", e))?;
                w.writer.BeginWriting().map_err(|e| err("begin", e))?;
                Ok(w)
            }
        }

        /// Adds interleaved 16-bit samples.
        pub fn write(&mut self, samples: &[i16]) -> Result<(), String> {
            if samples.is_empty() {
                return Ok(());
            }
            let bytes = samples.len() * 2;
            let frames = (samples.len() / usize::from(self.channels)) as u64;
            unsafe {
                let buf = MFCreateMemoryBuffer(bytes as u32).map_err(|e| err("buffer", e))?;
                let mut ptr: *mut u8 = std::ptr::null_mut();
                buf.Lock(&mut ptr, None, None)
                    .map_err(|e| err("buffer", e))?;
                std::ptr::copy_nonoverlapping(samples.as_ptr() as *const u8, ptr, bytes);
                buf.Unlock().map_err(|e| err("buffer", e))?;
                buf.SetCurrentLength(bytes as u32)
                    .map_err(|e| err("buffer", e))?;
                let sample = MFCreateSample().map_err(|e| err("sample", e))?;
                sample.AddBuffer(&buf).map_err(|e| err("sample", e))?;
                // Times are in 100-nanosecond units.
                let t = |f: u64| (f as i128 * 10_000_000 / i128::from(self.rate)) as i64;
                sample
                    .SetSampleTime(t(self.frames))
                    .map_err(|e| err("sample", e))?;
                sample
                    .SetSampleDuration(t(self.frames + frames) - t(self.frames))
                    .map_err(|e| err("sample", e))?;
                self.writer
                    .WriteSample(self.stream, &sample)
                    .map_err(|e| err("write", e))?;
            }
            self.frames += frames;
            Ok(())
        }

        pub fn finish(mut self) -> Result<(), String> {
            let r = unsafe { self.writer.Finalize() }.map_err(|e| err("finish", e));
            self.shutdown();
            r
        }

        fn shutdown(&mut self) {
            if self.started {
                self.started = false;
                unsafe {
                    let _ = MFShutdown();
                    if self.com {
                        CoUninitialize();
                    }
                }
            }
        }
    }

    impl Drop for AacWriter {
        fn drop(&mut self) {
            self.shutdown();
        }
    }
}
