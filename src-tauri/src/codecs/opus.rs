// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Opus decoding (`.opus`, Ogg Opus) through the pure-Rust `opus-decoder`
//! crate (MIT or Apache-2.0), wrapped as a Symphonia decoder so Opus
//! files open, play and edit like any other format. The Ogg container is
//! read by Symphonia; this only turns Opus packets into samples.

use std::sync::OnceLock;

use opus_decoder::OpusDecoder;
use symphonia::core::audio::{
    AsGenericAudioBufferRef, AudioBuffer, AudioMut, AudioSpec, GenericAudioBufferRef,
};
use symphonia::core::codecs::audio::well_known::CODEC_ID_OPUS;
use symphonia::core::codecs::audio::{
    AudioCodecParameters, AudioDecoder, AudioDecoderOptions, FinalizeResult,
};
use symphonia::core::codecs::registry::{
    CodecRegistry, RegisterableAudioDecoder, SupportedAudioCodec,
};
use symphonia::core::codecs::CodecInfo;
use symphonia::core::errors::{decode_error, unsupported_error, Result};
use symphonia::core::packet::PacketRef;

/// Opus always decodes at 48 kHz.
const OPUS_RATE: u32 = 48_000;
/// The longest Opus packet: 120 ms at 48 kHz.
const MAX_FRAMES: usize = 5_760;

pub struct OpusAdapter {
    params: AudioCodecParameters,
    decoder: OpusDecoder,
    channels: usize,
    scratch: Vec<f32>,
    buf: AudioBuffer<f32>,
}

impl OpusAdapter {
    fn try_new(params: &AudioCodecParameters) -> Result<Self> {
        if params.codec != CODEC_ID_OPUS {
            return unsupported_error("opus: invalid codec");
        }
        let channels =
            params
                .channels
                .clone()
                .ok_or(symphonia::core::errors::Error::Unsupported(
                    "opus: unknown channels",
                ))?;
        let count = channels.count();
        if count == 0 || count > 2 {
            return unsupported_error("opus: only mono and stereo are supported");
        }
        let decoder = OpusDecoder::new(OPUS_RATE, count)
            .map_err(|_| symphonia::core::errors::Error::Unsupported("opus: decoder setup"))?;
        let mut params = params.clone();
        params.with_sample_rate(OPUS_RATE);
        Ok(Self {
            params,
            decoder,
            channels: count,
            scratch: vec![0.0; MAX_FRAMES * count],
            buf: AudioBuffer::new(AudioSpec::new(OPUS_RATE, channels), MAX_FRAMES),
        })
    }
}

impl AudioDecoder for OpusAdapter {
    fn reset(&mut self) {
        self.decoder.reset();
    }

    fn codec_info(&self) -> &CodecInfo {
        &Self::supported_codecs()[0].info
    }

    fn codec_params(&self) -> &AudioCodecParameters {
        &self.params
    }

    fn decode_ref(&mut self, packet: &PacketRef<'_>) -> Result<GenericAudioBufferRef<'_>> {
        let n = match self
            .decoder
            .decode_float(packet.data, &mut self.scratch, false)
        {
            Ok(n) => n.min(MAX_FRAMES),
            Err(_) => {
                self.buf.clear();
                return decode_error("opus: bad packet");
            }
        };
        self.buf.clear();
        self.buf.render_uninit(Some(n));
        self.buf
            .copy_from_slice_interleaved(&&self.scratch[..n * self.channels]);
        Ok(self.buf.as_generic_audio_buffer_ref())
    }

    fn finalize(&mut self) -> FinalizeResult {
        FinalizeResult::default()
    }

    fn last_decoded(&self) -> GenericAudioBufferRef<'_> {
        self.buf.as_generic_audio_buffer_ref()
    }
}

impl RegisterableAudioDecoder for OpusAdapter {
    fn try_registry_new(
        params: &AudioCodecParameters,
        _opts: &AudioDecoderOptions,
    ) -> Result<Box<dyn AudioDecoder>> {
        Ok(Box::new(OpusAdapter::try_new(params)?))
    }

    fn supported_codecs() -> &'static [SupportedAudioCodec] {
        &[SupportedAudioCodec {
            id: CODEC_ID_OPUS,
            info: CodecInfo {
                short_name: "opus",
                long_name: "Opus",
                profiles: &[],
            },
        }]
    }
}

/// Every decoder the app uses: Symphonia's built-in ones plus Opus.
pub fn codecs() -> &'static CodecRegistry {
    static REGISTRY: OnceLock<CodecRegistry> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut r = CodecRegistry::new();
        symphonia::default::register_enabled_codecs(&mut r);
        r.register_audio_decoder::<OpusAdapter>();
        r
    })
}
