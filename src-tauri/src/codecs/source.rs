// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

//! Opening a file for the decoder.
//!
//! Most files are handed over as they are. A MOGG (a multitrack Ogg file,
//! as rhythm games ship their songs) is an Ogg Vorbis stream, often with
//! many channels, behind a small header: a version number, then where
//! the Ogg data starts. Version 10 files are plain and play once the
//! header is skipped, and some MOGG files are plain Ogg with no header.
//! Later versions are encrypted and are refused with
//! a clear reason; Audiform does not break their encryption.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use symphonia::core::io::{MediaSource, MediaSourceStream};

use super::ProbeError;

/// The MOGG version whose audio is not encrypted.
const PLAIN_MOGG: u32 = 10;

/// A file read from `start` on, as if it began there.
struct Offset {
    file: File,
    start: u64,
    len: u64,
}

impl Read for Offset {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.file.read(buf)
    }
}

impl Seek for Offset {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let at = match pos {
            SeekFrom::Start(p) => self.file.seek(SeekFrom::Start(self.start + p))?,
            SeekFrom::Current(d) => self.file.seek(SeekFrom::Current(d))?,
            SeekFrom::End(d) => self.file.seek(SeekFrom::End(d))?,
        };
        Ok(at.saturating_sub(self.start))
    }
}

impl MediaSource for Offset {
    fn is_seekable(&self) -> bool {
        true
    }

    fn byte_len(&self) -> Option<u64> {
        Some(self.len)
    }
}

/// True for a `.mogg` file name.
pub fn is_mogg(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("mogg"))
}

/// Where a MOGG's Ogg data starts, read from its header.
pub fn mogg_start(file: &mut File) -> Result<u64, ProbeError> {
    let mut head = [0u8; 8];
    file.seek(SeekFrom::Start(0))?;
    file.read_exact(&mut head)
        .map_err(|_| ProbeError::Damaged("the MOGG header is cut short".into()))?;
    // Some MOGG files are plain Ogg with no header at all.
    if &head[..4] == b"OggS" {
        return Ok(0);
    }
    let version = u32::from_le_bytes([head[0], head[1], head[2], head[3]]);
    let start = u64::from(u32::from_le_bytes([head[4], head[5], head[6], head[7]]));
    if version != PLAIN_MOGG {
        return Err(ProbeError::UnsupportedCodec(format!(
            "this MOGG is encrypted (version {version}); only unencrypted MOGG files (version 10) can be opened"
        )));
    }
    let mut magic = [0u8; 4];
    file.seek(SeekFrom::Start(start))?;
    file.read_exact(&mut magic)
        .map_err(|_| ProbeError::Damaged("the MOGG header points past the end".into()))?;
    if &magic != b"OggS" {
        return Err(ProbeError::Damaged(
            "no Ogg audio where the MOGG header says".into(),
        ));
    }
    Ok(start)
}

/// The file as the decoder should read it, and the extension to hint with.
pub fn open(path: &Path) -> Result<(MediaSourceStream<'static>, Option<String>), ProbeError> {
    let mut file = File::open(path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => ProbeError::NotFound,
        _ => ProbeError::Io(e),
    })?;
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_string);
    if is_mogg(path) {
        let start = mogg_start(&mut file)?;
        let total = file.metadata()?.len();
        file.seek(SeekFrom::Start(start))?;
        let src = Offset {
            file,
            start,
            len: total.saturating_sub(start),
        };
        return Ok((
            MediaSourceStream::new(Box::new(src), Default::default()),
            Some("ogg".into()),
        ));
    }
    Ok((
        MediaSourceStream::new(Box::new(file), Default::default()),
        ext,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn mogg(version: u32, ogg: &[u8]) -> tempfile::NamedTempFile {
        let mut f = tempfile::Builder::new().suffix(".mogg").tempfile().unwrap();
        let start: u32 = 8 + 16;
        f.write_all(&version.to_le_bytes()).unwrap();
        f.write_all(&start.to_le_bytes()).unwrap();
        f.write_all(&[0u8; 16]).unwrap();
        f.write_all(ogg).unwrap();
        f.flush().unwrap();
        f
    }

    #[test]
    fn plain_moggs_start_at_their_ogg_data() {
        let f = mogg(10, b"OggS rest of the stream");
        let mut file = File::open(f.path()).unwrap();
        assert_eq!(mogg_start(&mut file).unwrap(), 24);
        let (mut mss, hint) = open(f.path()).unwrap();
        assert_eq!(hint.as_deref(), Some("ogg"));
        let mut first = [0u8; 4];
        mss.read_exact(&mut first).unwrap();
        assert_eq!(&first, b"OggS");
    }

    #[test]
    fn headerless_moggs_are_plain_ogg() {
        let mut f = tempfile::Builder::new().suffix(".mogg").tempfile().unwrap();
        f.write_all(b"OggS\0\x02rest").unwrap();
        f.flush().unwrap();
        let mut file = File::open(f.path()).unwrap();
        assert_eq!(mogg_start(&mut file).unwrap(), 0);
    }

    #[test]
    fn moggs_with_more_than_eight_channels_open() {
        // A headerless MOGG of 10 channels, as many rhythm-game songs are.
        let ogg = std::fs::read(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/media/ten_channels.ogg"),
        )
        .unwrap();
        let mut f = tempfile::Builder::new().suffix(".mogg").tempfile().unwrap();
        f.write_all(&ogg).unwrap();
        f.flush().unwrap();
        let info = crate::codecs::probe(f.path()).unwrap();
        assert_eq!(info.channels, 10);
        let mut r = crate::codecs::AudioReader::open(f.path(), 10, None).unwrap();
        let mut buf = Vec::new();
        while buf.len() < 48_000 && r.read_into(&mut buf).unwrap() {}
        // Every channel carries its own tone.
        for ch in 0..10 {
            assert!(
                buf.iter().skip(ch).step_by(10).any(|v| v.abs() > 0.1),
                "channel {ch}"
            );
        }
    }

    #[test]
    fn encrypted_and_broken_moggs_say_why() {
        let f = mogg(11, b"OggS");
        let mut file = File::open(f.path()).unwrap();
        let e = mogg_start(&mut file).unwrap_err().to_string();
        assert!(e.contains("encrypted"), "{e}");
        let f = mogg(10, b"nope");
        let mut file = File::open(f.path()).unwrap();
        assert!(matches!(mogg_start(&mut file), Err(ProbeError::Damaged(_))));
    }

    #[test]
    fn a_plain_mogg_decodes_like_its_ogg() {
        let ogg = std::fs::read(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/media/tone.ogg"),
        )
        .unwrap();
        let f = mogg(10, &ogg);
        let info = crate::codecs::probe(f.path()).unwrap();
        let plain = crate::codecs::probe(
            &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/media/tone.ogg"),
        )
        .unwrap();
        assert_eq!(
            (info.sample_rate, info.channels),
            (plain.sample_rate, plain.channels)
        );
        let mut r = crate::codecs::AudioReader::open(f.path(), 2, None).unwrap();
        let mut buf = Vec::new();
        while buf.len() < 48_000 && r.read_into(&mut buf).unwrap() {}
        assert!(buf.iter().any(|v| v.abs() > 0.01));
    }
}
