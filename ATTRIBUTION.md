# Attribution

Code ported or adapted from other projects is credited here. Each entry lists the project, the source file, its license, and what was used. Required license notices are kept in the adapted files.

| Project | Source file | License | What was used | Our file |
| --- | --- | --- | --- | --- |
| NeuralAmpModelerCore (sdatkinson/NeuralAmpModelerCore, Copyright (c) 2023 Steven Atkinson) | `NAM/wavenet/model.cpp`, `NAM/lstm.cpp`, `NAM/conv1d.cpp`, `NAM/dsp.cpp`, `NAM/activations.h` | MIT | How a `.nam` capture's weights are laid out and played (the WaveNet and LSTM forward passes, written fresh in Rust), and its quick tanh formula, so captures sound as they do in that project's plug-in. Our output was checked against theirs on their example captures (largest difference about 6e-6). | `src-tauri/src/dsp/nam.rs` |
| `vst3` crate (coupler-rs/vst3-rs, Copyright (c) Micah Johnston) | `examples/gain.rs` | MIT or Apache-2.0 | The example gain plug-in, as the base of a test plug-in (look-ahead, saved settings and a crash switch added). Used by the plug-in tests only; never shipped. | `src-tauri/plughost/testplug/src/lib.rs` |

## From later Altered Audio Editor releases

Features Altered Audio Editor added after Audiform's starting point (0.2.0), brought in under GPL v3.0 or later. Their files keep `Copyright (c) The GeekCast Radio Network, LLC.`

| Feature | Altered Audio Editor version | Our files |
| --- | --- | --- |
| Delay and Echo effect, and its four presets | 0.4.1, 0.5.0 | `src-tauri/src/dsp/effects/echo.rs`, presets in `src/panels/voice-chain/chain.ts` |
| Split Channels to Tracks | 0.4.0 | `src/panels/properties/split-channels.ts` |
| Capture Noise Print (Shift+P) | 0.4.0 | `captureNoisePrint` in `src/app/App.svelte` |
| Export at a loudness target | 0.4.0 | `normalize` in `src-tauri/src/export/mixdown.rs`, the Loudness choice in `src/app/ExportDialog.svelte` |
| Hearing a track's built-in effects while monitoring; ASIO monitoring on the input's own driver; ASIO device lists asked again when they come back empty | 0.4.0 to 0.5.0 | `src-tauri/src/engine/record.rs` (`MonitorMix`, `open_monitor`), `src-tauri/src/commands/devices.rs` (`retry_if_empty`) |

## Published formulas

| Source | Use | Our file |
| --- | --- | --- |
| Robert Bristow-Johnson, "Cookbook formulae for audio EQ biquad filter coefficients" (published freely for use) | Coefficient formulas for the low-pass, high-pass, band-pass, notch, peak and shelf filters behind Parametric EQ, Hum Cut and De-Esser. Formulas only; the filter code is our own. | `src-tauri/src/dsp/biquad.rs` |
| Sophocles J. Orfanidis, "Lecture Notes on Elliptic Filter Design" (Rutgers University, freely published) | The textbook elliptic design method: Landen's transformation for the Jacobi elliptic functions, the degree equation, and the pole and zero formulas. Method only; the code is our own. | `src-tauri/src/dsp/elliptic.rs` |
| MP4 file format (ISO/IEC 14496-12 box layout, as publicly documented) | The box layout for writing M4A tags (`moov/udta/meta/ilst`). Our own writer. | `src-tauri/src/export/m4a_tags.rs` |

## Bundled assets

| Asset | Source | License | Use |
| --- | --- | --- | --- |
| Source Sans 3 (regular, semibold; Latin) | npm package `@fontsource/source-sans-3` | SIL Open Font License 1.1 | Interface font, bundled into the app build. The license travels with the package. |
| `src/assets/themes/meadow-photo.webp` | "BlueSkyWhiteCloudsGrassland.jpg" by Raysonho @ Open Grid Scheduler / Grid Engine, [Wikimedia Commons](https://commons.wikimedia.org/wiki/File:BlueSkyWhiteCloudsGrassland.jpg) | CC0 1.0 (public domain dedication) | Meadow Aero theme background; resized and color-adjusted (greener grass, deeper sky). |
| `src/assets/themes/moonlight.webp` | "Night sky with moon and stars over cotton field in Batesville, Texas" by the U.S. Department of Agriculture, [Wikimedia Commons](https://commons.wikimedia.org/wiki/File:Night_sky_with_moon_and_stars_over_cotton_field_in_Batesville,_Texas.jpg) | Public domain (work of the U.S. federal government) | Moonlight Aero theme background; resized, color-graded to the theme's navy and teal, horizon lights removed. |
| `src/assets/themes/aqua.webp`, `prism.webp`, `droplets.png`, `orb.png`, `cursor.svg`, `walnut.svg`, `brushed.svg`, `matte.svg` | Made for Audiform | GPL-3.0-or-later, like the app | Aqua Aero scene, the Prism backdrop, water droplets, glass orb transport buttons and the glass pointer. |

## Libraries with notable licenses

Most dependencies are MIT or Apache-2.0 and are listed in `Cargo.lock` and `package-lock.json`. These are called out because of their license or what they do:

| Library | License | Use |
| --- | --- | --- |
| LAME 3.100 (via `mp3lame-sys` and `mp3lame-encoder`) | LGPL-2.0-or-later (LAME); MIT (bindings) | MP3 encoding. Built from source and linked into the app; LGPL is compatible with the app's GPL-3.0-or-later. |
| `flacenc` | Apache-2.0 | FLAC encoding. |
| `opus-decoder` | MIT or Apache-2.0 | Opus decoding. A patched copy lives in `src-tauri/vendor/opus-decoder` (faster forward FFT through `rustfft`); see the note there. |
| `symphonia-common` | MPL-2.0 | Shared Xiph and MPEG helpers for decoding. A patched copy lives in `src-tauri/vendor/symphonia-common` so Ogg Vorbis files with more than 8 channels (multitrack Ogg and MOGG stems) open, AAC layouts given by a program config element are read, and AAC channel configuration 7 reads as 7.1 with side and rear pairs; the changed file stays under MPL-2.0. See the note there. |
| `symphonia-codec-aac` | MPL-2.0 | AAC decoding. A patched copy lives in `src-tauri/vendor/symphonia-codec-aac` so AAC with more than two channels (5.1, 7.1) decodes with each channel on its speaker, and HE-AAC is refused clearly so Windows can decode it instead; the changed files stay under MPL-2.0. See the note there. |
| FFmpeg DTS core decoder (`libavcodec/dca_core.c`, `dcadsp.c`, `dcadct.c`, `synth_filter.c`, tables from `dcadata.c` and `dcahuff.c`, FFmpeg 6.1.1; Copyright (C) 2016 foo86, 2004 Gildas Bazin and others) | LGPL-2.1-or-later | Ported to Rust in `src-tauri/src/codecs/dts/` (core.rs and the generated tables.rs keep the LGPL-2.1-or-later notice), to decode DTS WAV and `.dts` files. Fixed-point core only; output matches FFmpeg's bit-exact decode. LGPL-2.1-or-later is compatible with the app's GPL-3.0-or-later. |
| ASIO SDK 2.3 (Steinberg, via `cpal`'s `asio` feature, Windows release builds only) | GPL-3.0 (dual licensed by Steinberg; we use the GPLv3 option) | Talking to ASIO audio drivers. ASIO is a trademark of Steinberg Media Technologies GmbH. We do not use the ASIO logo. |
| `zip` | MIT | Reading zip downloads of recordings. |
| `vst3` (bindings generated from the VST 3 SDK headers) and `com-scrape-types` | MIT or Apache-2.0 | Hosting VST3 plug-ins, in a separate process (`src-tauri/plughost`). The VST 3 SDK itself is MIT licensed by Steinberg (since 2025). VST is a trademark of Steinberg Media Technologies GmbH; we do not use the VST logo. VST2 is not supported (its SDK is no longer licensed). |
| `libloading` | ISC | Loading a plug-in's library inside the plug-in host process. |
| `windows-sys` (Windows only, plug-in host) | MIT or Apache-2.0 | The window a plug-in draws its own controls in. |
| Symphonia | MPL-2.0 | Decoding WAV, AIFF, FLAC, MP3, AAC, ALAC, Ogg Vorbis and ADPCM. |
| `windows` (Windows only) | MIT or Apache-2.0 | Calls the AAC encoder built into Windows (Media Foundation) for M4A export. No AAC encoder is bundled; the operating system provides it. |

