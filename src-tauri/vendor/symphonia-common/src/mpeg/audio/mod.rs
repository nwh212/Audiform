// Symphonia
// Copyright (c) 2019-2026 The Project Symphonia Developers.
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use symphonia_core::audio::{Channels, Position, layouts};
use symphonia_core::codecs::CodecProfile;
use symphonia_core::codecs::audio::well_known::profiles::*;
use symphonia_core::errors::{Result, decode_error, unsupported_error};
use symphonia_core::io::{BitReaderLtr, FiniteBitStream, ReadBitsLtr};

/// MPEG4 audio object types.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AudioObjectType {
    #[default]
    None,
    Main,
    Lc,
    Ssr,
    Ltp,
    Sbr,
    Scalable,
    TwinVq,
    Celp,
    Hvxc,
    Ttsi,
    MainSynth,
    WavetableSynth,
    GeneralMidi,
    Algorithmic,
    ErAacLc,
    ErAacLtp,
    ErAacScalable,
    ErTwinVq,
    ErBsac,
    ErAacLd,
    ErCelp,
    ErHvxc,
    ErHiln,
    ErParametric,
    Ssc,
    Ps,
    MpegSurround,
    Layer1,
    Layer2,
    Layer3,
    Dst,
    Als,
    Sls,
    SlsNonCore,
    ErAacEld,
    SmrSimple,
    SmrMain,
    Reserved,
    Unknown,
}

impl std::fmt::Display for AudioObjectType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", AUDIO_OBJECT_TYPE_NAMES[*self as usize])
    }
}

const AUDIO_OBJECT_TYPES: &[AudioObjectType] = &[
    AudioObjectType::None,
    AudioObjectType::Main,
    AudioObjectType::Lc,
    AudioObjectType::Ssr,
    AudioObjectType::Ltp,
    AudioObjectType::Sbr,
    AudioObjectType::Scalable,
    AudioObjectType::TwinVq,
    AudioObjectType::Celp,
    AudioObjectType::Hvxc,
    AudioObjectType::Reserved,
    AudioObjectType::Reserved,
    AudioObjectType::Ttsi,
    AudioObjectType::MainSynth,
    AudioObjectType::WavetableSynth,
    AudioObjectType::GeneralMidi,
    AudioObjectType::Algorithmic,
    AudioObjectType::ErAacLc,
    AudioObjectType::Reserved,
    AudioObjectType::ErAacLtp,
    AudioObjectType::ErAacScalable,
    AudioObjectType::ErTwinVq,
    AudioObjectType::ErBsac,
    AudioObjectType::ErAacLd,
    AudioObjectType::ErCelp,
    AudioObjectType::ErHvxc,
    AudioObjectType::ErHiln,
    AudioObjectType::ErParametric,
    AudioObjectType::Ssc,
    AudioObjectType::Ps,
    AudioObjectType::MpegSurround,
    AudioObjectType::Reserved, // Escape
    AudioObjectType::Layer1,
    AudioObjectType::Layer2,
    AudioObjectType::Layer3,
    AudioObjectType::Dst,
    AudioObjectType::Als,
    AudioObjectType::Sls,
    AudioObjectType::SlsNonCore,
    AudioObjectType::ErAacEld,
    AudioObjectType::SmrSimple,
    AudioObjectType::SmrMain,
];

const AUDIO_OBJECT_TYPE_NAMES: &[&str] = &[
    "None",
    "AAC Main",
    "AAC LC",
    "AAC SSR",
    "AAC LTP",
    "SBR",
    "AAC Scalable",
    "TwinVQ",
    "CELP",
    "HVXC",
    // "(Reserved10)",
    // "(Reserved11)",
    "TTSI",
    "Main synthetic",
    "Wavetable synthesis",
    "General MIDI",
    "Algorithmic Synthesis and Audio FX",
    "ER AAC LC",
    // "(Reserved18)",
    "ER AAC LTP",
    "ER AAC Scalable",
    "ER TwinVQ",
    "ER BSAC",
    "ER AAC LD",
    "ER CELP",
    "ER HVXC",
    "ER HILN",
    "ER Parametric",
    "SSC",
    "PS",
    "MPEG Surround",
    // "(Escape)",
    "Layer-1",
    "Layer-2",
    "Layer-3",
    "DST",
    "ALS",
    "SLS",
    "SLS non-core",
    "ER AAC ELD",
    "SMR Simple",
    "SMR Main",
    "(Reserved)",
    "(Unknown)",
];

/// Try to get the audio object type from the given audio object type index.
pub fn get_mpeg4_audio_object_type_by_index(index: u32) -> Option<AudioObjectType> {
    AUDIO_OBJECT_TYPES.get(index as usize).copied()
}

/// MPEG4 audio sample rate result.
pub enum Mpeg4AudioSampleRate {
    /// The sample rate is known.
    SampleRate(u32),
    /// The sample rate should be read manually.
    ///
    /// For the MPEG4 Audio Specific Config, the sample rate should be read as the next 24-bits
    /// after the sample rate index. For ADTS, this is an error.
    Escape,
    /// The sample rate index was invalid.
    Invalid,
}

/// Try to get the audio sample rate given the sample rate index.
pub fn get_mpeg4_audio_sample_rate_by_index(index: u32) -> Mpeg4AudioSampleRate {
    const MPEG4_AUDIO_SAMPLE_RATES: [u32; 13] =
        [96000, 88200, 64000, 48000, 44100, 32000, 24000, 22050, 16000, 12000, 11025, 8000, 7350];

    match index {
        0..=12 => Mpeg4AudioSampleRate::SampleRate(MPEG4_AUDIO_SAMPLE_RATES[index as usize]),
        15 => Mpeg4AudioSampleRate::Escape,
        _ => Mpeg4AudioSampleRate::Invalid,
    }
}

/// MPEG4 audio channel configuration result.
pub enum Mpeg4AudioChannels {
    /// The channel configuration is known.
    Channels(Channels),
    /// The channel configuration is defined in-band based on the audio object type (e.g., from the
    /// program configuration element).
    Escape,
    /// The channel configuration index was invalid.
    Invalid,
}

/// Try to get the set of channels given the configuration index.
pub fn get_mpeg4_audio_channels_by_config_index(index: u32) -> Mpeg4AudioChannels {
    let channels = match index {
        0 => return Mpeg4AudioChannels::Escape,
        1 => layouts::CHANNEL_LAYOUT_MONO,
        2 => layouts::CHANNEL_LAYOUT_STEREO,
        3 => layouts::CHANNEL_LAYOUT_AAC_3P0,
        4 => layouts::CHANNEL_LAYOUT_AAC_4P0,
        5 => layouts::CHANNEL_LAYOUT_AAC_5P0,
        6 => layouts::CHANNEL_LAYOUT_AAC_5P1,
        // Audiform: 7.1 with side and rear pairs, as 7.1 AAC is made and read today, rather
        // than the older reading with two pairs of front speakers.
        7 => layouts::CHANNEL_LAYOUT_7P1,
        _ => return Mpeg4AudioChannels::Invalid,
    };
    Mpeg4AudioChannels::Channels(channels)
}

/// MPEG4 Audio Specific Configuration.
#[non_exhaustive]
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct AudioSpecificConfig {
    pub object_type: AudioObjectType,
    pub sample_rate: u32,
    pub channels: Option<Channels>,
    pub samples: usize,
    pub sbr_ps_info: Option<(u32, Option<Channels>)>,
    pub sbr_present: bool,
    pub ps_present: bool,
    /// Audiform: when a program config element gives the layout, its elements in the order
    /// it lists them, and the speaker of each of their channels in that order (empty when the
    /// layout could not be placed and the channels are numbered instead).
    pub pce_elements: Vec<PceElement>,
    pub element_positions: Vec<Position>,
}

impl AudioSpecificConfig {
    /// Read the audio specific configuration from the provided buffer. ISO14496-3-2009
    pub fn read(buf: &[u8]) -> Result<AudioSpecificConfig> {
        let mut bs = BitReaderLtr::new(buf);

        let mut asc = AudioSpecificConfig {
            object_type: Self::read_audio_object_type(&mut bs)?,
            sample_rate: Self::read_sampling_frequency(&mut bs)?,
            ..Default::default()
        };

        if asc.sample_rate == 0 {
            return decode_error("common (mp4a): a sample rate of 0 is invalid");
        }

        asc.channels = Self::read_channel_config(&mut bs)?;

        if (asc.object_type == AudioObjectType::Sbr) || (asc.object_type == AudioObjectType::Ps) {
            asc.sbr_present = true;
            if asc.object_type == AudioObjectType::Ps {
                asc.ps_present = true;
            }
            let ext_srate = Self::read_sampling_frequency(&mut bs)?;
            asc.object_type = Self::read_audio_object_type(&mut bs)?;

            let ext_chans = if asc.object_type == AudioObjectType::ErBsac {
                Self::read_channel_config(&mut bs)?
            }
            else {
                None
            };

            asc.sbr_ps_info = Some((ext_srate, ext_chans));
        }

        match asc.object_type {
            AudioObjectType::Main
            | AudioObjectType::Lc
            | AudioObjectType::Ssr
            | AudioObjectType::Scalable
            | AudioObjectType::TwinVq
            | AudioObjectType::ErAacLc
            | AudioObjectType::ErAacLtp
            | AudioObjectType::ErAacScalable
            | AudioObjectType::ErTwinVq
            | AudioObjectType::ErBsac
            | AudioObjectType::ErAacLd => {
                // GASpecificConfig
                let short_frame = bs.read_bool()?;

                asc.samples = if short_frame { 960 } else { 1024 };

                let depends_on_core = bs.read_bool()?;

                if depends_on_core {
                    let _delay = bs.read_bits_leq32(14)?;
                }

                let extension_flag = bs.read_bool()?;

                // Audiform: read the layout from the program config element.
                if asc.channels.is_none() {
                    let (elements, positions) = read_program_config(&mut bs)?;
                    let count = elements.iter().map(|e| u16::from(e.channels)).sum();
                    asc.pce_elements = elements;
                    match positions {
                        Some(positions) => {
                            let mask =
                                positions.iter().fold(Position::empty(), |acc, p| acc | *p);
                            asc.channels = Some(Channels::Positioned(mask));
                            asc.element_positions = positions;
                        }
                        None => asc.channels = Some(Channels::Discrete(count)),
                    }
                }

                if (asc.object_type == AudioObjectType::Scalable)
                    || (asc.object_type == AudioObjectType::ErAacScalable)
                {
                    let _layer = bs.read_bits_leq32(3)?;
                }

                if extension_flag {
                    if asc.object_type == AudioObjectType::ErBsac {
                        let _num_subframes = bs.read_bits_leq32(5)? as usize;
                        let _layer_length = bs.read_bits_leq32(11)?;
                    }

                    if (asc.object_type == AudioObjectType::ErAacLc)
                        || (asc.object_type == AudioObjectType::ErAacLtp)
                        || (asc.object_type == AudioObjectType::ErAacScalable)
                        || (asc.object_type == AudioObjectType::ErAacLd)
                    {
                        let _section_data_resilience = bs.read_bool()?;
                        let _scalefactors_resilience = bs.read_bool()?;
                        let _spectral_data_resilience = bs.read_bool()?;
                    }

                    let extension_flag3 = bs.read_bool()?;

                    if extension_flag3 {
                        return unsupported_error("common (mp4a): version3 extensions");
                    }
                }
            }
            AudioObjectType::Celp => {
                return unsupported_error("common (mp4a): CELP config");
            }
            AudioObjectType::Hvxc => {
                return unsupported_error("common (mp4a): HVXC config");
            }
            AudioObjectType::Ttsi => {
                return unsupported_error("common (mp4a): TTS config");
            }
            AudioObjectType::MainSynth
            | AudioObjectType::WavetableSynth
            | AudioObjectType::GeneralMidi
            | AudioObjectType::Algorithmic => {
                return unsupported_error("common (mp4a): structured audio config");
            }
            AudioObjectType::ErCelp => {
                return unsupported_error("common (mp4a): ER CELP config");
            }
            AudioObjectType::ErHvxc => {
                return unsupported_error("common (mp4a): ER HVXC config");
            }
            AudioObjectType::ErHiln | AudioObjectType::ErParametric => {
                return unsupported_error("common (mp4a): parametric config");
            }
            AudioObjectType::Ssc => {
                return unsupported_error("common (mp4a): SSC config");
            }
            AudioObjectType::MpegSurround => {
                // bs.ignore_bits(1)?; // sacPayloadEmbedding
                return unsupported_error("common (mp4a): MPEG Surround config");
            }
            AudioObjectType::Layer1 | AudioObjectType::Layer2 | AudioObjectType::Layer3 => {
                return unsupported_error("common (mp4a): MPEG Layer 1/2/3 config");
            }
            AudioObjectType::Dst => {
                return unsupported_error("common (mp4a): DST config");
            }
            AudioObjectType::Als => {
                // bs.ignore_bits(5)?; // fillBits
                return unsupported_error("common (mp4a): ALS config");
            }
            AudioObjectType::Sls | AudioObjectType::SlsNonCore => {
                return unsupported_error("common (mp4a): SLS config");
            }
            AudioObjectType::ErAacEld => {
                return unsupported_error("common (mp4a): ELD config");
            }
            AudioObjectType::SmrSimple | AudioObjectType::SmrMain => {
                return unsupported_error("common (mp4a): symbolic music config");
            }
            _ => {}
        };

        match asc.object_type {
            AudioObjectType::ErAacLc
            | AudioObjectType::ErAacLtp
            | AudioObjectType::ErAacScalable
            | AudioObjectType::ErTwinVq
            | AudioObjectType::ErBsac
            | AudioObjectType::ErAacLd
            | AudioObjectType::ErCelp
            | AudioObjectType::ErHvxc
            | AudioObjectType::ErHiln
            | AudioObjectType::ErParametric
            | AudioObjectType::ErAacEld => {
                let ep_config = bs.read_bits_leq32(2)?;

                if (ep_config == 2) || (ep_config == 3) {
                    return unsupported_error("common (mp4a): error protection config");
                }
                // if ep_config == 3 {
                //     let direct_mapping = bs.read_bit()?;
                //     validate!(direct_mapping);
                // }
            }
            _ => {}
        };

        if asc.sbr_ps_info.is_some() && (bs.bits_left() >= 16) {
            let sync = bs.read_bits_leq32(11)?;

            if sync == 0x2B7 {
                let ext_otype = Self::read_audio_object_type(&mut bs)?;
                if ext_otype == AudioObjectType::Sbr {
                    asc.sbr_present = bs.read_bool()?;
                    if asc.sbr_present {
                        let _ext_srate = Self::read_sampling_frequency(&mut bs)?;
                        if bs.bits_left() >= 12 {
                            let sync = bs.read_bits_leq32(11)?;
                            if sync == 0x548 {
                                asc.ps_present = bs.read_bool()?;
                            }
                        }
                    }
                }
                if ext_otype == AudioObjectType::Ps {
                    asc.sbr_present = bs.read_bool()?;
                    if asc.sbr_present {
                        let _ext_srate = Self::read_sampling_frequency(&mut bs)?;
                    }
                    let _ext_channels = bs.read_bits_leq32(4)?;
                }
            }
        }

        Ok(asc)
    }

    fn read_audio_object_type<B: ReadBitsLtr>(bs: &mut B) -> Result<AudioObjectType> {
        let index = match bs.read_bits_leq32(5)? {
            index if index < 31 => index as usize,
            31 => (bs.read_bits_leq32(6)? + 32) as usize,
            _ => unreachable!(),
        };

        let aot = AUDIO_OBJECT_TYPES.get(index).copied().unwrap_or(AudioObjectType::Unknown);

        Ok(aot)
    }

    fn read_sampling_frequency<B: ReadBitsLtr>(bs: &mut B) -> Result<u32> {
        let index = bs.read_bits_leq32(4)?;
        let rate = match get_mpeg4_audio_sample_rate_by_index(index) {
            Mpeg4AudioSampleRate::SampleRate(rate) => rate,
            Mpeg4AudioSampleRate::Escape => bs.read_bits_leq32(24)?,
            Mpeg4AudioSampleRate::Invalid => {
                return decode_error("common (mp4a): invalid sample rate");
            }
        };
        Ok(rate)
    }

    fn read_channel_config<B: ReadBitsLtr>(bs: &mut B) -> Result<Option<Channels>> {
        let index = bs.read_bits_leq32(4)?;
        let channels = match get_mpeg4_audio_channels_by_config_index(index) {
            Mpeg4AudioChannels::Channels(channels) => Some(channels),
            Mpeg4AudioChannels::Escape => None,
            Mpeg4AudioChannels::Invalid => {
                return decode_error("common (mp4a): invalid channel configuration");
            }
        };
        Ok(channels)
    }
}

pub fn get_audio_codec_profile(asc: &AudioSpecificConfig) -> Option<CodecProfile> {
    match asc.object_type {
        AudioObjectType::Main => Some(CODEC_PROFILE_AAC_MAIN),
        AudioObjectType::Ssr => Some(CODEC_PROFILE_AAC_SSR),
        AudioObjectType::Ltp => Some(CODEC_PROFILE_AAC_LTP),
        AudioObjectType::Lc => {
            if asc.ps_present {
                Some(CODEC_PROFILE_AAC_HE_V2)
            }
            else if asc.sbr_present {
                Some(CODEC_PROFILE_AAC_HE)
            }
            else {
                Some(CODEC_PROFILE_AAC_LC)
            }
        }
        _ => None,
    }
}

/// Audiform: one element a program config element lists: its kind as the raw data block
/// names it (0 single channel, 1 channel pair, 3 low-frequency), its instance tag, and its
/// number of channels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PceElement {
    pub id: u8,
    pub tag: u8,
    pub channels: u8,
}

/// Audiform: reads a program config element (ISO/IEC 14496-3, 4.4.1.1) and returns the speaker
/// of each channel, in the order the elements are listed (and carried). Front elements map to
/// center, left and right, then left and right of center; side pairs to side left and right;
/// back pairs to rear left and right and a back single to rear center. Returns the elements,
/// and the speakers of their channels when every channel has its own; a layout this cannot
/// place comes back without speakers, its channels numbered in the order listed. A raw data
/// block may carry the elements in another order: decoders match them by kind and tag.
///
/// The comment field is byte aligned relative to the start of the reader, which is the start
/// of the audio specific config or of the raw data block, as the element requires.
pub fn read_program_config<B: ReadBitsLtr>(
    bs: &mut B,
) -> Result<(Vec<PceElement>, Option<Vec<Position>>)> {
    let _tag = bs.read_bits_leq32(4)?;
    let _object_type = bs.read_bits_leq32(2)?;
    let _rate_index = bs.read_bits_leq32(4)?;
    let front = bs.read_bits_leq32(4)?;
    let side = bs.read_bits_leq32(4)?;
    let back = bs.read_bits_leq32(4)?;
    let lfe = bs.read_bits_leq32(2)?;
    let assoc = bs.read_bits_leq32(3)?;
    let cc = bs.read_bits_leq32(4)?;
    for _ in 0..2 {
        // Mono, then stereo mixdown.
        if bs.read_bool()? {
            bs.ignore_bits(4)?;
        }
    }
    if bs.read_bool()? {
        // Matrix mixdown index and pseudo-surround flag.
        bs.ignore_bits(3)?;
    }
    let mut elements = Vec::new();
    let mut read_elements = |n: u32, elements: &mut Vec<PceElement>| -> Result<Vec<bool>> {
        let mut v = Vec::new();
        for _ in 0..n {
            let is_cpe = bs.read_bool()?;
            let tag = bs.read_bits_leq32(4)? as u8;
            elements.push(PceElement {
                id: if is_cpe { 1 } else { 0 },
                tag,
                channels: if is_cpe { 2 } else { 1 },
            });
            v.push(is_cpe);
        }
        Ok(v)
    };
    let front = read_elements(front, &mut elements)?;
    let side = read_elements(side, &mut elements)?;
    let back = read_elements(back, &mut elements)?;
    for _ in 0..lfe {
        let tag = bs.read_bits_leq32(4)? as u8;
        elements.push(PceElement { id: 3, tag, channels: 1 });
    }
    bs.ignore_bits(4 * assoc)?;
    bs.ignore_bits(5 * cc)?;
    bs.realign();
    let comment = bs.read_bits_leq32(8)?;
    bs.ignore_bits(8 * comment)?;

    if elements.is_empty() {
        return decode_error("common (mp4a): program config without channels");
    }
    let place = || -> Option<Vec<Position>> {
        let mut out = Vec::new();
        let mut front_pairs = [
            (Position::FRONT_LEFT, Position::FRONT_RIGHT),
            (Position::FRONT_LEFT_CENTER, Position::FRONT_RIGHT_CENTER),
            (Position::FRONT_LEFT_WIDE, Position::FRONT_RIGHT_WIDE),
        ]
        .into_iter();
        let mut front_single = Some(Position::FRONT_CENTER);
        for &is_cpe in &front {
            if is_cpe {
                let (l, r) = match front_pairs.next() {
                    Some(pair) => pair,
                    None => return None,
                };
                out.extend([l, r]);
            }
            else {
                match front_single.take() {
                    Some(c) => out.push(c),
                    None => return None,
                }
            }
        }
        let mut side_pair = Some((Position::SIDE_LEFT, Position::SIDE_RIGHT));
        for &is_cpe in &side {
            match (is_cpe, side_pair.take()) {
                (true, Some((l, r))) => out.extend([l, r]),
                _ => return None,
            }
        }
        let mut back_pair = Some((Position::REAR_LEFT, Position::REAR_RIGHT));
        let mut back_single = Some(Position::REAR_CENTER);
        for &is_cpe in &back {
            if is_cpe {
                match back_pair.take() {
                    Some((l, r)) => out.extend([l, r]),
                    None => return None,
                }
            }
            else {
                match back_single.take() {
                    Some(c) => out.push(c),
                    None => return None,
                }
            }
        }
        match lfe {
            0 => (),
            1 => out.push(Position::LFE1),
            _ => {
                out.push(Position::LFE1);
                out.push(Position::LFE2);
            }
        }
        Some(out)
    };
    Ok((elements, place()))
}
