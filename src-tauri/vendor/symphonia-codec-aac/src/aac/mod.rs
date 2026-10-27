// Symphonia
// Copyright (c) 2019-2026 The Project Symphonia Developers.
//
// Previous Author: Kostya Shishkov <kostya.shiskov@gmail.com>
//
// This source file includes code originally written for the NihAV
// project. With the author's permission, it has been relicensed for,
// and ported to the Symphonia project.
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use symphonia_core::audio::{
    AsGenericAudioBufferRef, AudioBuffer, AudioSpec, Channels, GenericAudioBufferRef, Position,
};
use symphonia_core::codecs::CodecInfo;
use symphonia_core::codecs::audio::well_known::CODEC_ID_AAC;
use symphonia_core::codecs::audio::{AudioCodecParameters, AudioDecoderOptions};
use symphonia_core::codecs::audio::{AudioDecoder, FinalizeResult};
use symphonia_core::codecs::registry::{RegisterableAudioDecoder, SupportedAudioCodec};
use symphonia_core::errors::{Result, decode_error, unsupported_error};
use symphonia_core::io::{BitReaderLtr, FiniteBitStream, ReadBitsLtr};
use symphonia_core::packet::PacketRef;
use symphonia_core::{codec_profile, support_audio_codec};

use symphonia_common::mpeg::audio::{AudioObjectType, AudioSpecificConfig};

mod codebooks;
mod common;
mod cpe;
mod dsp;
mod ics;
mod window;

use common::*;

/// Advanced Audio Coding (AAC) decoder.
///
/// Implements a decoder for Advanced Audio Decoding Low-Complexity (AAC-LC) as defined in
/// ISO/IEC 13818-7 and ISO/IEC 14496-3.
pub struct AacDecoder {
    // info: NACodecInfoRef,
    asc: AudioSpecificConfig,
    pairs: Vec<cpe::ChannelPair>,
    dsp: dsp::Dsp,
    sbinfo: GASubbandInfo,
    params: AudioCodecParameters,
    buf: AudioBuffer<f32>,
    /// For each channel in bitstream (element) order, the plane it is written to. (Audiform)
    order: Vec<usize>,
    /// With a program config element: each element's kind and tag, and its planes. (Audiform)
    tagged: Vec<((u32, u32), (usize, usize))>,
    /// The planes of each element decoded in this frame, in order. (Audiform)
    pair_planes: Vec<(usize, usize)>,
}

/// The speaker of each channel in the order the elements carry them, for the layouts of
/// channel configurations 1 to 7 (ISO/IEC 14496-3, table 1.19). (Audiform)
const ELEMENT_ORDERS: &[&[Position]] = &[
    &[Position::FRONT_CENTER],
    &[Position::FRONT_LEFT, Position::FRONT_RIGHT],
    &[Position::FRONT_CENTER, Position::FRONT_LEFT, Position::FRONT_RIGHT],
    &[Position::FRONT_CENTER, Position::FRONT_LEFT, Position::FRONT_RIGHT, Position::REAR_CENTER],
    &[
        Position::FRONT_CENTER,
        Position::FRONT_LEFT,
        Position::FRONT_RIGHT,
        Position::REAR_LEFT,
        Position::REAR_RIGHT,
    ],
    &[
        Position::FRONT_CENTER,
        Position::FRONT_LEFT,
        Position::FRONT_RIGHT,
        Position::REAR_LEFT,
        Position::REAR_RIGHT,
        Position::LFE1,
    ],
    // Configuration 7: 7.1 with side and rear pairs (see symphonia-common).
    &[
        Position::FRONT_CENTER,
        Position::FRONT_LEFT,
        Position::FRONT_RIGHT,
        Position::SIDE_LEFT,
        Position::SIDE_RIGHT,
        Position::REAR_LEFT,
        Position::REAR_RIGHT,
        Position::LFE1,
    ],
];

/// Maps bitstream channel order to buffer planes, which follow the speaker positions' bit
/// order. None for a layout no channel configuration describes. (Audiform)
fn element_order(channels: &Channels, from_pce: &[Position]) -> Option<Vec<usize>> {
    // Numbered channels (a program config this cannot place) stay in element order.
    if let Channels::Discrete(n) = channels {
        return Some((0..usize::from(*n)).collect());
    }
    let Channels::Positioned(mask) = channels else { return None };
    let order: &[Position] = if from_pce.is_empty() {
        ELEMENT_ORDERS.iter().find(|order| {
            order.iter().fold(Position::empty(), |acc, p| acc | *p) == *mask
        })?
    }
    else {
        from_pce
    };
    Some(
        order
            .iter()
            .map(|p| (mask.bits() & (p.bits() - 1)).count_ones() as usize)
            .collect(),
    )
}

impl AacDecoder {
    pub fn try_new(params: &AudioCodecParameters, _opts: &AudioDecoderOptions) -> Result<Self> {
        // This decoder only supports AAC.
        if params.codec != CODEC_ID_AAC {
            return unsupported_error("aac: invalid codec");
        }

        // If extra data present, parse the audio specific config
        let asc = if let Some(extra_data_buf) = &params.extra_data {
            validate!(extra_data_buf.len() >= 2);
            AudioSpecificConfig::read(extra_data_buf)?
        }
        else {
            // Otherwise, assume there is no ASC and use the codec parameters for ADTS.
            let mut asc = AudioSpecificConfig::default();

            asc.object_type = AudioObjectType::Lc;
            asc.samples = 1024;

            asc.sample_rate = match params.sample_rate {
                Some(rate) => rate,
                None => return unsupported_error("aac: sample rate is required"),
            };

            asc.channels = params.channels.clone();

            asc
        };

        // The channel configuration must be known.
        //
        // TODO: Support getting this from program configuration element (PCE). However, this would
        // require deferring the rest of the initialization until the PCE has been read.
        let channels = match &asc.channels {
            Some(channels) => channels.clone(),
            _ => return unsupported_error("aac: channels or channel layout is required"),
        };

        // Check complexity. Audiform: any layout of channel configurations 1 to 7.
        let order = match element_order(&channels, &asc.element_positions) {
            Some(order) => order,
            None => return unsupported_error("aac: aac too complex"),
        };
        if asc.sbr_present {
            return unsupported_error("HE-AAC (AAC with SBR)");
        }
        if asc.object_type != AudioObjectType::Lc || asc.samples != 1024 {
            return unsupported_error("aac: aac too complex");
        }

        // Clone and amend the codec parameters with information from the extra data.
        let mut params = params.clone();

        params.with_channels(channels.clone()).with_sample_rate(asc.sample_rate);

        let sbinfo = GASubbandInfo::find(asc.sample_rate);

        let buf = AudioBuffer::new(AudioSpec::new(asc.sample_rate, channels), asc.samples);

        // Audiform: with a program config element, place elements by kind and tag.
        let mut tagged = Vec::new();
        let mut ch = 0;
        for e in &asc.pce_elements {
            let a = order.get(ch).copied().unwrap_or(0);
            let b = order.get(ch + 1).copied().unwrap_or(0);
            tagged.push(((u32::from(e.id), u32::from(e.tag)), (a, if e.channels == 2 { b } else { 0 })));
            ch += usize::from(e.channels);
        }

        Ok(AacDecoder {
            asc,
            pairs: Vec::new(),
            dsp: dsp::Dsp::new(),
            sbinfo,
            params,
            buf,
            order,
            tagged,
            pair_planes: Vec::new(),
        })
    }

    fn set_pair(&mut self, pair_no: usize, channel: usize, pair: bool) -> Result<()> {
        if self.pairs.len() <= pair_no {
            self.pairs.push(cpe::ChannelPair::new(pair, channel, self.sbinfo));
        }
        else {
            validate!(self.pairs[pair_no].channel == channel);
            validate!(self.pairs[pair_no].is_pair == pair);
        }

        let max_channels = self.asc.channels.as_ref().map_or(0, |channels| channels.count());
        validate!(if pair { channel + 1 } else { channel } < max_channels);

        Ok(())
    }

    /// Audiform: notes the planes of the element decoded at `pair_no`: by kind and tag when a
    /// program config element gave the layout, otherwise by its place in the frame.
    fn place(&mut self, pair_no: usize, channel: usize, key: (u32, u32)) -> Result<()> {
        let planes = if self.tagged.is_empty() {
            match self.order.get(channel) {
                Some(&a) => (a, self.order.get(channel + 1).copied().unwrap_or(0)),
                None => return decode_error("aac: more channels than the layout"),
            }
        }
        else {
            match self.tagged.iter().find(|(k, _)| *k == key) {
                Some(&(_, planes)) => planes,
                None => return decode_error("aac: element not in the program config"),
            }
        };
        if self.pair_planes.len() <= pair_no {
            self.pair_planes.resize(pair_no + 1, (0, 0));
        }
        self.pair_planes[pair_no] = planes;
        Ok(())
    }

    fn decode_ga<B: ReadBitsLtr + FiniteBitStream>(&mut self, bs: &mut B) -> Result<()> {
        let mut cur_pair = 0;
        let mut cur_ch = 0;
        while bs.bits_left() > 3 {
            let id = bs.read_bits_leq32(3)?;

            match id {
                0 => {
                    // ID_SCE
                    let tag = bs.read_bits_leq32(4)?;
                    self.place(cur_pair, cur_ch, (0, tag))?;
                    self.set_pair(cur_pair, cur_ch, false)?;
                    self.pairs[cur_pair].decode_ga_sce(bs, self.asc.object_type)?;
                    cur_pair += 1;
                    cur_ch += 1;
                }
                1 => {
                    // ID_CPE
                    let tag = bs.read_bits_leq32(4)?;
                    self.place(cur_pair, cur_ch, (1, tag))?;
                    self.set_pair(cur_pair, cur_ch, true)?;
                    self.pairs[cur_pair].decode_ga_cpe(bs, self.asc.object_type)?;
                    cur_pair += 1;
                    cur_ch += 2;
                }
                2 => {
                    // ID_CCE
                    return unsupported_error("aac: coupling channel element");
                }
                3 => {
                    // ID_LFE
                    let tag = bs.read_bits_leq32(4)?;
                    self.place(cur_pair, cur_ch, (3, tag))?;
                    self.set_pair(cur_pair, cur_ch, false)?;
                    self.pairs[cur_pair].decode_ga_sce(bs, self.asc.object_type)?;
                    cur_pair += 1;
                    cur_ch += 1;
                }
                4 => {
                    // ID_DSE
                    let _id = bs.read_bits_leq32(4)?;
                    let align = bs.read_bool()?;
                    let mut count = bs.read_bits_leq32(8)?;
                    if count == 255 {
                        count += bs.read_bits_leq32(8)?;
                    }
                    if align {
                        bs.realign(); // ????
                    }
                    bs.ignore_bits(count * 8)?; // no SBR payload or such
                }
                5 => {
                    // ID_PCE. Audiform: the layout comes from the audio specific config; an
                    // element repeated in the stream is read past.
                    symphonia_common::mpeg::audio::read_program_config(bs)?;
                }
                6 => {
                    // ID_FIL
                    let mut count = bs.read_bits_leq32(4)? as usize;
                    if count == 15 {
                        count += bs.read_bits_leq32(8)? as usize;
                        count -= 1;
                    }

                    // Check if the ID_FIL element contains SBR data. Note that ID_FIL elements with
                    // SBR data may not contain other extension payloads.
                    if count > 0 {
                        let ext_type = bs.read_bits_leq32(4)?;

                        match ext_type {
                            // EXT_SBR_DATA (0xd)
                            // EXT_SBR_DATA_CRC (0xe)
                            // Audiform: HE-AAC that signals itself only here. Decoding just
                            // the core gives half the bandwidth at half the output rate:
                            // refuse it, so the app can hand the file to a decoder that
                            // has SBR.
                            0xd | 0xe => return unsupported_error("HE-AAC (AAC with SBR)"),
                            // EXT_FILL (0x0)
                            // EXT_FILL_DATA (0x1)
                            // EXT_DATA_ELEMENT (0x2)
                            // EXT_DYNAMIC_RANGE (0xb)
                            // EXT_SAC_DATA (0xc)
                            _ => (),
                        }

                        // Ignore extension payload(s).
                        bs.ignore_bits(4)?;
                        for _ in 0..count - 1 {
                            bs.ignore_bits(8)?;
                        }
                    }
                }
                7 => {
                    // ID_TERM
                    break;
                }
                _ => unreachable!(),
            };
        }
        let rate_idx = GASubbandInfo::find_idx(self.asc.sample_rate);
        for pair in 0..cur_pair {
            let planes = self.pair_planes[pair];
            self.pairs[pair].synth_audio(&mut self.dsp, &mut self.buf, rate_idx, planes);
        }
        Ok(())
    }

    // fn flush(&mut self) {
    //     for pair in self.pairs.iter_mut() {
    //         pair.ics[0].delay = [0.0; 1024];
    //         pair.ics[1].delay = [0.0; 1024];
    //     }
    // }

    fn decode_inner(&mut self, packet: &PacketRef<'_>) -> Result<()> {
        // Clear the audio output buffer.
        self.buf.clear();
        self.buf.render_uninit(None);

        let mut bs = BitReaderLtr::new(packet.data);

        // Choose decode step based on the object type.
        match self.asc.object_type {
            AudioObjectType::Lc => self.decode_ga(&mut bs)?,
            _ => return unsupported_error("aac: object type"),
        }

        Ok(())
    }
}

impl AudioDecoder for AacDecoder {
    fn reset(&mut self) {
        for pair in self.pairs.iter_mut() {
            pair.reset();
        }
    }

    fn codec_info(&self) -> &CodecInfo {
        // Only one codec is supported.
        &Self::supported_codecs().first().expect("at least one codec registered").info
    }

    fn codec_params(&self) -> &AudioCodecParameters {
        &self.params
    }

    fn decode_ref(&mut self, packet: &PacketRef<'_>) -> Result<GenericAudioBufferRef<'_>> {
        if let Err(e) = self.decode_inner(packet) {
            self.buf.clear();
            Err(e)
        }
        else {
            Ok(self.buf.as_generic_audio_buffer_ref())
        }
    }

    fn finalize(&mut self) -> FinalizeResult {
        Default::default()
    }

    fn last_decoded(&self) -> GenericAudioBufferRef<'_> {
        self.buf.as_generic_audio_buffer_ref()
    }
}

impl RegisterableAudioDecoder for AacDecoder {
    fn try_registry_new(
        params: &AudioCodecParameters,
        opts: &AudioDecoderOptions,
    ) -> Result<Box<dyn AudioDecoder>>
    where
        Self: Sized,
    {
        Ok(Box::new(AacDecoder::try_new(params, opts)?))
    }

    fn supported_codecs() -> &'static [SupportedAudioCodec] {
        use symphonia_core::codecs::audio::well_known::profiles::CODEC_PROFILE_AAC_LC;

        &[support_audio_codec!(
            CODEC_ID_AAC,
            "aac",
            "Advanced Audio Coding",
            &[codec_profile!(CODEC_PROFILE_AAC_LC, "aac-lc", "Low Complexity"),]
        )]
    }
}
